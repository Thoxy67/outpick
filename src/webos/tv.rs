//! The TV backend: inputs from `eim/getAllInputStatus`, the app in front from
//! a `getForegroundAppInfo` subscription, and switching with
//! `applicationManager/launch`, as Luma's `platform.rs` and Glasshouse's
//! `home.js` do. LS2 calls go through Luma's FFI; `luna-send` is the fallback.

use super::luna;
use crate::model::{Backend, Command, KeyOwner, Preview, Shared, Source, SourceKind, decode_bmp, icon_from_tv};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;
use serde_json::{Value, json};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

pub(crate) fn call(uri: &str, payload: &Value) -> Result<Value, String> {
    match luna::call_with_timeout(uri, payload, Duration::from_secs(4)) {
        Ok(v) => Ok(v),
        Err(ffi) => {
            // Unjailed root (Homebrew Channel) can always use the CLI.
            let out = std::process::Command::new("/usr/bin/luna-send")
                .args(["-n", "1", "-w", "4000", uri, &payload.to_string()])
                .stdin(std::process::Stdio::null())
                .output()
                .map_err(|e| format!("{ffi}; luna-send: {e}"))?;
            serde_json::from_slice(&out.stdout).map_err(|e| format!("{ffi}; luna-send: {e}"))
        }
    }
}

/// Pictures for the tiles. While the picker is open, the input in front is
/// captured (video plane only, so not the picker itself) a few times a second,
/// to a file in /tmp (RAM); every input keeps its last picture as
/// `preview-<id>.bmp` in the shared folder, written once per session and input
/// (`flush_preview`), shown until it is seen again. webOS 6 refuses video
/// capture even to root, so there it stops at once and the tiles stay plain.
/// Set when capture access changes, so a stopped preview loop tries again.
static PREVIEW_RETRY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Where live pictures land: tmpfs, not the eMMC.
const LIVE_PATH: &str = "/tmp/outpick-live.bmp";

/// The newest live picture (source id, BMP bytes) not yet kept in the shared folder.
static UNSAVED: std::sync::Mutex<Option<(String, Vec<u8>)>> = std::sync::Mutex::new(None);

fn preview_path(id: &str) -> String {
    format!("{}/preview-{id}.bmp", super::takeover::DIR)
}

/// Keeps the newest live picture as its input's last picture (tmp file, then rename).
pub fn flush_preview() {
    let Some((id, bmp)) = UNSAVED.lock().ok().and_then(|mut u| u.take()) else { return };
    let path = preview_path(&id);
    let tmp = format!("{path}.tmp");
    if let Err(e) = std::fs::write(&tmp, bmp).and_then(|()| std::fs::rename(&tmp, &path)) {
        eprintln!("preview: {path}: {e}");
    }
}

