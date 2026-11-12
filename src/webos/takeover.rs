//! Source key takeover: a compositor key filter, as Glasshouse does it
//! (`glasshouse/server/lib/remotekeys.js`), that opens this picker on the
//! remote's Source/Input key instead of LG's.
//!
//! The compositor (surface-manager) runs remote keys through the JavaScript
//! filters named by the configd value `com.webos.surfacemanager.keyFilters`
//! ({file, handler} pairs, LG's own in /usr/lib/qt5/qml/KeyFilters). Ours is
//! put first; it takes only the Source keys (and Multikey, if chosen) and hands
//! every other key on. If the picker cannot be launched (uninstalled, broken,
//! the app manager silent), the filter opens LG's picker instead, so the key
//! never goes dead.
//!
//! The filter is code the compositor runs, so it lives in root's own
//! /var/lib/outpick (0755), never in the folder shared with the jailed
//! app. Its text is the same whatever keys are chosen: those are the configd
//! value `com.thoxy.outpick.keys`, which the filter follows. Changing the
//! chain makes the compositor reload every filter, LG's included (in Game mode
//! that pops the Game Dashboard), so the chain is written only to put the
//! filter in or take it out, or when a new version changes its text.
//!
//! Detection reads the live chain: whether our filter is there and its file
//! exists, whether another filter ahead of LG's takes the Input key
//! (Glasshouse's), and whether the TV has native C++ filters (webOS 11), where
//! a JavaScript filter cannot be inserted.
//!
//! Writing needs root. The app runs jailed (uid 6053) when SAM starts it, so
//! it asks the Homebrew Channel to run this binary as root with
//! `--takeover on|off`. A boot hook (`rootside::hook_script`) puts the
//! takeover back if configd loses it, and cleans up once the app is uninstalled.

use super::tv::call;
use crate::model::KeyOwner;
use crate::rootside::{self, CHAIN, CHAIN_CPP, FILTER, GLASSHOUSE_KEYS, HOOK, KEYS, OLD_FILTER, ROOT_DIR, WANTED, is_ours};
use serde_json::{Map, Value, json};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

pub const APP_ID: &str = super::APP_ID;
pub const DIR: &str = rootside::SHARED_DIR;

/// The shared folder (settings, previews) must stay writable by the jailed
/// app (uid 6053) even when root created it; sticky, so only a file's owner
/// may replace or delete it.
pub fn open_dir() -> Result<(), String> {
    open_dir_for(None)
}

/// As root for the app (`--fix-permissions <uid>`): also hands it its settings
/// file, should root own it (the sticky folder would keep the app from
/// replacing it).
pub fn open_dir_for(uid: Option<u32>) -> Result<(), String> {
    std::fs::create_dir_all(DIR).map_err(|e| format!("{DIR}: {e}"))?;
    std::fs::set_permissions(DIR, std::fs::Permissions::from_mode(0o1777)).map_err(|e| format!("{DIR}: {e}"))?;
    for name in ["config.json", "config.json.tmp"] {
        let path = format!("{DIR}/{name}");
        // Never through a link: root would change whatever it points at.
        let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
        if !meta.file_type().is_file() {
            continue;
        }
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666));
        if let Some(uid) = uid {
            use std::os::unix::fs::MetadataExt;
            if meta.uid() != uid {
                let _ = std::os::unix::fs::lchown(&path, Some(uid), None);
            }
        }
    }
    Ok(())
}

fn hbc_exec(args: &str) -> Result<Value, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let command = format!("'{}' {args}", exe.display());
    call("luna://org.webosbrew.hbchannel.service/exec", &json!({ "command": command }))
}

/// From the jailed app: has root (Homebrew Channel) open the folder up.
pub fn open_dir_as_root() -> Result<(), String> {
    // SAFETY: getuid has no preconditions.
    let uid = unsafe { libc::getuid() };
    let v = hbc_exec(&format!("--fix-permissions {uid}"))?;
    if v.get("returnValue").and_then(Value::as_bool) == Some(true) { Ok(()) } else { Err(v.to_string()) }
}

