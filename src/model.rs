//! Sources as the TV reports them, and the backend that lists and switches them.

use crate::icons::SourceIcon;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, mpsc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    Hdmi,
    LiveTv,
    Analog,
    /// A USB stick or disk, opened in LG's media player.
    Usb,
    /// A DLNA/UPnP media server on the network.
    MediaServer,
    /// The TV's home screen (LG's, or one that took its place: Luma, Glasshouse Home).
    Home,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    /// Stable key used by the config (`HDMI_1`, `LIVE_TV`, or an app id).
    pub id: String,
    /// What `applicationManager/launch` opens.
    pub app_id: String,
    /// The TV's own name for it ("HDMI 1", or the label set in LG's input settings).
    pub label: String,
    pub kind: SourceKind,
    /// Cable plugged in (HDMI) or always true.
    pub connected: bool,
    /// A device name read over CEC/SIMPLINK or the universal-remote setup ("Mecool").
    pub device: Option<String>,
    /// The icon the TV picked, mapped onto ours.
    pub tv_icon: SourceIcon,
    /// Launch parameters (`{"requestDevice": id}` for storage).
    pub params: Option<serde_json::Value>,
    /// The device's maker, from the universal-remote setup or its CEC vendor id ("Denon").
    pub brand: Option<String>,
    /// The TV's label was set by the user in LG's input settings (not generated).
    pub label_custom: bool,
    /// HDMI: whether a picture signal is present (None when unknown).
    pub signal: Option<bool>,
}

impl Source {
    /// The name of what is plugged in: maker and model when both are known
    /// ("Denon AVC-S670H"), else the CEC name, the maker, or the TV's label.
    /// A label the user set in LG's settings always wins.
    pub fn device_name(&self) -> String {
        if self.label_custom {
            return self.label.clone();
        }
        let model = self.device.as_deref().map(str::trim).filter(|m| !m.is_empty());
        let brand = self.brand.as_deref().map(str::trim).filter(|b| !b.is_empty());
        // A CEC name with digits is a model ("AVC-S670H") and takes its maker in
        // front; without, it is a name the user gave the device ("Salon").
        let model_like = model.is_some_and(|m| m.chars().any(|c| c.is_ascii_digit()));
        match (brand, model) {
            (Some(b), Some(m)) if m.to_lowercase().contains(&b.to_lowercase()) => m.to_string(),
            (Some(b), Some(m)) if model_like => format!("{b} {m}"),
            (Some(_), Some(m)) => m.to_string(),
            (None, Some(m)) => m.to_string(),
            (Some(b), None) => b.to_string(),
            (None, None) => self.label.clone(),
        }
    }

    /// The maker, when the name shown does not already say it.
    pub fn brand_hint(&self) -> Option<String> {
        let b = self.brand.as_deref()?.trim();
        (!b.is_empty() && !self.device_name().to_lowercase().contains(&b.to_lowercase())).then(|| b.to_string())
    }

    /// "HDMI 3" for an input, from its id.
    pub fn port_name(&self) -> String {
        self.id.replace('_', " ")
    }

    /// The home screen entry; `name` is the home in use ("Luma", "LG").
    pub fn home(name: &str) -> Self {
        Source {
            id: "HOME".into(),
            app_id: "com.webos.app.home".into(),
            label: "Home".into(),
            kind: SourceKind::Home,
            connected: true,
            device: Some(name.into()),
            tv_icon: SourceIcon::Home,
            // As the remote's Home key opens it, so a replacement home opens as home.
            params: Some(serde_json::json!({ "activateType": "home", "launchType": "homeKey" })),
            brand: None,
            label_custom: false,
            signal: None,
        }
    }

    pub fn live_tv() -> Self {
        Source {
            id: "LIVE_TV".into(),
            app_id: "com.webos.app.livetv".into(),
            label: "Live TV".into(),
            kind: SourceKind::LiveTv,
            connected: true,
            device: None,
            tv_icon: SourceIcon::Antenna,
            params: None,
            brand: None,
            label_custom: false,
            signal: None,
        }
    }
}

