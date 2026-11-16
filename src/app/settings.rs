//! Customize: a root page of sections, each opening a page of its options.

use super::pickers::{POSITION_GRID, layout_glyph};
use super::sources::COL_SHOW;
use super::widgets::{HELP_H, cycle, cycle_num, fit, step, text_width, walk};
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
    ShowCustomizeTile,
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
    ShowApps,
    AppsView,
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
    LongPress,
    HoldGap,
    Sources,
    Reset,
    AvrEnabled,
    AvrDevice,
    AvrProtocol,
    CheckUpdates,
    Update,
}

/// Customize's sections, as the root page lists them: (title key, icon, its options).
const PAGES: &[(&str, &str, &[Item])] = &[
    ("section.appearance", "palette", &[Item::Theme, Item::Accent, Item::Corners, Item::Size, Item::ShowPanel, Item::Opacity, Item::Dim, Item::Animations]),
    ("section.layout", "layout-grid", &[Item::Layout, Item::Position, Item::ShowIcons, Item::ShowHeader, Item::ShowHints]),
    (
        "section.inputs",
        "cable",
        &[Item::Sources, Item::Naming, Item::ShowDevices, Item::ShowDisconnected, Item::ShowHome, Item::ShowLiveTv, Item::ShowUsb, Item::ShowMediaServers, Item::ShowApps, Item::AppsView],
    ),
    ("section.previews", "monitor", &[Item::ShowPreviews, Item::PreviewFps, Item::CaptureAccess]),
    ("section.clock", "timer", &[Item::ClockShow, Item::ClockStyle, Item::ClockSize, Item::Clock24h, Item::ClockSeconds, Item::ClockPlace]),
    ("section.remote", "tv", &[Item::SourceKey, Item::MultikeyOpens, Item::SwitchDelay, Item::AutoClose, Item::LongPress, Item::HoldGap]),
    ("section.avr", "speaker", &[Item::AvrEnabled, Item::AvrDevice, Item::AvrProtocol]),
    ("section.general", "settings", &[Item::Language, Item::ShowCustomizeTile, Item::CheckUpdates, Item::Update, Item::Reset]),
];

/// What each section holds, said under the root page's rows (as `PAGES`).
const PAGE_HELP: [&str; 8] = [
    "help.section.appearance",
    "help.section.layout",
    "help.section.inputs",
    "help.section.previews",
    "help.section.clock",
    "help.section.remote",
    "help.section.avr",
    "help.section.general",
];

/// The sentence saying what `item` does, shown under the rows while it is selected.
fn help_key(item: Item) -> &'static str {
    match item {
        Item::Language => "help.language",
        Item::ShowCustomizeTile => "help.show_customize_tile",
        Item::Theme => "help.theme",
        Item::Accent => "help.accent",
        Item::Corners => "help.corners",
        Item::Size => "help.size",
        Item::ShowPanel => "help.show_panel",
        Item::Opacity => "help.opacity",
        Item::Dim => "help.dim",
        Item::Animations => "help.animations",
        Item::Layout => "help.layout",
        Item::Position => "help.position",
        Item::ShowIcons => "help.show_icons",
        Item::ShowHeader => "help.show_header",
        Item::ShowHints => "help.show_hints",
        Item::ShowDisconnected => "help.show_disconnected",
        Item::ShowDevices => "help.show_devices",
        Item::ShowLiveTv => "help.show_live_tv",
        Item::Naming => "help.naming",
        Item::ShowPreviews => "help.show_previews",
        Item::PreviewFps => "help.preview_fps",
        Item::CaptureAccess => "help.capture_access",
        Item::ShowHome => "help.show_home",
        Item::ShowUsb => "help.show_usb",
        Item::ShowMediaServers => "help.show_media_servers",
        Item::ShowApps => "help.show_apps",
        Item::AppsView => "help.apps_view",
        Item::AutoClose => "help.auto_close",
        Item::ClockShow => "help.clock_show",
        Item::ClockStyle => "help.clock_style",
        Item::ClockSize => "help.clock_size",
        Item::Clock24h => "help.clock_24h",
        Item::ClockSeconds => "help.clock_seconds",
        Item::ClockPlace => "help.clock_place",
        Item::SourceKey => "help.source_key",
        Item::SwitchDelay => "help.switch_delay",
        Item::MultikeyOpens => "help.multikey_opens",
        Item::LongPress => "help.long_press",
        Item::HoldGap => "help.hold_gap",
        Item::Sources => "help.sources",
        Item::Reset => "help.reset",
        Item::AvrEnabled => "help.avr_enabled",
        Item::AvrDevice => "help.avr_device",
        Item::AvrProtocol => "help.avr_protocol",
        Item::CheckUpdates => "help.check_updates",
        Item::Update => "help.update",
    }
}

/// Clock sizes: tenths up to 160 %, then fifths (big clocks need fewer stops).
const CLOCK_SIZES: [f32; 20] = [0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 1.1, 1.2, 1.3, 1.4, 1.5, 1.6, 1.8, 2.0, 2.2, 2.4, 2.6, 2.8, 3.0];

