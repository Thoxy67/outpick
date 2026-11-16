//! Sources as the TV reports them, and the backend that lists and switches them.

use crate::icons::SourceIcon;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(feature = "webos"), allow(dead_code))] // used by the TV backend (and tests)
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
    /// Not a source: the tile that opens the list of installed apps (Customize's Apps switch).
    Apps,
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
    /// HDMI-CEC (SIMPLINK) reaches the device behind this input.
    pub cec: bool,
    /// Whether that device is on, as SIMPLINK reports it (webOS 25; None when unknown).
    pub cec_power: Option<bool>,
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
            cec: false,
            cec_power: None,
        }
    }

    /// The Apps tile: opens the list of installed apps, never launched itself.
    pub fn apps() -> Self {
        Source {
            id: "APPS".into(),
            app_id: String::new(),
            label: "Applications".into(),
            kind: SourceKind::Apps,
            connected: true,
            device: None,
            tv_icon: SourceIcon::Apps,
            params: None,
            brand: None,
            label_custom: false,
            signal: None,
            cec: false,
            cec_power: None,
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
            cec: false,
            cec_power: None,
        }
    }
}

/// What an HDMI input says about the device behind it, from `eim/getAllInputStatus`.
#[derive(Default, Clone, Debug)]
#[cfg_attr(not(feature = "webos"), allow(dead_code))] // used by the TV backend (and tests)
pub struct DeviceHints {
    /// HDMI-CEC: the device's OSD name, vendor id (IEEE OUI) and device type
    /// (0 TV, 1 recorder, 3 tuner/set-top box, 4 playback, 5 audio system).
    pub cec_name: String,
    pub cec_vendor: Option<u64>,
    pub cec_type: Option<u64>,
    /// HDMI source product description info frame: maker, product, source type ("Digital STB", "PC general"…).
    pub spd_vendor: String,
    pub spd_product: String,
    pub spd_type: String,
    /// LG's own guess and the user's choice in the TV's input settings: icon file, label, universal-remote brand/service.
    pub lg_icon: String,
    pub label: String,
    pub remote_brand: String,
    pub remote_service: String,
}

/// The icon of an HDMI input's device. HDMI-CEC first (the device's own name, maker
/// and device type); then the HDMI info frame and LG's settings, for devices without
/// CEC (most PCs) or with an anonymous CEC name.
#[cfg_attr(not(feature = "webos"), allow(dead_code))] // used by the TV backend (and tests)
pub fn detect_icon(h: &DeviceHints, kind: SourceKind) -> SourceIcon {
    let words = |s: &str| -> Vec<String> { s.to_ascii_lowercase().split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_string).collect() };
    let low = |s: &str| s.to_ascii_lowercase();
    let cec_maker = h.cec_vendor.and_then(cec_vendor).unwrap_or("");
    let cec = low(&format!("{} {}", h.cec_name, cec_maker));
    let cec_w = words(&cec);
    let all = low(&format!("{} {} {} {} {} {} {} {} {}", cec, h.spd_vendor, h.spd_product, h.spd_type, h.lg_icon, h.label, h.remote_brand, h.remote_service, ""));
    let all_w = words(&all);
    let w_has = |ws: &Vec<String>, list: &[&str]| ws.iter().any(|w| list.contains(&w.as_str()));
    let any = |s: &str, list: &[&str]| list.iter().any(|x| s.contains(x));

    // 1. Consoles by name or maker (CEC, then the info frame and labels).
    for text in [&cec, &all] {
        let ws = words(text);
        if text.contains("xbox") || text.contains("microsoft") {
            return SourceIcon::Xbox;
        }
        if text.contains("playstation") || w_has(&ws, &["ps3", "ps4", "ps5", "scei", "sie"]) {
            return SourceIcon::PlayStation;
        }
        if text.contains("nintendo") {
            return SourceIcon::NintendoSwitch;
        }
    }
    // 2. Audio systems: CEC device type 5, or LG's home theater / sound bar icon.
    if h.cec_type == Some(5) || any(&low(&h.lg_icon), &["hometheater", "soundbar", "audio"]) {
        return SourceIcon::Speaker;
    }
    // 3. Android TV / Google TV boxes: by CEC name or maker (Google, NVIDIA Shield…), else
    //    by the chip in the info frame (Amlogic, Rockchip… reference boards) or the box's brand.
    let android_names = ["android", "shield", "chromecast", "google", "fire tv", "firetv", "fire stick", "mi box", "mibox", "xiaomi", "mecool", "ugoos", "zidoo", "homatics", "formuler", "dune hd"];
    let chips = ["amlogic", "meson", "rockchip", "allwinner", "hisilicon", "realtek", "mediatek"];
    if any(&cec, &android_names) || matches!(h.cec_vendor, Some(0x001A11 | 0x00044B)) {
        return SourceIcon::Android;
    }
    let spd = low(&format!("{} {}", h.spd_vendor, h.spd_product));
    if any(&spd, &chips) || any(&spd, &android_names) || any(&low(&h.remote_brand), &android_names) || all.contains("android") {
        return SourceIcon::Android;
    }
    // 4. PCs: rarely on CEC (a Pulse-Eight / libCEC adapter, Kodi), else the info frame's
    //    "PC general" source type, a graphics maker, or LG's PC icon / a "PC" label.
    if matches!(h.cec_vendor, Some(0x001582)) || any(&cec, &["kodi", "libcec", "pulse-eight"])
        || low(&h.spd_type).contains("pc")
        || w_has(&words(&spd), &["nvidia", "geforce", "amd", "radeon", "intel", "ati"])
        || w_has(&all_w, &["pc", "computer", "ordinateur", "laptop", "windows", "steam"])
        || any(&low(&h.lg_icon), &["pc", "computer", "laptop"])
    {
        return SourceIcon::Pc;
    }
    // 5. Apple TV, Roku and other players by CEC name or maker; then LG's icon and labels.
    if any(&cec, &["apple tv", "appletv", "roku"]) || matches!(h.cec_vendor, Some(0x0010FA)) || w_has(&cec_w, &["apple"]) {
        return SourceIcon::StreamingBox;
    }
    if h.cec_type == Some(3) {
        return SourceIcon::StreamingBox;
    }
    icon_from_tv(&all, kind)
}

