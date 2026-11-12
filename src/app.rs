//! The overlay: a picker of sources, plus Customize and Sources screens, all
//! driven by the remote's arrows, OK and Back.
//!
//! Like term's modes (`term/crates/gui/src/app/modes.rs`), every screen keeps an
//! explicit selection and handles keys itself; nothing relies on egui focus,
//! which a TV remote has no Tab key for. Everything is painted from `Tokens`,
//! and every string comes from `locales/*.lang` through `i18n`.

use crate::config::{Config, Corners, Layout, Naming, Position};
use crate::i18n::{self, Language, t, tf};
use crate::icons::{self, SourceIcon};
use crate::model::{Backend, Command, KeyOwner, Source, SourceKind, TvState};
use crate::theme::{self, Accent, ThemeKind, Tokens, ink_on, medium, mix, regular, semibold, veil};
use egui::{Align2, Color32, CornerRadius, Mesh, Pos2, Rect, Shadow, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};

#[derive(Clone, Debug, PartialEq)]
pub enum Nav {
    Up,
    Down,
    Left,
    Right,
    Ok,
    Back,
    /// The remote's Source/Input key, pressed again while the picker is open.
    Source,
    /// Long Back / Exit: close from anywhere.
    Exit,
    /// Typed text (on-screen keyboard or a real keyboard), while renaming.
    Text(String),
    /// Backspace while renaming; Back elsewhere.
    Erase,
    /// The Magic Remote's pointer moved (screen points): hovering selects.
    Pointer(Pos2),
    /// A click (the Magic Remote's OK while its pointer shows).
    Click(Pos2),
    /// Wheel: positive is up.
    Scroll(f32),
}

/// What a point on screen does under the pointer.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Hit {
    /// The panel itself (a click there does nothing).
    Panel,
    Tile(usize),
    SettingRow(usize),
    SettingPrev(usize),
    SettingNext(usize),
    SourceCell(usize, usize),
    OptionRow(usize),
    /// A key hint chip: clicking it is pressing that key.
    Key(HintKey),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum HintKey {
    Ok,
    Back,
    Options,
}

/// What a row of the options screen does.
#[derive(Clone, Copy, Debug, PartialEq)]
enum OptAction {
    Switch,
    /// A SIMPLINK key (eim/cec/sendKeyEvent), as LG's screen remote sends them.
    Cec(u32),
    Rename,
    Hide,
}

/// SIMPLINK key codes (LG's ScreenRemote KeyCodeModel.qml).
const CEC_POWER: u32 = 0x0120_2000;
const CEC_HOME: u32 = 0x0120_2001;
const CEC_INFO: u32 = 0x0120_2002;
const CEC_MENU: u32 = 0x0120_2006;

const OPTIONS: [OptAction; 7] = [
    OptAction::Switch,
    OptAction::Cec(CEC_POWER),
    OptAction::Cec(CEC_HOME),
    OptAction::Cec(CEC_MENU),
    OptAction::Cec(CEC_INFO),
    OptAction::Rename,
    OptAction::Hide,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Continue,
    Close,
}

#[derive(Clone, Debug, PartialEq)]
enum Screen {
    Picker,
    Settings { sel: usize, confirm_reset: bool },
    Sources { sel: usize, col: usize, grabbed: bool },
    /// Renaming source `id`; returns to the Sources row `sel`.
    Rename { id: String, sel: usize, text: String, tv_name: String, from_options: bool },
    /// HDMI-CEC options of input `id` (opened with Down, or Right in a list).
    Options { id: String, sel: usize },
}

/// Columns of a Sources row.
const COL_SHOW: usize = 0;
const COL_ICON: usize = 1;
const COL_RENAME: usize = 2;
const COL_MOVE: usize = 3;
const COLS: usize = 4;

/// Options of Customize.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Item {
    Language,
    Theme,
    Accent,
    Corners,
    Size,
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
    AutoSwitch,
    SourceKey,
    MultikeyOpens,
    RestoreLg,
    Sources,
    Reset,
}

