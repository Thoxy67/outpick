//! Denon and Marantz AV receivers over the network: which one is behind this TV's HDMI
//! input, what its inputs are called, which one is on, and switching between them.
//!
//! Two ways to talk to it, chosen in Customize (Automatic: HTTP, else telnet):
//! - HTTP, as Home Assistant's `denonavr` does by default, which any number of clients
//!   can use at once:
//!   `GET /goform/Deviceinfo.xml` (port 8080 on 2016+ models, else 80): model and MAC;
//!   `POST /goform/AppCommand.xml`: power, input, the inputs' names and which are in use;
//!   `GET /goform/formiPhoneAppDirect.xml?SIGAME`: a raw telnet command (`ZMON`, `SI…`).
//! - Telnet (port 23): the same commands and `PW?`, `SI?`, `SSFUN ?`, `SSSOD ?`. Most
//!   receivers take one telnet client at a time (often Home Assistant itself), so each
//!   exchange opens and closes its own short connection.
//!
//! The receiver is identified (model, MAC) over HTTP in both cases.
//!
//! Receivers are found with SSDP (their AiosDevice and MediaRenderer announcements), at the
//! addresses of the media servers the TV lists, and by trying port 8080 on every address
//! of this /24 (SSDP gets no answer inside the app's jail on some TVs). With
//! several on the network, the one this TV is plugged into is the one whose model matches
//! the HDMI-CEC name of the TV's input (then its power state as CEC reports it); a choice
//! made in Customize, or an IP typed there, always wins.

use crate::model::{Shared, Source, SourceKind};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// What Customize sets.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AvrConfig {
    /// Look for a receiver and offer its inputs.
    pub enabled: bool,
    /// The receiver chosen by hand among those found (its MAC); None = automatic.
    pub device: Option<String>,
    /// An address typed by hand ("192.168.1.58", optionally ":port"); wins over the rest.
    pub ip: Option<String>,
    /// How to talk to it.
    pub protocol: Protocol,
}

impl Default for AvrConfig {
    fn default() -> Self {
        AvrConfig { enabled: true, device: None, ip: None, protocol: Protocol::Auto }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Protocol {
    /// HTTP, and telnet when HTTP does not answer.
    #[default]
    Auto,
    Http,
    Telnet,
}

impl Protocol {
    pub const ALL: [Protocol; 3] = [Protocol::Auto, Protocol::Http, Protocol::Telnet];
}

/// A receiver on the network.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Receiver {
    pub ip: String,
    pub port: u16,
    /// Upper-case hex, no separators ("0006780A1B2C"): the same whatever its IP.
    pub mac: String,
    pub model: String,
    /// Its friendly name ("Denon AVC-S670H", or what the user called it).
    pub name: String,
    pub brand: String,
    /// Main zone on, when it was found.
    #[serde(skip)]
    pub power: Option<bool>,
}

/// An input of the receiver: its telnet code ("GAME", "SAT/CBL") and its name as set on it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AvrInput {
    pub code: String,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Status {
    /// Not looked for (switched off in Customize).
    #[default]
    Off,
    Searching,
    /// Talking to `AvrState::device`.
    Ready,
    /// None on the network (or none answering at the address typed).
    NotFound,
    /// Several, and nothing tells which one is this TV's: to be chosen in Customize.
    Ambiguous,
    /// Found before, not answering now.
    Unreachable,
}

/// What the UI shows of the receiver.
#[derive(Clone, Debug, Default)]
pub struct AvrState {
    pub status: Status,
    /// Every receiver found by the last search.
    pub found: Vec<Receiver>,
    /// The one the automatic choice takes (MAC), if any.
    pub auto_pick: Option<String>,
    /// The one in use.
    pub device: Option<Receiver>,
    /// The TV input it is plugged into (`HDMI_3`).
    pub tv_input: Option<String>,
    pub power: Option<bool>,
    /// The input selected on it (a code: "GAME", "TV").
    pub input: Option<String>,
    /// Its inputs worth offering (video ones and TV Audio), in its own order, with their names.
    pub inputs: Vec<AvrInput>,
    /// Its sound mode as it shows it ("Dolby PLII Movie").
    pub sound: Option<String>,
    /// Its main zone's volume as its display shows it (0 to 98, in 0.5 steps), and whether
    /// it is muted; None when it does not say (its buttons are then not offered).
    pub volume: Option<f32>,
    pub muted: Option<bool>,
    /// The protocol set in Customize, and the one that answered last (Http or Telnet).
    pub protocol: Protocol,
    pub link: Option<Protocol>,
}

impl AvrState {
    /// The input selected now, when it is one of the video inputs and the receiver is on.
    pub fn active_input(&self) -> Option<&AvrInput> {
        self.selected_input().filter(|i| is_video(&i.code))
    }

    /// The input selected now (TV Audio included), when the receiver is on.
    pub fn selected_input(&self) -> Option<&AvrInput> {
        if self.power != Some(true) {
            return None;
        }
        let cur = self.input.as_deref()?;
        self.inputs.iter().find(|i| same_code(&i.code, cur))
    }

    /// Whether the receiver's inputs can be offered on TV input `id`.
    pub fn serves(&self, id: &str) -> bool {
        self.device.is_some() && !self.inputs.is_empty() && self.tv_input.as_deref() == Some(id)
    }
}

// ---------------------------------------------------------------------------------------
// Answers, parsed (plain text in, plain data out: tested with captured answers).

/// The text of every `<name>…</name>` in `xml`, trimmed (no nesting, no attributes).
fn tags<'a>(xml: &'a str, name: &str) -> Vec<&'a str> {
    let (open, close) = (format!("<{name}>"), format!("</{name}>"));
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find(&open) {
        rest = &rest[i + open.len()..];
        let Some(j) = rest.find(&close) else { break };
        out.push(rest[..j].trim());
        rest = &rest[j + close.len()..];
    }
    out
}

fn tag<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    tags(xml, name).into_iter().next()
}

/// Model, MAC and maker from `Deviceinfo.xml`; None when it is not a receiver's.
pub fn parse_device_info(xml: &str) -> Option<(String, String, String)> {
    let model = tag(xml, "ModelName").or_else(|| tag(xml, "ManualModelName")).filter(|m| !m.is_empty())?;
    let mac = tag(xml, "MacAddress").unwrap_or("").to_ascii_uppercase();
    let brand = match tag(xml, "BrandCode") {
        Some("1") => "Marantz",
        _ => "Denon",
    };
    Some((model.to_string(), mac, brand.to_string()))
}

/// What one `AppCommand.xml` answer says.
#[derive(Debug, Default, PartialEq)]
pub struct Report {
    pub power: Option<bool>,
    pub input: Option<String>,
    pub name: Option<String>,
    /// All inputs in use (audio ones too), as (code, name).
    pub inputs: Vec<AvrInput>,
    pub sound: Option<String>,
    pub volume: Option<f32>,
    pub muted: Option<bool>,
}

/// The status requests (the receivers take at most 5 commands per request, and answer
/// `<error>1</error>` to more): power, input, sound mode and friendly name; then the
/// inputs' names and which are in use.
const STATUS_REQUEST: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<tx>\n",
    "<cmd id=\"1\">GetAllZonePowerStatus</cmd>\n",
    "<cmd id=\"1\">GetAllZoneSource</cmd>\n",
    "<cmd id=\"1\">GetSurroundModeStatus</cmd>\n",
    "<cmd id=\"1\">GetFriendlyName</cmd>\n",
    "</tx>\n"
);
const NAMES_REQUEST: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<tx>\n",
    "<cmd id=\"1\">GetRenameSource</cmd>\n",
    "<cmd id=\"1\">GetDeletedSource</cmd>\n",
    "</tx>\n"
);
/// Volume and mute have their own request: their answers' `<zone1>` would read as power in
/// `parse_report`.
const VOLUME_REQUEST: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<tx>\n",
    "<cmd id=\"1\">GetVolumeLevel</cmd>\n",
    "<cmd id=\"1\">GetMuteStatus</cmd>\n",
    "</tx>\n"
);

/// A volume as the receiver's display shows it, when it is one (0 to 98).
fn displayed(v: f32) -> Option<f32> {
    (0.0..=98.0).contains(&v).then_some(v)
}

