//! Shared drawing: the panel, its header and key hints, animation, and small helpers.

use super::{App, Hit, HintKey, Screen};
use crate::config::{Config, Layout, Position};
use crate::i18n::t;
use crate::theme::{Tokens, regular, semibold, veil};
use egui::{Align2, Color32, CornerRadius, Mesh, Pos2, Rect, Shadow, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};

impl App {
    pub(super) fn panel_rect(&self, screen: Rect, size: Vec2) -> Rect {
        if let Some(left) = self.docked_left() {
            let m = 24.0 * self.cfg.scale;
            let w = size.x.min(screen.width() - 2.0 * m);
            let x = if left { screen.min.x + m } else { screen.max.x - m - w };
            return Rect::from_min_max(pos2(x, screen.min.y + m), pos2(x + w, screen.max.y - m));
        }
        let m = 56.0 * self.cfg.scale;
        let size = size.min(screen.size() - Vec2::splat(2.0 * m));
        let c = screen.center();
        let min = match self.cfg.position {
            Position::Bottom => pos2(c.x - size.x / 2.0, screen.max.y - m - size.y),
            Position::Top => pos2(c.x - size.x / 2.0, screen.min.y + m),
            Position::Center => c - size / 2.0,
            Position::Left => pos2(screen.min.x + m, c.y - size.y / 2.0),
            Position::Right => pos2(screen.max.x - m - size.x, c.y - size.y / 2.0),
            Position::TopLeft => pos2(screen.min.x + m, screen.min.y + m),
            Position::TopRight => pos2(screen.max.x - m - size.x, screen.min.y + m),
            Position::BottomLeft => pos2(screen.min.x + m, screen.max.y - m - size.y),
            Position::BottomRight => pos2(screen.max.x - m - size.x, screen.max.y - m - size.y),
        };
        Rect::from_min_size(min, size)
    }

    pub(super) fn panel(&self, p: &egui::Painter, rect: Rect, alpha: f32) {
        self.add_hit(rect, Hit::Panel);
        // Without the panel background only the tiles, title and hints show (the
        // settings screens keep theirs, for legibility).
        if !self.cfg.show_panel && self.screen == Screen::Picker {
            return;
        }
        let s = self.cfg.scale;
        let k = &self.t;
        let r = k.radius_lg * 2.0 * s;
        let shadow = Shadow { offset: [0, (14.0 * s) as i8], blur: (48.0 * s).min(255.0) as u8, spread: 0, color: veil(k.shadow, alpha) };
        p.add(shadow.as_shape(rect, CornerRadius::same(r as u8)));
        let fill = veil(k.card, self.cfg.panel_opacity * alpha);
        p.rect(rect, r, fill, Stroke::new(1.0 * s, veil(k.card_border, alpha)), StrokeKind::Inside);
    }

    /// The picker drawn without its panel: its texts sit on the picture, as the clock's do.
    fn bare(&self) -> bool {
        !self.cfg.show_panel && self.screen == Screen::Picker
    }

    /// Text on the panel, or shadowed white on the picture when there is no panel.
    #[allow(clippy::too_many_arguments)]
    fn label(&self, p: &egui::Painter, at: Pos2, anchor: Align2, text: &str, font: egui::FontId, color: Color32, dim: bool, alpha: f32) -> Rect {
        if self.bare() {
            let white = if dim { Color32::from_white_alpha(210) } else { Color32::WHITE };
            crate::clock::shadowed(p, at, anchor, text, font, white, alpha)
        } else {
            p.text(at, anchor, text, font, veil(color, alpha))
        }
    }

    pub(super) fn header(&self, p: &egui::Painter, rect: Rect, title: &str, subtitle: &str, alpha: f32) {
        let s = self.cfg.scale;
        let k = &self.t;
        let pad = 28.0 * s;
        // Title and subtitle share one centre line.
        let yc = rect.min.y + pad + 19.0 * s;
        let tw = text_width(p, title, semibold(30.0 * s));
        self.label(p, pos2(rect.min.x + pad, yc), Align2::LEFT_CENTER, title, semibold(30.0 * s), k.text, false, alpha);
        if !subtitle.is_empty() {
            let room = rect.width() - pad * 2.0 - tw - 24.0 * s;
            let sub = fit(p, subtitle, regular(18.0 * s), room);
            self.label(p, pos2(rect.max.x - pad, yc), Align2::RIGHT_CENTER, &sub, regular(18.0 * s), k.text_dim, true, alpha);
        }
    }

