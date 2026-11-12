//! Colour/shape tokens and egui style, ported from Photocraft's theme system
//! (`photocraft/crates/ui-egui/src/theme.rs`): the same `Tokens` model, the same
//! palettes, Inter in three weights. Sizes here are for a 10-foot UI: the
//! picker multiplies every length by the user's size setting.

use crate::i18n::t;
use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ThemeKind {
    #[default]
    Studio,
    /// Studio on true black, kind to an OLED panel at night.
    Oled,
    SoftDark,
    AdwaitaDark,
    BreezeDark,
    SolarizedDark,
    StudioLight,
    SoftLight,
}

impl ThemeKind {
    pub const ALL: [ThemeKind; 8] = [
        ThemeKind::Studio,
        ThemeKind::Oled,
        ThemeKind::SoftDark,
        ThemeKind::AdwaitaDark,
        ThemeKind::BreezeDark,
        ThemeKind::SolarizedDark,
        ThemeKind::StudioLight,
        ThemeKind::SoftLight,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ThemeKind::Studio => t("theme.studio"),
            ThemeKind::Oled => t("theme.oled"),
            ThemeKind::SoftDark => t("theme.soft_dark"),
            ThemeKind::AdwaitaDark => t("theme.adwaita_dark"),
            ThemeKind::BreezeDark => t("theme.breeze_dark"),
            ThemeKind::SolarizedDark => t("theme.solarized"),
            ThemeKind::StudioLight => t("theme.studio_light"),
            ThemeKind::SoftLight => t("theme.soft_light"),
        }
    }
}

/// Accent colour: the theme's own, or one of a few presets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Accent {
    #[default]
    Theme,
    Violet,
    Blue,
    Cyan,
    Teal,
    Green,
    Amber,
    Orange,
    Rose,
    Red,
    White,
}

impl Accent {
    pub const ALL: [Accent; 11] = [
        Accent::Theme,
        Accent::Violet,
        Accent::Blue,
        Accent::Cyan,
        Accent::Teal,
        Accent::Green,
        Accent::Amber,
        Accent::Orange,
        Accent::Rose,
        Accent::Red,
        Accent::White,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Accent::Theme => t("accent.theme"),
            Accent::Violet => t("accent.violet"),
            Accent::Blue => t("accent.blue"),
            Accent::Cyan => t("accent.cyan"),
            Accent::Teal => t("accent.teal"),
            Accent::Green => t("accent.green"),
            Accent::Amber => t("accent.amber"),
            Accent::Orange => t("accent.orange"),
            Accent::Rose => t("accent.rose"),
            Accent::Red => t("accent.red"),
            Accent::White => t("accent.white"),
        }
    }

    pub fn rgb(self) -> Option<Color32> {
        Some(match self {
            Accent::Theme => return None,
            Accent::Violet => Color32::from_rgb(139, 124, 246),
            Accent::Blue => Color32::from_rgb(55, 142, 240),
            Accent::Cyan => Color32::from_rgb(34, 190, 230),
            Accent::Teal => Color32::from_rgb(32, 196, 160),
            Accent::Green => Color32::from_rgb(79, 184, 107),
            Accent::Amber => Color32::from_rgb(240, 190, 90),
            Accent::Orange => Color32::from_rgb(245, 140, 60),
            Accent::Rose => Color32::from_rgb(240, 110, 160),
            Accent::Red => Color32::from_rgb(240, 96, 96),
            Accent::White => Color32::from_rgb(240, 240, 244),
        })
    }
}

/// Colour and shape tokens (Photocraft's `Tokens`, minus the editor-only fields).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub dark: bool,
    pub chrome: Color32,
    pub card: Color32,
    pub card_border: Color32,
    pub field: Color32,
    pub field_border: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_faint: Color32,
    pub icon: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub accent_border: Color32,
    pub accent_text: Color32,
    pub separator: Color32,
    pub shadow: Color32,
    pub primary_bg: Color32,
    pub primary_text: Color32,
    pub success: Color32,
    pub danger: Color32,
    pub warning: Color32,
    pub radius_sm: f32,
    pub radius: f32,
    pub radius_lg: f32,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

