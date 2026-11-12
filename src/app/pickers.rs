//! The style and position pickers, and placing the clock.

use super::entries::Entry;
use super::settings::{Item, item_index};
use super::{App, Hit, Nav, Screen};
use crate::config::{Layout, Position};
use crate::i18n::t;
use crate::model::TvState;
use crate::theme::{mix, regular, semibold, veil};
use egui::{Align2, Color32, Rect, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};

/// The nine positions as they sit on the screen, row by row.
pub(super) const POSITION_GRID: [Position; 9] = [
    Position::TopLeft,
    Position::Top,
    Position::TopRight,
    Position::Left,
    Position::Center,
    Position::Right,
    Position::BottomLeft,
    Position::Bottom,
    Position::BottomRight,
];

impl App {
    /// Placing the clock: the arrows move it (1 % of the screen a step), OK keeps, Back cancels.
    pub(super) fn clock_key(&mut self, k: &Nav) {
        let Screen::ClockPlace { orig } = self.screen.clone() else { return };
        let c = &mut self.cfg.clock;
        match k {
            Nav::Left => c.x = (c.x - 0.01).max(0.0),
            Nav::Right => c.x = (c.x + 0.01).min(1.0),
            Nav::Up => c.y = (c.y - 0.015).max(0.0),
            Nav::Down => c.y = (c.y + 0.015).min(1.0),
            Nav::Ok => {
                self.persist();
                self.screen = Screen::Settings { sel: item_index(Item::ClockPlace), confirm_reset: false };
            }
            Nav::Back | Nav::Erase | Nav::Source => {
                self.cfg.clock = orig;
                self.screen = Screen::Settings { sel: item_index(Item::ClockPlace), confirm_reset: false };
            }
            _ => {}
        }
    }

    /// The whole screen: the interface faded where it really sits, the clock framed, and
    /// a small bar of hints.
    pub(super) fn draw_clock_place(&mut self, ui: &Ui, p: &egui::Painter, st: &TvState, entries: &[Entry], screen: Rect, alpha: f32) {
        let s = self.cfg.scale;
        let k = self.t;
        self.draw_picker(ui, p, st, entries, screen, Vec2::ZERO, alpha * 0.35);
        self.hits.borrow_mut().clear();
        let mut c = self.cfg.clock;
        c.show = true;
        let r = crate::clock::draw(p, screen, &c, &k, alpha);
        p.rect_stroke(r.expand(10.0 * s), 10.0 * s, Stroke::new(2.0 * s, veil(k.accent, alpha)), StrokeKind::Outside);
        let hints = [("◀ ▲ ▼ ▶", t("hint.move")), (t("key.ok"), t("hint.confirm")), (t("key.back"), t("hint.cancel"))];
        // Wide enough for the help line and every hint, so nothing leaves the bar.
        let help = p.layout_no_wrap(t("clock.place_help").to_string(), regular(17.0 * s), k.text_dim).size().x;
        let w = (self.footer_width(p, &hints, 1.0) + 56.0 * s).max(help + 64.0 * s).max(520.0 * s);
        let bar = Rect::from_center_size(pos2(screen.center().x, screen.min.y + 70.0 * s), vec2(w.min(screen.width() - 32.0 * s), 110.0 * s));
        p.rect(bar, k.radius_lg * 2.0 * s, veil(k.card, 0.92 * alpha), Stroke::new(1.0, veil(k.card_border, alpha)), StrokeKind::Inside);
        p.text(pos2(bar.center().x, bar.min.y + 28.0 * s), Align2::CENTER_CENTER, t("clock.place_help"), regular(17.0 * s), veil(k.text_dim, alpha));
        self.footer(p, bar, &hints, alpha);
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
    }

    /// Keys of the style and position pickers: arrows choose, OK keeps, Back cancels.
    pub(super) fn pick_key(&mut self, k: &Nav, ctx: &egui::Context) {
        let back = |item| Screen::Settings { sel: item_index(item), confirm_reset: false };
        match self.screen.clone() {
            Screen::StylePick { mut sel } => {
                let n = Layout::ALL.len();
                match k {
                    Nav::Left | Nav::Up => sel = (sel + n - 1) % n,
                    Nav::Right | Nav::Down => sel = (sel + 1) % n,
                    Nav::Ok => {
                        self.cfg.layout = Layout::ALL[sel];
                        self.retheme(ctx);
                        self.persist();
                        self.screen = back(Item::Layout);
                        return;
                    }
                    Nav::Back | Nav::Erase | Nav::Source => {
                        self.screen = back(Item::Layout);
                        return;
                    }
                    _ => {}
                }
                self.screen = Screen::StylePick { sel };
            }
            Screen::PositionPick { mut sel } => {
                // The arrows move across the screen, as the panel would.
                let (r, c) = (sel / 3, sel % 3);
                match k {
                    Nav::Left if c > 0 => sel -= 1,
                    Nav::Right if c < 2 => sel += 1,
                    Nav::Up if r > 0 => sel -= 3,
                    Nav::Down if r < 2 => sel += 3,
                    Nav::Ok => {
                        self.cfg.position = POSITION_GRID[sel];
                        self.retheme(ctx);
                        self.persist();
                        self.screen = back(Item::Position);
                        return;
                    }
                    Nav::Back | Nav::Erase | Nav::Source => {
                        self.screen = back(Item::Position);
                        return;
                    }
                    _ => {}
                }
                self.screen = Screen::PositionPick { sel };
            }
            _ => {}
        }
    }

