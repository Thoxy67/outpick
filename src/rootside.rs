//! The pure part of what the app does as root on the TV (`webos::takeover`,
//! `webos::capture`): the compositor key filter's code, the filter chain and
//! capture whitelist edits, and the boot hook. Kept free of any TV call so the
//! desktop build tests it.

use serde_json::{Value, json};

/// The compositor's JavaScript key filter chain (configd).
pub const CHAIN: &str = "com.webos.surfacemanager.keyFilters";
/// webOS 11's native key filters: no JavaScript chain there.
pub const CHAIN_CPP: &str = "com.webos.surfacemanager.keyFiltersCpp";
/// Glasshouse's key mappings (another filter that may take the Input key).
pub const GLASSHOUSE_KEYS: &str = "glasshouse.remoteKeys";
/// Which keys our filter takes, `{"source": bool, "multikey": bool}`; the
/// filter subscribes to it, so a change applies with nothing reloaded.
pub const KEYS: &str = "com.thoxy.outpick.keys";
/// Our filter's handler: how our chain entry is recognised, wherever its file.
pub const HANDLER: &str = "handleOutpick";

/// Root's own folder: the filter (code the compositor runs) and the takeover
/// marker. 0755 root, so no jailed app can change them.
pub const ROOT_DIR: &str = "/var/lib/outpick";
pub const FILTER: &str = "/var/lib/outpick/keyfilter.js";
/// Present while the takeover is wanted: the boot hook puts it back if configd lost it.
pub const WANTED: &str = "/var/lib/outpick/wanted";
/// The folder shared with the jailed app (settings, previews). Earlier builds
/// kept the filter here too.
pub const SHARED_DIR: &str = "/media/developer/outpick";
pub const OLD_FILTER: &str = "/media/developer/lg-c1-source/keyfilter.js";
/// Before the Outpick rename: the app id and the filter's handler (settings: config.rs).
pub const LEGACY_APP_ID: &str = "com.thoxy.sourcepicker";
pub const LEGACY_HANDLER: &str = "handleLgSourcePicker";
/// The Homebrew Channel runs these at boot (BusyBox run-parts: no dot in the name).
pub const HOOK_DIR: &str = "/var/lib/webosbrew/init.d";
pub const HOOK: &str = "/var/lib/webosbrew/init.d/70-outpick";

pub const WHITELIST_SYSTEM: &str = "/etc/palm/whitelist-conf.json";
pub const WHITELIST_USER: &str = "/home/root/whitelist-conf.json";
/// Left by earlier builds, which kept the user file aside here.
pub const WHITELIST_BACKUP: &str = "/home/root/whitelist-conf.json.outpick.bak";

/// Where the Homebrew Channel installs the app, by app id.
pub fn app_binaries(app_id: &str) -> [String; 2] {
    [
        format!("/media/developer/apps/usr/palm/applications/{app_id}/outpick"),
        format!("/media/cryptofs/apps/usr/palm/applications/{app_id}/outpick"),
    ]
}

// ---- the filter chain ----

pub fn is_ours(entry: &Value) -> bool {
    matches!(entry.get("handler").and_then(Value::as_str), Some(h) if h == HANDLER || h == LEGACY_HANDLER)
}

/// The chain with every entry of ours taken out, the rest in order.
pub fn chain_without_ours(chain: Vec<Value>) -> Vec<Value> {
    chain.into_iter().filter(|e| !is_ours(e)).collect()
}

/// The chain with ours first, once, pointing at `file`; the rest in order.
pub fn chain_with_ours_first(chain: Vec<Value>, file: &str) -> Vec<Value> {
    let mut out = vec![json!({ "file": file, "handler": HANDLER })];
    out.extend(chain_without_ours(chain));
    out
}

pub fn keys_value(source: bool, multikey: bool) -> Value {
    json!({ "source": source, "multikey": multikey })
}

/// The `sed` program the boot hook takes our entry out of the chain with, on
/// configd's one-line JSON array (entries are flat `{file, handler}` objects).
pub const CHAIN_SED: &str = r#"s/{[^{}]*"handleOutpick"[^{}]*} *,\{0,1\}//g; s/{[^{}]*"handleLgSourcePicker"[^{}]*} *,\{0,1\}//g; s/, *\]/]/g"#;

