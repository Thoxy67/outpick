//! A source's options (Down / Right on an HDMI input, or a long OK on any tile): the network
//! receiver's inputs and sound mode, its device's HDMI-CEC keys, switch, rename, hide.
//!
//! They are grouped in sections of buttons laid out like the picker: tiles in a row or a
//! grid, rows of buttons in a list or the sidebar. The arrows move between buttons as they
//! sit on screen; the arrow opposite the one that opened the options goes back from its edge.

use super::entries::Entry;
use super::widgets::{RIM, fit, gradient, text_width, vertical};
use super::{App, Hit, Nav, Screen};
use crate::config::Layout;
use crate::i18n::t;
use crate::icons;
use crate::model::{Command, SourceKind, TvState};
use crate::theme::{ink_on, medium, mix, regular, semibold, veil};
use egui::{Align2, Color32, Rect, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};

/// What a button of the options screen does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum OptAction {
    Switch,
    /// Input `n` of the network receiver (`AvrState::inputs`), shown through this input.
    Avr(usize),
    /// The receiver's volume: down or up by 1.0, mute on or off.
    VolumeDown,
    VolumeUp,
    Mute,
    /// Sound mode group `n` of the receiver (`denon::SOUND_MODES`).
    Sound(usize),
    /// The receiver's setup menu (its remote's Setup key), then its keys from this remote.
    AvrSetup,
    /// Which receiver on the network is this one (several found).
    AvrChoose,
    /// A SIMPLINK key (eim/cec/sendKeyEvent), as LG's screen remote sends them.
    Cec(u32),
    Rename,
    Hide,
}

/// The sections, in the order they show.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Group {
    /// The receiver's inputs.
    Receiver,
    /// Its volume and mute, as chips.
    Volume,
    /// Its sound modes, as chips.
    Sound,
    /// The device behind the input: HDMI-CEC keys, the receiver's menu and choice.
    Device,
    /// This input in the picker: switch, rename, hide.
    Input,
}

impl Group {
    fn title(self) -> &'static str {
        match self {
            Group::Receiver => t("options.group_receiver"),
            Group::Volume => t("options.group_volume"),
            Group::Sound => t("options.group_sound"),
            Group::Device => t("options.group_device"),
            Group::Input => t("options.group_input"),
        }
    }
}

fn group_of(a: OptAction) -> Group {
    match a {
        OptAction::Avr(_) => Group::Receiver,
        OptAction::VolumeDown | OptAction::VolumeUp | OptAction::Mute => Group::Volume,
        OptAction::Sound(_) => Group::Sound,
        OptAction::AvrSetup | OptAction::AvrChoose | OptAction::Cec(_) => Group::Device,
        OptAction::Switch | OptAction::Rename | OptAction::Hide => Group::Input,
    }
}

/// The name of sound mode group `i` (`denon::SOUND_MODES`).
fn sound_label(i: usize) -> &'static str {
    match crate::denon::SOUND_MODES[i] {
        "MOVIE" => t("sound.movie"),
        "MUSIC" => t("sound.music"),
        "GAME" => t("sound.game"),
        "DIRECT" => "Direct",
        "PURE DIRECT" => "Pure Direct",
        "STEREO" => t("sound.stereo"),
        _ => t("sound.auto"),
    }
}

/// What a chip (a volume key or a sound mode) says.
fn chip_label(a: OptAction) -> &'static str {
    match a {
        OptAction::VolumeDown => "−",
        OptAction::VolumeUp => "+",
        OptAction::Mute => t("options.btn_mute"),
        OptAction::Sound(m) => sound_label(m),
        _ => "",
    }
}

/// A volume as the receiver shows it: "45", "45.5" ("45,5" in French).
fn volume_text(v: f32) -> String {
    let whole = v.floor() as i32;
    if v - whole as f32 >= 0.25 { crate::i18n::tf("options.volume_half", &[("n", &whole.to_string())]) } else { whole.to_string() }
}

/// SIMPLINK key codes (LG's ScreenRemote KeyCodeModel.qml).
const CEC_POWER: u32 = 0x0120_2000;
const CEC_HOME: u32 = 0x0120_2001;
const CEC_INFO: u32 = 0x0120_2002;
const CEC_MENU: u32 = 0x0120_2006;

/// `Screen::Options { sel }` meaning "the Rename button" (back from renaming), whatever shows.
pub(super) const SEL_RENAME: usize = usize::MAX;
/// `Screen::Options { sel }` meaning "as they open": on the receiver's input in use (the
/// ticked one) when it is listed, else on the first button.
pub(super) const SEL_START: usize = usize::MAX - 1;

