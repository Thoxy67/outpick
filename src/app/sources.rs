//! The Sources editor (show, icon, rename, order) and the rename field.

use super::options::SEL_RENAME;
use super::settings::{Item, item_index};
use super::widgets::{cycle, fit, keep_visible, text_width};
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

impl App {
    pub(super) fn sources_key(&mut self, k: &Nav, st: &TvState) {
        let Screen::Sources { mut sel, mut col, mut grabbed } = self.screen else { return };
        let all = self.all_entries(st);
        let n = all.len();
        let back_to_settings = Screen::Settings { sel: item_index(Item::Sources), confirm_reset: false };
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
            Nav::Up => sel = (sel + n - 1) % n,
            Nav::Down => sel = (sel + 1) % n,
            Nav::Left if !grabbed => col = (col + COLS - 1) % COLS,
            Nav::Right if !grabbed => col = (col + 1) % COLS,
            Nav::Ok => {
                let id = &ids[sel];
                match col {
                    COL_SHOW => {
                        let o = self.cfg.source_mut(id);
                        o.hidden = !o.hidden;
                    }
                    COL_ICON => {
                        let o = self.cfg.source_mut(id);
                        o.icon = cycle(&SourceIcon::ALL, o.icon, 1);
                    }
                    COL_RENAME => {
                        let text = self.cfg.source(id).and_then(|o| o.name.clone()).unwrap_or_default();
                        let tv_name = self.tv_name(&all[sel].source);
                        self.screen = Screen::Rename { id: id.clone(), sel, text, tv_name, from_options: false };
                        return;
                    }
                    _ => {
                        if !grabbed && self.cfg.order.len() != n {
                            self.cfg.order = ids.clone();
                        }
                        grabbed = !grabbed;
                    }
                }
            }
            Nav::Back | Nav::Erase | Nav::Source if grabbed => grabbed = false,
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
        let want = vec2(880.0 * s, head + n.max(1) as f32 * row_h + foot);
        let rect = self.panel_rect(screen, want).translate(offset);
        self.panel(p, rect, alpha);
        self.header(p, rect, t("sources.title"), t(if grabbed { "sources.moving" } else { "sources.pick_column" }), alpha);

        let area = Rect::from_min_max(pos2(rect.min.x + pad * 0.6, rect.min.y + head), pos2(rect.max.x - pad * 0.6, rect.max.y - foot));
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
            clip.rect(r, k.radius * 1.5 * s, veil(row_fill, alpha), Stroke::new(ring, veil(k.accent, alpha)), StrokeKind::Inside);

            // Cells: visibility and icon on the left, rename and move on the right.
            let cw = 60.0 * s;
            let ch = r.height() - 12.0 * s;
            let cells = [
                Rect::from_center_size(pos2(r.min.x + 10.0 * s + cw / 2.0, r.center().y), vec2(cw, ch)),
                Rect::from_center_size(pos2(r.min.x + 18.0 * s + cw * 1.5, r.center().y), vec2(cw, ch)),
                Rect::from_center_size(pos2(r.max.x - 18.0 * s - cw * 1.5, r.center().y), vec2(cw, ch)),
                Rect::from_center_size(pos2(r.max.x - 10.0 * s - cw / 2.0, r.center().y), vec2(cw, ch)),
            ];
            if selected && !grabbed {
                clip.rect_stroke(cells[col], k.radius * s, Stroke::new(2.5 * s, veil(k.accent, alpha)), StrokeKind::Outside);
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
        let ok = match col {
            COL_SHOW => t("hint.toggle"),
            COL_ICON => t("hint.next_icon"),
            COL_RENAME => t("hint.rename"),
            _ if grabbed => t("hint.drop"),
            _ => t("hint.move"),
        };
        self.footer(p, rect, &[(t("key.ok"), ok), ("◀ ▶", t("hint.column")), (t("key.back"), t("hint.back"))], alpha);
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_rename(&mut self, ui: &Ui, p: &egui::Painter, screen: Rect, offset: Vec2, alpha: f32, text: &str, tv_name: &str) {
        let s = self.cfg.scale;
        let k = self.t;
        let pad = 28.0 * s;
        let want = vec2(820.0 * s, 300.0 * s);
        // Kept in the top half: LG's on-screen keyboard rises from the bottom.
        let size = want.min(screen.size() - Vec2::splat(112.0 * s));
        let rect = Rect::from_min_size(pos2(screen.center().x - size.x / 2.0, screen.min.y + 90.0 * s), size).translate(offset);
        self.panel(p, rect, alpha);
        self.header(p, rect, t("rename.title"), tv_name, alpha);

        let field = Rect::from_min_size(pos2(rect.min.x + pad, rect.min.y + 96.0 * s), vec2(rect.width() - pad * 2.0, 66.0 * s));
        p.rect(field, k.radius * 1.5 * s, veil(k.field, alpha), Stroke::new(2.0 * s, veil(k.accent, alpha)), StrokeKind::Inside);
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
        p.text(pos2(field.min.x, field.max.y + 20.0 * s), Align2::LEFT_TOP, t("rename.empty"), regular(17.0 * s), veil(k.text_dim, alpha));
        self.footer(p, rect, &[(t("key.ok"), t("hint.confirm")), (t("key.back"), t("hint.cancel"))], alpha);
    }
}
