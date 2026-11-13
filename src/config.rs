//! User settings: one JSON file, every field optional, edited from the overlay
//! itself (Customize) or by hand.

use crate::i18n::{Language, t};
use crate::icons::SourceIcon;
use crate::theme::{Accent, ThemeKind};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Layout {
    /// One row of large tiles.
    #[default]
    Row,
    /// Tiles in a grid.
    Grid,
    /// A vertical list in a floating panel.
    List,
    /// A full-height vertical list docked to the left or right edge.
    Sidebar,
}

impl Layout {
    pub const ALL: [Layout; 4] = [Layout::Row, Layout::Grid, Layout::List, Layout::Sidebar];
    pub fn label(self) -> &'static str {
        match self {
            Layout::Row => t("layout.row"),
            Layout::Grid => t("layout.grid"),
            Layout::List => t("layout.list"),
            Layout::Sidebar => t("layout.sidebar"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Position {
    #[default]
    Bottom,
    Center,
    Top,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Position {
    pub const ALL: [Position; 9] = [
        Position::Bottom,
        Position::BottomLeft,
        Position::BottomRight,
        Position::Center,
        Position::Left,
        Position::Right,
        Position::Top,
        Position::TopLeft,
        Position::TopRight,
    ];

    /// Which side a docked sidebar uses (left for any left-hand position).
    pub fn is_left(self) -> bool {
        matches!(self, Position::Left | Position::TopLeft | Position::BottomLeft)
    }
    pub fn label(self) -> &'static str {
        match self {
            Position::Bottom => t("position.bottom"),
            Position::Center => t("position.center"),
            Position::Top => t("position.top"),
            Position::Left => t("position.left"),
            Position::Right => t("position.right"),
            Position::TopLeft => t("position.top_left"),
            Position::TopRight => t("position.top_right"),
            Position::BottomLeft => t("position.bottom_left"),
            Position::BottomRight => t("position.bottom_right"),
        }
    }
}

/// What an input is called.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Naming {
    /// What is plugged in: "Denon AVC-S670H" (a label set in LG's settings still wins).
    #[default]
    Device,
    /// The TV's own label ("Denon Barre de son", "HDMI 1").
    TvLabel,
}

impl Naming {
    pub const ALL: [Naming; 2] = [Naming::Device, Naming::TvLabel];
    pub fn label(self) -> &'static str {
        match self {
            Naming::Device => t("naming.device"),
            Naming::TvLabel => t("naming.tv"),
        }
    }
}

/// Corner shape of panels and tiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Corners {
    /// As the theme draws them.
    #[default]
    Theme,
    Square,
    Round,
}

impl Corners {
    pub const ALL: [Corners; 3] = [Corners::Theme, Corners::Square, Corners::Round];
    pub fn label(self) -> &'static str {
        match self {
            Corners::Theme => t("corners.theme"),
            Corners::Square => t("corners.square"),
            Corners::Round => t("corners.round"),
        }
    }
    /// Multiplier on the theme's radii.
    pub fn factor(self) -> f32 {
        match self {
            Corners::Theme => 1.0,
            Corners::Square => 0.3,
            Corners::Round => 1.8,
        }
    }
}

/// The look of the on-screen clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ClockStyle {
    /// Large digits with a soft shadow.
    #[default]
    Minimal,
    /// The time in a pill of the theme's colours.
    Pill,
    /// The time over the day and date.
    Stacked,
    /// A dial with hands.
    Analog,
    /// Each digit on its own tile, as a flip clock.
    Tiles,
    /// LED seven-segment digits in the accent colour, unlit segments faint.
    SevenSegment,
    /// OnePlus-like: bold hours in the accent colour over white minutes.
    OnePlus,
}

impl ClockStyle {
    pub const ALL: [ClockStyle; 7] = [
        ClockStyle::Minimal,
        ClockStyle::Pill,
        ClockStyle::Stacked,
        ClockStyle::Analog,
        ClockStyle::Tiles,
        ClockStyle::SevenSegment,
        ClockStyle::OnePlus,
    ];
    pub fn label(self) -> &'static str {
        match self {
            ClockStyle::Minimal => t("clock.minimal"),
            ClockStyle::Pill => t("clock.pill"),
            ClockStyle::Stacked => t("clock.stacked"),
            ClockStyle::Analog => t("clock.analog"),
            ClockStyle::Tiles => t("clock.tiles"),
            ClockStyle::SevenSegment => t("clock.seven_segment"),
            ClockStyle::OnePlus => t("clock.oneplus"),
        }
    }
}

