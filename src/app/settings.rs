//! Customize: its options, their keys and their rows.

use super::pickers::{POSITION_GRID, layout_glyph};
use super::sources::COL_SHOW;
use super::widgets::{cycle, fit, step, text_width};
use super::{App, Hit, Nav, Screen};
use crate::config::{Config, Corners, Layout, Naming, Position};
use crate::i18n::{Language, t, tf};
use crate::icons;
use crate::model::{Command, KeyOwner, TvState};
use crate::theme::{Accent, ThemeKind, ink_on, medium, mix, regular, semibold, veil};
use egui::{Align2, Color32, Rect, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};

/// Options of Customize.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Item {
    Language,
    Theme,
    Accent,
    Corners,
    Size,
    ShowPanel,
    Opacity,
    Dim,
    Animations,
    Layout,
    Position,
    ShowIcons,
    ShowHeader,
    ShowHints,
    ShowDisconnected,
    ShowDevices,
    ShowLiveTv,
    Naming,
    ShowPreviews,
    PreviewFps,
    CaptureAccess,
    ShowHome,
    ShowUsb,
    ShowMediaServers,
    AutoClose,
    ClockShow,
    ClockStyle,
    ClockSize,
    Clock24h,
    ClockSeconds,
    ClockPlace,
    SourceKey,
    SwitchDelay,
    MultikeyOpens,
    RestoreLg,
    Sources,
    Reset,
}

/// Customize, section by section: (section title key, its options).
const SECTIONS: &[(&str, &[Item])] = &[
    ("section.general", &[Item::Language]),
    ("section.appearance", &[Item::Theme, Item::Accent, Item::Corners, Item::Size, Item::ShowPanel, Item::Opacity, Item::Dim, Item::Animations]),
    ("section.layout", &[Item::Layout, Item::Position, Item::ShowIcons, Item::ShowHeader, Item::ShowHints]),
    ("section.content", &[Item::Naming, Item::ShowPreviews, Item::PreviewFps, Item::CaptureAccess, Item::ShowHome, Item::ShowDisconnected, Item::ShowDevices, Item::ShowLiveTv, Item::ShowUsb, Item::ShowMediaServers]),
    ("section.behaviour", &[Item::AutoClose]),
    ("section.clock", &[Item::ClockShow, Item::ClockStyle, Item::ClockSize, Item::Clock24h, Item::ClockSeconds, Item::ClockPlace]),
    ("section.remote", &[Item::SourceKey, Item::SwitchDelay, Item::MultikeyOpens, Item::RestoreLg]),
    ("section.sources", &[Item::Sources, Item::Reset]),
];

/// The selectable options in order (the selection indexes this).
fn items() -> Vec<Item> {
    SECTIONS.iter().flat_map(|(_, items)| items.iter().copied()).collect()
}

fn row_kind_of(item: Item, cfg: &Config) -> RowKind {
    match item {
        Item::Size => RowKind::Slider(0.7, 1.5),
        Item::Opacity => RowKind::Slider(0.4, 1.0),
        Item::Dim => RowKind::Slider(0.0, 0.8),
        Item::ClockSize => RowKind::Slider(0.4, 3.0),
        Item::ShowPanel => RowKind::Toggle(cfg.show_panel),
        Item::ClockShow => RowKind::Toggle(cfg.clock.show),
        Item::Clock24h => RowKind::Toggle(cfg.clock.hour24),
        Item::ClockSeconds => RowKind::Toggle(cfg.clock.seconds),
        Item::ClockPlace => RowKind::Page,
        Item::Animations => RowKind::Toggle(cfg.animations),
        Item::ShowIcons => RowKind::Toggle(cfg.show_icons),
        Item::ShowHeader => RowKind::Toggle(cfg.show_header),
        Item::ShowHints => RowKind::Toggle(cfg.show_hints),
        Item::ShowHome => RowKind::Toggle(cfg.show_home),
        Item::ShowPreviews => RowKind::Toggle(cfg.show_previews),
        Item::MultikeyOpens => RowKind::Toggle(cfg.multikey_opens),
        Item::ShowDisconnected => RowKind::Toggle(cfg.show_disconnected),
        Item::ShowDevices => RowKind::Toggle(cfg.show_device_names),
        Item::ShowLiveTv => RowKind::Toggle(cfg.show_live_tv),
        Item::ShowUsb => RowKind::Toggle(cfg.show_usb),
        Item::ShowMediaServers => RowKind::Toggle(cfg.show_media_servers),
        Item::Sources => RowKind::Page,
        Item::Reset | Item::RestoreLg | Item::CaptureAccess => RowKind::Button,
        _ => RowKind::Choice,
    }
}

