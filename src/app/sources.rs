//! The Sources editor (show, icon, rename, order) and the rename field.

use super::options::SEL_RENAME;
use super::settings::Item;
use super::widgets::{HELP_H, RIM, fit, keep_visible, text_width, walk};
use super::{App, Hit, Nav, Screen};
use crate::i18n::{t, tf};
use crate::icons::{self, SourceIcon};
use crate::model::{SourceKind, TvState};
use crate::theme::{medium, regular, semibold, veil};
use egui::{Align2, Color32, Rect, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};

/// Columns of a Sources row.
pub(super) const COL_SHOW: usize = 0;
const COL_ICON: usize = 1;
const COL_RENAME: usize = 2;
const COL_MOVE: usize = 3;
const COLS: usize = 4;
/// Columns of the icon picker's grid.
const ICON_COLS: usize = 5;

impl App {
    pub(super) fn sources_key(&mut self, k: &Nav, st: &TvState) {
        let Screen::Sources { mut sel, mut col, mut grabbed } = self.screen else { return };
        let all = self.all_entries(st);
        let n = all.len();
        let back_to_settings = self.settings_at(Item::Sources);
        if n == 0 {
            if matches!(k, Nav::Back | Nav::Erase) {
                self.screen = back_to_settings;
            }
            return;
        }
        sel = sel.min(n - 1);
        let ids: Vec<String> = all.iter().map(|e| e.source.id.clone()).collect();
        match k {
            Nav::Up | Nav::Down if grabbed => {
                let to = if *k == Nav::Up { sel.checked_sub(1) } else { (sel + 1 < n).then_some(sel + 1) };
                if let Some(to) = to {
                    let mut order = ids.clone();
                    order.swap(sel, to);
                    self.cfg.order = order;
                    sel = to;
                }
            }
            Nav::Up => sel = walk(sel, n, -1, self.held),
            Nav::Down => sel = walk(sel, n, 1, self.held),
            Nav::Left if !grabbed => col = walk(col, COLS, -1, self.held),
            Nav::Right if !grabbed => col = walk(col, COLS, 1, self.held),
            Nav::Ok => {
                let id = &ids[sel];
                match col {
                    COL_SHOW => {
                        let o = self.cfg.source_mut(id);
                        o.hidden = !o.hidden;
                    }
                    COL_ICON => {
                        let cur = self.cfg.source(id).map(|o| o.icon).unwrap_or_default();
                        let at = SourceIcon::ALL.iter().position(|i| *i == cur).unwrap_or(0);
                        self.screen = Screen::IconPick { id: id.clone(), sel: at };
                        return;
                    }
                    COL_RENAME => {
                        let text = self.cfg.source(id).and_then(|o| o.name.clone()).unwrap_or_default();
                        let tv_name = self.tv_name(&all[sel].source);
                        self.screen = Screen::Rename { id: id.clone(), sel, text, tv_name, from_options: false };
                        return;
                    }
                    _ => {
                        // Grabbing remembers the order, so Back can put it back; OK drops.
                        if grabbed {
                            self.grab_order = None;
                        } else {
                            self.grab_order = Some(self.cfg.order.clone());
                            if self.cfg.order.len() != n {
                                self.cfg.order = ids.clone();
                            }
                        }
                        grabbed = !grabbed;
                    }
                }
            }
            Nav::Back | Nav::Erase | Nav::Source if grabbed => {
                if let Some(order) = self.grab_order.take() {
                    let id = ids[sel].clone();
                    self.cfg.order = order;
                    // Follow the row back to where it was.
                    sel = self.all_entries(st).iter().position(|e| e.source.id == id).unwrap_or(sel);
                }
                grabbed = false;
            }
            Nav::Back | Nav::Erase | Nav::Source => {
                self.persist();
                self.screen = back_to_settings;
                return;
            }
            _ => {}
        }
        self.screen = Screen::Sources { sel, col, grabbed };
    }