/// Maps the icon file names LG uses in `eim/getAllInputStatus` onto ours.
pub fn icon_from_tv(name: &str, kind: SourceKind) -> SourceIcon {
    let n = name.to_ascii_lowercase();
    let has = |s: &str| n.contains(s);
    // Whole words, so "xbox" is not a box and "spc" not a PC.
    let words: Vec<&str> = n.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let word = |s: &str| words.contains(&s);
    if has("xbox") {
        SourceIcon::Xbox
    } else if has("playstation") {
        SourceIcon::PlayStation
    } else if has("stream") || has("settop") || word("stb") || word("ott") || word("box") || has("streamingbox") {
        SourceIcon::StreamingBox
    } else if has("game") || has("console") || has("playstation") || has("xbox") || has("switch") {
        SourceIcon::Console
    } else if word("pc") || has("computer") {
        SourceIcon::Pc
    } else if has("laptop") {
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
#[cfg_attr(not(feature = "webos"), allow(dead_code))] // used by the TV backend (and tests)
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

#[cfg_attr(not(feature = "webos"), allow(dead_code))] // used by the TV backend (and tests)
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
#[cfg_attr(not(feature = "webos"), allow(dead_code))] // used by the TV backend (and tests)
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
        0x0050F2 => "Microsoft",
        0x0009BF => "Nintendo",
        0x001582 => "Pulse-Eight",
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
#[cfg_attr(not(feature = "webos"), allow(dead_code))] // used by the TV backend (and tests)
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

/// A capture's file is whole: as long as its header says, with every row it announces.
/// The capture service may answer before it has finished writing.
#[cfg_attr(not(feature = "webos"), allow(dead_code))] // used by the TV backend (and tests)
pub fn bmp_complete(b: &[u8]) -> bool {
    b.get(2..6).is_some_and(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as usize == b.len()) && decode_bmp(b).is_some()
}

/// A frame with nothing to show: black, or nearly one flat colour (an input
/// switching, no signal yet, a blanked video plane). Judged on a sample of pixels.
#[cfg_attr(not(feature = "webos"), allow(dead_code))] // used by the TV backend (and tests)
pub fn frame_blank(rgba: &[u8]) -> bool {
    let luma: Vec<f32> = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .step_by(7)
        .map(|p| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32)
        .collect();
    if luma.is_empty() {
        return true;
    }
    let mean = luma.iter().sum::<f32>() / luma.len() as f32;
    let var = luma.iter().map(|l| (l - mean) * (l - mean)).sum::<f32>() / luma.len() as f32;
    mean < 12.0 || var.sqrt() < 5.0
}

/// The preview rules' times. A frame is shown once its input has been in front
/// `SETTLE` (the picture of a switch is black or still the old input's). It may be
/// remembered for the next openings once its input has been in front `KEEP_FIRST`
/// if that input was already there when the picker opened (only its report may be
/// corrected at first: Live TV, then the HDMI input), `KEEP` after a switch seen
/// while open (the picture resyncs). It is then kept once confirmed (`pending_fate`).
#[cfg_attr(not(feature = "webos"), allow(dead_code))]
pub const PREVIEW_SETTLE: Duration = Duration::from_millis(400);
#[cfg_attr(not(feature = "webos"), allow(dead_code))]
pub const PREVIEW_KEEP_FIRST: Duration = Duration::from_millis(800);
#[cfg_attr(not(feature = "webos"), allow(dead_code))]
pub const PREVIEW_KEEP: Duration = Duration::from_secs(2);
#[cfg_attr(not(feature = "webos"), allow(dead_code))]
pub const PREVIEW_CONFIRM: Duration = Duration::from_millis(500);

/// The app in front as the preview loop has seen it, and since when.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(not(feature = "webos"), allow(dead_code))]
pub struct Front {
    pub app: Option<String>,
    pub since: Instant,
    /// The first app seen in front since the process started: it was there before the
    /// picker opened, not switched to while it is open.
    pub first: bool,
}

#[cfg_attr(not(feature = "webos"), allow(dead_code))]
impl Front {
    /// What the process sees first (None until the TV has said).
    pub fn new(app: Option<String>, now: Instant) -> Self {
        Front { app, since: now, first: true }
    }

    /// Notes what is in front at `now`; `since` moves only when it changes. True on a change.
    pub fn observe(&mut self, app: Option<String>, now: Instant) -> bool {
        if app == self.app {
            return false;
        }
        // Learning what is in front is not a switch; any change after that is.
        let first = self.first && self.app.is_none();
        *self = Front { app, since: now, first };
        true
    }

    /// How long its input must have been in front for a frame to be remembered.
    pub fn keep_after(&self) -> Duration {
        if self.first { PREVIEW_KEEP_FIRST } else { PREVIEW_KEEP }
    }
}

/// What a captured frame may be used for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(feature = "webos"), allow(dead_code))]
pub enum Verdict {
    /// Not filed anywhere, and why.
    Reject(&'static str),
    /// The live tile's picture.
    Show,
    /// Also its input's remembered picture, once confirmed (`pending_fate`).
    Keep,
}

/// Judges a frame: `before` is what was in front when it was asked for (at `asked`),
/// `after` when it came back; `closing`, the picker started closing meanwhile; `launch`,
/// when an app or input was last launched. Any change in between, even there and back,
/// and any launch since the input came in front (the app in front is reported late),
/// files it nowhere.
#[cfg_attr(not(feature = "webos"), allow(dead_code))]
pub fn judge_frame(before: &Front, after: &Front, asked: Instant, launch: Option<Instant>, closing: bool, blank: bool) -> Verdict {
    if closing {
        return Verdict::Reject("closing");
    }
    if before != after {
        return Verdict::Reject("input changed");
    }
    if before.app.is_none() {
        return Verdict::Reject("nothing in front");
    }
    if launch.is_some_and(|l| l >= before.since) {
        return Verdict::Reject("switching");
    }
    let stable = asked.saturating_duration_since(before.since);
    if stable < PREVIEW_SETTLE {
        return Verdict::Reject("settling");
    }
    if blank {
        return Verdict::Reject("blank");
    }
    if stable >= before.keep_after() { Verdict::Keep } else { Verdict::Show }
}

/// What becomes of a `Keep` frame waiting to be remembered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(feature = "webos"), allow(dead_code))]
pub enum Fate {
    Save,
    Wait,
    Drop,
}

/// A `Keep` frame taken under `front`, back at `done`. It is surely its input's when
/// something was launched after it came back (that input was on screen until then:
/// the user's pick), or when the picker closes before any launch, or when the same
/// input is still seen in front `PREVIEW_CONFIRM` later. A change of the app in front
/// with no launch after the frame (a switch from elsewhere, reported late) drops it.
#[cfg_attr(not(feature = "webos"), allow(dead_code))]
pub fn pending_fate(front: &Front, done: Instant, now_front: &Front, now: Instant, launch: Option<Instant>, closing: bool) -> Fate {
    if launch.is_some_and(|l| l >= done) {
        return Fate::Save;
    }
    if front != now_front {
        return Fate::Drop;
    }
    if closing || now.saturating_duration_since(done) >= PREVIEW_CONFIRM {
        return Fate::Save;
    }
    Fate::Wait
}

/// A small picture decoded off the UI thread (an app's icon).
#[derive(Clone)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    /// RGBA, top row first.
    pub rgba: Arc<Vec<u8>>,
}

/// An installed app, as the Apps list shows it.
#[derive(Clone)]
pub struct AppInfo {
    pub id: String,
    pub title: String,
    /// Its launch point's parameters (None: a plain launch).
    pub params: Option<serde_json::Value>,
    /// The colour LG paints behind its icon (`iconColor`): some icons are white on transparent.
    pub color: Option<[u8; 3]>,
    /// None when no PNG could be read: the list draws a generic icon.
    pub icon: Option<Image>,
}

/// The installed apps, asked for only while their list is open.
#[derive(Clone)]
pub enum Apps {
    Loading,
    Ready(Vec<AppInfo>),
    Failed(String),
}

/// Pixels beyond which an icon is not decoded (a launcher icon is 80 to 400): 4 MiB of
/// RGBA at most, briefly, before `shrink` brings it down to what a row or card draws.
#[cfg_attr(not(feature = "webos"), allow(dead_code))] // used by the TV backend
pub const ICON_MAX_PX: u32 = 1024;

/// `img` brought down to at most `max` pixels a side by averaging whole blocks (alpha
/// weighted, so transparent pixels do not darken the edges); as it is when it fits.
#[cfg_attr(not(feature = "webos"), allow(dead_code))] // used by the TV backend (and tests)
pub fn shrink(img: Image, max: usize) -> Image {
    let f = img.width.max(img.height).div_ceil(max.max(1));
    if f <= 1 {
        return img;
    }
    let (w, h) = (img.width.div_ceil(f), img.height.div_ceil(f));
    let mut out = Vec::with_capacity(w * h * 4);
    for by in 0..h {
        for bx in 0..w {
            let (mut sum, mut alpha, mut n) = ([0u64; 3], 0u64, 0u64);
            for y in by * f..((by + 1) * f).min(img.height) {
                for x in bx * f..((bx + 1) * f).min(img.width) {
                    let o = (y * img.width + x) * 4;
                    let a = img.rgba[o + 3] as u64;
                    for c in 0..3 {
                        sum[c] += img.rgba[o + c] as u64 * a;
                    }
                    alpha += a;
                    n += 1;
                }
            }
            let ch = |c: usize| sum[c].checked_div(alpha).unwrap_or(0) as u8;
            out.extend_from_slice(&[ch(0), ch(1), ch(2), (alpha / n.max(1)) as u8]);
        }
    }
    Image { width: w, height: h, rgba: Arc::new(out) }
}

/// Decodes a PNG of at most `max`×`max` pixels to RGBA. Palette, grey, 16-bit and RGB
/// images are expanded by the decoder's transformations; anything else is None.
#[cfg(any(feature = "desktop", feature = "webos"))]
#[cfg_attr(not(feature = "webos"), allow(dead_code))] // used by the TV backend (and tests)
pub fn decode_png(bytes: &[u8], max: u32) -> Option<Image> {
    use png::{BitDepth, ColorType};
    // A header claiming a huge picture fails here, before any allocation.
    let mut d = png::Decoder::new_with_limits(std::io::Cursor::new(bytes), png::Limits { bytes: 8 << 20 });
    d.set_transformations(png::Transformations::normalize_to_color8());
    let mut r = d.read_info().ok()?;
    let (w, h) = r.info().size();
    if w == 0 || h == 0 || w > max || h > max {
        return None;
    }
    let mut buf = vec![0u8; r.output_buffer_size()?];
    let frame = r.next_frame(&mut buf).ok()?;
    if frame.bit_depth != BitDepth::Eight {
        return None;
    }
    let (w, h) = (frame.width as usize, frame.height as usize);
    let data = buf.get(..frame.buffer_size())?;
    let channels = match frame.color_type {
        ColorType::Grayscale => 1,
        ColorType::GrayscaleAlpha => 2,
        ColorType::Rgb => 3,
        ColorType::Rgba => 4,
        ColorType::Indexed => return None, // expanded to RGB(A) by the transformations
    };
    let stride = frame.line_size;
    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let row = data.get(y * stride..y * stride + w * channels)?;
        for px in row.chunks_exact(channels) {
            rgba.extend_from_slice(&match *px {
                [g] => [g, g, g, 255],
                [g, a] => [g, g, g, a],
                [r, g, b] => [r, g, b, 255],
                [r, g, b, a] => [r, g, b, a],
                _ => return None,
            });
        }
    }
    Some(Image { width: w, height: h, rgba: Arc::new(rgba) })
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
    /// The Denon/Marantz receiver on the network, if any.
    pub avr: crate::denon::AvrState,
    /// Addresses of Denon/Marantz receivers the TV already knows (their media servers).
    pub receiver_hints: Vec<String>,
    /// The installed apps while their list is open (None otherwise: nothing is kept).
    pub apps: Option<Apps>,
    /// Bumped at each new `apps` (the icons may come in a second time, from root's copies).
    pub apps_seq: u64,
    /// The answer to the last `OpenApp` (None while it is on its way).
    pub app_launch: Option<Result<(), String>>,
    /// The update check, its notes and the install.
    pub update: UpdateState,
}

/// What is known about a newer version, shown in Customize and on the Update screen.
#[derive(Clone, Default)]
pub struct UpdateState {
    /// A check is on its way.
    pub checking: bool,
    /// How the last check ended (remembered between openings): None before any.
    pub checked: Option<Result<(), String>>,
    /// The newer version found (remembered until it is installed).
    pub offer: Option<crate::update::Offer>,
    /// Its release notes, read when the Update screen opens.
    pub notes: Option<Notes>,
    /// Bumped at each new `notes`, so the UI prepares them once.
    pub notes_seq: u64,
    pub install: Option<Install>,
}

#[derive(Clone)]
pub enum Notes {
    Loading,
    Ready(crate::update::Notes),
    Failed(String),
}

/// The Homebrew Channel's install of the offer.
#[derive(Clone, Debug, PartialEq)]
pub enum Install {
    /// Its last status ("Downloading…") and, while downloading, how far (0..1).
    Running { status: String, progress: Option<f32> },
    Done,
    Failed(String),
}

pub type Shared = Arc<Mutex<TvState>>;

#[cfg_attr(feature = "webos", allow(dead_code))] // Refresh comes from the desktop preview (F5)
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
    /// Shows input `app_id` (the receiver's HDMI input; None for TV Audio, the TV's own
    /// sound) and selects input `code` on the receiver, turning it on first if needed.
    AvrSelect { app_id: Option<String>, code: String },
    /// Sends raw command `cmd` to the receiver (its setup menu's keys), first showing
    /// input `app_id` when given (where the receiver's menu appears).
    AvrKey { app_id: Option<String>, cmd: String },
    /// Lists the installed apps into `TvState::apps` (while it is still `Loading`), their
    /// icons read for rows drawing them `icon_px` pixels wide.
    /// Opens app `id` from the Apps list with its launch point's params, and says how that
    /// went in `TvState::app_launch`.
    OpenApp { id: String, params: Option<serde_json::Value> },
    ListApps {
        #[cfg_attr(not(feature = "webos"), allow(dead_code))] // the mock's icons are made, not read
        icon_px: u32,
    },
    /// Asks the feed for a newer version, in the background: `manual` now, else only when
    /// the last check is a day old.
    CheckUpdate { manual: bool },
    /// Reads the offer's release notes into `UpdateState::notes`.
    UpdateNotes,
    /// Has the Homebrew Channel install the offer.
    InstallUpdate,
}

