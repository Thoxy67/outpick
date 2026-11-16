//! The Apps tile's list: the installed apps with their own icons, as a grid of cards or a
//! list of rows (Customize's Apps view), read from the TV each time it opens. Off (the Apps
//! switch), nothing here runs.

use super::widgets::{RIM, fit, gradient, keep_visible, walk};
use super::{App, Hit, Nav, Screen};
use crate::config::AppsView;
use crate::i18n::{t, tf};
use crate::icons;
use crate::model::{AppInfo, Apps, Command, TvState};
use crate::theme::{ink_on, regular, semibold, veil};
use egui::{Align2, Color32, Rect, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};

/// A row's icon and height (list), in points × scale: under the launch points' small
/// icons' 80 px at every size on a 1080p screen.
const ROW_ICON: f32 = 48.0;
const ROW_H: f32 = 72.0;
/// A card's size, its icon (larger than the small icons: their large variants are read)
/// and the space between cards (grid).
const CARD: Vec2 = vec2(176.0, 158.0);
const CARD_ICON: f32 = 88.0;
const GAP: f32 = 14.0;
/// Cards across at most, when the screen has room.
const MAX_COLS: usize = 6;

/// The card after `k` from `sel` among `n` in `cols` columns, in reading order: Left and
/// Right go on to the row before or after at a row's ends (around the grid's ends on a
/// press, stopping there while `held`); Up on the first row and Down on the last do
/// nothing; Down above an incomplete last row, in a column it does not have, goes to the
/// last card.
pub(super) fn grid_step(sel: usize, n: usize, cols: usize, k: &Nav, held: bool) -> usize {
    if n == 0 {
        return 0;
    }
    let (sel, cols) = (sel.min(n - 1), cols.max(1));
    match k {
        Nav::Left => walk(sel, n, -1, held),
        Nav::Right => walk(sel, n, 1, held),
        Nav::Up if sel >= cols => sel - cols,
        Nav::Down if sel + cols < n => sel + cols,
        Nav::Down if sel / cols < (n - 1) / cols => n - 1,
        _ => sel,
    }
}

/// `title` as the font can draw it: characters it lacks are left out, and when that leaves
/// less than half of it (another script, emoji), the app id stands in, which is plain ASCII.
pub(super) fn display_title(title: &str, id: &str, mut has: impl FnMut(char) -> bool) -> String {
    let kept: String = title.chars().filter(|c| c.is_whitespace() || has(*c)).collect();
    let kept = kept.split_whitespace().collect::<Vec<_>>().join(" ");
    let all = title.chars().filter(|c| !c.is_whitespace()).count();
    let drawn = kept.chars().filter(|c| !c.is_whitespace()).count();
    if drawn == 0 || drawn * 2 < all { id.to_string() } else { kept }
}

impl App {
    /// Opens the list from the Apps tile (OK, or Open in its options), and asks for it.
    pub(super) fn open_apps(&mut self) {
        if let Ok(mut st) = self.backend.state.lock() {
            st.apps = Some(Apps::Loading);
            st.app_launch = None;
        }
        let side = match self.cfg.apps_view {
            AppsView::Grid => CARD_ICON,
            AppsView::List => ROW_ICON,
        };
        let icon_px = (side * self.cfg.scale * self.ppp).ceil() as u32;
        let _ = self.backend.commands.send(Command::ListApps { icon_px });
        self.armed_at = None;
        self.app_launching = false;
        self.app_error = None;
        // Back returns to the Apps tile, not to the input in front.
        self.moved_since_open = true;
        self.screen = Screen::AppPick { sel: 0, placed: false };
    }

    /// The list opened at start (the TV's traced diagnostic launch), over the picker.
    #[cfg(feature = "webos")]
    pub fn open_apps_for_trace(&mut self) {
        self.open_apps();
    }

    /// Each new list: its titles as the font draws them, and room for its icons' textures
    /// (uploaded as their rows show). Leaving frees them, and the list.
    pub(super) fn sync_app_icons(&mut self, ctx: &egui::Context, st: &TvState) {
        if !matches!(self.screen, Screen::AppPick { .. }) {
            self.app_tex = Vec::new();
            self.app_titles = Vec::new();
            self.app_tex_seq = 0;
            if (st.apps.is_some() || st.app_launch.is_some())
                && let Ok(mut shared) = self.backend.state.lock()
            {
                shared.apps = None;
                shared.app_launch = None;
            }
            return;
        }
        let Some(Apps::Ready(list)) = &st.apps else { return };
        if self.app_tex_seq == st.apps_seq {
            return;
        }
        self.app_tex_seq = st.apps_seq;
        self.app_tex = vec![None; list.len()];
        let font = semibold(20.0);
        self.app_titles = ctx.fonts_mut(|f| list.iter().map(|a| display_title(&a.title, &a.id, |c| f.has_glyph(&font, c))).collect());
    }

