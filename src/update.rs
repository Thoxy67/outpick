//! Updates: the Homebrew Channel feed Outpick is published in, its changelog, and what is
//! remembered between openings (`update.json` next to the settings). Pure logic, tested on
//! any machine; the TV fetches with curl (`webos::update`), the desktop preview simulates.

use serde::{Deserialize, Serialize};
use crate::model::Install;
use serde_json::Value;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};

pub const APP_ID: &str = "com.thoxy.outpick";
/// Fixed addresses: nothing read from the network ever chooses where to read next.
pub const FEED_URL: &str = "https://git.thoxy.xyz/thoxy/outpick/raw/branch/hbc/repo.json";
pub const CHANGELOG_URL: &str = "https://git.thoxy.xyz/thoxy/outpick/raw/branch/main/CHANGELOG.md";
/// Where an offered package may come from: the feed's host, this repository's releases.
const IPK_PREFIX: &str = "https://git.thoxy.xyz/thoxy/outpick/releases/download/";
/// Bytes read at most: the feed (a 1.3 KiB file today), the changelog (newest first, so
/// the start is what matters), and the notes kept from it.
pub const FEED_MAX: usize = 64 << 10;
pub const CHANGELOG_MAX: usize = 128 << 10;
pub const NOTES_MAX: usize = 64 << 10;
/// Seconds between automatic checks.
pub const EVERY_SECS: u64 = 24 * 3600;
/// How far the clock may go back before the last check counts as unknown (a TV clock
/// corrected by a few minutes must not start a check, one set back by days must not stop them).
const CLOCK_SLACK_SECS: u64 = 3600;
pub const INSTALLED: &str = env!("CARGO_PKG_VERSION");

/// A version: x.y.z (1 to 4 numbers), an optional pre-release after '-', build after '+'
/// ignored, a leading 'v' allowed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    core: [u64; 4],
    pre: Vec<String>,
}

pub fn parse_version(s: &str) -> Option<Version> {
    let s = s.trim();
    let s = s.strip_prefix(['v', 'V']).unwrap_or(s);
    if s.is_empty() || s.len() > 32 || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b".-+".contains(&b)) {
        return None;
    }
    let s = s.split_once('+').map_or(s, |(v, _)| v);
    let (core, pre) = s.split_once('-').map_or((s, None), |(c, p)| (c, Some(p)));
    let nums: Vec<&str> = core.split('.').collect();
    if nums.len() > 4 {
        return None;
    }
    let mut out = [0u64; 4];
    for (i, n) in nums.iter().enumerate() {
        if n.is_empty() || n.len() > 9 || !n.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        out[i] = n.parse().ok()?;
    }
    let pre: Vec<String> = match pre {
        None => Vec::new(),
        Some(p) => {
            let ids: Vec<String> = p.split('.').map(str::to_string).collect();
            if ids.iter().any(String::is_empty) {
                return None;
            }
            ids
        }
    };
    Some(Version { core: out, pre })
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        // As semver: the numbers, then a pre-release below its release, then the
        // pre-release's identifiers (numbers below words, numbers by value).
        self.core.cmp(&other.core).then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => {
                for (a, b) in self.pre.iter().zip(&other.pre) {
                    let o = match (a.parse::<u64>(), b.parse::<u64>()) {
                        (Ok(x), Ok(y)) => x.cmp(&y),
                        (Ok(_), Err(_)) => Ordering::Less,
                        (Err(_), Ok(_)) => Ordering::Greater,
                        (Err(_), Err(_)) => a.cmp(b),
                    };
                    if o != Ordering::Equal {
                        return o;
                    }
                }
                self.pre.len().cmp(&other.pre.len())
            }
        })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Whether `offered` is worth installing over `installed`: newer, and not a pre-release
/// unless a pre-release is what runs (a stable build is offered stable builds only).
pub fn worth(offered: &Version, installed: &Version) -> bool {
    offered > installed && (offered.pre.is_empty() || !installed.pre.is_empty())
}

/// What the feed offers: a version newer than this one and where the Homebrew Channel
/// downloads it from, with the hash it checks it against.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Offer {
    pub version: String,
    pub ipk_url: String,
    pub sha256: String,
    #[serde(default)]
    pub size: Option<u64>,
}

impl Offer {
    /// The offer if every field is what the installer may be handed (the hash lowercased,
    /// as the Homebrew Channel compares it with its own hex digest), else why not.
    pub fn checked(mut self) -> Result<Offer, String> {
        if parse_version(&self.version).is_none() {
            return Err(format!("unreadable version {:?}", short(&self.version)));
        }
        if self.sha256.len() != 64 || !self.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("the package's sha256 is not 64 hex digits".into());
        }
        self.sha256.make_ascii_lowercase();
        if !valid_ipk_url(&self.ipk_url) {
            return Err("the package is not on git.thoxy.xyz's Outpick releases".into());
        }
        if self.size.is_some_and(|s| s == 0 || s > 64 << 20) {
            self.size = None;
        }
        Ok(self)
    }
}