/// A TV (or a mock): fills `Shared` from a worker thread and takes commands.
pub struct Backend {
    pub state: Shared,
    pub commands: mpsc::Sender<Command>,
    /// Live preview rate the UI wants (pictures per second, 0 = off).
    pub preview_fps: Arc<AtomicU32>,
    /// The receiver's worker (it waits for the settings from the UI).
    pub avr: crate::denon::Handle,
}

impl Backend {
    pub fn set_preview_fps(&self, fps: u32) {
        self.preview_fps.store(fps, Ordering::Relaxed);
    }
}

/// Mock inputs for the desktop preview, shaped like the C1's.
#[cfg(not(feature = "webos"))]
pub fn mock_backend(repaint: impl Fn() + Send + Clone + 'static) -> Backend {
    let state: Shared = Arc::default();
    let (tx, rx) = mpsc::channel::<Command>();
    // The receiver is a real one: the desktop preview talks to the network too, unless
    // OUTPICK_MOCK_AVR asks for a simulated one behind HDMI 2.
    let fake_avr = std::env::var_os("OUTPICK_MOCK_AVR").is_some();
    let avr = if fake_avr { crate::denon::Handle::detached() } else { crate::denon::start(state.clone(), repaint.clone()) };
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
            cec: false,
            cec_power: None,
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
            cec: false,
            cec_power: None,
        };
        {
            let mut st = s.lock().unwrap();
            let mut sources = vec![
                Source::live_tv(),
                src(1, "Mecool OTT", true, Some("TV Thoxy"), SourceIcon::Android),
                src(2, "Denon Barre de son", true, Some("AVC-S660H"), SourceIcon::Speaker),
                src(3, "PC", true, Some("Gaming PC"), SourceIcon::Pc),
                src(4, "Xbox", true, Some("Xbox Series X"), SourceIcon::Xbox),
            ];
            sources[2].brand = Some("Denon".into());
            sources[1].brand = Some("Mecool".into());
            sources[1].cec = true;
            sources[2].cec = true;
            sources[2].cec_power = Some(true);
            sources[4].label_custom = true;
            st.sources = sources;
            st.storage = vec![
                store("USB:mock", "SanDisk Ultra", SourceKind::Usb, "1"),
                store("DLNA:freebox", "Freebox Server", SourceKind::MediaServer, "Media server"),
            ];
            // OUTPICK_MOCK_FG: another app in front (an app id from `mock_apps`, say).
            st.foreground = Some(std::env::var("OUTPICK_MOCK_FG").unwrap_or_else(|_| "com.webos.app.hdmi1".into()));
            st.key_owner = KeyOwner::Lg;
            st.home = Some(Source::home("Luma"));
            st.loaded = true;
            if fake_avr {
                st.avr = mock_avr();
            }
            mock_update_start(&mut st.update);
        }
        if mock_update_has("slow") {
            // Checking on opening, for a while.
            let (s, repaint) = (s.clone(), repaint.clone());
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(30));
                mock_update_found(&mut s.lock().unwrap().update);
                repaint();
            });
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
                Command::Launch(app, params) => {
                    if std::env::var_os("OUTPICK_DEBUG").is_some() {
                        eprintln!("launch {app} {params:?}");
                    }
                    s.lock().unwrap().foreground = Some(app)
                }
                Command::Takeover(on) => s.lock().unwrap().key_owner = if on { KeyOwner::Picker } else { KeyOwner::Lg },
                Command::CaptureAccess(on) => s.lock().unwrap().capture_access = Some(on),
                Command::Cec { app_id, key } if std::env::var_os("OUTPICK_DEBUG").is_some() => eprintln!("cec {app_id} {key:#x}"),
                Command::AvrSelect { app_id, code } => {
                    if let Some(app) = app_id {
                        s.lock().unwrap().foreground = Some(app);
                    }
                    if fake_avr {
                        let mut st = s.lock().unwrap();
                        st.avr.power = Some(true);
                        st.avr.input = Some(code);
                    } else {
                        crate::denon::select(&s, &code);
                    }
                }
                Command::AvrKey { app_id, cmd } => {
                    if let Some(app) = app_id {
                        s.lock().unwrap().foreground = Some(app);
                    }
                    if fake_avr {
                        mock_avr_key(&mut s.lock().unwrap().avr, &cmd);
                    } else {
                        crate::denon::send(&s, &cmd);
                    }
                }
                Command::ListApps { .. } => {
                    // OUTPICK_MOCK_APPS: slow (loading for 5 s), empty or error, to see those states.
                    let mode = std::env::var("OUTPICK_MOCK_APPS").unwrap_or_default();
                    std::thread::sleep(std::time::Duration::from_millis(if mode == "slow" { 5000 } else { 300 }));
                    let apps = match mode.as_str() {
                        "empty" => Apps::Ready(Vec::new()),
                        "error" => Apps::Failed("Denied method call \"listLaunchPoints\"".into()),
                        _ => Apps::Ready(mock_apps()),
                    };
                    let mut st = s.lock().unwrap();
                    if matches!(st.apps, Some(Apps::Loading)) {
                        st.apps = Some(apps);
                        st.apps_seq += 1;
                    }
                }
                Command::OpenApp { id, params } => {
                    if std::env::var_os("OUTPICK_DEBUG").is_some() {
                        eprintln!("open app {id} {params:?}");
                    }
                    let mut st = s.lock().unwrap();
                    st.app_launch = Some(if std::env::var("OUTPICK_MOCK_APPS").as_deref() == Ok("launchfail") {
                        Err(format!("{id}: Application not found"))
                    } else {
                        st.foreground = Some(id);
                        Ok(())
                    });
                }
                Command::CheckUpdate { manual: true } => {
                    let (s, repaint) = (s.clone(), repaint.clone());
                    s.lock().unwrap().update.checking = true;
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(if mock_update_has("slow") { 30_000 } else { 1200 }));
                        let mut st = s.lock().unwrap();
                        let u = &mut st.update;
                        u.checking = false;
                        if mock_update_has("error") {
                            u.checked = Some(Err(MOCK_UPDATE_ERROR.into()));
                        } else if mock_update_has("available") || mock_update_has("slow") {
                            mock_update_found(u);
                        } else {
                            u.checked = Some(Ok(()));
                            u.offer = None;
                        }
                        drop(st);
                        repaint();
                    });
                }
                Command::UpdateNotes => {
                    let Some(offer) = s.lock().unwrap().update.offer.clone() else { continue };
                    {
                        let mut st = s.lock().unwrap();
                        st.update.notes = Some(Notes::Loading);
                        st.update.notes_seq += 1;
                    }
                    let (s, repaint) = (s.clone(), repaint.clone());
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(if mock_update_has("notesslow") { 30_000 } else { 400 }));
                        let notes = if mock_update_has("notesfail") {
                            Notes::Failed("curl: (28) Operation timed out after 20001 milliseconds with 0 bytes received".into())
                        } else {
                            Notes::Ready(crate::update::parse_changelog(MOCK_CHANGELOG, crate::update::INSTALLED, &offer.version))
                        };
                        let mut st = s.lock().unwrap();
                        st.update.notes = Some(notes);
                        st.update.notes_seq += 1;
                        drop(st);
                        repaint();
                    });
                }
                Command::InstallUpdate => {
                    // Downloading with its progress, verifying, installing: then done, or the
                    // checksum refused (installfail).
                    let (s, repaint) = (s.clone(), repaint.clone());
                    std::thread::spawn(move || {
                        let slow = if mock_update_has("installslow") { 10 } else { 1 };
                        let set = |i: Install| {
                            s.lock().unwrap().update.install = Some(i);
                            repaint();
                        };
                        let running = |status: &str, progress: Option<f32>| Install::Running { status: status.into(), progress };
                        set(running("Downloading…", None));
                        for p in 0..=20 {
                            std::thread::sleep(std::time::Duration::from_millis(120 * slow));
                            set(running("Downloading…", Some(p as f32 / 20.0)));
                        }
                        set(running("Verifying…", None));
                        std::thread::sleep(std::time::Duration::from_millis(600 * slow));
                        if mock_update_has("installfail") {
                            set(Install::Failed("Invalid file checksum (ab12… expected, got 9f3c…)".into()));
                            return;
                        }
                        set(running("Installing…", None));
                        std::thread::sleep(std::time::Duration::from_millis(1500 * slow));
                        set(Install::Done);
                    });
                }
                Command::Refresh | Command::Cec { .. } | Command::CheckUpdate { .. } => {}
            }
            repaint();
        }
    });
    Backend { state, commands: tx, preview_fps: Arc::new(AtomicU32::new(0)), avr }
}