/// The on-screen clock: where (centre, as fractions of the screen), how big, which format.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ClockConfig {
    pub show: bool,
    pub style: ClockStyle,
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub hour24: bool,
    pub seconds: bool,
}

impl Default for ClockConfig {
    fn default() -> Self {
        ClockConfig { show: false, style: ClockStyle::Minimal, x: 0.9, y: 0.09, size: 1.0, hour24: true, seconds: false }
    }
}

/// Per-source overrides, keyed by `Source::id`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct SourceOverride {
    pub id: String,
    /// Name shown instead of the TV's label.
    pub name: Option<String>,
    pub icon: SourceIcon,
    pub hidden: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    /// File format version (1; files without one are version 1 too).
    pub version: u32,
    pub language: Language,
    pub theme: ThemeKind,
    pub accent: Accent,
    pub layout: Layout,
    pub position: Position,
    pub corners: Corners,
    /// Icons on the tiles and rows.
    pub show_icons: bool,
    /// The title line ("Inputs" and what is on).
    pub show_header: bool,
    /// The key hints at the bottom.
    pub show_hints: bool,
    /// Size of everything, 0.8 .. 1.5.
    pub scale: f32,
    /// The panel behind the tiles (off: only the tiles, title and hints).
    pub show_panel: bool,
    /// Panel opacity, 0.4 .. 1.0 (the picture shows through below 1).
    pub panel_opacity: f32,
    /// How much the picture behind is darkened, 0 .. 0.8.
    pub dim: f32,
    /// How inputs are named.
    pub naming: Naming,
    /// Pictures of the sources in their tiles (live for the one in front).
    pub show_previews: bool,
    /// Live picture rate while the picker is open (1, 2 or 4 per second).
    pub preview_fps: u32,
    pub show_disconnected: bool,
    pub show_device_names: bool,
    pub show_live_tv: bool,
    /// A Home entry (the home in use) when not already on it.
    pub show_home: bool,
    /// USB sticks and disks (opened in LG's media player).
    pub show_usb: bool,
    /// DLNA/UPnP media servers on the network.
    pub show_media_servers: bool,
    /// Show the Customize tile in the picker. Without it the settings are still reached with
    /// the arrow that has nothing to do in the layout (Up in a row or on a grid's first row,
    /// Left in a list or sidebar); the footer then says so.
    pub show_customize_tile: bool,
    /// Close by itself after this many seconds without a key (0 = never).
    pub auto_close_secs: u32,
    /// Source pressed again in the picker moves to the next input; it is taken
    /// this many seconds after the last press (the arrows never start this).
    pub switch_delay_secs: u32,
    /// The remote's Source/Input key opens the picker (with the key takeover on).
    pub source_key_opens: bool,
    /// The remote's Multikey (accessibility) button opens the picker too, for
    /// remotes with no Source button (with the Source key takeover on).
    pub multikey_opens: bool,
    /// Animations (selection glide, fade).
    pub animations: bool,
    /// Source order by id; sources not listed follow in the TV's order.
    pub order: Vec<String>,
    pub sources: Vec<SourceOverride>,
    pub clock: ClockConfig,
    /// The Denon/Marantz receiver on the network.
    pub avr: crate::denon::AvrConfig,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            version: 1,
            language: Language::Auto,
            theme: ThemeKind::Studio,
            accent: Accent::Theme,
            layout: Layout::Row,
            position: Position::Bottom,
            corners: Corners::Theme,
            show_icons: true,
            show_header: true,
            show_hints: true,
            scale: 1.0,
            show_panel: true,
            panel_opacity: 0.92,
            dim: 0.35,
            naming: Naming::Device,
            show_previews: true,
            preview_fps: 2,
            show_disconnected: true,
            show_device_names: true,
            show_live_tv: true,
            show_home: true,
            show_usb: true,
            show_media_servers: false,
            show_customize_tile: true,
            auto_close_secs: 15,
            switch_delay_secs: 3,
            animations: true,
            source_key_opens: true,
            multikey_opens: false,
            order: Vec::new(),
            sources: Vec::new(),
            clock: ClockConfig::default(),
            avr: crate::denon::AvrConfig::default(),
        }
    }
}

