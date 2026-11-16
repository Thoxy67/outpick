//! Shared drawing: the panel, its header and key hints, animation, and small helpers.

use super::{App, Hit, HintKey, Screen};
use crate::config::{Config, Layout, Position};
use crate::i18n::t;
use crate::icons;
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
        if !self.cfg.show_panel && self.picker_shown() {
            return;
        }
        let s = self.cfg.scale;
        let k = &self.t;
        let r = k.radius_lg * 2.0 * s;
        let shadow = Shadow { offset: [0, (14.0 * s) as i8], blur: (48.0 * s).min(255.0) as u8, spread: 0, color: veil(k.shadow, alpha) };
        p.add(shadow.as_shape(rect, CornerRadius::same(r as u8)));
        let fill = veil(k.card, self.cfg.panel_opacity * alpha);
        p.rect(rect, r, fill, self.border(1.0 * s, veil(k.card_border, alpha)), StrokeKind::Inside);
        self.rim(p, rect, r, alpha);
    }

    /// Glass only: light caught by a shape's edge, one thin line just inside its rounded
    /// outline, brightest along the top, fading down the sides, a fainter return along the
    /// bottom. `alpha` 1 for the panel, less for tiles and buttons; small shapes get less.
    pub(super) fn rim(&self, p: &egui::Painter, rect: Rect, radius: f32, alpha: f32) {
        if !self.t.glass {
            return;
        }
        let s = self.cfg.scale;
        let w = (1.25 * s).max(1.0);
        // A chip must not look heavier than a tile.
        let peak = 170.0 * alpha.clamp(0.0, 1.0) * (rect.height() / (110.0 * s)).clamp(0.45, 1.0);
        let mut points = Vec::new();
        let inner = rect.shrink(w / 2.0);
        egui::epaint::tessellator::path::rounded_rectangle(&mut points, egui::epaint::RoundedRect::new(inner, (radius - w / 2.0).max(0.0)));
        let stroke = egui::epaint::PathStroke::new_uv(w, move |b, pos| {
            let v = ((pos.y - b.min.y) / b.height().max(1.0)).clamp(0.0, 1.0);
            let light = 0.16 + 0.84 * (1.0 - v).powf(2.5) + 0.28 * v.powi(6);
            Color32::from_white_alpha((peak * light).min(255.0) as u8)
        });
        p.add(egui::epaint::PathShape::closed_line(points, stroke));
    }

    /// A shape's uniform border; none on glass, where its rim is the one edge.
    pub(super) fn border(&self, width: f32, color: Color32) -> Stroke {
        if self.t.glass { Stroke::NONE } else { Stroke::new(width, color) }
    }

    /// The picker is what is drawn: on its own screen, or as Customize's miniature.
    fn picker_shown(&self) -> bool {
        self.screen == Screen::Picker || self.mini
    }

    /// The picker drawn without its panel: its texts sit on the picture, as the clock's do.
    fn bare(&self) -> bool {
        !self.cfg.show_panel && self.picker_shown()
    }

    /// Text on the panel, or shadowed white on the picture when there is no panel.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn label(&self, p: &egui::Painter, at: Pos2, anchor: Align2, text: &str, font: egui::FontId, color: Color32, dim: bool, alpha: f32) -> Rect {
        if self.bare() {
            let white = if dim { Color32::from_white_alpha(210) } else { Color32::WHITE };
            crate::clock::shadowed(p, at, anchor, text, font, white, alpha)
        } else {
            p.text(at, anchor, text, font, veil(color, alpha))
        }
    }

    /// The title (after `icon`, if any) and on the right a subtitle.
    pub(super) fn header(&self, p: &egui::Painter, rect: Rect, icon: Option<&str>, title: &str, subtitle: &str, alpha: f32) {
        let s = self.cfg.scale;
        let k = &self.t;
        let pad = 28.0 * s;
        // Title and subtitle share one centre line.
        let yc = rect.min.y + pad + 19.0 * s;
        let mut x = rect.min.x + pad;
        if let Some(icon) = icon {
            let size = 30.0 * s;
            let ink = if self.bare() { Color32::WHITE } else { k.accent_text };
            icons::paint(p, icon, Rect::from_center_size(pos2(x + size / 2.0, yc), Vec2::splat(size)), veil(ink, alpha));
            x += size + 14.0 * s;
        }
        let tw = text_width(p, title, semibold(30.0 * s)) + (x - rect.min.x - pad);
        self.label(p, pos2(x, yc), Align2::LEFT_CENTER, title, semibold(30.0 * s), k.text, false, alpha);
        if !subtitle.is_empty() {
            let room = rect.width() - pad * 2.0 - tw - 24.0 * s;
            let sub = fit(p, subtitle, regular(18.0 * s), room);
            self.label(p, pos2(rect.max.x - pad, yc), Align2::RIGHT_CENTER, &sub, regular(18.0 * s), k.text_dim, true, alpha);
        }
    }

    /// What the selection does, under a list: a hairline across `band` (the `HELP_H` reserved
    /// under the rows), an info icon and a sentence on two lines at most.
    pub(super) fn help_line(&self, p: &egui::Painter, band: Rect, help: &str, alpha: f32) {
        let s = self.cfg.scale;
        let k = &self.t;
        let help_r = Rect::from_min_max(pos2(band.min.x + 20.0 * s, band.min.y + 8.0 * s), pos2(band.max.x - 20.0 * s, band.max.y));
        p.rect_filled(Rect::from_min_size(pos2(band.min.x + 14.0 * s, help_r.min.y), vec2(band.width() - 28.0 * s, 1.0 * s)), 0.0, veil(k.separator, alpha));
        let ir = Rect::from_center_size(pos2(help_r.min.x + 9.0 * s, help_r.min.y + 22.0 * s), Vec2::splat(18.0 * s));
        icons::paint(p, "info", ir, veil(k.text_faint, alpha));
        let text_x = help_r.min.x + 28.0 * s;
        let galley = p.layout(help.to_string(), regular(16.5 * s), veil(k.text_dim, alpha), help_r.max.x - text_x);
        p.with_clip_rect(help_r).galley(pos2(text_x, help_r.min.y + 11.0 * s), galley, veil(k.text_dim, alpha));
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
            } else if *label == t("hint.options") {
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

    /// The selected row of a list (Customize, CEC options): a soft accent background with a
    /// ring. The Sources editor keeps its own single shape: drawing it this way moves a
    /// couple of anti-aliased pixels.
    pub(super) fn selected_row(&self, p: &egui::Painter, r: Rect, alpha: f32) {
        let s = self.cfg.scale;
        let k = &self.t;
        p.rect_filled(r, k.radius * 1.5 * s, veil(k.accent_soft, alpha));
        p.rect_stroke(r, k.radius * 1.5 * s, Stroke::new(2.0 * s, veil(k.ring, alpha)), StrokeKind::Inside);
    }

    pub(super) fn anim(&self, ui: &Ui, name: &str, target: f32, secs: f32) -> f32 {
        let secs = if self.cfg.animations && !self.snap { secs } else { 0.0 };
        // The miniature's own: its glides never move the real picker's.
        ui.ctx().animate_value_with_time(egui::Id::unique(("lg-source-anim", self.mini, name)), target, secs)
    }
}