/// Installed apps for the desktop preview: more than fit in either view, most with a generated icon (some
/// white on a coloured `iconColor`, as LG's are), one without (the generic icon shows).
#[cfg(not(feature = "webos"))]
fn mock_apps() -> Vec<AppInfo> {
    let icon = |hue: [u8; 3], glyph_only: bool| {
        let n = 96usize;
        let mut rgba = vec![0u8; n * n * 4];
        for y in 0..n {
            for x in 0..n {
                let (dx, dy) = (x as f32 - 47.5, y as f32 - 47.5);
                let ring = (dx * dx + dy * dy).sqrt();
                let o = (y * n + x) * 4;
                let px = if glyph_only {
                    // A white ring on transparent, drawn over the app's colour.
                    if (22.0..32.0).contains(&ring) { [255, 255, 255, 255] } else { [0; 4] }
                } else {
                    // A full tile: a diagonal gradient with a light disc.
                    let t = (x + y) as f32 / (2 * n) as f32;
                    let shade = |c: u8| (c as f32 * (0.55 + 0.45 * t)) as u8;
                    if ring < 20.0 { [245, 245, 245, 255] } else { [shade(hue[0]), shade(hue[1]), shade(hue[2]), 255] }
                };
                rgba[o..o + 4].copy_from_slice(&px);
            }
        }
        Some(Image { width: n, height: n, rgba: Arc::new(rgba) })
    };
    let app = |id: &str, title: &str, color: Option<[u8; 3]>, icon: Option<Image>| AppInfo { id: id.into(), title: title.into(), params: None, color, icon };
    let mut apps = vec![
        app("netflix", "Netflix", None, icon([200, 20, 30], false)),
        app("amazon", "Prime Video", Some([5, 120, 255]), icon([0; 3], true)),
        app("youtube.leanback.v4", "YouTube", None, icon([230, 30, 30], false)),
        app("com.disney.disneyplus-prod", "Disney+", None, icon([20, 40, 120], false)),
        app("com.apple.appletv", "Apple TV", Some([20, 20, 20]), icon([0; 3], true)),
        app("spotify-beehive", "Spotify", None, icon([30, 185, 84], false)),
        app("tv.twitch.tv.starshot.lg", "Twitch", Some([145, 70, 255]), icon([0; 3], true)),
        app("cdp-30", "Plex", None, icon([230, 160, 10], false)),
        app("org.xbmc.kodi", "Kodi", None, None),
        app("com.webos.app.browser", "Navigateur Web", Some([255, 255, 255]), icon([40, 110, 220], false)),
        app("com.webos.app.mediadiscovery", "Lecteur multimédia", None, icon([90, 60, 200], false)),
        app("org.webosbrew.hbchannel", "Homebrew Channel", Some([207, 6, 82]), icon([0; 3], true)),
        app("com.ecklf.luma", "Luma", None, icon([40, 40, 50], false)),
        app("arte.tv", "ARTE", None, icon([250, 90, 30], false)),
        app("fr.francetv.webos", "france.tv", None, icon([30, 60, 200], false)),
        app("molotov", "Molotov", None, icon([60, 60, 60], false)),
        app("com.canalplus.app", "CANAL+", None, icon([10, 10, 10], false)),
        app("tf1.plus", "TF1+", None, icon([20, 90, 200], false)),
        app("m6.plus", "M6+", Some([140, 30, 200]), icon([0; 3], true)),
        app("deezer", "Deezer", None, icon([160, 50, 220], false)),
        app("dazn", "DAZN", None, None),
        app("crunchyroll", "Crunchyroll", None, icon([245, 120, 30], false)),
        app("pluto.tv", "Pluto TV", None, icon([250, 220, 30], false)),
        app("rakuten.tv", "Rakuten TV", None, icon([200, 20, 50], false)),
        app("com.webos.app.camera", "Caméra", Some([126, 86, 177]), icon([0; 3], true)),
        app("com.webos.app.accessibility", "Accessibilité", None, icon([180, 99, 73], false)),
        // A title far too long, one in a script the font lacks (its id shows), an input
        // pinned to the home screen (a launch point with params: never ticked).
        app("com.example.long", "A streaming service with a remarkably long name indeed", None, icon([90, 90, 160], false)),
        app("jp.example.tv", "\u{65e5}\u{672c}\u{30c6}\u{30ec}\u{30d3}", None, icon([220, 40, 40], false)),
        AppInfo { params: Some(serde_json::json!({"id": "uniqueId", "value": "4"})), ..app("com.webos.app.hdmi1", "Salon", Some([109, 117, 127]), icon([0; 3], true)) },
    ];
    apps.sort_by_key(|a| a.title.to_lowercase());
    apps
}