    /// The panel's shape for the current layout, placed at `pos` inside `inner`.
    pub(super) fn panel_glyph_rect(&self, inner: Rect, pos: Position) -> Rect {
        let (px, py) = position_anchor(pos);
        // The panel's shape follows the layout: wide for a row, tall for a list.
        let (pw, ph) = layout_shape(self.cfg.layout);
        let size = vec2(inner.width() * pw, inner.height() * ph);
        Rect::from_min_size(pos2(inner.min.x + (inner.width() - size.x) * px, inner.min.y + (inner.height() - size.y) * py), size)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_style_pick(&mut self, ui: &Ui, p: &egui::Painter, screen: Rect, alpha: f32, sel: usize) {
        let s = self.cfg.scale;
        let k = self.t;
        let (cw, ch, gap, pad) = (230.0 * s, 200.0 * s, 20.0 * s, 28.0 * s);
        let n = Layout::ALL.len() as f32;
        let size = vec2(n * cw + (n - 1.0) * gap + 2.0 * pad, 64.0 * s + ch + 92.0 * s);
        // Always centred: the picker does not move while a style is chosen.
        let rect = Rect::from_center_size(screen.center(), size.min(screen.size() - Vec2::splat(40.0 * s)));
        self.panel(p, rect, alpha);
        self.header(p, rect, t("settings.layout"), t("pick.style_help"), alpha);
        let top = rect.min.y + 64.0 * s + 12.0 * s;
        let hx = self.anim(ui, "pick-x", rect.min.x + pad + sel as f32 * (cw + gap), 0.14);
        for (i, layout) in Layout::ALL.iter().enumerate() {
            let card = Rect::from_min_size(pos2(rect.min.x + pad + i as f32 * (cw + gap), top), vec2(cw, ch));
            self.add_hit(card, Hit::PickCell(i));
            let current = *layout == self.cfg.layout;
            p.rect(card, k.radius_lg * 1.4 * s, veil(k.field, alpha * 0.8), Stroke::new(1.0, veil(k.field_border, alpha)), StrokeKind::Inside);
            let screen_r = Rect::from_min_size(pos2(card.min.x + 24.0 * s, card.min.y + 22.0 * s), vec2(cw - 48.0 * s, (cw - 48.0 * s) * 9.0 / 16.0));
            p.rect(screen_r, 6.0 * s, veil(k.card, alpha), Stroke::new(2.0 * s, veil(k.text_dim, alpha)), StrokeKind::Inside);
            let ink = if i == sel { k.accent } else { k.text_dim };
            let glyph = screen_r.shrink(4.0 * s);
            layout_glyph(p, glyph, *layout, veil(ink, alpha), (glyph.height() * 0.06).max(1.5));
            p.text(pos2(card.center().x, card.max.y - 34.0 * s), Align2::CENTER_CENTER, layout.label(), semibold(20.0 * s), veil(if i == sel { k.text } else { k.text_dim }, alpha));
            if current {
                p.text(pos2(card.center().x, card.max.y - 12.0 * s), Align2::CENTER_CENTER, t("pick.current"), regular(14.0 * s), veil(k.accent_text, alpha));
            }
        }
        let ring = Rect::from_min_size(pos2(hx, top), vec2(cw, ch)).expand(5.0 * s);
        p.rect_stroke(ring, k.radius_lg * 1.4 * s + 5.0 * s, Stroke::new(3.0 * s, veil(k.accent, alpha)), StrokeKind::Outside);
        self.footer(p, rect, &[("◀ ▶", t("hint.choose")), (t("key.ok"), t("hint.confirm")), (t("key.back"), t("hint.cancel"))], alpha);
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_position_pick(&mut self, ui: &Ui, p: &egui::Painter, screen: Rect, alpha: f32, sel: usize) {
        let s = self.cfg.scale;
        let k = self.t;
        let pad = 28.0 * s;
        let (sw, sh) = (560.0 * s, 315.0 * s);
        let size = vec2(sw + 2.0 * pad, 64.0 * s + 12.0 * s + sh + 14.0 * s + 72.0 * s);
        let rect = Rect::from_center_size(screen.center(), size.min(screen.size() - Vec2::splat(40.0 * s)));
        self.panel(p, rect, alpha);
        let name = POSITION_GRID[sel].label();
        self.header(p, rect, t("settings.position"), name, alpha);
        let mini = Rect::from_min_size(pos2(rect.min.x + pad, rect.min.y + 76.0 * s), vec2(sw, sh));
        p.rect(mini, 10.0 * s, veil(mix(k.card, Color32::BLACK, 0.35), alpha), Stroke::new(2.0 * s, veil(k.text_dim, alpha)), StrokeKind::Inside);
        let inner = mini.shrink(14.0 * s);
        // The nine places, faint; each one is a pointer target too.
        for (i, pos) in POSITION_GRID.iter().enumerate() {
            let g = self.panel_glyph_rect(inner, *pos);
            let cell = Rect::from_min_size(pos2(mini.min.x + (i % 3) as f32 * sw / 3.0, mini.min.y + (i / 3) as f32 * sh / 3.0), vec2(sw / 3.0, sh / 3.0));
            self.add_hit(cell, Hit::PickCell(i));
            if *pos == self.cfg.position && i != sel {
                p.rect_stroke(g, 4.0 * s, Stroke::new(1.5 * s, veil(k.text_faint, alpha)), StrokeKind::Inside);
            }
        }
        // The panel, gliding to the chosen place.
        let target = self.panel_glyph_rect(inner, POSITION_GRID[sel]);
        let at = Rect::from_min_size(pos2(self.anim(ui, "pos-x", target.min.x, 0.14), self.anim(ui, "pos-y", target.min.y, 0.14)), target.size());
        p.rect(at, 5.0 * s, veil(k.accent_soft, alpha), Stroke::new(2.5 * s, veil(k.accent, alpha)), StrokeKind::Inside);
        let glyph = at.shrink(3.0 * s);
        layout_glyph(p, glyph, self.cfg.layout, veil(k.accent, alpha), (glyph.height() * 0.06).max(1.5));
        self.footer(p, rect, &[("◀ ▲ ▼ ▶", t("hint.move")), (t("key.ok"), t("hint.confirm")), (t("key.back"), t("hint.cancel"))], alpha);
    }
}

/// Where a panel at `pos` sits on the screen, as fractions of the free room (0 = left/top).
fn position_anchor(pos: Position) -> (f32, f32) {
    match pos {
        Position::TopLeft => (0.0, 0.0),
        Position::Top => (0.5, 0.0),
        Position::TopRight => (1.0, 0.0),
        Position::Left => (0.0, 0.5),
        Position::Center => (0.5, 0.5),
        Position::Right => (1.0, 0.5),
        Position::BottomLeft => (0.0, 1.0),
        Position::Bottom => (0.5, 1.0),
        Position::BottomRight => (1.0, 1.0),
    }
}

/// The panel's shape in a layout, as fractions of the screen (width, height).
fn layout_shape(layout: Layout) -> (f32, f32) {
    match layout {
        Layout::Row => (0.62, 0.24),
        Layout::Grid => (0.42, 0.42),
        Layout::List => (0.26, 0.6),
        Layout::Sidebar => (0.24, 0.84),
    }
}

/// A layout drawn in `r` (a small screen with its tiles), in `ink`, tiles rounded by `radius`.
pub(super) fn layout_glyph(p: &egui::Painter, r: Rect, layout: Layout, ink: Color32, radius: f32) {
    let (w, h) = (r.width(), r.height());
    let b = |x0: f32, y0: f32, x1: f32, y1: f32| {
        let rr = Rect::from_min_max(pos2(r.min.x + w * x0, r.min.y + h * y0), pos2(r.min.x + w * x1, r.min.y + h * y1));
        p.rect_filled(rr, radius, ink);
    };
    match layout {
        Layout::Row => (0..4).for_each(|t| b(0.1 + t as f32 * 0.21, 0.5, 0.27 + t as f32 * 0.21, 0.84)),
        Layout::Grid => (0..6).for_each(|t| {
            let (c, rw) = ((t % 3) as f32, (t / 3) as f32);
            b(0.24 + c * 0.19, 0.24 + rw * 0.28, 0.38 + c * 0.19, 0.44 + rw * 0.28)
        }),
        Layout::List => (0..3).for_each(|t| b(0.34, 0.2 + t as f32 * 0.22, 0.66, 0.34 + t as f32 * 0.22)),
        Layout::Sidebar => b(0.7, 0.12, 0.88, 0.88),
    }
}
