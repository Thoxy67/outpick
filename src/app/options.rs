//! A source's options (Down / Right on an HDMI input, or a long OK on any tile): switch
//! to it, the network receiver's inputs behind it, its HDMI-CEC keys, rename, hide.

use super::entries::Entry;
use super::widgets::{fit, keep_visible};
use super::{App, Hit, Nav, Screen};
use crate::i18n::t;
use crate::icons;
use crate::model::{Command, SourceKind, TvState};
use crate::theme::{medium, regular, semibold, veil};
use egui::{Align2, Rect, Ui, Vec2, pos2, vec2};

/// What a row of the options screen does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum OptAction {
    Switch,
    /// Input `n` of the network receiver (`AvrState::inputs`), shown through this input.
    Avr(usize),
    /// The receiver's sound mode, changed with Left/Right.
    AvrSound,
    /// The receiver's setup menu (its remote's Setup key), then its keys from this remote.
    AvrSetup,
    /// Which receiver on the network is this one (several found).
    AvrChoose,
    /// A SIMPLINK key (eim/cec/sendKeyEvent), as LG's screen remote sends them.
    Cec(u32),
    Rename,
    Hide,
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

/// SIMPLINK key codes (LG's ScreenRemote KeyCodeModel.qml).
const CEC_POWER: u32 = 0x0120_2000;
const CEC_HOME: u32 = 0x0120_2001;
const CEC_INFO: u32 = 0x0120_2002;
const CEC_MENU: u32 = 0x0120_2006;

/// `Screen::Options { sel }` meaning "the Rename row" (back from renaming), whatever rows show.
pub(super) const SEL_RENAME: usize = usize::MAX;

/// The rows for source `e`: Switch unless it is already on screen, the receiver's inputs
/// when it is behind this input, the CEC keys only when SIMPLINK reaches its device.
pub(super) fn option_rows(e: &Entry, st: &TvState) -> Vec<OptAction> {
    let mut rows = Vec::with_capacity(12);
    if !e.current {
        rows.push(OptAction::Switch);
    }
    if e.avr {
        rows.extend((0..st.avr.inputs.len()).map(OptAction::Avr));
        // Both need the receiver on (its menu does not show in standby).
        if st.avr.power == Some(true) {
            rows.extend([OptAction::AvrSound, OptAction::AvrSetup]);
        }
    }
    // Several receivers on the network: which one is behind this input can be chosen here.
    if st.avr.status != crate::denon::Status::Off && st.avr.found.len() > 1 && crate::denon::is_receiver_input(&e.source) {
        rows.push(OptAction::AvrChoose);
    }
    if e.source.cec && e.source.kind == SourceKind::Hdmi {
        rows.extend([CEC_POWER, CEC_HOME, CEC_MENU, CEC_INFO].map(OptAction::Cec));
    }
    rows.extend([OptAction::Rename, OptAction::Hide]);
    rows
}