/// The main zone's volume (as displayed) and mute from the answer to `VOLUME_REQUEST`.
/// `<volume>` is in dB relative to the reference level (-80 to +18): displayed = dB + 80.
pub fn parse_volume(xml: &str) -> (Option<f32>, Option<bool>) {
    let (mut volume, mut muted) = (None, None);
    for cmd in xml.split("<cmd>").skip(1) {
        let zone = tag(cmd, "zone1").unwrap_or(cmd);
        if let Some(v) = tag(zone, "volume") {
            volume = v.trim_end_matches("dB").trim().parse::<f32>().ok().and_then(|db| displayed(db + 80.0));
        }
        if let Some(m) = tag(zone, "mute") {
            muted = match m.to_ascii_lowercase().as_str() {
                "on" => Some(true),
                "off" => Some(false),
                _ => None,
            };
        }
    }
    (volume, muted)
}

/// A telnet volume ("45", or "455" for 45.5) as displayed.
fn telnet_volume(v: &str) -> Option<f32> {
    if !v.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: f32 = v.parse().ok()?;
    displayed(match v.len() {
        2 => n,
        3 => n / 10.0,
        _ => return None,
    })
}

pub fn parse_report(xml: &str) -> Report {
    let mut r = Report::default();
    let mut renamed: Vec<(String, String)> = Vec::new();
    let mut unused: Vec<String> = Vec::new();
    for cmd in xml.split("<cmd>").skip(1) {
        if cmd.contains("<functionrename>") {
            for item in tags(cmd, "list") {
                if let (Some(n), Some(r)) = (tag(item, "name"), tag(item, "rename")) {
                    renamed.push((n.to_string(), r.to_string()));
                }
            }
        } else if cmd.contains("<functiondelete>") {
            for item in tags(cmd, "list") {
                if let (Some(n), Some("0")) = (tag(item, "name"), tag(item, "use")) {
                    unused.push(n.to_string());
                }
            }
        } else if let Some(name) = tag(cmd, "friendlyname") {
            r.name = Some(name.to_string()).filter(|n| !n.is_empty());
        } else if let Some(mode) = tag(cmd, "surround") {
            r.sound = Some(mode.to_string()).filter(|m| !m.is_empty());
        } else if let Some(src) = tag(cmd, "source") {
            r.input = Some(src.to_string()).filter(|s| !s.is_empty());
        } else if let Some(p) = tag(cmd, "zone1") {
            r.power = match p {
                "ON" => Some(true),
                "OFF" | "STANDBY" => Some(false),
                _ => None,
            };
        }
    }
    for (n, shown) in renamed {
        if unused.contains(&n) {
            continue;
        }
        if let Some(code) = code_of(&n) {
            let name = if shown.is_empty() { n.clone() } else { shown };
            r.inputs.push(AvrInput { code: code.to_string(), name });
        }
    }
    r
}

/// What telnet answers to `PW?`, `SI?`, `MV?`, `MU?`, `SSFUN ?`, `SSSOD ?` and `NSFRN ?`, as
/// one report. Lines: "PWON", "SIGAME", "MV455", "MUOFF", "SSFUNSAT/CBL Freebox Player",
/// "SSSODBD DEL", "NSFRN Denon …".
pub fn parse_telnet(text: &str) -> Report {
    let mut r = Report::default();
    let mut renamed: Vec<(String, String)> = Vec::new();
    let mut unused: Vec<String> = Vec::new();
    for line in text.split(['\r', '\n']).map(str::trim).filter(|l| !l.is_empty()) {
        if let Some(p) = line.strip_prefix("PW") {
            r.power = match p {
                "ON" => Some(true),
                "STANDBY" | "OFF" => Some(false),
                _ => r.power,
            };
        } else if let Some(p) = line.strip_prefix("ZM") {
            // The main zone's own state wins over the whole unit's.
            match p {
                "ON" => r.power = Some(true),
                "OFF" => r.power = Some(false),
                _ => {}
            }
        } else if let Some(rest) = line.strip_prefix("SSFUN") {
            if let Some((code, name)) = rest.split_once(' ')
                && code != "END"
            {
                renamed.push((code.to_string(), name.trim().to_string()));
            }
        } else if let Some(rest) = line.strip_prefix("SSSOD") {
            if let Some((code, "DEL")) = rest.split_once(' ').map(|(c, u)| (c, u.trim())) {
                unused.push(code.to_string());
            }
        } else if let Some(v) = line.strip_prefix("MV") {
            // "MVMAX 98" is the most it may be set to, not its level.
            if let Some(v) = telnet_volume(v.trim()) {
                r.volume = Some(v);
            }
        } else if let Some(m) = line.strip_prefix("MU") {
            match m {
                "ON" => r.muted = Some(true),
                "OFF" => r.muted = Some(false),
                _ => {}
            }
        } else if let Some(mode) = line.strip_prefix("MS") {
            r.sound = Some(mode.trim().to_string()).filter(|m| !m.is_empty());
        } else if let Some(name) = line.strip_prefix("NSFRN ") {
            r.name = Some(name.trim().to_string()).filter(|n| !n.is_empty());
        } else if let Some(src) = line.strip_prefix("SI") {
            r.input = Some(src.trim().to_string()).filter(|s| !s.is_empty());
        }
    }
    for (code, shown) in renamed {
        if unused.iter().any(|u| same_code(u, &code)) {
            continue;
        }
        if let Some(code) = code_of(&code) {
            let name = if shown.is_empty() { code.to_string() } else { shown };
            r.inputs.push(AvrInput { code: code.to_string(), name });
        }
    }
    r
}

/// The telnet code (`SI<code>`) of an input as `AppCommand.xml` names it.
pub fn code_of(name: &str) -> Option<&'static str> {
    Some(match name.trim().to_ascii_uppercase().as_str() {
        "CBL/SAT" | "SAT/CBL" => "SAT/CBL",
        "MEDIA PLAYER" | "MPLAY" => "MPLAY",
        "BLU-RAY" | "BD" => "BD",
        "GAME" | "GAME1" => "GAME",
        "GAME2" => "GAME2",
        "DVD" => "DVD",
        "DVR" => "DVR",
        "8K" => "8K",
        "AUX1" | "AUX" => "AUX1",
        "AUX2" => "AUX2",
        "AUX3" => "AUX3",
        "AUX4" => "AUX4",
        "TV AUDIO" | "TV" => "TV",
        "PHONO" => "PHONO",
        "CD" => "CD",
        "TUNER" | "FM" => "TUNER",
        "NETWORK" | "NET" => "NET",
        "BLUETOOTH" | "BT" => "BT",
        "IPOD/USB" | "USB/IPOD" | "USB" => "USB/IPOD",
        _ => return None,
    })
}

/// Inputs that carry a picture (HDMI): the ones worth offering on a TV.
pub fn is_video(code: &str) -> bool {
    matches!(code, "SAT/CBL" | "MPLAY" | "BD" | "GAME" | "GAME2" | "DVD" | "DVR" | "8K" | "AUX1" | "AUX2" | "AUX3" | "AUX4")
}

/// Inputs offered in the receiver's options: the video ones, and TV Audio (the TV's own
/// sound through the receiver).
pub fn offered(code: &str) -> bool {
    is_video(code) || code == "TV"
}

/// Sound modes offered with Left/Right, as the receiver's remote groups them: their
/// telnet codes (`MS<code>`).
pub const SOUND_MODES: [&str; 7] = ["MOVIE", "MUSIC", "GAME", "DIRECT", "PURE DIRECT", "STEREO", "AUTO"];

/// Which of `SOUND_MODES` a mode the receiver reports belongs to ("Dolby PLII Movie" → MOVIE).
pub fn sound_group(mode: &str) -> Option<usize> {
    let m = format!(" {} ", mode.trim().to_ascii_uppercase());
    let has = |w: &str| m.contains(w);
    let group = if has("PURE") {
        "PURE DIRECT"
    } else if has("DIRECT") {
        "DIRECT"
    } else if has("GAME") || has(" GM ") {
        "GAME"
    } else if has("STEREO") {
        "STEREO"
    } else if has("MUSIC") || has(" MS ") {
        "MUSIC"
    } else if has("MOVIE") || has("CINEMA") || has(" MV ") {
        "MOVIE"
    } else if has("AUTO") {
        "AUTO"
    } else {
        return None;
    };
    SOUND_MODES.iter().position(|g| *g == group)
}

/// Whether two codes name the same input ("GAME1" is reported as "GAME" on some models).
pub fn same_code(a: &str, b: &str) -> bool {
    let norm = |c: &str| code_of(c).map(str::to_string).unwrap_or_else(|| c.trim().to_ascii_uppercase());
    norm(a) == norm(b)
}