impl Tokens {
    pub fn new(kind: ThemeKind, accent: Accent) -> Self {
        let mut t = Self::for_kind(kind);
        if let Some(a) = accent.rgb() {
            t.accent = a;
            t.accent_soft = Color32::from_rgba_unmultiplied(a.r(), a.g(), a.b(), if t.dark { 46 } else { 36 });
            t.accent_border = Color32::from_rgba_unmultiplied(a.r(), a.g(), a.b(), 130);
            t.accent_text = if t.dark { mix(a, Color32::WHITE, 0.55) } else { mix(a, Color32::BLACK, 0.35) };
        }
        t
    }

    fn for_kind(kind: ThemeKind) -> Self {
        match kind {
            ThemeKind::Studio => Tokens {
                dark: true,
                chrome: rgb(20, 20, 21),
                card: rgb(26, 26, 28),
                card_border: rgb(40, 40, 44),
                field: rgb(35, 35, 38),
                field_border: rgb(52, 52, 57),
                hover: rgb(44, 44, 48),
                pressed: rgb(56, 56, 62),
                text: rgb(236, 236, 240),
                text_dim: rgb(150, 150, 158),
                text_faint: rgb(96, 96, 104),
                icon: rgb(196, 196, 204),
                accent: rgb(139, 124, 246),
                accent_soft: Color32::from_rgba_unmultiplied(139, 124, 246, 46),
                accent_border: Color32::from_rgba_unmultiplied(160, 148, 255, 110),
                accent_text: rgb(214, 208, 255),
                separator: rgb(38, 38, 42),
                shadow: Color32::from_black_alpha(140),
                primary_bg: rgb(246, 246, 248),
                primary_text: rgb(12, 12, 14),
                success: rgb(79, 184, 107),
                danger: rgb(240, 96, 96),
                warning: rgb(240, 190, 90),
                radius_sm: 6.0,
                radius: 8.0,
                radius_lg: 12.0,
            },
            ThemeKind::Oled => Tokens {
                chrome: rgb(0, 0, 0),
                card: rgb(8, 8, 9),
                card_border: rgb(30, 30, 33),
                field: rgb(18, 18, 20),
                field_border: rgb(40, 40, 44),
                hover: rgb(30, 30, 33),
                pressed: rgb(42, 42, 46),
                ..Self::for_kind(ThemeKind::Studio)
            },
            ThemeKind::SoftDark => Tokens {
                dark: true,
                chrome: rgb(58, 58, 68),
                card: rgb(54, 54, 64),
                card_border: rgb(70, 70, 82),
                field: rgb(64, 64, 75),
                field_border: rgb(84, 84, 98),
                hover: rgb(68, 68, 80),
                pressed: rgb(74, 74, 88),
                text: rgb(238, 238, 246),
                text_dim: rgb(212, 212, 224),
                text_faint: rgb(190, 190, 206),
                icon: rgb(214, 214, 226),
                accent: rgb(138, 154, 232),
                accent_soft: Color32::from_rgba_unmultiplied(138, 154, 232, 48),
                accent_border: Color32::from_rgba_unmultiplied(138, 154, 232, 150),
                accent_text: rgb(190, 200, 252),
                separator: rgb(36, 36, 44),
                shadow: Color32::from_black_alpha(110),
                primary_bg: rgb(86, 104, 196),
                primary_text: Color32::WHITE,
                success: rgb(120, 210, 150),
                danger: rgb(250, 160, 172),
                warning: rgb(240, 200, 130),
                radius_sm: 7.0,
                radius: 10.0,
                radius_lg: 14.0,
            },
            ThemeKind::AdwaitaDark => Tokens {
                dark: true,
                chrome: rgb(46, 46, 50),
                card: rgb(54, 54, 58),
                card_border: rgb(64, 64, 68),
                field: rgb(64, 64, 68),
                field_border: rgb(76, 76, 80),
                hover: rgb(72, 72, 76),
                pressed: rgb(86, 86, 90),
                text: Color32::WHITE,
                text_dim: rgb(192, 192, 194),
                text_faint: rgb(145, 145, 148),
                icon: rgb(235, 235, 237),
                accent: rgb(53, 132, 228),
                accent_soft: Color32::from_rgba_unmultiplied(53, 132, 228, 60),
                accent_border: Color32::from_rgba_unmultiplied(120, 174, 237, 140),
                accent_text: rgb(120, 174, 237),
                separator: rgb(24, 24, 27),
                shadow: Color32::from_black_alpha(140),
                primary_bg: rgb(53, 132, 228),
                primary_text: Color32::WHITE,
                success: rgb(87, 227, 137),
                danger: rgb(255, 123, 99),
                warning: rgb(248, 228, 92),
                radius_sm: 6.0,
                radius: 9.0,
                radius_lg: 12.0,
            },
            ThemeKind::BreezeDark => Tokens {
                dark: true,
                chrome: rgb(38, 42, 48),
                card: rgb(29, 33, 38),
                card_border: rgb(52, 56, 61),
                field: rgb(38, 42, 48),
                field_border: rgb(68, 73, 79),
                hover: rgb(50, 54, 59),
                pressed: rgb(58, 63, 69),
                text: rgb(250, 251, 253),
                text_dim: rgb(200, 206, 213),
                text_faint: rgb(164, 170, 177),
                icon: rgb(236, 238, 240),
                accent: rgb(58, 172, 232),
                accent_soft: Color32::from_rgba_unmultiplied(58, 172, 232, 40),
                accent_border: Color32::from_rgba_unmultiplied(58, 172, 232, 160),
                accent_text: rgb(80, 184, 238),
                separator: rgb(46, 50, 54),
                shadow: Color32::from_black_alpha(150),
                primary_bg: rgb(30, 118, 176),
                primary_text: Color32::WHITE,
                success: rgb(39, 174, 96),
                danger: rgb(227, 111, 123),
                warning: rgb(244, 120, 6),
                radius_sm: 5.0,
                radius: 5.0,
                radius_lg: 6.0,
            },
            ThemeKind::SolarizedDark => Tokens {
                dark: true,
                chrome: rgb(7, 54, 66),
                card: rgb(7, 54, 66),
                card_border: rgb(0, 37, 47),
                field: rgb(0, 37, 47),
                field_border: rgb(21, 80, 95),
                hover: rgb(14, 74, 90),
                pressed: rgb(18, 89, 107),
                text: rgb(147, 161, 161),
                text_dim: rgb(101, 123, 131),
                text_faint: rgb(88, 110, 117),
                icon: rgb(147, 161, 161),
                accent: rgb(38, 139, 210),
                accent_soft: Color32::from_rgba_unmultiplied(38, 139, 210, 46),
                accent_border: Color32::from_rgba_unmultiplied(38, 139, 210, 120),
                accent_text: rgb(253, 246, 227),
                separator: rgb(0, 37, 47),
                shadow: Color32::from_black_alpha(150),
                primary_bg: rgb(38, 139, 210),
                primary_text: rgb(253, 246, 227),
                success: rgb(133, 153, 0),
                danger: rgb(220, 50, 47),
                warning: rgb(181, 137, 0),
                radius_sm: 6.0,
                radius: 8.0,
                radius_lg: 12.0,
            },
            ThemeKind::StudioLight => Tokens {
                dark: false,
                chrome: rgb(246, 246, 248),
                card: rgb(252, 252, 253),
                card_border: rgb(222, 222, 228),
                field: rgb(242, 242, 245),
                field_border: rgb(214, 214, 220),
                hover: rgb(232, 232, 237),
                pressed: rgb(220, 220, 226),
                text: rgb(24, 24, 28),
                text_dim: rgb(96, 96, 106),
                text_faint: rgb(150, 150, 160),
                icon: rgb(60, 60, 68),
                accent: rgb(108, 92, 231),
                accent_soft: Color32::from_rgba_unmultiplied(108, 92, 231, 36),
                accent_border: Color32::from_rgba_unmultiplied(108, 92, 231, 120),
                accent_text: rgb(80, 64, 200),
                separator: rgb(226, 226, 232),
                shadow: Color32::from_black_alpha(50),
                primary_bg: rgb(20, 20, 24),
                primary_text: rgb(250, 250, 252),
                success: rgb(30, 140, 70),
                danger: rgb(210, 60, 60),
                warning: rgb(190, 130, 20),
                radius_sm: 6.0,
                radius: 8.0,
                radius_lg: 12.0,
            },
            ThemeKind::SoftLight => Tokens {
                dark: false,
                chrome: rgb(232, 230, 236),
                card: rgb(240, 238, 243),
                card_border: rgb(208, 205, 215),
                field: rgb(250, 249, 252),
                field_border: rgb(190, 187, 201),
                hover: rgb(228, 226, 234),
                pressed: rgb(212, 209, 221),
                text: rgb(44, 42, 56),
                text_dim: rgb(62, 60, 76),
                text_faint: rgb(80, 78, 96),
                icon: rgb(64, 62, 80),
                accent: rgb(110, 128, 216),
                accent_soft: Color32::from_rgba_unmultiplied(110, 128, 216, 44),
                accent_border: Color32::from_rgba_unmultiplied(110, 128, 216, 170),
                accent_text: rgb(46, 58, 146),
                separator: rgb(210, 207, 218),
                shadow: Color32::from_black_alpha(40),
                primary_bg: rgb(80, 98, 190),
                primary_text: Color32::WHITE,
                success: rgb(30, 130, 70),
                danger: rgb(166, 44, 62),
                warning: rgb(140, 72, 0),
                radius_sm: 7.0,
                radius: 10.0,
                radius_lg: 14.0,
            },
        }
    }
}