/// The package's address, strictly: https on the feed's host, this repository's release
/// assets (`…/releases/download/<tag>/<file>.ipk`), and only characters that need no
/// quoting anywhere (letters, digits, '.', '_', '-' and the slashes between the parts).
pub fn valid_ipk_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix(IPK_PREFIX) else { return false };
    if url.len() > 256 {
        return false;
    }
    let parts: Vec<&str> = rest.split('/').collect();
    let part_ok = |p: &str| {
        !p.is_empty() && p.len() <= 96 && !p.starts_with('.') && !p.contains("..") && p.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    };
    parts.len() == 2 && parts.iter().all(|p| part_ok(p)) && parts[1].ends_with(".ipk") && parts[1].len() > 4
}

fn short(s: &str) -> String {
    s.chars().take(40).collect()
}

/// The feed (`repo.json`): None when this version is current, the offer when Outpick is
/// listed in a newer version, an error when the feed is not what it should be. Among
/// several packages only Outpick's count; of several Outpicks, the newest.
pub fn parse_feed(v: &Value, installed: &str) -> Result<Option<Offer>, String> {
    let installed = parse_version(installed).ok_or("this build's version is unreadable")?;
    let packages = v.get("packages").and_then(Value::as_array).ok_or("not a Homebrew Channel feed")?;
    let ours: Vec<&Value> = packages
        .iter()
        .filter(|p| p.get("id").and_then(Value::as_str) == Some(APP_ID))
        .filter_map(|p| p.get("manifest"))
        .filter(|m| m.get("id").and_then(Value::as_str).is_none_or(|id| id == APP_ID))
        .collect();
    if ours.is_empty() {
        return Err("Outpick is not in the feed".into());
    }
    let newest = ours
        .iter()
        .filter_map(|m| Some((parse_version(m.get("version")?.as_str()?)?, *m)))
        .max_by(|a, b| a.0.cmp(&b.0))
        .ok_or("the feed's Outpick has no readable version")?;
    if !worth(&newest.0, &installed) {
        return Ok(None);
    }
    let m = newest.1;
    let text = |k: &str| m.get(k).and_then(Value::as_str).map(str::to_string);
    let offer = Offer {
        version: text("version").unwrap_or_default(),
        ipk_url: text("ipkUrl").ok_or("the package has no ipkUrl")?,
        sha256: m.pointer("/ipkHash/sha256").and_then(Value::as_str).ok_or("the package has no sha256")?.to_string(),
        size: m.get("ipkSize").and_then(Value::as_u64),
    };
    offer.checked().map(Some)
}

/// What is kept between openings, in `update.json` next to the settings.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Remembered {
    /// When the last check ended (Unix seconds), whatever it found.
    pub checked_at: Option<u64>,
    /// Why it failed, if it did.
    pub error: Option<String>,
    /// What it found, until it is installed.
    pub offer: Option<Offer>,
    /// The version that last started: another one means an update just ran.
    pub ran: Option<String>,
}

impl Remembered {
    pub fn path() -> PathBuf {
        crate::config::Config::path().with_file_name("update.json")
    }

    /// Read leniently (the folder is shared, so the file is not trusted): an offer that is
    /// not valid, or not newer than `installed` (it has been installed), is dropped.
    pub fn load_from(path: &Path, installed: &str) -> Remembered {
        let bytes = std::fs::metadata(path)
            .ok()
            .filter(|m| m.is_file() && m.len() <= 16 << 10)
            .and_then(|_| std::fs::read(path).ok());
        let mut r: Remembered = bytes.and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let current = parse_version(installed);
        r.offer = r.offer.and_then(|o| o.checked().ok()).filter(|o| {
            let v = parse_version(&o.version);
            matches!((&v, &current), (Some(v), Some(c)) if worth(v, c))
        });
        r.error = r.error.map(|e| short_error(&e));
        r
    }

    /// Written beside and renamed into place, readable and replaceable by the jailed app.
    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        let tmp = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&tmp);
        std::fs::write(&tmp, bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o666));
        }
        std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// An automatic check is due: none yet, the last one a day old, or the clock went back
    /// well before it (it could stay before it for months).
    pub fn due(&self, now: u64) -> bool {
        match self.checked_at {
            None => true,
            Some(at) => now.saturating_add(CLOCK_SLACK_SECS) < at || now.saturating_sub(at) >= EVERY_SECS,
        }
    }

    /// A check that ended now with `result`.
    pub fn record(&mut self, now: u64, result: &Result<Option<Offer>, String>) {
        self.checked_at = Some(now);
        match result {
            Ok(offer) => {
                self.offer = offer.clone();
                self.error = None;
            }
            // A failed check keeps what an earlier one found.
            Err(e) => self.error = Some(short_error(e)),
        }
    }
}