/// The icon for an input, from what it is called on the receiver, then what it is.
pub fn icon_for(input: &AvrInput) -> crate::icons::SourceIcon {
    use crate::icons::SourceIcon;
    let hint = crate::model::detect_icon(
        &crate::model::DeviceHints { cec_name: input.name.clone(), label: input.name.clone(), ..Default::default() },
        SourceKind::Hdmi,
    );
    if hint != SourceIcon::Cable {
        return hint;
    }
    match input.code.as_str() {
        "GAME" | "GAME2" => SourceIcon::Console,
        "BD" | "DVD" => SourceIcon::Movie,
        "SAT/CBL" | "DVR" => SourceIcon::StreamingBox,
        "MPLAY" => SourceIcon::Cast,
        _ => SourceIcon::Cable,
    }
}

/// Letters and digits only, upper-case: "Denon AVC-S670H" → "DENONAVCS670H".
fn squash(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect()
}

/// Whether a receiver's model is what a TV input calls its device (its CEC name
/// "AVC-S670H", or "Denon AVC-S670H").
fn model_matches(model: &str, device: &str) -> bool {
    let (m, d) = (squash(model), squash(device));
    !m.is_empty() && !d.is_empty() && (m == d || d.contains(&m) || m.contains(&d) && d.len() >= 6)
}

/// Whether a TV input's device is a Denon or Marantz (a receiver, as far as we can tell).
pub fn is_receiver_input(s: &Source) -> bool {
    let text = format!("{} {} {}", s.brand.as_deref().unwrap_or(""), s.device.as_deref().unwrap_or(""), s.label).to_ascii_lowercase();
    s.kind == SourceKind::Hdmi && s.connected && (text.contains("denon") || text.contains("marantz"))
}

/// The TV's receivers: HDMI inputs whose device is a Denon or Marantz.
fn receiver_inputs(sources: &[Source]) -> Vec<&Source> {
    sources.iter().filter(|s| is_receiver_input(s)).collect()
}

/// The TV input receiver `r` is plugged into: the Denon/Marantz input of its model, else
/// the only Denon/Marantz input.
pub fn tv_input_for(sources: &[Source], r: &Receiver) -> Option<String> {
    let candidates = receiver_inputs(sources);
    input_of_model(sources, r).or_else(|| (candidates.len() == 1).then(|| candidates[0].id.clone()))
}

/// The Denon/Marantz TV input whose CEC name is receiver `r`'s model.
pub fn input_of_model(sources: &[Source], r: &Receiver) -> Option<String> {
    receiver_inputs(sources).iter().find(|s| s.device.as_deref().is_some_and(|d| model_matches(&r.model, d))).map(|s| s.id.clone())
}

/// What the automatic choice decides.
#[derive(Debug, PartialEq)]
pub enum Pick {
    One(usize),
    Ambiguous,
    None,
}

/// The receiver this TV is plugged into, among `found`: the only one; else the ones whose
/// model matches the CEC name of a Denon/Marantz input; then those whose power matches
/// what CEC reports for it; then the one used last time.
pub fn choose(found: &[Receiver], sources: &[Source], last: Option<&str>) -> Pick {
    if found.is_empty() {
        return Pick::None;
    }
    let mut pool: Vec<usize> = (0..found.len()).collect();
    let inputs = receiver_inputs(sources);
    let by_model: Vec<usize> = pool
        .iter()
        .copied()
        .filter(|&i| inputs.iter().any(|s| s.device.as_deref().is_some_and(|d| model_matches(&found[i].model, d))))
        .collect();
    if !by_model.is_empty() {
        pool = by_model;
    }
    if pool.len() > 1 {
        let powers: Vec<bool> = inputs.iter().filter_map(|s| s.cec_power).collect();
        if let [power] = powers[..] {
            let by_power: Vec<usize> = pool.iter().copied().filter(|&i| found[i].power == Some(power)).collect();
            if !by_power.is_empty() {
                pool = by_power;
            }
        }
    }
    match pool[..] {
        [one] => Pick::One(one),
        _ => match last.and_then(|mac| pool.iter().copied().find(|&i| found[i].mac == mac)) {
            Some(i) => Pick::One(i),
            None => Pick::Ambiguous,
        },
    }
}

/// "192.168.1.58" or "192.168.1.58:8080" (no port: the usual ones are tried).
pub fn parse_address(text: &str) -> Option<(String, Option<u16>)> {
    let text = text.trim();
    let (host, port) = match text.rsplit_once(':') {
        Some((h, p)) => (h, Some(p.parse::<u16>().ok().filter(|p| *p > 0)?)),
        None => (text, None),
    };
    host.parse::<std::net::Ipv4Addr>().ok()?;
    Some((host.to_string(), port))
}

// ---------------------------------------------------------------------------------------
// The network.

const CONNECT_TIMEOUT: Duration = Duration::from_millis(1200);
const IO_TIMEOUT: Duration = Duration::from_millis(2500);

/// A plain HTTP/1.0 exchange; the body of a 200 answer.
fn http(ip: &str, port: u16, method: &str, path: &str, body: Option<&str>) -> Result<String, String> {
    let addr: SocketAddr = (ip, port).to_socket_addrs().map_err(|e| e.to_string())?.next().ok_or("no address")?;
    let mut s = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).map_err(|e| e.to_string())?;
    s.set_read_timeout(Some(IO_TIMEOUT)).map_err(|e| e.to_string())?;
    s.set_write_timeout(Some(IO_TIMEOUT)).map_err(|e| e.to_string())?;
    let mut req = format!("{method} {path} HTTP/1.0\r\nHost: {ip}:{port}\r\nConnection: close\r\n");
    if let Some(b) = body {
        req.push_str(&format!("Content-Type: text/xml; charset=utf-8\r\nContent-Length: {}\r\n", b.len()));
    }
    req.push_str("\r\n");
    if let Some(b) = body {
        req.push_str(b);
    }
    s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut raw = Vec::new();
    // A server that keeps the socket open past its answer ends at the timeout: keep what came.
    if let Err(e) = s.read_to_end(&mut raw)
        && raw.is_empty()
    {
        return Err(e.to_string());
    }
    let text = String::from_utf8_lossy(&raw);
    let (head, body) = text.split_once("\r\n\r\n").ok_or("bad answer")?;
    let status = head.split_whitespace().nth(1).unwrap_or("");
    if status != "200" {
        return Err(format!("HTTP {status}"));
    }
    if head.to_ascii_lowercase().contains("transfer-encoding: chunked") {
        return Ok(unchunk(body));
    }
    Ok(body.to_string())
}

fn unchunk(body: &str) -> String {
    let mut out = String::new();
    let mut rest = body;
    while let Some((size, after)) = rest.split_once("\r\n") {
        let Ok(n) = usize::from_str_radix(size.trim(), 16) else { break };
        if n == 0 {
            break;
        }
        // Cut short, or a size that falls inside a character (the text was made valid
        // UTF-8 before this): keep what is left as it is.
        let Some(chunk) = after.get(..n) else {
            out.push_str(after);
            break;
        };
        out.push_str(chunk);
        rest = after[n..].strip_prefix("\r\n").unwrap_or(&after[n..]);
    }
    out
}

/// The receiver at `ip` (on `port`, or 8080 then 80), with its state.
pub fn probe(ip: &str, port: Option<u16>) -> Option<Receiver> {
    let ports: &[u16] = match &port {
        Some(p) => std::slice::from_ref(p),
        None => &[8080, 80],
    };
    for &port in ports {
        let Ok(xml) = http(ip, port, "GET", "/goform/Deviceinfo.xml", None) else { continue };
        let Some((model, mac, brand)) = parse_device_info(&xml) else { continue };
        let report = http(ip, port, "POST", "/goform/AppCommand.xml", Some(STATUS_REQUEST)).map(|x| parse_report(&x)).unwrap_or_default();
        let name = report.name.clone().unwrap_or_else(|| format!("{brand} {model}"));
        return Some(Receiver { ip: ip.to_string(), port, mac, model, name, brand, power: report.power });
    }
    None
}