fn preview_loop(state: Shared, fps: Arc<AtomicU32>, repaint: impl Fn()) {
    let dir = super::takeover::DIR;
    // Last pictures from earlier sessions: every preview-<id>.bmp in the folder
    // (read straight away, not after the inputs are listed, which comes later).
    {
        let stored: Vec<(String, Preview)> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                let id = name.strip_prefix("preview-")?.strip_suffix(".bmp")?.to_string();
                let (width, height, rgba) = decode_bmp(&std::fs::read(e.path()).ok()?)?;
                Some((id, Preview { width, height, rgba: Arc::new(rgba), seq: 1, live: false }))
            })
            .collect();
        state.lock().unwrap().previews.extend(stored);
    }
    repaint();
    let _ = super::takeover::open_dir();
    let mut seq = 1u64;
    let mut via_root = false;
    // Capturing to /tmp; after two failed read-backs, straight into the shared folder.
    let mut to_tmp = true;
    let mut tmp_failures = 0;
    let mut logged = false;
    loop {
        let rate = fps.load(Ordering::Relaxed);
        if rate == 0 {
            flush_preview();
            std::thread::sleep(Duration::from_millis(250));
            continue;
        }
        let started = Instant::now();
        let mut period = Duration::from_millis(1000 / rate.max(1) as u64);
        if via_root {
            // A root shell and a luna-send process per picture: at most one a second.
            period = period.max(Duration::from_secs(1));
        }
        // The source whose app is in front (an HDMI input, or Live TV).
        let id = {
            let st = state.lock().unwrap();
            let front = st.foreground.clone().unwrap_or_default();
            if front == "com.webos.app.livetv" {
                Some("LIVE_TV".to_string())
            } else {
                st.sources.iter().find(|s| s.app_id == front && s.kind == SourceKind::Hdmi).map(|s| s.id.clone())
            }
        };
        if let Some(id) = id {
            let path = if to_tmp { LIVE_PATH.to_string() } else { preview_path(&id) };
            let payload = json!({ "path": path, "method": "VIDEO", "width": 320, "height": 180, "format": "BMP" });
            let result = if via_root {
                let cmd = format!("luna-send -n 1 -w 3000 luna://com.webos.service.capture/executeOneShot '{payload}'");
                call("luna://org.webosbrew.hbchannel.service/exec", &json!({ "command": cmd }))
                    .and_then(|v| serde_json::from_str::<Value>(v.get("stdoutString").and_then(Value::as_str).unwrap_or("{}")).map_err(|e| e.to_string()))
            } else {
                call("luna://com.webos.service.capture/executeOneShot", &payload)
            };
            match result {
                Ok(v) if v.get("returnValue").and_then(Value::as_bool) == Some(true) => {
                    let bmp = std::fs::read(&path).ok();
                    match bmp.as_deref().and_then(decode_bmp) {
                        Some((width, height, rgba)) => {
                            if !logged {
                                logged = true;
                                eprintln!("preview: {}", if via_root { "via-root" } else { "direct" });
                            }
                            tmp_failures = 0;
                            if to_tmp {
                                // A new input in front: keep the last picture of the one before.
                                if UNSAVED.lock().is_ok_and(|u| u.as_ref().is_some_and(|(prev, _)| *prev != id)) {
                                    flush_preview();
                                }
                                if let (Ok(mut u), Some(bmp)) = (UNSAVED.lock(), bmp) {
                                    *u = Some((id.clone(), bmp));
                                }
                            }
                            seq += 1;
                            let mut st = state.lock().unwrap();
                            st.preview_supported = Some(true);
                            for p in st.previews.values_mut() {
                                p.live = false;
                            }
                            st.previews.insert(id, Preview { width, height, rgba: Arc::new(rgba), seq, live: true });
                            drop(st);
                            repaint();
                        }
                        None if to_tmp => {
                            tmp_failures += 1;
                            if tmp_failures >= 2 {
                                eprintln!("preview: /tmp unreadable, capturing to the shared folder");
                                to_tmp = false;
                                continue;
                            }
                        }
                        None => {}
                    }
                }
                Ok(v) => {
                    let denied = v.get("errorText").and_then(Value::as_str).unwrap_or("").contains("PERMISSION");
                    // SAFETY: geteuid has no preconditions.
                    if denied && !via_root && unsafe { libc::geteuid() } != 0 {
                        via_root = true; // the jailed app may not capture; root may
                        continue;
                    }
                    if !denied && to_tmp {
                        // The service may refuse a path outside its usual folders: the shared one.
                        eprintln!("preview: {v}; capturing to the shared folder");
                        to_tmp = false;
                        continue;
                    }
                    eprintln!("preview: {v}");
                    state.lock().unwrap().preview_supported = Some(false);
                    repaint();
                    // Wait for capture access to change (Customize), then try again.
                    while !PREVIEW_RETRY.swap(false, Ordering::Relaxed) {
                        std::thread::sleep(Duration::from_millis(300));
                    }
                    via_root = false;
                    to_tmp = true;
                    tmp_failures = 0;
                    logged = false;
                    state.lock().unwrap().preview_supported = None;
                    continue;
                }
                Err(e) => eprintln!("preview: {e}"),
            }
        }
        std::thread::sleep(period.saturating_sub(started.elapsed()));
    }
}

