//! The installed apps for the Apps list: `applicationManager/listLaunchPoints` and their
//! icons, asked for each time the list opens (nothing is kept between openings but root's
//! copies below).
//!
//! The app runs jailed (webOS 6: uid 5912 chrooted in /var/palm/jail/<id>), where most apps'
//! folders are not mounted (/usr/palm/applications, /mnt/otncabi, the store apps'
//! /media/cryptofs). Root copies the icons it cannot read into the shared folder
//! (`outpick --copy-app-icons`, run through the Homebrew Channel), named after their path
//! and listed in an index with each file's size and time, so an updated icon is copied
//! again and an uninstalled app's dropped. The list is shown at once with the copies there
//! are, then again once root has brought them up to date. If the jailed app is refused
//! the list itself, root asks for it the same way. Which way each part went is in
//! `Report` (`outpick --list-apps` prints it; with tracing on, the app writes it to
//! /tmp/outpick-apps.json).

use super::takeover::{DIR, is_root};
use super::tv::call;
use crate::model::{AppInfo, ICON_MAX_PX, Image, decode_png, shrink};
use crate::parse::{LaunchPoint, icon_candidates, parse_launch_points};
use serde_json::{Map, Value, json};
use std::io::Read;
use std::os::unix::fs::{MetadataExt, PermissionsExt};

const LIST: &str = "luna://com.webos.applicationManager/listLaunchPoints";
const EXEC: &str = "luna://org.webosbrew.hbchannel.service/exec";
/// A launcher icon is a few KiB (400 px ones some 100): anything bigger is not read.
const MAX_BYTES: u64 = 2 << 20;

fn copies() -> String {
    format!("{DIR}/app-icons")
}

fn index_path() -> String {
    format!("{}/index.json", copies())
}

/// The copy of icon `path`: named after the path (FNV-1a), whatever characters it holds.
fn copy_name(path: &str) -> String {
    let h = path.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3));
    format!("{h:016x}.png")
}

