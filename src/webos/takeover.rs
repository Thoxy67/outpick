//! Source key takeover: a compositor key filter, as Glasshouse does it
//! (`glasshouse/server/lib/remotekeys.js`), that opens this picker on the
//! remote's Source/Input key instead of LG's.
//!
//! The compositor (surface-manager) runs remote keys through the JavaScript
//! filters named by the configd value `com.webos.surfacemanager.keyFilters`
//! ({file, handler} pairs, LG's own in /usr/lib/qt5/qml/KeyFilters). Ours is
//! put first; it takes only the two Source keys and hands every other key on.
//! If the picker cannot be launched (uninstalled, broken), the filter opens
//! LG's picker instead, so the key never goes dead.
//!
//! Detection reads the live chain: whether our filter is there and its file
//! exists, whether another filter ahead of LG's takes the Input key
//! (Glasshouse's), and whether the TV has native C++ filters (webOS 11), where
//! a JavaScript filter cannot be inserted.
//!
//! Writing the chain needs root. The app runs jailed (uid 6053) when SAM
//! starts it, so it asks the Homebrew Channel to run this binary as root with
//! `--takeover on|off`. Changing the chain makes the compositor reload every
//! filter, so it is written only when something actually changes.

use super::tv::call;
use crate::model::KeyOwner;
use serde_json::{Value, json};

pub const APP_ID: &str = super::APP_ID;
const CHAIN: &str = "com.webos.surfacemanager.keyFilters";
const CHAIN_CPP: &str = "com.webos.surfacemanager.keyFiltersCpp";
const HANDLER: &str = "handleLgSourcePicker";
pub const DIR: &str = "/media/developer/lg-c1-source";

/// The shared folder (settings + filter) must stay writable by the jailed
/// app (uid 6053) even when root created it.
pub fn open_dir() -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(DIR).map_err(|e| format!("{DIR}: {e}"))?;
    std::fs::set_permissions(DIR, std::fs::Permissions::from_mode(0o777)).map_err(|e| format!("{DIR}: {e}"))?;
    let cfg = format!("{DIR}/config.json");
    if std::path::Path::new(&cfg).exists() {
        let _ = std::fs::set_permissions(&cfg, std::fs::Permissions::from_mode(0o666));
    }
    Ok(())
}

/// From the jailed app: has root (Homebrew Channel) open the folder up.
pub fn open_dir_as_root() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let command = format!("'{}' --fix-permissions", exe.display());
    let v = call("luna://org.webosbrew.hbchannel.service/exec", &json!({ "command": command }))?;
    if v.get("returnValue").and_then(Value::as_bool) == Some(true) { Ok(()) } else { Err(v.to_string()) }
}

fn filter_path() -> String {
    format!("{DIR}/keyfilter.js")
}

fn filter_source() -> String {
    format!(
        r#"/* Written by lg-c1-source ({APP_ID}): the remote's Source key opens it.
 * First in {CHAIN}; every other key goes on to LG's filters.
 * Undo: Customize > Source key, or `source-picker --takeover off` as root. */
var lgSourcePickerId = "{APP_ID}";

function lgSourcePickerInFront() {{
    try {{
        return typeof global !== "undefined" && typeof global.isForegroundApp === "function" &&
            global.isForegroundApp(lgSourcePickerId) === true;
    }} catch (e) {{
        return false;
    }}
}}

// LG's own picker, if ours cannot be opened.
function lgSourcePickerFallback() {{
    lunaCall.send("com.webos.service.systemUIManager", "/launch",
        JSON.stringify({{ "appId": "com.webos.app.quickinputpicker" }}), function () {{}});
}}

function {HANDLER}(key, pressed, autoRepeat) {{
    try {{
        if (key !== WebOS.Key_webOS_InputSource && key !== WebOS.Key_webOS_LocalInputSource)
            return KeyPolicy.NextPolicy;
        if (lgSourcePickerInFront()) {{
            // Already open: a further press moves to the next input, as LG's does
            // (sent as Tab, which the picker reads as the Source key).
            if (!autoRepeat) global.sendKeyToFocusWindow(Qt.Key_Tab, pressed, 15);
            return KeyPolicy.Accepted;
        }}
        if (pressed && !autoRepeat) {{
            lunaCall.send("com.webos.applicationManager", "/launch",
                JSON.stringify({{ "id": lgSourcePickerId }}), function (payload) {{
                    try {{
                        var r = typeof payload === "string" ? JSON.parse(payload) : payload;
                        if (!r || r.returnValue === false) lgSourcePickerFallback();
                    }} catch (e) {{
                        lgSourcePickerFallback();
                    }}
                }});
        }}
        return KeyPolicy.Accepted;
    }} catch (e) {{
        console.warn("lg-c1-source keyfilter: " + e);
        return KeyPolicy.NextPolicy;
    }}
}}
"#
    )
}

fn read_configs(names: &[&str]) -> Result<Value, String> {
    let v = call("luna://com.webos.service.config/getConfigs", &json!({ "configNames": names }))?;
    if v.get("returnValue").and_then(Value::as_bool) == Some(false) {
        return Err(format!("configd: {}", v.get("errorText").and_then(Value::as_str).unwrap_or("refused")));
    }
    Ok(v)
}