/// Maps the icon file names LG uses in `eim/getAllInputStatus` onto ours.
pub fn icon_from_tv(name: &str, kind: SourceKind) -> SourceIcon {
    let n = name.to_ascii_lowercase();
    let has = |s: &str| n.contains(s);
    if has("stream") || has("settop") || has("stb") || has(" ott") || has("box") || has("ott") || has("box") || has("cable") && has("box") {
        SourceIcon::StreamingBox
    } else if has("game") || has("console") || has("playstation") || has("xbox") || has("switch") {
        SourceIcon::Console
    } else if has("pc") || has("computer") || has("laptop") {
        SourceIcon::Computer
    } else if has("sound") || has("bar") || has("theater") || has("hometheater") || has("denon") || has("sonos") || has("audio") || has("speaker") || has("receiver") || has("avr") {
        SourceIcon::Speaker
    } else if has("blu") || has("dvd") || has("player") {
        SourceIcon::Movie
    } else if has("camera") || has("monitor") {
        SourceIcon::Monitor
    } else {
        match kind {
            SourceKind::LiveTv => SourceIcon::Antenna,
            SourceKind::Usb => SourceIcon::Usb,
            SourceKind::MediaServer => SourceIcon::Server,
            SourceKind::Home => SourceIcon::Home,
            _ => SourceIcon::Cable,
        }
    }
}

/// Who answers the remote's Source key.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum KeyOwner {
    /// Not read yet.
    #[default]
    Pending,
    /// LG's own picker (the TV as it came).
    Lg,
    /// This picker, through our key filter.
    Picker,
    /// Another key filter (Glasshouse's).
    Other(String),
    /// Our filter is listed but broken (its file is gone).
    Broken(String),
    /// Native key filters (webOS 11): no takeover possible.
    Unsupported,
    /// The chain could not be read.
    Unknown(String),
}

impl KeyOwner {
    pub fn code(&self) -> &'static str {
        match self {
            KeyOwner::Pending | KeyOwner::Unknown(_) => "unknown",
            KeyOwner::Lg => "lg",
            KeyOwner::Picker => "picker",
            KeyOwner::Other(_) => "other",
            KeyOwner::Broken(_) => "broken",
            KeyOwner::Unsupported => "unsupported",
        }
    }
    pub fn detail(&self) -> &str {
        match self {
            KeyOwner::Other(s) | KeyOwner::Broken(s) | KeyOwner::Unknown(s) => s,
            _ => "",
        }
    }
}

/// Makers by HDMI-CEC vendor id (IEEE OUI), for devices that report a model
/// name but no brand. Chip makers (Amlogic, Realtek…) are left out on purpose.
pub fn cec_vendor(id: u64) -> Option<&'static str> {
    Some(match id {
        0x0005CD => "Denon",
        0x000678 => "Marantz",
        0x00A0DE => "Yamaha",
        0x0009B0 => "Onkyo",
        0x080046 | 0x08001F => "Sony",
        0x0000F0 => "Samsung",
        0x00E091 => "LG",
        0x001A11 => "Google",
        0x00903E => "Philips",
        0x008045 => "Panasonic",
        0x000039 => "Toshiba",
        0x0010FA => "Apple",
        0x534850 => "Sharp",
        0x00044B => "NVIDIA",
        0x000CE7 => "MediaTek",
        0x0020C7 => "Akai",
        0x6B746D => "Vizio",
        0x00D0D5 => "Grundig",
        0x000D93 => "Bose",
        0x0050C2 | 0x00D38D => "Harman Kardon",
        0x5CAD76 => "TCL",
        0x9C645E => "Harman",
        0x0013E0 => "Sonos",
        _ => return None,
    })
}

