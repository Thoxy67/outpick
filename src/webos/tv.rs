//! The TV backend: inputs from `eim/getAllInputStatus`, the app in front from
//! a `getForegroundAppInfo` subscription, and switching with
//! `applicationManager/launch`, as Luma's `platform.rs` and Glasshouse's
//! `home.js` do. LS2 calls go through Luma's FFI; `luna-send` is the fallback.

use super::luna;
use crate::model::{Backend, Command, Preview, Shared, Source, SourceKind, decode_bmp, icon_from_tv};
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
/// to `preview-<id>.bmp` in the shared folder; every input keeps its last
/// picture there, shown until it is seen again. webOS 6 refuses video
/// capture even to root, so there it stops at once and the tiles stay plain.
/// Set when capture access changes, so a stopped preview loop tries again.
static PREVIEW_RETRY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn preview_loop(state: Shared, fps: Arc<AtomicU32>, repaint: impl Fn()) {
    let dir = super::takeover::DIR;
    let path_of = |id: &str| format!("{dir}/preview-{id}.bmp");
    // Last pictures from earlier sessions.
    {
        let ids: Vec<String> = state.lock().unwrap().sources.iter().map(|s| s.id.clone()).chain([String::from("LIVE_TV")]).collect();
        let mut st = state.lock().unwrap();
        for id in ids {
            if let Some((width, height, rgba)) = std::fs::read(path_of(&id)).ok().and_then(|b| decode_bmp(&b)) {
                st.previews.insert(id, Preview { width, height, rgba: Arc::new(rgba), seq: 1, live: false });
            }
        }
    }
    repaint();
    let mut seq = 1u64;
    let mut via_root = false;
    loop {
        let rate = fps.load(Ordering::Relaxed);
        if rate == 0 {
            std::thread::sleep(Duration::from_millis(250));
            continue;
        }
        let started = Instant::now();
        let period = Duration::from_millis(1000 / rate.max(1) as u64);
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
            let _ = super::takeover::open_dir();
            let path = path_of(&id);
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
                    if let Some((width, height, rgba)) = std::fs::read(&path).ok().and_then(|b| decode_bmp(&b)) {
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
                }
                Ok(v) => {
                    let denied = v.get("errorText").and_then(Value::as_str).unwrap_or("").contains("PERMISSION");
                    // SAFETY: geteuid has no preconditions.
                    if denied && !via_root && unsafe { libc::geteuid() } != 0 {
                        via_root = true; // the jailed app may not capture; root may
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
            .map(|m| m.lines().any(|l| l.contains("com.ecklf.luma") && l.contains("com.webos.app.home")))
            .unwrap_or(false);
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
        let label_custom = d.get("modified").and_then(Value::as_bool).unwrap_or(false);
        // Icon hints: LG's icon file, its label, the HDMI info frame and the CEC device type.
        let cec_type = match cec.as_ref().and_then(|c| c.get("cecpDevType")).and_then(Value::as_u64) {
            Some(5) => "audio",
            Some(3) => "settop",
            Some(1) => "player",
            _ => "",
        };
        let hints = format!("{} {} {} {} {}", s("icon"), label, s("spdSourceDeviceInfo"), text(&urcu, "serviceType").unwrap_or_default(), cec_type);
        let tv_icon = icon_from_tv(&hints, kind);
        out.push(Source { tv_icon, id, app_id, label, kind, connected, device, params: None, brand, label_custom, signal });
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

pub fn start(repaint: impl Fn() + Send + Sync + Clone + 'static) -> (Backend, JoinHandle<()>) {
    let state: Shared = Default::default();
    let (tx, rx) = mpsc::channel::<Command>();

    let repaint_for_preview = repaint.clone();
    let s = state.clone();
    let worker = std::thread::Builder::new()
        .name("tv".into())
        .spawn(move || {
            let state = s.clone();
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
                            repaint();
                        }
                    },
                );
                if let Err(e) = sub {
                    eprintln!("storage subscription: {e}");
                    if let Ok(v) = call("luna://com.webos.service.attachedstoragemanager/listDevices", &json!({"deviceType": ["usb", "dms"]})) {
                        state.lock().unwrap().storage = parse_storage(&v);
                    }
                }
            }

            // Opened from the screen saver (or a blanked screen): wake the TV first.
            wake_screen();
            // The TV's UI language, for the Automatic language setting.
            if let Ok(v) = call("luna://com.webos.settingsservice/getSystemSettings", &json!({"keys": ["localeInfo"]})) {
                if let Some(ui) = v.pointer("/settings/localeInfo/locales/UI").and_then(Value::as_str) {
                    crate::i18n::set_system_locale(ui);
                }
            }
            refresh(&s);
            s.lock().unwrap().capture_access = super::capture::status();
            {
                let home = detect_home();
                let owner = super::takeover::detect();
                let mut st = s.lock().unwrap();
                st.home = Some(home);
                st.key_owner = owner;
            }
            repaint();
            // Runs until the UI drops its sender, so a last Switch is always sent.
            while let Ok(cmd) = rx.recv() {
                match cmd {
                    Command::Refresh => {
                        refresh(&s);
                        s.lock().unwrap().key_owner = super::takeover::detect();
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