/// The buttons for source `e`, in reading order: the receiver's inputs and sound modes when
/// it is behind this input, the device's keys (CEC only when SIMPLINK reaches it; power
/// last, out of the way of a stray OK), then this input's own.
pub(super) fn option_rows(e: &Entry, st: &TvState) -> Vec<OptAction> {
    let mut rows = Vec::with_capacity(20);
    if e.avr {
        rows.extend((0..st.avr.inputs.len()).map(OptAction::Avr));
        // The receiver must be on (its volume, modes and menu do nothing in standby).
        if st.avr.power == Some(true) {
            // Only when it reports its volume.
            if st.avr.volume.is_some() {
                rows.extend([OptAction::VolumeDown, OptAction::VolumeUp, OptAction::Mute]);
            }
            rows.extend((0..crate::denon::SOUND_MODES.len()).map(OptAction::Sound));
        }
    }
    if e.source.cec && e.source.kind == SourceKind::Hdmi {
        rows.extend([CEC_HOME, CEC_MENU, CEC_INFO, CEC_POWER].map(OptAction::Cec));
    }
    if e.avr && st.avr.power == Some(true) {
        rows.push(OptAction::AvrSetup);
    }
    // Several receivers on the network: which one is behind this input can be chosen here.
    if st.avr.status != crate::denon::Status::Off && st.avr.found.len() > 1 && crate::denon::is_receiver_input(&e.source) {
        rows.push(OptAction::AvrChoose);
    }
    if !e.current {
        rows.push(OptAction::Switch);
    }
    rows.extend([OptAction::Rename, OptAction::Hide]);
    rows
}

/// `sel` as a button index: where the `pinned` action now is when it is still listed, else
/// `sel` (SEL_START, SEL_RENAME and out-of-range values resolved).
fn resolve(sel: usize, rows: &[OptAction], pinned: Option<OptAction>, st: &TvState) -> usize {
    if let Some(i) = pinned.and_then(|p| rows.iter().position(|a| *a == p)) {
        return i;
    }
    if sel == SEL_START {
        let cur = st.avr.selected_input().map(|i| i.code.as_str());
        let ticked = |a: &OptAction| matches!(a, OptAction::Avr(n) if cur.is_some() && st.avr.inputs.get(*n).map(|i| i.code.as_str()) == cur);
        return rows.iter().position(ticked).unwrap_or(0);
    }
    if sel < rows.len() { sel } else { rows.iter().position(|a| *a == OptAction::Rename).unwrap_or(0) }
}

/// Which way the arrows go from button `cur` among `geo` (where each button was drawn):
/// to the nearest one on the next line up or down, or beside it on the same line. Right
/// at the end of a line goes on to the start of the next one; Left at the start of a line
/// back to the end of the one above, when `flow_left` (not where Left leaves the options).
fn step(geo: &[(OptAction, Rect)], cur: OptAction, k: &Nav, flow_left: bool) -> Option<OptAction> {
    let r = geo.iter().find(|(a, _)| *a == cur)?.1;
    let line = |o: &Rect| (o.min.y - r.min.y).abs() < 2.0;
    let dx = |o: &Rect| (o.center().x - r.center().x).abs();
    let pick = |it: Vec<&(OptAction, Rect)>| it.into_iter().min_by(|a, b| dx(&a.1).total_cmp(&dx(&b.1))).map(|(a, _)| *a);
    // The top of the line above and below, if any.
    let above = geo.iter().map(|(_, o)| o.min.y).filter(|y| *y < r.min.y - 2.0).fold(f32::NEG_INFINITY, f32::max);
    let below = geo.iter().map(|(_, o)| o.min.y).filter(|y| *y > r.min.y + 2.0).fold(f32::INFINITY, f32::min);
    let on = |y: f32| move |o: &&(OptAction, Rect)| (o.1.min.y - y).abs() < 2.0;
    let x = |o: &&(OptAction, Rect)| o.1.center().x;
    match k {
        Nav::Left => pick(geo.iter().filter(|(_, o)| line(o) && o.center().x < r.center().x).collect())
            .or_else(|| flow_left.then(|| geo.iter().filter(on(above)).max_by(|a, b| x(a).total_cmp(&x(b))).map(|(a, _)| *a)).flatten()),
        Nav::Right => pick(geo.iter().filter(|(_, o)| line(o) && o.center().x > r.center().x).collect())
            .or_else(|| geo.iter().filter(on(below)).min_by(|a, b| x(a).total_cmp(&x(b))).map(|(a, _)| *a)),
        Nav::Up => pick(geo.iter().filter(on(above)).collect()),
        Nav::Down => pick(geo.iter().filter(on(below)).collect()),
        _ => None,
    }
}