pub fn is_root() -> bool {
    // SAFETY: geteuid has no preconditions.
    unsafe { libc::geteuid() == 0 }
}

/// The named configd values present (missing ones are left out), in one call.
fn read_configs(names: &[&str]) -> Result<Map<String, Value>, String> {
    let v = call("luna://com.webos.service.config/getConfigs", &json!({ "configNames": names }))?;
    if v.get("returnValue").and_then(Value::as_bool) == Some(false) && v.get("missingConfigs").is_none() {
        return Err(format!("configd: {}", v.get("errorText").and_then(Value::as_str).unwrap_or("refused")));
    }
    Ok(v.get("configs").and_then(Value::as_object).cloned().unwrap_or_default())
}

fn chain_of(configs: &Map<String, Value>) -> Vec<Value> {
    configs.get(CHAIN).and_then(Value::as_array).cloned().unwrap_or_default()
}

fn set_config(name: &str, value: Value) -> Result<(), String> {
    let v = call("luna://com.webos.service.config/setConfigs", &json!({ "configs": { name: value } }))?;
    if v.get("returnValue").and_then(Value::as_bool) == Some(false) {
        return Err(format!("configd: {}", v.get("errorText").and_then(Value::as_str).unwrap_or("refused")));
    }
    Ok(())
}

fn write_chain(entries: Vec<Value>) -> Result<(), String> {
    set_config(CHAIN, Value::Array(entries))
}

/// Who answers the Source key now. One configd call.
pub fn detect() -> KeyOwner {
    let configs = match read_configs(&[CHAIN, CHAIN_CPP, GLASSHOUSE_KEYS]) {
        Ok(c) => c,
        Err(e) => return KeyOwner::Unknown(e),
    };
    // webOS 11: native filters, no JavaScript chain to join.
    if configs.contains_key(CHAIN_CPP) {
        return KeyOwner::Unsupported;
    }
    let lg_dir = |e: &Value| e.get("file").and_then(Value::as_str).is_some_and(|f| f.starts_with("/usr/lib/"));
    for e in &chain_of(&configs) {
        if is_ours(e) {
            let file = e.get("file").and_then(Value::as_str).unwrap_or("");
            return if Path::new(file).exists() { KeyOwner::Picker } else { KeyOwner::Broken("filter file missing".into()) };
        }
        if lg_dir(e) {
            // LG's own filters come first from here: nothing else took the key.
            break;
        }
        // A foreign filter ahead of LG's: Glasshouse maps Input from its configd value.
        let file = e.get("file").and_then(Value::as_str).unwrap_or("");
        if file.contains("tvweb") || e.get("handler").and_then(Value::as_str) == Some("handleGlasshouseKeys") {
            let input_mapped = configs
                .get(GLASSHOUSE_KEYS)
                .and_then(|v| v.get("keys"))
                .and_then(Value::as_object)
                .is_some_and(|o| o.keys().any(|n| n.to_ascii_lowercase().contains("input")));
            if input_mapped {
                return KeyOwner::Other("Glasshouse".into());
            }
        }
    }
    KeyOwner::Lg
}

/// Written beside and renamed into place: a reader never sees half a file.
pub fn write_atomic(path: &str, bytes: &[u8], mode: u32) -> Result<(), String> {
    let tmp = format!("{path}.tmp");
    let _ = std::fs::remove_file(&tmp);
    std::fs::write(&tmp, bytes).map_err(|e| format!("{tmp}: {e}"))?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(mode)).map_err(|e| format!("{tmp}: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{path}: {e}"))
}

fn write_if_changed(path: &str, text: &str, mode: u32) -> Result<bool, String> {
    if std::fs::read_to_string(path).ok().as_deref() == Some(text) {
        return Ok(false);
    }
    write_atomic(path, text.as_bytes(), mode)?;
    Ok(true)
}

