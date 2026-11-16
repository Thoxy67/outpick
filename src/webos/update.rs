//! The update check on the TV: the feed and the changelog read with the TV's own curl (the
//! app has no TLS of its own), the install handed to the Homebrew Channel, which downloads
//! the package, checks its sha256 and installs it.
//!
//! curl runs as the jailed app if it can reach the server, else as root through the
//! Homebrew Channel's exec. Either way certificates are verified against the TV's CA store,
//! only https is allowed (redirects too), and time and size are bounded. The addresses are
//! constants: nothing that came from the network reaches a command line. The one value from
//! the feed handed on, the package's address, goes to the installer in a JSON payload, and
//! only once `update::valid_ipk_url` has checked it character by character.
//!
//! Each check, notes read and install runs on its own thread, never joined: closing the app
//! does not wait for the network (an unfinished check is simply not recorded, and the
//! Homebrew Channel carries an install on without us).

use super::apps::as_root;
use super::luna;
use super::takeover::is_root;
use crate::model::{Install, KeyOwner, Notes, Shared};
use crate::update::{self, APP_ID, CHANGELOG_MAX, CHANGELOG_URL, FEED_MAX, FEED_URL, INSTALLED, Offer, Remembered, curl_args, now_secs};
use serde_json::{Value, json};
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const CURL: &str = "/usr/bin/curl";
const INSTALL: &str = "luna://org.webosbrew.hbchannel.service/install";

static CHECKING: AtomicBool = AtomicBool::new(false);
static READING_NOTES: AtomicBool = AtomicBool::new(false);
static INSTALLING: AtomicBool = AtomicBool::new(false);

/// Why the app's own curl failed: unreachable from the jail (root may do better), or a
/// real answer (a certificate refused, an HTTP error), which root would only repeat.
enum Failure {
    Unreachable(String),
    Failed(String),
}

/// The app's own curl; the body, cut at `max` when `partial`.
fn direct(url: &'static str, max: usize, partial: bool) -> Result<Vec<u8>, Failure> {
    let mut child = std::process::Command::new(CURL)
        .args(curl_args(url, max, partial))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| Failure::Unreachable(format!("{CURL}: {e}")))?;
    let mut body = Vec::new();
    if let Some(out) = child.stdout.take() {
        let _ = out.take(max as u64 + 1).read_to_end(&mut body);
    }
    let over = body.len() > max;
    if over {
        // A server ignoring the range: what is needed is read.
        let _ = child.kill();
        body.truncate(max);
    }
    let mut err = String::new();
    if let Some(e) = child.stderr.take() {
        let _ = e.take(4096).read_to_string(&mut err);
    }
    let status = child.wait().map_err(|e| Failure::Failed(e.to_string()))?;
    if over {
        return if partial { Ok(body) } else { Err(Failure::Failed("the feed is too large".into())) };
    }
    if status.success() {
        return Ok(body);
    }
    let text = Some(err.trim()).filter(|e| !e.is_empty()).map(str::to_string).unwrap_or_else(|| format!("curl: {status}"));
    // 5, 6: a name not resolved; 7: no connection.
    match status.code() {
        Some(5..=7) => Err(Failure::Unreachable(text)),
        _ => Err(Failure::Failed(text)),
    }
}

/// The same curl as root, through the Homebrew Channel (its arguments are constants).
fn via_root(url: &'static str, max: usize, partial: bool) -> Result<Vec<u8>, String> {
    let args = curl_args(url, max, partial);
    debug_assert!(args.iter().all(|a| !a.contains('\'')));
    let command = format!("{CURL} {}", args.iter().map(|a| format!("'{a}'")).collect::<Vec<_>>().join(" "));
    let mut body = as_root(&command)?.into_bytes();
    if body.len() > max {
        if !partial {
            return Err("the feed is too large".into());
        }
        body.truncate(max);
    }
    Ok(body)
}

/// `url`'s body, and the way it came: "direct" (the app's curl) or "root".
pub fn fetch(url: &'static str, max: usize, partial: bool) -> Result<(Vec<u8>, &'static str), String> {
    match direct(url, max, partial) {
        Ok(b) => Ok((b, "direct")),
        Err(Failure::Failed(e)) => Err(e),
        Err(Failure::Unreachable(e)) if is_root() => Err(e),
        Err(Failure::Unreachable(e)) => via_root(url, max, partial).map(|b| (b, "root")).map_err(|r| format!("{e}; root: {r}")),
    }
}

/// The feed's answer for `installed`, and the way it came.
fn ask_feed(installed: &str) -> Result<(Option<Offer>, &'static str), String> {
    let (body, how) = fetch(FEED_URL, FEED_MAX, false)?;
    let v: Value = serde_json::from_slice(&body).map_err(|e| format!("feed: {e}"))?;
    Ok((update::parse_feed(&v, installed)?, how))
}