/// An address typed by hand: identified over HTTP, else (telnet allowed) taken on its
/// telnet answer alone, named as it calls itself.
fn probe_typed(ip: &str, port: Option<u16>, p: Protocol) -> Option<Receiver> {
    if let Some(r) = probe(ip, port) {
        return Some(r);
    }
    if p == Protocol::Http {
        return None;
    }
    let bare = Receiver { ip: ip.to_string(), port: port.unwrap_or(80), mac: format!("telnet:{ip}"), brand: "Denon".into(), ..Default::default() };
    let rep = telnet_report(&bare).ok()?;
    Some(Receiver { name: rep.name.clone().unwrap_or_else(|| ip.to_string()), power: rep.power, ..bare })
}

const TELNET_PORT: u16 = 23;

/// One short telnet exchange: sends `cmds` (50 ms apart, as the receivers want), then
/// reads what comes back until `done` says it is all there, or the time is up.
fn telnet(ip: &str, cmds: &[&str], done: impl Fn(&str) -> bool) -> Result<String, String> {
    let addr: SocketAddr = (ip, TELNET_PORT).to_socket_addrs().map_err(|e| e.to_string())?.next().ok_or("no address")?;
    let mut s = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).map_err(|e| format!("telnet: {e}"))?;
    s.set_read_timeout(Some(Duration::from_millis(150))).map_err(|e| e.to_string())?;
    s.set_write_timeout(Some(IO_TIMEOUT)).map_err(|e| e.to_string())?;
    let mut got = Vec::new();
    let mut buf = [0u8; 1024];
    let mut read = |s: &mut TcpStream, got: &mut Vec<u8>| -> Result<(), String> {
        match s.read(&mut buf) {
            Ok(0) => Err("telnet: closed".into()),
            Ok(n) => {
                got.extend_from_slice(&buf[..n]);
                Ok(())
            }
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => Ok(()),
            Err(e) => Err(format!("telnet: {e}")),
        }
    };
    for cmd in cmds {
        s.write_all(format!("{cmd}\r").as_bytes()).map_err(|e| format!("telnet: {e}"))?;
        std::thread::sleep(Duration::from_millis(50));
    }
    let deadline = Instant::now() + Duration::from_millis(1500);
    while Instant::now() < deadline {
        if done(&String::from_utf8_lossy(&got)) {
            break;
        }
        if let Err(e) = read(&mut s, &mut got) {
            // Closed by the receiver (another client took the line): keep what came.
            if got.is_empty() {
                return Err(e);
            }
            break;
        }
    }
    Ok(String::from_utf8_lossy(&got).into_owned())
}

fn telnet_report(r: &Receiver) -> Result<Report, String> {
    // MV? and MU? before the lists, so they are answered before the lists' END that ends the read.
    let text = telnet(&r.ip, &["PW?", "ZM?", "SI?", "MS?", "MV?", "MU?", "SSFUN ?", "SSSOD ?", "NSFRN ?"], |t| {
        t.contains("SI") && t.contains("SSFUN END") && t.contains("SSSOD END") && t.contains("NSFRN")
    })?;
    let mut rep = parse_telnet(&text);
    if rep.power.is_none() && rep.input.is_none() {
        return Err("telnet: no answer".into());
    }
    // Cut by the deadline: the input list is unknown, not shorter.
    if !(text.contains("SSFUN END") && text.contains("SSSOD END")) {
        rep.inputs.clear();
    }
    Ok(rep)
}

/// The receiver's state, over `p` (Auto: HTTP, else telnet); and which one answered.
fn report(r: &Receiver, p: Protocol) -> Result<(Report, Protocol), String> {
    let http_report = || -> Result<(Report, Protocol), String> {
        let post = |body: &str| http(&r.ip, r.port, "POST", "/goform/AppCommand.xml", Some(body));
        let status = post(STATUS_REQUEST)?;
        let names = post(NAMES_REQUEST)?;
        if status.contains("<error>") || names.contains("<error>") {
            return Err("AppCommand refused".into());
        }
        let mut rep = parse_report(&format!("{status}{names}"));
        // Without it the rest still stands: no volume, that is all.
        if let Ok((v, m)) = http_volume(r) {
            (rep.volume, rep.muted) = (v, m);
        }
        Ok((rep, Protocol::Http))
    };
    match p {
        Protocol::Http => http_report(),
        Protocol::Telnet => telnet_report(r).map(|x| (x, Protocol::Telnet)),
        Protocol::Auto => http_report().or_else(|e| telnet_report(r).map(|x| (x, Protocol::Telnet)).map_err(|t| format!("{e}; {t}"))),
    }
}

fn http_volume(r: &Receiver) -> Result<(Option<f32>, Option<bool>), String> {
    let xml = http(&r.ip, r.port, "POST", "/goform/AppCommand.xml", Some(VOLUME_REQUEST))?;
    if xml.contains("<error>") {
        return Err("AppCommand refused".into());
    }
    Ok(parse_volume(&xml))
}

/// The volume and mute alone, over the protocol that answered last.
fn volume_report(r: &Receiver, p: Protocol) -> Result<(Option<f32>, Option<bool>), String> {
    let over_telnet = || {
        let text = telnet(&r.ip, &["MV?", "MU?"], |t| t.contains("MV") && t.contains("MU"))?;
        let rep = parse_telnet(&text);
        Ok((rep.volume, rep.muted))
    };
    match p {
        Protocol::Telnet => over_telnet(),
        _ => http_volume(r).or_else(|e| if p == Protocol::Auto { over_telnet().map_err(|t: String| format!("{e}; {t}")) } else { Err(e) }),
    }
}

/// The volume keys: relative steps only, so a misread level can never be sent back louder.
pub const VOLUME_UP: &str = "MVUP";
pub const VOLUME_DOWN: &str = "MVDOWN";
pub const MUTE_ON: &str = "MUON";
pub const MUTE_OFF: &str = "MUOFF";

/// The HTTP path that passes telnet command `cmd` on: spaces encoded ("MNMEN ON" sent raw
/// breaks the request line, and the receiver answers 400).
fn command_path(cmd: &str) -> String {
    format!("/goform/formiPhoneAppDirect.xml?{}", cmd.replace(' ', "%20"))
}

/// Sends a telnet command ("ZMON", "SIGAME"), over HTTP or telnet as `p` says.
fn command(r: &Receiver, p: Protocol, cmd: &str) -> Result<(), String> {
    let over_http = || http(&r.ip, r.port, "GET", &command_path(cmd), None).map(|_| ());
    let over_telnet = || telnet(&r.ip, &[cmd], |_| true).map(|_| ());
    match p {
        Protocol::Http => over_http(),
        Protocol::Telnet => over_telnet(),
        Protocol::Auto => over_http().or_else(|e| over_telnet().map_err(|t| format!("{e}; {t}"))),
    }
}

/// The addresses that answer an SSDP search for receivers within `wait`.
fn ssdp(wait: Duration) -> Vec<String> {
    let Ok(sock) = UdpSocket::bind("0.0.0.0:0") else { return Vec::new() };
    let _ = sock.set_multicast_ttl_v4(2);
    let _ = sock.set_read_timeout(Some(Duration::from_millis(200)));
    let search = |st: &str| format!("M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 1\r\nST: {st}\r\n\r\n");
    let targets = ["urn:schemas-denon-com:device:AiosDevice:1", "urn:schemas-upnp-org:device:MediaRenderer:1", "upnp:rootdevice"];
    let mut ips: Vec<String> = Vec::new();
    let start = Instant::now();
    let mut sent = 0;
    let mut buf = [0u8; 2048];
    while start.elapsed() < wait {
        // Twice, a moment apart: UDP may lose the first.
        if sent < 2 && start.elapsed() >= Duration::from_millis(300) * sent {
            for st in targets {
                let _ = sock.send_to(search(st).as_bytes(), "239.255.255.250:1900");
            }
            sent += 1;
        }
        if let Ok((_, from)) = sock.recv_from(&mut buf) {
            let ip = from.ip().to_string();
            if !ips.contains(&ip) {
                ips.push(ip);
            }
        }
    }
    ips
}

/// This machine's address on the LAN (no packet sent: the route to a multicast group).
fn own_ipv4() -> Option<std::net::Ipv4Addr> {
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("239.255.255.250:1900").ok()?;
    match sock.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(ip) if !ip.is_loopback() && !ip.is_unspecified() => Some(ip),
        _ => None,
    }
}

/// A thread for one connection: a small stack, as 64 default ones would reserve 128 MB of
/// address space on the TV's 32-bit ARM.
fn small_thread() -> std::thread::Builder {
    std::thread::Builder::new().stack_size(64 * 1024)
}

