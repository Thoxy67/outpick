//! Video capture access for the source previews on webOS 6.
//!
//! webOS 6's capture service (/usr/sbin/captureservice) only lets clients
//! listed in its whitelist capture the video plane; anyone else, root
//! included, gets REFUSED_PERMISSION_DENIED. It reads /etc/palm/whitelist-conf.json
//! and, when present, /home/root/whitelist-conf.json, whose `forceCaptureClient`
//! list is the one to join. "On" adds this app to that list (the file made from
//! the system one if there is none), "off" takes only this app out, keeping
//! everyone else's entries, and deletes the file only when what is left says
//! nothing the system file does not. Either then restarts the service so it
//! reads it. Root only; the app asks the Homebrew Channel to run
//! `--capture-access on|off`. The pure edits are in `crate::rootside`.

use super::tv::call;
use crate::rootside::{self, WHITELIST_BACKUP as BACKUP, WHITELIST_SYSTEM as SYSTEM, WHITELIST_USER as USER};
use serde_json::{Value, json};

const CLIENT: &str = super::APP_ID;

/// The whitelist as this TV has it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Access {
    /// This app is on the list.
    pub listed: bool,
    /// The capture service holds unlisted clients to it (webOS 6; a webOS 25
    /// TV has the same files but lets the app capture anyway).
    pub enforced: bool,
}

/// None when the TV has no such list (nothing to do).
pub fn access() -> Option<Access> {
    if !std::path::Path::new(SYSTEM).exists() {
        return None;
    }
    let listed = std::fs::read_to_string(USER).is_ok_and(|t| rootside::whitelist_lists(&t, CLIENT));
    let enforced = rootside::whitelist_enforced(&std::fs::read_to_string("/etc/os-release").unwrap_or_default());
    Some(Access { listed, enforced })
}

/// Some(true) when we are on the list, Some(false) when the TV has the list and
/// we are not on it, None when the TV has no such list (nothing to do).
pub fn status() -> Option<bool> {
    access().map(|a| a.listed)
}

/// Restarts the capture service so it reads its whitelist again
/// (LS2 starts it again on the next call).
fn restart_service() {
    for entry in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
        let cmd = std::fs::read(entry.path().join("cmdline")).unwrap_or_default();
        if cmd.starts_with(b"/usr/sbin/captureservice")
            && let Ok(pid) = entry.file_name().to_string_lossy().parse::<i32>()
        {
            // SAFETY: a plain signal to a process we just identified.
            unsafe { libc::kill(pid, libc::SIGTERM) };
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(1500));
}

fn parse(text: &str) -> Option<Value> {
    serde_json::from_str(text).ok()
}

/// Root only.
pub fn set(on: bool) -> Result<(), String> {
    if status().is_none() {
        return Err("this TV has no capture whitelist (not needed)".into());
    }
    let system = std::fs::read_to_string(SYSTEM).map_err(|e| format!("{SYSTEM}: {e}"))?;
    // Earlier builds kept the user file aside here; one that says nothing more
    // than the system file is of no use.
    if let (Some(bak), Some(sys)) = (std::fs::read_to_string(BACKUP).ok().and_then(|t| parse(&t)), parse(&system))
        && rootside::whitelist_same(&bak, &sys)
    {
        let _ = std::fs::remove_file(BACKUP);
    }
    let current = std::fs::read_to_string(USER).ok();
    let changed = if on {
        let text = rootside::whitelist_add(current.as_deref().unwrap_or(&system), CLIENT).map_err(|e| format!("{USER}: {e}"))?;
        if current.as_deref().and_then(parse) == parse(&text) {
            false
        } else {
            super::takeover::write_atomic(USER, text.as_bytes(), 0o644)?;
            true
        }
    } else {
        match current {
            None => false,
            Some(cur) => match rootside::whitelist_remove(&cur, CLIENT, &system) {
                None => {
                    std::fs::remove_file(USER).map_err(|e| format!("{USER}: {e}"))?;
                    true
                }
                Some(text) if parse(&text) != parse(&cur) => {
                    super::takeover::write_atomic(USER, text.as_bytes(), 0o644)?;
                    true
                }
                Some(_) => false,
            },
        }
    };
    if changed {
        restart_service();
    }
    super::takeover::sync_hook();
    Ok(())
}

/// From the app: as root directly, else through the Homebrew Channel.
pub fn request(on: bool) -> Result<(), String> {
    if super::takeover::is_root() {
        return set(on);
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let command = format!("'{}' --capture-access {}", exe.display(), if on { "on" } else { "off" });
    let v = call("luna://org.webosbrew.hbchannel.service/exec", &json!({ "command": command }))?;
    if v.get("returnValue").and_then(Value::as_bool) == Some(true) {
        Ok(())
    } else {
        Err(v.get("stderrString").or_else(|| v.get("errorText")).and_then(Value::as_str).unwrap_or("refused").trim().to_string())
    }
}

/// `outpick --capture-access on|off|status`.
pub fn cli(arg: &str) -> i32 {
    let result = match arg {
        "on" => set(true),
        "off" | "restore" => set(false),
        "status" => Ok(()),
        _ => {
            eprintln!("usage: outpick --capture-access on|off|status");
            return 2;
        }
    };
    let a = access();
    let state = match a {
        Some(Access { listed: true, .. }) => "granted",
        Some(Access { listed: false, .. }) => "blocked",
        None => "not-needed",
    };
    println!(
        "{}",
        json!({ "access": state, "enforced": a.map(|a| a.enforced), "ok": result.is_ok(), "error": result.as_ref().err() })
    );
    if result.is_ok() { 0 } else { 1 }
}