/// How the buttons look: tiles (icon above the label) like a row or grid's, or rows (icon
/// beside it) like a list's.
#[derive(Clone, Copy, PartialEq)]
enum Style {
    Tiles { cols: usize },
    Rows { cols: usize },
}

/// Where everything goes, relative to the top-left of the scrolled content.
struct Placed {
    buttons: Vec<(OptAction, Rect)>,
    headings: Vec<(Group, f32)>,
    height: f32,
}

impl App {
    /// The sound mode to show: the one just asked for (until the receiver reports it, a few
    /// seconds at most), else the receiver's.
    fn sound_shown(&mut self, st: &TvState, now: f64) -> Option<usize> {
        let reported = st.avr.sound.as_deref().and_then(crate::denon::sound_group);
        match self.sound_pending {
            Some((m, at)) if now - at < 4.0 && reported != Some(m) => Some(m),
            _ => {
                self.sound_pending = None;
                reported
            }
        }
    }

    /// The volume and mute to show: those just asked for (until the receiver reports them, a
    /// few seconds at most), else the receiver's.
    fn volume_shown(&mut self, st: &TvState, now: f64) -> (Option<f32>, Option<bool>) {
        let (volume, muted) = (st.avr.volume, st.avr.muted);
        match self.volume_pending {
            Some((v, m, at)) if now - at < 4.0 && (v.is_some_and(|v| Some(v) != volume) || m.is_some_and(|m| Some(m) != muted)) => {
                (v.or(volume), m.or(muted))
            }
            _ => {
                self.volume_pending = None;
                (volume, muted)
            }
        }
    }

    /// The arrow that goes back to the picker from the buttons' edge: the opposite of the
    /// one that opened them (Up under a row or grid, Left beside a list).
    fn options_back_nav(&self) -> Nav {
        if vertical(self.cfg.layout) { Nav::Left } else { Nav::Up }
    }