impl Config {
    /// `$OUTPICK_CONFIG`, else /media/developer/outpick/config.json on
    /// the TV (writable by the jailed app and its root helper, kept across
    /// reinstalls), else the desktop config dir.
    pub fn path() -> PathBuf {
        if let Some(p) = std::env::var_os("OUTPICK_CONFIG") {
            return p.into();
        }
        if cfg!(feature = "webos") {
            return "/media/developer/outpick/config.json".into();
        }
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| ".".into());
        base.join("outpick/config.json")
    }

    pub fn load() -> Self {
        let path = Self::path();
        // Before the Outpick rename the settings lived in /media/developer/lg-c1-source.
        let legacy = || {
            (cfg!(feature = "webos") && std::env::var_os("OUTPICK_CONFIG").is_none())
                .then(|| std::fs::read("/media/developer/lg-c1-source/config.json").ok())
                .flatten()
        };
        let Some(bytes) = std::fs::read(&path).ok().or_else(legacy) else {
            return Config::default();
        };
        match Self::from_json(&bytes) {
            Ok(c) => c,
            Err(e) => {
                // Unreadable as a whole: keep the user's file aside, since the next save
                // replaces it.
                let bad = path.with_extension("json.bad");
                let kept = std::fs::write(&bad, &bytes).map(|()| bad.display().to_string());
                eprintln!("config: {e}; using defaults (file kept as {kept:?})");
                Config::default()
            }
        }
    }

    /// A settings file, leniently: a value that does not fit (a typo, an unknown theme, a
    /// wrong type) falls back to its default alone, and the rest is kept. Fails only when
    /// the file is not a JSON object at all.
    pub fn from_json(bytes: &[u8]) -> Result<Config, String> {
        let v: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        let Value::Object(user) = v else { return Err("not a JSON object".into()) };
        let kept = serde_json::to_vec(&lenient::<Config>(user, "")).map_err(|e| e.to_string())?;
        let mut c: Config = serde_json::from_slice(&kept).unwrap_or_default();
        c.clamp();
        Ok(c)
    }

    pub fn save(&self) {
        if let Err(e) = self.try_save() {
            #[cfg(feature = "webos")]
            {
                // Most likely the folder was created by root (the takeover helper)
                // without write access for the jailed app: have root open it, then
                // retry. Inline (a fraction of a second, once): the process may exit
                // right after this save.
                let fixed = crate::webos::takeover::open_dir_as_root();
                if let Err(e2) = self.try_save() {
                    eprintln!("config: {e}; after the permission fix ({fixed:?}): {e2}");
                }
                return;
            }
            #[cfg(not(feature = "webos"))]
            eprintln!("config: {e}");
        }
    }

    fn try_save(&self) -> Result<(), String> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o1777));
            }
        }
        let tmp = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(&self.pruned()).map_err(|e| e.to_string())?;
        {
            use std::io::Write;
            // On disk before the rename, so a power cut leaves the old file or the new one.
            let err = |e: std::io::Error| format!("{}: {e}", tmp.display());
            let mut f = std::fs::File::create(&tmp).map_err(err)?;
            f.write_all(&bytes).map_err(err)?;
            f.sync_all().map_err(err)?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o666));
        }
        std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// As saved: without per-source entries that change nothing, and without empty or
    /// repeated ids in the order.
    fn pruned(&self) -> Config {
        let mut c = self.clone();
        c.sources.retain(|o| {
            o.name.as_deref().is_some_and(|n| !n.trim().is_empty()) || o.icon != SourceIcon::Auto || o.hidden
        });
        let mut seen: Vec<String> = Vec::new();
        c.order.retain(|id| {
            let new = !id.is_empty() && !seen.contains(id);
            if new {
                seen.push(id.clone());
            }
            new
        });
        c
    }

    pub fn clamp(&mut self) {
        self.version = 1;
        // The picker offers 1, 2 and 4 pictures a second: a hand-set value snaps to the nearest.
        self.preview_fps = match self.preview_fps {
            0..=1 => 1,
            2..=3 => 2,
            _ => 4,
        };
        self.scale = self.scale.clamp(0.8, 1.5);
        self.panel_opacity = self.panel_opacity.clamp(0.4, 1.0);
        self.dim = self.dim.clamp(0.0, 0.8);
        self.auto_close_secs = self.auto_close_secs.min(120);
        self.switch_delay_secs = self.switch_delay_secs.clamp(1, 10);
        self.clock.x = self.clock.x.clamp(0.0, 1.0);
        self.clock.y = self.clock.y.clamp(0.0, 1.0);
        self.clock.size = self.clock.size.clamp(0.4, 3.0);
    }

    pub fn source(&self, id: &str) -> Option<&SourceOverride> {
        self.sources.iter().find(|s| s.id == id)
    }

    pub fn source_mut(&mut self, id: &str) -> &mut SourceOverride {
        if let Some(i) = self.sources.iter().position(|s| s.id == id) {
            return &mut self.sources[i];
        }
        self.sources.push(SourceOverride { id: id.into(), ..Default::default() });
        self.sources.last_mut().unwrap()
    }
}

