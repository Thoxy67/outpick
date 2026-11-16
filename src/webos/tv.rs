//! The TV backend: inputs from `eim/getAllInputStatus`, the app in front from
//! a `getForegroundAppInfo` subscription, and switching with
//! `applicationManager/launch`, as Luma's `platform.rs` and Glasshouse's
//! `home.js` do. LS2 calls go through Luma's FFI; `luna-send` is the fallback.

use super::luna;
use crate::model::{
    Backend, Command, Fate, Front, KeyOwner, PREVIEW_SETTLE, Preview, Shared, Source, SourceKind, Verdict, decode_bmp, frame_blank, judge_frame, pending_fate,
};
use crate::parse::{foreground_of, parse_inputs, parse_storage, receiver_addresses};
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
/// `preview-<id>.bmp` in the shared folder (`flush_preview`), shown until it is
/// seen again. A picture is filed under an input only if that input was in front,
/// unchanged, from a moment before it was asked for until after it came back, and
/// remembered only once the input has been in front a while (`crate::model::judge_frame`):
/// a switch shows black or the old input for a moment, and the app in front is
/// reported a little late. Black frames replace nothing. webOS 6 refuses video
/// capture even to root, so there it stops at once and the tiles stay plain.
/// Set when capture access changes, so a stopped preview loop tries again.
static PREVIEW_RETRY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Where live pictures land: tmpfs, not the eMMC, in a folder of our own without the
/// sticky bit, so the last picture (the capture service's, root's) can be removed
/// before the next is asked for and an old one is never read for a new one.
const LIVE_DIR: &str = "/tmp/outpick-live";

/// When an app or input was last launched from here (sent by the worker): a picture
/// taken before it shows the input being left, one taken after may show the switch.
static LAUNCHED: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);

/// Notes that a launch is about to be sent.
fn note_launch() {
    if let Ok(mut l) = LAUNCHED.lock() {
        *l = Some(Instant::now());
    }
}

fn last_launch() -> Option<Instant> {
    LAUNCHED.lock().ok().and_then(|l| *l)
}

/// The newest confirmed live picture (source id, BMP bytes) not yet kept in the shared folder.
static UNSAVED: std::sync::Mutex<Option<(String, Vec<u8>)>> = std::sync::Mutex::new(None);

fn preview_path(id: &str) -> String {
    format!("{}/preview-{id}.bmp", super::takeover::DIR)
}

/// The capture file: in /tmp, or in the shared folder when /tmp does not work.
fn live_path(to_tmp: bool) -> String {
    if to_tmp {
        let _ = std::fs::create_dir(LIVE_DIR);
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(LIVE_DIR, std::fs::Permissions::from_mode(0o777));
        format!("{LIVE_DIR}/live.bmp")
    } else {
        format!("{}/live.bmp", super::takeover::DIR)
    }
}

/// Keeps the newest confirmed live picture as its input's last picture (tmp file, then rename).
pub fn flush_preview() {
    let Some((id, bmp)) = UNSAVED.lock().ok().and_then(|mut u| u.take()) else { return };
    let path = preview_path(&id);
    // Our own tmp name: another Outpick (or root) may be writing the same picture.
    let tmp = format!("{path}.{}.tmp", std::process::id());
    if let Err(e) = std::fs::write(&tmp, bmp).and_then(|()| std::fs::rename(&tmp, &path)) {
        let _ = std::fs::remove_file(&tmp);
        eprintln!("preview: {path}: {e}");
    }
    preview_trace(&format!("saved {id}"));
}

/// With tracing on, what became of each picture, in /tmp/outpick-preview.log.
fn preview_trace(line: &str) {
    if super::tracing() {
        use std::io::Write;
        static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
        let ms = START.get_or_init(Instant::now).elapsed().as_millis();
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/outpick-preview.log") {
            let _ = writeln!(f, "{ms:>6} ms  {line}");
        }
    }
}

/// Asks for one 320×180 picture of the video plane at `path`, removing what is there
/// first (`fresh`), also by root's own shell when root takes the picture.
fn capture(path: &str, via_root: bool, fresh: bool) -> Result<Value, String> {
    if fresh {
        let _ = std::fs::remove_file(path);
    }
    let payload = json!({ "path": path, "method": "VIDEO", "width": 320, "height": 180, "format": "BMP" });
    if via_root {
        // Root's /tmp may not be the jailed app's: the folder is made there too.
        let mut rm = if path.starts_with(LIVE_DIR) { format!("mkdir -p {LIVE_DIR} && chmod 777 {LIVE_DIR}; ") } else { String::new() };
        if fresh {
            rm += &format!("rm -f '{path}'; ");
        }
        let cmd = format!("{rm}luna-send -n 1 -w 3000 luna://com.webos.service.capture/executeOneShot '{payload}'");
        call("luna://org.webosbrew.hbchannel.service/exec", &json!({ "command": cmd }))
            .and_then(|v| serde_json::from_str::<Value>(v.get("stdoutString").and_then(Value::as_str).unwrap_or("{}")).map_err(|e| e.to_string()))
    } else {
        call("luna://com.webos.service.capture/executeOneShot", &payload)
    }
}