/// Every address of this /24 with the receivers' HTTP port (8080) open: the last resort
/// when SSDP gets no answer (blocked in the app's jail on some TVs).
fn sweep() -> Vec<String> {
    let Some(me) = own_ipv4() else { return Vec::new() };
    let [a, b, c, mine] = me.octets();
    let hosts: Vec<u8> = (1..=254).filter(|h| *h != mine).collect();
    let open = |ip: String| -> Option<String> {
        let addr: SocketAddr = format!("{ip}:8080").parse().ok()?;
        TcpStream::connect_timeout(&addr, Duration::from_millis(400)).ok().map(|_| ip)
    };
    let mut out = Vec::new();
    for chunk in hosts.chunks(64) {
        // A thread the system refuses is an address not tried (`thread::spawn` would panic).
        let tries: Vec<_> =
            chunk.iter().map(|h| format!("{a}.{b}.{c}.{h}")).filter_map(|ip| small_thread().spawn(move || open(ip)).ok()).collect();
        out.extend(tries.into_iter().filter_map(|t| t.join().ok().flatten()));
    }
    out
}

/// Every receiver answering on the network: those SSDP finds, those the TV already knows
/// (`hints`), and every host with the receivers' port open.
fn discover(hints: &[String]) -> Vec<Receiver> {
    let probe_all = |ips: Vec<String>| -> Vec<Receiver> {
        let probes: Vec<_> = ips.into_iter().filter_map(|ip| small_thread().spawn(move || probe(&ip, None)).ok()).collect();
        probes.into_iter().filter_map(|h| h.join().ok().flatten()).collect()
    };
    let mut ips = ssdp(Duration::from_millis(1500));
    for h in hints {
        if !ips.contains(h) {
            ips.push(h.clone());
        }
    }
    // The network too, always: SSDP may get no answer, and the TV may know only some.
    for ip in sweep() {
        if !ips.contains(&ip) {
            ips.push(ip);
        }
    }
    let mut found = probe_all(ips);
    // One entry per receiver (it may answer on two addresses), listed by address.
    found.sort_by(|a, b| (&a.mac, &a.ip).cmp(&(&b.mac, &b.ip)));
    found.dedup_by(|a, b| a.mac == b.mac && !a.mac.is_empty());
    found.sort_by(|a, b| a.ip.cmp(&b.ip));
    found
}

/// `outpick --avr-scan`: each way of finding receivers, and what answers, on stdout.
#[cfg_attr(not(feature = "webos"), allow(dead_code))]
pub fn scan_report() {
    let own = own_ipv4().map(|i| i.to_string()).unwrap_or_else(|| "none".into());
    println!("own address: {own}");
    let ssdp_ips = ssdp(Duration::from_millis(1500));
    println!("ssdp: {}", ssdp_ips.join(" "));
    let swept = sweep();
    println!("port 8080 open: {}", swept.join(" "));
    for ip in ssdp_ips.iter().chain(&swept) {
        match probe(ip, None) {
            Some(r) => {
                let input = report(&r, Protocol::Http).ok().and_then(|(rep, _)| rep.input).unwrap_or_default();
                println!("receiver {} {} {} mac {} input {input}", r.ip, r.brand, r.model, r.mac);
            }
            None => println!("{ip}: not a receiver"),
        }
    }
}

// ---------------------------------------------------------------------------------------
// The worker: finds the receiver, then follows it while the app runs.

/// The last receiver used and its inputs, so the next start shows them at once.
#[derive(Serialize, Deserialize, Default, PartialEq)]
struct Cache {
    device: Option<Receiver>,
    inputs: Vec<AvrInput>,
}

fn cache_path() -> std::path::PathBuf {
    crate::config::Config::path().with_file_name("avr.json")
}