/// Whether `item` is shown: rows that cannot apply now are left out.
fn visible(item: Item, cfg: &Config, st: &TvState) -> bool {
    match item {
        Item::Opacity => cfg.show_panel,
        Item::PreviewFps => cfg.show_previews && st.preview_supported != Some(false),
        // Only where capture needs allowing: not when it already works without it.
        Item::CaptureAccess => st.capture_access.is_some() && !(st.capture_access == Some(false) && st.preview_supported == Some(true)),
        Item::ClockStyle | Item::ClockSize | Item::Clock24h | Item::ClockSeconds | Item::ClockPlace => cfg.clock.show,
        Item::AvrDevice | Item::AvrProtocol => cfg.avr.enabled,
        Item::AppsView => cfg.show_apps,
        _ => true,
    }
}

/// The options section `page` shows now (the selection on that page indexes this).
pub(super) fn page_items(page: usize, cfg: &Config, st: &TvState) -> Vec<Item> {
    PAGES[page].2.iter().copied().filter(|&i| visible(i, cfg, st)).collect()
}

fn page_of(item: Item) -> usize {
    PAGES.iter().position(|(_, _, items)| items.contains(&item)).unwrap_or(0)
}

/// Customize at `page` (None: the root) with row `sel` selected.
fn settings_screen(page: Option<usize>, sel: usize, confirm_reset: bool) -> Screen {
    match page {
        None => Screen::Settings { sel, confirm_reset },
        Some(page) => Screen::SettingsPage { page, sel, confirm_reset },
    }
}

fn row_kind_of(item: Item) -> RowKind {
    match item {
        Item::Size => RowKind::Slider(0.8, 1.5),
        Item::Opacity => RowKind::Slider(0.4, 1.0),
        Item::Dim => RowKind::Slider(0.0, 0.8),
        Item::ClockSize => RowKind::Slider(0.4, 3.0),
        Item::ShowPanel
        | Item::ShowCustomizeTile
        | Item::ClockShow
        | Item::Clock24h
        | Item::ClockSeconds
        | Item::Animations
        | Item::ShowIcons
        | Item::ShowHeader
        | Item::ShowHints
        | Item::ShowHome
        | Item::ShowPreviews
        | Item::MultikeyOpens
        | Item::SourceKey
        | Item::CaptureAccess
        | Item::ShowDisconnected
        | Item::ShowDevices
        | Item::ShowLiveTv
        | Item::ShowUsb
        | Item::ShowMediaServers
        | Item::ShowApps
        | Item::AvrEnabled
        | Item::CheckUpdates => RowKind::Toggle,
        Item::ClockPlace | Item::Sources | Item::AvrDevice | Item::Update => RowKind::Page,
        Item::Reset => RowKind::Button,
        _ => RowKind::Choice,
    }
}

/// How a Customize row is shown and used.
#[derive(Clone, Copy, PartialEq)]
enum RowKind {
    /// ‹ value ›, changed with Left/Right (OK steps forward too).
    Choice,
    /// A gauge between two bounds, changed with Left/Right.
    Slider(f32, f32),
    /// An on/off switch, flipped with OK.
    Toggle,
    /// Opens another screen with OK (or Right).
    Page,
    /// Does something once with OK.
    Button,
}

/// A drawn row of Customize.
#[derive(Clone, Copy)]
enum SettingsRow {
    /// Root page: section `n` of `PAGES`.
    Page(usize),
    /// Section page: an option.
    Option(Item),
}

impl App {
    /// Customize where `item` is, with it selected: where sub-screens return to.
    pub(super) fn settings_at(&self, item: Item) -> Screen {
        let st = self.backend.state.lock().map(|s| s.clone()).unwrap_or_default();
        let page = page_of(item);
        let sel = page_items(page, &self.cfg, &st).iter().position(|&i| i == item).unwrap_or(0);
        settings_screen(Some(page), sel, false)
    }

    pub(super) fn settings_key(&mut self, k: &Nav, ctx: &egui::Context, st: &TvState) {
        let (page, sel, confirm_reset) = match self.screen {
            Screen::Settings { sel, .. } => (None, sel, false),
            Screen::SettingsPage { page, sel, confirm_reset } => (Some(page), sel, confirm_reset),
            _ => return,
        };
        match page {
            None => self.settings_root_key(k, sel),
            Some(page) => self.settings_page_key(k, ctx, st, page, sel, confirm_reset),
        }
    }

    /// The root page: one row per section; Back or Left goes back to the picker.
    fn settings_root_key(&mut self, k: &Nav, mut sel: usize) {
        let n = PAGES.len();
        sel = sel.min(n - 1);
        match k {
            Nav::Up => sel = walk(sel, n, -1, self.held),
            Nav::Down => sel = walk(sel, n, 1, self.held),
            // A held arrow only moves: it opens and leaves nothing.
            Nav::Left | Nav::Right if self.held => {}
            Nav::Ok | Nav::Right => {
                self.screen = settings_screen(Some(sel), 0, false);
                return;
            }
            Nav::Back | Nav::Erase | Nav::Source | Nav::Left => {
                self.persist();
                self.screen = Screen::Picker;
                return;
            }
            _ => {}
        }
        self.screen = settings_screen(None, sel, false);
    }