/// Reads a capture back: only a file written since it was asked for (`asked`), and
/// whole. The service may answer before it has finished writing: wait a little for
/// the rest rather than read half a picture. Err says why there is none.
fn read_capture(path: &str, asked: std::time::SystemTime) -> Result<Vec<u8>, &'static str> {
    // File times come from a coarse clock, up to a tick behind ours.
    let asked = asked - Duration::from_millis(50);
    let mut why = "missing";
    for attempt in 0..6 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(25));
        }
        let Ok(meta) = std::fs::metadata(path) else { continue };
        if meta.modified().is_ok_and(|m| m < asked) {
            why = "old file";
            continue;
        }
        match std::fs::read(path) {
            Ok(b) if crate::model::bmp_complete(&b) => return Ok(b),
            Ok(_) => why = "incomplete",
            Err(_) => why = "unreadable",
        }
    }
    Err(why)
}

/// The app in front, and the source it is (an HDMI input, or Live TV).
fn in_front(state: &Shared) -> (Option<String>, Option<String>) {
    let st = state.lock().unwrap();
    let app = st.foreground.clone();
    let id = match app.as_deref() {
        Some("com.webos.app.livetv") => Some("LIVE_TV".to_string()),
        Some(front) => st.sources.iter().find(|s| s.app_id == front && s.kind == SourceKind::Hdmi).map(|s| s.id.clone()),
        None => None,
    };
    (app, id)
}

/// A frame to remember, waiting to be confirmed (`crate::model::pending_fate`).
struct Pending {
    id: String,
    front: Front,
    done: Instant,
    bmp: Vec<u8>,
    preview: Preview,
}

/// Settles a pending frame: remembered (for the tile and the next openings), dropped, or kept waiting.
fn settle(pending: &mut Option<Pending>, front: &Front, closing: bool, remembered: &mut std::collections::HashMap<String, Preview>) {
    let Some(p) = pending.as_ref() else { return };
    match pending_fate(&p.front, p.done, front, Instant::now(), last_launch(), closing) {
        Fate::Wait => {}
        Fate::Drop => {
            preview_trace(&format!("dropped {}", p.id));
            *pending = None;
        }
        Fate::Save => {
            let p = pending.take().unwrap();
            preview_trace(&format!("confirmed {}", p.id));
            remembered.insert(p.id.clone(), Preview { live: false, ..p.preview });
            if let Ok(mut u) = UNSAVED.lock() {
                *u = Some((p.id, p.bmp));
            }
        }
    }
}

