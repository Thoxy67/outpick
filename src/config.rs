//! User settings: one JSON file, every field optional, edited from the overlay
//! itself (Customize) or by hand.

use crate::i18n::{Language, t};
use crate::icons::SourceIcon;
use crate::theme::{Accent, ThemeKind};
use serde::{Deserialize, Serialize};
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
    /// Size of everything, 0.7 .. 1.5.
    pub scale: f32,
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
    /// Show the Customize tile in the picker (settings stay reachable with the Source key held).
    pub show_customize_tile: bool,
    /// Close by itself after this many seconds without a key (0 = never).
    pub auto_close_secs: u32,
    /// Switch to the highlighted source this many seconds after the last key,
    /// as LG's picker does when Source is pressed repeatedly (0 = only on OK).
    pub auto_switch_secs: u32,
    /// Animations (selection glide, fade).
    pub animations: bool,
    /// Source order by id; sources not listed follow in the TV's order.
    pub order: Vec<String>,
    pub sources: Vec<SourceOverride>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
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
            auto_switch_secs: 0,
            animations: true,
            order: Vec::new(),
            sources: Vec::new(),
        }
    }
}

impl Config {
    /// `$LG_SOURCE_CONFIG`, else /media/developer/lg-c1-source/config.json on
    /// the TV (writable by the jailed app and its root helper, kept across
    /// reinstalls), else the desktop config dir.
    pub fn path() -> PathBuf {
        if let Some(p) = std::env::var_os("LG_SOURCE_CONFIG") {
            return p.into();
        }
        if cfg!(feature = "webos") {
            return "/media/developer/lg-c1-source/config.json".into();
        }
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| ".".into());
        base.join("lg-c1-source/config.json")
    }

    pub fn load() -> Self {
        let mut c: Config = std::fs::read(Self::path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).map_err(|e| eprintln!("config: {e}; using defaults")).ok())
            .unwrap_or_default();
        c.clamp();
        c
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
                let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o777));
            }
        }
        let tmp = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o666));
        }
        std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn clamp(&mut self) {
        self.scale = self.scale.clamp(0.7, 1.5);
        self.panel_opacity = self.panel_opacity.clamp(0.4, 1.0);
        self.dim = self.dim.clamp(0.0, 0.8);
        self.auto_close_secs = self.auto_close_secs.min(120);
        self.auto_switch_secs = self.auto_switch_secs.min(10);
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