    pub(super) fn options_key(&mut self, k: &Nav, st: &TvState, now: f64) {
        let Screen::Options { id, mut sel } = self.screen.clone() else { return };
        let all = self.all_entries(st);
        let Some(pos) = all.iter().position(|e| e.source.id == id) else {
            self.screen = Screen::Picker;
            return;
        };
        let e = all[pos].clone();
        let rows = option_rows(&e, st);
        let n = rows.len();
        sel = resolve(sel, &rows, self.opt_sel, st);
        match k {
            Nav::Up | Nav::Down | Nav::Left | Nav::Right => {
                // Among the buttons as they were last drawn (the same ones, or nearly).
                let geo: Vec<(OptAction, Rect)> = self.opt_geo.iter().filter(|(a, _)| rows.contains(a)).copied().collect();
                // Left flows onto the line above, except where it is the way back.
                let flow_left = self.options_back_nav() != Nav::Left;
                let next = if geo.is_empty() {
                    match k {
                        Nav::Up | Nav::Left => sel.checked_sub(1),
                        _ => (sel + 1 < n).then_some(sel + 1),
                    }
                } else {
                    step(&geo, rows[sel], k, flow_left).and_then(|a| rows.iter().position(|r| *r == a))
                };
                match next {
                    Some(i) => sel = i,
                    // From the edge, back to the picker; not a held arrow, which stops there.
                    None if *k == self.options_back_nav() && !self.held => {
                        self.screen = Screen::Picker;
                        return;
                    }
                    None => {}
                }
            }
            Nav::Ok => match rows[sel] {
                // On the Apps tile: its list.
                OptAction::Switch if e.source.kind == SourceKind::Apps => {
                    self.open_apps();
                    return;
                }
                OptAction::Switch => {
                    if !e.current {
                        let _ = self.backend.commands.send(Command::Launch(e.source.app_id.clone(), e.source.params.clone()));
                    }
                    self.close(now);
                    return;
                }
                OptAction::Avr(i) => {
                    if let Some(input) = st.avr.inputs.get(i) {
                        // TV Audio is the TV's own sound: the picture stays where it is.
                        let app_id = crate::denon::is_video(&input.code).then(|| e.source.app_id.clone());
                        let _ = self.backend.commands.send(Command::AvrSelect { app_id, code: input.code.clone() });
                    }
                    self.close(now);
                    return;
                }
                // The options stay open too, the level shown as asked until the receiver says.
                OptAction::VolumeDown | OptAction::VolumeUp | OptAction::Mute => {
                    let (volume, muted) = self.volume_shown(st, now);
                    let (cmd, want) = match rows[sel] {
                        OptAction::Mute if muted == Some(true) => (crate::denon::MUTE_OFF, (None, Some(false))),
                        OptAction::Mute => (crate::denon::MUTE_ON, (None, Some(true))),
                        OptAction::VolumeUp => (crate::denon::VOLUME_UP, (volume.map(|v| (v + 1.0).min(98.0)), None)),
                        _ => (crate::denon::VOLUME_DOWN, (volume.map(|v| (v - 1.0).max(0.0)), None)),
                    };
                    let old = self.volume_pending.filter(|p| now - p.2 < 4.0);
                    self.volume_pending = Some((want.0.or(old.and_then(|p| p.0)), want.1.or(old.and_then(|p| p.1)), now));
                    let _ = self.backend.commands.send(Command::AvrKey { app_id: None, cmd: cmd.to_string() });
                }
                // The options stay open: other modes can be tried in turn.
                OptAction::Sound(m) => {
                    self.sound_pending = Some((m, now));
                    self.sound_asked = Some(m);
                    let cmd = format!("MS{}", crate::denon::SOUND_MODES[m]);
                    let _ = self.backend.commands.send(Command::AvrKey { app_id: None, cmd });
                }
                OptAction::AvrSetup => {
                    let cmd = crate::denon::MENU_ON.to_string();
                    let _ = self.backend.commands.send(Command::AvrKey { app_id: Some(e.source.app_id.clone()), cmd });
                    self.opt_sel = Some(OptAction::AvrSetup);
                    self.screen = Screen::AvrRemote { id };
                    return;
                }
                OptAction::AvrChoose => {
                    self.opt_sel = Some(OptAction::AvrChoose);
                    self.screen = Screen::AvrPick { sel: self.avr_pick_current(st).unwrap_or(0), back: Some(id) };
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
            // Back, the Source key or another long OK closes them.
            Nav::Back | Nav::Erase | Nav::Source | Nav::Menu => {
                self.screen = Screen::Picker;
                return;
            }
            _ => {}
        }
        self.opt_sel = rows.get(sel).copied();
        self.screen = Screen::Options { id, sel };
    }

    /// The buttons' look in this layout, and the width they take.
    fn options_style(&self, rows: &[OptAction]) -> (Style, f32) {
        let s = self.cfg.scale;
        match self.cfg.layout {
            Layout::Row | Layout::Grid => {
                // As many columns as the fullest section, between 4 and the layout's most.
                let most = if self.cfg.layout == Layout::Row { 6 } else { 4 };
                let fullest = [Group::Receiver, Group::Device, Group::Input]
                    .into_iter()
                    .map(|g| rows.iter().filter(|a| group_of(**a) == g).count())
                    .max()
                    .unwrap_or(0);
                let cols = fullest.clamp(4, most);
                (Style::Tiles { cols }, cols as f32 * TILE.x * s + (cols - 1) as f32 * GAP * s)
            }
            Layout::List => (Style::Rows { cols: 2 }, 640.0 * s),
            Layout::Sidebar => (Style::Rows { cols: 2 }, 440.0 * s),
        }
    }

    /// Lays the sections out in `width`: a heading each, then its buttons in lines.
    fn place_options(&self, p: &egui::Painter, rows: &[OptAction], style: Style, width: f32) -> Placed {
        let s = self.cfg.scale;
        let gap = GAP * s;
        let mut out = Placed { buttons: Vec::with_capacity(rows.len()), headings: Vec::new(), height: 0.0 };
        let mut y = 0.0;
        for g in [Group::Receiver, Group::Volume, Group::Sound, Group::Device, Group::Input] {
            let items: Vec<OptAction> = rows.iter().copied().filter(|a| group_of(*a) == g).collect();
            if items.is_empty() {
                continue;
            }
            out.headings.push((g, y));
            y += HEADING * s;
            if matches!(g, Group::Sound | Group::Volume) {
                // Chips as wide as their names (− and + no narrower than a thumb), onto the
                // next line when the line is full.
                let h = CHIP_H * s;
                let mut x = 0.0;
                for a in items {
                    let least = if g == Group::Volume { CHIP_MIN_W * s } else { 0.0 };
                    let w = (text_width(p, chip_label(a), medium(18.0 * s)) + 48.0 * s).max(least);
                    if x > 0.0 && x + w > width {
                        x = 0.0;
                        y += h + gap;
                    }
                    out.buttons.push((a, Rect::from_min_size(pos2(x, y), vec2(w, h))));
                    x += w + gap;
                }
                y += h;
            } else {
                let (cols, size) = match style {
                    Style::Tiles { cols } => (cols, TILE * s),
                    Style::Rows { cols } => (cols, vec2((width - gap * (cols - 1) as f32) / cols as f32, ROW_H * s)),
                };
                for (i, a) in items.iter().enumerate() {
                    let (c, r) = (i % cols, i / cols);
                    out.buttons.push((*a, Rect::from_min_size(pos2(c as f32 * (size.x + gap), y + r as f32 * (size.y + gap)), size)));
                }
                let lines = items.len().div_ceil(cols);
                y += lines as f32 * size.y + (lines - 1) as f32 * gap;
            }
            y += SECTION_GAP * s;
        }
        out.height = (y - SECTION_GAP * s).max(0.0);
        out
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_options(&mut self, ui: &Ui, p: &egui::Painter, st: &TvState, screen: Rect, offset: Vec2, alpha: f32, id: &str, sel: usize) {
        let s = self.cfg.scale;
        let k = self.t;
        let all = self.all_entries(st);
        let Some(e) = all.iter().find(|e| e.source.id == id).cloned() else { return };
        let src = &e.source;
        let rows = option_rows(&e, st);
        let sel = resolve(sel, &rows, self.opt_sel, st);
        let (style, width) = self.options_style(&rows);
        let placed = self.place_options(p, &rows, style, width);

        // What is known of the device: CEC name, maker, the receiver's address, cable and
        // signal (only what is known: webOS 6 never reports the signal).
        let yes_no = |b: bool, y: &'static str, n: &'static str| t(if b { y } else { n }).to_string();
        let receiver = st.avr.device.as_ref().filter(|_| e.avr);
        let facts: Vec<(&str, String)> = [
            src.device.clone().map(|d| (t("info.cec"), d)),
            src.brand.clone().filter(|_| receiver.is_none()).map(|b| (t("info.brand"), b)),
            receiver.map(|r| (t("info.network"), r.ip.clone())),
            (src.kind == SourceKind::Hdmi).then(|| (t("info.cable"), yes_no(src.connected, "value.plugged", "value.unplugged"))),
            src.signal.map(|v| (t("info.signal"), yes_no(v, "value.present", "value.absent"))),
        ]
        .into_iter()
        .flatten()
        .collect();
        // Side by side, two to a line where it is narrow.
        let fact_cols = if width < 600.0 * s { 2 } else { facts.len().max(1) };
        let fact_lines = facts.len().div_ceil(fact_cols);
        let fact_h = if facts.is_empty() { 0.0 } else { fact_lines as f32 * 62.0 * s + 12.0 * s + 22.0 * s };

        let pad = 28.0 * s;
        let head = pad + 38.0 * s + 22.0 * s;
        let foot = 72.0 * s;
        let want = vec2(width + 2.0 * pad, head + fact_h + placed.height + foot + 12.0 * s);
        let rect = self.panel_rect(screen, want).translate(offset);
        self.panel(p, rect, alpha);
        // The Apps tile has no port: its subtitle instead.
        let where_ = if src.kind == SourceKind::Apps { e.sub.clone().unwrap_or_default() } else { src.port_name() };
        self.header(p, rect, None, &e.name, &where_, alpha);
        let left = rect.min.x + pad;
        let inner_w = rect.width() - 2.0 * pad;

        if !facts.is_empty() {
            let info = Rect::from_min_size(pos2(left, rect.min.y + head), vec2(inner_w, fact_h - 22.0 * s));
            p.rect_filled(info, k.radius * 1.5 * s, veil(k.field, alpha * 0.8));
            let cw = info.width() / fact_cols as f32;
            for (i, (label, value)) in facts.iter().enumerate() {
                let x = info.min.x + cw * (i % fact_cols) as f32 + 18.0 * s;
                let y = info.min.y + 6.0 * s + (i / fact_cols) as f32 * 62.0 * s;
                p.text(pos2(x, y + 20.0 * s), Align2::LEFT_CENTER, *label, semibold(14.0 * s), veil(k.text_faint, alpha));
                p.text(pos2(x, y + 44.0 * s), Align2::LEFT_CENTER, fit(p, value, medium(18.0 * s), cw - 26.0 * s), medium(18.0 * s), veil(k.text, alpha));
            }
        }

        // The sections, scrolled to keep the selection in view when they do not all fit.
        let area = Rect::from_min_max(pos2(left, rect.min.y + head + fact_h), pos2(rect.max.x - pad, rect.max.y - foot));
        let overflow = (placed.height - area.height()).max(0.0);
        let sel_rect = placed.buttons.iter().find(|(a, _)| Some(a) == rows.get(sel)).map(|(_, r)| *r);
        let want_scroll = sel_rect.map(|r| (r.center().y - area.height() / 2.0).clamp(0.0, overflow)).unwrap_or(0.0);
        let scroll = self.anim(ui, &format!("options-scroll-{id}"), want_scroll, 0.15);
        let origin = area.min.to_vec2() - vec2(0.0, scroll);
        self.opt_geo = placed.buttons.iter().map(|(a, r)| (*a, r.translate(vec2(0.0, -scroll)))).collect();
        let clip = p.with_clip_rect(area.expand2(vec2(10.0 * s, 8.0 * s)));

        let now = ui.input(|i| i.time);
        let sound = self.sound_shown(st, now);
        let (volume, muted) = self.volume_shown(st, now);
        if self.sound_pending.is_some() || self.volume_pending.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
        }
        let active = st.avr.selected_input().map(|i| i.code.clone());

        // Headings: the section's name, and on its right what applies to all its buttons.
        for (g, y) in &placed.headings {
            let y = origin.y + y + 13.0 * s;
            if y < area.min.y - HEADING * s || y > area.max.y {
                continue;
            }
            let title = clip.text(pos2(left, y), Align2::LEFT_CENTER, g.title(), semibold(14.0 * s), veil(k.text_faint, alpha));
            let note: String = match g {
                Group::Receiver if st.avr.power == Some(false) => t("options.avr_powers_on").into(),
                Group::Receiver => st.avr.device.as_ref().map(|d| d.name.clone()).unwrap_or_default(),
                Group::Volume if muted == Some(true) => t("options.muted").into(),
                Group::Volume => volume.map(volume_text).unwrap_or_default(),
                Group::Sound => st.avr.sound.clone().unwrap_or_default(),
                Group::Device if !e.current && rows.iter().any(|a| matches!(a, OptAction::Cec(_))) => t("options.switches_first").into(),
                _ => String::new(),
            };
            if !note.is_empty() {
                let room = area.max.x - title.max.x - 24.0 * s;
                clip.text(pos2(area.max.x, y), Align2::RIGHT_CENTER, fit(p, &note, regular(15.0 * s), room), regular(15.0 * s), veil(k.text_faint, alpha));
            }
        }

        let base = veil(mix(k.field, k.card, 0.35), alpha * self.cfg.panel_opacity.max(0.75));
        let border = self.border(1.0, veil(k.field_border, alpha * 0.6));
        let r = k.radius_lg * 1.2 * s;
        for (i, (action, at)) in placed.buttons.iter().enumerate() {
            let b = at.translate(origin);
            if b.max.y < area.min.y - 10.0 * s || b.min.y > area.max.y + 10.0 * s {
                continue;
            }
            let i = rows.iter().position(|a| a == action).unwrap_or(i);
            self.add_hit(b.intersect(area), Hit::OptionRow(i));
            let selected = i == sel;

            // Sound modes and volume keys: a chip, filled with the accent when it is the
            // receiver's mode, or for Mute when it is muted.
            if matches!(group_of(*action), Group::Sound | Group::Volume) {
                let on = match action {
                    OptAction::Sound(m) => sound == Some(*m),
                    OptAction::Mute => muted == Some(true),
                    _ => false,
                };
                let radius = b.height() / 2.0;
                let (fill, ink) = if on { (veil(k.accent, alpha), veil(ink_on(k.accent), alpha)) } else if selected { (veil(k.hover, alpha), veil(k.text, alpha)) } else { (base, veil(k.text_dim, alpha)) };
                clip.rect(b, radius, fill, if on { Stroke::NONE } else { border }, StrokeKind::Inside);
                self.rim(&clip, b, radius, alpha * RIM);
                if selected {
                    clip.rect_stroke(b.expand(4.0 * s), radius + 4.0 * s, Stroke::new(3.0 * s, veil(k.ring, alpha)), StrokeKind::Outside);
                }
                // − and + larger: a single sign reads small at 18.
                let size = if matches!(action, OptAction::VolumeDown | OptAction::VolumeUp) { 26.0 } else { 18.0 };
                clip.text(b.center(), Align2::CENTER_CENTER, chip_label(*action), medium(size * s), ink);
                continue;
            }

            let avr_input = match action {
                OptAction::Avr(n) => st.avr.inputs.get(*n),
                _ => None,
            };
            let (icon, label): (&str, &str) = match action {
                OptAction::Switch if src.kind == SourceKind::Apps => ("layout-grid", t("options.btn_open_apps")),
                OptAction::Switch => ("circle-check", t("options.btn_switch")),
                OptAction::Avr(_) => (avr_input.map(|i| crate::denon::icon_for(i).svg()).unwrap_or("cable"), avr_input.map(|i| i.name.as_str()).unwrap_or("")),
                OptAction::Sound(_) | OptAction::VolumeDown | OptAction::VolumeUp | OptAction::Mute => unreachable!(),
                OptAction::AvrSetup => ("settings", t("options.btn_avr_setup")),
                OptAction::AvrChoose => ("speaker", t("options.btn_avr_choose")),
                OptAction::Cec(CEC_POWER) => (
                    "power",
                    match src.cec_power {
                        Some(true) => t("options.btn_power_off"),
                        Some(false) => t("options.btn_power_on"),
                        None => t("options.btn_power"),
                    },
                ),
                OptAction::Cec(CEC_HOME) => ("house", t("options.btn_home")),
                OptAction::Cec(CEC_MENU) => ("menu", t("options.btn_menu")),
                OptAction::Cec(_) => ("info", t("options.btn_info")),
                OptAction::Rename => ("pencil", t("options.rename")),
                OptAction::Hide => ("eye-off", t("options.btn_hide")),
            };
            // The receiver's input in use: ticked, its icon in the accent.
            let current = avr_input.is_some_and(|i| active.as_deref() == Some(i.code.as_str()));

            clip.rect(b, r, if selected { veil(k.hover, alpha) } else { base }, border, StrokeKind::Inside);
            self.rim(&clip, b, r, alpha * RIM);
            if selected {
                clip.rect_stroke(b.expand(4.0 * s), r + 4.0 * s, Stroke::new(3.0 * s, veil(k.ring, alpha)), StrokeKind::Outside);
            }
            let icon_c = if selected || current { k.accent_text } else { k.icon };
            let tick = |at: egui::Pos2| {
                let d = 22.0 * s;
                clip.circle_filled(at, d / 2.0, veil(k.accent, alpha));
                icons::paint(&clip, "check", Rect::from_center_size(at, Vec2::splat(d * 0.62)), veil(ink_on(k.accent), alpha));
            };
            match style {
                Style::Tiles { .. } => {
                    let ir = Rect::from_center_size(pos2(b.center().x, b.min.y + 0.40 * b.height()), Vec2::splat(38.0 * s));
                    icons::paint(&clip, icon, ir, veil(icon_c, alpha));
                    let font = semibold(18.0 * s);
                    clip.text(pos2(b.center().x, b.min.y + 0.76 * b.height()), Align2::CENTER_CENTER, fit(p, label, font.clone(), b.width() - 20.0 * s), font, veil(k.text, alpha));
                    if current {
                        tick(pos2(b.max.x - 19.0 * s, b.min.y + 19.0 * s));
                    }
                }
                Style::Rows { .. } => {
                    let ir = Rect::from_center_size(pos2(b.min.x + 30.0 * s, b.center().y), Vec2::splat(26.0 * s));
                    icons::paint(&clip, icon, ir, veil(icon_c, alpha));
                    let x = b.min.x + 56.0 * s;
                    let room = b.max.x - x - if current { 44.0 * s } else { 14.0 * s };
                    let font = medium(19.0 * s);
                    clip.text(pos2(x, b.center().y), Align2::LEFT_CENTER, fit(p, label, font.clone(), room), font, veil(k.text, alpha));
                    if current {
                        tick(pos2(b.max.x - 24.0 * s, b.center().y));
                    }
                }
            }
        }

        // Content cut by the scroll fades out at that edge, as the picker's does.
        if overflow > 0.0 {
            let outer = area.expand2(vec2(10.0 * s, 8.0 * s));
            let edge = 40.0 * s;
            // Only on a (nearly) opaque panel: over a see-through one the tint shows as a band.
            let fade_c = if self.cfg.panel_opacity >= 0.85 { veil(k.card, alpha * self.cfg.panel_opacity) } else { Color32::TRANSPARENT };
            let clear = Color32::TRANSPARENT;
            if scroll > 0.5 {
                gradient(p, Rect::from_min_max(outer.min, pos2(outer.max.x, outer.min.y + edge)), fade_c, clear, true);
            }
            if scroll < overflow - 0.5 {
                gradient(p, Rect::from_min_max(pos2(outer.min.x, outer.max.y - edge), outer.max), clear, fade_c, true);
            }
        }
        self.footer(p, rect, &[("◀ ▲ ▼ ▶", t("hint.choose")), (t("key.ok"), t("hint.confirm")), (t("key.back"), t("hint.back"))], alpha);
    }
}

/// A tile button (row and grid layouts), and the space between buttons.
const TILE: Vec2 = vec2(164.0, 118.0);
const GAP: f32 = 14.0;
/// A row button (list and sidebar layouts), a sound mode chip.
const ROW_H: f32 = 62.0;
const CHIP_H: f32 = 50.0;
const CHIP_MIN_W: f32 = 84.0;
/// A section's heading line, and the space before the next section.
const HEADING: f32 = 34.0;
const SECTION_GAP: f32 = 22.0;

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: f32, y: f32) -> Rect {
        Rect::from_min_size(pos2(x, y), vec2(100.0, 50.0))
    }