fn preview_loop(state: Shared, fps: Arc<AtomicU32>, repaint: impl Fn()) {
    let dir = super::takeover::DIR;
    // Last pictures from earlier sessions: every preview-<id>.bmp in the folder
    // (read straight away, not after the inputs are listed, which comes later);
    // a black one kept by an earlier version is left out.
    let mut remembered: std::collections::HashMap<String, Preview> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let id = name.strip_prefix("preview-")?.strip_suffix(".bmp")?.to_string();
            let (width, height, rgba) = decode_bmp(&std::fs::read(e.path()).ok()?)?;
            if frame_blank(&rgba) {
                return None;
            }
            Some((id, Preview { width, height, rgba: Arc::new(rgba), seq: 1, live: false }))
        })
        .collect();
    state.lock().unwrap().previews.extend(remembered.clone());
    repaint();
    let _ = super::takeover::open_dir();
    let mut seq = 1u64;
    let mut via_root = false;
    // Capturing to /tmp; after two pictures not found there, into the shared folder.
    let mut to_tmp = true;
    let mut tmp_failures = 0;
    let mut logged = false;
    let mut front = Front::new(in_front(&state).0, Instant::now());
    let mut pending: Option<Pending> = None;
    let mut next = Instant::now();
    loop {
        // The app in front is looked at every 50 ms, so a change between two pictures is seen.
        let now = Instant::now();
        let (app, _) = in_front(&state);
        let rate = fps.load(Ordering::Relaxed);
        if front.observe(app, now) {
            // (Debug formatting is compiled out of the TV build.)
            preview_trace(&format!("front {}", front.app.as_deref().unwrap_or("-")));
            // A frame of the input left is kept if it was taken before a launch from here.
            settle(&mut pending, &front, false, &mut remembered);
            // The input before keeps its last confirmed picture, on its tile too.
            flush_preview();
            let mut st = state.lock().unwrap();
            let ids: Vec<String> = st.previews.iter().filter(|(_, p)| p.live).map(|(id, _)| id.clone()).collect();
            for id in ids {
                match remembered.get(&id) {
                    Some(r) if st.previews.get(&id).is_some_and(|p| p.seq == r.seq) => {}
                    Some(r) => {
                        seq += 1;
                        st.previews.insert(id, Preview { seq, ..r.clone() });
                    }
                    None => {
                        st.previews.remove(&id);
                    }
                }
            }
            for p in st.previews.values_mut() {
                p.live = false;
            }
            drop(st);
            repaint();
        }
        // Closing (or previews off): a frame from before any launch is the input's.
        settle(&mut pending, &front, rate == 0, &mut remembered);
        if rate == 0 {
            flush_preview();
            std::thread::sleep(Duration::from_millis(50));
            next = Instant::now();
            continue;
        }
        if now < next {
            std::thread::sleep((next - now).min(Duration::from_millis(50)));
            continue;
        }
        let mut period = Duration::from_millis(1000 / rate.max(1) as u64);
        if via_root {
            // A root shell and a luna-send process per picture: at most one a second.
            period = period.max(Duration::from_secs(1));
        }
        next = now + period;
        let Some(id) = in_front(&state).1 else { continue };
        let before = front.clone();
        if before.app.is_none() || now.saturating_duration_since(before.since) < PREVIEW_SETTLE {
            // Just switched or opened: nothing worth asking for yet.
            next = before.since + PREVIEW_SETTLE;
            continue;
        }
        // A picture as soon as one may be remembered, for a quick pick at a slow rate.
        let keep_at = before.since + before.keep_after();
        if now < keep_at {
            next = next.min(keep_at);
        }
        let path = live_path(to_tmp);
        let asked = std::time::SystemTime::now();
        let result = capture(&path, via_root, true);
        match result {
            Ok(v) if v.get("returnValue").and_then(Value::as_bool) == Some(true) => {
                let took = now.elapsed().as_millis();
                let bmp = read_capture(&path, asked);
                let decoded = bmp.as_deref().ok().and_then(decode_bmp);
                match (bmp, decoded) {
                    (Ok(bmp), Some((width, height, rgba))) => {
                        if !logged {
                            logged = true;
                            eprintln!("preview: {}", if via_root { "via-root" } else { "direct" });
                        }
                        tmp_failures = 0;
                        // What is in front now, as `front` would see it: a change files the picture nowhere.
                        let done = Instant::now();
                        let mut seen = front.clone();
                        seen.observe(in_front(&state).0, done);
                        let closing = fps.load(Ordering::Relaxed) == 0;
                        let verdict = judge_frame(&before, &seen, now, last_launch(), closing, frame_blank(&rgba));
                        let what = match verdict {
                            Verdict::Reject(why) => why,
                            Verdict::Show => "shown",
                            Verdict::Keep => "shown, to keep",
                        };
                        preview_trace(&format!("{id}: {what}, answered in {took} ms, read in {} ms", done.saturating_duration_since(now).as_millis().saturating_sub(took)));
                        if matches!(verdict, Verdict::Reject(_)) {
                            continue;
                        }
                        seq += 1;
                        let preview = Preview { width, height, rgba: Arc::new(rgba), seq, live: true };
                        if verdict == Verdict::Keep && pending.is_none() {
                            pending = Some(Pending { id: id.clone(), front: before, done, bmp, preview: preview.clone() });
                        }
                        let mut st = state.lock().unwrap();
                        st.preview_supported = Some(true);
                        for p in st.previews.values_mut() {
                            p.live = false;
                        }
                        st.previews.insert(id, preview);
                        drop(st);
                        repaint();
                    }
                    (Err(why), _) if to_tmp && why == "missing" => {
                        preview_trace(&format!("{id}: {why} in /tmp"));
                        tmp_failures += 1;
                        if tmp_failures >= 2 {
                            eprintln!("preview: /tmp unreadable, capturing to the shared folder");
                            to_tmp = false;
                            next = Instant::now();
                        }
                    }
                    (bmp, _) => preview_trace(&format!("{id}: {}", bmp.err().unwrap_or("undecodable"))),
                }
            }
            Ok(v) => {
                let denied = v.get("errorText").and_then(Value::as_str).unwrap_or("").contains("PERMISSION");
                // SAFETY: geteuid has no preconditions.
                if denied && !via_root && unsafe { libc::geteuid() } != 0 {
                    via_root = true; // the jailed app may not capture; root may
                    next = Instant::now();
                    continue;
                }
                if !denied && to_tmp {
                    // The service may refuse a path outside its usual folders: the shared one.
                    eprintln!("preview: {v}; capturing to the shared folder");
                    to_tmp = false;
                    next = Instant::now();
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
                next = Instant::now();
            }
            Err(e) => eprintln!("preview: {e}"),
        }
    }
}