/// The `sed -n` program that picks the chain's array out of configd's reply.
pub fn chain_extract_sed() -> String {
    format!(r#"s/.*"{}": *\(\[[^]]*\]\).*/\1/p"#, CHAIN.replace('.', "\\."))
}

// ---- the key filter ----

/// The filter's code. The same text whatever keys are chosen (those are read
/// from configd at run time), so the chain is only rewritten to put it in or
/// take it out, or when a new version of the app changes this text.
pub fn filter_source(app_id: &str) -> String {
    FILTER_TEMPLATE
        .replace("@APP_ID@", app_id)
        .replace("@HANDLER@", HANDLER)
        .replace("@KEYS@", KEYS)
        .replace("@CHAIN@", CHAIN)
}

const FILTER_TEMPLATE: &str = r#"/* Written by outpick (@APP_ID@): the remote's Source key opens it,
 * and, if chosen, a short press on Multikey (a long one stays LG's accessibility).
 * Which of the two is the configd value @KEYS@,
 * { "source": bool, "multikey": bool }, followed from init(): a change applies
 * at once, with nothing reloaded. First in @CHAIN@;
 * every other key goes on to LG's filters.
 * Undo: Customize > Source key, or `outpick --takeover off` as root. */
var outpickId = "@APP_ID@";
// Until configd answers: the Source key only.
var outpickSource = true;
var outpickMultikey = false;
var outpickSourceKeys = (function () {
    var keys = [];
    try { keys.push(WebOS.Key_webOS_InputSource); } catch (e) {}
    try { keys.push(WebOS.Key_webOS_LocalInputSource); } catch (e) {}
    return keys;
})();
var outpickMultiDown = false;
var outpickMultiLong = false;
var outpickPress = 0;
var outpickLaunch = 0;

function outpickKeysChanged(payload) {
    try {
        var r = typeof payload === "string" ? JSON.parse(payload) : payload;
        var v = r && r.configs && r.configs["@KEYS@"];
        if (v && typeof v === "object") {
            outpickSource = v.source !== false;
            outpickMultikey = v.multikey === true;
        }
    } catch (e) {
        console.warn("outpick keyfilter: " + e + "; keys left as they were");
    }
}

var outpickFollowing = false;

function init() {
    if (outpickFollowing) return;
    outpickFollowing = true;
    try {
        lunaCall.subscribe("com.webos.service.config", "/getConfigs",
            JSON.stringify({ "configNames": ["@KEYS@"], "subscribe": true }), outpickKeysChanged, true);
    } catch (e) {
        console.warn("outpick keyfilter: " + e + "; Source key only");
    }
}

function outpickInFront() {
    try {
        return typeof global !== "undefined" && typeof global.isForegroundApp === "function" &&
            global.isForegroundApp(outpickId) === true;
    } catch (e) {
        return false;
    }
}

// LG's own picker, if ours cannot be opened.
function outpickFallback() {
    lunaCall.send("com.webos.service.systemUIManager", "/launch",
        JSON.stringify({ "appId": "com.webos.app.quickinputpicker" }), function () {});
}

// Opens the picker, or, when it is already open, moves it to the next input
// (sent as Tab, which the picker reads as the Source key).
function outpickOpen() {
    if (outpickInFront()) {
        global.sendKeyToFocusWindow(Qt.Key_Tab, true, 15);
        global.sendKeyToFocusWindow(Qt.Key_Tab, false, 15);
        return;
    }
    var launch = ++outpickLaunch;
    var answered = false;
    lunaCall.send("com.webos.applicationManager", "/launch",
        JSON.stringify({ "id": outpickId }), function (payload) {
            answered = true;
            try {
                var r = typeof payload === "string" ? JSON.parse(payload) : payload;
                if (!r || r.returnValue === false) outpickFallback();
            } catch (e) {
                outpickFallback();
            }
        });
    // The app manager may never answer: LG's picker then, so the key is never dead.
    try {
        genericTimer.set(1500, function () {
            if (!answered && launch === outpickLaunch && !outpickInFront()) outpickFallback();
        });
    } catch (e) {}
}

// What LG's long Multikey press opens (appLaunch.js launchAccessibilityOverlay).
function outpickAccessibility() {
    try {
        if (typeof configdService !== "undefined" && configdService.supportNMRM)
            applicationManager.launch("com.palm.app.settings", JSON.stringify({ "target": "accessibility", "item": "audioGuidance" }));
        else
            applicationManager.launch("com.webos.app.accessibility-overlay", JSON.stringify({ "quickMode": true }));
    } catch (e) {
        applicationManager.launch("com.webos.app.accessibility", "{}");
    }
}

// First in the chain, it sees every key: an error must hand the key on.
function @HANDLER@(key, pressed, autoRepeat) {
    try {
        // Should the compositor not have called init(), follow the keys from the first key on.
        if (!outpickFollowing) init();
        if (outpickSourceKeys.indexOf(key) >= 0) {
            if (!outpickSource) return KeyPolicy.NextPolicy;
            if (pressed && !autoRepeat) outpickOpen();
            return KeyPolicy.Accepted;
        }
        var multi = -1;
        try { multi = WebOS.Key_webOS_Multikey; } catch (e) {}
        // Off: LG's, except the release of a press taken while it was on.
        if (key !== multi || (!outpickMultikey && !outpickMultiDown))
            return KeyPolicy.NextPolicy;
        // Multikey: short press = the picker, held 2 s = LG's accessibility.
        if (pressed) {
            if (!autoRepeat) {
                outpickMultiDown = true;
                outpickMultiLong = false;
                var press = ++outpickPress;
                try {
                    genericTimer.set(2000, function () {
                        if (outpickMultiDown && press === outpickPress) {
                            outpickMultiLong = true;
                            outpickAccessibility();
                        }
                    });
                } catch (e) {}
            }
            return KeyPolicy.Accepted;
        }
        outpickMultiDown = false;
        if (!outpickMultiLong) outpickOpen();
        return KeyPolicy.Accepted;
    } catch (e) {
        console.warn("outpick keyfilter: " + e);
        return KeyPolicy.NextPolicy;
    }
}
"#;

// ---- the capture whitelist (webOS 6) ----

fn force_list(v: &Value) -> impl Iterator<Item = &str> {
    v.get("forceCaptureClient").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str)
}

pub fn whitelist_lists(json: &str, id: &str) -> bool {
    serde_json::from_str::<Value>(json).is_ok_and(|v| force_list(&v).any(|c| c == id))
}

/// `json` (the user file, else the system one) with `id` in `forceCaptureClient`.
pub fn whitelist_add(json: &str, id: &str) -> Result<String, String> {
    let mut conf: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let list = conf
        .as_object_mut()
        .ok_or("whitelist is not an object")?
        .entry("forceCaptureClient")
        .or_insert_with(|| json!([]));
    let arr = list.as_array_mut().ok_or("forceCaptureClient is not a list")?;
    // The id this app had before the Outpick rename goes: one entry, the current one.
    arr.retain(|c| c.as_str() != Some(LEGACY_APP_ID));
    if !arr.iter().any(|c| c.as_str() == Some(id)) {
        arr.push(json!(id));
    }
    serde_json::to_string_pretty(&conf).map_err(|e| e.to_string())
}

/// Equal as whitelists: key order, spacing and an empty `forceCaptureClient`
/// against none at all do not matter.
pub fn whitelist_same(a: &Value, b: &Value) -> bool {
    fn norm(v: &Value) -> Value {
        let mut v = v.clone();
        if let Some(o) = v.as_object_mut()
            && o.get("forceCaptureClient").and_then(Value::as_array).is_some_and(|l| l.is_empty())
        {
            o.remove("forceCaptureClient");
        }
        v
    }
    norm(a) == norm(b)
}

/// The user file without `id`, everyone else's entries kept. None when what is
/// left says nothing the system file does not (the file can go). An unreadable
/// file is left as it is.
pub fn whitelist_remove(json: &str, id: &str, system_json: &str) -> Option<String> {
    let Ok(mut conf) = serde_json::from_str::<Value>(json) else {
        return Some(json.to_string());
    };
    if let Some(arr) = conf.get_mut("forceCaptureClient").and_then(Value::as_array_mut) {
        arr.retain(|c| c.as_str() != Some(id) && c.as_str() != Some(LEGACY_APP_ID));
    }
    if let Ok(system) = serde_json::from_str::<Value>(system_json)
        && whitelist_same(&conf, &system)
    {
        return None;
    }
    Some(serde_json::to_string_pretty(&conf).unwrap_or_else(|_| json.to_string()))
}

/// The boot hook's whitelist step (node, ES5): argv user, system, backup, id.
/// Takes `id` out of the user file, deletes the file if what is left equals the
/// system one, deletes a backup equal to the system file; exits 0 when the user
/// file changed (the capture service must reread it).
const WHITELIST_NODE: &str = r#"var fs = require("fs"), a = process.argv.slice(1), u = a[0], s = a[1], b = a[2], id = a[3];
function norm(v) {
    if (Array.isArray(v)) return v.map(norm);
    if (v && typeof v === "object") {
        var o = {};
        Object.keys(v).sort().forEach(function (k) {
            if (k === "forceCaptureClient" && Array.isArray(v[k]) && v[k].length === 0) return;
            o[k] = norm(v[k]);
        });
        return o;
    }
    return v;
}
function read(p) { try { return JSON.parse(fs.readFileSync(p, "utf8")); } catch (e) { return undefined; } }
var sys = read(s), same = function (x) { return sys !== undefined && JSON.stringify(norm(x)) === JSON.stringify(norm(sys)); };
var bak = read(b);
if (bak !== undefined && same(bak)) fs.unlinkSync(b);
var c = read(u);
if (c === undefined || !c || !Array.isArray(c.forceCaptureClient) || c.forceCaptureClient.indexOf(id) < 0) process.exit(1);
c.forceCaptureClient = c.forceCaptureClient.filter(function (x) { return x !== id; });
if (same(c)) { fs.unlinkSync(u); process.exit(0); }
fs.writeFileSync(u + ".tmp", JSON.stringify(c, null, 2));
fs.renameSync(u + ".tmp", u);
"#;

/// Whether the capture service enforces its whitelist, from /etc/os-release.
/// Only webOS 6 (a C1 on 6.5.3) is known to: a QNED on webOS 25 (10.3.2) has
/// the same files but lets the app capture unlisted. Unknown versions: assumed.
pub fn whitelist_enforced(os_release: &str) -> bool {
    let major = os_release
        .lines()
        .find_map(|l| l.strip_prefix("VERSION_ID="))
        .map(|v| v.trim().trim_matches('"'))
        .and_then(|v| v.split('.').next()?.parse::<u32>().ok());
    major.is_none_or(|m| m <= 6)
}

// ---- the boot hook ----

/// The boot hook, copied (not linked) into the Homebrew Channel's init.d by
/// `--takeover on` and `--capture-access on`, so that it still runs once the
/// app is uninstalled: then it undoes what the app set up as root and deletes
/// itself. While the app is there it runs `outpick --boot`.
pub fn hook_script(app_id: &str) -> String {
    let [developer, cryptofs] = app_binaries(app_id);
    HOOK_TEMPLATE
        .replace("@APP_ID@", app_id)
        .replace("@APP_DEV@", &developer)
        .replace("@APP_CRYPTOFS@", &cryptofs)
        .replace("@CHAIN@", CHAIN)
        .replace("@CHAIN_EXTRACT@", &chain_extract_sed())
        .replace("@HANDLER@", HANDLER)
        .replace("@CHAIN_SED@", CHAIN_SED)
        .replace("@ROOT_DIR@", ROOT_DIR)
        .replace("@SHARED_DIR@", SHARED_DIR)
        .replace("@WL_USER@", WHITELIST_USER)
        .replace("@WL_SYSTEM@", WHITELIST_SYSTEM)
        .replace("@WL_BACKUP@", WHITELIST_BACKUP)
        .replace("@WL_NODE@", WHITELIST_NODE)
        .replace("@WL_ID_RE@", &app_id.replace('.', "\\."))
}

const HOOK_TEMPLATE: &str = r#"#!/bin/sh
# outpick (@APP_ID@) boot hook, written by `outpick --takeover on`
# or `--capture-access on`. A copy, not a link: once the app is uninstalled it
# still runs, once, to undo what the app set up as root, then deletes itself.
APP=@APP_DEV@
[ -x "$APP" ] || APP=@APP_CRYPTOFS@
if [ -x "$APP" ]; then
    # Puts the Source key back if configd lost it; removes this hook when nothing is on.
    "$APP" --boot </dev/null >/dev/null 2>&1 &
    exit 0
fi

# The app is gone. Our key filter out of the compositor's chain, LG's left as
# they are; configd may still be starting, and if it never answers this hook
# stays for the next boot.
ok=
for _ in 1 2 3 4 5 6; do
    chain=$(luna-send -n 1 luna://com.webos.service.config/getConfigs '{"configNames":["@CHAIN@"]}' 2>/dev/null |
        tr -d '\n' | sed -n '@CHAIN_EXTRACT@')
    case "$chain" in
    '') ;;
    *'"@HANDLER@"'*)
        rest=$(echo "$chain" | sed '@CHAIN_SED@')
        luna-send -n 1 luna://com.webos.service.config/setConfigs "{\"configs\":{\"@CHAIN@\":$rest}}" 2>/dev/null |
            grep -q '"returnValue": *true' && ok=1
        ;;
    *) ok=1 ;;
    esac
    [ -n "$ok" ] && break
    sleep 5
