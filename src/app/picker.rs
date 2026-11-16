//! The picker itself: its keys, its tiles and the source previews.

use super::entries::Entry;
use super::widgets::{RIM, fit, gradient, grid_cols, text_width, two_lines, vertical, walk};
use super::{App, Nav, Screen, Hit};
use crate::config::Layout;
use crate::i18n::{t, tf};
use crate::icons::{self, SourceIcon};
use crate::model::{Command, Source, SourceKind, TvState};
use crate::theme::{ink_on, mix, regular, semibold, veil};
use egui::{Align2, Color32, Pos2, Rect, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};

impl App {
    pub(super) fn picker_key(&mut self, k: &Nav, entries: &[Entry], now: f64) {
        let n = entries.len() + usize::from(self.cfg.show_customize_tile);
        // Nothing to pick (or the inputs could not be read): OK opens Customize.
        let empty = self.backend.state.lock().map(|st| empty_state(&st, entries)).unwrap_or(false);
        if empty || n == 0 {
            match k {
                Nav::Ok => self.open_customize(),
                Nav::Back | Nav::Erase => self.close(now),
                _ => {}
            }
            return;
        }
        let cols = match self.cfg.layout {
            Layout::Row => n,
            Layout::Grid => grid_cols(n),
            Layout::List | Layout::Sidebar => 1,
        };
        let list = vertical(self.cfg.layout);
        let grid = self.cfg.layout == Layout::Grid;
        let old = self.sel;
        // The key that opens an HDMI input's options: Down, or Right in a vertical list
        // (where Down moves). In a grid, Down on the last row. A long OK opens them anywhere.
        // Not a held arrow: that one only moves, it never opens another screen.
        let opens_options = !self.held && *k == self.options_key_nav() && self.arrow_opens_options(entries);
        match k {
            _ if opens_options => {
                if let Some(id) = self.selected_hdmi(entries) {
                    self.opt_sel = None;
                    self.screen = Screen::Options { id, sel: super::options::SEL_START };
                    return;
                }
            }
            // Long OK: the options of any source, in every layout (the Customize tile opens).
            Nav::Menu => {
                self.armed_at = None;
                self.opt_sel = None;
                match entries.get(self.sel) {
                    Some(e) => self.screen = Screen::Options { id: e.source.id.clone(), sel: super::options::SEL_START },
                    None => self.open_customize(),
                }
                return;
            }
            // Source pressed again cycles the inputs and the highlighted one is taken
            // after `switch_delay_secs`, as on LG's picker. Only HDMI inputs with a cable
            // and Live TV take part (not USB, DLNA, Home or Customize), unless there are none.
            // The Apps tile never does: it opens a list, it is not an input to switch to.
            // The arrows never start this countdown: they cancel it, and OK switches then.
            Nav::Source => {
                let quick: Vec<usize> = entries
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| matches!(e.source.kind, SourceKind::Hdmi | SourceKind::LiveTv) && e.source.connected)
                    .map(|(i, _)| i)
                    .collect();
                let quick = if quick.is_empty() { (0..entries.len()).filter(|&i| entries[i].source.kind != SourceKind::Apps).collect() } else { quick };
                if let Some(&next) = quick.iter().find(|&&i| i > self.sel).or(quick.first()) {
                    self.sel = next;
                    self.armed_at = Some(now);
                }
            }
            Nav::Left if !list => self.sel = walk(self.sel, n, -1, self.held),
            Nav::Right if !list => self.sel = walk(self.sel, n, 1, self.held),
            Nav::Up if list => self.sel = walk(self.sel, n, -1, self.held),
            Nav::Down if list => self.sel = walk(self.sel, n, 1, self.held),
            Nav::Up if grid && self.sel >= cols => self.sel -= cols,
            // The arrow that leads nowhere in this layout opens Customize (not while a quick
            // switch counts down: an arrow cancels that first).
            Nav::Up | Nav::Left if *k == self.customize_nav() && self.armed_at.is_none() && !self.held => {
                self.open_customize();
                return;
            }
            Nav::Down if grid && self.sel + cols < n => self.sel += cols,
            Nav::Down if grid && self.sel / cols < (n - 1) / cols => self.sel = n - 1,
            Nav::Ok => self.activate_selected(entries, now),
            Nav::Back | Nav::Erase => self.close(now),
            _ => {}
        }
        if *k != Nav::Source {
            // Any other key means the user is choosing: OK switches then.
            self.armed_at = None;
        }
        if self.sel != old {
            self.moved_since_open = true;
        }
    }

    /// Asks for live pictures (or none), and uploads frames that changed.
    pub(super) fn sync_previews(&mut self, ctx: &egui::Context, st: &TvState) {
        let on = self.cfg.show_previews && st.preview_supported != Some(false) && self.closing.is_none();
        self.backend.set_preview_fps(if on { self.cfg.preview_fps.clamp(1, 4) } else { 0 });
        if !self.cfg.show_previews {
            return;
        }
        // A picture taken back (filed under an input it did not show) leaves its tile plain.
        self.preview_tex.retain(|id, _| st.previews.contains_key(id));
        for (id, pv) in &st.previews {
            if self.preview_tex.get(id).is_some_and(|(seq, _)| *seq == pv.seq) {
                continue;
            }
            let image = egui::ColorImage::from_rgba_unmultiplied([pv.width, pv.height], &pv.rgba);
            match self.preview_tex.get_mut(id) {
                Some((seq, tex)) => {
                    tex.set(image, egui::TextureOptions::LINEAR);
                    *seq = pv.seq;
                }
                None => {
                    let tex = ctx.load_texture(format!("preview-{id}"), image, egui::TextureOptions::LINEAR);
                    self.preview_tex.insert(id.clone(), (pv.seq, tex));
                }
            }
        }
    }

    /// A source's picture filling `rect` (cropped to cover), darkened so the
    /// tile's text stays readable; the live one less than a remembered one.
    fn paint_preview(&self, p: &egui::Painter, rect: Rect, radius: f32, id: &str, live: bool, alpha: f32) -> bool {
        let Some((_, tex)) = self.preview_tex.get(id).filter(|_| self.cfg.show_previews) else { return false };
        let [w, h] = tex.size();
        let (img_a, rect_a) = (w as f32 / h.max(1) as f32, rect.width() / rect.height().max(1.0));
        let uv = if rect_a < img_a {
            let u = rect_a / img_a;
            Rect::from_min_max(pos2((1.0 - u) / 2.0, 0.0), pos2((1.0 + u) / 2.0, 1.0))
        } else {
            let v = img_a / rect_a;
            Rect::from_min_max(pos2(0.0, (1.0 - v) / 2.0), pos2(1.0, (1.0 + v) / 2.0))
        };
        let tint = veil(Color32::WHITE, alpha);
        p.add(egui::epaint::RectShape::filled(rect, radius, tint).with_texture(tex.id(), uv));
        let shade = if live { 0.42 } else { 0.62 };
        p.rect_filled(rect, radius, Color32::from_black_alpha((shade * alpha * 255.0) as u8));
        true
    }

    /// The key that opens CEC options in this layout.
    pub(super) fn options_key_nav(&self) -> Nav {
        if vertical(self.cfg.layout) { Nav::Right } else { Nav::Down }
    }

    /// The selected tile's id if it is a connected HDMI input with options of its device
    /// (HDMI-CEC keys, or the network receiver's inputs): the arrow opens those.
    fn selected_hdmi(&self, entries: &[Entry]) -> Option<String> {
        let e = entries.get(self.sel)?;
        (e.source.kind == SourceKind::Hdmi && e.source.connected && (e.source.cec || e.avr)).then(|| e.source.id.clone())
    }

    /// Whether the arrow that opens the options does it from the selected tile (in a grid,
    /// only from the last row, where Down has nowhere else to go).
    fn arrow_opens_options(&self, entries: &[Entry]) -> bool {
        let n = entries.len() + usize::from(self.cfg.show_customize_tile);
        let last_row = self.cfg.layout != Layout::Grid || {
            let cols = grid_cols(n);
            self.sel / cols == (n.max(1) - 1) / cols
        };
        last_row && self.selected_hdmi(entries).is_some()
    }

    /// The arrow that moves nowhere in this layout, which opens Customize: Up in a row or on
    /// a grid's first row, Left in a list.
    fn customize_nav(&self) -> Nav {
        if vertical(self.cfg.layout) { Nav::Left } else { Nav::Up }
    }

    /// Opens Customize. The one place the picker builds the Settings screen.
    pub(super) fn open_customize(&mut self) {
        self.screen = Screen::Settings { sel: 0, confirm_reset: false };
        self.moved_since_open = false;
    }

    pub(super) fn activate_selected(&mut self, entries: &[Entry], now: f64) {
        if self.sel >= entries.len() {
            self.open_customize();
            return;
        }
        let e = &entries[self.sel];
        // The Apps tile opens its list instead of launching anything.
        if e.source.kind == SourceKind::Apps {
            self.open_apps();
            return;
        }
        if !e.current {
            let _ = self.backend.commands.send(Command::Launch(e.source.app_id.clone(), e.source.params.clone()));
        }
        self.close(now);
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_picker(&mut self, ui: &Ui, p: &egui::Painter, st: &TvState, entries: &[Entry], screen: Rect, offset: Vec2, alpha: f32) {
        let s = self.cfg.scale;
        let k = self.t;
        let n = entries.len() + usize::from(self.cfg.show_customize_tile);
        self.sel = self.sel.min(n.saturating_sub(1));
        let pad = 28.0 * s;
        let head = if self.cfg.show_header { pad + 38.0 * s + 20.0 * s } else { pad };
        let foot = if self.cfg.show_hints { 30.0 * s + 16.0 * s + 26.0 * s } else { pad - 8.0 * s };

        // Tiles grow to fit the longest name (within reason) rather than cut it.
        let longest = entries.iter().map(|e| text_width(p, &e.name, semibold(22.0 * s))).fold(0.0, f32::max);
        let mut tile_w = (longest + 36.0 * s).clamp(210.0 * s, 290.0 * s);
        // A long row shrinks its tiles to fit the screen before it has to scroll.
        if self.cfg.layout == Layout::Row && n > 0 {
            let room = screen.width() - 2.0 * 56.0 * s - 2.0 * pad;
            let fit_w = (room - 18.0 * s * (n as f32 - 1.0)) / n as f32;
            if fit_w < tile_w {
                tile_w = fit_w.max(160.0 * s);
            }
        }
        let (mut tile, gap, cols, rows) = match self.cfg.layout {
            Layout::Row => (vec2(tile_w, 196.0 * s), 18.0 * s, n.max(1), 1),
            Layout::Grid => {
                let c = grid_cols(n);
                (vec2(tile_w, 196.0 * s), 18.0 * s, c, n.div_ceil(c).max(1))
            }
            Layout::List => (vec2(560.0, 84.0) * s, 8.0 * s, 1, n.max(1)),
            Layout::Sidebar => (vec2(440.0, 84.0) * s, 8.0 * s, 1, n.max(1)),
        };
        // In a list, the selected row pushes its neighbours apart so its ring never
        // touches them: rows above stay, it moves one step down, rows below two.
        let push = if vertical(self.cfg.layout) { 9.0 * s } else { 0.0 };
        let mut content = vec2(
            cols as f32 * tile.x + cols.saturating_sub(1) as f32 * gap,
            rows as f32 * tile.y + rows.saturating_sub(1) as f32 * gap + 2.0 * push,
        );
        // The panel is at least as wide as its key hints (the Sidebar keeps its width and
        // compacts them instead).
        let hint_w = if self.cfg.show_hints {
            let probe = [(t("key.ok"), t("hint.switch")), ("▲ ▼", t("hint.move")), ("▶", t("hint.options")), (t("key.back"), t("hint.close"))];
            self.footer_width(p, &probe, 1.0) + 2.0 * pad
        } else {
            0.0
        };
        let min_w = if self.cfg.layout == Layout::Sidebar { 0.0 } else { (640.0 * s).max(hint_w) };
        let want = vec2((content.x + pad * 2.0).max(min_w), head + content.y + foot + 8.0 * s);
        let rect = self.panel_rect(screen, want).translate(offset);
        self.panel(p, rect, alpha);
        self.last_panel_frac = Some((self.cfg.layout, rect.size() / screen.size()));

        let target = entries.get(self.sel).filter(|_| self.switch_left.is_some());
        let subtitle = if let (Some(e), Some((_, secs))) = (target, self.switch_left) {
            tf("picker.switching", &[("name", &e.name), ("n", &secs.to_string())])
        } else if let Some(err) = &st.error {
            tf("error.inputs", &[("error", err)])
        } else if !st.loaded {
            t("picker.loading").to_string()
        } else {
            entries
                .iter()
                .find(|e| e.current)
                .map(|e| {
                    // Through the receiver: what it shows too ("Denon AVC-S670H · Game").
                    let name = match st.avr.active_input().filter(|_| e.avr) {
                        Some(i) => format!("{} · {}", e.name, i.name),
                        None => e.name.clone(),
                    };
                    tf("picker.watching", &[("name", &name)])
                })
                .unwrap_or_default()
        };
        if self.cfg.show_header {
            self.header(p, rect, None, t("picker.title"), &subtitle, alpha);
        }

        // Lay tiles out, scrolling to keep the selection in view.
        let empty = empty_state(st, entries);
        let n = if empty { 0 } else { n };
        let area = Rect::from_min_max(pos2(rect.min.x + pad, rect.min.y + head), pos2(rect.max.x - pad, rect.max.y - foot - 8.0 * s));
        // A list's rows span the panel, however wide its hints made it.
        if vertical(self.cfg.layout) {
            tile.x = area.width();
            content.x = tile.x;
        }
        let cell = |i: usize| -> Rect {
            let (c, r) = (i % cols, i / cols);
            Rect::from_min_size(pos2(area.min.x + c as f32 * (tile.x + gap), area.min.y + r as f32 * (tile.y + gap)), tile)
        };
        let sel_now = self.sel;
        let offsets: Vec<f32> = (0..n)
            .map(|i| {
                let want = if push == 0.0 || i < sel_now { 0.0 } else if i == sel_now { push } else { 2.0 * push };
                self.anim(ui, &format!("push-{i}"), want, 0.16)
            })
            .collect();
        let cell = |i: usize| -> Rect { cell(i).translate(vec2(0.0, offsets.get(i).copied().unwrap_or(0.0))) };
        let target = cell(self.sel);
        let overflow_x = (content.x - area.width()).max(0.0);
        let overflow_y = (content.y - area.height()).max(0.0);
        let want_scroll = if overflow_x > 0.0 {
            (target.center().x - area.min.x - area.width() / 2.0).clamp(0.0, overflow_x)
        } else if overflow_y > 0.0 {
            (target.center().y - area.min.y - area.height() / 2.0).clamp(0.0, overflow_y)
        } else {
            0.0
        };
        self.scroll = self.anim(ui, "scroll", want_scroll, 0.18);
        let shift = if overflow_x > 0.0 { vec2(-self.scroll, 0.0) } else { vec2(0.0, -self.scroll) };
        // Centre a short row in a wide panel.
        let centre = vec2(((area.width() - content.x) / 2.0).max(0.0), 0.0);
        let place = |r: Rect| r.translate(shift + centre);
        let clip = p.with_clip_rect(area.expand(10.0 * s));

        // Gliding highlight behind the selected tile.
        let tr = place(target);
        let hl = Rect::from_min_size(pos2(self.anim(ui, "hx", tr.min.x, 0.14), self.anim(ui, "hy", tr.min.y, 0.14)), tr.size());
        let r = k.radius_lg * 1.4 * s;
        let visible = |rect_i: &Rect| rect_i.intersects(area.expand(tile.x));

        // Three passes, so nothing is ever drawn over the gliding highlight:
        // every tile's background, then the highlight once, then the contents.
        for i in 0..n {
            let rect_i = place(cell(i));
            if !visible(&rect_i) {
                continue;
            }
            let base = veil(mix(k.field, k.card, 0.35), alpha * self.cfg.panel_opacity.max(0.75));
            clip.rect(rect_i, r, base, self.border(1.0, veil(k.field_border, alpha * 0.6)), StrokeKind::Inside);
            if let Some(e) = entries.get(i) {
                let live = st.previews.get(&e.source.id).is_some_and(|p| p.live);
                if self.paint_preview(&clip, rect_i, r, &e.source.id, live, alpha) {
                    clip.rect_stroke(rect_i, r, self.border(1.0, veil(k.field_border, alpha * 0.6)), StrokeKind::Inside);
                }
            }
            self.rim(&clip, rect_i, r, alpha * RIM);
            self.add_hit(rect_i.intersect(area), Hit::Tile(i));
        }
        if n > 0 {
            let pictured = entries.get(self.sel).is_some_and(|e| self.pictured(e));
            clip.rect_filled(hl, r, veil(k.hover, alpha * if pictured { 0.25 } else { 1.0 }));
            clip.rect_stroke(hl.expand(5.0 * s), r + 5.0 * s, Stroke::new(3.0 * s, veil(k.ring, alpha)), StrokeKind::Outside);
            // Quick switch countdown: a bar along the bottom of the selection that empties.
            // and the seconds left in a pill at its top-left corner.
            if let Some((f, secs)) = self.switch_left {
                let track = Rect::from_min_max(pos2(hl.min.x + r, hl.max.y - 9.0 * s), pos2(hl.max.x - r, hl.max.y - 5.0 * s));
                clip.rect_filled(track, 2.0 * s, veil(k.field_border, alpha));
                clip.rect_filled(Rect::from_min_size(track.min, vec2(track.width() * f, track.height())), 2.0 * s, veil(k.accent, alpha));
                let label = format!("{secs} s");
                let font = semibold(17.0 * s);
                let tw = text_width(&clip, &label, font.clone());
                let size = vec2(tw + 20.0 * s, 26.0 * s);
                // Top-left on a tile; in a list, at the right end (left of the ACTIF badge).
                let pill = if vertical(self.cfg.layout) {
                    let badge = if entries.get(self.sel).is_some_and(|e| e.current) { 120.0 * s } else { 0.0 };
                    Align2::RIGHT_CENTER.anchor_size(pos2(hl.max.x - 18.0 * s - badge, hl.center().y), size)
                } else {
                    Rect::from_min_size(hl.min + vec2(10.0 * s, 10.0 * s), size)
                };
                clip.rect_filled(pill, 13.0 * s, veil(k.accent, alpha));
                clip.text(pill.center(), Align2::CENTER_CENTER, label, font, veil(ink_on(k.accent), alpha));
            }
        }
        for i in 0..n {
            let rect_i = place(cell(i));
            if !visible(&rect_i) {
                continue;
            }
            // How selected this tile is, 0..1, faded so colours never jump.
            let fade = if self.cfg.animations && !self.snap { 0.16 } else { 0.0 };
            let selected = ui.ctx().animate_bool_with_time(egui::Id::unique(("lg-source-tile", i)), i == self.sel, fade);
            match entries.get(i) {
                Some(e) => self.draw_tile(ui, &clip, rect_i, e.icon.svg(), e, selected, self.pictured(e), alpha),
                None => {
                    let e = Entry {
                        source: Source::live_tv(),
                        name: t("picker.customize").into(),
                        // A newer version is said here, and nowhere else in the picker.
                        sub: Some(t(if self.update_notice(st).is_some() { "picker.customize_update" } else { "picker.customize_sub" }).into()),
                        icon: SourceIcon::Auto,
                        current: false,
                        hidden: false,
                        avr: false,
                    };
                    self.draw_tile(ui, &clip, rect_i, "settings", &e, selected, false, alpha);
                }
            }
        }

        // Content cut by the panel's edge fades out there, with a chevron when it is sideways.
        let edge = 48.0 * s;
        // Only on a (nearly) opaque panel: over a see-through one the tint shows as a band.
        let fade_c = if self.cfg.panel_opacity >= 0.85 { veil(k.card, alpha * self.cfg.panel_opacity) } else { Color32::TRANSPARENT };
        let clear = Color32::TRANSPARENT;
        let outer = area.expand(10.0 * s);
        if overflow_x > 0.0 {
            for (left, hidden) in [(true, self.scroll > 0.5), (false, self.scroll < overflow_x - 0.5)] {
                if !hidden {
                    continue;
                }
                let band = if left {
                    Rect::from_min_max(outer.min, pos2(outer.min.x + edge, outer.max.y))
                } else {
                    Rect::from_min_max(pos2(outer.max.x - edge, outer.min.y), outer.max)
                };
                if self.cfg.show_panel {
                    let (a, b) = if left { (fade_c, clear) } else { (clear, fade_c) };
                    gradient(p, band, a, b, false);
                }
                let at = pos2(if left { band.min.x + 16.0 * s } else { band.max.x - 16.0 * s }, band.center().y);
                p.circle_filled(at, 20.0 * s, veil(k.card, alpha * 0.9));
                icons::paint(p, if left { "chevron-left" } else { "chevron-right" }, Rect::from_center_size(at, Vec2::splat(30.0 * s)), veil(k.text, alpha));
            }
        } else if overflow_y > 0.0 && self.cfg.show_panel {
            if self.scroll > 0.5 {
                gradient(p, Rect::from_min_max(outer.min, pos2(outer.max.x, outer.min.y + edge)), fade_c, clear, true);
            }
            if self.scroll < overflow_y - 0.5 {
                gradient(p, Rect::from_min_max(pos2(outer.min.x, outer.max.y - edge), outer.max), clear, fade_c, true);
            }
        }

        if empty {
            // Nothing to show: say why, and the way to Customize.
            let line1 = st.error.as_ref().map(|e| tf("error.inputs", &[("error", e)])).unwrap_or_else(|| t("picker.empty").to_string());
            let c = area.center();
            let f1 = semibold(22.0 * s);
            self.label(p, pos2(c.x, c.y - 16.0 * s), Align2::CENTER_CENTER, &fit(p, &line1, f1.clone(), area.width()), f1, k.text, false, alpha);
            self.label(p, pos2(c.x, c.y + 18.0 * s), Align2::CENTER_CENTER, t("picker.empty_hint"), regular(18.0 * s), k.text_dim, true, alpha);
            if self.cfg.show_hints {
                self.footer(p, rect, &[(t("key.ok"), t("picker.customize")), (t("key.back"), t("hint.close"))], alpha);
            }
            if st.loaded {
                self.snap = false;
            }
            return;
        }

        let mv = match self.cfg.layout {
            Layout::Grid => "◀ ▲ ▼ ▶",
            l if vertical(l) => "▲ ▼",
            _ => "◀ ▶",
        };
        if self.cfg.show_hints && self.switch_left.is_some() {
            // Counting down: OK takes it now, an arrow stops it.
            let cancel = if vertical(self.cfg.layout) { "▲ ▼" } else { "◀ ▶" };
            self.footer(p, rect, &[(t("key.ok"), t("hint.switch_now")), (cancel, t("hint.cancel")), (t("key.back"), t("hint.close"))], alpha);
        } else if self.cfg.show_hints {
            let ok = if entries.get(self.sel).is_some_and(|e| e.source.kind == SourceKind::Apps) { t("hint.open") } else { t("hint.switch") };
            let mut hints = vec![(t("key.ok"), ok), (mv, t("hint.move"))];
            // The options: with the arrow where it opens them from this tile, else a long OK
            // (which opens them from any tile, in every layout).
            if self.arrow_opens_options(entries) {
                hints.push((if vertical(self.cfg.layout) { "▶" } else { "▼" }, t("hint.options")));
            } else if self.sel < entries.len() {
                hints.push((t("key.ok_hold"), t("hint.options")));
            }
            // Without the Customize tile, the arrow that opens it.
            if !self.cfg.show_customize_tile {
                hints.push((if vertical(self.cfg.layout) { "◀" } else { "▲" }, t("picker.customize")));
            }
            hints.push((t("key.back"), t("hint.close")));
            self.footer(p, rect, &hints, alpha);
        }
        if st.loaded {
            self.snap = false;
        }
    }

    /// Whether tile `e` shows its source's picture (the highlight and the text follow it).
    fn pictured(&self, e: &Entry) -> bool {
        self.cfg.show_previews && self.preview_tex.contains_key(&e.source.id)
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_tile(&self, _ui: &Ui, p: &egui::Painter, rect: Rect, icon: &str, e: &Entry, selected: f32, pictured: bool, alpha: f32) {
        let s = self.cfg.scale;
        let k = &self.t;
        let dimmed = !e.source.connected && !e.current;
        // Over a picture (shaded dark whatever the theme) the text is light, so a light
        // theme's dark ink never sits on it.
        let (fg, icon_tint, sub_c) = if pictured {
            let white = |a: f32| Color32::from_white_alpha((a * 255.0) as u8);
            let icon_sel = if k.dark { k.accent_text } else { white(0.85) };
            (Color32::WHITE, mix(white(0.85), icon_sel, selected), white(0.72))
        } else {
            (
                mix(if dimmed { k.text_faint } else { k.text }, k.text, selected),
                mix(if dimmed { k.text_faint } else { k.icon }, k.accent_text, selected),
                if dimmed { k.text_faint } else { k.text_dim },
            )
        };
        // The selected icon grows a little, eased in with the selection.
        let lift = 1.0 + 0.08 * selected;

        if vertical(self.cfg.layout) {
            // The icon grows around a fixed centre; the label never moves.
            let base = 38.0 * s;
            let ir = Rect::from_center_size(pos2(rect.min.x + 22.0 * s + base / 2.0, rect.center().y), Vec2::splat(base * lift));
            let x = if self.cfg.show_icons {
                icons::paint(p, icon, ir, veil(icon_tint, alpha));
                rect.min.x + 22.0 * s + base + 22.0 * s
            } else {
                rect.min.x + 26.0 * s
            };
            let badge_w = if e.current { 120.0 * s } else { 0.0 };
            let maxw = rect.max.x - x - 20.0 * s - badge_w;
            let (ny, sy) = if e.sub.is_some() { (rect.center().y - 13.0 * s, rect.center().y + 15.0 * s) } else { (rect.center().y, 0.0) };
            p.text(pos2(x, ny), Align2::LEFT_CENTER, fit(p, &e.name, semibold(23.0 * s), maxw), semibold(23.0 * s), veil(fg, alpha));
            if let Some(sub) = &e.sub {
                p.text(pos2(x, sy), Align2::LEFT_CENTER, fit(p, sub, regular(17.0 * s), maxw), regular(17.0 * s), veil(sub_c, alpha));
            }
            if e.current {
                self.badge(p, pos2(rect.max.x - 18.0 * s, rect.center().y), Align2::RIGHT_CENTER, alpha);
            }
            return;
        }

        // Fixed anchors, so icons and names line up from tile to tile whatever each one holds:
        // the icon centred at 36 % of the height, the first name line at 64 %, then a line
        // further down the subtitle (or the name's second line).
        let h = rect.height();
        let has_icon = self.cfg.show_icons;
        if has_icon {
            let ir = Rect::from_center_size(pos2(rect.center().x, rect.min.y + 0.36 * h), Vec2::splat(60.0 * s * lift));
            icons::paint(p, icon, ir, veil(icon_tint, alpha));
        }
        let maxw = rect.width() - 28.0 * s;
        // A long name takes two lines rather than being cut.
        let name_font = semibold(22.0 * s);
        let sub_font = regular(17.0 * s);
        let lines = two_lines(p, &e.name, name_font.clone(), maxw);
        let line_h = 26.0 * s;
        let first = rect.min.y + if has_icon { 0.64 * h } else { 0.44 * h };
        for (li, line) in lines.iter().enumerate() {
            p.text(pos2(rect.center().x, first + line_h * li as f32), Align2::CENTER_CENTER, line, name_font.clone(), veil(fg, alpha));
        }
        if let Some(sub) = &e.sub {
            let sy = first + line_h * lines.len() as f32;
            // Under a two-line name only if it still fits in the tile.
            if sy + 11.0 * s <= rect.max.y - 6.0 * s {
                p.text(pos2(rect.center().x, sy), Align2::CENTER_CENTER, fit(p, sub, sub_font.clone(), maxw), sub_font, veil(sub_c, alpha));
            }
        }
        if e.current {
            self.badge(p, pos2(rect.max.x - 12.0 * s, rect.min.y + 12.0 * s), Align2::RIGHT_TOP, alpha);
        }
    }

    /// The pill on the source being watched.
    fn badge(&self, p: &egui::Painter, at: Pos2, anchor: Align2, alpha: f32) {
        let s = self.cfg.scale;
        let k = &self.t;
        let f = semibold(15.0 * s);
        let label = t("picker.now");
        let w = text_width(p, label, f.clone()) + 30.0 * s;
        let r = anchor.anchor_size(at, vec2(w, 24.0 * s));
        p.rect_filled(r, 12.0 * s, veil(k.accent, alpha));
        let ink = ink_on(k.accent);
        p.circle_filled(pos2(r.min.x + 11.0 * s, r.center().y), 3.5 * s, veil(ink, alpha));
        p.text(pos2(r.min.x + 18.0 * s, r.center().y), Align2::LEFT_CENTER, label, f, veil(ink, alpha));
    }

}

/// The picker has nothing to offer: no entries once loaded, or the inputs could not be read.
fn empty_state(st: &TvState, entries: &[Entry]) -> bool {
    st.error.is_some() || (st.loaded && entries.is_empty())
}