/// OUTPICK_MOCK_UPDATE, a comma-separated list, simulates the update check without the
/// network: available (a newer version remembered from an earlier opening), uptodate,
/// error, slow (checking on opening), with notesfail / notesslow for the release notes
/// and installfail / installslow for the install. Unset: never checked yet.
#[cfg(not(feature = "webos"))]
fn mock_update_has(flag: &str) -> bool {
    std::env::var("OUTPICK_MOCK_UPDATE").is_ok_and(|v| v.split(',').any(|f| f.trim() == flag))
}

#[cfg(not(feature = "webos"))]
const MOCK_UPDATE_ERROR: &str = "curl: (60) SSL certificate problem: unable to get local issuer certificate";

#[cfg(not(feature = "webos"))]
fn mock_update_found(u: &mut UpdateState) {
    u.checking = false;
    u.checked = Some(Ok(()));
    // Two versions after this one, so the notes show both.
    u.offer = Some(crate::update::Offer {
        version: "0.2.2".into(),
        ipk_url: "https://git.thoxy.xyz/thoxy/outpick/releases/download/v0.2.2/com.thoxy.outpick_0.2.2_arm.ipk".into(),
        sha256: "ab".repeat(32),
        size: Some(1_812_345),
    });
}

#[cfg(not(feature = "webos"))]
fn mock_update_start(u: &mut UpdateState) {
    if mock_update_has("available") {
        mock_update_found(u);
    } else if mock_update_has("uptodate") {
        u.checked = Some(Ok(()));
    } else if mock_update_has("error") {
        u.checked = Some(Err(MOCK_UPDATE_ERROR.into()));
    } else if mock_update_has("slow") {
        u.checking = true;
    }
}

/// Release notes for the simulated offer, as `tools/release/changelog.py` writes them: two
/// versions after this one (and this one, which is not shown), with an arrow the font has
/// and an emoji it lacks.
#[cfg(not(feature = "webos"))]
const MOCK_CHANGELOG: &str = "## v0.2.2 — 2026-12-04

### Update check every 24 h, with the release notes and install · [`1a2b3c4`](https://git.thoxy.xyz/thoxy/outpick/commit/1a2b3c4)

- **Update:** once a day on opening, the Homebrew Channel feed is asked for a newer version in the background. The picker never waits for it, and closing never waits either.
- **Customize:** General has a Check for updates switch and an Update row: checking, up to date, the version found or the error. OK on it checks now, or opens the notes of every version since yours.
- **Install:** OK on the Update screen hands the package to the Homebrew Channel, which checks its sha256 \u{1f512} and installs it; the screen follows its progress → done.

### Apps list: icons load faster on webOS 6 · [`5d6e7f8`](https://git.thoxy.xyz/thoxy/outpick/commit/5d6e7f8)

- **Apps:** root's copies of the icons are made once per icon change instead of at every opening.

## v0.2.1 — 2026-11-20

### Receiver: the volume shows while it changes · [`9a8b7c6`](https://git.thoxy.xyz/thoxy/outpick/commit/9a8b7c6)

- **Receiver:** the volume and mute shown in the options follow the remote's keys at once, and the receiver's own answer a moment later.
- **Fix:** a receiver that took longer than 2 s to answer was dropped as unreachable; it now gets 4 s, as `denon::select` already did.

### Clock: seconds without drift · [`3c2d1e0`](https://git.thoxy.xyz/thoxy/outpick/commit/3c2d1e0)

- **Clock:** the seconds tick on the second, not a quarter second late.
- **Tests:** 9 more, with the notes parser and the feed checks run on the published repo.json.

### Sources: rename from the options, the TV's name back with an empty field · [`7e6f5a4`](https://git.thoxy.xyz/thoxy/outpick/commit/7e6f5a4)

- **Sources:** the options of an input open its rename field directly; validating an empty field gives the input its TV name back.
- **Keyboard:** LG's on-screen keyboard opens with the current name selected, so typing replaces it.
- **Layout:** a long name is cut with an ellipsis on its tile instead of running over the next one.