    /// Width the key hints take at `zoom` (1 = normal size), from the left padding on.
    pub(super) fn footer_width(&self, p: &egui::Painter, hints: &[(&str, &str)], zoom: f32) -> f32 {
        let s = self.cfg.scale * zoom;
        let mut w = 0.0;
        for (key, label) in hints {
            w += text_width(p, key, semibold(15.0 * s)).max(18.0 * s) + 16.0 * s;
            w += 10.0 * s + text_width(p, label, regular(17.0 * s)) + 26.0 * s;
        }
        w - 26.0 * s
    }

    /// The key hints along the panel's bottom. They always stay inside it: when they do not
    /// fit, the arrow hints (the obvious ones) go first, then the text gets a little smaller.
    pub(super) fn footer(&self, p: &egui::Painter, rect: Rect, hints: &[(&str, &str)], alpha: f32) {
        let k = &self.t;
        let room = rect.width() - 2.0 * 28.0 * self.cfg.scale;
        let mut hints: Vec<(&str, &str)> = hints.to_vec();
        while self.footer_width(p, &hints, 1.0) > room {
            let Some(i) = hints.iter().position(|(key, _)| key.contains('◀') || key.contains('▲')) else { break };
            hints.remove(i);
        }
        let width = self.footer_width(p, &hints, 1.0);
        let zoom = if width > room { (room / width).max(0.7) } else { 1.0 };
        let s = self.cfg.scale * zoom;
        let y = rect.max.y - 26.0 * self.cfg.scale - 16.0 * self.cfg.scale + (1.0 - zoom) * 15.0 * self.cfg.scale;
        let mut x = rect.min.x + 28.0 * self.cfg.scale;
        let clip = p.with_clip_rect(rect.shrink(4.0));
        for (key, label) in &hints {
            let kf = semibold(15.0 * s);
            let kw = text_width(p, key, kf.clone()).max(18.0 * s) + 16.0 * s;
            let cap = Rect::from_min_size(pos2(x, y), vec2(kw, 30.0 * s));
            let nav = if *key == t("key.ok") {
                Some(HintKey::Ok)
            } else if *key == t("key.back") {
                Some(HintKey::Back)
            } else if *label == t("hint.cec_options") {
                Some(HintKey::Options)
            } else {
                None
            };
            // Without the panel the key caps are solid, so they read over any picture.
            let cap_fill = if self.bare() { k.card } else { k.field };
            clip.rect(cap, 7.0 * s, veil(cap_fill, alpha), Stroke::new(1.0, veil(k.field_border, alpha)), StrokeKind::Inside);
            clip.text(cap.center(), Align2::CENTER_CENTER, *key, kf, veil(k.text, alpha));
            x = cap.max.x + 10.0 * s;
            let r = self.label(&clip, pos2(x, cap.center().y), Align2::LEFT_CENTER, label, regular(17.0 * s), k.text_dim, true, alpha);
            if let Some(nav) = nav {
                self.add_hit(cap.union(r).expand(6.0 * s), Hit::Key(nav));
            }
            x = r.max.x + 26.0 * s;
        }
    }

    /// The selected row of a list (Customize, CEC options): a soft accent background with an
    /// accent ring. The Sources editor keeps its own single shape: drawing it this way moves a
    /// couple of anti-aliased pixels.
    pub(super) fn selected_row(&self, p: &egui::Painter, r: Rect, alpha: f32) {
        let s = self.cfg.scale;
        let k = &self.t;
        p.rect_filled(r, k.radius * 1.5 * s, veil(k.accent_soft, alpha));
        p.rect_stroke(r, k.radius * 1.5 * s, Stroke::new(2.0 * s, veil(k.accent, alpha)), StrokeKind::Inside);
    }