/// Which home screen answers `com.webos.app.home`: Luma takes it by bind-mounting
/// its binary over /usr/bin/com.webos.app.home (and notes it in /var/run/luma-home);
/// Glasshouse Home puts a web app under the id; otherwise it is LG's own.
fn detect_home() -> Source {
    let luma_marker = std::fs::read_to_string("/var/run/luma-home").map(|s| s.contains("luma")).unwrap_or(false);
    let luma_mount = std::fs::read_to_string("/proc/mounts")
        .map(|m| m.lines().any(|l| l.contains("/usr/bin/com.webos.app.home") && !l.starts_with("none")))
        .unwrap_or(false)
        || std::fs::read_to_string("/proc/self/mountinfo")
            .map(|m| m.lines().any(|l| (l.contains("com.ecklf.luma") || l.contains("luma-home")) && l.contains("com.webos.app.home")))
            .unwrap_or(false)
        // webOS 25: Luma's manifest method mounts its staged app over LG's Home app.
        || std::path::Path::new("/var/lib/luma-home/app").exists() && std::fs::read_to_string("/proc/mounts").map(|m| m.contains("/usr/palm/applications/com.webos.app.home")).unwrap_or(false);
    if luma_marker || luma_mount {
        return Source::home("Luma");
    }
    if let Ok(v) = call("luna://com.webos.applicationManager/getAppInfo", &json!({ "id": "com.webos.app.home" })) {
        let info = v.get("appInfo");
        let s = |k: &str| info.and_then(|i| i.get(k)).and_then(Value::as_str).unwrap_or("").to_ascii_lowercase();
        if s("type") == "web" || s("folderpath").contains("tvweb") || s("title").contains("glasshouse") {
            return Source::home("Glasshouse Home");
        }
    }
    Source::home("LG")
}

/// Ends a screen saver or a blanked screen, so the picker is seen at once.
fn wake_screen() {
    let state = call("luna://com.webos.service.tvpower/power/getPowerState", &json!({}))
        .ok()
        .and_then(|v| v.get("state").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_default();
    match state.as_str() {
        s if s.starts_with("Screen Saver") => {
            let _ = call("luna://com.webos.applicationManager/closeByAppId", &json!({"id": "com.webos.app.screensaver"}));
        }
        "Screen Off" => {
            let _ = call("luna://com.webos.service.tvpower/power/turnOnScreen", &json!({}));
        }
        _ => {}
    }
}

pub fn parse_inputs(v: &Value) -> Vec<Source> {
    let mut out = Vec::new();
    for d in v.get("devices").and_then(Value::as_array).into_iter().flatten() {
        let s = |k: &str| d.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let id = s("id");
        let app_id = s("appId");
        if id.is_empty() || app_id.is_empty() {
            continue;
        }
        let kind = if id.starts_with("HDMI") { SourceKind::Hdmi } else { SourceKind::Analog };
        let connected = d
            .get("hdmiPlugIn")
            .or_else(|| d.get("connected"))
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let label = Some(s("label")).filter(|l| !l.is_empty()).unwrap_or_else(|| id.replace('_', " "));
        let sub = d.get("subList").and_then(Value::as_array).cloned().unwrap_or_default();
        let entry = |kind: &str| sub.iter().find(|x| x.get("id").and_then(Value::as_str) == Some(kind)).cloned();
        let cec = entry("SIMPLINK");
        let urcu = entry("URCU");
        let text = |v: &Option<Value>, k: &str| {
            v.as_ref().and_then(|x| x.get(k)).and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty()).map(str::to_string)
        };
        // Model: the device's own CEC name. Maker: the universal-remote brand, else the CEC vendor id.
        // Without a CEC name, the HDMI info frame's product, unless it is a chip
        // maker's reference board ("Amlogic" / "MBox Meson Ref").
        let chip = |v: &str| {
            let v = v.to_ascii_lowercase();
            ["amlogic", "realtek", "mediatek", "rockchip", "allwinner", "hisilicon", "broadcom", "mstar", "novatek"].iter().any(|c| v.contains(c))
        };
        let spd_vendor = Some(s("spdVendorName")).filter(|v| !v.trim().is_empty() && !chip(v));
        let spd_product = Some(s("spdProductDescription"))
            .filter(|p| !p.trim().is_empty() && !p.contains(" Ref") && !chip(p) && !p.to_ascii_lowercase().contains("meson"));
        let device = text(&cec, "osdName").or_else(|| spd_product.clone());
        let brand = text(&urcu, "brandName")
            .or_else(|| cec.as_ref().and_then(|c| c.get("vendorId")).and_then(Value::as_u64).and_then(crate::model::cec_vendor).map(str::to_string))
            .or(spd_vendor);
        let signal = d.get("hdmiSignalExist").and_then(Value::as_bool);
        let cec_power = cec.as_ref().and_then(|c| c.get("cecPower")).and_then(Value::as_str).and_then(|p| match p {
            "On" => Some(true),
            "Off" => Some(false),
            _ => None,
        });
        let label_custom = d.get("modified").and_then(Value::as_bool).unwrap_or(false);
        // The icon: HDMI-CEC first, then the info frame and LG's settings.
        let hints = crate::model::DeviceHints {
            cec_name: text(&cec, "osdName").unwrap_or_default(),
            cec_vendor: cec.as_ref().and_then(|c| c.get("vendorId")).and_then(Value::as_u64),
            cec_type: cec.as_ref().and_then(|c| c.get("cecpDevType")).and_then(Value::as_u64),
            spd_vendor: s("spdVendorName"),
            spd_product: s("spdProductDescription"),
            spd_type: s("spdSourceDeviceInfo"),
            lg_icon: s("icon"),
            label: label.clone(),
            remote_brand: text(&urcu, "brandName").unwrap_or_default(),
            remote_service: text(&urcu, "serviceType").unwrap_or_default(),
        };
        let tv_icon = crate::model::detect_icon(&hints, kind);
        out.push(Source { tv_icon, id, app_id, label, kind, connected, device, params: None, brand, label_custom, signal, cec: cec.is_some(), cec_power });
    }
    out
}