    /// A section page: its options; Back (or Left where Left changes nothing) goes up.
    fn settings_page_key(&mut self, k: &Nav, ctx: &egui::Context, st: &TvState, page: usize, mut sel: usize, mut confirm_reset: bool) {
        let all = page_items(page, &self.cfg, st);
        let n = all.len();
        sel = sel.min(n.saturating_sub(1));
        let item = all[sel];
        let kind = row_kind_of(item);
        let adjustable = matches!(kind, RowKind::Choice | RowKind::Slider(..));
        match k {
            Nav::Up => {
                sel = walk(sel, n, -1, self.held);
                confirm_reset = false;
            }
            Nav::Down => {
                sel = walk(sel, n, 1, self.held);
                confirm_reset = false;
            }
            Nav::Left | Nav::Right if adjustable => {
                self.adjust(item, if *k == Nav::Left { -1 } else { 1 });
                self.retheme(ctx);
            }
            // A held arrow only moves and adjusts: it opens and leaves nothing.
            Nav::Left | Nav::Right if self.held => {}
            Nav::Back | Nav::Erase | Nav::Left => {
                self.screen = settings_screen(None, page, false);
                return;
            }
            Nav::Source => {
                self.persist();
                self.screen = Screen::Picker;
                return;
            }
            Nav::Ok | Nav::Right => match item {
                Item::ClockPlace => {
                    self.screen = Screen::ClockPlace { orig: self.cfg.clock };
                    return;
                }
                Item::Sources => {
                    self.persist();
                    self.screen = Screen::Sources { sel: 0, col: COL_SHOW, grabbed: false };
                    return;
                }
                Item::AvrDevice => {
                    self.backend.avr.search();
                    self.screen = Screen::AvrPick { sel: self.avr_pick_current(st).unwrap_or(0), back: None };
                    return;
                }
                // With a newer version known: its notes and install; else OK checks now (an
                // explicit request, switch or not).
                Item::Update if st.update.offer.is_some() => {
                    self.open_update(st);
                    return;
                }
                Item::Update if *k == Nav::Ok && !st.update.checking => {
                    if let Ok(mut shared) = self.backend.state.lock() {
                        shared.update.checking = true;
                    }
                    let _ = self.backend.commands.send(Command::CheckUpdate { manual: true });
                }
                // Right only opens pages.
                _ if *k == Nav::Right => {}
                Item::Layout => {
                    let at = Layout::ALL.iter().position(|l| *l == self.cfg.layout).unwrap_or(0);
                    self.screen = Screen::StylePick { sel: at };
                    return;
                }
                Item::Position => {
                    let at = POSITION_GRID.iter().position(|p| *p == self.cfg.position).unwrap_or(7);
                    self.screen = Screen::PositionPick { sel: at };
                    return;
                }
                Item::SourceKey => self.toggle_source_key(st),
                Item::MultikeyOpens => self.toggle_multikey(st),
                Item::CaptureAccess => {
                    if self.toggle_state(item, st).1 {
                        let _ = self.backend.commands.send(Command::CaptureAccess(st.capture_access != Some(true)));
                    }
                }
                Item::ShowPreviews if st.preview_supported == Some(false) => {}
                Item::Reset if confirm_reset => {
                    // Inputs and the remote's settings stay: the key filter keeps matching the config.
                    // The Apps tile goes back off and its view to the grid, with the other tiles'
                    // switches (its name, icon and place stay with the inputs', for when it is
                    // turned on again).
                    let c = &self.cfg;
                    self.cfg = Config {
                        order: c.order.clone(),
                        sources: c.sources.clone(),
                        language: c.language,
                        source_key_opens: c.source_key_opens,
                        multikey_opens: c.multikey_opens,
                        long_press_ms: c.long_press_ms,
                        hold_gap_ms: c.hold_gap_ms,
                        avr: c.avr.clone(),
                        show_customize_tile: c.show_customize_tile,
                        // Not a matter of looks: whether the TV asks the network stays as chosen.
                        check_updates: c.check_updates,
                        ..Config::default()
                    };
                    self.retheme(ctx);
                    confirm_reset = false;
                }
                Item::Reset => confirm_reset = true,
                Item::CheckUpdates => {
                    self.adjust(item, 1);
                    if self.cfg.check_updates {
                        // Turned on: a check if one is due, as on opening.
                        let _ = self.backend.commands.send(Command::CheckUpdate { manual: false });
                    }
                }
                item => {
                    self.adjust(item, 1);
                    self.retheme(ctx);
                }
            },
            _ => {}
        }
        // Rows can appear or go (a switch shows its details): keep the selection in range.
        let n = page_items(page, &self.cfg, st).len();
        self.screen = settings_screen(Some(page), sel.min(n.saturating_sub(1)), confirm_reset);
    }

    /// Whether the remote's keys can be changed now.
    fn remote_ready(st: &TvState) -> bool {
        !st.takeover_busy && !matches!(st.key_owner, KeyOwner::Unsupported)
    }

    /// The Source key switch: this picker answers it, or LG's does.
    fn toggle_source_key(&mut self, st: &TvState) {
        if !Self::remote_ready(st) {
            return;
        }
        let on = !self.toggle_state(Item::SourceKey, st).0;
        self.cfg.source_key_opens = on;
        // The root helper reads both keys from the saved config.
        self.persist();
        let _ = self.backend.commands.send(Command::Takeover(self.cfg.source_key_opens || self.cfg.multikey_opens));
    }