pub(super) fn item_index(item: Item) -> usize {
    items().iter().position(|&i| i == item).unwrap_or(0)
}

/// How a Customize row is shown and used.
#[derive(Clone, Copy, PartialEq)]
enum RowKind {
    /// ‹ value ›, changed with Left/Right (OK steps forward too).
    Choice,
    /// A gauge between two bounds, changed with Left/Right.
    Slider(f32, f32),
    /// An on/off switch, flipped with OK (or Left/Right).
    Toggle(bool),
    /// Opens another screen with OK.
    Page,
    /// Does something once with OK.
    Button,
}

/// A drawn row of Customize: a section title, or option `n` of `items()`.
#[derive(Clone, Copy)]
enum SettingsRow {
    Section(&'static str),
    Option(usize),
}

fn settings_rows() -> Vec<SettingsRow> {
    let mut rows = Vec::new();
    let mut n = 0;
    for (title, items) in SECTIONS {
        rows.push(SettingsRow::Section(title));
        for _ in *items {
            rows.push(SettingsRow::Option(n));
            n += 1;
        }
    }
    rows
}

impl App {
    pub(super) fn settings_key(&mut self, k: &Nav, ctx: &egui::Context, st: &TvState) {
        let Screen::Settings { mut sel, mut confirm_reset } = self.screen else { return };
        let all = items();
        let n = all.len();
        match k {
            Nav::Up => {
                sel = (sel + n - 1) % n;
                confirm_reset = false;
            }
            Nav::Down => {
                sel = (sel + 1) % n;
                confirm_reset = false;
            }
            Nav::Left | Nav::Right | Nav::Ok if all[sel] == Item::SourceKey => self.toggle_takeover(st),
            Nav::Ok | Nav::Left | Nav::Right if all[sel] == Item::MultikeyOpens => {
                self.adjust(Item::MultikeyOpens, 1);
                // The root helper reads the keys from the saved config.
                self.persist();
                let on = self.cfg.multikey_opens || matches!(st.key_owner, KeyOwner::Picker);
                if !matches!(st.key_owner, KeyOwner::Unsupported) {
                    let _ = self.backend.commands.send(Command::Takeover(on));
                }
            }
            // Buttons and pages do nothing with Left/Right.
            Nav::Left | Nav::Right if matches!(row_kind_of(all[sel], &self.cfg), RowKind::Button | RowKind::Page) => {}
            Nav::Left | Nav::Right => {
                self.adjust(all[sel], if *k == Nav::Left { -1 } else { 1 });
                self.retheme(ctx);
            }
            Nav::Ok if all[sel] == Item::ClockPlace => {
                self.screen = Screen::ClockPlace { orig: self.cfg.clock };
                return;
            }
            Nav::Ok if all[sel] == Item::Layout => {
                let at = Layout::ALL.iter().position(|l| *l == self.cfg.layout).unwrap_or(0);
                self.screen = Screen::StylePick { sel: at };
                return;
            }
            Nav::Ok if all[sel] == Item::Position => {
                let at = POSITION_GRID.iter().position(|p| *p == self.cfg.position).unwrap_or(7);
                self.screen = Screen::PositionPick { sel: at };
                return;
            }
            Nav::Ok if all[sel] == Item::CaptureAccess => {
                if self.button_caption(Item::CaptureAccess, false, st).1 {
                    let _ = self.backend.commands.send(Command::CaptureAccess(st.capture_access != Some(true)));
                }
            }
            Nav::Ok if all[sel] == Item::RestoreLg => {
                if self.button_caption(Item::RestoreLg, false, st).1 {
                    let _ = self.backend.commands.send(Command::Takeover(false));
                }
            }
            Nav::Ok => match all[sel] {
                Item::Sources => {
                    self.persist();
                    self.screen = Screen::Sources { sel: 0, col: COL_SHOW, grabbed: false };
                    return;
                }
                Item::Reset if confirm_reset => {
                    let keep = (self.cfg.order.clone(), self.cfg.sources.clone(), self.cfg.language);
                    self.cfg = Config { order: keep.0, sources: keep.1, language: keep.2, ..Config::default() };
                    self.retheme(ctx);
                    confirm_reset = false;
                }
                Item::Reset => confirm_reset = true,
                item => {
                    self.adjust(item, 1);
                    self.retheme(ctx);
                }
            },
            Nav::Back | Nav::Erase | Nav::Source => {
                self.persist();
                self.screen = Screen::Picker;
                return;
            }
            _ => {}
        }
        self.screen = Screen::Settings { sel, confirm_reset };
    }