/// An error as kept and shown: one line, 160 characters at most.
pub fn short_error(e: &str) -> String {
    let line = e.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() > 160 { line.chars().take(159).collect::<String>() + "…" } else { line }
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// curl's arguments for `url`: https only, redirects too, verified (curl's default, never
/// turned off), at most `max` bytes (`partial`: the first `max` bytes are enough).
pub fn curl_args(url: &str, max: usize, partial: bool) -> Vec<String> {
    let mut args: Vec<String> = ["-sS", "--fail", "-L", "--max-redirs", "3", "--proto", "=https", "--proto-redir", "=https", "--connect-timeout", "8", "--max-time", "20"]
        .map(String::from)
        .to_vec();
    if partial {
        // Newest first: the start of the file holds the versions after this one.
        args.extend(["--range".into(), format!("0-{}", max - 1), "--max-filesize".into(), (1usize << 20).to_string()]);
    } else {
        args.extend(["--max-filesize".into(), max.to_string()]);
    }
    args.push(url.into());
    args
}

/// What a message of the Homebrew Channel's install says: progress, done, or why not.
pub fn install_step(v: &Value) -> Install {
    if v.get("returnValue").and_then(Value::as_bool) == Some(false) {
        let e = v.get("errorText").and_then(Value::as_str).unwrap_or("refused");
        return Install::Failed(short_error(e));
    }
    if v.get("finished").and_then(Value::as_bool) == Some(true) {
        return Install::Done;
    }
    let status = v.get("statusText").and_then(Value::as_str).unwrap_or("").to_string();
    let progress = v.get("progress").and_then(Value::as_f64).map(|p| (p / 100.0).clamp(0.0, 1.0) as f32);
    Install::Running { status, progress }
}

/// One version's notes, as `tools/release/changelog.py` writes them.
#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    pub date: String,
    pub entries: Vec<Entry>,
}