    #[test]
    fn arrows_follow_the_buttons_on_screen() {
        // Two on the first line, three on the second, one wide below.
        let geo = [
            (OptAction::Avr(0), r(0.0, 0.0)),
            (OptAction::Avr(1), r(110.0, 0.0)),
            (OptAction::Cec(CEC_HOME), r(0.0, 100.0)),
            (OptAction::Cec(CEC_MENU), r(110.0, 100.0)),
            (OptAction::Cec(CEC_INFO), r(220.0, 100.0)),
            (OptAction::Rename, r(0.0, 200.0)),
        ];
        let go = |cur: OptAction, k: Nav| step(&geo, cur, &k, true);
        assert_eq!(go(OptAction::Avr(0), Nav::Right), Some(OptAction::Avr(1)));
        assert_eq!(go(OptAction::Avr(0), Nav::Up), None);
        assert_eq!(go(OptAction::Avr(1), Nav::Down), Some(OptAction::Cec(CEC_MENU)));
        assert_eq!(go(OptAction::Cec(CEC_INFO), Nav::Up), Some(OptAction::Avr(1)));
        assert_eq!(go(OptAction::Cec(CEC_INFO), Nav::Down), Some(OptAction::Rename));
        assert_eq!(go(OptAction::Rename, Nav::Down), None);

        // Right at the end of a line goes on to the start of the next, Left at the start of
        // a line back to the end of the one above.
        assert_eq!(go(OptAction::Avr(1), Nav::Right), Some(OptAction::Cec(CEC_HOME)));
        assert_eq!(go(OptAction::Cec(CEC_INFO), Nav::Right), Some(OptAction::Rename));
        assert_eq!(go(OptAction::Cec(CEC_HOME), Nav::Left), Some(OptAction::Avr(1)));
        assert_eq!(go(OptAction::Rename, Nav::Left), Some(OptAction::Cec(CEC_INFO)));
        // Not past the first and last buttons.
        assert_eq!(go(OptAction::Avr(0), Nav::Left), None);
        assert_eq!(go(OptAction::Rename, Nav::Right), None);
        // Where Left goes back to the picker, it does not flow: the start of a line is the edge.
        assert_eq!(step(&geo, OptAction::Cec(CEC_HOME), &Nav::Left, false), None);
        assert_eq!(step(&geo, OptAction::Cec(CEC_MENU), &Nav::Left, false), Some(OptAction::Cec(CEC_HOME)));
        assert_eq!(step(&geo, OptAction::Avr(1), &Nav::Right, false), Some(OptAction::Cec(CEC_HOME)));
    }

