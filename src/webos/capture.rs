//! Video capture access for the source previews on webOS 6.
//!
//! webOS 6's capture service (/usr/sbin/captureservice) only lets clients
//! listed in its whitelist capture the video plane; anyone else, root
//! included, gets REFUSED_PERMISSION_DENIED. It reads /etc/palm/whitelist-conf.json
//! and, when present, /home/root/whitelist-conf.json, whose `forceCaptureClient`
//! list is the one to join. "On" writes that file (from the system one, keeping
//! any file already there aside as a backup) with this app added, then restarts
//! the service so it reads it; "off" puts things back as they were. Root only;
//! the app asks the Homebrew Channel to run `--capture-access on|off`.

use super::tv::call;
use serde_json::{Value, json};

const SYSTEM: &str = "/etc/palm/whitelist-conf.json";
const USER: &str = "/home/root/whitelist-conf.json";
const BACKUP: &str = "/home/root/whitelist-conf.json.lg-c1-source.bak";
const CLIENT: &str = super::APP_ID;

/// Some(true) when we are on the list, Some(false) when the TV has the list and
/// we are not on it, None when the TV has no such list (nothing to do).
pub fn status() -> Option<bool> {
    if !std::path::Path::new(SYSTEM).exists() {
        return None;
    }
    let listed = std::fs::read_to_string(USER)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.get("forceCaptureClient").and_then(Value::as_array).cloned())
        .is_some_and(|l| l.iter().any(|c| c.as_str() == Some(CLIENT)));
    Some(listed)
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

/// Root only.
pub fn set(on: bool) -> Result<(), String> {
    if status().is_none() {
        return Err("this TV has no capture whitelist (not needed)".into());
    }
    if on {
        let mut conf: Value = match std::fs::read_to_string(USER) {
            Ok(t) => {
                if !std::path::Path::new(BACKUP).exists() {
                    std::fs::write(BACKUP, &t).map_err(|e| format!("{BACKUP}: {e}"))?;
                }
                serde_json::from_str(&t).map_err(|e| format!("{USER}: {e}"))?
            }
            Err(_) => serde_json::from_str(&std::fs::read_to_string(SYSTEM).map_err(|e| format!("{SYSTEM}: {e}"))?)
                .map_err(|e| format!("{SYSTEM}: {e}"))?,
        };
        let list = conf
            .as_object_mut()
            .ok_or("whitelist is not an object")?
            .entry("forceCaptureClient")
            .or_insert_with(|| json!([]));
        let arr = list.as_array_mut().ok_or("forceCaptureClient is not a list")?;
        if !arr.iter().any(|c| c.as_str() == Some(CLIENT)) {
            arr.push(json!(CLIENT));
        }
        let text = serde_json::to_string_pretty(&conf).map_err(|e| e.to_string())?;
        std::fs::write(USER, text).map_err(|e| format!("{USER}: {e}"))?;
    } else if std::path::Path::new(BACKUP).exists() {
        std::fs::rename(BACKUP, USER).map_err(|e| format!("{USER}: {e}"))?;
    } else {
        let _ = std::fs::remove_file(USER);
    }
    restart_service();
    Ok(())
}

/// From the app: as root directly, else through the Homebrew Channel.
pub fn request(on: bool) -> Result<(), String> {
    // SAFETY: geteuid has no preconditions.
    if unsafe { libc::geteuid() } == 0 {
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

/// `source-picker --capture-access on|off|status`.
pub fn cli(arg: &str) -> i32 {
    let result = match arg {
        "on" => set(true),
        "off" | "restore" => set(false),
        "status" => Ok(()),
        _ => {
            eprintln!("usage: source-picker --capture-access on|off|status");
            return 2;
        }
    };
    let state = match status() {
        Some(true) => "granted",
        Some(false) => "blocked",
        None => "not-needed",
    };
    println!("{}", json!({ "access": state, "ok": result.is_ok(), "error": result.as_ref().err() }));
    if result.is_ok() { 0 } else { 1 }
}