/// USB storage and DLNA servers, as LG's picker lists them
/// (`InputPicker/Controllers/ConnectionsInterfaces/Interfaces.qml`), opened in
/// the media player with `{"requestDevice": deviceId}`.
pub fn parse_storage(v: &Value) -> Vec<Source> {
    let mut out = Vec::new();
    for d in v.get("devices").and_then(Value::as_array).into_iter().flatten() {
        let s = |k: &str| d.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let id = s("deviceId");
        if id.is_empty() {
            continue;
        }
        let (kind, prefix, device) = match s("deviceType").as_str() {
            "usb" => {
                let port = d.get("portNum").and_then(Value::as_u64).map(|p| if p < 0x10 { p } else { p / 0x10 });
                let ejecting = matches!(s("deviceStatus").as_str(), "EJECTING" | "HIGHPRIORITY_EJECTING");
                let detail = if ejecting { "ejecting".to_string() } else { port.map(|p| p.to_string()).unwrap_or_default() };
                (SourceKind::Usb, "USB", Some(detail))
            }
            "dms" => (SourceKind::MediaServer, "DLNA", None),
            _ => continue,
        };
        let name = Some(s("deviceName")).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| prefix.to_string());
        out.push(Source {
            id: format!("{prefix}:{id}"),
            app_id: "com.webos.app.mediadiscovery".into(),
            label: name,
            kind,
            connected: true,
            device,
            tv_icon: icon_from_tv("", kind),
            params: Some(json!({ "requestDevice": id })),
            brand: None,
            label_custom: false,
            signal: None,
            cec: false,
            cec_power: None,
        });
    }
    out
}

fn refresh(state: &Shared) {
    let result = call("luna://com.webos.service.eim/getAllInputStatus", &json!({}));
    let mut st = state.lock().unwrap();
    match result {
        Ok(v) if v.get("returnValue").and_then(Value::as_bool) != Some(false) => {
            st.sources = parse_inputs(&v);
            st.error = None;
            RAW.lock().unwrap().inputs = Some(v);
        }
        Ok(v) => st.error = Some(format!("Inputs: {}", v.get("errorText").and_then(Value::as_str).unwrap_or("refused"))),
        Err(e) => st.error = Some(format!("Inputs: {e}")),
    }
    st.loaded = true;
}

fn foreground_of(v: &Value) -> Option<String> {
    v.get("foregroundAppInfo")
        .and_then(Value::as_array)
        .and_then(|l| {
            l.iter()
                .filter_map(|a| a.get("appId").and_then(Value::as_str))
                .find(|id| !id.is_empty() && *id != super::APP_ID)
        })
        .map(str::to_string)
        .or_else(|| v.get("appId").and_then(Value::as_str).filter(|id| *id != super::APP_ID).map(str::to_string))
}

/// The last session's inputs, storage, home and Source key owner, shown at once
/// on the next start while the TV is asked again (stale-while-revalidate).
/// tmpfs, so a reboot clears it.
const CACHE_PATH: &str = "/tmp/outpick.state.json";

