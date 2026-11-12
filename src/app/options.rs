//! The HDMI-CEC options of an input.

use super::entries::Entry;
use super::widgets::fit;
use super::{App, Hit, Nav, Screen};
use crate::i18n::t;
use crate::icons;
use crate::model::{Command, TvState};
use crate::theme::{medium, regular, semibold, veil};
use egui::{Align2, Rect, Ui, Vec2, pos2, vec2};

/// What a row of the options screen does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum OptAction {
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

/// `Screen::Options { sel }` meaning "the Rename row" (back from renaming), whatever rows show.
pub(super) const SEL_RENAME: usize = usize::MAX;

/// The rows for input `e`: Switch unless it is already on screen, the CEC keys only when
/// SIMPLINK reaches its device.
pub(super) fn option_rows(e: &Entry) -> Vec<OptAction> {
    let mut rows = Vec::with_capacity(7);
    if !e.current {
        rows.push(OptAction::Switch);
    }
    if e.source.cec {
        rows.extend([CEC_POWER, CEC_HOME, CEC_MENU, CEC_INFO].map(OptAction::Cec));
    }
    rows.extend([OptAction::Rename, OptAction::Hide]);
    rows
}

/// `sel` as a row index (SEL_RENAME and out-of-range values resolved).
fn resolve(sel: usize, rows: &[OptAction]) -> usize {
    if sel < rows.len() { sel } else { rows.iter().position(|a| *a == OptAction::Rename).unwrap_or(0) }
}

impl App {
    pub(super) fn options_key(&mut self, k: &Nav, st: &TvState, now: f64) {
        let Screen::Options { id, mut sel } = self.screen.clone() else { return };
        let all = self.all_entries(st);
        let Some(pos) = all.iter().position(|e| e.source.id == id) else {
            self.screen = Screen::Picker;
            return;
        };
        let e = all[pos].clone();
        let rows = option_rows(&e);
        let n = rows.len();
        sel = resolve(sel, &rows);
        match k {
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

    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_options(&mut self, ui: &Ui, p: &egui::Painter, st: &TvState, screen: Rect, offset: Vec2, alpha: f32, id: &str, sel: usize) {
        let s = self.cfg.scale;
        let k = self.t;
        let all = self.all_entries(st);
        let Some(e) = all.iter().find(|e| e.source.id == id).cloned() else { return };
        let src = &e.source;
        let rows = option_rows(&e);
        let sel = resolve(sel, &rows);
        let pad = 28.0 * s;
        let head = pad + 38.0 * s + 18.0 * s;
        let info_h = 96.0 * s;
        let row_h = 52.0 * s;
        let foot = 72.0 * s;
        let want = vec2(760.0 * s, head + info_h + rows.len() as f32 * row_h + foot);
        let rect = self.panel_rect(screen, want).translate(offset);
        self.panel(p, rect, alpha);
        self.header(p, rect, &e.name, &src.port_name(), alpha);

        // What the TV knows about the device: CEC name, maker, cable and signal (only
        // what is known: webOS 6 never reports the signal).
        let yes_no = |b: bool, y: &'static str, n: &'static str| t(if b { y } else { n }).to_string();
        let facts: Vec<(&str, String)> = [
            src.device.clone().map(|d| (t("info.cec"), d)),
            src.brand.clone().map(|b| (t("info.brand"), b)),
            Some((t("info.cable"), yes_no(src.connected, "value.plugged", "value.unplugged"))),
            src.signal.map(|v| (t("info.signal"), yes_no(v, "value.present", "value.absent"))),
        ]
        .into_iter()
        .flatten()
        .collect();
        let info = Rect::from_min_size(pos2(rect.min.x + pad, rect.min.y + head), vec2(rect.width() - pad * 2.0, info_h - 16.0 * s));
        p.rect_filled(info, k.radius * 1.5 * s, veil(k.field, alpha * 0.8));
        let cw = info.width() / facts.len() as f32;
        for (i, (label, value)) in facts.iter().enumerate() {
            let x = info.min.x + cw * i as f32 + 18.0 * s;
            p.text(pos2(x, info.min.y + 24.0 * s), Align2::LEFT_CENTER, *label, semibold(15.0 * s), veil(k.text_faint, alpha));
            p.text(pos2(x, info.min.y + 52.0 * s), Align2::LEFT_CENTER, fit(p, value, medium(18.0 * s), cw - 26.0 * s), medium(18.0 * s), veil(k.text, alpha));
        }

        let area = Rect::from_min_max(pos2(rect.min.x + pad * 0.6, info.max.y + 16.0 * s), pos2(rect.max.x - pad * 0.6, rect.max.y - foot));
        let clip = p.with_clip_rect(area.expand2(vec2(6.0 * s, 0.0)));
        for (i, action) in rows.iter().enumerate() {
            let r = Rect::from_min_size(pos2(area.min.x, area.min.y + i as f32 * row_h), vec2(area.width(), row_h - 6.0 * s));
            self.add_hit(r.intersect(area), Hit::OptionRow(i));
            let selected = i == sel;
            if selected {
                self.selected_row(&clip, r, alpha);
            }
            let (icon, label) = match action {
                OptAction::Switch => ("circle-check", t("options.switch")),
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
            if matches!(action, OptAction::Cec(_)) {
                // CEC keys go to the active input: say so where it applies.
                let note = if e.current { "" } else { t("options.switches_first") };
                clip.text(pos2(r.max.x - 16.0 * s, r.center().y), Align2::RIGHT_CENTER, note, regular(15.0 * s), veil(k.text_faint, alpha));
            }
        }
        self.footer(p, rect, &[("▲ ▼", t("hint.choose")), (t("key.ok"), t("hint.confirm")), (t("key.back"), t("hint.back"))], alpha);
        let _ = ui;
    }
}