    /// Hands the Source key to this picker, or back to LG, from what is detected now.
    fn toggle_takeover(&self, st: &TvState) {
        if st.takeover_busy || matches!(st.key_owner, KeyOwner::Unsupported) {
            return;
        }
        let on = !matches!(st.key_owner, KeyOwner::Picker);
        let _ = self.backend.commands.send(Command::Takeover(on));
    }

    /// Changes `item` one step in `dir`.
    fn adjust(&mut self, item: Item, dir: i32) {
        let c = &mut self.cfg;
        match item {
            Item::Language => c.language = cycle(&Language::ALL, c.language, dir),
            Item::Theme => c.theme = cycle(&ThemeKind::ALL, c.theme, dir),
            Item::Accent => c.accent = cycle(&Accent::ALL, c.accent, dir),
            Item::Layout => c.layout = cycle(&Layout::ALL, c.layout, dir),
            Item::Position => c.position = cycle(&Position::ALL, c.position, dir),
            Item::Corners => c.corners = cycle(&Corners::ALL, c.corners, dir),
            Item::Animations => c.animations = !c.animations,
            Item::ShowIcons => c.show_icons = !c.show_icons,
            Item::ShowHeader => c.show_header = !c.show_header,
            Item::ShowHints => c.show_hints = !c.show_hints,
            Item::Size => c.scale = step(c.scale, 0.1, dir),
            Item::Opacity => c.panel_opacity = step(c.panel_opacity, 0.05, dir),
            Item::Dim => c.dim = step(c.dim, 0.05, dir),
            Item::ShowPanel => c.show_panel = !c.show_panel,
            Item::ClockShow => c.clock.show = !c.clock.show,
            Item::ClockStyle => c.clock.style = cycle(&crate::config::ClockStyle::ALL, c.clock.style, dir),
            Item::ClockSize => c.clock.size = step(c.clock.size, 0.1, dir),
            Item::Clock24h => c.clock.hour24 = !c.clock.hour24,
            Item::ClockSeconds => c.clock.seconds = !c.clock.seconds,
            Item::ClockPlace => {}
            Item::ShowDisconnected => c.show_disconnected = !c.show_disconnected,
            Item::ShowDevices => c.show_device_names = !c.show_device_names,
            Item::ShowLiveTv => c.show_live_tv = !c.show_live_tv,
            Item::Naming => c.naming = cycle(&Naming::ALL, c.naming, dir),
            Item::ShowPreviews => c.show_previews = !c.show_previews,
            Item::PreviewFps => c.preview_fps = cycle(&[1, 2, 4], c.preview_fps, dir),
            Item::ShowHome => c.show_home = !c.show_home,
            Item::ShowUsb => c.show_usb = !c.show_usb,
            Item::ShowMediaServers => c.show_media_servers = !c.show_media_servers,
            Item::AutoClose => c.auto_close_secs = cycle(&[0, 5, 10, 15, 30, 60], c.auto_close_secs, dir),
            Item::SwitchDelay => c.switch_delay_secs = cycle(&[1, 2, 3, 4, 5, 7, 10], c.switch_delay_secs, dir),
            Item::MultikeyOpens => c.multikey_opens = !c.multikey_opens,
            Item::Sources | Item::Reset | Item::SourceKey | Item::RestoreLg | Item::CaptureAccess => {}
        }
    }