/// The notes of the versions after `installed` up to `upto`, and the way they came.
fn read_notes(installed: &str, upto: &str) -> Result<(update::Notes, &'static str, usize), String> {
    let (body, how) = fetch(CHANGELOG_URL, CHANGELOG_MAX, true)?;
    let cut = body.len() >= CHANGELOG_MAX;
    let mut text = String::from_utf8_lossy(&body).into_owned();
    if cut {
        // The last line may be half there.
        text.truncate(text.rfind('\n').unwrap_or(0));
    }
    let mut notes = update::parse_changelog(&text, installed, upto);
    notes.truncated |= cut && !update::reaches(&text, installed);
    Ok((notes, how, body.len()))
}

/// What the last openings found, shown at once (before any check).
pub fn load(state: &Shared) {
    let r = Remembered::load_from(&Remembered::path(), INSTALLED);
    let mut st = state.lock().unwrap();
    st.update.checked = r.checked_at.map(|_| r.error.clone().map_or(Ok(()), Err));
    st.update.offer = r.offer;
}

/// At start, once the Source key's owner is known: the first start of this version (after
/// an update, or the first install) records itself, which drops an offer it fulfils from
/// the file, and has root bring the key filter and boot hook up to this version's text,
/// should they differ (a no-op otherwise), so an update needs no reboot to finish.
pub fn after_start(owner: &KeyOwner) {
    let path = Remembered::path();
    let mut r = Remembered::load_from(&path, INSTALLED);
    if r.ran.as_deref() == Some(INSTALLED) {
        return;
    }
    // Our filter is in the chain (the jail cannot see its file, so the app may read it as
    // broken): the takeover is on.
    if matches!(owner, KeyOwner::Picker | KeyOwner::Broken(_))
        && let Err(e) = super::takeover::request(true)
    {
        eprintln!("update: key filter refresh: {e}");
        return; // tried again next time
    }
    r.ran = Some(INSTALLED.into());
    if let Err(e) = r.save_to(&path) {
        eprintln!("update: {e}");
    }
    super::trace("update: first start of this version");
}

/// A check in the background: now when `manual`, else only when the last one is a day old.
pub fn check(state: &Shared, repaint: &dyn Fn(), manual: bool) {
    let path = Remembered::path();
    if !manual && !Remembered::load_from(&path, INSTALLED).due(now_secs()) {
        return;
    }
    if CHECKING.swap(true, Ordering::SeqCst) {
        return;
    }
    state.lock().unwrap().update.checking = true;
    repaint();
    let started = Instant::now();
    let result = ask_feed(INSTALLED);
    let ms = started.elapsed().as_millis();
    // Read again: another opening may have written it meanwhile.
    let mut r = Remembered::load_from(&path, INSTALLED);
    r.record(now_secs(), &result.as_ref().map(|(o, _)| o.clone()).map_err(String::clone));
    let saved = r.save_to(&path);
    if let Err(e) = &saved {
        eprintln!("update: {e}");
    }
    {
        let mut st = state.lock().unwrap();
        let u = &mut st.update;
        u.checking = false;
        u.checked = Some(r.error.clone().map_or(Ok(()), Err));
        if u.offer != r.offer {
            u.notes = None; // another version: its own notes
        }
        u.offer = r.offer.clone();
    }
    CHECKING.store(false, Ordering::SeqCst);
    repaint();
    super::trace(&format!("update: checked in {ms} ms"));
    if super::tracing() {
        let back = Remembered::load_from(&path, INSTALLED);
        let summary = json!({
            // SAFETY: getuid has no preconditions.
            "uid": unsafe { libc::getuid() },
            "manual": manual,
            "path": result.as_ref().ok().map(|(_, how)| *how),
            "ms": ms,
            "offer": r.offer,
            "error": r.error,
            "stateFile": path.display().to_string(),
            "saved": saved.err(),
            "readBack": back == r,
        });
        let _ = std::fs::write("/tmp/outpick-update.json", summary.to_string());
    }
}

/// The offer's notes, read in the background (again after a failure).
pub fn notes(state: &Shared, repaint: &dyn Fn()) {
    let Some(offer) = state.lock().unwrap().update.offer.clone() else { return };
    if READING_NOTES.swap(true, Ordering::SeqCst) {
        return;
    }
    {
        let mut st = state.lock().unwrap();
        st.update.notes = Some(Notes::Loading);
        st.update.notes_seq += 1;
    }
    repaint();
    let result = read_notes(INSTALLED, &offer.version);
    {
        let mut st = state.lock().unwrap();
        // Still the same offer.
        if st.update.offer.as_ref() == Some(&offer) {
            st.update.notes = Some(match result {
                Ok((n, how, bytes)) => {
                    super::trace(&format!("update: notes {how} ({bytes} bytes, {} versions)", n.releases.len()));
                    Notes::Ready(n)
                }
                Err(e) => Notes::Failed(update::short_error(&e)),
            });
            st.update.notes_seq += 1;
        }
    }
    READING_NOTES.store(false, Ordering::SeqCst);
    repaint();
}