/// `user` key by key, keeping only the keys that still let a `T` be read (a missing key
/// takes its default, as `T` is `#[serde(default)]`; `path` names the object in the log).
/// The arrays `order` and `sources` lose only their bad elements, and `clock` is taken key
/// by key the same way.
fn lenient<T: DeserializeOwned>(user: Map<String, Value>, path: &str) -> Value {
    let mut out = Map::new();
    for (k, v) in user {
        let v = match (path, k.as_str(), v) {
            ("", "clock", Value::Object(m)) => lenient::<ClockConfig>(m, "clock."),
            ("", "avr", Value::Object(m)) => lenient::<crate::denon::AvrConfig>(m, "avr."),
            ("", "order", Value::Array(a)) => elements::<String>(a, "order"),
            ("", "sources", Value::Array(a)) => elements::<SourceOverride>(a, "sources"),
            (_, _, v) => v,
        };
        out.insert(k.clone(), v);
        if !reads_as::<T>(&out) {
            eprintln!("config: {path}{k}: invalid value, using the default");
            out.remove(&k);
        }
    }
    Value::Object(out)
}

/// Whether `v` reads as a `T`. Through the text, not `T::deserialize(&Value)`: the TV build
/// already holds the text deserializers of the settings types, and a second set reading
/// from `Value` would cost it some 7 kB.
fn reads_as<T: DeserializeOwned>(v: &impl Serialize) -> bool {
    serde_json::to_vec(v).is_ok_and(|b| serde_json::from_slice::<T>(&b).is_ok())
}