    /// Icon, label and value of a Customize row.
    /// The button's caption, and whether pressing it does anything now.
    fn button_caption(&self, item: Item, confirm_reset: bool, st: &TvState) -> (&'static str, bool) {
        match item {
            Item::Reset => (t(if confirm_reset { "settings.reset_confirm_button" } else { "settings.reset_button" }), true),
            Item::CaptureAccess => match st.capture_access {
                Some(true) => (t("capture.restore_button"), !st.takeover_busy),
                Some(false) => (t("capture.allow_button"), !st.takeover_busy),
                None => (t("capture.allow_button"), false),
            },
            _ => (
                t("settings.restore_button"),
                !st.takeover_busy && !matches!(st.key_owner, KeyOwner::Lg | KeyOwner::Unsupported),
            ),
        }
    }

    fn owner_label(owner: &KeyOwner) -> String {
        match owner {
            KeyOwner::Picker => t("remote.this_picker").into(),
            KeyOwner::Lg => t("remote.lg").into(),
            KeyOwner::Other(name) => tf("remote.other", &[("name", name)]),
            KeyOwner::Broken(d) => tf("remote.broken", &[("detail", d)]),
            KeyOwner::Unsupported => t("remote.unsupported").into(),
            KeyOwner::Pending | KeyOwner::Unknown(_) => t("remote.unknown").into(),
        }
    }