/// Root's folder for the filter: a real directory, root's, 0755.
fn root_dir() -> Result<(), String> {
    if std::fs::symlink_metadata(ROOT_DIR).is_ok_and(|m| !m.file_type().is_dir()) {
        std::fs::remove_file(ROOT_DIR).map_err(|e| format!("{ROOT_DIR}: {e}"))?;
    }
    std::fs::create_dir_all(ROOT_DIR).map_err(|e| format!("{ROOT_DIR}: {e}"))?;
    let _ = std::os::unix::fs::lchown(ROOT_DIR, Some(0), Some(0));
    std::fs::set_permissions(ROOT_DIR, std::fs::Permissions::from_mode(0o755)).map_err(|e| format!("{ROOT_DIR}: {e}"))
}

/// Puts the filter in, taking the keys chosen (`source`, `multikey`), or takes
/// it out when neither is. Root only. Writes only what differs: the keys value
/// when the keys change, the chain only to add or remove the filter or when its
/// text changed.
pub fn apply(source: bool, multikey: bool) -> Result<(), String> {
    let configs = read_configs(&[CHAIN, CHAIN_CPP, KEYS])?;
    let chain = chain_of(&configs);
    if !(source || multikey) {
        let rest = rootside::chain_without_ours(chain.clone());
        if rest.len() != chain.len() {
            write_chain(rest)?;
        }
        // configd cannot delete a value: the keys value stays, read by nothing.
        for f in [FILTER, WANTED, OLD_FILTER] {
            let _ = std::fs::remove_file(f);
        }
        let _ = std::fs::remove_dir(ROOT_DIR);
        sync_hook();
        return Ok(());
    }
    if configs.contains_key(CHAIN_CPP) {
        return Err("this TV uses native key filters (webOS 11): not supported".into());
    }
    // The keys first, so the filter finds them when it starts.
    let keys = rootside::keys_value(source, multikey);
    if configs.get(KEYS) != Some(&keys) {
        set_config(KEYS, keys)?;
    }
    root_dir()?;
    let changed = write_if_changed(FILTER, &rootside::filter_source(APP_ID), 0o644)?;
    let target = rootside::chain_with_ours_first(chain.clone(), FILTER);
    if target != chain {
        // Added, moved first, or moved here from the shared folder (earlier builds).
        write_chain(target)?;
    } else if changed {
        // Same entry, new code: the compositor rereads the files only when the chain changes.
        write_chain(rootside::chain_without_ours(chain))?;
        write_chain(target)?;
    }
    let _ = std::fs::remove_file(OLD_FILTER);
    write_if_changed(WANTED, "on\n", 0o644)?;
    sync_hook();
    Ok(())
}

/// `--takeover on|off` (the UI's Source key row): on takes the keys chosen in
/// the saved settings, the Source key at least when nothing else is chosen.
pub fn set(on: bool) -> Result<(), String> {
    if !on {
        return apply(false, false);
    }
    let c = crate::config::Config::load();
    apply(c.source_key_opens || !c.multikey_opens, c.multikey_opens)
}

/// From the app: as root directly, else through the Homebrew Channel's root exec.
pub fn request(on: bool) -> Result<(), String> {
    if is_root() {
        return set(on);
    }
    let v = hbc_exec(&format!("--takeover {}", if on { "on" } else { "off" }))?;
    if v.get("returnValue").and_then(Value::as_bool) == Some(true) {
        Ok(())
    } else {
        let err = v.get("stderrString").or_else(|| v.get("errorText")).and_then(Value::as_str).unwrap_or("Homebrew Channel refused");
        Err(err.trim().to_string())
    }
}