/// The glass rim of tiles, buttons and cards, against the panel's.
pub(super) const RIM: f32 = 0.6;

/// Height kept under a list for `App::help_line`.
pub(super) const HELP_H: f32 = 66.0;

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

/// The entry after (`dir` 1) or before (-1) `sel` among `n`: around the end on a press,
/// stopping there while the arrow is `held` (a held arrow must not fly past the last row
/// back to the first).
pub(super) fn walk(sel: usize, n: usize, dir: i32, held: bool) -> usize {
    if n == 0 {
        return 0;
    }
    let (to, last) = (sel as i32 + dir, n as i32 - 1);
    (if held { to.clamp(0, last) } else { to.rem_euclid(n as i32) }) as usize
}

/// The next (`dir` 1) or previous (-1) value of a sorted list of numbers, wrapping around.
/// A value not in the list (set by hand in config.json) steps from where it falls in it:
/// 7 in [5, 10, 15] goes to 10 or 5, not to the first entry.
pub(super) fn cycle_num(all: &[u32], cur: u32, dir: i32) -> u32 {
    let n = all.len();
    if n == 0 {
        return cur;
    }
    if let Some(i) = all.iter().position(|&v| v == cur) {
        return all[(i as i32 + dir).rem_euclid(n as i32) as usize];
    }
    // Between all[below - 1] and all[below]: up takes the next larger, down the next smaller.
    let below = all.iter().take_while(|&&v| v < cur).count();
    if dir > 0 {
        all[below % n]
    } else if dir < 0 {
        all[(below + n - 1) % n]
    } else {
        // No step: the nearest value.
        *all.iter().min_by_key(|&&v| v.abs_diff(cur)).unwrap()
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_wraps_and_starts_from_the_first_unknown() {
        let all = [Layout::Row, Layout::Grid, Layout::List];
        assert_eq!(cycle(&all, Layout::Row, 1), Layout::Grid);
        assert_eq!(cycle(&all, Layout::Row, -1), Layout::List);
        assert_eq!(cycle(&all, Layout::List, 1), Layout::Row);
        assert_eq!(cycle(&all, Layout::Sidebar, 1), Layout::Grid);
    }

    #[test]
    fn cycle_num_steps_from_where_the_value_falls() {
        let secs = [0, 5, 10, 15, 30, 60];
        assert_eq!(cycle_num(&secs, 10, 1), 15);
        assert_eq!(cycle_num(&secs, 10, -1), 5);
        assert_eq!(cycle_num(&secs, 60, 1), 0);
        assert_eq!(cycle_num(&secs, 0, -1), 60);
        // Hand-set values.
        assert_eq!(cycle_num(&secs, 7, 1), 10);
        assert_eq!(cycle_num(&secs, 7, -1), 5);
        assert_eq!(cycle_num(&secs, 7, 0), 5);
        assert_eq!(cycle_num(&secs, 120, 1), 0);
        assert_eq!(cycle_num(&secs, 120, -1), 60);
        let fps = [1, 2, 4];
        assert_eq!(cycle_num(&fps, 3, 1), 4);
        assert_eq!(cycle_num(&fps, 3, -1), 2);
        assert_eq!(cycle_num(&[], 3, 1), 3);
    }

    #[test]
    fn walk_wraps_on_a_press_and_stops_while_held() {
        assert_eq!(walk(1, 4, 1, false), 2);
        assert_eq!(walk(3, 4, 1, false), 0);
        assert_eq!(walk(0, 4, -1, false), 3);
        assert_eq!(walk(3, 4, 1, true), 3);
        assert_eq!(walk(0, 4, -1, true), 0);
        assert_eq!(walk(2, 4, -1, true), 1);
        assert_eq!(walk(0, 0, 1, false), 0);
    }

    #[test]
    fn step_snaps_to_the_grid() {
        assert!((step(1.0, 0.1, 1) - 1.1).abs() < 1e-6);
        assert!((step(1.03, 0.1, -1) - 0.9).abs() < 1e-6);
        assert!((step(0.35, 0.05, 1) - 0.4).abs() < 1e-6);
    }

    #[test]
    fn keep_visible_and_grid_cols() {
        assert_eq!(keep_visible(3, 10, 5), 0);
        assert_eq!(keep_visible(0, 4, 10), 0);
        assert_eq!(keep_visible(5, 4, 10), 3);
        assert_eq!(keep_visible(9, 4, 10), 6);
        let cols: Vec<usize> = (0..=9).map(grid_cols).collect();
        assert_eq!(cols, [1, 1, 2, 3, 4, 3, 3, 4, 4, 4]);
    }
}
