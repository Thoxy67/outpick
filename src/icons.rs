//! Lucide icons (ISC), recoloured to white so `Image::tint` gives any colour,
//! as Photocraft's `icons.rs` does with its SVG set.

use serde::{Deserialize, Serialize};

macro_rules! icons {
    ($($name:literal),* $(,)?) => {
        const ICONS: &[(&str, &str)] = &[$(($name, include_str!(concat!("../assets/icons/", $name, ".svg")))),*];
    };
}

icons!(
    "tv", "monitor", "gamepad-2", "speaker", "cast", "usb", "box", "radio-tower", "film", "laptop",
    "house", "cable", "settings", "check", "chevron-left", "chevron-right", "eye", "eye-off",
    "palette", "layout-grid", "list", "rows-3", "arrow-up", "arrow-down", "rotate-ccw", "sun",
    "moon", "type", "maximize-2", "timer", "circle-check", "pencil", "server", "hard-drive", "power", "info", "menu",
);

/// Registers every icon with egui's loaders (`bytes://icon/<name>.svg`).
pub fn install(ctx: &egui::Context) {
    egui_extras::install_image_loaders(ctx);
    for (name, svg) in ICONS {
        let white = svg.replace("currentColor", "#ffffff");
        ctx.include_bytes(format!("bytes://icon/{name}.svg"), white.into_bytes());
    }
}

pub fn uri(name: &str) -> String {
    format!("bytes://icon/{name}.svg")
}

/// Paints icon `name` into `rect`, tinted, through `p` (so it obeys the
/// painter's clip rect, unlike `Image::paint_at`).
pub fn paint(p: &egui::Painter, name: &str, rect: egui::Rect, tint: egui::Color32) {
    let ctx = p.ctx();
    let image = egui::Image::from_uri(uri(name)).fit_to_exact_size(rect.size());
    match image.load_for_size(ctx, rect.size()) {
        Ok(egui::load::TexturePoll::Ready { texture }) => {
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            p.image(texture.id, rect, uv, tint);
        }
        Ok(egui::load::TexturePoll::Pending { .. }) => ctx.request_repaint(),
        Err(_) => {}
    }
}

/// What a source tile shows. `Auto` follows what the TV reports for the input.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SourceIcon {
    #[default]
    Auto,
    Tv,
    Monitor,
    StreamingBox,
    Console,
    Computer,
    Speaker,
    Cast,
    Movie,
    Antenna,
    Usb,
    Home,
    Cable,
    Server,
    Drive,
}

impl SourceIcon {
    pub const ALL: [SourceIcon; 15] = [
        SourceIcon::Auto,
        SourceIcon::Tv,
        SourceIcon::Monitor,
        SourceIcon::StreamingBox,
        SourceIcon::Console,
        SourceIcon::Computer,
        SourceIcon::Speaker,
        SourceIcon::Cast,
        SourceIcon::Movie,
        SourceIcon::Antenna,
        SourceIcon::Usb,
        SourceIcon::Home,
        SourceIcon::Cable,
        SourceIcon::Server,
        SourceIcon::Drive,
    ];

    pub fn svg(self) -> &'static str {
        match self {
            SourceIcon::Auto | SourceIcon::Cable => "cable",
            SourceIcon::Tv => "tv",
            SourceIcon::Monitor => "monitor",
            SourceIcon::StreamingBox => "box",
            SourceIcon::Console => "gamepad-2",
            SourceIcon::Computer => "laptop",
            SourceIcon::Speaker => "speaker",
            SourceIcon::Cast => "cast",
            SourceIcon::Movie => "film",
            SourceIcon::Antenna => "radio-tower",
            SourceIcon::Usb => "usb",
            SourceIcon::Home => "house",
            SourceIcon::Server => "server",
            SourceIcon::Drive => "hard-drive",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SourceIcon::Auto => crate::i18n::t("icon.auto"),
            SourceIcon::Tv => crate::i18n::t("icon.tv"),
            SourceIcon::Monitor => crate::i18n::t("icon.monitor"),
            SourceIcon::StreamingBox => crate::i18n::t("icon.streaming_box"),
            SourceIcon::Console => crate::i18n::t("icon.console"),
            SourceIcon::Computer => crate::i18n::t("icon.computer"),
            SourceIcon::Speaker => crate::i18n::t("icon.speaker"),
            SourceIcon::Cast => crate::i18n::t("icon.cast"),
            SourceIcon::Movie => crate::i18n::t("icon.movie"),
            SourceIcon::Antenna => crate::i18n::t("icon.antenna"),
            SourceIcon::Usb => crate::i18n::t("icon.usb"),
            SourceIcon::Home => crate::i18n::t("icon.home"),
            SourceIcon::Cable => crate::i18n::t("icon.cable"),
            SourceIcon::Server => crate::i18n::t("icon.server"),
            SourceIcon::Drive => crate::i18n::t("icon.drive"),
        }
    }
}