/// Linear blend of two colours (term's `look::mix`).
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

/// The colour at `alpha` (0..1) opacity.
pub fn veil(c: Color32, alpha: f32) -> Color32 {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    Color32::from_rgba_unmultiplied(r, g, b, (a as f32 * alpha.clamp(0.0, 1.0)).round() as u8)
}

/// Text colour that reads on `fill` (term's `look::ink_on`).
pub fn ink_on(fill: Color32) -> Color32 {
    let l = 0.2126 * fill.r() as f32 + 0.7152 * fill.g() as f32 + 0.0722 * fill.b() as f32;
    if l > 150.0 { rgb(14, 14, 16) } else { Color32::WHITE }
}

pub const MEDIUM: &str = "medium";
pub const SEMIBOLD: &str = "semibold";

pub fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}
pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MEDIUM.into()))
}
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

/// Inter first in every family, egui's defaults kept behind it for symbols and emoji.
pub fn install_fonts(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();
    let fonts: [(&str, &'static [u8]); 3] = [
        ("Inter-Regular", include_bytes!("../assets/fonts/Inter-Regular.ttf")),
        ("Inter-Medium", include_bytes!("../assets/fonts/Inter-Medium.ttf")),
        ("Inter-SemiBold", include_bytes!("../assets/fonts/Inter-SemiBold.ttf")),
    ];
    for (name, bytes) in fonts {
        defs.font_data.insert(name.into(), Arc::new(FontData::from_static(bytes)));
    }
    let fallback: Vec<String> = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let family = |first: &str| {
        let mut v = vec![first.to_string()];
        v.extend(fallback.iter().cloned());
        v
    };
    defs.families.insert(FontFamily::Proportional, family("Inter-Regular"));
    defs.families.insert(FontFamily::Name(MEDIUM.into()), family("Inter-Medium"));
    defs.families.insert(FontFamily::Name(SEMIBOLD.into()), family("Inter-SemiBold"));
    ctx.set_fonts(defs);
}

/// egui's own style follows the tokens too (for any stock widget we use).
pub fn apply(ctx: &egui::Context, t: &Tokens) {
    let mut v = if t.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = Color32::TRANSPARENT;
    v.window_fill = t.card;
    v.window_stroke = egui::Stroke::new(1.0, t.card_border);
    v.extreme_bg_color = t.field;
    v.override_text_color = Some(t.text);
    v.selection.bg_fill = t.accent_soft;
    v.selection.stroke = egui::Stroke::new(1.0, t.accent);
    ctx.set_visuals(v);
}