    /// App `i`'s icon on the GPU, uploaded the first time its row or card shows.
    fn app_texture(&mut self, ctx: &egui::Context, i: usize, a: &AppInfo) -> Option<egui::TextureId> {
        let img = a.icon.as_ref()?;
        let slot = self.app_tex.get_mut(i)?;
        if slot.is_none() {
            let options = egui::TextureOptions { mipmap_mode: Some(egui::TextureFilter::Linear), ..egui::TextureOptions::LINEAR };
            let image = egui::ColorImage::from_rgba_unmultiplied([img.width, img.height], &img.rgba);
            *slot = Some(ctx.load_texture(format!("app-{i}"), image, options));
        }
        slot.as_ref().map(|t| t.id())
    }

    /// Frees the textures of rows or cards far from those shown (more than a screen away),
    /// so a long list holds three screens of icons at most.
    fn trim_app_textures(&mut self, first: usize, shown: usize) {
        let keep = first.saturating_sub(shown)..first + 2 * shown;
        for (i, slot) in self.app_tex.iter_mut().enumerate() {
            if !keep.contains(&i) {
                *slot = None;
            }
        }
    }

    /// Arrows choose (in two directions in the grid), OK opens the app and closes once the
    /// TV took it, Back / Source (and Left in the list) go back to the picker, on the Apps tile.
    pub(super) fn app_pick_key(&mut self, k: &Nav, st: &TvState, now: f64) {
        let Screen::AppPick { mut sel, mut placed } = self.screen else { return };
        let list: &[AppInfo] = match &st.apps {
            Some(Apps::Ready(l)) => l,
            _ => &[],
        };
        let n = list.len();
        let grid = self.cfg.apps_view == AppsView::Grid;
        match k {
            Nav::Back | Nav::Erase | Nav::Source => {
                self.screen = Screen::Picker;
                return;
            }
            // In the list, Left goes back too (not a held arrow, which only moves).
            Nav::Left if !grid && self.held => {}
            Nav::Left if !grid => {
                self.screen = Screen::Picker;
                return;
            }
            Nav::Up | Nav::Down | Nav::Left | Nav::Right if n > 0 => {
                let next = if grid {
                    grid_step(sel, n, self.app_cols, k, self.held)
                } else if matches!(k, Nav::Up | Nav::Down) {
                    walk(sel.min(n - 1), n, if *k == Nav::Up { -1 } else { 1 }, self.held)
                } else {
                    sel
                };
                if next != sel {
                    self.app_error = None;
                }
                sel = next;
                placed = true;
            }
            Nav::Ok if !self.app_launching => {
                if let Some(a) = list.get(sel) {
                    if a.params.is_none() && st.foreground.as_deref() == Some(a.id.as_str()) {
                        self.close(now); // already in front
                        return;
                    }
                    if let Ok(mut shared) = self.backend.state.lock() {
                        shared.app_launch = None;
                    }
                    let _ = self.backend.commands.send(Command::OpenApp { id: a.id.clone(), params: a.params.clone() });
                    self.app_launching = true;
                    self.app_error = None;
                }
            }
            _ => {}
        }
        self.screen = Screen::AppPick { sel, placed };
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_app_pick(&mut self, ui: &Ui, p: &egui::Painter, st: &TvState, screen: Rect, offset: Vec2, alpha: f32, mut sel: usize, placed: bool) {
        let s = self.cfg.scale;
        let k = self.t;
        let now = ui.input(|i| i.time);
        // The TV's answer to OK: gone once it took the app, else why not, staying open.
        if self.app_launching && self.closing.is_none() {
            match &st.app_launch {
                Some(Ok(())) => self.close(now),
                Some(Err(e)) => {
                    self.app_launching = false;
                    self.app_error = Some(e.clone());
                }
                None => {}
            }
        }
        let list: &[AppInfo] = match &st.apps {
            Some(Apps::Ready(l)) => l,
            _ => &[],
        };
        let n = list.len();
        // The app in front (its own launch point, not one with params, like an input pinned
        // to the home screen).
        let in_front = |a: &AppInfo| a.params.is_none() && st.foreground.as_deref() == Some(a.id.as_str());
        // Once the list comes: on the app in front, if it is one of them.
        let just_placed = !placed && matches!(st.apps, Some(Apps::Ready(_)));
        if just_placed {
            sel = list.iter().position(in_front).unwrap_or(0);
            self.screen = Screen::AppPick { sel, placed: true };
        }
        let sel = sel.min(n.saturating_sub(1));
        let grid = self.cfg.apps_view == AppsView::Grid;
        let pad = 28.0 * s;
        let head = pad + 38.0 * s + 18.0 * s;
        let foot = 72.0 * s;
        let (card, gap) = (CARD * s, GAP * s);
        let want = if grid {
            let cols = MAX_COLS.min(n.max(4));
            let rows = n.div_ceil(cols).max(2);
            vec2(cols as f32 * (card.x + gap) - gap + 2.0 * pad, head + rows as f32 * (card.y + gap) + 14.0 * s + foot)
        } else {
            vec2(780.0 * s, head + n.max(4) as f32 * ROW_H * s + foot + 8.0 * s)
        };
        let rect = self.panel_rect(screen, want).translate(offset);
        self.panel(p, rect, alpha);
        let subtitle = if self.app_launching {
            t("apps.opening").to_string()
        } else if let Some(e) = &self.app_error {
            tf("apps.launch_failed", &[("error", e)])
        } else {
            match n {
                0 => String::new(),
                1 => t("apps.count_one").to_string(),
                n => tf("apps.count", &[("n", &n.to_string())]),
            }
        };
        self.header(p, rect, None, t("apps.title"), &subtitle, alpha);
        let inset = if grid { pad } else { pad * 0.6 };
        let area = Rect::from_min_max(pos2(rect.min.x + inset, rect.min.y + head), pos2(rect.max.x - inset, rect.max.y - foot - 8.0 * s));

        // Loading, nothing found, or refused: say so; Back still goes back.
        let message = match &st.apps {
            Some(Apps::Ready(l)) if !l.is_empty() => None,
            Some(Apps::Ready(_)) => Some((t("apps.empty").to_string(), t("apps.empty_hint"))),
            Some(Apps::Failed(e)) => Some((tf("apps.error", &[("error", e)]), t("apps.error_hint"))),
            _ => Some((t("apps.loading").to_string(), "")),
        };
        if let Some((line1, line2)) = message {
            let c = area.center();
            let f1 = semibold(21.0 * s);
            p.text(pos2(c.x, c.y - 14.0 * s), Align2::CENTER_CENTER, fit(p, &line1, f1.clone(), area.width() - 40.0 * s), f1, veil(k.text, alpha));
            if !line2.is_empty() {
                let f2 = regular(17.0 * s);
                p.text(pos2(c.x, c.y + 18.0 * s), Align2::CENTER_CENTER, fit(p, line2, f2.clone(), area.width() - 40.0 * s), f2, veil(k.text_dim, alpha));
            }
            self.footer(p, rect, &[(t("key.back"), t("hint.back"))], alpha);
            return;
        }

        let ctx = ui.ctx().clone();
        // Slack around the rows for the selection ring: sideways, and above and below in the grid.
        let ring_room = if grid { 7.0 * s } else { 0.0 };
        let clip = p.with_clip_rect(area.expand2(vec2(8.0 * s, ring_room)));
        let icon_r = k.radius * 1.2 * s;
        // An app's icon in `ir`: its own on its launcher colour (white logos need it), else a box.
        let paint_icon = |app: &mut Self, i: usize, a: &AppInfo, ir: Rect, selected: bool| match app.app_texture(&ctx, i, a) {
            Some(tex) => {
                let [w, h] = a.icon.as_ref().map(|img| [img.width, img.height]).unwrap_or([1, 1]);
                let fitted = Rect::from_center_size(ir.center(), vec2(w as f32, h as f32) * (ir.width() / w.max(h).max(1) as f32));
                if let Some([cr, cg, cb]) = a.color {
                    clip.rect_filled(fitted, icon_r, veil(Color32::from_rgb(cr, cg, cb), alpha));
                }
                let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
                clip.add(egui::epaint::RectShape::filled(fitted, icon_r, veil(Color32::WHITE, alpha)).with_texture(tex, uv));
            }
            None => {
                clip.rect_filled(ir, icon_r, veil(k.field, alpha));
                icons::paint(&clip, "box", ir.shrink(ir.width() * 0.23), veil(if selected { k.accent_text } else { k.icon }, alpha));
            }
        };
        let title_of = |app: &Self, i: usize, a: &AppInfo| app.app_titles.get(i).cloned().unwrap_or_else(|| a.title.clone());

        let (overflow, scroll, first, shown) = if grid {
            let cols = (((area.width() + gap) / (card.x + gap)).floor() as usize).clamp(1, MAX_COLS);
            self.app_cols = cols;
            let rows = n.div_ceil(cols);
            let pitch = card.y + gap;
            let visible = (((area.height() - ring_room + gap) / pitch).floor() as usize).max(1);
            let first_row = keep_visible(sel / cols, visible, rows);
            let y0 = self.anim(ui, "apps-grid-scroll", first_row as f32 * pitch, if just_placed { 0.0 } else { 0.15 });
            let left = area.center().x - (cols as f32 * (card.x + gap) - gap) / 2.0;
            let top = area.min.y + ring_room;
            let cell = |i: usize| Rect::from_min_size(pos2(left + (i % cols) as f32 * (card.x + gap), top + (i / cols) as f32 * pitch - y0), card);
            let radius = k.radius_lg * 1.4 * s;
            for (i, a) in list.iter().enumerate() {
                let c = cell(i);
                if c.max.y < area.min.y - pitch || c.min.y > area.max.y + pitch {
                    continue;
                }
                self.add_hit(c.intersect(area), Hit::PickCell(i));
                let selected = i == sel;
                clip.rect(c, radius, veil(if selected { k.accent_soft } else { k.field }, alpha * 0.8), self.border(1.0, veil(k.field_border, alpha)), StrokeKind::Inside);
                self.rim(&clip, c, radius, alpha * RIM);
                let ir = Rect::from_center_size(pos2(c.center().x, c.min.y + 14.0 * s + CARD_ICON * s / 2.0), Vec2::splat(CARD_ICON * s));
                paint_icon(self, i, a, ir, selected);
                let font = semibold(17.0 * s);
                let title = fit(p, &title_of(self, i, a), font.clone(), card.x - 20.0 * s);
                clip.text(pos2(c.center().x, c.max.y - 25.0 * s), Align2::CENTER_CENTER, title, font, veil(if selected { k.text } else { k.text_dim }, alpha));
                // The app in front: a tick in its corner, as the options tick the receiver's input.
                if in_front(a) {
                    let at = pos2(c.max.x - 19.0 * s, c.min.y + 19.0 * s);
                    clip.circle_filled(at, 11.0 * s, veil(k.accent, alpha));
                    icons::paint(&clip, "check", Rect::from_center_size(at, Vec2::splat(14.0 * s)), veil(ink_on(k.accent), alpha));
                }
            }
            // The ring glides from card to card, as the icon picker's does.
            let target = cell(sel);
            let at = pos2(self.anim(ui, "apps-ring-x", target.min.x, 0.12), self.anim(ui, "apps-ring-y", target.min.y, if just_placed { 0.0 } else { 0.12 }));
            let ring = Rect::from_min_size(at, card).expand(4.0 * s);
            clip.rect_stroke(ring, radius + 4.0 * s, Stroke::new(3.0 * s, veil(k.ring, alpha)), StrokeKind::Outside);
            (rows.saturating_sub(visible) as f32 * pitch, y0, first_row * cols, visible * cols)
        } else {
            self.app_cols = 1;
            let row_h = ROW_H * s;
            let visible = (area.height() / row_h).floor().max(1.0) as usize;
            let first = keep_visible(sel, visible, n);
            let y0 = self.anim(ui, "apps-scroll", first as f32 * row_h, if just_placed { 0.0 } else { 0.15 });
            for (i, a) in list.iter().enumerate() {
                let r = Rect::from_min_size(pos2(area.min.x, area.min.y + i as f32 * row_h - y0), vec2(area.width(), row_h - 8.0 * s));
                if r.max.y < area.min.y - row_h || r.min.y > area.max.y + row_h {
                    continue;
                }
                self.add_hit(r.intersect(area), Hit::OptionRow(i));
                let selected = i == sel;
                if selected {
                    self.selected_row(&clip, r, alpha);
                }
                let ir = Rect::from_center_size(pos2(r.min.x + 16.0 * s + ROW_ICON * s / 2.0, r.center().y), Vec2::splat(ROW_ICON * s));
                paint_icon(self, i, a, ir, selected);
                // The app in front: ticked, as the receiver list ticks the one in use.
                let current = in_front(a);
                let x = ir.max.x + 20.0 * s;
                let maxw = r.max.x - x - if current { 56.0 * s } else { 16.0 * s };
                let font = semibold(21.0 * s);
                clip.text(pos2(x, r.center().y), Align2::LEFT_CENTER, fit(p, &title_of(self, i, a), font.clone(), maxw), font, veil(k.text, alpha));
                if current {
                    let cr = Rect::from_center_size(pos2(r.max.x - 28.0 * s, r.center().y), Vec2::splat(24.0 * s));
                    icons::paint(&clip, "check", cr, veil(if selected { k.accent_text } else { k.accent }, alpha));
                }
            }
            (n.saturating_sub(visible) as f32 * row_h, y0, first, visible)
        };
        self.trim_app_textures(first, shown);

        // What the scroll cuts fades out at that edge, as the options' do; not on glass, where
        // the band shows as a straight strip across the see-through panel (the clip cuts there).
        if overflow > 0.0 && self.cfg.panel_opacity >= 0.85 && !k.glass {
            let outer = area.expand2(vec2(8.0 * s, ring_room));
            let edge = 40.0 * s;
            let (fade_c, clear) = (veil(k.card, alpha * self.cfg.panel_opacity), Color32::TRANSPARENT);
            if scroll > 0.5 {
                gradient(p, Rect::from_min_max(outer.min, pos2(outer.max.x, outer.min.y + edge)), fade_c, clear, true);
            }
            if scroll < overflow - 0.5 {
                gradient(p, Rect::from_min_max(pos2(outer.min.x, outer.max.y - edge), outer.max), clear, fade_c, true);
            }
        }
        let arrows = if grid { "◀ ▲ ▼ ▶" } else { "▲ ▼" };
        self.footer(p, rect, &[(arrows, t("hint.choose")), (t("key.ok"), t("hint.open")), (t("key.back"), t("hint.back"))], alpha);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_arrows() {
        // 10 cards in 4 columns: rows 0-3, 4-7, 8-9.
        let go = |sel: usize, k: Nav| grid_step(sel, 10, 4, &k, false);
        assert_eq!(go(0, Nav::Right), 1);
        // Reading order across rows' ends, around the grid's ends on a press.
        assert_eq!(go(3, Nav::Right), 4);
        assert_eq!(go(4, Nav::Left), 3);
        assert_eq!(go(9, Nav::Right), 0);
        assert_eq!(go(0, Nav::Left), 9);
        // Held: stops at the grid's ends, still crosses rows.
        assert_eq!(grid_step(9, 10, 4, &Nav::Right, true), 9);
        assert_eq!(grid_step(0, 10, 4, &Nav::Left, true), 0);
        assert_eq!(grid_step(3, 10, 4, &Nav::Right, true), 4);
        // Up on the first row, Down on the last: nothing.
        assert_eq!(go(2, Nav::Up), 2);
        assert_eq!(go(8, Nav::Down), 8);
        assert_eq!(go(9, Nav::Down), 9);
        assert_eq!(go(6, Nav::Up), 2);
        assert_eq!(go(1, Nav::Down), 5);
        // Above the incomplete last row: a column it has goes straight down, one it lacks
        // to the last card.
        assert_eq!(go(5, Nav::Down), 9);
        assert_eq!(go(6, Nav::Down), 9);
        assert_eq!(go(7, Nav::Down), 9);
        assert_eq!(go(4, Nav::Down), 8);
        // From two rows up, Down goes one row at a time.
        assert_eq!(go(3, Nav::Down), 7);
        // Odd sizes: one card, an empty grid, a selection out of range, one column.
        assert_eq!(grid_step(0, 1, 4, &Nav::Down, false), 0);
        assert_eq!(grid_step(0, 1, 4, &Nav::Right, false), 0);
        assert_eq!(grid_step(5, 0, 4, &Nav::Down, false), 0);
        assert_eq!(grid_step(42, 10, 4, &Nav::Up, false), 5);
        assert_eq!(grid_step(1, 3, 1, &Nav::Down, false), 2);
        assert_eq!(grid_step(1, 3, 0, &Nav::Up, false), 0);
        // OK and the rest do not move.
        assert_eq!(go(5, Nav::Ok), 5);
    }

    #[test]
    fn titles_the_font_cannot_draw() {
        let latin = |c: char| (c as u32) < 0x180;
        assert_eq!(display_title("Netflix", "netflix", latin), "Netflix");
        assert_eq!(display_title("Lecteur multimédia", "x", latin), "Lecteur multimédia");
        // A few missing characters are left out (and their spaces closed up).
        assert_eq!(display_title("Movies \u{1f3ac} Plus", "m", latin), "Movies Plus");
        // Mostly another script: the id.
        assert_eq!(display_title("\u{65e5}\u{672c}\u{8a9e}\u{30c6}\u{30ec}\u{30d3} TV", "jp.tv.app", latin), "jp.tv.app");
        assert_eq!(display_title("\u{1f3ac}\u{1f3ae}", "emoji.app", latin), "emoji.app");
        assert_eq!(display_title("", "empty.app", latin), "empty.app");
        assert_eq!(display_title("   ", "blank.app", latin), "blank.app");
    }
}