done
rm -rf @ROOT_DIR@

# The app off the capture whitelist, everyone else's entries kept.
restart=
if [ -f @WL_USER@ ] && grep -q '"@WL_ID_RE@"' @WL_USER@; then
    if command -v node >/dev/null 2>&1; then
        node -e '@WL_NODE@' @WL_USER@ @WL_SYSTEM@ @WL_BACKUP@ @APP_ID@ && restart=1
    else
        tr -d '\n' < @WL_USER@ | sed 's/"@WL_ID_RE@" *,\{0,1\}//g; s/, *\]/]/g' > @WL_USER@.tmp &&
            mv -f @WL_USER@.tmp @WL_USER@ && restart=1
    fi
fi
if [ -n "$restart" ]; then
    for p in /proc/[0-9]*; do
        case "$(tr '\0' ' ' < "$p/cmdline" 2>/dev/null)" in
        /usr/sbin/captureservice*) kill "${p#/proc/}" 2>/dev/null ;;
        esac
    done
fi

# Its files, keeping the settings (config.json) for a reinstall.
rm -f @SHARED_DIR@/preview-*.bmp @SHARED_DIR@/keyfilter.js @SHARED_DIR@/config.json.tmp
[ -n "$ok" ] && rm -f "$0"
exit 0
"#;