/// The TV's last answers, kept as they came for the cache (parsed again on load).
#[derive(Default)]
struct Raw {
    inputs: Option<Value>,
    storage: Option<Value>,
    locale: Option<String>,
}
static RAW: std::sync::Mutex<Raw> = std::sync::Mutex::new(Raw { inputs: None, storage: None, locale: None });

/// Written to a temporary file and renamed, so a reader never sees half of it.
fn save_cache(state: &Shared) {
    let cache = {
        // Same order as `refresh`: state, then RAW.
        let st = state.lock().unwrap();
        let raw = RAW.lock().unwrap();
        if st.error.is_some() || raw.inputs.is_none() {
            return;
        }
        json!({
            "inputs": raw.inputs,
            "storage": raw.storage,
            "home": st.home.as_ref().and_then(|h| h.device.clone()),
            "keyOwner": [st.key_owner.code(), st.key_owner.detail()],
            "locale": raw.locale,
        })
    };
    let tmp = format!("{CACHE_PATH}.tmp");
    if std::fs::write(&tmp, cache.to_string()).is_ok() {
        let _ = std::fs::rename(&tmp, CACHE_PATH);
    }
}

fn load_cache(state: &Shared) {
    let Some(v) = std::fs::read(CACHE_PATH).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) else { return };
    let Some(inputs) = v.get("inputs").filter(|i| i.is_object()) else { return };
    let mut st = state.lock().unwrap();
    st.sources = parse_inputs(inputs);
    if let Some(storage) = v.get("storage").filter(|s| s.is_object()) {
        st.storage = parse_storage(storage);
    }
    st.home = v.get("home").and_then(Value::as_str).map(Source::home);
    let detail = v.pointer("/keyOwner/1").and_then(Value::as_str).unwrap_or("").to_string();
    st.key_owner = match v.pointer("/keyOwner/0").and_then(Value::as_str).unwrap_or("") {
        "lg" => KeyOwner::Lg,
        "picker" => KeyOwner::Picker,
        "other" => KeyOwner::Other(detail),
        "broken" => KeyOwner::Broken(detail),
        "unsupported" => KeyOwner::Unsupported,
        _ => KeyOwner::Pending, // read again by the probe
    };
    st.loaded = true;
    drop(st);
    if let Some(locale) = v.get("locale").and_then(Value::as_str) {
        crate::i18n::set_system_locale(locale);
        RAW.lock().unwrap().locale = Some(locale.to_string());
    }
    super::trace("state loaded (cache)");
}