/// `sel` as a row index: where the `pinned` action now is when it is still listed, else
/// `sel` (SEL_RENAME and out-of-range values resolved).
fn resolve(sel: usize, rows: &[OptAction], pinned: Option<OptAction>) -> usize {
    if let Some(i) = pinned.and_then(|p| rows.iter().position(|a| *a == p)) {
        return i;
    }
    if sel < rows.len() { sel } else { rows.iter().position(|a| *a == OptAction::Rename).unwrap_or(0) }
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
        sel = resolve(sel, &rows, self.opt_sel);
        match k {
            // The sound mode row: Left/Right go through the modes (not out of the options).
            Nav::Left | Nav::Right | Nav::Ok if rows[sel] == OptAction::AvrSound => {
                let modes = crate::denon::SOUND_MODES;
                let cur = self.sound_shown(st, now).or(self.sound_asked);
                let next = match (cur, k) {
                    (Some(c), Nav::Left) => (c + modes.len() - 1) % modes.len(),
                    (Some(c), _) => (c + 1) % modes.len(),
                    (None, _) => 0,
                };
                self.sound_pending = Some((next, now));
                self.sound_asked = Some(next);
                let _ = self.backend.commands.send(Command::AvrKey { app_id: None, cmd: format!("MS{}", modes[next]) });
            }
            Nav::Up => sel = (sel + n - 1) % n,
            Nav::Down => sel = (sel + 1) % n,
            Nav::Ok => match rows[sel] {
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
                OptAction::AvrSound => {}
                OptAction::AvrSetup => {
                    let cmd = crate::denon::MENU_ON.to_string();
                    let _ = self.backend.commands.send(Command::AvrKey { app_id: Some(e.source.app_id.clone()), cmd });
                    self.screen = Screen::AvrRemote { id };
                    return;
                }
                OptAction::AvrChoose => {
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
            // The key that opened the options, or Back, closes them.
            Nav::Back | Nav::Erase | Nav::Source | Nav::Left | Nav::Menu => {
                self.screen = Screen::Picker;
                return;
            }
            _ => {}
        }
        self.opt_sel = rows.get(sel).copied();
        self.screen = Screen::Options { id, sel };
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_options(&mut self, ui: &Ui, p: &egui::Painter, st: &TvState, screen: Rect, offset: Vec2, alpha: f32, id: &str, sel: usize) {
        let s = self.cfg.scale;
        let k = self.t;
        let all = self.all_entries(st);
        let Some(e) = all.iter().find(|e| e.source.id == id).cloned() else { return };
        let src = &e.source;
        let rows = option_rows(&e, st);
        let sel = resolve(sel, &rows, self.opt_sel);
        let pad = 28.0 * s;
        let head = pad + 38.0 * s + 18.0 * s;
        let info_h = 96.0 * s;
        let row_h = 52.0 * s;
        let foot = 72.0 * s;
        let want = vec2(760.0 * s, head + info_h + rows.len() as f32 * row_h + foot);
        let rect = self.panel_rect(screen, want).translate(offset);
        self.panel(p, rect, alpha);
        self.header(p, rect, &e.name, &src.port_name(), alpha);

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
        let info = Rect::from_min_size(pos2(rect.min.x + pad, rect.min.y + head), vec2(rect.width() - pad * 2.0, info_h - 16.0 * s));
        p.rect_filled(info, k.radius * 1.5 * s, veil(k.field, alpha * 0.8));
        let cw = info.width() / facts.len().max(1) as f32;
        for (i, (label, value)) in facts.iter().enumerate() {
            let x = info.min.x + cw * i as f32 + 18.0 * s;
            p.text(pos2(x, info.min.y + 24.0 * s), Align2::LEFT_CENTER, *label, semibold(15.0 * s), veil(k.text_faint, alpha));
            p.text(pos2(x, info.min.y + 52.0 * s), Align2::LEFT_CENTER, fit(p, value, medium(18.0 * s), cw - 26.0 * s), medium(18.0 * s), veil(k.text, alpha));
        }

        // The rows, scrolled to keep the selection in view when they do not all fit.
        let area = Rect::from_min_max(pos2(rect.min.x + pad * 0.6, info.max.y + 16.0 * s), pos2(rect.max.x - pad * 0.6, rect.max.y - foot));
        let visible = (area.height() / row_h).floor().max(1.0) as usize;
        let first = keep_visible(sel, visible, rows.len());
        let y0 = self.anim(ui, &format!("options-scroll-{id}"), first as f32 * row_h, 0.15);
        let clip = p.with_clip_rect(area.expand2(vec2(6.0 * s, 0.0)));
        let active = st.avr.selected_input().map(|i| i.code.clone());
        let now = ui.input(|i| i.time);
        let sound = self.sound_shown(st, now);
        if self.sound_pending.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
        }
        for (i, action) in rows.iter().enumerate() {
            let r = Rect::from_min_size(pos2(area.min.x, area.min.y + i as f32 * row_h - y0), vec2(area.width(), row_h - 6.0 * s));
            if r.max.y < area.min.y - row_h || r.min.y > area.max.y + row_h {
                continue;
            }
            self.add_hit(r.intersect(area), Hit::OptionRow(i));
            let selected = i == sel;
            if selected {
                self.selected_row(&clip, r, alpha);
            }
            let avr_input = match action {
                OptAction::Avr(n) => st.avr.inputs.get(*n),
                _ => None,
            };
            let avr_icon = avr_input.map(|i| crate::denon::icon_for(i).svg());
            let (icon, label): (&str, &str) = match action {
                OptAction::Switch => ("circle-check", t("options.switch")),
                OptAction::Avr(_) => (avr_icon.unwrap_or("cable"), avr_input.map(|i| i.name.as_str()).unwrap_or("")),
                OptAction::AvrSound => ("speaker", t("options.avr_sound")),
                OptAction::AvrSetup => ("settings", t("options.avr_setup")),
                OptAction::AvrChoose => ("speaker", t("options.avr_choose")),
                OptAction::Cec(CEC_POWER) => (
                    "power",
                    match src.cec_power {
                        Some(true) => t("options.cec_power_off"),
                        Some(false) => t("options.cec_power_on"),
                        None => t("options.cec_power"),
                    },
                ),
                OptAction::Cec(CEC_HOME) => ("house", t("options.cec_home")),
                OptAction::Cec(CEC_MENU) => ("menu", t("options.cec_menu")),
                OptAction::Cec(_) => ("info", t("options.cec_info")),
                OptAction::Rename => ("pencil", t("options.rename")),
                OptAction::Hide => ("eye-off", t("options.hide")),
            };
            let ir = Rect::from_center_size(pos2(r.min.x + 30.0 * s, r.center().y), Vec2::splat(24.0 * s));
            icons::paint(&clip, icon, ir, veil(if selected { k.accent_text } else { k.icon }, alpha));
            clip.text(pos2(r.min.x + 58.0 * s, r.center().y), Align2::LEFT_CENTER, label, medium(20.0 * s), veil(k.text, alpha));
            if *action == OptAction::AvrSound {
                // ‹ Film ›, with the receiver's own name for the mode on its left.
                let value = sound.map(sound_label).unwrap_or("—");
                let vfont = medium(19.0 * s);
                let chev = 22.0 * s;
                let right = r.max.x - 14.0 * s;
                let vr = clip.text(pos2(right - chev - 8.0 * s, r.center().y), Align2::RIGHT_CENTER, value, vfont, veil(if selected { k.accent_text } else { k.text_dim }, alpha));
                if selected {
                    let ink = veil(k.accent_text, alpha);
                    icons::paint(&clip, "chevron-left", Rect::from_center_size(pos2(vr.min.x - 8.0 * s - chev / 2.0, r.center().y), Vec2::splat(chev)), ink);
                    icons::paint(&clip, "chevron-right", Rect::from_center_size(pos2(right - chev / 2.0, r.center().y), Vec2::splat(chev)), ink);
                }
                if let Some(raw) = st.avr.sound.as_deref() {
                    let room = (vr.min.x - chev - 24.0 * s) - (r.min.x + 58.0 * s + 150.0 * s);
                    clip.text(pos2(vr.min.x - chev - 24.0 * s, r.center().y), Align2::RIGHT_CENTER, fit(p, raw, regular(15.0 * s), room.max(0.0)), regular(15.0 * s), veil(k.text_faint, alpha));
                }
                continue;
            }
            // What OK does besides, where it applies.
            let note = match (action, avr_input) {
                (OptAction::Avr(_), Some(i)) if active.as_deref() == Some(i.code.as_str()) && !crate::denon::is_video(&i.code) => t("options.avr_active"),
                (OptAction::Avr(_), Some(i)) if active.as_deref() == Some(i.code.as_str()) && e.current => t("options.avr_showing"),
                (OptAction::Avr(_), Some(_)) if st.avr.power == Some(false) => t("options.avr_powers_on"),
                (OptAction::AvrChoose, _) => match &st.avr.device {
                    Some(d) => d.ip.as_str(),
                    None => t("options.avr_unsure"),
                },
                // CEC keys go to the active input: say so where it applies.
                (OptAction::Cec(_), _) if !e.current => t("options.switches_first"),
                _ => "",
            };
            clip.text(pos2(r.max.x - 16.0 * s, r.center().y), Align2::RIGHT_CENTER, note, regular(15.0 * s), veil(k.text_faint, alpha));
        }
        if rows.get(sel) == Some(&OptAction::AvrSound) {
            self.footer(p, rect, &[("▲ ▼", t("hint.choose")), ("◀ ▶", t("hint.change")), (t("key.back"), t("hint.back"))], alpha);
        } else {
            self.footer(p, rect, &[("▲ ▼", t("hint.choose")), (t("key.ok"), t("hint.confirm")), (t("key.back"), t("hint.back"))], alpha);
        }
    }
}