/// One app's icon in the report: its id, "direct", "copy" (root's) or "generic", the size.
type IconNote = (String, &'static str, Option<(usize, usize)>);

/// How the list and each icon were had.
#[derive(Default)]
pub struct Report {
    /// "direct" or "root".
    pub list: &'static str,
    pub icons: Vec<IconNote>,
    /// Root was asked to bring its copies up to date: its answer.
    pub root_copy: Option<String>,
}

impl Report {
    pub fn json(&self, apps: &Result<Vec<AppInfo>, String>) -> Value {
        let count = |how: &str| self.icons.iter().filter(|i| i.1 == how).count();
        json!({
            // SAFETY: getuid has no preconditions.
            "uid": unsafe { libc::getuid() },
            "list": self.list,
            "error": apps.as_ref().err(),
            "count": self.icons.len(),
            "icons": { "direct": count("direct"), "copy": count("copy"), "generic": count("generic") },
            "rootCopy": self.root_copy,
            "apps": self.icons.iter().map(|(id, how, size)| json!({ "id": id, "icon": how, "size": size.map(|(w, h)| [w, h]) })).collect::<Vec<_>>(),
        })
    }
}

fn answered(v: &Value) -> bool {
    v.get("returnValue").and_then(Value::as_bool) == Some(true)
}

fn error_of(v: &Value) -> String {
    v.get("errorText").and_then(Value::as_str).unwrap_or("refused").to_string()
}

/// Runs `command` as root through the Homebrew Channel; its standard output.
pub(super) fn as_root(command: &str) -> Result<String, String> {
    let v = call(EXEC, &json!({ "command": command }))?;
    if !answered(&v) {
        return Err(v.get("stderrString").and_then(Value::as_str).filter(|e| !e.trim().is_empty()).map(str::to_string).unwrap_or_else(|| error_of(&v)));
    }
    Ok(v.get("stdoutString").and_then(Value::as_str).unwrap_or("").to_string())
}

/// The launch points: asked directly, else (refused to the jailed app) through root.
fn list() -> Result<(Vec<LaunchPoint>, &'static str), String> {
    let own = super::APP_ID;
    let direct = match call(LIST, &json!({})) {
        Ok(v) if answered(&v) => return Ok((parse_launch_points(&v, own), "direct")),
        Ok(v) => error_of(&v),
        Err(e) => e,
    };
    if is_root() {
        return Err(direct);
    }
    let out = as_root(&format!("luna-send -n 1 -w 4000 {LIST} '{{}}'")).map_err(|e| format!("{direct}; root: {e}"))?;
    let v: Value = serde_json::from_str(&out).map_err(|e| format!("{direct}; root: {e}"))?;
    if !answered(&v) {
        return Err(format!("{direct}; root: {}", error_of(&v)));
    }
    Ok((parse_launch_points(&v, own), "root"))
}

/// A regular file's bytes, if it is at most `MAX_BYTES` (looked at before it is opened: a
/// pipe or a device would block or never end).
fn read_small(path: &str) -> Option<Vec<u8>> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_BYTES {
        return None;
    }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    std::fs::File::open(path).ok()?.take(MAX_BYTES).read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

/// An icon file as the list draws it: a PNG, brought down to twice the size it is drawn.
fn decode(bytes: &[u8], px: u32) -> Option<Image> {
    decode_png(bytes, ICON_MAX_PX).map(|img| shrink(img, (2 * px as usize).clamp(64, 256)))
}

/// The first of `p`'s icons that reads and decodes here.
fn icon_of(p: &LaunchPoint, px: u32) -> Option<Image> {
    icon_candidates(p, px).into_iter().find_map(|f| read_small(f).and_then(|b| decode(&b, px)))
}

/// Root's index: icon path → {size, mtime, file}.
fn read_index() -> Map<String, Value> {
    read_small(&index_path()).and_then(|b| serde_json::from_slice::<Value>(&b).ok()).and_then(|v| v.as_object().cloned()).unwrap_or_default()
}

/// The icon of `p` from root's copies: the first of its files that root copied.
fn copied_icon(p: &LaunchPoint, px: u32, index: &Map<String, Value>) -> Option<Image> {
    icon_candidates(p, px).into_iter().find_map(|f| {
        let file = index.get(f)?.get("file")?.as_str()?;
        // Only names root gives (no path in them).
        let file = file.strip_suffix(".png").filter(|h| h.len() == 16 && h.bytes().all(|b| b.is_ascii_hexdigit()))?;
        read_small(&format!("{}/{file}.png", copies())).and_then(|b| decode(&b, px))
    })
}

/// The installed apps with their icons for rows or cards drawing them `icon_px` pixels wide.
/// `publish` gets the list once, and again when root's copies have brought more icons.
pub fn load(icon_px: u32, mut publish: impl FnMut(Result<Vec<AppInfo>, String>, &Report)) {
    let mut report = Report::default();
    let (points, how) = match list() {
        Ok(l) => l,
        Err(e) => return publish(Err(e), &report),
    };
    report.list = how;
    let mut icons: Vec<(Option<Image>, &'static str)> = points.iter().map(|p| (icon_of(p, icon_px), "direct")).collect();
    let build = |icons: &[(Option<Image>, &'static str)], report: &mut Report| {
        report.icons.clear();
        points
            .iter()
            .zip(icons)
            .map(|(p, (icon, how))| {
                report.icons.push((p.id.clone(), if icon.is_some() { how } else { "generic" }, icon.as_ref().map(|i| (i.width, i.height))));
                AppInfo { id: p.id.clone(), title: p.title.clone(), params: p.params.clone(), color: p.color, icon: icon.clone() }
            })
            .collect::<Vec<_>>()
    };
    // Those the jailed app cannot read itself (some file to try, none readable).
    let wanted: Vec<usize> = (0..points.len()).filter(|&i| icons[i].0.is_none() && !icon_candidates(&points[i], icon_px).is_empty()).collect();
    if is_root() || wanted.is_empty() {
        let apps = build(&icons, &mut report);
        return publish(Ok(apps), &report);
    }
    // At once with the copies root made before, then once it has updated them.
    let index = read_index();
    for &i in &wanted {
        if let Some(img) = copied_icon(&points[i], icon_px, &index) {
            icons[i] = (Some(img), "copy");
        }
    }
    let apps = build(&icons, &mut report);
    publish(Ok(apps), &report);
    let exe = std::env::current_exe().map(|e| e.display().to_string()).unwrap_or_default();
    report.root_copy = Some(match as_root(&format!("'{exe}' --copy-app-icons {icon_px}")) {
        Ok(out) => out.trim().to_string(),
        Err(e) => format!("error: {e}"),
    });
    let index = read_index();
    for &i in &wanted {
        icons[i] = match copied_icon(&points[i], icon_px, &index) {
            Some(img) => (Some(img), "copy"),
            None => (None, "generic"),
        };
    }
    let apps = build(&icons, &mut report);
    publish(Ok(apps), &report);
}

/// Root's folder for the copies: a real directory, root's, 0755, so the jailed app (which
/// may write in the shared folder) cannot have root write through a link of its own.
fn copies_dir() -> Result<String, String> {
    let d = copies();
    match std::fs::symlink_metadata(&d) {
        Ok(m) if m.is_dir() && m.uid() == 0 => {}
        Ok(m) if m.is_dir() => std::fs::remove_dir_all(&d).map_err(|e| format!("{d}: {e}"))?,
        Ok(_) => std::fs::remove_file(&d).map_err(|e| format!("{d}: {e}"))?,
        Err(_) => {}
    }
    let _ = std::fs::create_dir(&d);
    std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o755)).map_err(|e| format!("{d}: {e}"))?;
    Ok(d)
}

/// `outpick --copy-app-icons <px>`, as root: brings the copies up to date with the apps
/// listed now. A file copied before is kept while its size and time are the same; one that
/// changed is copied again; those no app uses any more are deleted.
pub fn copy_cli(px: u32) -> i32 {
    if !is_root() {
        eprintln!("--copy-app-icons: root only");
        return 1;
    }
    let result = (|| -> Result<(usize, usize, usize), String> {
        let (points, _) = list()?;
        let dir = copies_dir()?;
        let old = read_index();
        let mut index = Map::new();
        let (mut copied, mut kept) = (0, 0);
        for p in &points {
            // As the app picks it: the first file that is a PNG.
            for f in icon_candidates(p, px) {
                if index.contains_key(f) {
                    break;
                }
                let Ok(meta) = std::fs::metadata(f) else { continue };
                let (size, mtime) = (meta.len(), meta.mtime());
                let name = copy_name(f);
                let same = old.get(f).is_some_and(|e| e.get("size") == Some(&json!(size)) && e.get("mtime") == Some(&json!(mtime)) && e.get("file") == Some(&json!(name)));
                if same && std::path::Path::new(&format!("{dir}/{name}")).exists() {
                    kept += 1;
                } else {
                    let Some(bytes) = read_small(f).filter(|b| b.starts_with(b"\x89PNG\r\n\x1a\n")) else { continue };
                    super::takeover::write_atomic(&format!("{dir}/{name}"), &bytes, 0o644)?;
                    copied += 1;
                }
                index.insert(f.to_string(), json!({ "size": size, "mtime": mtime, "file": name }));
                break;
            }
        }
        // What no listed app uses any more.
        let used: Vec<&str> = index.values().filter_map(|e| e.get("file")?.as_str()).collect();
        let mut dropped = 0;
        for e in std::fs::read_dir(&dir).map_err(|e| format!("{dir}: {e}"))?.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name != "index.json" && !used.contains(&name.as_str()) {
                let _ = std::fs::remove_file(e.path());
                dropped += 1;
            }
        }
        super::takeover::write_atomic(&index_path(), Value::Object(index).to_string().as_bytes(), 0o644)?;
        Ok((copied, kept, dropped))
    })();
    match result {
        Ok((copied, kept, dropped)) => {
            println!("{}", json!({ "copied": copied, "kept": kept, "dropped": dropped }));
            0
        }
        Err(e) => {
            println!("{}", json!({ "error": e }));
            1
        }
    }
}

/// `outpick --list-apps [px]`: what the list would show here, and which way each part came
/// (run as root, or as the jailed app to see what it can reach); the last state, after
/// root's copies if they were needed.
pub fn list_cli(px: u32) -> i32 {
    let mut last = (Err("not listed".to_string()), Value::Null);
    load(px, |apps, report| {
        let summary = report.json(&apps);
        last = (apps, summary);
    });
    println!("{}", last.1);
    if last.0.is_ok() { 0 } else { 1 }
}