fn chain() -> Result<Vec<Value>, String> {
    let v = read_configs(&[CHAIN])?;
    Ok(v.pointer(&format!("/configs/{}", CHAIN.replace('/', "~1")))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

fn is_ours(entry: &Value) -> bool {
    entry.get("handler").and_then(Value::as_str) == Some(HANDLER)
}

/// Who answers the Source key now.
pub fn detect() -> KeyOwner {
    // webOS 11: native filters, no JavaScript chain to join.
    if let Ok(v) = read_configs(&[CHAIN_CPP])
        && v.pointer(&format!("/configs/{CHAIN_CPP}")).is_some()
    {
        return KeyOwner::Unsupported;
    }
    let entries = match chain() {
        Ok(e) => e,
        Err(e) => return KeyOwner::Unknown(e),
    };
    let lg_dir = |e: &Value| e.get("file").and_then(Value::as_str).is_some_and(|f| f.starts_with("/usr/lib/"));
    for e in &entries {
        if is_ours(e) {
            return if std::path::Path::new(&filter_path()).exists() {
                KeyOwner::Picker
            } else {
                KeyOwner::Broken("filter file missing".into())
            };
        }
        if lg_dir(e) {
            // LG's own filters come first from here: nothing else took the key.
            break;
        }
        // A foreign filter ahead of LG's: Glasshouse maps Input from its configd value.
        let file = e.get("file").and_then(Value::as_str).unwrap_or("");
        if file.contains("tvweb") || e.get("handler").and_then(Value::as_str) == Some("handleGlasshouseKeys") {
            let input_mapped = read_configs(&["glasshouse.remoteKeys"])
                .ok()
                .and_then(|v| v.pointer("/configs/glasshouse.remoteKeys/keys").cloned())
                .and_then(|k| k.as_object().map(|o| o.keys().any(|n| n.to_ascii_lowercase().contains("input"))))
                .unwrap_or(false);
            if input_mapped {
                return KeyOwner::Other("Glasshouse".into());
            }
        }
    }
    KeyOwner::Lg
}

fn write_chain(entries: Vec<Value>) -> Result<(), String> {
    let v = call("luna://com.webos.service.config/setConfigs", &json!({ "configs": { CHAIN: entries } }))?;
    if v.get("returnValue").and_then(Value::as_bool) == Some(false) {
        return Err(format!("configd: {}", v.get("errorText").and_then(Value::as_str).unwrap_or("refused")));
    }
    Ok(())
}

/// Installs (`on`) or removes the filter. Root only.
pub fn set(on: bool) -> Result<(), String> {
    if let KeyOwner::Unsupported = detect() {
        return Err("this TV uses native key filters (webOS 11): not supported".into());
    }
    let entries = chain()?;
    let ours = entries.iter().filter(|e| is_ours(e)).count();
    let mut rest: Vec<Value> = entries.into_iter().filter(|e| !is_ours(e)).collect();
    if on {
        open_dir()?;
        let changed = std::fs::read_to_string(filter_path()).ok().as_deref() != Some(filter_source().as_str());
        std::fs::write(filter_path(), filter_source()).map_err(|e| format!("{}: {e}", filter_path()))?;
        // Already first and alone with the same code: nothing to reload.
        if ours == 1 && !changed && chain()?.first().is_some_and(is_ours) {
            return Ok(());
        }
        // A new filter version is only read when the chain changes: take ours out first.
        if ours > 0 && changed {
            write_chain(rest.clone())?;
        }
        rest.insert(0, json!({ "file": filter_path(), "handler": HANDLER }));
        write_chain(rest)
    } else {
        if ours > 0 {
            write_chain(rest)?;
        }
        let _ = std::fs::remove_file(filter_path());
        Ok(())
    }
}

/// From the app: as root directly, else through the Homebrew Channel's root exec.
pub fn request(on: bool) -> Result<(), String> {
    // SAFETY: geteuid has no preconditions.
    if unsafe { libc::geteuid() } == 0 {
        return set(on);
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let command = format!("'{}' --takeover {}", exe.display(), if on { "on" } else { "off" });
    let v = call("luna://org.webosbrew.hbchannel.service/exec", &json!({ "command": command }))?;
    if v.get("returnValue").and_then(Value::as_bool) == Some(true) {
        Ok(())
    } else {
        let err = v.get("stderrString").or_else(|| v.get("errorText")).and_then(Value::as_str).unwrap_or("Homebrew Channel refused");
        Err(err.trim().to_string())
    }
}

/// `source-picker --takeover on|off|status` (as root over ssh, or from the app).
pub fn cli(arg: &str) -> i32 {
    let result = match arg {
        "on" => set(true),
        "off" | "restore" => set(false),
        "status" => Ok(()),
        _ => {
            eprintln!("usage: source-picker --takeover on|off|status");
            return 2;
        }
    };
    let owner = detect();
    println!("{}", json!({ "owner": owner.code(), "detail": owner.detail(), "ok": result.is_ok(), "error": result.as_ref().err() }));
    if result.is_ok() { 0 } else { 1 }
}