/// A commit: its subject, and its paragraphs as bullets.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// Empty for text that came before any subject.
    pub title: String,
    pub bullets: Vec<Bullet>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Bullet {
    /// "Receiver" in "**Receiver:** …".
    pub area: Option<String>,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Notes {
    /// Newest first.
    pub releases: Vec<Release>,
    /// More was there than `NOTES_MAX` keeps.
    pub truncated: bool,
}

/// The sections of `md` (CHANGELOG.md) for the versions after `after` up to `upto`, newest
/// first, as plain text: headings without their commit link, inline code and links as
/// their text, at most `NOTES_MAX` bytes of it.
pub fn parse_changelog(md: &str, after: &str, upto: &str) -> Notes {
    let (Some(after), Some(upto)) = (parse_version(after), parse_version(upto)) else { return Notes::default() };
    let mut notes = Notes::default();
    let mut kept = 0usize;
    // The section being read, if it is one of those wanted.
    let mut current: Option<(Version, Release)> = None;
    let mut in_section = false;
    let finish = |current: &mut Option<(Version, Release)>, notes: &mut Notes| {
        if let Some(r) = current.take() {
            notes.releases.push(r.1);
        }
    };
    let mut last_was_bullet = false;
    for raw in md.lines() {
        let line = raw.trim_end();
        if let Some(head) = line.strip_prefix("## ") {
            finish(&mut current, &mut notes);
            in_section = true;
            last_was_bullet = false;
            let (name, date) = split_dash(head);
            current = parse_version(name).filter(|v| *v > after && *v <= upto).map(|v| {
                (v, Release { version: name.trim_start_matches(['v', 'V']).to_string(), date: inline(date), entries: Vec::new() })
            });
            continue;
        }
        if !in_section || notes.truncated {
            continue;
        }
        let Some((_, release)) = current.as_mut() else { continue };
        // Counts what is kept; false (and nothing more is read) once over the limit.
        let mut add = |text: &str| -> bool {
            kept += text.len();
            if kept > NOTES_MAX {
                notes.truncated = true;
            }
            kept <= NOTES_MAX
        };
        if let Some(subject) = line.strip_prefix("### ") {
            // The commit link after " · " is for the web page.
            let subject = subject.rfind(" · [").map_or(subject, |i| &subject[..i]);
            let title = inline(subject);
            if add(&title) {
                release.entries.push(Entry { title, bullets: Vec::new() });
            }
            last_was_bullet = false;
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            last_was_bullet = false;
            continue;
        }
        // The release's footer lines are links for the web page.
        if trimmed.starts_with("**Full changelog:**") || trimmed.starts_with("**Contributors:**") {
            continue;
        }
        let item = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* "));
        let bullet = match item {
            Some(item) => {
                let (area, text) = match item.strip_prefix("**").and_then(|r| r.split_once(":**")) {
                    Some((area, text)) if !area.is_empty() && area.len() <= 40 => (Some(inline(area)), inline(text.trim())),
                    _ => (None, inline(item)),
                };
                Bullet { area, text }
            }
            // A bullet's wrapped line goes on with it.
            None if last_was_bullet && raw.starts_with(char::is_whitespace) => {
                let more = inline(trimmed);
                if add(&more)
                    && let Some(b) = release.entries.last_mut().and_then(|e| e.bullets.last_mut())
                {
                    b.text.push(' ');
                    b.text.push_str(&more);
                }
                continue;
            }
            None => Bullet { area: None, text: inline(trimmed) },
        };
        if !add(&bullet.text) {
            continue;
        }
        if release.entries.is_empty() {
            release.entries.push(Entry { title: String::new(), bullets: Vec::new() });
        }
        if let Some(e) = release.entries.last_mut() {
            e.bullets.push(bullet);
        }
        last_was_bullet = item.is_some();
    }
    finish(&mut current, &mut notes);
    notes.releases.sort_by_key(|r| std::cmp::Reverse(parse_version(&r.version)));
    notes
}

/// Whether `md` has a section for `after` or an older version: a changelog cut short (newest
/// first) still holds every version after it.
pub fn reaches(md: &str, after: &str) -> bool {
    let Some(after) = parse_version(after) else { return false };
    md.lines().filter_map(|l| l.strip_prefix("## ")).filter_map(|h| parse_version(split_dash(h).0)).any(|v| v <= after)
}

/// "v0.2.0 — 2026-11-13" → ("v0.2.0", "2026-11-13").
fn split_dash(head: &str) -> (&str, &str) {
    for sep in [" — ", " – ", " - "] {
        if let Some((a, b)) = head.split_once(sep) {
            return (a.trim(), b.trim());
        }
    }
    (head.trim(), "")
}

/// Markdown inline syntax reduced to its text: [label](url) → label, `code` → code, bold
/// markers dropped.
pub fn inline(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(c) = rest.chars().next() {
        if c == '['
            && let Some(close) = rest.find("](")
            && let Some(end) = rest[close + 2..].find(')')
            && !rest[1..close].contains('[')
        {
            out.push_str(&inline(&rest[1..close]));
            rest = &rest[close + 2 + end + 1..];
            continue;
        }
        if c == '`'
            && let Some(end) = rest[1..].find('`')
        {
            out.push_str(&rest[1..1 + end]);
            rest = &rest[end + 2..];
            continue;
        }
        if let Some(r) = rest.strip_prefix("**") {
            rest = r;
            continue;
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// What stands in for a character the font lacks (only used when the font has all of it).
fn substitute(c: char) -> Option<&'static str> {
    Some(match c {
        '→' | '\u{27f6}' => "->",
        '←' | '\u{27f5}' => "<-",
        '\u{2194}' => "<->",
        '\u{21d2}' => "=>",
        '\u{21d0}' => "<=",
        '\u{2265}' => ">=",
        '\u{2264}' => "<=",
        '\u{2260}' => "!=",
        '\u{2248}' | '\u{223c}' => "~",
        '…' => "...",
        '—' | '–' | '‒' | '−' => "-",
        '‘' | '’' | '′' => "'",
        '“' | '”' | '″' => "\"",
        '•' | '·' => "-",
        '×' => "x",
        '\u{a0}' | '\u{2009}' | '\u{202f}' => " ",
        _ => return None,
    })
}

/// `text` as the font can draw it (`has`): what it lacks is replaced when an ASCII stand-in
/// exists (→ becomes ->), else left out, never drawn as an empty box; the spaces that
/// leaves are closed up.
pub fn printable(text: &str, mut has: impl FnMut(char) -> bool) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_control() || c == '\u{fe0f}' || c == '\u{200d}' {
            if c == '\t' {
                out.push(' ');
            }
            continue;
        }
        if c == ' ' || has(c) {
            out.push(c);
        } else if let Some(sub) = substitute(c)
            && sub.chars().all(|s| s == ' ' || has(s))
        {
            out.push_str(sub);
        }
    }
    out.split(' ').filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn v(s: &str) -> Version {
        parse_version(s).unwrap_or_else(|| panic!("{s}"))
    }

    #[test]
    fn versions_compare_as_numbers() {
        assert!(v("0.2.1") > v("0.2.0"));
        assert!(v("0.10.0") > v("0.9.9"));
        assert!(v("1.0") == v("1.0.0"));
        assert!(v("v0.2.0") == v("0.2.0"));
        assert!(v("0.2.0+build.7") == v("0.2.0"));
        assert!(v("1.2.3.4") > v("1.2.3"));
        // Pre-releases: below their release, numbers by value, numbers below words.
        assert!(v("0.2.1-rc.1") < v("0.2.1"));
        assert!(v("0.2.1-rc.1") > v("0.2.0"));
        assert!(v("0.2.1-rc.2") < v("0.2.1-rc.10"));
        assert!(v("0.2.1-1") < v("0.2.1-alpha"));
        assert!(v("0.2.1-alpha") < v("0.2.1-alpha.1"));
        for bad in ["", "v", "0..1", "1.2.3.4.5", "a.b.c", "1.2.3-", "1.2.3-rc..1", "1 .2", "0.2.0'", "1234567890.0.0", "0.2.0;rm"] {
            assert_eq!(parse_version(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn pre_releases_only_for_pre_releases() {
        assert!(worth(&v("0.2.1"), &v("0.2.0")));
        assert!(!worth(&v("0.2.0"), &v("0.2.0")));
        assert!(!worth(&v("0.1.9"), &v("0.2.0")));
        assert!(!worth(&v("0.2.1-rc.1"), &v("0.2.0")));
        assert!(worth(&v("0.2.1-rc.2"), &v("0.2.1-rc.1")));
        assert!(worth(&v("0.2.1"), &v("0.2.1-rc.1")));
    }

    fn fixture() -> Value {
        let path = format!("{}/tests/fixtures/repo.json", env!("CARGO_MANIFEST_DIR"));
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn the_published_feed() {
        let feed = fixture();
        // 0.2.0 is published: nothing for 0.2.0, an update for 0.1.0.
        assert_eq!(parse_feed(&feed, "0.2.0"), Ok(None));
        let offer = parse_feed(&feed, "0.1.0").unwrap().unwrap();
        assert_eq!(offer.version, "0.2.0");
        assert_eq!(offer.ipk_url, "https://git.thoxy.xyz/thoxy/outpick/releases/download/v0.2.0/com.thoxy.outpick_0.2.0_arm.ipk");
        assert_eq!(offer.sha256, "05769855e51d6daac429dd7b7ba5d373e70fba544e6360971dd46362b2cee60e");
        assert_eq!(offer.size, Some(1810800));
    }

    /// A feed of `packages`, each (id, manifest).
    fn feed(packages: &[(&str, Value)]) -> Value {
        json!({ "packages": packages.iter().map(|(id, m)| json!({ "id": id, "manifest": m })).collect::<Vec<_>>() })
    }

    fn manifest(version: &str) -> Value {
        json!({
            "id": APP_ID,
            "version": version,
            "ipkUrl": format!("https://git.thoxy.xyz/thoxy/outpick/releases/download/v{version}/com.thoxy.outpick_{version}_arm.ipk"),
            "ipkHash": { "sha256": "AB".repeat(32) },
            "ipkSize": 1_900_000,
        })
    }

    #[test]
    fn feeds_that_offer_or_not() {
        // Several packages: only ours counts, the newest of ours.
        let f = feed(&[("org.other.app", manifest("9.9.9")), (APP_ID, manifest("0.2.1")), (APP_ID, manifest("0.3.0")), (APP_ID, manifest("0.2.5"))]);
        let o = parse_feed(&f, "0.2.0").unwrap().unwrap();
        assert_eq!(o.version, "0.3.0");
        // The hash is lowercased for the Homebrew Channel's comparison.
        assert_eq!(o.sha256, "ab".repeat(32));
        assert_eq!(o.size, Some(1_900_000));
        // Not ours at all, or a manifest of another id under ours.
        assert!(parse_feed(&feed(&[("org.other.app", manifest("9.9.9"))]), "0.2.0").is_err());
        let mut other = manifest("0.3.0");
        other["id"] = json!("org.evil");
        assert!(parse_feed(&feed(&[(APP_ID, other)]), "0.2.0").is_err());
        // Not a feed.
        assert!(parse_feed(&json!({"apps": []}), "0.2.0").is_err());
        assert!(parse_feed(&json!([1, 2]), "0.2.0").is_err());
        // Older or same: nothing, whatever the rest says.
        assert_eq!(parse_feed(&feed(&[(APP_ID, manifest("0.1.0"))]), "0.2.0"), Ok(None));
        // A stable build is not offered a pre-release.
        assert_eq!(parse_feed(&feed(&[(APP_ID, manifest("0.2.1-rc.1"))]), "0.2.0"), Ok(None));
    }

    #[test]
    fn feeds_with_a_bad_package() {
        let with = |f: &dyn Fn(&mut Value)| {
            let mut m = manifest("0.2.1");
            f(&mut m);
            parse_feed(&feed(&[(APP_ID, m)]), "0.2.0")
        };
        // Missing fields.
        assert!(with(&|m| drop(m.as_object_mut().unwrap().remove("ipkUrl"))).is_err());
        assert!(with(&|m| drop(m.as_object_mut().unwrap().remove("ipkHash"))).is_err());
        assert!(with(&|m| m["ipkHash"] = json!({"md5": "x"})).is_err());
        assert!(with(&|m| drop(m.as_object_mut().unwrap().remove("version"))).is_err());
        // No size is fine (it is only shown).
        assert!(with(&|m| drop(m.as_object_mut().unwrap().remove("ipkSize"))).unwrap().unwrap().size.is_none());
        // Bad hashes.
        assert!(with(&|m| m["ipkHash"]["sha256"] = json!("ab".repeat(31) + "a")).is_err());
        assert!(with(&|m| m["ipkHash"]["sha256"] = json!("zz".repeat(32))).is_err());
        assert!(with(&|m| m["ipkHash"]["sha256"] = json!("ab".repeat(33))).is_err());
        assert!(with(&|m| m["ipkHash"]["sha256"] = json!(42)).is_err());
        // Package addresses other than this repository's release assets over https.
        for url in [
            "http://git.thoxy.xyz/thoxy/outpick/releases/download/v0.2.1/com.thoxy.outpick_0.2.1_arm.ipk",
            "https://evil.example/thoxy/outpick/releases/download/v0.2.1/com.thoxy.outpick_0.2.1_arm.ipk",
            "https://git.thoxy.xyz.evil.example/thoxy/outpick/releases/download/v0.2.1/a.ipk",
            "https://git.thoxy.xyz@evil.example/thoxy/outpick/releases/download/v0.2.1/a.ipk",
            "https://git.thoxy.xyz:8443/thoxy/outpick/releases/download/v0.2.1/a.ipk",
            "https://git.thoxy.xyz/other/outpick/releases/download/v0.2.1/a.ipk",
            "https://git.thoxy.xyz/thoxy/outpick/releases/download/v0.2.1/../../x/a.ipk",
            "https://git.thoxy.xyz/thoxy/outpick/releases/download/v0.2.1/a.ipk?x=1",
            "https://git.thoxy.xyz/thoxy/outpick/releases/download/v0.2.1/a%20b.ipk",
            "https://git.thoxy.xyz/thoxy/outpick/releases/download/v0.2.1/a'b.ipk",
            "https://git.thoxy.xyz/thoxy/outpick/releases/download/v0.2.1/a\"b.ipk",
            "https://git.thoxy.xyz/thoxy/outpick/releases/download/v0.2.1/a b.ipk",
            "https://git.thoxy.xyz/thoxy/outpick/releases/download/v0.2.1/a.sh",
            "https://git.thoxy.xyz/thoxy/outpick/releases/download/v0.2.1/.ipk",
            "https://git.thoxy.xyz/thoxy/outpick/releases/download//a.ipk",
            "https://git.thoxy.xyz/thoxy/outpick/releases/download/a.ipk",
            "https://git.thoxy.xyz/thoxy/outpick/releases/download/v0.2.1/x/a.ipk",
            "https://git.thoxy.xyz/thoxy/outpick/releases/download/v0.2.1/a.ipk\n",
            "HTTPS://git.thoxy.xyz/thoxy/outpick/releases/download/v0.2.1/a.ipk",
        ] {
            assert!(with(&|m| m["ipkUrl"] = json!(url)).is_err(), "{url:?}");
            assert!(!valid_ipk_url(url), "{url:?}");
        }
        assert!(with(&|m| m["ipkUrl"] = json!(42)).is_err());
        assert!(valid_ipk_url("https://git.thoxy.xyz/thoxy/outpick/releases/download/v0.2.1/com.thoxy.outpick_0.2.1_arm.ipk"));
        assert!(!valid_ipk_url(&format!("https://git.thoxy.xyz/thoxy/outpick/releases/download/v0.2.1/{}.ipk", "a".repeat(200))));
    }

    #[test]
    fn remembered_checks_and_offers() {
        let day = EVERY_SECS;
        let mut r = Remembered::default();
        assert!(r.due(1_800_000_000));
        let offer = parse_feed(&feed(&[(APP_ID, manifest("0.2.1"))]), "0.2.0");
        r.record(1_800_000_000, &offer);
        assert!(r.offer.is_some() && r.error.is_none());
        assert!(!r.due(1_800_000_000 + day - 1));
        assert!(r.due(1_800_000_000 + day));
        // A clock set back a little: not due; back by days: due, not stuck until it catches up.
        assert!(!r.due(1_800_000_000 - 600));
        assert!(r.due(1_800_000_000 - 2 * day));
        assert!(r.due(0));
        // A failed check keeps the offer, says why, and waits a day too.
        r.record(1_800_100_000, &Err("curl: (6) Could not resolve host:\n git.thoxy.xyz".into()));
        assert!(r.offer.is_some());
        assert_eq!(r.error.as_deref(), Some("curl: (6) Could not resolve host: git.thoxy.xyz"));
        assert!(!r.due(1_800_100_001));
        // Nothing newer: the offer goes.
        r.record(1_800_200_000, &Ok(None));
        assert!(r.offer.is_none() && r.error.is_none());
    }

    #[test]
    fn remembered_file_round_trip_and_distrust() {
        let dir = std::env::temp_dir().join(format!("outpick-update-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("update.json");
        let mut r = Remembered { ran: Some("0.2.0".into()), ..Default::default() };
        r.record(1_800_000_000, &parse_feed(&feed(&[(APP_ID, manifest("0.2.1"))]), "0.2.0"));
        r.save_to(&path).unwrap();
        assert_eq!(Remembered::load_from(&path, "0.2.0"), r);
        // Once 0.2.1 runs, the offer is gone.
        assert_eq!(Remembered::load_from(&path, "0.2.1").offer, None);
        // An offer edited in the (shared) file is dropped if it is not a valid one.
        let mut bad = r.clone();
        bad.offer.as_mut().unwrap().ipk_url = "https://evil.example/x.ipk".into();
        bad.save_to(&path).unwrap();
        let loaded = Remembered::load_from(&path, "0.2.0");
        assert_eq!(loaded.offer, None);
        assert_eq!(loaded.checked_at, Some(1_800_000_000));
        // Not JSON, or missing: nothing remembered.
        std::fs::write(&path, b"{not json").unwrap();
        assert_eq!(Remembered::load_from(&path, "0.2.0"), Remembered::default());
        assert_eq!(Remembered::load_from(&dir.join("none.json"), "0.2.0"), Remembered::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_local_changelog() {
        let md = std::fs::read_to_string(format!("{}/CHANGELOG.md", env!("CARGO_MANIFEST_DIR"))).unwrap();
        let notes = parse_changelog(&md, "0.1.0", "0.2.0");
        assert!(!notes.truncated);
        assert_eq!(notes.releases.len(), 1);
        let r = &notes.releases[0];
        assert_eq!((r.version.as_str(), r.date.as_str()), ("0.2.0", "2026-11-13"));
        assert_eq!(r.entries[0].title, "README: network receivers, long OK for the options, the Network receiver section");
        assert!(r.entries[0].bullets.is_empty());
        let denon = &r.entries[1];
        assert_eq!(denon.title, "Denon / Marantz receivers over the network; options on a long OK in every layout");
        assert_eq!(denon.bullets[0].area.as_deref(), Some("Receiver"));
        assert!(denon.bullets[0].text.starts_with("the Denon or Marantz behind an HDMI input"));
        // No markdown, links or commit hashes left anywhere.
        for e in &r.entries {
            assert!(!e.title.contains('`') && !e.title.contains("](") && !e.title.contains(" · "), "{}", e.title);
            for b in &e.bullets {
                assert!(!b.text.contains("**") && !b.text.contains("]("), "{}", b.text);
            }
        }
        // Every version up to 0.2.0, newest first; none after it.
        let all = parse_changelog(&md, "0.0.0", "0.2.0");
        let versions: Vec<&str> = all.releases.iter().map(|r| r.version.as_str()).collect();
        assert_eq!(versions, ["0.2.0", "0.1.0"]);
        assert!(parse_changelog(&md, "0.2.0", "0.2.0").releases.is_empty());
        assert!(parse_changelog(&md, "0.2.0", "0.3.0").releases.is_empty());
    }

    const SYNTHETIC: &str = "## v0.3.0 — 2026-12-24

### Update screen → shows `notes` · [`abc1234`](https://git.thoxy.xyz/thoxy/outpick/commit/abc)

- **Update:** see [the feed](https://example.org/feed) and `repo.json`
- plain bullet
  wrapped onto a second line

**Full changelog:** [v0.2.1...v0.3.0](https://example.org)

## v0.2.1 — 2026-12-01

Maintenance release (no user-facing commits).

## v0.2.1-rc.1 — 2026-11-30

### A pre-release

## v0.2.0 — 2026-11-13

### Old · [`x`](y)

## What's changed

- not a version: skipped
";

    #[test]
    fn a_synthetic_changelog() {
        let n = parse_changelog(SYNTHETIC, "0.2.0", "0.3.0");
        let versions: Vec<&str> = n.releases.iter().map(|r| r.version.as_str()).collect();
        assert_eq!(versions, ["0.3.0", "0.2.1", "0.2.1-rc.1"]);
        let top = &n.releases[0];
        assert_eq!(top.date, "2026-12-24");
        assert_eq!(top.entries.len(), 1);
        assert_eq!(top.entries[0].title, "Update screen → shows notes");
        assert_eq!(
            top.entries[0].bullets,
            [
                Bullet { area: Some("Update".into()), text: "see the feed and repo.json".into() },
                Bullet { area: None, text: "plain bullet wrapped onto a second line".into() },
            ]
        );
        // A paragraph without a subject.
        assert_eq!(n.releases[1].entries, [Entry { title: String::new(), bullets: vec![Bullet { area: None, text: "Maintenance release (no user-facing commits).".into() }] }]);
        // Out of order in the file: still newest first.
        let shuffled = "## v0.2.1 — a\n### one\n## v0.3.0 — b\n### two\n";
        let n = parse_changelog(shuffled, "0.2.0", "0.3.0");
        assert_eq!(n.releases.iter().map(|r| r.version.as_str()).collect::<Vec<_>>(), ["0.3.0", "0.2.1"]);
        // Nothing usable.
        assert_eq!(parse_changelog("", "0.2.0", "0.3.0"), Notes::default());
        assert_eq!(parse_changelog("<html>404</html>", "0.2.0", "0.3.0"), Notes::default());
        assert_eq!(parse_changelog(SYNTHETIC, "bad", "0.3.0"), Notes::default());
        // Whether a cut file still went down to the installed version.
        assert!(reaches(SYNTHETIC, "0.2.0"));
        assert!(!reaches(SYNTHETIC, "0.1.0"));
        assert!(!reaches("## v0.3.0 — x\n", "0.2.0"));
    }

    #[test]
    fn a_huge_changelog_is_cut() {
        let mut md = String::from("## v0.3.0 — 2026-12-24\n\n### Big\n\n");
        for i in 0..5000 {
            md.push_str(&format!("- **Area {}:** {}\n", i % 7, "words ".repeat(10)));
        }
        let n = parse_changelog(&md, "0.2.0", "0.3.0");
        assert!(n.truncated);
        let kept: usize = n.releases[0].entries[0].bullets.iter().map(|b| b.text.len()).sum();
        assert!(kept <= NOTES_MAX && kept > NOTES_MAX / 2, "{kept}");
    }

    #[test]
    fn install_messages() {
        // As the Homebrew Channel's service.js answers (`install`, wrapped by its `ta`).
        assert_eq!(install_step(&json!({"statusText": "Downloading…"})), Install::Running { status: "Downloading…".into(), progress: None });
        assert_eq!(install_step(&json!({"statusText": "Downloading…", "progress": 42})), Install::Running { status: "Downloading…".into(), progress: Some(0.42) });
        assert_eq!(install_step(&json!({"statusText": "Verifying…"})), Install::Running { status: "Verifying…".into(), progress: None });
        assert_eq!(install_step(&json!({"returnValue": true, "statusText": "Finished.", "finished": true})), Install::Done);
        assert_eq!(install_step(&json!({"returnValue": false, "errorText": "Invalid file checksum"})), Install::Failed("Invalid file checksum".into()));
        assert_eq!(install_step(&json!({"returnValue": false})), Install::Failed("refused".into()));
    }

    #[test]
    fn curl_is_bounded_and_verified() {
        let a = curl_args(FEED_URL, FEED_MAX, false);
        for want in ["--fail", "=https", "--proto-redir", "--max-time", "--max-filesize"] {
            assert!(a.iter().any(|x| x == want), "{want}");
        }
        assert!(!a.iter().any(|x| x == "-k" || x == "--insecure" || x.contains('\'')));
        assert_eq!(a.last().map(String::as_str), Some(FEED_URL));
        assert!(curl_args(CHANGELOG_URL, CHANGELOG_MAX, true).iter().any(|x| x == "0-131071"));
    }

    #[test]
    fn inline_markdown() {
        assert_eq!(inline("see [the docs](https://x.y/z) and `a_b`"), "see the docs and a_b");
        assert_eq!(inline("**bold** text"), "bold text");
        assert_eq!(inline("[`hash`](url)"), "hash");
        assert_eq!(inline("unclosed [bracket and `tick"), "unclosed [bracket and `tick");
        assert_eq!(inline("a [b] c"), "a [b] c");
    }

    #[test]
    fn printable_text() {
        let latin = |c: char| (c as u32) < 0x180;
        assert_eq!(printable("A → B", latin), "A -> B");
        assert_eq!(printable("\u{2265} 2 · done…", latin), ">= 2 · done...");
        // No stand-in: left out, and the space closed up.
        assert_eq!(printable("Fixed \u{1f41b} bug \u{2714}", latin), "Fixed bug");
        assert_eq!(printable("tab\there\u{7}", latin), "tab here");
        assert_eq!(printable("\u{65e5}\u{672c}", latin), "");
        // A stand-in the font cannot draw either is not used.
        assert_eq!(printable("a→b", |c: char| c.is_ascii_alphabetic()), "ab");
        // The font has it: kept.
        assert_eq!(printable("A → B", |_| true), "A → B");
    }
}