    fn setting_row(&self, item: Item, confirm_reset: bool, st: &TvState) -> (&'static str, &'static str, String) {
        let c = &self.cfg;
        let onoff = |b: bool| t(if b { "value.on" } else { "value.off" }).to_string();
        let secs = |v: u32, off: &'static str| if v == 0 { t(off).to_string() } else { tf("value.seconds", &[("n", &v.to_string())]) };
        let lang = |l: Language| match l {
            Language::Auto => t("value.auto"),
            Language::English => t("language.english"),
            Language::French => t("language.french"),
        };
        match item {
            Item::Language => ("type", t("settings.language"), lang(c.language).into()),
            Item::Theme => ("palette", t("settings.theme"), c.theme.label().into()),
            Item::Accent => ("palette", t("settings.accent"), c.accent.label().into()),
            Item::Layout => ("layout-grid", t("settings.layout"), c.layout.label().into()),
            Item::Position => ("maximize-2", t("settings.position"), c.position.label().into()),
            Item::Corners => ("layout-grid", t("settings.corners"), c.corners.label().into()),
            Item::Animations => ("circle-check", t("settings.animations"), onoff(c.animations)),
            Item::ShowIcons => ("eye", t("settings.show_icons"), onoff(c.show_icons)),
            Item::ShowHeader => ("type", t("settings.show_header"), onoff(c.show_header)),
            Item::ShowHints => ("list", t("settings.show_hints"), onoff(c.show_hints)),
            Item::Size => ("type", t("settings.size"), format!("{:.0} %", c.scale * 100.0)),
            Item::Opacity => ("sun", t("settings.opacity"), format!("{:.0} %", c.panel_opacity * 100.0)),
            Item::Dim => ("moon", t("settings.dim"), format!("{:.0} %", c.dim * 100.0)),
            Item::ShowPanel => ("layout-grid", t("settings.show_panel"), onoff(c.show_panel)),
            Item::ClockShow => ("timer", t("settings.clock_show"), onoff(c.clock.show)),
            Item::ClockStyle => ("timer", t("settings.clock_style"), c.clock.style.label().into()),
            Item::ClockSize => ("type", t("settings.clock_size"), format!("{:.0} %", c.clock.size * 100.0)),
            Item::Clock24h => ("timer", t("settings.clock_24h"), onoff(c.clock.hour24)),
            Item::ClockSeconds => ("timer", t("settings.clock_seconds"), onoff(c.clock.seconds)),
            Item::ClockPlace => ("maximize-2", t("settings.clock_place"), t("settings.clock_place_value").into()),
            Item::ShowDisconnected => ("cable", t("settings.show_disconnected"), onoff(c.show_disconnected)),
            Item::ShowDevices => ("tv", t("settings.show_devices"), onoff(c.show_device_names)),
            Item::ShowLiveTv => ("radio-tower", t("settings.show_live_tv"), onoff(c.show_live_tv)),
            Item::Naming => ("type", t("settings.naming"), c.naming.label().into()),
            Item::ShowPreviews => (
                "monitor",
                t("settings.show_previews"),
                if st.preview_supported == Some(false) { t("value.unavailable").into() } else { onoff(c.show_previews) },
            ),
            Item::PreviewFps => ("timer", t("settings.preview_fps"), tf("value.fps", &[("n", &c.preview_fps.to_string())])),
            Item::CaptureAccess => (
                "monitor",
                t("settings.capture_access"),
                match st.capture_access {
                    _ if st.takeover_busy => t("remote.applying").into(),
                    Some(true) => t("capture.granted").into(),
                    // Capture works now, but LG can switch it off (DRM, some apps):
                    // allowing keeps the previews working then too.
                    Some(false) if st.preview_supported == Some(true) => t("capture.works").into(),
                    Some(false) => t("capture.blocked").into(),
                    None => t("capture.not_needed").into(),
                },
            ),
            Item::ShowHome => ("house", t("settings.show_home"), onoff(c.show_home)),
            Item::ShowUsb => ("usb", t("settings.show_usb"), onoff(c.show_usb)),
            Item::ShowMediaServers => ("server", t("settings.show_media_servers"), onoff(c.show_media_servers)),
            Item::AutoClose => ("timer", t("settings.auto_close"), secs(c.auto_close_secs, "value.never")),
            Item::SwitchDelay => ("timer", t("settings.switch_delay"), format!("{} s", c.switch_delay_secs)),
            Item::MultikeyOpens => ("menu", t("settings.multikey_opens"), onoff(c.multikey_opens)),
            Item::SourceKey => (
                "tv",
                t("settings.source_key"),
                if st.takeover_busy { t("remote.applying").into() } else { Self::owner_label(&st.key_owner) },
            ),
            Item::RestoreLg => (
                "rotate-ccw",
                t("settings.restore_lg"),
                match &st.takeover_error {
                    Some(e) => tf("remote.failed", &[("error", e)]),
                    None => tf("remote.detected", &[("owner", &Self::owner_label(&st.key_owner))]),
                },
            ),
            Item::Sources => ("rows-3", t("settings.sources"), t("settings.sources_value").into()),
            Item::Reset => (
                "rotate-ccw",
                t("settings.reset"),
                t(if confirm_reset { "settings.reset_confirm" } else { "settings.reset_value" }).into(),
            ),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_settings(&mut self, ui: &Ui, p: &egui::Painter, st: &TvState, screen: Rect, offset: Vec2, alpha: f32, sel: usize, confirm_reset: bool) {
        let s = self.cfg.scale;
        let k = self.t;
        let pad = 28.0 * s;
        let head = pad + 38.0 * s + 18.0 * s;
        let foot = 72.0 * s;
        let row_h = 52.0 * s;
        let sec_h = 44.0 * s;
        let all = items();
        let rows = settings_rows();
        let height_of = |r: &SettingsRow| match r {
            SettingsRow::Section(_) => sec_h,
            SettingsRow::Option(_) => row_h,
        };
        let total: f32 = rows.iter().map(height_of).sum();
        let want = vec2(860.0 * s, head + total + foot + 8.0 * s);
        let rect = self.panel_rect(screen, want).translate(offset);
        self.panel(p, rect, alpha);
        self.header(p, rect, t("settings.title"), t("settings.subtitle"), alpha);

        let area = Rect::from_min_max(pos2(rect.min.x + pad * 0.6, rect.min.y + head), pos2(rect.max.x - pad * 0.6, rect.max.y - foot));
        // Row tops; scroll so the selected option sits mid-list (its section title too, near the top).
        let mut tops = Vec::with_capacity(rows.len());
        let mut y = 0.0;
        for r in &rows {
            tops.push(y);
            y += height_of(r);
        }
        let sel_row = rows.iter().position(|r| matches!(r, SettingsRow::Option(i) if *i == sel)).unwrap_or(0);
        let want_scroll = if total <= area.height() {
            0.0
        } else {
            (tops[sel_row] + row_h / 2.0 - area.height() / 2.0).clamp(0.0, total - area.height())
        };
        // Only sideways slack (for the selection ring): rows never spill over the title or hints.
        let clip = p.with_clip_rect(area.expand2(vec2(6.0 * s, 0.0)));
        let y0 = self.anim(ui, "settings-scroll", want_scroll, 0.15);
        for (ri, row) in rows.iter().enumerate() {
            let top = area.min.y + tops[ri] - y0;
            if top + row_h < area.min.y - row_h || top > area.max.y + row_h {
                continue;
            }
            let (i, item) = match *row {
                SettingsRow::Section(key) => {
                    // Section title: small caps label with a hairline, not selectable.
                    let ty = top + sec_h - 14.0 * s;
                    let label = t(key).to_uppercase();
                    let lr = clip.text(pos2(area.min.x + 16.0 * s, ty), Align2::LEFT_CENTER, label, semibold(14.0 * s), veil(k.text_faint, alpha));
                    let line = Rect::from_min_max(pos2(lr.max.x + 12.0 * s, ty - 0.5 * s), pos2(area.max.x - 14.0 * s, ty + 0.5 * s));
                    clip.rect_filled(line, 0.0, veil(k.separator, alpha));
                    continue;
                }
                SettingsRow::Option(i) => (i, all[i]),
            };
            let r = Rect::from_min_size(pos2(area.min.x, top), vec2(area.width(), row_h - 6.0 * s));
            self.add_hit(r.intersect(area), Hit::SettingRow(i));
            let selected = i == sel;
            if selected {
                self.selected_row(&clip, r, alpha);
            }
            let (icon, label, value) = self.setting_row(item, confirm_reset, st);
            let ir = Rect::from_center_size(pos2(r.min.x + 30.0 * s, r.center().y), Vec2::splat(24.0 * s));
            icons::paint(&clip, icon, ir, veil(if selected { k.accent_text } else { k.icon }, alpha));
            clip.text(pos2(r.min.x + 58.0 * s, r.center().y), Align2::LEFT_CENTER, label, medium(20.0 * s), veil(k.text, alpha));

            let chev = 22.0 * s;
            let right = r.max.x - 14.0 * s;
            let vfont = medium(19.0 * s);
            let vcol = if selected { k.accent_text } else { k.text_dim };
            let cy = r.center().y;
            // ‹ value › for rows changed with Left/Right; the right chevron's place is
            // kept so the value never moves when the row is selected.
            let arrows = |left_edge: f32| {
                if selected {
                    let cl = Rect::from_center_size(pos2(left_edge - 8.0 * s - chev / 2.0, cy), Vec2::splat(chev));
                    let cr = Rect::from_center_size(pos2(right - chev / 2.0, cy), Vec2::splat(chev));
                    icons::paint(&clip, "chevron-left", cl, veil(k.accent_text, alpha));
                    icons::paint(&clip, "chevron-right", cr, veil(k.accent_text, alpha));
                    // Generous targets: a pointer is less precise than a mouse.
                    self.add_hit(cl.expand(10.0 * s).intersect(area), Hit::SettingPrev(i));
                    self.add_hit(cr.expand(10.0 * s).intersect(area), Hit::SettingNext(i));
                }
            };
            match row_kind_of(item, &self.cfg) {
                RowKind::Choice if matches!(item, Item::Layout | Item::Position) => {
                    // Pictures rather than words: what each choice looks like on screen.
                    let value_right = right - chev - 8.0 * s;
                    let strip_left = if item == Item::Layout {
                        self.layout_strip(&clip, value_right, cy, selected, alpha)
                    } else {
                        self.position_picture(&clip, value_right, cy, selected, alpha)
                    };
                    let tr = clip.text(pos2(strip_left - 14.0 * s, cy), Align2::RIGHT_CENTER, &value, vfont, veil(vcol, alpha));
                    arrows(tr.min.x);
                }
                RowKind::Choice => {
                    let value_right = right - chev - 8.0 * s;
                    let vw = text_width(p, &value, vfont.clone());
                    clip.text(pos2(value_right, cy), Align2::RIGHT_CENTER, &value, vfont, veil(vcol, alpha));
                    let mut left_edge = value_right - vw;
                    if item == Item::Accent {
                        clip.circle_filled(pos2(left_edge - 18.0 * s, cy), 8.0 * s, veil(self.t.accent, alpha));
                        left_edge -= 30.0 * s;
                    }
                    arrows(left_edge);
                }
                RowKind::Slider(min, max) => {
                    // A gauge with its value, adjusted with Left/Right.
                    let v = match item {
                        Item::Size => self.cfg.scale,
                        Item::ClockSize => self.cfg.clock.size,
                        Item::Opacity => self.cfg.panel_opacity,
                        _ => self.cfg.dim,
                    };
                    let frac = ((v - min) / (max - min)).clamp(0.0, 1.0);
                    let value_right = right - chev - 8.0 * s;
                    let vw = 64.0 * s;
                    clip.text(pos2(value_right, cy), Align2::RIGHT_CENTER, &value, vfont, veil(vcol, alpha));
                    let track = Rect::from_min_max(pos2(value_right - vw - 160.0 * s, cy - 3.0 * s), pos2(value_right - vw, cy + 3.0 * s));
                    let on = if selected { k.accent } else { k.text_dim };
                    clip.rect_filled(track, 3.0 * s, veil(k.field_border, alpha));
                    let fill = Rect::from_min_max(track.min, pos2(track.min.x + track.width() * frac, track.max.y));
                    clip.rect_filled(fill, 3.0 * s, veil(on, alpha));
                    clip.circle_filled(pos2(fill.max.x, cy), 8.0 * s, veil(on, alpha));
                    arrows(track.min.x - 10.0 * s);
                }
                RowKind::Toggle(on) => {
                    // A switch (Photocraft's toggle), eased between its two states.
                    let fade = if self.cfg.animations { 0.15 } else { 0.0 };
                    let t_on = ui.ctx().animate_bool_with_time(egui::Id::unique(("lg-source-switch", i)), on, fade);
                    let sw = Rect::from_min_size(pos2(right - 52.0 * s, cy - 15.0 * s), vec2(52.0 * s, 30.0 * s));
                    clip.rect_filled(sw, 15.0 * s, veil(mix(k.field_border, k.accent, t_on), alpha));
                    let knob_x = sw.min.x + 15.0 * s + (sw.width() - 30.0 * s) * t_on;
                    clip.circle_filled(pos2(knob_x, cy), 11.0 * s, veil(mix(k.text, ink_on(k.accent), t_on), alpha));
                    clip.text(pos2(sw.min.x - 14.0 * s, cy), Align2::RIGHT_CENTER, &value, vfont, veil(vcol, alpha));
                }
                RowKind::Page => {
                    clip.text(pos2(right - chev - 8.0 * s, cy), Align2::RIGHT_CENTER, &value, vfont, veil(vcol, alpha));
                    let cr = Rect::from_center_size(pos2(right - chev / 2.0, cy), Vec2::splat(chev));
                    icons::paint(&clip, "chevron-right", cr, veil(if selected { k.accent_text } else { k.text_faint }, alpha));
                }
                RowKind::Button => {
                    // A real button: its action as the caption, a status on its left.
                    let (caption, enabled) = self.button_caption(item, confirm_reset, st);
                    let bf = semibold(17.0 * s);
                    let bw = text_width(p, caption, bf.clone()) + 36.0 * s;
                    let br = Rect::from_min_size(pos2(right - bw, cy - 17.0 * s), vec2(bw, 34.0 * s));
                    let (fill, ink, stroke) = if !enabled {
                        (Color32::TRANSPARENT, k.text_faint, k.field_border)
                    } else if selected {
                        (k.accent, ink_on(k.accent), k.accent)
                    } else {
                        (k.field, k.text, k.field_border)
                    };
                    clip.rect(br, 17.0 * s, veil(fill, alpha), Stroke::new(1.5 * s, veil(stroke, alpha)), StrokeKind::Inside);
                    clip.text(br.center(), Align2::CENTER_CENTER, caption, bf, veil(ink, alpha));
                    let label_end = r.min.x + 58.0 * s + text_width(p, label, medium(20.0 * s)) + 24.0 * s;
                    let room = (br.min.x - 14.0 * s - label_end).max(0.0);
                    clip.text(pos2(br.min.x - 14.0 * s, cy), Align2::RIGHT_CENTER, fit(p, &value, regular(16.5 * s), room), regular(16.5 * s), veil(k.text_faint, alpha));
                }
            }
        }
        // The hints say what the selected row's keys actually do.
        let back = (t("key.back"), t("hint.done"));
        let choose = ("▲ ▼", t("hint.choose"));
        let hints: Vec<(&str, &str)> = match all.get(sel).map(|&it| (it, row_kind_of(it, &self.cfg))) {
            Some((Item::Layout | Item::Position, RowKind::Choice)) => vec![choose, ("◀ ▶", t("hint.change")), (t("key.ok"), t("hint.open")), back],
            Some((_, RowKind::Choice)) => vec![choose, ("◀ ▶", t("hint.change")), back],
            Some((_, RowKind::Slider(..))) => vec![choose, ("◀ ▶", t("hint.adjust")), back],
            Some((_, RowKind::Toggle(_))) => vec![choose, (t("key.ok"), t("hint.toggle_onoff")), back],
            Some((_, RowKind::Page)) => vec![choose, (t("key.ok"), t("hint.open")), back],
            Some((it, RowKind::Button)) => {
                let (caption, enabled) = self.button_caption(it, confirm_reset, st);
                if enabled { vec![choose, (t("key.ok"), caption), back] } else { vec![choose, back] }
            }
            None => vec![choose, back],
        };
        self.footer(p, rect, &hints, alpha);

    }

    /// The four layouts as tiny screens, the current one in the accent colour.
    /// Drawn right-aligned at `right`; returns its left edge.
    fn layout_strip(&self, p: &egui::Painter, right: f32, cy: f32, selected: bool, alpha: f32) -> f32 {
        let s = self.cfg.scale;
        let k = &self.t;
        let (w, h, gap) = (44.0 * s, 28.0 * s, 8.0 * s);
        let n = Layout::ALL.len() as f32;
        let left = right - n * w - (n - 1.0) * gap;
        for (i, layout) in Layout::ALL.iter().enumerate() {
            let r = Rect::from_min_size(pos2(left + i as f32 * (w + gap), cy - h / 2.0), vec2(w, h));
            let current = *layout == self.cfg.layout;
            let ink = if current { if selected { k.accent_text } else { k.accent } } else { k.text_faint };
            p.rect(r, 4.0 * s, veil(if current { k.accent_soft } else { Color32::TRANSPARENT }, alpha), Stroke::new(1.5 * s, veil(ink, alpha)), StrokeKind::Inside);
            layout_glyph(p, r, *layout, veil(ink, alpha), 1.5 * s);
        }
        left
    }

    /// A tiny screen with the panel where it will appear. Returns its left edge.
    fn position_picture(&self, p: &egui::Painter, right: f32, cy: f32, selected: bool, alpha: f32) -> f32 {
        let s = self.cfg.scale;
        let k = &self.t;
        let (w, h) = (56.0 * s, 32.0 * s);
        let r = Rect::from_min_size(pos2(right - w, cy - h / 2.0), vec2(w, h));
        let ink = if selected { k.accent_text } else { k.accent };
        p.rect(r, 4.0 * s, Color32::TRANSPARENT, Stroke::new(1.5 * s, veil(k.text_dim, alpha)), StrokeKind::Inside);
        let panel = self.panel_glyph_rect(r.shrink(4.0 * s), self.cfg.position);
        p.rect_filled(panel, 2.0 * s, veil(ink, alpha));
        r.min.x
    }
}