fn load_cache() -> Cache {
    std::fs::read(cache_path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// Written only when it differs from `last` (the TV's storage is flash), which it then becomes.
fn save_cache(c: Cache, last: &mut Cache) {
    if *last == c {
        return;
    }
    *last = c;
    let c = &*last;
    let path = cache_path();
    let tmp = path.with_extension("json.tmp");
    if let Ok(bytes) = serde_json::to_vec(c)
        && std::fs::write(&tmp, bytes).is_ok()
    {
        let _ = std::fs::rename(&tmp, &path);
    }
}

enum Msg {
    Config(AvrConfig),
    Search,
}

/// Talks to the worker.
pub struct Handle {
    tx: mpsc::Sender<Msg>,
}

impl Handle {
    /// A handle with no worker behind it (the desktop preview's simulated receiver).
    #[cfg(not(feature = "webos"))]
    pub fn detached() -> Handle {
        Handle { tx: mpsc::channel().0 }
    }
    pub fn configure(&self, c: AvrConfig) {
        let _ = self.tx.send(Msg::Config(c));
    }
    /// Looks for receivers again (Customize's receiver list opened).
    pub fn search(&self) {
        let _ = self.tx.send(Msg::Search);
    }
}

/// Starts the worker; it waits for the first `configure`.
pub fn start(state: Shared, repaint: impl Fn() + Send + 'static) -> Handle {
    let (tx, rx) = mpsc::channel();
    let _ = std::thread::Builder::new().name("avr".into()).spawn(move || worker(state, rx, repaint));
    Handle { tx }
}

/// How often the receiver's power and input are read while the app runs.
const POLL: Duration = Duration::from_secs(2);

/// The newest settings among those waiting (a held arrow on a Customize row sends many):
/// `cfg` updated, and whether a search was asked for meanwhile.
fn drain(rx: &mpsc::Receiver<Msg>, cfg: &mut AvrConfig) -> bool {
    let mut search = false;
    while let Ok(m) = rx.try_recv() {
        match m {
            Msg::Config(c) => *cfg = c,
            Msg::Search => search = true,
        }
    }
    search
}

/// What ends a wait: new settings, an explicit search, the time, or the app.
enum Wake {
    Config,
    Search,
    Timeout,
    Quit,
}

/// Waits up to `wait` (None: until something comes), then takes every message waiting.
fn wait(rx: &mpsc::Receiver<Msg>, cfg: &mut AvrConfig, wait: Option<Duration>) -> Wake {
    let first = match wait {
        Some(d) => rx.recv_timeout(d).map_err(|e| matches!(e, mpsc::RecvTimeoutError::Disconnected)),
        None => rx.recv().map_err(|_| true),
    };
    let wake = match first {
        Ok(Msg::Config(c)) => {
            *cfg = c;
            Wake::Config
        }
        Ok(Msg::Search) => Wake::Search,
        Err(true) => return Wake::Quit,
        Err(false) => Wake::Timeout,
    };
    match (drain(rx, cfg), wake) {
        (true, Wake::Config | Wake::Timeout) => Wake::Search,
        (_, w) => w,
    }
}

/// The settings that pick the receiver (not the protocol, which only changes how to talk).
fn same_target(a: &AvrConfig, b: &AvrConfig) -> bool {
    a.enabled == b.enabled && a.device == b.device && a.ip == b.ip
}

fn worker(state: Shared, rx: mpsc::Receiver<Msg>, repaint: impl Fn()) {
    let Ok(Msg::Config(mut cfg)) = rx.recv() else { return };
    drain(&rx, &mut cfg);
    let mut saved = load_cache();
    let mut last_mac = saved.device.as_ref().map(|d| d.mac.clone());
    let mut cached = Some(Cache { device: saved.device.clone(), inputs: saved.inputs.clone() });
    // A search was asked for (Customize's receiver list), and how long to wait before
    // looking again when none was found.
    let mut asked = false;
    let mut retry = Duration::from_secs(10);
    let set = |f: &dyn Fn(&mut AvrState)| {
        if let Ok(mut st) = state.lock() {
            f(&mut st.avr);
        }
        repaint();
    };
    'outer: loop {
        set(&|a| a.protocol = cfg.protocol);
        if !cfg.enabled {
            set(&|a| *a = AvrState::default());
            cached = None;
            match wait(&rx, &mut cfg, None) {
                Wake::Quit => return,
                Wake::Search => asked = true,
                _ => {}
            }
            continue;
        }
        // The last receiver's inputs at once (stale while it is asked again), on start only.
        if let Some(c) = cached.take()
            && let Some(dev) = c.device
            && cfg.ip.as_ref().is_none_or(|t| parse_address(t).is_some_and(|(ip, _)| ip == dev.ip))
            && cfg.device.as_ref().is_none_or(|m| *m == dev.mac)
        {
            let inputs = c.inputs;
            let sources = state.lock().map(|s| s.sources.clone()).unwrap_or_default();
            let tv = tv_input_for(&sources, &dev);
            set(&|a| {
                a.status = Status::Searching;
                a.device = Some(dev.clone());
                a.inputs = inputs.clone();
                a.tv_input = tv.clone();
            });
            // Then what it says now (a fraction of a second), before the slower search.
            if let Ok((r, link)) = report(&dev, cfg.protocol) {
                let inputs: Vec<AvrInput> = r.inputs.into_iter().filter(|i| offered(&i.code)).collect();
                set(&|a| {
                    a.power = r.power;
                    a.input = r.input.clone();
                    a.sound = r.sound.clone();
                    (a.volume, a.muted) = (r.volume, r.muted);
                    if !inputs.is_empty() {
                        a.inputs = inputs.clone();
                    }
                    a.link = Some(link);
                });
            }
        } else {
            set(&|a| a.status = Status::Searching);
        }

        // The TV's inputs (CEC names, to choose) and media servers (where receivers are)
        // come first: wait for them a moment.
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline && !state.lock().map(|s| s.loaded).unwrap_or(true) {
            std::thread::sleep(Duration::from_millis(100));
        }
        let (sources, hints) = state.lock().map(|s| (s.sources.clone(), s.receiver_hints.clone())).unwrap_or_default();

        // Which receiver: the address typed, else one found on the network.
        let device = if let Some(text) = cfg.ip.clone() {
            let typed = parse_address(&text).and_then(|(ip, port)| probe_typed(&ip, port, cfg.protocol));
            // The others too, for Customize's list, only when it asks: the typed one is
            // followed at once.
            if asked {
                let mut found = discover(&hints);
                if let Some(t) = &typed
                    && !found.iter().any(|r| r.ip == t.ip)
                {
                    found.push(t.clone());
                }
                let auto_mac = match choose(&found, &sources, last_mac.as_deref()) {
                    Pick::One(i) => Some(found[i].mac.clone()),
                    _ => None,
                };
                set(&|a| {
                    a.found = found.clone();
                    a.auto_pick = auto_mac.clone();
                });
            } else {
                set(&|a| a.found = typed.iter().cloned().collect());
            }
            typed
        } else {
            // Only where a receiver is likely: a Denon/Marantz input, one known on the
            // network, one used before, or a search asked for. Not on every TV every 10 s.
            let likely = asked || last_mac.is_some() || cfg.device.is_some() || !hints.is_empty() || sources.iter().any(is_receiver_input);
            let found = if likely { discover(&hints) } else { Vec::new() };
            let pick = choose(&found, &sources, last_mac.as_deref());
            let ambiguous = pick == Pick::Ambiguous;
            let auto = match pick {
                Pick::One(i) => Some(found[i].clone()),
                _ => None,
            };
            let picked = match &cfg.device {
                Some(mac) => found.iter().find(|r| r.mac == *mac).cloned(),
                None => auto.clone(),
            };
            let auto_mac = auto.map(|a| a.mac);
            set(&|a| {
                a.found = found.clone();
                a.auto_pick = auto_mac.clone();
                if picked.is_none() {
                    a.status = if ambiguous && cfg.device.is_none() { Status::Ambiguous } else { Status::NotFound };
                }
            });
            if ambiguous && cfg.device.is_none() {
                // The user chooses in Customize: no point searching again by itself.
                retry = Duration::MAX;
            }
            picked
        };
        asked = false;
        let Some(device) = device else {
            set(&|a| {
                a.device = None;
                a.inputs.clear();
                a.tv_input = None;
                a.power = None;
                a.input = None;
                a.sound = None;
                (a.volume, a.muted) = (None, None);
                if a.status == Status::Searching {
                    a.status = Status::NotFound;
                }
            });
            // Wait for a change in Customize or a search; look again now and then (the
            // receiver may be waking up), less and less often.
            let timeout = (retry != Duration::MAX).then_some(retry);
            match wait(&rx, &mut cfg, timeout) {
                Wake::Quit => return,
                Wake::Search => asked = true,
                Wake::Timeout => retry = (retry * 3).min(Duration::from_secs(60)),
                Wake::Config => retry = Duration::from_secs(10),
            }
            continue;
        };
        retry = Duration::from_secs(10);
        last_mac = Some(device.mac.clone());
        // Another receiver than the one shown: nothing of the old one may stay (commands
        // would go to it until the new one answers).
        set(&|a| {
            if a.device.as_ref().map(|d| &d.mac) != Some(&device.mac) {
                a.device = Some(device.clone());
                a.inputs.clear();
                a.power = None;
                a.input = None;
                a.sound = None;
                (a.volume, a.muted) = (None, None);
                a.tv_input = None;
            }
        });

        // Follow it: power, input, names.
        let mut failures = 0;
        loop {
            match report(&device, cfg.protocol) {
                Ok((r, link)) => {
                    failures = 0;
                    let inputs: Vec<AvrInput> = r.inputs.into_iter().filter(|i| offered(&i.code)).collect();
                    let sources = state.lock().map(|s| s.sources.clone()).unwrap_or_default();
                    let tv = tv_input_for(&sources, &device);
                    let mut dev = device.clone();
                    if let Some(n) = r.name {
                        dev.name = n;
                    }
                    dev.power = r.power;
                    // A short answer (telnet cut by its deadline) keeps the inputs known.
                    if !inputs.is_empty() {
                        save_cache(Cache { device: Some(Receiver { power: None, ..dev.clone() }), inputs: inputs.clone() }, &mut saved);
                    }
                    set(&|a| {
                        a.status = Status::Ready;
                        a.device = Some(dev.clone());
                        a.power = r.power;
                        a.input = r.input.clone();
                        a.sound = r.sound.clone();
                        // A volume answer lost once keeps the last one (its buttons stay put).
                        a.volume = r.volume.or(a.volume);
                        a.muted = r.muted.or(a.muted);
                        if !inputs.is_empty() {
                            a.inputs = inputs.clone();
                        }
                        a.tv_input = tv.clone();
                        a.link = Some(link);
                    });
                }
                Err(e) => {
                    failures += 1;
                    if failures == 1 {
                        eprintln!("avr {}: {e}", device.ip);
                    }
                    if failures >= 3 {
                        // Its state is unknown now: the tile no longer shows an input.
                        set(&|a| {
                            a.status = Status::Unreachable;
                            a.power = None;
                        });
                    }
                    if failures >= 5 && cfg.ip.is_none() {
                        // Gone for good, or another address (DHCP): look for it again.
                        continue 'outer;
                    }
                }
            }
            let before = cfg.clone();
            match wait(&rx, &mut cfg, Some(if failures >= 3 { POLL * 3 } else { POLL })) {
                Wake::Quit => return,
                Wake::Search => {
                    asked = true;
                    continue 'outer;
                }
                Wake::Config if !same_target(&before, &cfg) => continue 'outer,
                // Only the protocol changed: keep following the same receiver.
                Wake::Config => set(&|a| a.protocol = cfg.protocol),
                Wake::Timeout => {}
            }
        }
    }
}

/// The receiver in use, its power and the protocol to use; waits a moment for it only
/// while it is being searched for (just after start), not when there is none.
fn current(state: &Shared) -> Option<(Receiver, Option<bool>, Protocol)> {
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        let (d, p, l, searching) = state
            .lock()
            .map(|s| (s.avr.device.clone(), s.avr.power, s.avr.link.unwrap_or(s.avr.protocol), s.avr.status == Status::Searching))
            .unwrap_or_default();
        if let Some(d) = d {
            return Some((d, p, l));
        }
        if !searching || Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Sends a raw command (`MNMEN ON`, `MNCUP`…) to the receiver in use.
pub fn send(state: &Shared, cmd: &str) {
    let Some((device, _, link)) = current(state) else {
        eprintln!("avr: no receiver for {cmd}");
        return;
    };
    // − and + move it by 1.0: two of the receiver's 0.5 steps, a moment apart so the
    // second is not dropped.
    let times = if cmd == VOLUME_UP || cmd == VOLUME_DOWN { 2 } else { 1 };
    for i in 0..times {
        if i > 0 {
            std::thread::sleep(Duration::from_millis(150));
        }
        if let Err(e) = command(&device, link, cmd) {
            eprintln!("avr {cmd}: {e}");
            return;
        }
    }
    // The volume shows at once too.
    if cmd.starts_with("MV") || cmd.starts_with("MU") {
        std::thread::sleep(Duration::from_millis(300));
        if let Ok((volume, muted)) = volume_report(&device, link)
            && let Ok(mut st) = state.lock()
        {
            st.avr.volume = volume.or(st.avr.volume);
            st.avr.muted = muted.or(st.avr.muted);
        }
    }
    // A sound mode change shows at once, not at the next poll.
    if cmd.starts_with("MS") {
        std::thread::sleep(Duration::from_millis(400));
        if let Ok((r, _)) = report(&device, link)
            && let Ok(mut st) = state.lock()
        {
            st.avr.sound = r.sound;
        }
    }
}

/// The setup menu's keys, as the receiver's own remote sends them.
pub const MENU_ON: &str = "MNMEN ON";
pub const MENU_OFF: &str = "MNMEN OFF";
pub const MENU_UP: &str = "MNCUP";
pub const MENU_DOWN: &str = "MNCDN";
pub const MENU_LEFT: &str = "MNCLT";
pub const MENU_RIGHT: &str = "MNCRT";
pub const MENU_ENTER: &str = "MNENT";
pub const MENU_BACK: &str = "MNRTN";

/// Selects input `code` on the receiver, turning it on first if needed. Blocking (a few
/// seconds at most): called from the TV worker, which the app waits for before it exits.
pub fn select(state: &Shared, code: &str) {
    let Some((device, power, link)) = current(state) else {
        eprintln!("avr: no receiver for {code}");
        return;
    };
    if power != Some(true) {
        if let Err(e) = command(&device, link, "ZMON") {
            eprintln!("avr power on: {e}");
            return;
        }
        // It ignores an input change while it wakes up.
        std::thread::sleep(Duration::from_millis(1500));
    }
    // Sent again if it did not take (a receiver still waking up), but given up as soon as
    // it does not answer: the app waits for this before it exits.
    for attempt in 0..3 {
        if let Err(e) = command(&device, link, &format!("SI{code}")) {
            eprintln!("avr input {code}: {e}");
            return;
        }
        std::thread::sleep(Duration::from_millis(700));
        match report(&device, link) {
            Ok((r, _)) if r.input.as_deref().is_some_and(|i| same_code(i, code)) => {
                if let Ok(mut st) = state.lock() {
                    st.avr.input = r.input;
                    st.avr.power = r.power;
                    st.avr.sound = r.sound;
                }
                return;
            }
            Err(e) => {
                eprintln!("avr input {code}: {e}");
                return;
            }
            Ok(_) if attempt < 2 => std::thread::sleep(Duration::from_millis(800)),
            Ok(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons::SourceIcon;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    fn hdmi(id: &str, device: &str, brand: &str, power: Option<bool>) -> Source {
        Source {
            id: id.into(),
            app_id: format!("com.webos.app.{}", id.to_lowercase().replace('_', "")),
            label: id.replace('_', " "),
            kind: SourceKind::Hdmi,
            connected: true,
            device: Some(device.into()),
            tv_icon: SourceIcon::Speaker,
            params: None,
            brand: Some(brand.into()),
            label_custom: false,
            signal: None,
            cec: true,
            cec_power: power,
        }
    }

    fn receiver(ip: &str, model: &str, mac: &str, power: Option<bool>) -> Receiver {
        Receiver { ip: ip.into(), port: 8080, mac: mac.into(), model: model.into(), name: format!("Denon {model}"), brand: "Denon".into(), power }
    }

    #[test]
    fn device_info() {
        let (model, mac, brand) = parse_device_info(&fixture("s670-Deviceinfo.xml")).unwrap();
        assert_eq!((model.as_str(), mac.as_str(), brand.as_str()), ("AVC-S670H", "0006780A1B2C", "Denon"));
        assert_eq!(parse_device_info("<html>nope</html>"), None);
        assert_eq!(parse_device_info("<ModelName></ModelName><BrandCode>1</BrandCode>"), None);
        assert_eq!(parse_device_info("<ModelName>SR6015</ModelName><BrandCode>1</BrandCode>").unwrap().2, "Marantz");
    }

    #[test]
    fn s670_report() {
        // Renamed inputs (trailing spaces trimmed), "GAME1" as GAME, audio inputs kept here.
        let r = parse_report(&fixture("s670-AppCommand.xml"));
        assert_eq!(r.power, Some(true));
        assert_eq!(r.input.as_deref(), Some("TV"));
        assert_eq!(r.name.as_deref(), Some("Denon AVC-S670H"));
        let video: Vec<(&str, &str)> = r.inputs.iter().filter(|i| is_video(&i.code)).map(|i| (i.code.as_str(), i.name.as_str())).collect();
        assert_eq!(
            video,
            [("SAT/CBL", "Freebox Playe"), ("MPLAY", "KM2"), ("BD", "Salon"), ("GAME", "Game"), ("AUX1", "AUX1"), ("AUX2", "AUX2")]
        );
        assert!(r.inputs.iter().any(|i| i.code == "TV" && i.name == "TV Audio"));
        assert!(!r.inputs.iter().any(|i| i.name == "Source"));
    }

    #[test]
    fn s660_report_skips_unused_inputs() {
        let r = parse_report(&fixture("s660-AppCommand.xml"));
        let video: Vec<&str> = r.inputs.iter().filter(|i| is_video(&i.code)).map(|i| i.code.as_str()).collect();
        // CBL/SAT is set "not in use" on this one.
        assert_eq!(video, ["MPLAY", "BD", "GAME", "AUX1", "AUX2"]);
        assert_eq!(r.name.as_deref(), Some("Denon AVC-S660H"));
    }

    #[test]
    fn telnet_answers() {
        // As an AVC-S670H answers PW? ZM? SI? SSFUN ? SSSOD ? NSFRN ?, with events mixed in.
        let text = "PWON\rZMON\rSIGAME\rSVOFF\rSSFUNSAT/CBL Freebox Playe\rSSFUNMPLAY KM2\rSSFUNBD Salon\r\
                    SSFUNGAME Game\rSSFUNTV TV Audio\rSSFUN END\rSSSODSAT/CBL USE\rSSSODMPLAY USE\rSSSODBD DEL\r\
                    SSSODGAME USE\rSSSOD END\rNSFRN Denon AVC-S670H\r";
        let r = parse_telnet(text);
        assert_eq!(r.power, Some(true));
        assert_eq!(r.input.as_deref(), Some("GAME"));
        assert_eq!(r.name.as_deref(), Some("Denon AVC-S670H"));
        let names: Vec<(&str, &str)> = r.inputs.iter().map(|i| (i.code.as_str(), i.name.as_str())).collect();
        assert_eq!(names, [("SAT/CBL", "Freebox Playe"), ("MPLAY", "KM2"), ("GAME", "Game"), ("TV", "TV Audio")]);
        assert_eq!(parse_telnet("PWSTANDBY\r").power, Some(false));
        // The main zone off while the unit is on (another zone playing).
        assert_eq!(parse_telnet("PWON\rZMOFF\r").power, Some(false));
    }

    #[test]
    fn power_and_codes() {
        assert_eq!(parse_report("<rx><cmd><zone1>STANDBY</zone1></cmd></rx>").power, Some(false));
        assert_eq!(parse_report("<rx></rx>"), Report::default());
        assert!(same_code("GAME1", "GAME") && same_code("sat/cbl", "CBL/SAT") && !same_code("BD", "DVD"));
        assert_eq!(code_of("Teapot"), None);
        let state = AvrState {
            power: Some(true),
            input: Some("GAME".into()),
            inputs: vec![AvrInput { code: "GAME".into(), name: "PS5".into() }],
            ..Default::default()
        };
        assert_eq!(state.active_input().map(|i| i.name.as_str()), Some("PS5"));
        assert_eq!(AvrState { power: Some(false), ..state.clone() }.active_input(), None);
        assert_eq!(AvrState { input: Some("TV".into()), ..state }.active_input(), None);
    }

    #[test]
    fn icons() {
        let i = |code: &str, name: &str| icon_for(&AvrInput { code: code.into(), name: name.into() });
        assert_eq!(i("GAME", "Game"), SourceIcon::Console);
        assert_eq!(i("GAME", "PS5"), SourceIcon::PlayStation);
        assert_eq!(i("BD", "Salon"), SourceIcon::Movie);
        assert_eq!(i("SAT/CBL", "Freebox Playe"), SourceIcon::StreamingBox);
        assert_eq!(i("AUX1", "AUX1"), SourceIcon::Cable);
    }

    #[test]
    fn choosing_the_right_receiver() {
        // This house: the S670H in the living room (QNED), the S660H in the bedroom (C1).
        let found = [receiver("192.168.1.58", "AVC-S670H", "A", Some(true)), receiver("192.168.1.70", "AVC-S660H", "B", Some(true))];
        let qned = [hdmi("HDMI_1", "Salon", "Mecool", None), hdmi("HDMI_3", "AVC-S670H", "Denon", Some(true))];
        let c1 = [hdmi("HDMI_2", "AVC-S660H", "Denon", None)];
        assert_eq!(choose(&found, &qned, None), Pick::One(0));
        assert_eq!(choose(&found, &c1, None), Pick::One(1));
        assert_eq!(tv_input_for(&qned, &found[0]).as_deref(), Some("HDMI_3"));
        assert_eq!(tv_input_for(&c1, &found[1]).as_deref(), Some("HDMI_2"));

        // One receiver: it, whatever the TV says.
        assert_eq!(choose(&found[1..], &qned, None), Pick::One(0));
        assert_eq!(choose(&[], &qned, None), Pick::None);

        // Two of the same model: CEC's power state decides, else last time's, else the user.
        let twins = [receiver("192.168.1.58", "AVC-S670H", "A", Some(false)), receiver("192.168.1.59", "AVC-S670H", "B", Some(true))];
        assert_eq!(choose(&twins, &qned, None), Pick::One(1));
        let unknown_power = [hdmi("HDMI_3", "AVC-S670H", "Denon", None)];
        assert_eq!(choose(&twins, &unknown_power, None), Pick::Ambiguous);
        assert_eq!(choose(&twins, &unknown_power, Some("A")), Pick::One(0));
        assert_eq!(choose(&twins, &unknown_power, Some("Z")), Pick::Ambiguous);

        // A CEC name the user changed ("Salon"): no model tells, so still ambiguous.
        let renamed = [hdmi("HDMI_3", "Salon", "Denon", None)];
        assert_eq!(choose(&found, &renamed, None), Pick::Ambiguous);
        // ...but the only Denon input is still the one it is plugged into.
        assert_eq!(tv_input_for(&renamed, &found[0]).as_deref(), Some("HDMI_3"));
        // No Denon input at all: no TV input to switch to.
        assert_eq!(tv_input_for(&qned[..1], &found[0]), None);
    }

    #[test]
    fn addresses() {
        assert_eq!(parse_address(" 192.168.1.58 "), Some(("192.168.1.58".into(), None)));
        assert_eq!(parse_address("192.168.1.58:8080"), Some(("192.168.1.58".into(), Some(8080))));
        for bad in ["", "192.168.1", "192.168.1.300", "denon.local", "192.168.1.58:", "192.168.1.58:0", "1.2.3.4:99999"] {
            assert_eq!(parse_address(bad), None, "{bad}");
        }
    }

    #[test]
    fn sound_modes() {
        // As the S660H and S670H report them (trailing spaces included).
        let r = parse_report("<rx><cmd><surround>Dolby PLII Movie                    </surround></cmd><cmd><zone1>ON</zone1></cmd></rx>");
        assert_eq!(r.sound.as_deref(), Some("Dolby PLII Movie"));
        assert_eq!(r.power, Some(true));
        let group = |m: &str| sound_group(m).map(|i| SOUND_MODES[i]);
        assert_eq!(group("Dolby PLII Movie"), Some("MOVIE"));
        assert_eq!(group("Dolby PLII Music"), Some("MUSIC"));
        assert_eq!(group("DOLBY PLII MS"), Some("MUSIC"));
        assert_eq!(group("DOLBY PLII MV"), Some("MOVIE"));
        assert_eq!(group("Multi Ch Stereo"), Some("STEREO"));
        assert_eq!(group("Pure Direct"), Some("PURE DIRECT"));
        assert_eq!(group("Direct"), Some("DIRECT"));
        assert_eq!(group("Video Game"), Some("GAME"));
        assert_eq!(group("Dolby Digital"), None);
        // Telnet reports it after MS.
        assert_eq!(parse_telnet("MSDOLBY PLII MV\r").sound.as_deref(), Some("DOLBY PLII MV"));
        // TV Audio is offered, the audio-only inputs are not.
        assert!(offered("TV") && offered("GAME") && !offered("BT") && !offered("NET"));
        let st = AvrState {
            power: Some(true),
            input: Some("TV".into()),
            inputs: vec![AvrInput { code: "TV".into(), name: "TV Audio".into() }],
            ..Default::default()
        };
        // Selected, but not a picture shown through the input.
        assert_eq!(st.selected_input().map(|i| i.name.as_str()), Some("TV Audio"));
        assert_eq!(st.active_input(), None);
    }

    #[test]
    fn status_and_names_merged() {
        // The two answers of one poll, as the S660H gives them, read as one report.
        let status = "<rx><cmd><zone1>ON</zone1></cmd><cmd><zone1><source>MPLAY</source></zone1></cmd>\
                      <cmd><surround>Dolby PLII Movie   </surround></cmd><cmd><friendlyname>Denon AVC-S660H</friendlyname></cmd></rx>";
        // The names answer: the capture's rename and delete lists only.
        let full = fixture("s660-AppCommand.xml");
        let names = &full[full.find("<cmd>\n<functionrename>").unwrap()..full.find("<cmd>\n<friendlyname>").unwrap()];
        let r = parse_report(&format!("{status}<rx>{names}</rx>"));
        assert_eq!((r.power, r.input.as_deref(), r.sound.as_deref()), (Some(true), Some("MPLAY"), Some("Dolby PLII Movie")));
        assert!(r.inputs.iter().any(|i| i.code == "MPLAY" && i.name == "Media Player"));
    }

    #[test]
    fn volume_answers() {
        // As the S660H answers: no zone, -34.0 dB shown as 46.0.
        assert_eq!(parse_volume(&fixture("s660-Volume.xml")), (Some(46.0), Some(false)));
        // As others are documented to: dB relative to the reference, per zone.
        let xml = "<rx><cmd><zone1><volume>-34.5</volume></zone1><zone2><volume>-40.0</volume></zone2></cmd>\
                   <cmd><zone1><mute>on</mute></zone1><zone2><mute>off</mute></zone2></cmd></rx>";
        assert_eq!(parse_volume(xml), (Some(45.5), Some(true)));
        assert_eq!(parse_volume("<rx><cmd><zone1><mute>off</mute></zone1></cmd></rx>"), (None, Some(false)));
        // Nonsense stays unknown rather than a level.
        assert_eq!(parse_volume("<rx><cmd><zone1><volume>--</volume></zone1></cmd></rx>"), (None, None));
        assert_eq!(parse_volume("<rx><cmd><zone1><volume>40.0</volume></zone1></cmd></rx>"), (None, None));
        assert_eq!(parse_volume("<error>1</error>"), (None, None));
        // The volume answer's zone1 is not power: it never goes through parse_report.
        assert_eq!(parse_report(&fixture("s660-AppCommand.xml")).volume, None);

        // Over telnet: two digits, or three for a half step; MVMAX is the limit, not the level.
        assert_eq!(parse_telnet("MV46\rMVMAX 88\rMUOFF\r").volume, Some(46.0)); // the S660H's answer
        let r = parse_telnet("PWON\rMV455\rMVMAX 98\rMUOFF\r");
        assert_eq!((r.volume, r.muted, r.power), (Some(45.5), Some(false), Some(true)));
        assert_eq!(parse_telnet("MVMAX 98\rMV45\r").volume, Some(45.0));
        assert_eq!(parse_telnet("MV05\rMUON\r").volume, Some(5.0));
        assert_eq!(parse_telnet("MUON\r").muted, Some(true));
        assert_eq!(parse_telnet("MVMAX 98\r").volume, None);
        assert_eq!(parse_telnet("MV995\rMV1\rMVxy\r").volume, None);
    }

    #[test]
    fn command_paths() {
        assert_eq!(command_path(MENU_ON), "/goform/formiPhoneAppDirect.xml?MNMEN%20ON");
        assert_eq!(command_path("SISAT/CBL"), "/goform/formiPhoneAppDirect.xml?SISAT/CBL");
    }

    #[test]
    fn chunked_bodies() {
        assert_eq!(unchunk("5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n"), "hello world");
        assert_eq!(unchunk("zz\r\n"), "");
        // A size that falls inside a character must not panic.
        assert_eq!(unchunk("1\r\néa"), "éa");
    }
}