/// A source's picture: the live one for the input in front, else the last seen.
#[derive(Clone)]
pub struct Preview {
    pub width: usize,
    pub height: usize,
    /// RGBA, top row first.
    pub rgba: Arc<Vec<u8>>,
    /// Bumped at each new frame, so the UI re-uploads only what changed.
    pub seq: u64,
    /// Captured while the picker is open (the input in front), not a stored one.
    pub live: bool,
}

/// Decodes the uncompressed 24/32-bit BMP the TV's capture service writes.
pub fn decode_bmp(b: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    let u32_at = |o: usize| b.get(o..o + 4).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]));
    let i32_at = |o: usize| u32_at(o).map(|v| v as i32);
    if b.get(0..2)? != b"BM" {
        return None;
    }
    let data = u32_at(10)? as usize;
    let w = i32_at(18)?;
    let h = i32_at(22)?;
    let bpp = u16::from_le_bytes([*b.get(28)?, *b.get(29)?]) as usize;
    if w <= 0 || h == 0 || !(bpp == 24 || bpp == 32) || u32_at(30)? != 0 && u32_at(30)? != 3 {
        return None;
    }
    let (w, top_down, h) = (w as usize, h < 0, h.unsigned_abs() as usize);
    let stride = (w * bpp / 8).div_ceil(4) * 4;
    let mut rgba = vec![0u8; w * h * 4];
    for y in 0..h {
        let src_row = if top_down { y } else { h - 1 - y };
        let row = b.get(data + src_row * stride..data + src_row * stride + w * bpp / 8)?;
        for x in 0..w {
            let px = &row[x * bpp / 8..];
            let o = (y * w + x) * 4;
            rgba[o] = px[2];
            rgba[o + 1] = px[1];
            rgba[o + 2] = px[0];
            rgba[o + 3] = 255;
        }
    }
    Some((w, h, rgba))
}

/// What the TV side tells the UI. Written by the backend thread, read every frame.
#[derive(Default, Clone)]
pub struct TvState {
    pub sources: Vec<Source>,
    /// USB storage and DLNA servers (attachedstoragemanager).
    pub storage: Vec<Source>,
    /// The home screen in use, as detected.
    pub home: Option<Source>,
    /// App id in front (`com.webos.app.hdmi1`), to mark the current source.
    pub foreground: Option<String>,
    pub loaded: bool,
    pub error: Option<String>,
    /// Who answers the Source key, and a takeover change in progress or failed.
    pub key_owner: KeyOwner,
    pub takeover_busy: bool,
    pub takeover_error: Option<String>,
    /// Pictures by source id (live for the input in front, else last seen).
    pub previews: HashMap<String, Preview>,
    /// Whether this TV lets us capture the picture (None until tried).
    pub preview_supported: Option<bool>,
    /// webOS 6 capture whitelist: Some(true) listed, Some(false) not, None not needed.
    pub capture_access: Option<bool>,
}

pub type Shared = Arc<Mutex<TvState>>;

pub enum Command {
    /// Opens an app (an input's app, or the media player) with optional params.
    Launch(String, Option<serde_json::Value>),
    Refresh,
    /// Takes the Source key (true) or gives it back to LG (false).
    Takeover(bool),
    /// Sends a SIMPLINK (HDMI-CEC) key to the device on input `app_id`,
    /// switching to it first: LG only sends CEC keys to the active input.
    Cec { app_id: String, key: u32 },
    /// Adds this app to (true) or removes it from (false) the webOS 6 capture whitelist.
    CaptureAccess(bool),
}

/// A TV (or a mock): fills `Shared` from a worker thread and takes commands.
pub struct Backend {
    pub state: Shared,
    pub commands: mpsc::Sender<Command>,
    /// Live preview rate the UI wants (pictures per second, 0 = off).
    pub preview_fps: Arc<AtomicU32>,
}

impl Backend {
    pub fn set_preview_fps(&self, fps: u32) {
        self.preview_fps.store(fps, Ordering::Relaxed);
    }
}