#[cfg(test)]
mod tests {
    use super::*;

    const APP: &str = "com.thoxy.outpick";

    fn lg(handler: &str) -> Value {
        json!({ "handler": handler, "file": "/usr/lib/qt5/qml/KeyFilters/appLaunch.js" })
    }
    fn ours(file: &str) -> Value {
        json!({ "handler": HANDLER, "file": file })
    }

    #[test]
    fn chain_edits() {
        let chain = vec![ours(OLD_FILTER), lg("a"), ours(FILTER), lg("b")];
        assert_eq!(chain_without_ours(chain.clone()), vec![lg("a"), lg("b")]);
        assert_eq!(chain_with_ours_first(chain, FILTER), vec![ours(FILTER), lg("a"), lg("b")]);
        // Already in place: the same chain, so nothing is written.
        let fine = vec![ours(FILTER), lg("a")];
        assert_eq!(chain_with_ours_first(fine.clone(), FILTER), fine);
        assert_eq!(chain_with_ours_first(vec![], FILTER), vec![ours(FILTER)]);
        assert!(chain_without_ours(vec![]).is_empty());
    }

    fn balanced(text: &str) -> bool {
        let mut stack = Vec::new();
        for c in text.chars() {
            match c {
                '(' | '{' | '[' => stack.push(c),
                ')' => {
                    if stack.pop() != Some('(') {
                        return false;
                    }
                }
                '}' => {
                    if stack.pop() != Some('{') {
                        return false;
                    }
                }
                ']' => {
                    if stack.pop() != Some('[') {
                        return false;
                    }
                }
                _ => {}
            }
        }
        stack.is_empty()
    }