/// `outpick --preview-probe [n] [keep]`, over ssh: takes n pictures the way the loop does,
/// and prints for each what the file looked like when the service answered and what was
/// read back. With `keep`, the last picture is not removed first (how earlier builds asked).
pub fn preview_probe(n: usize, keep: bool) -> i32 {
    use std::os::unix::fs::MetadataExt;
    let path = live_path(true);
    let state = |p: &str| match std::fs::read(p) {
        Ok(b) => {
            let header = b.get(2..6).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]));
            let ino = std::fs::metadata(p).map(|m| m.ino()).unwrap_or(0);
            json!({ "len": b.len(), "bfSize": header, "ino": ino })
        }
        Err(_) => Value::Null,
    };
    let mut via_root = false;
    for i in 0..n {
        let before = state(&path);
        let (t0, asked) = (Instant::now(), std::time::SystemTime::now());
        let v = capture(&path, via_root, !keep);
        let took = t0.elapsed().as_millis();
        let at_answer = state(&path);
        let modified_after = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .map(|m| match m.duration_since(asked) {
                Ok(d) => d.as_millis() as i64,
                Err(e) => -(e.duration().as_millis() as i64),
            });
        let ok = matches!(&v, Ok(v) if v.get("returnValue").and_then(Value::as_bool) == Some(true));
        // SAFETY: geteuid has no preconditions.
        if !ok && !via_root && matches!(&v, Ok(v) if v.to_string().contains("PERMISSION")) && unsafe { libc::geteuid() } != 0 {
            via_root = true;
        }
        let t1 = Instant::now();
        let read = read_capture(&path, asked);
        let read_ms = t1.elapsed().as_millis();
        let picture = read.as_deref().ok().and_then(decode_bmp).map(|(w, h, rgba)| {
            let luma = rgba.as_chunks::<4>().0.iter().map(|p| p[0] as u64 * 299 + p[1] as u64 * 587 + p[2] as u64 * 114).sum::<u64>() / (w * h) as u64 / 1000;
            let hash = rgba.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ *b as u64).wrapping_mul(0x0100_0000_01b3));
            json!({ "w": w, "h": h, "luma": luma, "blank": frame_blank(&rgba), "hash": format!("{:08x}", hash >> 32) })
        });
        println!(
            "{}",
            json!({ "i": i, "via_root": via_root, "before": before, "answer_ms": took, "answer": v.map_err(|e| e.to_string()).unwrap_or_else(Value::String),
                "file_at_answer": at_answer, "mtime_minus_asked_ms": modified_after, "read": read.err(), "read_ms": read_ms, "picture": picture })
        );
        std::thread::sleep(Duration::from_millis(250));
    }
    0
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
        st.receiver_hints = receiver_addresses(storage);
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
    // And what the last update check found.
    super::update::load(&state);
    let (tx, rx) = mpsc::channel::<Command>();
    let avr = crate::denon::start(state.clone(), repaint.clone());

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
                        st.key_owner = owner.clone();
                    }
                    repaint();
                    save_cache(&shared);
                    // The first start after an update brings root's pieces up to date.
                    super::update::after_start(&owner);
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
                        if let Some(app) = foreground_of(&v, super::APP_ID) {
                            shared.lock().unwrap().foreground = Some(app);
                            repaint();
                        }
                    },
                );
                if let Err(e) = sub {
                    eprintln!("foreground subscription: {e}");
                    if let Ok(v) = call("luna://com.webos.applicationManager/getForegroundAppInfo", &json!({"extraInfo": true})) {
                        state.lock().unwrap().foreground = foreground_of(&v, super::APP_ID);
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
                            {
                                let mut st = shared.lock().unwrap();
                                st.storage = parse_storage(&v);
                                st.receiver_hints = receiver_addresses(&v);
                            }
                            RAW.lock().unwrap().storage = Some(v);
                            repaint();
                        }
                    },
                );
                if let Err(e) = sub {
                    eprintln!("storage subscription: {e}");
                    if let Ok(v) = call("luna://com.webos.service.attachedstoragemanager/listDevices", &json!({"deviceType": ["usb", "dms"]})) {
                        let mut st = state.lock().unwrap();
                        st.storage = parse_storage(&v);
                        st.receiver_hints = receiver_addresses(&v);
                        drop(st);
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
                            note_launch();
                            let _ = call("luna://com.webos.applicationManager/launch", &json!({ "id": app_id }));
                            std::thread::sleep(Duration::from_millis(2500));
                        }
                        if let Err(e) = call("luna://com.webos.service.eim/cec/sendKeyEvent", &json!({ "key": key })) {
                            eprintln!("cec key {key:#x}: {e}");
                        }
                    }
                    Command::AvrSelect { app_id, code } => {
                        let front = s.lock().unwrap().foreground.clone();
                        if let Some(app) = app_id.filter(|a| front.as_deref() != Some(a.as_str())) {
                            note_launch();
                            let _ = call("luna://com.webos.applicationManager/launch", &json!({ "id": app }));
                        }
                        crate::denon::select(&s, &code);
                    }
                    Command::AvrKey { app_id, cmd } => {
                        let front = s.lock().unwrap().foreground.clone();
                        if let Some(app) = app_id.filter(|a| front.as_deref() != Some(a.as_str())) {
                            // The receiver's menu shows on its HDMI output: the TV must be on
                            // that input, picture synced, before the menu is asked for (sent
                            // earlier, it opens unseen). Wait until the input is in front (4 s
                            // at most), then for the HDMI picture to settle.
                            note_launch();
                            let _ = call("luna://com.webos.applicationManager/launch", &json!({ "id": app }));
                            for _ in 0..40 {
                                if s.lock().unwrap().foreground.as_deref() == Some(app.as_str()) {
                                    break;
                                }
                                std::thread::sleep(Duration::from_millis(100));
                            }
                            std::thread::sleep(Duration::from_millis(1500));
                        }
                        crate::denon::send(&s, &cmd);
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
                    Command::ListApps { icon_px } => {
                        super::apps::load(icon_px, |apps, report| {
                            super::trace(&format!("apps: list {} ({} icons)", report.list, report.icons.len()));
                            if super::tracing() {
                                let _ = std::fs::write("/tmp/outpick-apps.json", report.json(&apps).to_string());
                            }
                            let mut st = s.lock().unwrap();
                            // Only while the list is open: Loading, or shown and getting root's icons.
                            if matches!(st.apps, Some(crate::model::Apps::Loading | crate::model::Apps::Ready(_))) {
                                st.apps = Some(match apps {
                                    Ok(a) => crate::model::Apps::Ready(a),
                                    Err(e) => crate::model::Apps::Failed(e),
                                });
                                st.apps_seq += 1;
                            }
                            drop(st);
                            repaint();
                        });
                    }
                    // Each on a thread of its own, never joined: the network never holds up a
                    // switch, nor the exit.
                    Command::CheckUpdate { manual } => {
                        let (st, r) = (s.clone(), repaint.clone());
                        let _ = std::thread::Builder::new().name("update".into()).spawn(move || super::update::check(&st, &r, manual));
                    }
                    Command::UpdateNotes => {
                        let (st, r) = (s.clone(), repaint.clone());
                        let _ = std::thread::Builder::new().name("update-notes".into()).spawn(move || super::update::notes(&st, &r));
                    }
                    Command::InstallUpdate => {
                        let (st, r) = (s.clone(), repaint.clone());
                        let _ = std::thread::Builder::new().name("update-install".into()).spawn(move || super::update::install(&st, r));
                    }
                    Command::OpenApp { id, params } => {
                        let mut payload = json!({ "id": id });
                        if let Some(p) = params {
                            payload["params"] = p;
                        }
                        note_launch();
                        let result = match call("luna://com.webos.applicationManager/launch", &payload) {
                            Ok(v) if v.get("returnValue").and_then(Value::as_bool) == Some(true) => Ok(()),
                            Ok(v) => Err(v.get("errorText").and_then(Value::as_str).unwrap_or("refused").to_string()),
                            Err(e) => Err(e),
                        };
                        s.lock().unwrap().app_launch = Some(result);
                    }
                    Command::Launch(app, params) => {
                        let mut payload = json!({ "id": app });
                        if let Some(p) = params {
                            payload["params"] = p;
                        }
                        note_launch();
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
    (Backend { state, commands: tx, preview_fps, avr }, worker)
}