    #[test]
    fn options_open_on_the_receivers_input() {
        use crate::denon::{AvrInput, AvrState};
        let input = |code: &str| AvrInput { code: code.into(), name: code.into() };
        let avr = AvrState { power: Some(true), input: Some("GAME".into()), inputs: vec![input("BD"), input("GAME")], ..Default::default() };
        let st = TvState { avr: avr.clone(), ..Default::default() };
        let rows = [OptAction::Avr(0), OptAction::Avr(1), OptAction::Rename, OptAction::Hide];
        assert_eq!(resolve(SEL_START, &rows, None, &st), 1);
        // A key pressed since: where it went stays.
        assert_eq!(resolve(SEL_START, &rows, Some(OptAction::Hide), &st), 3);
        assert_eq!(resolve(0, &rows, None, &st), 0);
        assert_eq!(resolve(SEL_RENAME, &rows, None, &st), 2);
        // Off, or on an input not offered: the first button.
        let off = TvState { avr: AvrState { power: Some(false), ..avr.clone() }, ..Default::default() };
        assert_eq!(resolve(SEL_START, &rows, None, &off), 0);
        let other = TvState { avr: AvrState { input: Some("NET".into()), ..avr }, ..Default::default() };
        assert_eq!(resolve(SEL_START, &rows, None, &other), 0);
    }
}