    /// The Multikey switch. The key filter it installs must not take the Source key
    /// along unseen: that one stays as its switch shows it.
    fn toggle_multikey(&mut self, st: &TvState) {
        if !Self::remote_ready(st) {
            return;
        }
        self.cfg.multikey_opens = !self.cfg.multikey_opens;
        if !matches!(st.key_owner, KeyOwner::Picker) {
            self.cfg.source_key_opens = false;
        }
        self.persist();
        let _ = self.backend.commands.send(Command::Takeover(self.cfg.source_key_opens || self.cfg.multikey_opens));
    }

    /// A switch row: whether it is on, and whether OK flips it now.
    fn toggle_state(&self, item: Item, st: &TvState) -> (bool, bool) {
        let c = &self.cfg;
        match item {
            Item::SourceKey => (c.source_key_opens && matches!(st.key_owner, KeyOwner::Picker), Self::remote_ready(st)),
            Item::MultikeyOpens => (c.multikey_opens, Self::remote_ready(st)),
            Item::CaptureAccess => (st.capture_access == Some(true), !st.takeover_busy && st.capture_access.is_some()),
            Item::ShowPreviews => (c.show_previews && st.preview_supported != Some(false), st.preview_supported != Some(false)),
            Item::ShowPanel => (c.show_panel, true),
            Item::ShowCustomizeTile => (c.show_customize_tile, true),
            Item::ClockShow => (c.clock.show, true),
            Item::Clock24h => (c.clock.hour24, true),
            Item::ClockSeconds => (c.clock.seconds, true),
            Item::Animations => (c.animations, true),
            Item::ShowIcons => (c.show_icons, true),
            Item::ShowHeader => (c.show_header, true),
            Item::ShowHints => (c.show_hints, true),
            Item::ShowHome => (c.show_home, true),
            Item::ShowDisconnected => (c.show_disconnected, true),
            Item::ShowDevices => (c.show_device_names, true),
            Item::ShowLiveTv => (c.show_live_tv, true),
            Item::ShowUsb => (c.show_usb, true),
            Item::ShowMediaServers => (c.show_media_servers, true),
            Item::ShowApps => (c.show_apps, true),
            Item::AvrEnabled => (c.avr.enabled, true),
            Item::CheckUpdates => (c.check_updates, true),
            _ => (false, false),
        }
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
            Item::ShowCustomizeTile => c.show_customize_tile = !c.show_customize_tile,
            Item::ShowIcons => c.show_icons = !c.show_icons,
            Item::ShowHeader => c.show_header = !c.show_header,
            Item::ShowHints => c.show_hints = !c.show_hints,
            Item::Size => c.scale = step(c.scale, 0.1, dir),
            Item::Opacity => c.panel_opacity = step(c.panel_opacity, 0.05, dir),
            Item::Dim => c.dim = step(c.dim, 0.05, dir),
            Item::ShowPanel => c.show_panel = !c.show_panel,
            Item::ClockShow => c.clock.show = !c.clock.show,
            Item::ClockStyle => c.clock.style = cycle(&crate::config::ClockStyle::ALL, c.clock.style, dir),
            Item::ClockSize => c.clock.size = step_in(&CLOCK_SIZES, c.clock.size, dir),
            Item::Clock24h => c.clock.hour24 = !c.clock.hour24,
            Item::ClockSeconds => c.clock.seconds = !c.clock.seconds,
            Item::ShowDisconnected => c.show_disconnected = !c.show_disconnected,
            Item::ShowDevices => c.show_device_names = !c.show_device_names,
            Item::ShowLiveTv => c.show_live_tv = !c.show_live_tv,
            Item::Naming => c.naming = cycle(&Naming::ALL, c.naming, dir),
            Item::ShowPreviews => c.show_previews = !c.show_previews,
            Item::PreviewFps => c.preview_fps = cycle_num(&[1, 2, 4], c.preview_fps, dir),
            Item::ShowHome => c.show_home = !c.show_home,
            Item::ShowUsb => c.show_usb = !c.show_usb,
            Item::ShowMediaServers => c.show_media_servers = !c.show_media_servers,
            Item::ShowApps => c.show_apps = !c.show_apps,
            Item::AppsView => c.apps_view = cycle(&crate::config::AppsView::ALL, c.apps_view, dir),
            Item::AvrEnabled => c.avr.enabled = !c.avr.enabled,
            Item::CheckUpdates => c.check_updates = !c.check_updates,
            Item::AvrProtocol => c.avr.protocol = cycle(&crate::denon::Protocol::ALL, c.avr.protocol, dir),
            Item::AutoClose => c.auto_close_secs = cycle_num(&[0, 5, 10, 15, 30, 60], c.auto_close_secs, dir),
            // Quick switch is always on: only its delay is set.
            Item::SwitchDelay => c.switch_delay_secs = cycle_num(&[1, 2, 3, 4, 5, 7, 10], c.switch_delay_secs, dir),
            Item::LongPress => c.long_press_ms = cycle_num(&crate::config::LONG_PRESS_MS, c.long_press_ms, dir),
            Item::HoldGap => c.hold_gap_ms = cycle_num(&crate::config::HOLD_GAP_MS, c.hold_gap_ms, dir),
            Item::ClockPlace | Item::Sources | Item::Reset | Item::SourceKey | Item::MultikeyOpens | Item::CaptureAccess | Item::AvrDevice | Item::Update => {}
        }
    }

    /// What is wrong with the remote's keys now, if anything ("" when all is plain).
    fn remote_status(st: &TvState) -> String {
        if st.takeover_busy {
            return t("remote.applying").into();
        }
        if let Some(e) = &st.takeover_error {
            return tf("remote.failed", &[("error", e)]);
        }
        match &st.key_owner {
            KeyOwner::Other(name) => tf("remote.other", &[("name", name)]),
            KeyOwner::Broken(d) => tf("remote.broken", &[("detail", d)]),
            KeyOwner::Unsupported => t("remote.unsupported").into(),
            KeyOwner::Unknown(_) => t("remote.unknown").into(),
            KeyOwner::Pending | KeyOwner::Lg | KeyOwner::Picker => String::new(),
        }
    }

    /// A section's summary on the root page.
    fn page_summary(&self, page: usize, st: &TvState) -> String {
        let c = &self.cfg;
        match PAGES[page].0 {
            "section.appearance" => format!("{} · {:.0} %", c.theme.label(), c.scale * 100.0),
            "section.layout" => format!("{} · {}", c.layout.label(), c.position.label()),
            "section.inputs" => match self.visible_entries(st).len() {
                1 => t("summary.inputs_one").into(),
                n => tf("summary.inputs", &[("n", &n.to_string())]),
            },
            "section.previews" if st.preview_supported == Some(false) => t("value.unavailable").into(),
            "section.previews" if !c.show_previews => t("summary.previews_off").into(),
            "section.previews" => tf("value.fps", &[("n", &c.preview_fps.to_string())]),
            "section.clock" if !c.clock.show => t("summary.clock_off").into(),
            "section.clock" => c.clock.style.label().into(),
            "section.avr" if !c.avr.enabled => t("summary.avr_off").into(),
            "section.avr" => super::avr::status_text(st),
            // Where a newer version is said, discreetly, besides the Customize tile.
            "section.general" if self.update_notice(st).is_some() => {
                tf("update.available", &[("version", self.update_notice(st).map_or("", |o| o.version.as_str()))])
            }
            "section.remote" => match &st.key_owner {
                _ if st.takeover_busy => t("remote.applying").into(),
                KeyOwner::Unsupported => t("remote.unsupported").into(),
                KeyOwner::Broken(d) => tf("remote.broken", &[("detail", d)]),
                KeyOwner::Other(name) => tf("summary.source_other", &[("name", name)]),
                KeyOwner::Picker if c.source_key_opens => t("summary.source_picker").into(),
                _ => t("summary.source_lg").into(),
            },
            _ => format!("{} · v{}", language_label(c.language), env!("CARGO_PKG_VERSION")),
        }
    }

    /// Label and value of an option row. For switches the value is a status, shown only
    /// when not plain (busy, failed, unavailable).
    fn setting_row(&self, item: Item, confirm_reset: bool, st: &TvState) -> (&'static str, String) {
        let c = &self.cfg;
        let secs = |v: u32, off: &'static str| if v == 0 { t(off).to_string() } else { tf("value.seconds", &[("n", &v.to_string())]) };
        let ms = |v: u32| tf("value.ms", &[("n", &v.to_string())]);
        let none = String::new;
        match item {
            Item::Language => (t("settings.language"), language_label(c.language).into()),
            Item::Theme => (t("settings.theme"), c.theme.label().into()),
            Item::Accent => (t("settings.accent"), c.accent.label().into()),
            Item::Layout => (t("settings.layout"), c.layout.label().into()),
            Item::Position => (t("settings.position"), c.position.label().into()),
            Item::Corners => (t("settings.corners"), c.corners.label().into()),
            Item::Animations => (t("settings.animations"), none()),
            Item::ShowCustomizeTile => (t("settings.show_customize_tile"), none()),
            Item::ShowIcons => (t("settings.show_icons"), none()),
            Item::ShowHeader => (t("settings.show_header"), none()),
            Item::ShowHints => (t("settings.show_hints"), none()),
            Item::Size => (t("settings.size"), format!("{:.0} %", c.scale * 100.0)),
            Item::Opacity => (t("settings.opacity"), format!("{:.0} %", c.panel_opacity * 100.0)),
            Item::Dim => (t("settings.dim"), format!("{:.0} %", c.dim * 100.0)),
            Item::ShowPanel => (t("settings.show_panel"), none()),
            Item::ClockShow => (t("settings.clock_show"), none()),
            Item::ClockStyle => (t("settings.clock_style"), c.clock.style.label().into()),
            Item::ClockSize => (t("settings.clock_size"), format!("{:.0} %", c.clock.size * 100.0)),
            Item::Clock24h => (t("settings.clock_24h"), none()),
            Item::ClockSeconds => (t("settings.clock_seconds"), none()),
            Item::ClockPlace => (t("settings.clock_place"), t("settings.clock_place_value").into()),
            Item::ShowDisconnected => (t("settings.show_disconnected"), none()),
            Item::ShowDevices => (t("settings.show_devices"), none()),
            Item::ShowLiveTv => (t("settings.show_live_tv"), none()),
            Item::Naming => (t("settings.naming"), c.naming.label().into()),
            Item::ShowPreviews => (
                t("settings.show_previews"),
                if st.preview_supported == Some(false) { t("value.unavailable").into() } else { none() },
            ),
            Item::PreviewFps => (t("settings.preview_fps"), tf("value.fps", &[("n", &c.preview_fps.to_string())])),
            Item::CaptureAccess => (
                t("settings.capture_access"),
                match st.capture_access {
                    _ if st.takeover_busy => t("remote.applying").into(),
                    Some(false) => t("capture.blocked").into(),
                    _ => none(),
                },
            ),
            Item::ShowHome => (t("settings.show_home"), none()),
            Item::ShowUsb => (t("settings.show_usb"), none()),
            Item::ShowMediaServers => (t("settings.show_media_servers"), none()),
            Item::ShowApps => (t("settings.show_apps"), none()),
            Item::AppsView => (t("settings.apps_view"), c.apps_view.label().into()),
            Item::AutoClose => (t("settings.auto_close"), secs(c.auto_close_secs, "value.never")),
            Item::SwitchDelay => (t("settings.switch_delay"), tf("value.seconds", &[("n", &c.switch_delay_secs.to_string())])),
            Item::LongPress => (t("settings.long_press"), ms(c.long_press_ms)),
            Item::HoldGap => (t("settings.hold_gap"), ms(c.hold_gap_ms)),
            Item::MultikeyOpens => (t("settings.multikey_opens"), Self::remote_status(st)),
            Item::SourceKey => (t("settings.source_key"), Self::remote_status(st)),
            Item::Sources => (t("settings.sources"), t("settings.sources_value").into()),
            Item::Reset => (t("settings.reset"), t(if confirm_reset { "settings.reset_confirm" } else { "settings.reset_value" }).into()),
            Item::AvrEnabled => (t("settings.avr_enabled"), none()),
            Item::AvrDevice => (t("settings.avr_device"), super::avr::status_text(st)),
            Item::AvrProtocol => (t("settings.avr_protocol"), super::avr::protocol_label(c.avr.protocol).into()),
            Item::CheckUpdates => (t("settings.check_updates"), none()),
            Item::Update => (t("settings.update"), super::update::status_text(st)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_settings(
        &mut self,
        ui: &Ui,
        p: &egui::Painter,
        st: &TvState,
        screen: Rect,
        offset: Vec2,
        alpha: f32,
        page: Option<usize>,
        sel: usize,
        confirm_reset: bool,
    ) {
        let s = self.cfg.scale;
        let k = self.t;
        let pad = 28.0 * s;
        let head = pad + 38.0 * s + 18.0 * s;
        let foot = 72.0 * s;
        let row_h = 52.0 * s;
        // Under the rows: what the selected one does, in a sentence (two lines at most).
        let help_h = HELP_H * s;
        let rows: Vec<SettingsRow> = match page {
            None => (0..PAGES.len()).map(SettingsRow::Page).collect(),
            Some(pg) => page_items(pg, &self.cfg, st).into_iter().map(SettingsRow::Option).collect(),
        };
        let total = rows.len() as f32 * row_h;
        let want = vec2(860.0 * s, head + total + help_h + foot + 8.0 * s);
        let rect = self.panel_rect(screen, want).translate(offset);
        self.panel(p, rect, alpha);
        // A section's page is titled by the section, with where it is on the right.
        match page {
            None => self.header(p, rect, None, t("settings.title"), &tf("settings.version", &[("version", env!("CARGO_PKG_VERSION"))]), alpha),
            Some(pg) => self.header(p, rect, Some(PAGES[pg].1), t(PAGES[pg].0), &format!("{} · {}", t("settings.title"), t("settings.subtitle")), alpha),
        }
        // The pages whose settings change the picker's look show it, live.
        if page.is_some_and(|pg| matches!(PAGES[pg].0, "section.appearance" | "section.layout")) {
            self.draw_mini_picker(ui, p, st, screen, rect, alpha);
        }

        let area = Rect::from_min_max(pos2(rect.min.x + pad * 0.6, rect.min.y + head), pos2(rect.max.x - pad * 0.6, rect.max.y - foot - help_h));
        let sel = sel.min(rows.len().saturating_sub(1));
        // Pages fit at every size on a 1080p screen; scroll only on smaller ones.
        let want_scroll = if total <= area.height() {
            0.0
        } else {
            (sel as f32 * row_h + row_h / 2.0 - area.height() / 2.0).clamp(0.0, total - area.height())
        };
        // Only sideways slack (for the selection ring): rows never spill over the title or hints.
        let clip = p.with_clip_rect(area.expand2(vec2(6.0 * s, 0.0)));
        let y0 = self.anim(ui, &format!("settings-scroll-{page:?}"), want_scroll, 0.15);
        for (this, row) in rows.iter().enumerate() {
            let top = area.min.y + this as f32 * row_h - y0;
            let item = match *row {
                SettingsRow::Page(_) => None,
                SettingsRow::Option(item) => Some(item),
            };
            if top + row_h < area.min.y - row_h || top > area.max.y + row_h {
                continue;
            }
            let r = Rect::from_min_size(pos2(area.min.x, top), vec2(area.width(), row_h - 6.0 * s));
            self.add_hit(r.intersect(area), Hit::SettingRow(this));
            let selected = this == sel;
            if selected {
                self.selected_row(&clip, r, alpha);
            }
            let chev = 22.0 * s;
            let right = r.max.x - 14.0 * s;
            let vfont = medium(19.0 * s);
            let vcol = if selected { k.accent_text } else { k.text_dim };
            let cy = r.center().y;

            let Some(item) = item else {
                // A section on the root page: icon, name, its summary, ›.
                let SettingsRow::Page(pg) = *row else { continue };
                let (key, icon, _) = PAGES[pg];
                let ir = Rect::from_center_size(pos2(r.min.x + 30.0 * s, cy), Vec2::splat(24.0 * s));
                icons::paint(&clip, icon, ir, veil(if selected { k.accent_text } else { k.icon }, alpha));
                let label = t(key);
                clip.text(pos2(r.min.x + 58.0 * s, cy), Align2::LEFT_CENTER, label, medium(20.0 * s), veil(k.text, alpha));
                let label_end = r.min.x + 58.0 * s + text_width(p, label, medium(20.0 * s)) + 24.0 * s;
                let value_right = right - chev - 8.0 * s;
                let summary = fit(p, &self.page_summary(pg, st), vfont.clone(), (value_right - label_end).max(0.0));
                clip.text(pos2(value_right, cy), Align2::RIGHT_CENTER, summary, vfont, veil(vcol, alpha));
                let cr = Rect::from_center_size(pos2(right - chev / 2.0, cy), Vec2::splat(chev));
                icons::paint(&clip, "chevron-right", cr, veil(if selected { k.accent_text } else { k.text_faint }, alpha));
                continue;
            };

            let (label, value) = self.setting_row(item, confirm_reset, st);
            let label_x = r.min.x + 20.0 * s;
            clip.text(pos2(label_x, cy), Align2::LEFT_CENTER, label, medium(20.0 * s), veil(k.text, alpha));
            let label_end = label_x + text_width(p, label, medium(20.0 * s)) + 24.0 * s;
            // ‹ value › for rows changed with Left/Right; the right chevron's place is
            // kept so the value never moves when the row is selected. A chevron fades
            // at its end of a gauge.
            let arrows = |left_edge: f32, at_min: bool, at_max: bool| {
                if selected {
                    let cl = Rect::from_center_size(pos2(left_edge - 8.0 * s - chev / 2.0, cy), Vec2::splat(chev));
                    let cr = Rect::from_center_size(pos2(right - chev / 2.0, cy), Vec2::splat(chev));
                    let ink = |end: bool| veil(if end { k.text_faint } else { k.accent_text }, alpha);
                    icons::paint(&clip, "chevron-left", cl, ink(at_min));
                    icons::paint(&clip, "chevron-right", cr, ink(at_max));
                    // Generous targets: a pointer is less precise than a mouse.
                    self.add_hit(cl.expand(10.0 * s).intersect(area), Hit::SettingPrev(this));
                    self.add_hit(cr.expand(10.0 * s).intersect(area), Hit::SettingNext(this));
                }
            };
            match row_kind_of(item) {
                RowKind::Choice if matches!(item, Item::Layout | Item::Position) => {
                    // Pictures rather than words: what each choice looks like on screen.
                    let value_right = right - chev - 8.0 * s;
                    let strip_left = if item == Item::Layout {
                        self.layout_strip(&clip, value_right, cy, selected, alpha)
                    } else {
                        self.position_picture(&clip, value_right, cy, selected, alpha)
                    };
                    let tr = clip.text(pos2(strip_left - 14.0 * s, cy), Align2::RIGHT_CENTER, &value, vfont, veil(vcol, alpha));
                    arrows(tr.min.x, false, false);
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
                    arrows(left_edge, false, false);
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
                    let vw = 80.0 * s;
                    clip.text(pos2(value_right, cy), Align2::RIGHT_CENTER, &value, vfont, veil(vcol, alpha));
                    let track = Rect::from_min_max(pos2(value_right - vw - 160.0 * s, cy - 3.0 * s), pos2(value_right - vw, cy + 3.0 * s));
                    let on = if selected { k.accent } else { k.text_dim };
                    clip.rect_filled(track, 3.0 * s, veil(k.field_border, alpha));
                    let fill = Rect::from_min_max(track.min, pos2(track.min.x + track.width() * frac, track.max.y));
                    clip.rect_filled(fill, 3.0 * s, veil(on, alpha));
                    clip.circle_filled(pos2(fill.max.x, cy), 8.0 * s, veil(on, alpha));
                    arrows(track.min.x - 10.0 * s, v <= min + 0.001, v >= max - 0.001);
                }
                RowKind::Toggle => {
                    // A switch (Photocraft's toggle), eased between its two states; dimmed
                    // when it cannot be flipped now. A status, if any, on its left.
                    let (on, enabled) = self.toggle_state(item, st);
                    let fade = if self.cfg.animations { 0.15 } else { 0.0 };
                    let t_on = ui.ctx().animate_bool_with_time(egui::Id::unique(("lg-source-switch", item as usize)), on, fade);
                    let a = if enabled { alpha } else { alpha * 0.45 };
                    let sw = Rect::from_min_size(pos2(right - 52.0 * s, cy - 15.0 * s), vec2(52.0 * s, 30.0 * s));
                    clip.rect_filled(sw, 15.0 * s, veil(mix(k.field_border, k.accent, t_on), a));
                    let knob_x = sw.min.x + 15.0 * s + (sw.width() - 30.0 * s) * t_on;
                    clip.circle_filled(pos2(knob_x, cy), 11.0 * s, veil(mix(k.text, ink_on(k.accent), t_on), a));
                    if !value.is_empty() {
                        let room = (sw.min.x - 14.0 * s - label_end).max(0.0);
                        let f = regular(16.5 * s);
                        clip.text(pos2(sw.min.x - 14.0 * s, cy), Align2::RIGHT_CENTER, fit(p, &value, f.clone(), room), f, veil(k.text_dim, alpha));
                    }
                }
                RowKind::Page => {
                    let room = (right - chev - 8.0 * s - label_end).max(0.0);
                    let vcol = if item == Item::Update && st.update.offer.is_some() && !selected { k.accent } else { vcol };
                    clip.text(pos2(right - chev - 8.0 * s, cy), Align2::RIGHT_CENTER, fit(p, &value, vfont.clone(), room), vfont, veil(vcol, alpha));
                    let cr = Rect::from_center_size(pos2(right - chev / 2.0, cy), Vec2::splat(chev));
                    icons::paint(&clip, "chevron-right", cr, veil(if selected { k.accent_text } else { k.text_faint }, alpha));
                }
                RowKind::Button => {
                    // A real button: its action as the caption, a status on its left.
                    let caption = t(if confirm_reset { "settings.reset_confirm_button" } else { "settings.reset_button" });
                    let bf = semibold(17.0 * s);
                    let bw = text_width(p, caption, bf.clone()) + 36.0 * s;
                    let br = Rect::from_min_size(pos2(right - bw, cy - 17.0 * s), vec2(bw, 34.0 * s));
                    let (fill, ink, stroke) = if selected { (k.accent, ink_on(k.accent), k.accent) } else { (k.field, k.text, k.field_border) };
                    clip.rect(br, 17.0 * s, veil(fill, alpha), Stroke::new(1.5 * s, veil(stroke, alpha)), StrokeKind::Inside);
                    clip.text(br.center(), Align2::CENTER_CENTER, caption, bf, veil(ink, alpha));
                    let room = (br.min.x - 14.0 * s - label_end).max(0.0);
                    clip.text(pos2(br.min.x - 14.0 * s, cy), Align2::RIGHT_CENTER, fit(p, &value, regular(16.5 * s), room), regular(16.5 * s), veil(k.text_faint, alpha));
                }
            }
        }
        // What the selected row does, under a hairline.
        let help = match rows.get(sel) {
            Some(SettingsRow::Page(pg)) => t(PAGE_HELP[*pg]).to_string(),
            // A failed check: why, in full (the row has room for the start of it).
            Some(SettingsRow::Option(Item::Update)) if !st.update.checking && st.update.offer.is_none() && matches!(st.update.checked, Some(Err(_))) => {
                let e = match &st.update.checked {
                    Some(Err(e)) => e.as_str(),
                    _ => "",
                };
                tf("help.update_failed", &[("error", e)])
            }
            Some(SettingsRow::Option(Item::Update)) if st.update.offer.is_some() => {
                tf("help.update_available", &[("version", st.update.offer.as_ref().map_or("", |o| o.version.as_str()))])
            }
            Some(SettingsRow::Option(item)) => t(help_key(*item)).to_string(),
            None => String::new(),
        };
        self.help_line(p, Rect::from_min_max(pos2(area.min.x, area.max.y), pos2(area.max.x, area.max.y + help_h)), &help, alpha);

        // The hints say what the selected row's keys actually do: Back goes up a level.
        let choose = ("▲ ▼", t("hint.choose"));
        let hints: Vec<(&str, &str)> = match page {
            None => vec![choose, (t("key.ok"), t("hint.open")), (t("key.back"), t("hint.done"))],
            Some(pg) => {
                let back = (t("key.back"), t("hint.back"));
                match page_items(pg, &self.cfg, st).get(sel).map(|&it| (it, row_kind_of(it))) {
                    Some((Item::Layout | Item::Position, _)) => vec![choose, ("◀ ▶", t("hint.change")), (t("key.ok"), t("hint.open")), back],
                    Some((_, RowKind::Choice)) => vec![choose, ("◀ ▶", t("hint.change")), back],
                    Some((Item::Update, _)) if st.update.offer.is_none() => {
                        if st.update.checking { vec![choose, back] } else { vec![choose, (t("key.ok"), t("hint.check")), back] }
                    }
                    Some((_, RowKind::Slider(..))) => vec![choose, ("◀ ▶", t("hint.adjust")), back],
                    Some((it, RowKind::Toggle)) if self.toggle_state(it, st).1 => vec![choose, (t("key.ok"), t("hint.toggle_onoff")), back],
                    Some((_, RowKind::Page)) => vec![choose, (t("key.ok"), t("hint.open")), back],
                    Some((_, RowKind::Button)) => {
                        vec![choose, (t("key.ok"), t(if confirm_reset { "settings.reset_confirm_button" } else { "settings.reset_button" })), back]
                    }
                    _ => vec![choose, back],
                }
            }
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

fn language_label(l: Language) -> &'static str {
    match l {
        Language::Auto => t("value.auto"),
        Language::English => t("language.english"),
        Language::French => t("language.french"),
    }
}

/// The value after (or before, `dir` < 0) `v` in the ascending `stops`.
fn step_in(stops: &[f32], v: f32, dir: i32) -> f32 {
    if dir > 0 {
        stops.iter().copied().find(|&s| s > v + 0.001).unwrap_or(v)
    } else {
        stops.iter().rev().copied().find(|&s| s < v - 0.001).unwrap_or(v)
    }
}