/// The elements of an array that read as a `T`.
fn elements<T: DeserializeOwned>(a: Vec<Value>, name: &str) -> Value {
    let n = a.len();
    let kept: Vec<Value> = a.into_iter().filter(|v| reads_as::<T>(v)).collect();
    if kept.len() < n {
        eprintln!("config: {name}: {} invalid entries dropped", n - kept.len());
    }
    Value::Array(kept)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The settings file of the LG C1, as an earlier build wrote it.
    const C1_LEGACY: &str = r#"{"language":"auto","theme":"studio","accent":"theme","layout":"row","position":"bottom","corners":"theme","showIcons":true,"showHeader":true,"showHints":true,"scale":1.0,"panelOpacity":0.92,"dim":0.35,"showDisconnected":true,"showDeviceNames":true,"showLiveTv":true,"showHome":true,"showUsb":true,"showMediaServers":false,"showCustomizeTile":true,"autoCloseSecs":15,"autoSwitchSecs":0,"animations":true,"order":[],"sources":[]}"#;
    /// The QNED's, hand-written.
    const QNED_LEGACY: &str = r#"{"multikeyOpens": true}"#;

    #[test]
    fn legacy_files_load() {
        let c1 = Config::from_json(C1_LEGACY.as_bytes()).unwrap();
        assert_eq!(c1, Config::default());
        let qned = Config::from_json(QNED_LEGACY.as_bytes()).unwrap();
        assert_eq!(qned, Config { multikey_opens: true, ..Config::default() });
    }

    #[test]
    fn one_bad_value_keeps_the_rest() {
        let c = Config::from_json(
            br#"{"theme":"no-such-theme","layout":"grid","scale":"big","dim":0.5,
                 "clock":{"show":true,"style":"sundial","x":0.25},
                 "order":["HDMI_2",7,"HDMI_1"],
                 "sources":[{"id":"HDMI_1","name":"Box","icon":"teapot"},{"id":"HDMI_2","hidden":true},3]}"#,
        )
        .unwrap();
        let d = Config::default();
        assert_eq!(c.theme, d.theme);
        assert_eq!(c.scale, d.scale);
        assert_eq!(c.layout, Layout::Grid);
        assert_eq!(c.dim, 0.5);
        assert!(c.clock.show);
        assert_eq!(c.clock.style, ClockStyle::Minimal);
        assert_eq!(c.clock.x, 0.25);
        assert_eq!(c.order, ["HDMI_2", "HDMI_1"]);
        assert_eq!(c.sources, [SourceOverride { id: "HDMI_2".into(), hidden: true, ..Default::default() }]);
        // A wrong type for an object or an array falls back as a whole.
        let c = Config::from_json(br#"{"clock":5,"sources":{"a":1},"showHome":false}"#).unwrap();
        assert_eq!(c, Config { show_home: false, ..Config::default() });
    }

    #[test]
    fn not_an_object_is_an_error() {
        assert!(Config::from_json(b"{\"theme\": \"studio\",").is_err());
        assert!(Config::from_json(b"[1, 2]").is_err());
        assert!(Config::from_json(b"").is_err());
    }

    #[test]
    fn clamp_bounds_everything() {
        let mut c = Config {
            version: 7,
            scale: 9.0,
            panel_opacity: 0.0,
            dim: -1.0,
            auto_close_secs: 999,
            switch_delay_secs: 0,
            preview_fps: 3,
            clock: ClockConfig { x: -2.0, y: 4.0, size: 10.0, ..Default::default() },
            ..Config::default()
        };
        c.clamp();
        assert_eq!(c.version, 1);
        assert_eq!((c.scale, c.panel_opacity, c.dim), (1.5, 0.4, 0.0));
        assert_eq!((c.auto_close_secs, c.switch_delay_secs), (120, 1));
        assert_eq!((c.clock.x, c.clock.y, c.clock.size), (0.0, 1.0, 3.0));
        for (fps, want) in [(0, 1), (1, 1), (2, 2), (3, 2), (4, 4), (60, 4)] {
            c.preview_fps = fps;
            c.clamp();
            assert_eq!(c.preview_fps, want, "{fps} fps");
        }
    }

    #[test]
    fn pruned_drops_what_changes_nothing() {
        let mut c = Config { order: vec!["HDMI_1".into(), "".into(), "HDMI_2".into(), "HDMI_1".into()], ..Config::default() };
        c.source_mut("HDMI_1").name = Some("Box".into());
        c.source_mut("HDMI_2").name = Some("  ".into());
        c.source_mut("HDMI_3").hidden = true;
        c.source_mut("HDMI_4").icon = SourceIcon::Pc;
        let _ = c.source_mut("HOME");
        let p = c.pruned();
        assert_eq!(p.order, ["HDMI_1", "HDMI_2"]);
        let ids: Vec<&str> = p.sources.iter().map(|o| o.id.as_str()).collect();
        assert_eq!(ids, ["HDMI_1", "HDMI_3", "HDMI_4"]);
    }

    /// Everything that goes through $OUTPICK_CONFIG, in one test so nothing races on it.
    #[test]
    fn save_load_on_disk() {
        let dir = std::env::temp_dir().join(format!("outpick-config-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("config.json");
        // SAFETY: the only test that reads or writes the environment.
        unsafe { std::env::set_var("OUTPICK_CONFIG", &path) };

        // No file: defaults.
        assert_eq!(Config::load(), Config::default());

        // Round trip, with the version written.
        let mut c = Config {
            theme: ThemeKind::ALL[1],
            layout: Layout::Sidebar,
            preview_fps: 4,
            order: vec!["HDMI_2".into()],
            ..Config::default()
        };
        c.source_mut("HDMI_2").name = Some("Console".into());
        c.clock.show = true;
        c.save();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"version\": 1"), "{text}");
        assert!(!dir.join("config.json.tmp").exists());
        assert_eq!(Config::load(), c);

        // A syntax error: defaults, and the file is kept aside.
        let broken = b"{\"theme\": \"studio\", oops";
        std::fs::write(&path, broken).unwrap();
        assert_eq!(Config::load(), Config::default());
        assert_eq!(std::fs::read(dir.join("config.json.bad")).unwrap(), broken);

        unsafe { std::env::remove_var("OUTPICK_CONFIG") };
        let _ = std::fs::remove_dir_all(&dir);
    }
}