/// From the app at launch: earlier builds kept the filter in the shared
/// (world-writable) folder. While it is still there and the chain points at it,
/// has root move it to /var/lib/outpick (the chain reloads once). Costs
/// nothing once moved: only a file check.
pub fn migrate() {
    // Capture access granted to the app's id before the Outpick rename: move it to the new id.
    if let Ok(wl) = std::fs::read_to_string(rootside::WHITELIST_USER)
        && wl.contains(rootside::LEGACY_APP_ID)
        && !wl.contains(super::APP_ID)
        && let Err(e) = super::capture::request(true)
    {
        eprintln!("capture access move: {e}");
    }
    if !Path::new(OLD_FILTER).exists() {
        return;
    }
    let Ok(configs) = read_configs(&[CHAIN]) else { return };
    let old = chain_of(&configs)
        .iter()
        .any(|e| is_ours(e) && e.get("file").and_then(Value::as_str) != Some(FILTER));
    if old && let Err(e) = request(true) {
        eprintln!("key filter move: {e}");
    }
}

/// Installs the boot hook while the takeover or the capture access is on,
/// removes it once both are off. Root only.
pub fn sync_hook() {
    let wanted = Path::new(WANTED).exists() || super::capture::status() == Some(true);
    if wanted {
        let _ = std::fs::create_dir_all(rootside::HOOK_DIR);
        // A copy, not a link into the app: it must outlive an uninstall.
        if std::fs::symlink_metadata(HOOK).is_ok_and(|m| !m.file_type().is_file()) {
            let _ = std::fs::remove_file(HOOK);
        }
        if let Err(e) = write_if_changed(HOOK, &rootside::hook_script(APP_ID), 0o755) {
            eprintln!("boot hook: {e}");
        }
    } else {
        let _ = std::fs::remove_file(HOOK);
    }
}

/// `outpick --boot`, from the boot hook (root): puts the takeover back
/// if it is wanted and configd lost it (or an update changed the filter), and
/// drops the hook if nothing needs it any more. Writes nothing when all is in place.
pub fn boot() -> i32 {
    if !is_root() {
        eprintln!("outpick --boot: root only");
        return 1;
    }
    if Path::new(WANTED).exists() {
        // configd and the compositor may still be starting.
        let mut last = Ok(());
        for _ in 0..12 {
            last = set(true);
            if last.is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_secs(5));
        }
        if let Err(e) = last {
            eprintln!("outpick --boot: {e}");
        }
    }
    sync_hook();
    0
}

/// `outpick --uninstall` (root): the Source key back to LG, the app off
/// the capture whitelist, the boot hook and every file but the settings gone.
pub fn uninstall() -> i32 {
    if !is_root() {
        eprintln!("outpick --uninstall: run it as root");
        return 1;
    }
    let mut errors = Vec::new();
    if let Err(e) = apply(false, false) {
        errors.push(format!("takeover: {e}"));
    }
    if super::capture::status() == Some(true)
        && let Err(e) = super::capture::set(false)
    {
        errors.push(format!("capture access: {e}"));
    }
    let _ = std::fs::remove_file(HOOK);
    let _ = std::fs::remove_dir_all(ROOT_DIR);
    if let Ok(dir) = std::fs::read_dir(DIR) {
        for entry in dir.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if (name.starts_with("preview-") && name.ends_with(".bmp")) || name == "keyfilter.js" || name == "config.json.tmp" {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    let owner = detect();
    println!("{}", json!({ "owner": owner.code(), "ok": errors.is_empty(), "errors": errors }));
    if errors.is_empty() { 0 } else { 1 }
}

/// `outpick --takeover on|off|status` (as root over ssh, or from the app).
pub fn cli(arg: &str) -> i32 {
    let result = match arg {
        "on" => set(true),
        "off" | "restore" => set(false),
        "status" => Ok(()),
        _ => {
            eprintln!("usage: outpick --takeover on|off|status");
            return 2;
        }
    };
    let owner = detect();
    let keys = read_configs(&[KEYS]).ok().and_then(|c| c.get(KEYS).cloned());
    println!(
        "{}",
        json!({ "owner": owner.code(), "detail": owner.detail(), "keys": keys, "ok": result.is_ok(), "error": result.as_ref().err() })
    );
    if result.is_ok() { 0 } else { 1 }
}
