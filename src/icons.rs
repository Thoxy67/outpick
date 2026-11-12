//! Lucide icons (ISC). `build.rs` rasterizes every `assets/icons/*.svg` into a
//! 96 px alpha mask at build time, so no SVG renderer ships in the binary;
//! here the masks become white textures that `paint` tints, as Photocraft's
//! `icons.rs` tints its recoloured SVGs.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

include!(concat!(env!("OUT_DIR"), "/icons.rs"));

/// The context textures are made in (`install`), and those made so far:
/// each icon is uploaded the first time it is painted, not all before the first frame.
static CONTEXT: OnceLock<egui::Context> = OnceLock::new();
static TEXTURES: OnceLock<Mutex<HashMap<&'static str, egui::TextureHandle>>> = OnceLock::new();

fn unrle(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(ICON_PX * ICON_PX);
    for pair in data.as_chunks::<2>().0 {
        out.extend(std::iter::repeat_n(pair[1], pair[0] as usize));
    }
    out.resize(ICON_PX * ICON_PX, 0);
    out
}

/// Remembers the context icons are uploaded to (once each, on first use:
/// white, alpha from the mask, mipmapped so small sizes stay smooth).
pub fn install(ctx: &egui::Context) {
    let _ = CONTEXT.set(ctx.clone());
}

fn texture(name: &str) -> Option<egui::TextureId> {
    let mut textures = TEXTURES.get_or_init(Default::default).lock().ok()?;
    if let Some(tex) = textures.get(name) {
        return Some(tex.id());
    }
    let (name, rle) = ICONS.iter().find(|(n, _)| *n == name)?;
    let options = egui::TextureOptions { mipmap_mode: Some(egui::TextureFilter::Linear), ..egui::TextureOptions::LINEAR };
    let pixels = unrle(rle).into_iter().map(egui::Color32::from_white_alpha).collect();
    let image = egui::ColorImage { size: [ICON_PX, ICON_PX], source_size: egui::vec2(ICON_PX as f32, ICON_PX as f32), pixels };
    let tex = CONTEXT.get()?.load_texture(format!("icon-{name}"), image, options);
    let id = tex.id();
    textures.insert(*name, tex);
    Some(id)
}

/// Paints icon `name` into `rect`, tinted, through `p` (so it obeys the
/// painter's clip rect).
pub fn paint(p: &egui::Painter, name: &str, rect: egui::Rect, tint: egui::Color32) {
    if let Some(tex) = texture(name) {
        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        p.image(tex, rect, uv, tint);
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
    PlayStation,
    Xbox,
    NintendoSwitch,
    Android,
    Pc,
}

impl SourceIcon {
    pub const ALL: [SourceIcon; 20] = [
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
        SourceIcon::PlayStation,
        SourceIcon::Xbox,
        SourceIcon::NintendoSwitch,
        SourceIcon::Android,
        SourceIcon::Pc,
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
            SourceIcon::PlayStation => "playstation",
            SourceIcon::Xbox => "xbox",
            SourceIcon::NintendoSwitch => "nintendo-switch",
            SourceIcon::Android => "android",
            SourceIcon::Pc => "pc-case",
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
            SourceIcon::PlayStation => "PlayStation",
            SourceIcon::Xbox => "Xbox",
            SourceIcon::NintendoSwitch => "Nintendo Switch",
            SourceIcon::Android => crate::i18n::t("icon.android"),
            SourceIcon::Pc => crate::i18n::t("icon.pc"),
        }
    }
}