**Full changelog:** [v0.2.0...v0.2.1](https://git.thoxy.xyz/thoxy/outpick/compare/v0.2.0...v0.2.1)

## v0.2.0 — 2026-11-13

### Already installed: not shown · [`95f3782`](https://git.thoxy.xyz/thoxy/outpick/commit/95f3782)
";

/// What the simulated receiver does with a command: its sound mode, volume and mute change
/// here, nothing goes to the network.
#[cfg(not(feature = "webos"))]
fn mock_avr_key(avr: &mut crate::denon::AvrState, cmd: &str) {
    use crate::denon::{MUTE_OFF, MUTE_ON, VOLUME_DOWN, VOLUME_UP};
    // 1.0 per press, as `denon::send` sends two 0.5 steps.
    let step = |avr: &mut crate::denon::AvrState, d: f32| avr.volume = avr.volume.map(|v| (v + d).clamp(0.0, 98.0));
    match cmd {
        VOLUME_UP => step(avr, 1.0),
        VOLUME_DOWN => step(avr, -1.0),
        MUTE_ON => avr.muted = Some(true),
        MUTE_OFF => avr.muted = Some(false),
        _ => {
            if let Some(mode) = cmd.strip_prefix("MS") {
                avr.sound = Some(mode.to_string());
            }
        }
    }
}

/// A Denon behind HDMI 2 for the desktop preview (OUTPICK_MOCK_AVR), with a second one on
/// the network so the choice shows too.
#[cfg(not(feature = "webos"))]
fn mock_avr() -> crate::denon::AvrState {
    use crate::denon::{AvrInput, AvrState, Protocol, Receiver, Status};
    let rx = |name: &str, model: &str, ip: &str, mac: &str| Receiver {
        ip: ip.into(),
        port: 80,
        mac: mac.into(),
        model: model.into(),
        name: name.into(),
        brand: "Denon".into(),
        power: Some(true),
    };
    let main = rx("Denon AVC-S660H", "AVC-S660H", "192.168.1.42", "0006780A1B2C");
    let input = |code: &str, name: &str| AvrInput { code: code.into(), name: name.into() };
    AvrState {
        status: Status::Ready,
        found: vec![main.clone(), rx("Salon", "AVR-X1700H", "192.168.1.57", "0006780A9F31")],
        auto_pick: Some(main.mac.clone()),
        device: Some(main),
        tv_input: Some("HDMI_2".into()),
        power: Some(true),
        input: Some("GAME".into()),
        inputs: vec![
            input("SAT/CBL", "Freebox"),
            input("MPLAY", "Apple TV"),
            input("GAME", "PlayStation 5"),
            input("BD", "Blu-ray"),
            input("TV", "TV Audio"),
        ],
        sound: Some("Dolby PLII Movie".into()),
        volume: Some(45.5),
        muted: Some(false),
        protocol: Protocol::Auto,
        link: Some(Protocol::Http),
    }
}

#[cfg(test)]
mod icon_tests {
    use super::*;

    fn hdmi(f: impl FnOnce(&mut DeviceHints)) -> SourceIcon {
        let mut h = DeviceHints::default();
        f(&mut h);
        detect_icon(&h, SourceKind::Hdmi)
    }

    #[test]
    fn real_devices() {
        // LG C1, HDMI 1: Mecool KM2 (Android TV), named "TV Thoxy" over CEC.
        let mecool = hdmi(|h| {
            h.cec_name = "TV Thoxy".into();
            h.cec_vendor = Some(0x1CA410);
            h.cec_type = Some(4);
            h.spd_vendor = "Amlogic".into();
            h.spd_product = "MBox Meson Ref".into();
            h.spd_type = "Digital STB".into();
            h.lg_icon = "streamingbox.png".into();
            h.label = "Mecool OTT".into();
            h.remote_brand = "Mecool".into();
            h.remote_service = "ott".into();
        });
        assert_eq!(mecool, SourceIcon::Android);
        // Denon AV receivers over CEC (audio system).
        let denon = hdmi(|h| {
            h.cec_name = "AVC-S670H".into();
            h.cec_vendor = Some(0x0005CD);
            h.cec_type = Some(5);
            h.lg_icon = "HDMI_3.png".into();
        });
        assert_eq!(denon, SourceIcon::Speaker);
        // Nothing plugged in.
        assert_eq!(hdmi(|h| h.lg_icon = "HDMI_4.png".into()), SourceIcon::Cable);
    }

    #[test]
    fn consoles_and_pcs() {
        assert_eq!(hdmi(|h| { h.cec_name = "PlayStation 5".into(); h.cec_vendor = Some(0x080046); h.cec_type = Some(4) }), SourceIcon::PlayStation);
        assert_eq!(hdmi(|h| { h.cec_name = "Xbox Series X".into(); h.cec_type = Some(4) }), SourceIcon::Xbox);
        assert_eq!(hdmi(|h| { h.cec_name = "XBOX ONE".into() }), SourceIcon::Xbox);
        assert_eq!(hdmi(|h| h.cec_vendor = Some(0x0050F2)), SourceIcon::Xbox);
        assert_eq!(hdmi(|h| h.cec_vendor = Some(0x0009BF)), SourceIcon::NintendoSwitch);
        assert_eq!(hdmi(|h| { h.cec_name = "SHIELD".into(); h.cec_vendor = Some(0x00044B) }), SourceIcon::Android);
        assert_eq!(hdmi(|h| h.cec_name = "Chromecast".into()), SourceIcon::Android);
        assert_eq!(hdmi(|h| { h.spd_vendor = "NVIDIA".into(); h.spd_type = "PC general".into() }), SourceIcon::Pc);
        assert_eq!(hdmi(|h| h.label = "PC".into()), SourceIcon::Pc);
        assert_eq!(hdmi(|h| h.lg_icon = "pc.png".into()), SourceIcon::Pc);
        assert_eq!(hdmi(|h| { h.cec_name = "Apple TV".into(); h.cec_vendor = Some(0x0010FA) }), SourceIcon::StreamingBox);
        // "xbox" holds "box", "spc" holds "pc": neither may decide.
        assert_eq!(icon_from_tv("xbox", SourceKind::Hdmi), SourceIcon::Xbox);
        assert_eq!(icon_from_tv("hdmi_spc", SourceKind::Hdmi), SourceIcon::Cable);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hdmi(label: &str, device: Option<&str>, brand: Option<&str>) -> Source {
        Source {
            id: "HDMI_3".into(),
            app_id: "com.webos.app.hdmi3".into(),
            label: label.into(),
            kind: SourceKind::Hdmi,
            connected: true,
            device: device.map(Into::into),
            tv_icon: SourceIcon::Cable,
            params: None,
            brand: brand.map(Into::into),
            label_custom: false,
            signal: None,
            cec: false,
            cec_power: None,
        }
    }

    #[test]
    fn device_names() {
        // A model takes its maker in front; a name without digits is the user's own.
        assert_eq!(hdmi("HDMI 3", Some("AVC-S670H"), Some("Denon")).device_name(), "Denon AVC-S670H");
        assert_eq!(hdmi("HDMI 1", Some("TV Thoxy"), Some("Mecool")).device_name(), "TV Thoxy");
        assert_eq!(hdmi("HDMI 1", Some("Denon AVR-X1700H"), Some("denon")).device_name(), "Denon AVR-X1700H");
        assert_eq!(hdmi("HDMI 1", Some(" PS5 "), None).device_name(), "PS5");
        assert_eq!(hdmi("HDMI 1", None, Some("Sony")).device_name(), "Sony");
        assert_eq!(hdmi("HDMI 1", Some(""), Some(" ")).device_name(), "HDMI 1");
        // A label set in LG's settings wins.
        let mut s = hdmi("Salon", Some("AVC-S670H"), Some("Denon"));
        s.label_custom = true;
        assert_eq!(s.device_name(), "Salon");
    }

    #[test]
    fn brand_hint_and_port_name() {
        let s = hdmi("HDMI 1", Some("TV Thoxy"), Some("Mecool"));
        assert_eq!(s.brand_hint().as_deref(), Some("Mecool"));
        assert_eq!(hdmi("HDMI 3", Some("AVC-S670H"), Some("Denon")).brand_hint(), None);
        assert_eq!(hdmi("HDMI 3", Some("AVC-S670H"), Some("  ")).brand_hint(), None);
        assert_eq!(hdmi("HDMI 3", None, None).brand_hint(), None);
        assert_eq!(s.port_name(), "HDMI 3");
        assert_eq!(Source::live_tv().port_name(), "LIVE TV");
    }

    /// A BMP with a 40-byte info header; `h` < 0 is top-down.
    fn bmp(w: i32, h: i32, bpp: u16, rows: &[&[u8]]) -> Vec<u8> {
        let data: Vec<u8> = rows.concat();
        let mut b = Vec::new();
        b.extend(b"BM");
        b.extend((54 + data.len() as u32).to_le_bytes());
        b.extend([0; 4]);
        b.extend(54u32.to_le_bytes());
        b.extend(40u32.to_le_bytes());
        b.extend(w.to_le_bytes());
        b.extend(h.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend(bpp.to_le_bytes());
        b.extend(0u32.to_le_bytes()); // BI_RGB
        b.extend([0; 20]);
        b.extend(data);
        b
    }

    const RED: [u8; 4] = [255, 0, 0, 255];
    const GREEN: [u8; 4] = [0, 255, 0, 255];
    const BLUE: [u8; 4] = [0, 0, 255, 255];
    const WHITE: [u8; 4] = [255, 255, 255, 255];

    #[test]
    fn bmp_24_bit_bottom_up() {
        // Rows of 2 BGR pixels (6 bytes) padded to 8; the last row in the file is the top.
        let bottom: &[u8] = &[255, 0, 0, 255, 255, 255, 0, 0]; // blue, white
        let top: &[u8] = &[0, 0, 255, 0, 255, 0, 0, 0]; // red, green
        let (w, h, px) = decode_bmp(&bmp(2, 2, 24, &[bottom, top])).unwrap();
        assert_eq!((w, h), (2, 2));
        assert_eq!(px, [RED, GREEN, BLUE, WHITE].concat());
    }

    #[test]
    fn bmp_32_bit_top_down() {
        let top: &[u8] = &[0, 0, 255, 0, 0, 255, 0, 0]; // red, green (BGRx)
        let bottom: &[u8] = &[255, 0, 0, 0, 255, 255, 255, 0]; // blue, white
        let (w, h, px) = decode_bmp(&bmp(2, -2, 32, &[top, bottom])).unwrap();
        assert_eq!((w, h), (2, 2));
        assert_eq!(px, [RED, GREEN, BLUE, WHITE].concat());
    }

    #[test]
    fn bmp_complete_wants_the_whole_file() {
        let row: &[u8] = &[0, 0, 255, 0, 255, 0, 0, 0];
        let whole = bmp(2, -2, 24, &[row, row]);
        assert!(bmp_complete(&whole));
        // Answered before the service finished writing: rows missing.
        assert!(!bmp_complete(&whole[..whole.len() - 3]));
        assert!(!bmp_complete(&whole[..30]));
        assert!(!bmp_complete(&[]));
        // A header claiming more than is there, or less.
        let mut long = whole.clone();
        long.extend([0; 8]);
        assert!(!bmp_complete(&long));
    }

    /// A 320×180 RGBA frame from a pixel function.
    fn frame(f: impl Fn(usize, usize) -> [u8; 3]) -> Vec<u8> {
        (0..180).flat_map(|y| (0..320).map(move |x| (x, y))).flat_map(|(x, y)| {
            let [r, g, b] = f(x, y);
            [r, g, b, 255]
        }).collect()
    }

    #[test]
    fn blank_frames() {
        assert!(frame_blank(&frame(|_, _| [0, 0, 0])));
        assert!(frame_blank(&frame(|x, y| [((x + y) % 3) as u8 + 4, 4, 6]))); // black with noise
        assert!(frame_blank(&frame(|_, _| [16, 32, 160]))); // a flat blue "no signal"
        assert!(frame_blank(&frame(|x, _| if x < 4 { [255; 3] } else { [0; 3] }))); // black but a line
        assert!(frame_blank(&[]));
        // A picture, even a dark one with black bars.
        assert!(!frame_blank(&frame(|x, y| [(x % 256) as u8, y as u8, 90])));
        assert!(!frame_blank(&frame(|x, y| if !(23..157).contains(&y) { [0; 3] } else { [(x / 4) as u8 + 10, 30, 40] })));
    }

    fn front(app: &str, since: Instant) -> Front {
        Front::new(Some(app.into()), since)
    }

    #[test]
    fn front_since_moves_only_on_a_change() {
        let t = Instant::now();
        let mut f = Front::new(None, t);
        // Learning what is in front is not a switch.
        assert!(f.observe(Some("com.webos.app.hdmi1".into()), t));
        assert!(f.first);
        assert!(!f.observe(Some("com.webos.app.hdmi1".into()), t + Duration::from_secs(1)));
        assert_eq!(f.since, t);
        assert!(f.observe(Some("com.webos.app.hdmi2".into()), t + Duration::from_secs(2)));
        assert_eq!((f.app.as_deref(), f.since, f.first), (Some("com.webos.app.hdmi2"), t + Duration::from_secs(2), false));
        assert_eq!(f.keep_after(), PREVIEW_KEEP);
    }

    #[test]
    fn frames_go_only_to_the_input_steadily_in_front() {
        let ms = Duration::from_millis;
        let t = Instant::now();
        let hdmi1 = front("com.webos.app.hdmi1", t);
        let mut switched = hdmi1.clone();
        switched.first = false;
        // There when the picker opened: remembered soon; switched to: only after a while.
        assert_eq!(judge_frame(&hdmi1, &hdmi1, t + ms(900), None, false, false), Verdict::Keep);
        assert_eq!(judge_frame(&hdmi1, &hdmi1, t + ms(600), None, false, false), Verdict::Show);
        assert_eq!(judge_frame(&switched, &switched, t + ms(1500), None, false, false), Verdict::Show);
        assert_eq!(judge_frame(&switched, &switched, t + ms(2100), None, false, false), Verdict::Keep);
        // Just switched (or just opened): the picture may be black or the old input's.
        assert_eq!(judge_frame(&hdmi1, &hdmi1, t + ms(150), None, false, false), Verdict::Reject("settling"));
        // The HDMI input reported while the capture ran, or there and back.
        let live = front("com.webos.app.livetv", t);
        let after = front("com.webos.app.hdmi1", t + ms(2600));
        assert_eq!(judge_frame(&live, &after, t + ms(2500), None, false, false), Verdict::Reject("input changed"));
        let back = front("com.webos.app.livetv", t + ms(2700));
        assert_eq!(judge_frame(&live, &back, t + ms(2500), None, false, false), Verdict::Reject("input changed"));
        // A launch sent: the app in front is about to change, reported late.
        assert_eq!(judge_frame(&hdmi1, &hdmi1, t + ms(5000), Some(t + ms(4900)), false, false), Verdict::Reject("switching"));
        assert_eq!(judge_frame(&hdmi1, &hdmi1, t + ms(5000), Some(t + ms(5010)), false, false), Verdict::Reject("switching"));
        // One from before this input came in front is no matter.
        assert_eq!(judge_frame(&switched, &switched, t + ms(5000), Some(t - ms(1)), false, false), Verdict::Keep);
        assert_eq!(judge_frame(&hdmi1, &hdmi1, t + ms(5000), None, true, false), Verdict::Reject("closing"));
        // Black: the last good picture stays.
        assert_eq!(judge_frame(&hdmi1, &hdmi1, t + ms(5000), None, false, true), Verdict::Reject("blank"));
        let none = Front::new(None, t);
        assert_eq!(judge_frame(&none, &none, t + ms(5000), None, false, false), Verdict::Reject("nothing in front"));
    }

    #[test]
    fn kept_frames_wait_to_be_confirmed() {
        let ms = Duration::from_millis;
        let t = Instant::now();
        let hdmi1 = front("com.webos.app.hdmi1", t);
        let done = t + ms(3000);
        assert_eq!(pending_fate(&hdmi1, done, &hdmi1, done + ms(200), None, false), Fate::Wait);
        assert_eq!(pending_fate(&hdmi1, done, &hdmi1, done + ms(500), None, false), Fate::Save);
        // Closing with nothing launched yet: the input was still showing.
        assert_eq!(pending_fate(&hdmi1, done, &hdmi1, done + ms(100), None, true), Fate::Save);
        // Picked just after the frame: it showed the input being left.
        let hdmi2 = front("com.webos.app.hdmi2", done + ms(400));
        assert_eq!(pending_fate(&hdmi1, done, &hdmi2, done + ms(400), Some(done + ms(50)), false), Fate::Save);
        // A launch before the frame came back: it may show the switch.
        assert_eq!(pending_fate(&hdmi1, done, &hdmi1, done + ms(100), Some(done - ms(10)), true), Fate::Save);
        assert_eq!(pending_fate(&hdmi1, done, &hdmi2, done + ms(400), Some(done - ms(10)), false), Fate::Drop);
        // Switched from elsewhere, reported just after the frame: it may already show the new input.
        assert_eq!(pending_fate(&hdmi1, done, &hdmi2, done + ms(600), None, false), Fate::Drop);
        // Gone and back: not the same stretch in front.
        let mut again = front("com.webos.app.hdmi1", done + ms(300));
        again.first = false;
        assert_eq!(pending_fate(&hdmi1, done, &again, done + ms(900), None, false), Fate::Drop);
    }

    /// The preview loop's bookkeeping over a session, in ms since the process started:
    /// what the TV reports in front from when, when a launch is sent and when the picker
    /// closes. Pictures are asked for every `period` ms (and when the input's keep time
    /// comes), back 30 ms later. Returns the pictures remembered: (app, asked at).
    fn session(fronts: &[(u64, &str)], period: u64, launch: Option<u64>, close: u64) -> Vec<(String, u64)> {
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        let app_at = |ms: u64| fronts.iter().rev().find(|(m, _)| *m <= ms).map(|(_, a)| a.to_string());
        let launch_by = |ms: u64| launch.filter(|l| *l <= ms).map(at);
        let mut front = Front::new(None, t0);
        let mut pending: Option<(Front, u64, u64)> = None;
        let mut saved = Vec::new();
        let (mut ms, mut next) = (0, 0);
        loop {
            let closing = ms >= close;
            front.observe(app_at(ms), at(ms));
            if let Some((f, asked, done)) = pending.clone() {
                match pending_fate(&f, at(done), &front, at(ms), launch_by(ms), closing) {
                    Fate::Save => {
                        saved.push((f.app.unwrap(), asked));
                        pending = None;
                    }
                    Fate::Drop => pending = None,
                    Fate::Wait => {}
                }
            }
            if closing {
                return saved;
            }
            if ms >= next {
                let before = front.clone();
                let keep_at = before.since + before.keep_after();
                next = ms + period;
                if at(ms) < keep_at {
                    next = next.min(keep_at.duration_since(t0).as_millis() as u64);
                }
                let done = ms + 30;
                let mut seen = front.clone();
                seen.observe(app_at(done), at(done));
                let verdict = judge_frame(&before, &seen, at(ms), launch_by(done), done >= close, false);
                if verdict == Verdict::Keep && pending.is_none() {
                    pending = Some((before, ms, done));
                }
                ms = done;
            } else {
                ms += 10;
            }
        }
    }

    #[test]
    fn open_and_pick_keeps_the_input_left() {
        let hdmi1 = [(200, "com.webos.app.hdmi1")];
        // In front when the picker opened (known at 200 ms), picked 1.2 s later: its picture
        // is refreshed, from before the pick, at any rate.
        for period in [250, 500, 1000] {
            let kept = session(&hdmi1, period, Some(1400), 1400);
            assert!(!kept.is_empty(), "{period}");
            assert!(kept.iter().all(|(app, asked)| app == "com.webos.app.hdmi1" && *asked >= 1000 && *asked + 30 <= 1400));
        }
        // Picked after 0.5 s: too soon to trust what the TV reports.
        assert!(session(&hdmi1, 250, Some(700), 700).is_empty());
        // Closed without a pick after a while: kept too.
        assert!(!session(&hdmi1, 250, None, 1500).is_empty());
    }

    #[test]
    fn the_early_live_tv_report_files_nothing() {
        // Live TV first, corrected to the HDMI input 500 ms later, picked at 3 s.
        let fronts = [(200, "com.webos.app.livetv"), (700, "com.webos.app.hdmi1")];
        for period in [250, 1000] {
            let kept = session(&fronts, period, Some(3000), 3000);
            assert!(kept.iter().all(|(app, _)| app == "com.webos.app.hdmi1"), "{kept:?}");
            // The correction is a change: the HDMI input waits its 2 s.
            assert!(kept.iter().all(|(_, asked)| *asked >= 2700));
        }
    }

    #[test]
    fn nothing_after_a_launch_and_two_seconds_after_a_switch() {
        // A switch sent from the picker at 3 s (a receiver's input), reported at 3.6 s;
        // the picker stays open until 8 s.
        let fronts = [(200, "com.webos.app.hdmi1"), (3600, "com.webos.app.hdmi2")];
        let kept = session(&fronts, 250, Some(3000), 8000);
        assert!(kept.iter().any(|(app, _)| app == "com.webos.app.hdmi1"));
        assert!(kept.iter().any(|(app, _)| app == "com.webos.app.hdmi2"));
        for (app, asked) in &kept {
            if app == "com.webos.app.hdmi1" {
                assert!(asked + 30 <= 3000, "{asked}");
            } else {
                assert!(*asked >= 5600, "{asked}");
            }
        }
        // Switched from elsewhere (no launch here): a frame not yet confirmed is dropped.
        let kept = session(&fronts, 250, None, 8000);
        assert!(kept.iter().all(|(app, asked)| app == "com.webos.app.hdmi2" || asked + 30 + 500 <= 3600));
    }

    #[cfg(any(feature = "desktop", feature = "webos"))]
    /// A 2×1 PNG of `color` and `depth` from raw sample bytes (with a palette and its
    /// transparency for indexed ones).
    fn png(color: png::ColorType, depth: png::BitDepth, data: &[u8], palette: Option<(&[u8], &[u8])>) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, 2, 1);
        enc.set_color(color);
        enc.set_depth(depth);
        if let Some((rgb, alpha)) = palette {
            enc.set_palette(rgb.to_vec());
            enc.set_trns(alpha.to_vec());
        }
        enc.write_header().unwrap().write_image_data(data).unwrap();
        out
    }

    #[cfg(any(feature = "desktop", feature = "webos"))]
    #[test]
    fn png_every_kind_becomes_rgba() {
        use png::{BitDepth::*, ColorType::*};
        let rgba = |b: &[u8]| decode_png(b, 512).map(|i| ((i.width, i.height), i.rgba.to_vec()));
        let want = |px: [[u8; 4]; 2]| Some(((2, 1), px.concat()));
        assert_eq!(rgba(&png(Rgba, Eight, &[255, 0, 0, 255, 0, 0, 255, 128], None)), want([RED, [0, 0, 255, 128]]));
        assert_eq!(rgba(&png(Rgb, Eight, &[255, 0, 0, 0, 255, 0], None)), want([RED, GREEN]));
        assert_eq!(rgba(&png(Grayscale, Eight, &[0, 255], None)), want([[0, 0, 0, 255], WHITE]));
        assert_eq!(rgba(&png(GrayscaleAlpha, Eight, &[10, 0, 200, 255], None)), want([[10, 10, 10, 0], [200, 200, 200, 255]]));
        // 16-bit samples keep their high byte.
        assert_eq!(rgba(&png(Rgb, Sixteen, &[255, 9, 0, 0, 0, 0, 0, 0, 0, 0, 255, 9], None)), want([RED, BLUE]));
        // A palette, its second entry transparent.
        let pal = png(Indexed, Eight, &[1, 0], Some((&[0, 255, 0, 255, 0, 0], &[255, 0])));
        assert_eq!(rgba(&pal), want([[255, 0, 0, 0], GREEN]));
        // A 2-bit palette: indexes 1 then 0, packed from the high bits.
        let pal2 = png(Indexed, Two, &[0b0100_0000], Some((&[0, 0, 255, 255, 255, 255], &[255, 255])));
        assert_eq!(rgba(&pal2), want([WHITE, BLUE]));
    }

    #[test]
    fn shrink_averages_blocks() {
        let img = |w: usize, h: usize, px: Vec<[u8; 4]>| Image { width: w, height: h, rgba: Arc::new(px.concat()) };
        // Fits: as it is.
        let small = shrink(img(2, 1, vec![RED, GREEN]), 2);
        assert_eq!((small.width, small.height), (2, 1));
        // 4×2 to 2×1: each 2×2 block averaged, a transparent pixel not darkening its block.
        let clear = [0, 0, 0, 0];
        let s = shrink(img(4, 2, vec![RED, RED, WHITE, clear, RED, RED, WHITE, WHITE]), 2);
        assert_eq!((s.width, s.height), (2, 1));
        assert_eq!(*s.rgba, [[255, 0, 0, 255], [255, 255, 255, 191]].concat());
        // Uneven sides round up: 3×1 into 1 side of 2 → 2×1, the last block narrower.
        let s = shrink(img(3, 1, vec![RED, BLUE, GREEN]), 2);
        assert_eq!((s.width, s.height), (2, 1));
        assert_eq!(*s.rgba, [[127, 0, 127, 255], GREEN].concat());
        // A huge one ends within the bound.
        let big = shrink(img(300, 1, vec![WHITE; 300]), 96);
        assert!(big.width <= 96 && big.height == 1);
    }

    #[cfg(any(feature = "desktop", feature = "webos"))]
    #[test]
    fn png_too_big_or_not_png_is_none() {
        let small = png(png::ColorType::Rgb, png::BitDepth::Eight, &[0; 6], None);
        assert!(decode_png(&small, 2).is_some());
        assert!(decode_png(&small, 1).is_none());
        assert!(decode_png(&small[..small.len() - 20], 512).is_none());
        assert!(decode_png(b"GIF89a", 512).is_none());
        assert!(decode_png(&bmp(2, 2, 24, &[&[0; 8], &[0; 8]]), 512).is_none());
        assert!(decode_png(&[], 512).is_none());
    }

    #[test]
    fn bmp_bad_input_is_none() {
        let good = bmp(2, 2, 24, &[&[0; 8], &[0; 8]]);
        // The last row's padding may be missing, not its pixels.
        for n in [0, 1, 2, 20, 29, 54, good.len() - 3] {
            assert_eq!(decode_bmp(&good[..n]), None, "{n} bytes");
        }
        assert_eq!(decode_bmp(&bmp(2, 2, 8, &[&[0; 4], &[0; 4]])), None);
        assert_eq!(decode_bmp(&bmp(0, 2, 24, &[])), None);
        let mut not_bmp = good.clone();
        not_bmp[0] = b'X';
        assert_eq!(decode_bmp(&not_bmp), None);
    }
}
