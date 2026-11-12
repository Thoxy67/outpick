//! The overlay: a picker of sources, plus Customize and Sources screens, all
//! driven by the remote's arrows, OK and Back.
//!
//! Like term's modes (`term/crates/gui/src/app/modes.rs`), every screen keeps an
//! explicit selection and handles keys itself; nothing relies on egui focus,
//! which a TV remote has no Tab key for. Everything is painted from `Tokens`,
//! and every string comes from `locales/*.lang` through `i18n`.

mod entries;
mod options;
mod picker;
mod pickers;
mod settings;
mod sources;
mod widgets;

use crate::config::{Config, Layout, Position};
use crate::i18n;
use crate::icons;
use crate::model::{Backend, Command, TvState};
use crate::theme::{self, Tokens};
use egui::{Color32, Pos2, Rect, Ui, Vec2, vec2};
use entries::Entry;
use widgets::{ease, gradient, tokens, vertical};

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
    PickCell(usize),
    /// A key hint chip: clicking it is pressing that key.
    Key(HintKey),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum HintKey {
    Ok,
    Back,
    Options,
}


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
    /// The four layouts as large cards; `sel` indexes `Layout::ALL`.
    StylePick { sel: usize },
    /// Choosing source `id`'s icon in a grid; `sel` indexes `SourceIcon::ALL`.
    IconPick { id: String, sel: usize },
    /// A screen in a 3×3 grid where the arrows move the panel; `sel` indexes `POSITION_GRID`.
    PositionPick { sel: usize },
    /// Moving the clock over the whole screen (the interface shown faded); `orig` is restored on Back.
    ClockPlace { orig: crate::config::ClockConfig },
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
    /// Quick switch: when Source was last pressed in the picker; the highlighted input
    /// is taken `switch_delay_secs` later unless another key comes first.
    armed_at: Option<f64>,
    /// How much of that countdown is left (1 → 0), for the bar under the selection.
    switch_left: Option<(f32, u32)>,
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
    /// The Sources editor's order when a row was grabbed: Back puts it back.
    grab_order: Option<Vec<String>>,
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
            armed_at: None,
            switch_left: None,
            opened_at: None,
            last_key: 0.0,
            closing: None,
            scroll: 0.0,
            grab_order: None,
            hits: Default::default(),
            last_click: f64::NEG_INFINITY,
            snap: true,
            preview_tex: Default::default(),
        }
    }

    /// Called when the app is shown again (a relaunch while it stays alive).
    #[cfg_attr(feature = "webos", allow(dead_code))] // F5 in the desktop preview
    pub fn reopen(&mut self) {
        self.snap = true;
        self.screen = Screen::Picker;
        self.picked_initial = false;
        self.moved_since_open = false;
        self.armed_at = None;
        self.opened_at = None;
        self.closing = None;
        let _ = self.backend.commands.send(Command::Refresh);
    }

    /// Called on every way out (closed, quit, window closed): saves the settings.
    #[cfg_attr(not(feature = "webos"), allow(dead_code))]
    pub fn shutdown(&mut self) {
        self.persist();
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
            let entries = self.visible_entries(&st);
            self.draw(ui, &st, &entries, (1.0 - ease(k)) * fade_in);
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
                // Placing the clock: a click puts it there.
                Nav::Click(pos) if matches!(self.screen, Screen::ClockPlace { .. }) => {
                    let r = ui.max_rect();
                    self.cfg.clock.x = ((pos.x - r.min.x) / r.width()).clamp(0.0, 1.0);
                    self.cfg.clock.y = ((pos.y - r.min.y) / r.height()).clamp(0.0, 1.0);
                }
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
        // The picker's entries, built once a frame, and again only after a key changed the config.
        let mut entries = self.visible_entries(&st);
        let mut built_for = (!translated.is_empty()).then(|| self.cfg.clone());
        let refresh = |app: &Self, entries: &mut Vec<Entry>, built_for: &mut Option<Config>| {
            if built_for.as_ref().is_some_and(|c| *c != app.cfg) {
                *entries = app.visible_entries(&st);
                *built_for = Some(app.cfg.clone());
            }
        };
        for k in &translated {
            if *k == Nav::Exit {
                self.close(now);
                break;
            }
            refresh(self, &mut entries, &mut built_for);
            match self.screen {
                Screen::Picker => self.picker_key(k, &entries, now),
                Screen::Settings { .. } => self.settings_key(k, &ctx, &st),
                Screen::Sources { .. } => self.sources_key(k, &st),
                Screen::Rename { .. } => self.rename_key(k),
                Screen::Options { .. } => self.options_key(k, &st, now),
                Screen::StylePick { .. } | Screen::PositionPick { .. } => self.pick_key(k, &ctx),
                Screen::ClockPlace { .. } => self.clock_key(k),
                Screen::IconPick { .. } => self.icon_key(k, &st),
            }
            if self.closing.is_some() {
                break;
            }
        }
        refresh(self, &mut entries, &mut built_for);

        // Timers: quick switch (Source pressed again and again, as on LG's picker) and auto close.
        self.switch_left = None;
        if self.screen != Screen::Picker {
            self.armed_at = None;
        }
        if self.screen == Screen::Picker && self.closing.is_none() {
            let idle = now - self.last_key;
            if let Some(armed) = self.armed_at {
                // Counted from the last Source press: nothing else (a resting pointer) restarts it.
                let total = self.cfg.switch_delay_secs as f64;
                let left = total - (now - armed);
                if left <= 0.0 {
                    self.armed_at = None;
                    self.activate_selected(&entries, now);
                } else {
                    self.switch_left = Some(((left / total) as f32, left.ceil() as u32));
                    ctx.request_repaint(); // the countdown bar moves every frame
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
        // Settings are saved once the keys pause (400 ms), so a kill loses at most the
        // last change; not while the clock is being placed (Back restores it). TV only:
        // the desktop's scripted screenshots must not rewrite their --config file.
        if cfg!(feature = "webos") && self.cfg != self.saved && !matches!(self.screen, Screen::ClockPlace { .. }) {
            let idle = now - self.last_key;
            if idle >= 0.4 {
                self.persist();
            } else {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(0.4 - idle));
            }
        }
        self.draw(ui, &st, &entries, fade_in);
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
            (Some(Hit::PickCell(i)), Screen::StylePick { sel } | Screen::PositionPick { sel } | Screen::IconPick { sel, .. }) => *sel = i,
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
            Some(Hit::OptionRow(_) | Hit::PickCell(_)) => vec![Nav::Ok],
            Some(Hit::Panel) => Vec::new(),
            None => vec![Nav::Back],
        }
    }

    fn close(&mut self, now: f64) {
        if self.closing.is_none() {
            self.closing = Some(now);
        }
    }

    fn draw(&mut self, ui: &mut Ui, st: &TvState, entries: &[Entry], alpha: f32) {
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
                    if let Some(i) = entries.iter().position(|e| e.current)
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
                self.draw_picker(ui, &p, st, entries, screen, offset, alpha);
            }
            Screen::Settings { sel, confirm_reset } => self.draw_settings(ui, &p, st, screen, offset, alpha, sel, confirm_reset),
            Screen::Sources { sel, col, grabbed } => self.draw_sources(ui, &p, st, screen, offset, alpha, sel, col, grabbed),
            Screen::Rename { text, tv_name, .. } => self.draw_rename(ui, &p, screen, offset, alpha, &text, &tv_name),
            Screen::Options { id, sel } => self.draw_options(ui, &p, st, screen, offset, alpha, &id, sel),
            Screen::StylePick { sel } => self.draw_style_pick(ui, &p, screen, alpha, sel),
            Screen::PositionPick { sel } => self.draw_position_pick(ui, &p, screen, alpha, sel),
            Screen::ClockPlace { .. } => self.draw_clock_place(ui, &p, st, entries, screen, alpha),
            Screen::IconPick { id, sel } => self.draw_icon_pick(ui, &p, st, screen, alpha, &id, sel),
        }
        // The clock, over everything but its own placing screen.
        if self.cfg.clock.show && !matches!(self.screen, Screen::ClockPlace { .. }) {
            crate::clock::draw(&p, screen, &self.cfg.clock, &self.t, alpha);
            let every = if self.cfg.clock.seconds { 250 } else { 1000 };
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(every));
        }
    }

    /// With the Sidebar layout, the edge every panel docks to (true = left).
    fn docked_left(&self) -> Option<bool> {
        (self.cfg.layout == Layout::Sidebar).then_some(self.cfg.position.is_left())
    }
}