/// Has the Homebrew Channel install the offer, following its progress. The app is replaced
/// while it runs: webOS closes it once the package is installed.
pub fn install(state: &Shared, repaint: impl Fn() + Send + Sync + Clone + 'static) {
    // Checked again here: it came from a file the jailed app shares.
    let offer = state.lock().unwrap().update.offer.clone().map(Offer::checked);
    let offer = match offer {
        Some(Ok(o)) => o,
        other => {
            let why = match other {
                Some(Err(e)) => e,
                _ => "no update to install".into(),
            };
            state.lock().unwrap().update.install = Some(Install::Failed(why));
            return repaint();
        }
    };
    if INSTALLING.swap(true, Ordering::SeqCst) {
        return;
    }
    state.lock().unwrap().update.install = Some(Install::Running { status: String::new(), progress: None });
    repaint();
    let payload = json!({ "id": APP_ID, "ipkUrl": offer.ipk_url, "ipkHash": offer.sha256, "subscribe": true });
    let (shared, wake) = (state.clone(), repaint.clone());
    let sub = luna::subscribe(None, INSTALL, &payload, Duration::from_secs(10), move |v| {
        let step = update::install_step(&v);
        super::trace(&format!("update: install {v}"));
        if !matches!(step, Install::Running { .. }) {
            INSTALLING.store(false, Ordering::SeqCst);
        }
        let mut st = shared.lock().unwrap();
        // A late "Downloading…" never hides how it ended.
        if !matches!(st.update.install, Some(Install::Done | Install::Failed(_))) || !matches!(step, Install::Running { .. }) {
            st.update.install = Some(step);
        }
        drop(st);
        wake();
    });
    if let Err(e) = sub {
        INSTALLING.store(false, Ordering::SeqCst);
        let mut st = state.lock().unwrap();
        if !matches!(st.update.install, Some(Install::Failed(_))) {
            st.update.install = Some(Install::Failed(update::short_error(&e)));
        }
        drop(st);
        repaint();
    }
}

/// `outpick --check-update [installed]` (over ssh, as root or as the jailed app): asks the
/// feed and reads the notes as the app would for version `installed` (this one by default;
/// an older one shows what it would be offered), then writes and reads back a state file in
/// /tmp, never the app's own. Installs nothing.
pub fn check_cli(installed: Option<&str>) -> i32 {
    let installed = installed.unwrap_or(INSTALLED);
    let curl = std::path::Path::new(CURL).exists();
    let started = Instant::now();
    let feed = ask_feed(installed);
    let feed_ms = started.elapsed().as_millis();
    let notes = feed.as_ref().ok().and_then(|(o, _)| o.clone()).map(|offer| {
        let started = Instant::now();
        let n = read_notes(installed, &offer.version);
        let ms = started.elapsed().as_millis();
        match n {
            Ok((n, how, bytes)) => json!({
                "path": how, "ms": ms, "bytes": bytes, "truncated": n.truncated,
                "versions": n.releases.iter().map(|r| json!({ "version": r.version, "date": r.date, "entries": r.entries.len() })).collect::<Vec<_>>(),
            }),
            Err(e) => json!({ "ms": ms, "error": e }),
        }
    });
    let tmp = std::path::PathBuf::from(format!("/tmp/outpick-update-cli-{}.json", std::process::id()));
    let mut r = Remembered { ran: Some(installed.into()), ..Default::default() };
    r.record(now_secs(), &feed.as_ref().map(|(o, _)| o.clone()).map_err(String::clone));
    let saved = r.save_to(&tmp);
    let back = Remembered::load_from(&tmp, installed) == r;
    let _ = std::fs::remove_file(&tmp);
    let summary = json!({
        // SAFETY: getuid has no preconditions.
        "uid": unsafe { libc::getuid() },
        "installed": installed,
        "curl": curl,
        "feed": match &feed {
            Ok((offer, how)) => json!({ "path": how, "ms": feed_ms, "offer": offer, "upToDate": offer.is_none() }),
            Err(e) => json!({ "ms": feed_ms, "error": e }),
        },
        "notes": notes,
        "state": { "written": saved.is_ok(), "error": saved.err(), "readBack": back, "due": r.due(now_secs()) },
    });
    println!("{summary}");
    if feed.is_ok() { 0 } else { 1 }
}