/// Mock inputs for the desktop preview, shaped like the C1's.
pub fn mock_backend(repaint: impl Fn() + Send + 'static) -> Backend {
    let state: Shared = Arc::default();
    let (tx, rx) = mpsc::channel::<Command>();
    let s = state.clone();
    std::thread::spawn(move || {
        let src = |n: u8, label: &str, connected: bool, device: Option<&str>, icon: SourceIcon| Source {
            id: format!("HDMI_{n}"),
            app_id: format!("com.webos.app.hdmi{n}"),
            label: label.into(),
            kind: SourceKind::Hdmi,
            connected,
            device: device.map(Into::into),
            tv_icon: icon,
            params: None,
            brand: None,
            label_custom: false,
            signal: None,
        };
        let store = |id: &str, name: &str, kind: SourceKind, sub: &str| Source {
            id: id.into(),
            app_id: "com.webos.app.mediadiscovery".into(),
            label: name.into(),
            kind,
            connected: true,
            device: Some(sub.into()),
            tv_icon: icon_from_tv("", kind),
            params: Some(serde_json::json!({"requestDevice": id})),
            brand: None,
            label_custom: false,
            signal: None,
        };
        {
            let mut st = s.lock().unwrap();
            let mut sources = vec![
                Source::live_tv(),
                src(1, "Mecool OTT", true, Some("TV Thoxy"), SourceIcon::StreamingBox),
                src(2, "Denon Barre de son", true, Some("AVC-S660H"), SourceIcon::Speaker),
                src(3, "HDMI 3", false, None, SourceIcon::Cable),
                src(4, "PlayStation 5", false, None, SourceIcon::Console),
            ];
            sources[2].brand = Some("Denon".into());
            sources[1].brand = Some("Mecool".into());
            sources[4].label_custom = true;
            st.sources = sources;
            st.storage = vec![
                store("USB:mock", "SanDisk Ultra", SourceKind::Usb, "1"),
                store("DLNA:freebox", "Freebox Server", SourceKind::MediaServer, "Media server"),
            ];
            st.foreground = Some("com.webos.app.hdmi1".into());
            st.key_owner = KeyOwner::Lg;
            st.home = Some(Source::home("Luma"));
            st.loaded = true;
        }
        repaint();
        // Fake pictures: a moving "live" one for the input in front, a still one for HDMI 2.
        {
            let picture = |hue: f32, phase: f32| {
                let (w, h) = (320usize, 180usize);
                let mut rgba = vec![255u8; w * h * 4];
                for y in 0..h {
                    for x in 0..w {
                        let o = (y * w + x) * 4;
                        let t = x as f32 / w as f32;
                        let band = (((t * 6.0 + phase).sin() * 0.5 + 0.5) * 60.0) as u8;
                        rgba[o] = (40.0 + 150.0 * hue * (1.0 - t)) as u8 + band / 2;
                        rgba[o + 1] = (60.0 + 80.0 * (y as f32 / h as f32)) as u8;
                        rgba[o + 2] = (90.0 + 120.0 * t) as u8;
                    }
                }
                Arc::new(rgba)
            };
            let mut st = s.lock().unwrap();
            st.preview_supported = Some(true);
            st.previews.insert("HDMI_2".into(), Preview { width: 320, height: 180, rgba: picture(0.2, 1.0), seq: 1, live: false });
            st.previews.insert("HDMI_1".into(), Preview { width: 320, height: 180, rgba: picture(1.0, 0.0), seq: 1, live: true });
        }
        repaint();
        while let Ok(cmd) = rx.recv() {
            match cmd {
                Command::Launch(app, _) => s.lock().unwrap().foreground = Some(app),
                Command::Takeover(on) => s.lock().unwrap().key_owner = if on { KeyOwner::Picker } else { KeyOwner::Lg },
                Command::CaptureAccess(on) => s.lock().unwrap().capture_access = Some(on),
                Command::Refresh | Command::Cec { .. } => {}
            }
            repaint();
        }
    });
    Backend { state, commands: tx, preview_fps: Arc::new(AtomicU32::new(0)) }
}