    pub(super) fn anim(&self, ui: &Ui, name: &str, target: f32, secs: f32) -> f32 {
        let secs = if self.cfg.animations && !self.snap { secs } else { 0.0 };
        ui.ctx().animate_value_with_time(egui::Id::unique(("lg-source-anim", name)), target, secs)
    }
}

/// The theme's tokens with the user's corner shape applied.
pub(super) fn tokens(cfg: &Config) -> Tokens {
    let mut t = Tokens::new(cfg.theme, cfg.accent);
    let f = cfg.corners.factor();
    t.radius_sm *= f;
    t.radius *= f;
    t.radius_lg *= f;
    t
}

/// Vertical layouts move with Up/Down.
pub(super) fn vertical(l: Layout) -> bool {
    matches!(l, Layout::List | Layout::Sidebar)
}

pub(super) fn grid_cols(n: usize) -> usize {
    match n {
        0..=4 => n.max(1),
        5..=6 => 3,
        _ => 4,
    }
}

pub(super) fn ease(t: f32) -> f32 {
    let x = t.clamp(0.0, 1.0);
    1.0 - (1.0 - x).powi(3)
}

pub(super) fn cycle<T: Copy + PartialEq>(all: &[T], cur: T, dir: i32) -> T {
    let i = all.iter().position(|&v| v == cur).unwrap_or(0) as i32;
    let n = all.len() as i32;
    all[((i + dir).rem_euclid(n)) as usize]
}

pub(super) fn step(v: f32, by: f32, dir: i32) -> f32 {
    ((v + by * dir as f32) / by).round() * by
}

/// First row to show so `sel` stays in view (term's `look::keep_visible`).
pub(super) fn keep_visible(sel: usize, visible: usize, total: usize) -> usize {
    if total <= visible { 0 } else { sel.saturating_sub(visible / 2).min(total - visible) }
}

pub(super) fn text_width(p: &egui::Painter, text: &str, font: egui::FontId) -> f32 {
    p.layout_no_wrap(text.to_string(), font, Color32::WHITE).size().x
}

/// `text` on one line if it fits, else split at a space onto two balanced lines
/// (the second shortened with an ellipsis if no split fits).
pub(super) fn two_lines(p: &egui::Painter, text: &str, font: egui::FontId, max: f32) -> Vec<String> {
    if text_width(p, text, font.clone()) <= max {
        return vec![text.to_string()];
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    // Among the cuts where both lines fit, the most balanced one (the shorter longest line).
    let balanced = (1..words.len())
        .map(|cut| (words[..cut].join(" "), words[cut..].join(" ")))
        .map(|(a, b)| {
            let w = text_width(p, &a, font.clone()).max(text_width(p, &b, font.clone()));
            (w, a, b)
        })
        .filter(|(w, ..)| *w <= max)
        .min_by(|x, y| x.0.total_cmp(&y.0));
    if let Some((_, a, b)) = balanced {
        return vec![a, b];
    }
    // Else the longest first line that fits; the rest goes below, shortened.
    for cut in (1..words.len()).rev() {
        let first = words[..cut].join(" ");
        if text_width(p, &first, font.clone()) <= max {
            let rest = words[cut..].join(" ");
            return vec![first, fit(p, &rest, font, max)];
        }
    }
    vec![fit(p, text, font, max)]
}

/// `text`, shortened with an ellipsis to fit `max` points.
pub(super) fn fit(p: &egui::Painter, text: &str, font: egui::FontId, max: f32) -> String {
    if text_width(p, text, font.clone()) <= max {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut n = chars.len();
    while n > 1 {
        n -= 1;
        let s = chars[..n].iter().collect::<String>().trim_end().to_string() + "…";
        if text_width(p, &s, font.clone()) <= max {
            return s;
        }
    }
    "…".into()
}

/// A two-colour gradient over `rect`: vertical (`a` top → `b` bottom) or horizontal.
pub(super) fn gradient(p: &egui::Painter, rect: Rect, a: Color32, b: Color32, vertical: bool) {
    let mut mesh = Mesh::default();
    let corners = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()];
    let colors = if vertical { [a, a, b, b] } else { [a, b, b, a] };
    for (pos, c) in corners.into_iter().zip(colors) {
        mesh.colored_vertex(pos, c);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(egui::Shape::mesh(mesh));
}