    pub(super) fn rename_key(&mut self, k: &Nav) {
        let Screen::Rename { id, sel, text, tv_name, from_options } = &mut self.screen else { return };
        let back = if *from_options {
            Screen::Options { id: id.clone(), sel: SEL_RENAME }
        } else {
            Screen::Sources { sel: *sel, col: COL_RENAME, grabbed: false }
        };
        match k {
            Nav::Text(s) => {
                for c in s.chars().filter(|c| !c.is_control()) {
                    if text.chars().count() < 40 {
                        text.push(c);
                    }
                }
            }
            Nav::Erase => {
                text.pop();
            }
            Nav::Ok => {
                let (id, name) = (id.clone(), text.trim().to_string());
                let keep = (!name.is_empty() && name != *tv_name).then_some(name);
                self.cfg.source_mut(&id).name = keep;
                self.persist();
                self.screen = back;
            }
            Nav::Back | Nav::Source => self.screen = back,
            _ => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_sources(&mut self, ui: &Ui, p: &egui::Painter, st: &TvState, screen: Rect, offset: Vec2, alpha: f32, sel: usize, col: usize, grabbed: bool) {
        let s = self.cfg.scale;
        let k = self.t;
        let all = self.all_entries(st);
        let n = all.len();
        let pad = 28.0 * s;
        let head = pad + 38.0 * s + 18.0 * s;
        let foot = 72.0 * s;
        let row_h = 70.0 * s;
        // The columns' names over the first row, and what the selected cell does under the last.
        let titles_h = 26.0 * s;
        let help_h = HELP_H * s;
        let want = vec2(880.0 * s, head + titles_h + n.max(1) as f32 * row_h + help_h + foot);
        let rect = self.panel_rect(screen, want).translate(offset);
        self.panel(p, rect, alpha);
        // How many show in the picker (the keys are said under the rows).
        let shown = all.iter().filter(|e| !e.hidden).count();
        let summary = if shown == 1 { t("summary.inputs_one").to_string() } else { tf("summary.inputs", &[("n", &shown.to_string())]) };
        self.header(p, rect, None, t("sources.title"), &summary, alpha);

        let area = Rect::from_min_max(pos2(rect.min.x + pad * 0.6, rect.min.y + head + titles_h), pos2(rect.max.x - pad * 0.6, rect.max.y - foot - help_h));
        let cells_of = |r: Rect| {
            // Visibility and icon on the left, rename and move on the right.
            let (cw, ch) = (60.0 * s, r.height() - 12.0 * s);
            [
                Rect::from_center_size(pos2(r.min.x + 10.0 * s + cw / 2.0, r.center().y), vec2(cw, ch)),
                Rect::from_center_size(pos2(r.min.x + 18.0 * s + cw * 1.5, r.center().y), vec2(cw, ch)),
                Rect::from_center_size(pos2(r.max.x - 18.0 * s - cw * 1.5, r.center().y), vec2(cw, ch)),
                Rect::from_center_size(pos2(r.max.x - 10.0 * s - cw / 2.0, r.center().y), vec2(cw, ch)),
            ]
        };
        // Small caps over each cell's column, as the options' section headings.
        let heads = cells_of(Rect::from_min_size(pos2(area.min.x, area.min.y - titles_h), vec2(area.width(), titles_h)));
        for (cell, key) in heads.iter().zip(["sources.col_show", "sources.col_icon", "sources.col_rename", "sources.col_move"]) {
            p.text(pos2(cell.center().x, area.min.y - titles_h / 2.0 - 2.0 * s), Align2::CENTER_CENTER, t(key), semibold(14.0 * s), veil(k.text_faint, alpha));
        }
        let visible = (area.height() / row_h).floor().max(1.0) as usize;
        let first = keep_visible(sel, visible, n);
        let y0 = self.anim(ui, "sources-scroll", first as f32 * row_h, 0.15);
        // Only sideways slack (for the selection ring): rows never spill over the title or hints.
        let clip = p.with_clip_rect(area.expand2(vec2(6.0 * s, 0.0)));
        for (i, e) in all.iter().enumerate() {
            let r = Rect::from_min_size(pos2(area.min.x, area.min.y + i as f32 * row_h - y0), vec2(area.width(), row_h - 8.0 * s));
            if r.max.y < area.min.y - row_h || r.min.y > area.max.y + row_h {
                continue;
            }
            let selected = i == sel;
            let row_fill = if selected && grabbed { k.accent_soft } else if selected { veil(k.hover, 0.8) } else { veil(k.field, 0.5) };
            let ring = if selected && grabbed { 2.0 * s } else { 0.0 };
            clip.rect(r, k.radius * 1.5 * s, veil(row_fill, alpha), Stroke::new(ring, veil(k.ring, alpha)), StrokeKind::Inside);

            let cells = cells_of(r);
            if selected && !grabbed {
                clip.rect_stroke(cells[col], k.radius * s, Stroke::new(2.5 * s, veil(k.ring, alpha)), StrokeKind::Outside);
            }
            for (c, cell) in cells.iter().enumerate() {
                self.add_hit(cell.intersect(area), Hit::SourceCell(i, c));
            }
            let cell_icon = |idx: usize, name: &str, size: f32, tint: Color32| {
                icons::paint(&clip, name, Rect::from_center_size(cells[idx].center(), Vec2::splat(size * s)), veil(tint, alpha));
            };
            cell_icon(COL_SHOW, if e.hidden { "eye-off" } else { "eye" }, 26.0, if e.hidden { k.text_faint } else { k.success });
            cell_icon(COL_ICON, e.icon.svg(), 30.0, if e.hidden { k.text_faint } else { k.icon });
            cell_icon(COL_RENAME, "pencil", 24.0, if selected && col == COL_RENAME { k.accent_text } else { k.text_dim });
            cell_icon(COL_MOVE, if selected && grabbed { "arrow-up" } else { "rows-3" }, 26.0, if selected && grabbed { k.accent_text } else { k.text_dim });

            let x = cells[COL_ICON].max.x + 18.0 * s;
            let maxw = cells[COL_RENAME].min.x - x - 12.0 * s;
            let fg = if e.hidden { k.text_faint } else { k.text };
            let ov = self.cfg.source(&e.source.id);
            let renamed = ov.and_then(|o| o.name.as_ref()).is_some_and(|n| !n.trim().is_empty());
            let detail = if selected && col == COL_ICON && !grabbed {
                tf("sources.icon", &[("name", ov.map(|o| o.icon).unwrap_or_default().label())])
            } else if e.hidden {
                t("sources.hidden").to_string()
            } else if renamed {
                tf("sources.renamed_from", &[("name", &self.tv_name(&e.source))])
            } else {
                e.sub.clone().unwrap_or_else(|| if e.source.kind == SourceKind::Hdmi { e.source.id.replace('_', " ") } else { String::new() })
            };
            let (ny, dy) = if detail.is_empty() { (r.center().y, 0.0) } else { (r.center().y - 11.0 * s, r.center().y + 15.0 * s) };
            clip.text(pos2(x, ny), Align2::LEFT_CENTER, fit(p, &e.name, semibold(21.0 * s), maxw), semibold(21.0 * s), veil(fg, alpha));
            if !detail.is_empty() {
                clip.text(pos2(x, dy), Align2::LEFT_CENTER, fit(p, &detail, regular(16.0 * s), maxw), regular(16.0 * s), veil(k.text_dim, alpha));
            }
        }
        let help = match col {
            _ if grabbed => "help.sources_grabbed",
            COL_SHOW => "help.sources_show",
            COL_ICON => "help.sources_icon",
            COL_RENAME => "help.sources_rename",
            _ => "help.sources_move",
        };
        self.help_line(p, Rect::from_min_max(pos2(area.min.x, area.max.y), pos2(area.max.x, area.max.y + help_h)), t(help), alpha);
        if grabbed {
            self.footer(p, rect, &[("▲ ▼", t("hint.move")), (t("key.ok"), t("hint.drop")), (t("key.back"), t("hint.cancel"))], alpha);
            return;
        }
        let ok = match col {
            COL_SHOW => t("hint.toggle"),
            COL_ICON => t("hint.choose_icon"),
            COL_RENAME => t("hint.rename"),
            _ => t("hint.move"),
        };
        self.footer(p, rect, &[(t("key.ok"), ok), ("◀ ▶", t("hint.column")), (t("key.back"), t("hint.back"))], alpha);
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_rename(&mut self, ui: &Ui, p: &egui::Painter, screen: Rect, offset: Vec2, alpha: f32, text: &str, tv_name: &str) {
        self.draw_text_field(ui, p, screen, offset, alpha, (t("rename.title"), tv_name), text, tv_name, t("rename.empty"));
    }

    /// A text field under a (title, subtitle) header, with what shows while it is empty and
    /// a line of help under it.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_text_field(&mut self, ui: &Ui, p: &egui::Painter, screen: Rect, offset: Vec2, alpha: f32, head: (&str, &str), text: &str, placeholder: &str, help: &str) {
        let tv_name = placeholder;
        let s = self.cfg.scale;
        let k = self.t;
        let pad = 28.0 * s;
        let want = vec2(820.0 * s, 300.0 * s);
        // Kept in the top half: LG's on-screen keyboard rises from the bottom.
        let size = want.min(screen.size() - Vec2::splat(112.0 * s));
        let rect = Rect::from_min_size(pos2(screen.center().x - size.x / 2.0, screen.min.y + 90.0 * s), size).translate(offset);
        self.panel(p, rect, alpha);
        self.header(p, rect, None, head.0, head.1, alpha);

        let field = Rect::from_min_size(pos2(rect.min.x + pad, rect.min.y + 96.0 * s), vec2(rect.width() - pad * 2.0, 66.0 * s));
        p.rect(field, k.radius * 1.5 * s, veil(k.field, alpha), Stroke::new(2.0 * s, veil(k.ring, alpha)), StrokeKind::Inside);
        let font = medium(26.0 * s);
        let inner = field.shrink2(vec2(20.0 * s, 0.0));
        let (shown, color) = if text.is_empty() { (tv_name.to_string(), k.text_faint) } else { (text.to_string(), k.text) };
        // Keep the end of a long name (where the caret is) in view.
        let mut shown_fit = shown.clone();
        while text_width(p, &shown_fit, font.clone()) > inner.width() - 20.0 * s && shown_fit.chars().count() > 1 {
            shown_fit = shown_fit.chars().skip(1).collect();
        }
        let tr = p.text(pos2(inner.min.x, inner.center().y), Align2::LEFT_CENTER, &shown_fit, font, veil(color, alpha));
        let blink = (ui.input(|i| i.time) * 2.0).floor() as i64 % 2 == 0;
        let caret_x = if text.is_empty() { inner.min.x } else { tr.max.x + 2.0 * s };
        if blink {
            p.rect_filled(Rect::from_center_size(pos2(caret_x, inner.center().y), vec2(2.5 * s, 32.0 * s)), 1.0, veil(k.accent, alpha));
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
        p.text(pos2(field.min.x, field.max.y + 20.0 * s), Align2::LEFT_TOP, help, regular(17.0 * s), veil(k.text_dim, alpha));
        self.footer(p, rect, &[(t("key.ok"), t("hint.confirm")), (t("key.back"), t("hint.cancel"))], alpha);
    }

    /// Keys of the icon picker: arrows move in the grid, OK keeps, Back cancels.
    pub(super) fn icon_key(&mut self, k: &Nav, st: &TvState) {
        let Screen::IconPick { id, mut sel } = self.screen.clone() else { return };
        let n = SourceIcon::ALL.len();
        let back = |app: &Self| {
            let row = app.all_entries(st).iter().position(|e| e.source.id == id).unwrap_or(0);
            Screen::Sources { sel: row, col: COL_ICON, grabbed: false }
        };
        match k {
            Nav::Left => sel = walk(sel, n, -1, self.held),
            Nav::Right => sel = walk(sel, n, 1, self.held),
            Nav::Up if sel >= ICON_COLS => sel -= ICON_COLS,
            Nav::Down if sel + ICON_COLS < n => sel += ICON_COLS,
            Nav::Ok => {
                self.cfg.source_mut(&id).icon = SourceIcon::ALL[sel];
                self.persist();
                self.screen = back(self);
                return;
            }
            Nav::Back | Nav::Erase | Nav::Source => {
                self.screen = back(self);
                return;
            }
            _ => {}
        }
        self.screen = Screen::IconPick { id, sel };
    }

    /// The icons as a grid of cards with their names, as the style picker shows layouts.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_icon_pick(&mut self, ui: &Ui, p: &egui::Painter, st: &TvState, screen: Rect, alpha: f32, id: &str, sel: usize) {
        let s = self.cfg.scale;
        let k = self.t;
        let all = self.all_entries(st);
        let entry = all.iter().find(|e| e.source.id == id);
        let name = entry.map(|e| e.name.clone()).unwrap_or_default();
        let current = self.cfg.source(id).map(|o| o.icon).unwrap_or_default();
        let rows = SourceIcon::ALL.len().div_ceil(ICON_COLS);
        let (cw, ch, gap, pad) = (156.0 * s, 112.0 * s, 14.0 * s, 28.0 * s);
        let top_h = 64.0 * s + 12.0 * s;
        let size = vec2(ICON_COLS as f32 * cw + (ICON_COLS - 1) as f32 * gap + 2.0 * pad, top_h + rows as f32 * ch + (rows - 1) as f32 * gap + 84.0 * s);
        let rect = Rect::from_center_size(screen.center(), size.min(screen.size() - Vec2::splat(40.0 * s)));
        self.panel(p, rect, alpha);
        self.header(p, rect, None, t("icon_pick.title"), &name, alpha);
        let cell = |i: usize| Rect::from_min_size(pos2(rect.min.x + pad + (i % ICON_COLS) as f32 * (cw + gap), rect.min.y + top_h + (i / ICON_COLS) as f32 * (ch + gap)), vec2(cw, ch));
        for (i, icon) in SourceIcon::ALL.iter().enumerate() {
            let card = cell(i);
            self.add_hit(card, Hit::PickCell(i));
            let chosen = i == sel;
            p.rect(card, k.radius_lg * 1.4 * s, veil(if chosen { k.accent_soft } else { k.field }, alpha * 0.8), self.border(1.0, veil(k.field_border, alpha)), StrokeKind::Inside);
            self.rim(p, card, k.radius_lg * 1.4 * s, alpha * RIM);
            // Auto shows what the TV picked for this input.
            let svg = match (icon, entry) {
                (SourceIcon::Auto, Some(e)) => e.source.tv_icon.svg(),
                _ => icon.svg(),
            };
            let tint = if chosen { k.accent_text } else if *icon == current { k.text } else { k.icon };
            icons::paint(p, svg, Rect::from_center_size(pos2(card.center().x, card.min.y + 42.0 * s), Vec2::splat(40.0 * s)), veil(tint, alpha));
            let label = fit(p, icon.label(), regular(16.0 * s), cw - 16.0 * s);
            let ink = if chosen || *icon == current { k.text } else { k.text_dim };
            p.text(pos2(card.center().x, card.max.y - 24.0 * s), Align2::CENTER_CENTER, label, regular(16.0 * s), veil(ink, alpha));
            if *icon == current {
                p.circle_filled(pos2(card.max.x - 14.0 * s, card.min.y + 14.0 * s), 5.0 * s, veil(k.accent, alpha));
            }
        }
        let target = cell(sel);
        let at = pos2(self.anim(ui, "icon-x", target.min.x, 0.12), self.anim(ui, "icon-y", target.min.y, 0.12));
        let ring = Rect::from_min_size(at, vec2(cw, ch)).expand(4.0 * s);
        p.rect_stroke(ring, k.radius_lg * 1.4 * s + 4.0 * s, Stroke::new(3.0 * s, veil(k.ring, alpha)), StrokeKind::Outside);
        self.footer(p, rect, &[("◀ ▲ ▼ ▶", t("hint.choose")), (t("key.ok"), t("hint.confirm")), (t("key.back"), t("hint.cancel"))], alpha);
    }
}