pub fn start(repaint: impl Fn() + Send + Sync + Clone + 'static) -> (Backend, JoinHandle<()>) {
    let state: Shared = Default::default();
    // Shown at once: what the last session saw, while the TV is asked again.
    load_cache(&state);
    let (tx, rx) = mpsc::channel::<Command>();

    let repaint_for_preview = repaint.clone();
    let s = state.clone();
    let worker = std::thread::Builder::new()
        .name("tv".into())
        .spawn(move || {
            let state = s.clone();
            // The inputs first: they are what the picker shows.
            refresh(&s);
            save_cache(&s);
            super::trace("state loaded (TV)");
            repaint();

            // Slower reads on their own thread: waking the screen, the home screen in
            // use, who has the Source key, capture access.
            {
                let shared = state.clone();
                let repaint = repaint.clone();
                let _ = std::thread::Builder::new().name("probe".into()).spawn(move || {
                    // Opened from the screen saver (or a blanked screen): wake the TV.
                    wake_screen();
                    let capture = super::capture::status();
                    shared.lock().unwrap().capture_access = capture;
                    let home = detect_home();
                    let owner = super::takeover::detect();
                    {
                        let mut st = shared.lock().unwrap();
                        st.home = Some(home);
                        st.key_owner = owner;
                    }
                    repaint();
                    save_cache(&shared);
                });
            }

            // The app in front, followed live; our own window is skipped.
            {
                let shared = state.clone();
                let repaint = repaint.clone();
                let sub = luna::subscribe(
                    None,
                    "luna://com.webos.applicationManager/getForegroundAppInfo",
                    &json!({"subscribe": true, "extraInfo": true}),
                    Duration::from_secs(3),
                    move |v| {
                        if let Some(app) = foreground_of(&v) {
                            shared.lock().unwrap().foreground = Some(app);
                            repaint();
                        }
                    },
                );
                if let Err(e) = sub {
                    eprintln!("foreground subscription: {e}");
                    if let Ok(v) = call("luna://com.webos.applicationManager/getForegroundAppInfo", &json!({"extraInfo": true})) {
                        state.lock().unwrap().foreground = foreground_of(&v);
                    }
                }
            }
            repaint();

            // The TV's UI language, for the Automatic language setting.
            if let Ok(v) = call("luna://com.webos.settingsservice/getSystemSettings", &json!({"keys": ["localeInfo"]})) {
                if let Some(ui) = v.pointer("/settings/localeInfo/locales/UI").and_then(Value::as_str) {
                    crate::i18n::set_system_locale(ui);
                    RAW.lock().unwrap().locale = Some(ui.to_string());
                }
            }

            // USB sticks come and go: follow attachedstoragemanager live.
            {
                let shared = state.clone();
                let repaint = repaint.clone();
                let sub = luna::subscribe(
                    None,
                    "luna://com.webos.service.attachedstoragemanager/listDevices",
                    &json!({"deviceType": ["usb", "dms"], "subscribe": true}),
                    Duration::from_secs(3),
                    move |v| {
                        if v.get("devices").is_some() {
                            shared.lock().unwrap().storage = parse_storage(&v);
                            RAW.lock().unwrap().storage = Some(v);
                            repaint();
                        }
                    },
                );
                if let Err(e) = sub {
                    eprintln!("storage subscription: {e}");
                    if let Ok(v) = call("luna://com.webos.service.attachedstoragemanager/listDevices", &json!({"deviceType": ["usb", "dms"]})) {
                        state.lock().unwrap().storage = parse_storage(&v);
                        RAW.lock().unwrap().storage = Some(v);
                    }
                }
            }
            repaint();
            // Runs until the UI drops its sender, so a last Switch is always sent.
            while let Ok(cmd) = rx.recv() {
                match cmd {
                    Command::Refresh => {
                        refresh(&s);
                        s.lock().unwrap().key_owner = super::takeover::detect();
                        save_cache(&s);
                    }
                    Command::CaptureAccess(on) => {
                        {
                            let mut st = s.lock().unwrap();
                            st.takeover_busy = true;
                            st.takeover_error = None;
                        }
                        repaint();
                        let result = super::capture::request(on);
                        let mut st = s.lock().unwrap();
                        st.takeover_busy = false;
                        st.takeover_error = result.err();
                        st.capture_access = super::capture::status();
                        drop(st);
                        PREVIEW_RETRY.store(true, Ordering::Relaxed);
                    }
                    Command::Cec { app_id, key } => {
                        // CEC keys go to the active input's device: switch there first.
                        let front = s.lock().unwrap().foreground.clone();
                        if front.as_deref() != Some(app_id.as_str()) {
                            let _ = call("luna://com.webos.applicationManager/launch", &json!({ "id": app_id }));
                            std::thread::sleep(Duration::from_millis(2500));
                        }
                        if let Err(e) = call("luna://com.webos.service.eim/cec/sendKeyEvent", &json!({ "key": key })) {
                            eprintln!("cec key {key:#x}: {e}");
                        }
                    }
                    Command::Takeover(on) => {
                        {
                            let mut st = s.lock().unwrap();
                            st.takeover_busy = true;
                            st.takeover_error = None;
                        }
                        repaint();
                        let result = super::takeover::request(on);
                        let owner = super::takeover::detect();
                        let mut st = s.lock().unwrap();
                        st.takeover_busy = false;
                        st.takeover_error = result.err();
                        st.key_owner = owner;
                    }
                    Command::Launch(app, params) => {
                        let mut payload = json!({ "id": app });
                        if let Some(p) = params {
                            payload["params"] = p;
                        }
                        if let Err(e) = call("luna://com.webos.applicationManager/launch", &payload) {
                            eprintln!("switch to {app}: {e}");
                        }
                    }
                }
                repaint();
            }
        })
        .expect("spawn tv thread");
    let preview_fps = Arc::new(AtomicU32::new(0));
    {
        let (st, fps, r) = (state.clone(), preview_fps.clone(), repaint_for_preview);
        let _ = std::thread::Builder::new().name("preview".into()).spawn(move || preview_loop(st, fps, r));
    }
    (Backend { state, commands: tx, preview_fps }, worker)
}