    #[test]
    fn filter_text() {
        let f = filter_source(APP);
        assert!(!f.contains('@'), "a placeholder is left");
        assert!(balanced(&f), "unbalanced braces or parens");
        assert!(f.contains(&format!("function {HANDLER}(key, pressed, autoRepeat)")));
        assert!(f.contains("function init()"));
        assert!(f.contains(&format!("\"{KEYS}\"")));
        assert!(f.contains(&format!("var outpickId = \"{APP}\";")));
        assert!(f.contains("genericTimer.set(1500"));
        // Constant: the chosen keys are not in the text.
        assert_eq!(f, filter_source(APP));
    }

    /// Checks the filter with node when the machine has it.
    #[test]
    fn filter_parses() {
        let dir = std::env::temp_dir().join(format!("outpick-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("keyfilter.js");
        std::fs::write(&file, filter_source(APP)).unwrap();
        if let Ok(out) = std::process::Command::new("node").arg("--check").arg(&file).output() {
            assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    const SYSTEM: &str = r#"{
    "permissionClient":[
        "com.webos.service.capturepermission"
    ],

    "forceCaptureClient":[
    ]
}"#;

    #[test]
    fn whitelist() {
        let added = whitelist_add(SYSTEM, APP).unwrap();
        assert!(whitelist_lists(&added, APP));
        assert!(!whitelist_lists(SYSTEM, APP));
        // Twice: listed once.
        let again = whitelist_add(&added, APP).unwrap();
        assert_eq!(again.matches(APP).count(), 1);
        // Only ours over the system file: the file can go.
        assert_eq!(whitelist_remove(&added, APP, SYSTEM), None);
        // Someone else's entry stays.
        let shared = whitelist_add(&whitelist_add(SYSTEM, "other.app").unwrap(), APP).unwrap();
        let left = whitelist_remove(&shared, APP, SYSTEM).expect("kept");
        assert!(whitelist_lists(&left, "other.app"));
        assert!(!whitelist_lists(&left, APP));
        // A system file without the list: an empty list left behind is the same.
        let bare = r#"{"permissionClient":["com.webos.service.capturepermission"]}"#;
        assert_eq!(whitelist_remove(&whitelist_add(bare, APP).unwrap(), APP, bare), None);
        // An unreadable file is left alone; an unreadable system file keeps ours removed.
        assert_eq!(whitelist_remove("{oops", APP, SYSTEM).as_deref(), Some("{oops"));
        assert!(whitelist_remove(&added, APP, "").is_some_and(|t| !whitelist_lists(&t, APP)));
        assert!(whitelist_add("[]", APP).is_err());
    }

    #[test]
    fn enforced() {
        assert!(whitelist_enforced("NAME=\"Rockhopper\"\nVERSION=\"6.5.3\"\nVERSION_ID=\"6.5.3\"\n"));
        assert!(!whitelist_enforced("VERSION=\"10.3.2\"\nVERSION_ID=10.3.2\n"));
        assert!(whitelist_enforced(""));
    }

    #[test]
    fn hook_text() {
        let h = hook_script(APP);
        assert!(h.starts_with("#!/bin/sh\n"));
        assert!(!h.contains('@'), "a placeholder is left");
        assert!(!HOOK.rsplit('/').next().unwrap().contains('.'), "run-parts skips dotted names");
        for bin in app_binaries(APP) {
            assert!(h.contains(&bin));
        }
        assert!(h.contains("--boot"));
        assert!(h.contains("rm -f \"$0\""));
        assert!(!h.contains("config.json "), "the settings must stay");
        // The node step sits in single quotes.
        assert!(!WHITELIST_NODE.contains('\''));
        if let Ok(out) = std::process::Command::new("sh").arg("-n").arg("-c").arg(&h).output() {
            assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        }
    }

    /// The hook's sed takes out exactly what chain_without_ours does.
    #[test]
    fn hook_sed_matches() {
        let cases = [
            vec![ours(OLD_FILTER), lg("a"), lg("b")],
            vec![lg("a"), ours(FILTER)],
            vec![lg("a"), ours(FILTER), ours(OLD_FILTER), lg("b")],
            vec![ours(FILTER)],
        ];
        for chain in cases {
            // As configd answers, the chain among other values.
            let reply = json!({ "missingConfigs": [CHAIN_CPP], "configs": { CHAIN: chain }, "returnValue": true });
            let Ok(line) = std::process::Command::new("sed").arg("-n").arg(chain_extract_sed()).stdin_pipe(&reply.to_string())
            else {
                return;
            };
            let Ok(out) = std::process::Command::new("sed").arg(CHAIN_SED).stdin_pipe(&line) else {
                return;
            };
            let got: Vec<Value> = serde_json::from_str(out.trim()).unwrap_or_else(|e| panic!("{e}: {out}"));
            assert_eq!(got, chain_without_ours(chain));
        }
    }

    /// The hook's node step on real files, when node is there.
    #[test]
    fn hook_node_whitelist() {
        if std::process::Command::new("node").arg("--version").output().is_err() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("outpick-wl-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (u, s, b) = (dir.join("user.json"), dir.join("system.json"), dir.join("user.json.bak"));
        std::fs::write(&s, SYSTEM).unwrap();
        let run = || {
            std::process::Command::new("node")
                .arg("-e")
                .arg(WHITELIST_NODE)
                .args([&u, &s, &b])
                .arg(APP)
                .status()
                .unwrap()
                .success()
        };
        // Only ours: deleted, as is a backup equal to the system file.
        std::fs::write(&u, whitelist_add(SYSTEM, APP).unwrap()).unwrap();
        std::fs::write(&b, SYSTEM).unwrap();
        assert!(run());
        assert!(!u.exists() && !b.exists());
        // With another app: kept, ours out.
        std::fs::write(&u, whitelist_add(&whitelist_add(SYSTEM, "other.app").unwrap(), APP).unwrap()).unwrap();
        assert!(run());
        let left = std::fs::read_to_string(&u).unwrap();
        assert!(whitelist_lists(&left, "other.app") && !whitelist_lists(&left, APP));
        // Not listed: nothing to do.
        assert!(!run());
        let _ = std::fs::remove_dir_all(&dir);
    }

    trait StdinPipe {
        fn stdin_pipe(&mut self, input: &str) -> std::io::Result<String>;
    }
    impl StdinPipe for std::process::Command {
        fn stdin_pipe(&mut self, input: &str) -> std::io::Result<String> {
            use std::io::Write;
            let mut child = self.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn()?;
            child.stdin.take().unwrap().write_all(input.as_bytes())?;
            let out = child.wait_with_output()?;
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        }
    }

    #[test]
    fn legacy_entry_is_replaced() {
        let legacy = json!({ "handler": LEGACY_HANDLER, "file": OLD_FILTER });
        let lg = json!({ "handler": "handleLg", "file": "/usr/lib/lg.js" });
        assert!(is_ours(&legacy));
        let chain = chain_with_ours_first(vec![lg.clone(), legacy], FILTER);
        assert_eq!(chain, vec![json!({ "file": FILTER, "handler": HANDLER }), lg]);
        let wl = whitelist_add(r#"{"forceCaptureClient":["com.thoxy.sourcepicker"]}"#, "com.thoxy.outpick").unwrap();
        assert!(wl.contains("com.thoxy.outpick") && !wl.contains("sourcepicker"));
    }

}