/// Customize, section by section: (section title key, its options).
const SECTIONS: &[(&str, &[Item])] = &[
    ("section.general", &[Item::Language]),
    ("section.appearance", &[Item::Theme, Item::Accent, Item::Corners, Item::Size, Item::Opacity, Item::Dim, Item::Animations]),
    ("section.layout", &[Item::Layout, Item::Position, Item::ShowIcons, Item::ShowHeader, Item::ShowHints]),
    ("section.content", &[Item::Naming, Item::ShowPreviews, Item::PreviewFps, Item::CaptureAccess, Item::ShowHome, Item::ShowDisconnected, Item::ShowDevices, Item::ShowLiveTv, Item::ShowUsb, Item::ShowMediaServers]),
    ("section.behaviour", &[Item::AutoClose, Item::AutoSwitch]),
    ("section.remote", &[Item::SourceKey, Item::MultikeyOpens, Item::RestoreLg]),
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

fn item_index(item: Item) -> usize {
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

/// One tile/row of the picker.
#[derive(Clone)]
struct Entry {
    source: Source,
    name: String,
    sub: Option<String>,
    icon: SourceIcon,
    current: bool,
    hidden: bool,
}

pub struct App {
    cfg: Config,
    saved: Config,
    t: Tokens,
    backend: Backend,
    screen: Screen,
    sel: usize,
    picked_initial: bool,
    moved_since_open: bool,
    opened_at: Option<f64>,
    last_key: f64,
    closing: Option<f64>,
    /// Clickable areas of the last frame, topmost last.
    hits: std::cell::RefCell<Vec<(Rect, Hit)>>,
    /// When the last click came, to drop an OK key the same press also sends.
    last_click: f64,
    /// Place the highlight without gliding (on open, when the current source is first known).
    snap: bool,
    /// Source pictures uploaded to the GPU, by source id, with the frame they hold.
    preview_tex: std::collections::HashMap<String, (u64, egui::TextureHandle)>,
    scroll: f32,
}

impl App {
    pub fn new(ctx: &egui::Context, backend: Backend) -> Self {
        theme::install_fonts(ctx);
        icons::install(ctx);
        let cfg = Config::load();
        i18n::select(cfg.language);
        let t = tokens(&cfg);
        theme::apply(ctx, &t);
        App {
            saved: cfg.clone(),
            cfg,
            t,
            backend,
            screen: Screen::Picker,
            sel: 0,
            picked_initial: false,
            moved_since_open: false,
            opened_at: None,
            last_key: 0.0,
            closing: None,
            scroll: 0.0,
            hits: Default::default(),
            last_click: f64::NEG_INFINITY,
            snap: true,
            preview_tex: Default::default(),
        }
    }

    /// Called when the app is shown again (a relaunch while it stays alive).
    pub fn reopen(&mut self) {
        self.snap = true;
        self.screen = Screen::Picker;
        self.picked_initial = false;
        self.moved_since_open = false;
        self.opened_at = None;
        self.closing = None;
        let _ = self.backend.commands.send(Command::Refresh);
    }

    /// Whether a text field is open: the runner shows the on-screen keyboard.
    pub fn wants_text_input(&self) -> bool {
        matches!(self.screen, Screen::Rename { .. }) && self.closing.is_none()
    }

    fn retheme(&mut self, ctx: &egui::Context) {
        self.cfg.clamp();
        self.t = tokens(&self.cfg);
        theme::apply(ctx, &self.t);
        i18n::select(self.cfg.language);
    }

    fn persist(&mut self) {
        if self.cfg != self.saved {
            self.cfg.save();
            self.saved = self.cfg.clone();
        }
    }

    /// A source's name before any rename: the device plugged in or the TV's
    /// label (per the Naming setting); Live TV and Home in the current language.
    fn tv_name_with(s: &Source, naming: Naming) -> String {
        match s.kind {
            SourceKind::LiveTv => t("source.live_tv").to_string(),
            SourceKind::Home => t("source.home").to_string(),
            SourceKind::Hdmi | SourceKind::Analog if naming == Naming::Device => s.device_name(),
            _ => s.label.clone(),
        }
    }

    fn tv_name(&self, s: &Source) -> String {
        Self::tv_name_with(s, self.cfg.naming)
    }

    /// All sources with the config applied, in display order (hidden ones included).
    fn all_entries(&self, st: &TvState) -> Vec<Entry> {
        let mut list: Vec<Source> = Vec::new();
        // Home first: the home screen in use (Luma, Glasshouse Home or LG's).
        if self.cfg.show_home && let Some(h) = &st.home {
            list.push(h.clone());
        }
        if self.cfg.show_live_tv && !st.sources.iter().any(|s| s.kind == SourceKind::LiveTv) {
            list.push(Source::live_tv());
        }
        list.extend(st.sources.iter().filter(|s| self.cfg.show_live_tv || s.kind != SourceKind::LiveTv).cloned());
        list.extend(st.storage.iter().filter(|s| match s.kind {
            SourceKind::Usb => self.cfg.show_usb,
            SourceKind::MediaServer => self.cfg.show_media_servers,
            _ => true,
        }).cloned());
        let rank = |id: &str| self.cfg.order.iter().position(|o| o == id).unwrap_or(usize::MAX);
        list.sort_by_key(|s| rank(&s.id)); // stable: unlisted keep the TV's order
        list.into_iter()
            .map(|s| {
                let ov = self.cfg.source(&s.id);
                let tv_name = self.tv_name(&s);
                let name = ov.and_then(|o| o.name.clone()).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| tv_name.clone());
                let icon = match ov.map(|o| o.icon).unwrap_or_default() {
                    SourceIcon::Auto => s.tv_icon,
                    i => i,
                };
                let sub = if s.kind == SourceKind::Home {
                    s.device.clone()
                } else if s.kind == SourceKind::Usb {
                    Some(match s.device.as_deref() {
                        Some("ejecting") => t("picker.ejecting").to_string(),
                        Some(port) if !port.is_empty() => tf("picker.usb", &[("port", port)]),
                        _ => t("picker.usb_storage").to_string(),
                    })
                } else if s.kind == SourceKind::MediaServer {
                    Some(t("picker.media_server").to_string())
                } else if !s.connected {
                    Some(t("picker.not_connected").to_string())
                } else if self.cfg.show_device_names && s.kind == SourceKind::Hdmi {
                    // Under a device name, the port; under the TV's label, what is plugged in.
                    match self.cfg.naming {
                        Naming::Device => Some(match s.brand_hint() {
                            Some(b) => format!("{b} · {}", s.port_name()),
                            None => s.port_name(),
                        }),
                        Naming::TvLabel => Some(s.device_name()).filter(|d| d != &name && d != &s.label).or_else(|| Some(s.port_name()).filter(|p| p != &name)),
                    }
                } else if self.cfg.show_device_names {
                    s.device.clone().filter(|d| d != &name)
                } else {
                    None
                };
                let current = (s.params.is_none() || s.kind == SourceKind::Home) && st.foreground.as_deref() == Some(s.app_id.as_str());
                Entry { hidden: ov.is_some_and(|o| o.hidden), name, sub, icon, current, source: s }
            })
            .collect()
    }

    fn visible_entries(&self, st: &TvState) -> Vec<Entry> {
        self.all_entries(st)
            .into_iter()
            .filter(|e| !e.hidden && (self.cfg.show_disconnected || e.source.connected || e.current))
            // Home only when away from it (on an input or in another app).
            .filter(|e| !(e.source.kind == SourceKind::Home && e.current))
            .collect()
    }

    pub fn ui(&mut self, ui: &mut Ui, keys: &[Nav]) -> Outcome {
        let ctx = ui.ctx().clone();
        i18n::select(self.cfg.language); // follows the TV's locale once it is read
        let now = ui.input(|i| i.time);
        let opened = *self.opened_at.get_or_insert(now);
        if !keys.is_empty() {
            self.last_key = now;
        }
        if self.last_key < opened {
            self.last_key = opened;
        }
        let st = self.backend.state.lock().map(|s| s.clone()).unwrap_or_default();
        self.sync_previews(&ctx, &st);

        // Fade in, and out before closing.
        let fade_in = if self.cfg.animations { ease(((now - opened) / 0.16) as f32) } else { 1.0 };
        if let Some(start) = self.closing {
            let k = if self.cfg.animations { ((now - start) / 0.12) as f32 } else { 1.0 };
            if k >= 1.0 {
                self.persist();
                return Outcome::Close;
            }
            ctx.request_repaint();
            self.draw(ui, &st, (1.0 - ease(k)) * fade_in);
            return Outcome::Continue;
        }
        if fade_in < 1.0 {
            ctx.request_repaint();
        }

        // The pointer becomes the same moves and presses as the keys.
        let mut translated: Vec<Nav> = Vec::with_capacity(keys.len());
        for k in keys {
            match k {
                Nav::Pointer(pos) => translated.extend(self.hover(*pos)),
                Nav::Click(pos) => {
                    self.last_click = now;
                    translated.extend(self.click(*pos));
                }
                Nav::Scroll(dy) => {
                    let vertical_screen = !matches!(self.screen, Screen::Picker) || vertical(self.cfg.layout);
                    let nav = match (dy > &0.0, vertical_screen) {
                        (true, true) => Nav::Up,
                        (false, true) => Nav::Down,
                        (true, false) => Nav::Left,
                        (false, false) => Nav::Right,
                    };
                    translated.push(nav);
                }
                // The Magic Remote's OK also comes as a key with its click: one press, one action.
                Nav::Ok if now - self.last_click < 0.25 => {}
                other => translated.push(other.clone()),
            }
        }
        for k in &translated {
            if *k == Nav::Exit {
                self.close(now);
                break;
            }
            match self.screen {
                Screen::Picker => self.picker_key(k, &st, now),
                Screen::Settings { .. } => self.settings_key(k, &ctx),
                Screen::Sources { .. } => self.sources_key(k, &st),
                Screen::Rename { .. } => self.rename_key(k),
                Screen::Options { .. } => self.options_key(k, &st, now),
            }
            if self.closing.is_some() {
                break;
            }
        }

        // Timers: auto switch (LG-style repeated Source presses) and auto close.
        if self.screen == Screen::Picker && self.closing.is_none() {
            let idle = now - self.last_key;
            if self.cfg.auto_switch_secs > 0 && self.moved_since_open {
                let left = self.cfg.auto_switch_secs as f64 - idle;
                if left <= 0.0 {
                    self.activate_selected(&st, now);
                } else {
                    ctx.request_repaint_after(std::time::Duration::from_secs_f64(left.min(1.0)));
                }
            }
            if self.cfg.auto_close_secs > 0 && self.closing.is_none() {
                let left = self.cfg.auto_close_secs as f64 - idle;
                if left <= 0.0 {
                    self.close(now);
                } else {
                    ctx.request_repaint_after(std::time::Duration::from_secs_f64(left.min(1.0)));
                }
            }
        }
        self.draw(ui, &st, fade_in);
        Outcome::Continue
    }

    fn hit_at(&self, pos: Pos2) -> Option<Hit> {
        self.hits.borrow().iter().rev().find(|(r, _)| r.contains(pos)).map(|(_, h)| *h)
    }

    fn add_hit(&self, rect: Rect, hit: Hit) {
        if rect.is_positive() {
            self.hits.borrow_mut().push((rect, hit));
        }
    }

    /// Hovering moves the selection there; nothing is pressed.
    fn hover(&mut self, pos: Pos2) -> Vec<Nav> {
        match (self.hit_at(pos), &mut self.screen) {
            (Some(Hit::Tile(i)), Screen::Picker) => {
                if self.sel != i {
                    self.sel = i;
                    self.moved_since_open = true;
                }
            }
            (Some(Hit::SettingRow(i) | Hit::SettingPrev(i) | Hit::SettingNext(i)), Screen::Settings { sel, confirm_reset }) => {
                if *sel != i {
                    *sel = i;
                    *confirm_reset = false;
                }
            }
            (Some(Hit::OptionRow(i)), Screen::Options { sel, .. }) => *sel = i,
            (Some(Hit::SourceCell(r, c)), Screen::Sources { sel, col, grabbed }) if !*grabbed => {
                *sel = r;
                *col = c;
            }
            _ => {}
        }
        Vec::new()
    }

    /// A click selects what is under it and presses it; outside the panel it closes.
    fn click(&mut self, pos: Pos2) -> Vec<Nav> {
        let hit = self.hit_at(pos);
        self.hover(pos);
        match hit {
            Some(Hit::Tile(_) | Hit::SettingRow(_) | Hit::SourceCell(..)) => vec![Nav::Ok],
            Some(Hit::SettingPrev(_)) => vec![Nav::Left],
            Some(Hit::SettingNext(_)) => vec![Nav::Right],
            Some(Hit::Key(HintKey::Ok)) => vec![Nav::Ok],
            Some(Hit::Key(HintKey::Back)) => vec![Nav::Back],
            Some(Hit::Key(HintKey::Options)) => vec![self.options_key_nav()],
            Some(Hit::OptionRow(_)) => vec![Nav::Ok],
            Some(Hit::Panel) => Vec::new(),
            None => vec![Nav::Back],
        }
    }

    fn close(&mut self, now: f64) {
        if self.closing.is_none() {
            self.closing = Some(now);
        }
    }

    // ---------------------------------------------------------------- keys

    fn picker_key(&mut self, k: &Nav, st: &TvState, now: f64) {
        let n = self.visible_entries(st).len() + usize::from(self.cfg.show_customize_tile);
        if n == 0 {
            if matches!(k, Nav::Back | Nav::Erase | Nav::Ok) {
                self.close(now);
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
        // The key that opens an HDMI input's CEC options: Down, or Right in a
        // vertical list (where Down moves). In a grid, Down on the last row.
        let last_row = !grid || self.sel / cols == (n - 1) / cols;
        let opens_options = (*k == self.options_key_nav()) && last_row && self.selected_hdmi(st).is_some();
        match k {
            _ if opens_options => {
                if let Some(id) = self.selected_hdmi(st) {
                    self.screen = Screen::Options { id, sel: 0 };
                    return;
                }
            }
            Nav::Source => self.sel = (self.sel + 1) % n,
            Nav::Left if !list => self.sel = (self.sel + n - 1) % n,
            Nav::Right if !list => self.sel = (self.sel + 1) % n,
            Nav::Up if list => self.sel = (self.sel + n - 1) % n,
            Nav::Down if list => self.sel = (self.sel + 1) % n,
            Nav::Up if grid && self.sel >= cols => self.sel -= cols,
            Nav::Down if grid && self.sel + cols < n => self.sel += cols,
            Nav::Down if grid && self.sel / cols < (n - 1) / cols => self.sel = n - 1,
            Nav::Ok => self.activate_selected(st, now),
            Nav::Back | Nav::Erase => self.close(now),
            _ => {}
        }
        if self.sel != old {
            self.moved_since_open = true;
        }
    }

    /// Asks for live pictures (or none), and uploads frames that changed.
    fn sync_previews(&mut self, ctx: &egui::Context, st: &TvState) {
        let on = self.cfg.show_previews && st.preview_supported != Some(false) && self.closing.is_none();
        self.backend.set_preview_fps(if on { self.cfg.preview_fps.clamp(1, 4) } else { 0 });
        if !self.cfg.show_previews {
            return;
        }
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
    fn options_key_nav(&self) -> Nav {
        if vertical(self.cfg.layout) { Nav::Right } else { Nav::Down }
    }

    /// The selected tile's id if it is a connected HDMI input (one with CEC options).
    fn selected_hdmi(&self, st: &TvState) -> Option<String> {
        let e = self.visible_entries(st).into_iter().nth(self.sel)?;
        (e.source.kind == SourceKind::Hdmi && e.source.connected).then_some(e.source.id)
    }

    fn options_key(&mut self, k: &Nav, st: &TvState, now: f64) {
        let Screen::Options { id, mut sel } = self.screen.clone() else { return };
        let all = self.all_entries(st);
        let Some(pos) = all.iter().position(|e| e.source.id == id) else {
            self.screen = Screen::Picker;
            return;
        };
        let e = all[pos].clone();
        let n = OPTIONS.len();
        match k {
            Nav::Up => sel = (sel + n - 1) % n,
            Nav::Down => sel = (sel + 1) % n,
            Nav::Ok => match OPTIONS[sel] {
                OptAction::Switch => {
                    if !e.current {
                        let _ = self.backend.commands.send(Command::Launch(e.source.app_id.clone(), e.source.params.clone()));
                    }
                    self.close(now);
                    return;
                }
                OptAction::Cec(key) => {
                    let _ = self.backend.commands.send(Command::Cec { app_id: e.source.app_id.clone(), key });
                    // Out of the way, so the device's own screen shows.
                    self.close(now);
                    return;
                }
                OptAction::Rename => {
                    let text = self.cfg.source(&id).and_then(|o| o.name.clone()).unwrap_or_default();
                    let tv_name = self.tv_name(&e.source);
                    self.screen = Screen::Rename { id, sel: pos, text, tv_name, from_options: true };
                    return;
                }
                OptAction::Hide => {
                    let o = self.cfg.source_mut(&id);
                    o.hidden = true;
                    self.persist();
                    self.screen = Screen::Picker;
                    return;
                }
            },
            // The key that opened the options, or Back, closes them.
            Nav::Back | Nav::Erase | Nav::Source | Nav::Left => {
                self.screen = Screen::Picker;
                return;
            }
            _ => {}
        }
        self.screen = Screen::Options { id, sel };
    }

    fn activate_selected(&mut self, st: &TvState, now: f64) {
        let entries = self.visible_entries(st);
        if self.sel >= entries.len() {
            self.screen = Screen::Settings { sel: 0, confirm_reset: false };
            self.moved_since_open = false;
            return;
        }
        let e = &entries[self.sel];
        if !e.current {
            let _ = self.backend.commands.send(Command::Launch(e.source.app_id.clone(), e.source.params.clone()));
        }
        self.close(now);
    }

    fn settings_key(&mut self, k: &Nav, ctx: &egui::Context) {
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
            Nav::Left | Nav::Right | Nav::Ok if all[sel] == Item::SourceKey => self.toggle_takeover(),
            // Buttons and pages do nothing with Left/Right.
            Nav::Left | Nav::Right if matches!(row_kind_of(all[sel], &self.cfg), RowKind::Button | RowKind::Page) => {}
            Nav::Left | Nav::Right => {
                self.adjust(all[sel], if *k == Nav::Left { -1 } else { 1 });
                self.retheme(ctx);
            }
            Nav::Ok | Nav::Left | Nav::Right if all[sel] == Item::MultikeyOpens => {
                self.adjust(Item::MultikeyOpens, 1);
                // The root helper reads the keys from the saved config.
                self.persist();
                let st = self.backend.state.lock().map(|s| s.clone()).unwrap_or_default();
                let on = self.cfg.multikey_opens || matches!(st.key_owner, KeyOwner::Picker);
                if !matches!(st.key_owner, KeyOwner::Unsupported) {
                    let _ = self.backend.commands.send(Command::Takeover(on));
                }
            }
            Nav::Ok if all[sel] == Item::CaptureAccess => {
                let st = self.backend.state.lock().map(|s| s.clone()).unwrap_or_default();
                if self.button_caption(Item::CaptureAccess, false, &st).1 {
                    let _ = self.backend.commands.send(Command::CaptureAccess(st.capture_access != Some(true)));
                }
            }
            Nav::Ok if all[sel] == Item::RestoreLg => {
                let st = self.backend.state.lock().map(|s| s.clone()).unwrap_or_default();
                if self.button_caption(Item::RestoreLg, false, &st).1 {
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
    fn toggle_takeover(&mut self) {
        let st = self.backend.state.lock().map(|s| s.clone()).unwrap_or_default();
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
            Item::AutoSwitch => c.auto_switch_secs = cycle(&[0, 1, 2, 3, 5], c.auto_switch_secs, dir),
            Item::MultikeyOpens => c.multikey_opens = !c.multikey_opens,
            Item::Sources | Item::Reset | Item::SourceKey | Item::RestoreLg | Item::CaptureAccess => {}
        }
    }

    fn sources_key(&mut self, k: &Nav, st: &TvState) {
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

    fn rename_key(&mut self, k: &Nav) {
        let Screen::Rename { id, sel, text, tv_name, from_options } = &mut self.screen else { return };
        let back = if *from_options {
            Screen::Options { id: id.clone(), sel: OPTIONS.iter().position(|a| *a == OptAction::Rename).unwrap_or(0) }
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

    // ---------------------------------------------------------------- drawing

    fn draw(&mut self, ui: &mut Ui, st: &TvState, alpha: f32) {
        self.hits.borrow_mut().clear();
        let screen = ui.max_rect();
        let s = self.cfg.scale;
        let p = ui.painter().clone();

        // Backdrop: darken toward the panel, so the picture stays visible elsewhere.
        if self.cfg.dim > 0.0 {
            let dark = Color32::from_black_alpha((self.cfg.dim * alpha * 255.0) as u8);
            let clear = Color32::TRANSPARENT;
            match self.docked_left() {
                Some(true) => gradient(&p, screen, dark, clear, false),
                Some(false) => gradient(&p, screen, clear, dark, false),
                None => match self.cfg.position {
                Position::Center => {
                    p.rect_filled(screen, 0.0, dark);
                }
                Position::Bottom | Position::BottomLeft | Position::BottomRight => gradient(&p, screen, clear, dark, true),
                Position::Top | Position::TopLeft | Position::TopRight => gradient(&p, screen, dark, clear, true),
                Position::Left => gradient(&p, screen, dark, clear, false),
                Position::Right => gradient(&p, screen, clear, dark, false),
                },
            }
        }

        // Rise in from the panel's edge.
        let rise = (1.0 - alpha) * 24.0 * s;
        let offset = match (self.docked_left(), self.cfg.position) {
            (Some(true), _) => vec2(-rise, 0.0),
            (Some(false), _) => vec2(rise, 0.0),
            (None, pos) => match pos {
            Position::Bottom | Position::BottomLeft | Position::BottomRight => vec2(0.0, rise),
            Position::Top | Position::TopLeft | Position::TopRight => vec2(0.0, -rise),
            Position::Left => vec2(-rise, 0.0),
            Position::Right => vec2(rise, 0.0),
            Position::Center => Vec2::ZERO,
            },
        };

        match self.screen.clone() {
            Screen::Picker => {
                // Start on the source being watched: wait for the app in front to be
                // known (it can arrive after the inputs), up to a second.
                // The app in front may be reported late or change once (Live TV, then
                // the HDMI input): follow it for a moment, until the user moves.
                let early = ui.input(|i| i.time) - self.opened_at.unwrap_or(0.0) < 2.5;
                if st.loaded && !self.moved_since_open && (early || !self.picked_initial) {
                    if let Some(i) = self.visible_entries(st).iter().position(|e| e.current)
                        && i != self.sel
                    {
                        self.sel = i;
                        self.snap = true;
                    }
                    if !self.picked_initial {
                        self.picked_initial = true;
                        self.snap = true;
                    }
                }
                self.draw_picker(ui, &p, st, screen, offset, alpha);
            }
            Screen::Settings { sel, confirm_reset } => self.draw_settings(ui, &p, st, screen, offset, alpha, sel, confirm_reset),
            Screen::Sources { sel, col, grabbed } => self.draw_sources(ui, &p, st, screen, offset, alpha, sel, col, grabbed),
            Screen::Rename { text, tv_name, .. } => self.draw_rename(ui, &p, screen, offset, alpha, &text, &tv_name),
            Screen::Options { id, sel } => self.draw_options(ui, &p, st, screen, offset, alpha, &id, sel),
        }
    }

    /// With the Sidebar layout, the edge every panel docks to (true = left).
    fn docked_left(&self) -> Option<bool> {
        (self.cfg.layout == Layout::Sidebar).then_some(self.cfg.position.is_left())
    }

    fn panel_rect(&self, screen: Rect, size: Vec2) -> Rect {
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

    fn panel(&self, p: &egui::Painter, rect: Rect, alpha: f32) {
        self.add_hit(rect, Hit::Panel);
        let s = self.cfg.scale;
        let k = &self.t;
        let r = k.radius_lg * 2.0 * s;
        let shadow = Shadow { offset: [0, (14.0 * s) as i8], blur: (48.0 * s).min(255.0) as u8, spread: 0, color: veil(k.shadow, alpha) };
        p.add(shadow.as_shape(rect, CornerRadius::same(r as u8)));
        let fill = veil(k.card, self.cfg.panel_opacity * alpha);
        p.rect(rect, r, fill, Stroke::new(1.0 * s, veil(k.card_border, alpha)), StrokeKind::Inside);
    }

    fn header(&self, p: &egui::Painter, rect: Rect, title: &str, subtitle: &str, alpha: f32) {
        let s = self.cfg.scale;
        let k = &self.t;
        let pad = 28.0 * s;
        // Title and subtitle share one centre line.
        let yc = rect.min.y + pad + 19.0 * s;
        let tw = text_width(p, title, semibold(30.0 * s));
        p.text(pos2(rect.min.x + pad, yc), Align2::LEFT_CENTER, title, semibold(30.0 * s), veil(k.text, alpha));
        if !subtitle.is_empty() {
            let room = rect.width() - pad * 2.0 - tw - 24.0 * s;
            let sub = fit(p, subtitle, regular(18.0 * s), room);
            p.text(pos2(rect.max.x - pad, yc), Align2::RIGHT_CENTER, sub, regular(18.0 * s), veil(k.text_dim, alpha));
        }
    }

    /// Width the key hints take at `zoom` (1 = normal size), from the left padding on.
    fn footer_width(&self, p: &egui::Painter, hints: &[(&str, &str)], zoom: f32) -> f32 {
        let s = self.cfg.scale * zoom;
        let mut w = 0.0;
        for (key, label) in hints {
            w += text_width(p, key, semibold(14.0 * s)).max(18.0 * s) + 16.0 * s;
            w += 10.0 * s + text_width(p, label, regular(17.0 * s)) + 26.0 * s;
        }
        w - 26.0 * s
    }

    /// The key hints along the panel's bottom. They always stay inside it: when they do not
    /// fit, the arrow hints (the obvious ones) go first, then the text gets a little smaller.
    fn footer(&self, p: &egui::Painter, rect: Rect, hints: &[(&str, &str)], alpha: f32) {
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
            let kf = semibold(14.0 * s);
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
            clip.rect(cap, 7.0 * s, veil(k.field, alpha), Stroke::new(1.0, veil(k.field_border, alpha)), StrokeKind::Inside);
            clip.text(cap.center(), Align2::CENTER_CENTER, *key, kf, veil(k.text, alpha));
            x = cap.max.x + 10.0 * s;
            let r = clip.text(pos2(x, cap.center().y), Align2::LEFT_CENTER, *label, regular(17.0 * s), veil(k.text_dim, alpha));
            if let Some(nav) = nav {
                self.add_hit(cap.union(r).expand(6.0 * s), Hit::Key(nav));
            }
            x = r.max.x + 26.0 * s;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_picker(&mut self, ui: &Ui, p: &egui::Painter, st: &TvState, screen: Rect, offset: Vec2, alpha: f32) {
        let s = self.cfg.scale;
        let k = self.t;
        let entries = self.visible_entries(st);
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
        let (tile, gap, cols, rows) = match self.cfg.layout {
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
        let content = vec2(
            cols as f32 * tile.x + cols.saturating_sub(1) as f32 * gap,
            rows as f32 * tile.y + rows.saturating_sub(1) as f32 * gap + 2.0 * push,
        );
        // The panel is at least as wide as its key hints (the Sidebar keeps its width and
        // compacts them instead).
        let hint_w = if self.cfg.show_hints {
            let probe = [(t("key.ok"), t("hint.switch")), ("▲ ▼", t("hint.move")), ("▶", t("hint.cec_options")), (t("key.back"), t("hint.close"))];
            self.footer_width(p, &probe, 1.0) + 2.0 * pad
        } else {
            0.0
        };
        let min_w = if self.cfg.layout == Layout::Sidebar { 0.0 } else { (640.0 * s).max(hint_w) };
        let want = vec2((content.x + pad * 2.0).max(min_w), head + content.y + foot + 8.0 * s);
        let rect = self.panel_rect(screen, want).translate(offset);
        self.panel(p, rect, alpha);

        let subtitle = if let Some(err) = &st.error {
            tf("error.inputs", &[("error", err)])
        } else if !st.loaded {
            t("picker.loading").to_string()
        } else {
            entries.iter().find(|e| e.current).map(|e| tf("picker.watching", &[("name", &e.name)])).unwrap_or_default()
        };
        if self.cfg.show_header {
            self.header(p, rect, t("picker.title"), &subtitle, alpha);
        }

        // Lay tiles out, scrolling to keep the selection in view.
        let area = Rect::from_min_max(pos2(rect.min.x + pad, rect.min.y + head), pos2(rect.max.x - pad, rect.max.y - foot - 8.0 * s));
        let cell = |i: usize| -> Rect {
            let (c, r) = (i % cols, i / cols);
            Rect::from_min_size(pos2(area.min.x + c as f32 * (tile.x + gap), area.min.y + r as f32 * (tile.y + gap)), tile)
        };
        let sel_now = self.sel;
        let offsets: Vec<f32> = (0..n)
            .map(|i| {
                let want = if push == 0.0 { 0.0 } else if i < sel_now { 0.0 } else if i == sel_now { push } else { 2.0 * push };
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
            clip.rect(rect_i, r, base, Stroke::new(1.0, veil(k.field_border, alpha * 0.6)), StrokeKind::Inside);
            if let Some(e) = entries.get(i) {
                let live = st.previews.get(&e.source.id).is_some_and(|p| p.live);
                if self.paint_preview(&clip, rect_i, r, &e.source.id, live, alpha) {
                    clip.rect_stroke(rect_i, r, Stroke::new(1.0, veil(k.field_border, alpha * 0.6)), StrokeKind::Inside);
                }
            }
            self.add_hit(rect_i.intersect(area), Hit::Tile(i));
        }
        if n > 0 {
            let pictured = entries.get(self.sel).is_some_and(|e| self.cfg.show_previews && self.preview_tex.contains_key(&e.source.id));
            clip.rect_filled(hl, r, veil(k.hover, alpha * if pictured { 0.25 } else { 1.0 }));
            clip.rect_stroke(hl.expand(5.0 * s), r + 5.0 * s, Stroke::new(3.0 * s, veil(k.accent, alpha)), StrokeKind::Outside);
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
                Some(e) => self.draw_tile(ui, &clip, rect_i, e.icon.svg(), e, selected, alpha),
                None => {
                    let e = Entry {
                        source: Source::live_tv(),
                        name: t("picker.customize").into(),
                        sub: Some(t("picker.customize_sub").into()),
                        icon: SourceIcon::Auto,
                        current: false,
                        hidden: false,
                    };
                    self.draw_tile(ui, &clip, rect_i, "settings", &e, selected, alpha);
                }
            }
        }

        let mv = match self.cfg.layout {
            Layout::Grid => "◀ ▲ ▼ ▶",
            l if vertical(l) => "▲ ▼",
            _ => "◀ ▶",
        };
        if self.cfg.show_hints {
            let mut hints = vec![(t("key.ok"), t("hint.switch")), (mv, t("hint.move"))];
            // Only on an HDMI input with a device behind it: its CEC options.
            if self.selected_hdmi(st).is_some() {
                hints.push((if vertical(self.cfg.layout) { "▶" } else { "▼" }, t("hint.cec_options")));
            }
            hints.push((t("key.back"), t("hint.close")));
            self.footer(p, rect, &hints, alpha);
        }
        if st.loaded {
            self.snap = false;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_tile(&self, _ui: &Ui, p: &egui::Painter, rect: Rect, icon: &str, e: &Entry, selected: f32, alpha: f32) {
        let s = self.cfg.scale;
        let k = &self.t;
        let dimmed = !e.source.connected && !e.current;
        let fg = mix(if dimmed { k.text_faint } else { k.text }, k.text, selected);
        let icon_tint = mix(if dimmed { k.text_faint } else { k.icon }, k.accent_text, selected);
        // The selected icon grows a little, eased in with the selection.
        let lift = 1.0 + 0.08 * selected;
        let sub_c = if dimmed { k.text_faint } else { k.text_dim };

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

        // Icon, name and subtitle as one block, centred in the tile.
        let isz = if self.cfg.show_icons { 60.0 * s } else { 0.0 };
        let icon_size = isz * lift;
        let maxw = rect.width() - 28.0 * s;
        // A long name takes two lines rather than being cut.
        let name_font = semibold(22.0 * s);
        let lines = two_lines(p, &e.name, name_font.clone(), maxw);
        let line_h = 26.0 * s;
        let (gap1, name_h, gap2, sub_h) = (if isz > 0.0 { 16.0 * s } else { 0.0 }, line_h * lines.len() as f32, 4.0 * s, 20.0 * s);
        let block = isz + gap1 + name_h + if e.sub.is_some() { gap2 + sub_h } else { 0.0 };
        let top = rect.center().y - block / 2.0;
        if isz > 0.0 {
            let ir = Rect::from_center_size(pos2(rect.center().x, top + isz / 2.0), Vec2::splat(icon_size));
            icons::paint(p, icon, ir, veil(icon_tint, alpha));
        }
        for (li, line) in lines.iter().enumerate() {
            let ly = top + isz + gap1 + line_h * (li as f32 + 0.5);
            p.text(pos2(rect.center().x, ly), Align2::CENTER_CENTER, line, name_font.clone(), veil(fg, alpha));
        }
        if let Some(sub) = &e.sub {
            let sy = top + isz + gap1 + name_h + gap2 + sub_h / 2.0;
            p.text(pos2(rect.center().x, sy), Align2::CENTER_CENTER, fit(p, sub, regular(16.5 * s), maxw), regular(16.5 * s), veil(sub_c, alpha));
        }
        if e.current {
            self.badge(p, pos2(rect.max.x - 12.0 * s, rect.min.y + 12.0 * s), Align2::RIGHT_TOP, alpha);
        }
    }

    /// The pill on the source being watched.
    fn badge(&self, p: &egui::Painter, at: Pos2, anchor: Align2, alpha: f32) {
        let s = self.cfg.scale;
        let k = &self.t;
        let f = semibold(13.0 * s);
        let label = t("picker.now");
        let w = text_width(p, label, f.clone()) + 30.0 * s;
        let r = anchor.anchor_size(at, vec2(w, 24.0 * s));
        p.rect_filled(r, 12.0 * s, veil(k.accent, alpha));
        let ink = ink_on(k.accent);
        p.circle_filled(pos2(r.min.x + 11.0 * s, r.center().y), 3.5 * s, veil(ink, alpha));
        p.text(pos2(r.min.x + 18.0 * s, r.center().y), Align2::LEFT_CENTER, label, f, veil(ink, alpha));
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
            Item::AutoSwitch => ("circle-check", t("settings.auto_switch"), secs(c.auto_switch_secs, "value.ok_only")),
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
    fn draw_settings(&mut self, ui: &Ui, p: &egui::Painter, st: &TvState, screen: Rect, offset: Vec2, alpha: f32, sel: usize, confirm_reset: bool) {
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
                clip.rect_filled(r, k.radius * 1.5 * s, veil(k.accent_soft, alpha));
                clip.rect_stroke(r, k.radius * 1.5 * s, Stroke::new(2.0 * s, veil(k.accent, alpha)), StrokeKind::Inside);
            }
            let (icon, label, value) = self.setting_row(item, confirm_reset, &st);
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

    #[allow(clippy::too_many_arguments)]
    fn draw_sources(&mut self, ui: &Ui, p: &egui::Painter, st: &TvState, screen: Rect, offset: Vec2, alpha: f32, sel: usize, col: usize, grabbed: bool) {
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
    fn draw_rename(&mut self, ui: &Ui, p: &egui::Painter, screen: Rect, offset: Vec2, alpha: f32, text: &str, tv_name: &str) {
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
        self.footer(p, rect, &[(t("key.ok"), t("hint.save")), (t("key.back"), t("hint.cancel"))], alpha);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_options(&mut self, ui: &Ui, p: &egui::Painter, st: &TvState, screen: Rect, offset: Vec2, alpha: f32, id: &str, sel: usize) {
        let s = self.cfg.scale;
        let k = self.t;
        let all = self.all_entries(st);
        let Some(e) = all.iter().find(|e| e.source.id == id).cloned() else { return };
        let src = &e.source;
        let pad = 28.0 * s;
        let head = pad + 38.0 * s + 18.0 * s;
        let info_h = 96.0 * s;
        let row_h = 52.0 * s;
        let foot = 72.0 * s;
        let want = vec2(760.0 * s, head + info_h + OPTIONS.len() as f32 * row_h + foot);
        let rect = self.panel_rect(screen, want).translate(offset);
        self.panel(p, rect, alpha);
        self.header(p, rect, &e.name, &src.port_name(), alpha);

        // What the TV knows about the device: CEC name, maker, cable and signal.
        let yes_no = |b: Option<bool>, y: &'static str, n: &'static str| match b {
            Some(true) => t(y).to_string(),
            Some(false) => t(n).to_string(),
            None => "—".to_string(),
        };
        let facts = [
            (t("info.cec"), src.device.clone().unwrap_or_else(|| "—".into())),
            (t("info.brand"), src.brand.clone().unwrap_or_else(|| "—".into())),
            (t("info.cable"), yes_no(Some(src.connected), "value.plugged", "value.unplugged")),
            (t("info.signal"), yes_no(src.signal, "value.present", "value.absent")),
        ];
        let info = Rect::from_min_size(pos2(rect.min.x + pad, rect.min.y + head), vec2(rect.width() - pad * 2.0, info_h - 16.0 * s));
        p.rect_filled(info, k.radius * 1.5 * s, veil(k.field, alpha * 0.8));
        let cw = info.width() / facts.len() as f32;
        for (i, (label, value)) in facts.iter().enumerate() {
            let x = info.min.x + cw * i as f32 + 18.0 * s;
            p.text(pos2(x, info.min.y + 24.0 * s), Align2::LEFT_CENTER, *label, semibold(13.0 * s), veil(k.text_faint, alpha));
            p.text(pos2(x, info.min.y + 52.0 * s), Align2::LEFT_CENTER, fit(p, value, medium(18.0 * s), cw - 26.0 * s), medium(18.0 * s), veil(k.text, alpha));
        }

        let area = Rect::from_min_max(pos2(rect.min.x + pad * 0.6, info.max.y + 16.0 * s), pos2(rect.max.x - pad * 0.6, rect.max.y - foot));
        let clip = p.with_clip_rect(area.expand2(vec2(6.0 * s, 0.0)));
        for (i, action) in OPTIONS.iter().enumerate() {
            let r = Rect::from_min_size(pos2(area.min.x, area.min.y + i as f32 * row_h), vec2(area.width(), row_h - 6.0 * s));
            self.add_hit(r.intersect(area), Hit::OptionRow(i));
            let selected = i == sel;
            if selected {
                clip.rect_filled(r, k.radius * 1.5 * s, veil(k.accent_soft, alpha));
                clip.rect_stroke(r, k.radius * 1.5 * s, Stroke::new(2.0 * s, veil(k.accent, alpha)), StrokeKind::Inside);
            }
            let (icon, label) = match action {
                OptAction::Switch => ("circle-check", t("options.switch")),
                OptAction::Cec(CEC_POWER) => ("power", t("options.cec_power")),
                OptAction::Cec(CEC_HOME) => ("house", t("options.cec_home")),
                OptAction::Cec(CEC_MENU) => ("menu", t("options.cec_menu")),
                OptAction::Cec(_) => ("info", t("options.cec_info")),
                OptAction::Rename => ("pencil", t("options.rename")),
                OptAction::Hide => ("eye-off", t("options.hide")),
            };
            let ir = Rect::from_center_size(pos2(r.min.x + 30.0 * s, r.center().y), Vec2::splat(24.0 * s));
            icons::paint(&clip, icon, ir, veil(if selected { k.accent_text } else { k.icon }, alpha));
            clip.text(pos2(r.min.x + 58.0 * s, r.center().y), Align2::LEFT_CENTER, label, medium(20.0 * s), veil(k.text, alpha));
            if matches!(action, OptAction::Cec(_)) {
                // CEC keys go to the active input: say so where it applies.
                let note = if e.current { "" } else { t("options.switches_first") };
                clip.text(pos2(r.max.x - 16.0 * s, r.center().y), Align2::RIGHT_CENTER, note, regular(15.0 * s), veil(k.text_faint, alpha));
            }
        }
        self.footer(p, rect, &[(t("key.ok"), t("hint.choose")), (t("key.back"), t("hint.back"))], alpha);
        let _ = ui;
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
            let b = |x0: f32, y0: f32, x1: f32, y1: f32| {
                let rr = Rect::from_min_max(pos2(r.min.x + w * x0, r.min.y + h * y0), pos2(r.min.x + w * x1, r.min.y + h * y1));
                p.rect_filled(rr, 1.5 * s, veil(ink, alpha));
            };
            match layout {
                Layout::Row => (0..4).for_each(|t| b(0.1 + t as f32 * 0.21, 0.5, 0.27 + t as f32 * 0.21, 0.84)),
                Layout::Grid => (0..6).for_each(|t| {
                    let (c, rw) = ((t % 3) as f32, (t / 3) as f32);
                    b(0.24 + c * 0.19, 0.24 + rw * 0.28, 0.38 + c * 0.19, 0.44 + rw * 0.28)
                }),
                Layout::List => (0..3).for_each(|t| b(0.34, 0.2 + t as f32 * 0.22, 0.66, 0.34 + t as f32 * 0.22)),
                Layout::Sidebar => {
                    b(0.7, 0.12, 0.88, 0.88);
                }
            }
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
        let (px, py) = match self.cfg.position {
            Position::TopLeft => (0.0, 0.0),
            Position::Top => (0.5, 0.0),
            Position::TopRight => (1.0, 0.0),
            Position::Left => (0.0, 0.5),
            Position::Center => (0.5, 0.5),
            Position::Right => (1.0, 0.5),
            Position::BottomLeft => (0.0, 1.0),
            Position::Bottom => (0.5, 1.0),
            Position::BottomRight => (1.0, 1.0),
        };
        // The panel's shape follows the layout: wide for a row, tall for a list.
        let (pw, ph) = match self.cfg.layout {
            Layout::Row => (0.62, 0.24),
            Layout::Grid => (0.42, 0.42),
            Layout::List => (0.26, 0.6),
            Layout::Sidebar => (0.24, 0.84),
        };
        let inner = r.shrink(4.0 * s);
        let size = vec2(inner.width() * pw, inner.height() * ph);
        let min = pos2(inner.min.x + (inner.width() - size.x) * px, inner.min.y + (inner.height() - size.y) * py);
        p.rect_filled(Rect::from_min_size(min, size), 2.0 * s, veil(ink, alpha));
        r.min.x
    }

    fn anim(&self, ui: &Ui, name: &str, target: f32, secs: f32) -> f32 {
        let secs = if self.cfg.animations && !self.snap { secs } else { 0.0 };
        ui.ctx().animate_value_with_time(egui::Id::unique(("lg-source-anim", name)), target, secs)
    }
}

/// The theme's tokens with the user's corner shape applied.
fn tokens(cfg: &Config) -> Tokens {
    let mut t = Tokens::new(cfg.theme, cfg.accent);
    let f = cfg.corners.factor();
    t.radius_sm *= f;
    t.radius *= f;
    t.radius_lg *= f;
    t
}

/// Vertical layouts move with Up/Down.
fn vertical(l: Layout) -> bool {
    matches!(l, Layout::List | Layout::Sidebar)
}

fn grid_cols(n: usize) -> usize {
    match n {
        0..=4 => n.max(1),
        5..=6 => 3,
        _ => 4,
    }
}

fn ease(t: f32) -> f32 {
    let x = t.clamp(0.0, 1.0);
    1.0 - (1.0 - x).powi(3)
}

fn cycle<T: Copy + PartialEq>(all: &[T], cur: T, dir: i32) -> T {
    let i = all.iter().position(|&v| v == cur).unwrap_or(0) as i32;
    let n = all.len() as i32;
    all[((i + dir).rem_euclid(n)) as usize]
}

fn step(v: f32, by: f32, dir: i32) -> f32 {
    ((v + by * dir as f32) / by).round() * by
}

/// First row to show so `sel` stays in view (term's `look::keep_visible`).
fn keep_visible(sel: usize, visible: usize, total: usize) -> usize {
    if total <= visible { 0 } else { sel.saturating_sub(visible / 2).min(total - visible) }
}

fn text_width(p: &egui::Painter, text: &str, font: egui::FontId) -> f32 {
    p.layout_no_wrap(text.to_string(), font, Color32::WHITE).size().x
}

/// `text` on one line if it fits, else split at a space onto two lines
/// (the second shortened with an ellipsis if it still does not fit).
fn two_lines(p: &egui::Painter, text: &str, font: egui::FontId, max: f32) -> Vec<String> {
    if text_width(p, text, font.clone()) <= max {
        return vec![text.to_string()];
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    // The longest first line that fits; the rest goes below.
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
fn fit(p: &egui::Painter, text: &str, font: egui::FontId, max: f32) -> String {
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
fn gradient(p: &egui::Painter, rect: Rect, a: Color32, b: Color32, vertical: bool) {
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
