//! Customize's network receiver: which one (Automatic, one of those found, or an address
//! typed by hand), and the address field.

use super::settings::Item;
use super::widgets::{HELP_H, fit, walk};
use super::{App, Hit, Nav, Screen};
use crate::denon::{Protocol, Status, input_of_model, parse_address};
use crate::i18n::{t, tf};
use crate::icons;
use crate::model::TvState;
use crate::theme::{regular, semibold, veil};
use egui::{Align2, Rect, Vec2, pos2, vec2};

/// The receiver in a few words: the one in use, or why there is none.
pub(super) fn status_text(st: &TvState) -> String {
    let a = &st.avr;
    match (a.status, &a.device) {
        (Status::Off, _) => t("summary.avr_off").into(),
        (Status::Unreachable, Some(d)) => tf("avr.unreachable", &[("name", &d.name)]),
        (Status::Ready | Status::Searching, Some(d)) => {
            let via = if a.link == Some(Protocol::Telnet) { " · telnet" } else { "" };
            format!("{} · {}{via}", d.name, d.ip)
        }
        (Status::Searching, None) => t("avr.searching").into(),
        (Status::Ambiguous, _) => tf("avr.ambiguous", &[("n", &a.found.len().to_string())]),
        _ => t("avr.none").into(),
    }
}

pub(super) fn protocol_label(p: Protocol) -> &'static str {
    match p {
        Protocol::Auto => t("avr.protocol_auto"),
        Protocol::Http => "HTTP",
        Protocol::Telnet => "Telnet",
    }
}

/// A row of the receiver list.
enum Row {
    Auto,
    Found(usize),
    Typed,
}

fn rows(st: &TvState) -> Vec<Row> {
    std::iter::once(Row::Auto).chain((0..st.avr.found.len()).map(Row::Found)).chain(std::iter::once(Row::Typed)).collect()
}

impl App {
    /// The row of the choice in use; None when it is a receiver not found this time.
    pub(super) fn avr_pick_current(&self, st: &TvState) -> Option<usize> {
        let c = &self.cfg.avr;
        if c.ip.is_some() {
            return Some(rows(st).len() - 1);
        }
        match &c.device {
            Some(mac) => st.avr.found.iter().position(|r| r.mac == *mac).map(|i| i + 1),
            None => Some(0),
        }
    }

    pub(super) fn avr_pick_key(&mut self, k: &Nav, st: &TvState) {
        let Screen::AvrPick { mut sel, back } = self.screen.clone() else { return };
        // Where Back and a choice lead: the options it was opened from, else Customize.
        let leave = |app: &Self| match &back {
            Some(id) => Screen::Options { id: id.clone(), sel: 0 },
            None => app.settings_at(Item::AvrDevice),
        };
        let list = rows(st);
        let n = list.len();
        sel = sel.min(n - 1);
        match k {
            Nav::Up => sel = walk(sel, n, -1, self.held),
            Nav::Down => sel = walk(sel, n, 1, self.held),
            Nav::Left if self.held => {}
            Nav::Ok => {
                match list[sel] {
                    Row::Auto => {
                        self.cfg.avr.device = None;
                        self.cfg.avr.ip = None;
                    }
                    Row::Found(i) => {
                        self.cfg.avr.device = Some(st.avr.found[i].mac.clone());
                        self.cfg.avr.ip = None;
                    }
                    Row::Typed => {
                        self.screen = Screen::IpEntry { text: self.cfg.avr.ip.clone().unwrap_or_default(), back, bad: false };
                        return;
                    }
                }
                self.persist();
                self.screen = leave(self);
                return;
            }
            Nav::Back | Nav::Erase | Nav::Source | Nav::Left => {
                self.screen = leave(self);
                return;
            }
            _ => {}
        }
        self.screen = Screen::AvrPick { sel, back };
    }

    /// The address field: digits, dots and a colon; OK keeps a valid address (empty: back
    /// to Automatic), Back cancels.
    pub(super) fn ip_key(&mut self, k: &Nav) {
        let Screen::IpEntry { text, back, bad } = &mut self.screen else { return };
        let back = back.clone();
        match k {
            Nav::Text(s) => {
                *bad = false;
                for c in s.chars().filter(|c| c.is_ascii_digit() || *c == '.' || *c == ':') {
                    if text.len() < 21 {
                        text.push(c);
                    }
                }
            }
            Nav::Erase => {
                *bad = false;
                text.pop();
            }
            Nav::Ok => {
                let typed = text.trim().to_string();
                if typed.is_empty() {
                    self.cfg.avr.ip = None;
                } else if parse_address(&typed).is_some() {
                    self.cfg.avr.ip = Some(typed);
                } else {
                    *bad = true; // not an address yet: keep typing
                    return;
                }
                self.persist();
                self.screen = match back {
                    Some(id) => Screen::Options { id, sel: 0 },
                    None => self.settings_at(Item::AvrDevice),
                };
            }
            Nav::Back | Nav::Source => {
                // Back on the "IP address…" row, the last one (after Automatic and those found).
                let found = self.backend.state.lock().map(|s| s.avr.found.len()).unwrap_or(0);
                self.screen = Screen::AvrPick { sel: found + 1, back };
            }
            _ => {}
        }
    }

    /// The receiver's setup menu: this remote's keys go to it (Back included: its own
    /// Return) until a long OK or Source comes back to the options; a long Back closes all.
    pub(super) fn avr_remote_key(&mut self, k: &Nav) {
        use crate::denon::{MENU_BACK, MENU_DOWN, MENU_ENTER, MENU_LEFT, MENU_OFF, MENU_RIGHT, MENU_UP};
        let Screen::AvrRemote { id } = self.screen.clone() else { return };
        let cmd = match k {
            Nav::Up => MENU_UP,
            Nav::Down => MENU_DOWN,
            Nav::Left => MENU_LEFT,
            Nav::Right => MENU_RIGHT,
            Nav::Ok => MENU_ENTER,
            Nav::Back | Nav::Erase => MENU_BACK,
            // Long OK or Source: done with the menu, back to the options.
            Nav::Menu | Nav::Source => {
                let _ = self.backend.commands.send(crate::model::Command::AvrKey { app_id: None, cmd: MENU_OFF.into() });
                self.screen = Screen::Options { id, sel: 0 };
                return;
            }
            _ => return,
        };
        let _ = self.backend.commands.send(crate::model::Command::AvrKey { app_id: None, cmd: cmd.into() });
    }

    /// A slim bar at the screen's bottom while the receiver's menu shows above it.
    pub(super) fn draw_avr_remote(&mut self, p: &egui::Painter, st: &TvState, screen: Rect, alpha: f32) {
        let s = self.cfg.scale;
        let hints = [("◀ ▲ ▼ ▶", t("hint.move")), (t("key.ok"), t("hint.confirm")), (t("key.back"), t("hint.back")), (t("key.back_hold"), t("hint.close"))];
        let width = (self.footer_width(p, &hints, 1.0) + 56.0 * s).max(720.0 * s).min(screen.width() - 80.0 * s);
        let rect = Rect::from_min_size(pos2(screen.center().x - width / 2.0, screen.max.y - 40.0 * s - 132.0 * s), vec2(width, 132.0 * s));
        self.panel(p, rect, alpha);
        let name = st.avr.device.as_ref().map(|d| d.name.as_str()).unwrap_or("");
        self.header(p, rect, None, t("avr.menu_title"), name, alpha);
        self.footer(p, rect, &hints, alpha);
    }

    pub(super) fn draw_avr_pick(&mut self, p: &egui::Painter, st: &TvState, screen: Rect, offset: Vec2, alpha: f32, sel: usize) {
        let s = self.cfg.scale;
        let k = self.t;
        let list = rows(st);
        let sel = sel.min(list.len() - 1);
        let current = self.avr_pick_current(st);
        let pad = 28.0 * s;
        let head = pad + 38.0 * s + 18.0 * s;
        let row_h = 74.0 * s;
        let foot = 72.0 * s;
        let help_h = HELP_H * s;
        let want = vec2(860.0 * s, head + list.len() as f32 * row_h + help_h + foot);
        let rect = self.panel_rect(screen, want).translate(offset);
        self.panel(p, rect, alpha);
        let subtitle = match (st.avr.status, st.avr.found.len()) {
            (Status::Searching, _) => t("avr.searching").to_string(),
            (_, 0) => t("avr.none").to_string(),
            (_, 1) => t("avr.found_one").to_string(),
            (_, n) => tf("avr.found", &[("n", &n.to_string())]),
        };
        self.header(p, rect, None, t("avr.title"), &subtitle, alpha);

        let area = Rect::from_min_max(pos2(rect.min.x + pad * 0.6, rect.min.y + head), pos2(rect.max.x - pad * 0.6, rect.max.y - foot - help_h));
        let visible = (area.height() / row_h).floor().max(1.0) as usize;
        let first = super::widgets::keep_visible(sel, visible, list.len());
        let clip = p.with_clip_rect(area.expand2(vec2(6.0 * s, 0.0)));
        let auto_name = st.avr.auto_pick.as_ref().and_then(|mac| st.avr.found.iter().find(|r| r.mac == *mac)).map(|r| format!("{} · {}", r.name, r.ip));
        for (i, row) in list.iter().enumerate().skip(first).take(visible) {
            let r = Rect::from_min_size(pos2(area.min.x, area.min.y + (i - first) as f32 * row_h), vec2(area.width(), row_h - 8.0 * s));
            self.add_hit(r.intersect(area), Hit::OptionRow(i));
            let selected = i == sel;
            if selected {
                self.selected_row(&clip, r, alpha);
            }
            let (icon, title, detail) = match row {
                Row::Auto => (
                    "rotate-ccw",
                    t("avr.auto").to_string(),
                    match (&auto_name, st.avr.status) {
                        (Some(n), _) => tf("avr.auto_takes", &[("name", n)]),
                        (None, Status::Searching) => t("avr.searching").to_string(),
                        (None, _) if st.avr.found.len() > 1 => t("avr.auto_unsure").to_string(),
                        (None, _) => t("avr.none").to_string(),
                    },
                ),
                Row::Found(n) => {
                    let r = &st.avr.found[*n];
                    let mut detail = format!("{} {} · {}", r.brand, r.model, r.ip);
                    // The one whose model this TV sees over HDMI-CEC says where it is plugged in.
                    if let Some(input) = input_of_model(&st.sources, r) {
                        detail.push_str(" · ");
                        detail.push_str(&tf("avr.on_input", &[("input", &input.replace('_', " "))]));
                    }
                    ("speaker", r.name.clone(), detail)
                }
                Row::Typed => (
                    "pencil",
                    t("avr.typed").to_string(),
                    self.cfg.avr.ip.clone().unwrap_or_else(|| t("avr.typed_help").to_string()),
                ),
            };
            let ir = Rect::from_center_size(pos2(r.min.x + 30.0 * s, r.center().y), Vec2::splat(26.0 * s));
            icons::paint(&clip, icon, ir, veil(if selected { k.accent_text } else { k.icon }, alpha));
            let x = r.min.x + 60.0 * s;
            let maxw = r.max.x - x - 56.0 * s;
            clip.text(pos2(x, r.center().y - 12.0 * s), Align2::LEFT_CENTER, fit(p, &title, semibold(20.0 * s), maxw), semibold(20.0 * s), veil(k.text, alpha));
            clip.text(pos2(x, r.center().y + 14.0 * s), Align2::LEFT_CENTER, fit(p, &detail, regular(16.0 * s), maxw), regular(16.0 * s), veil(k.text_dim, alpha));
            if Some(i) == current {
                let cr = Rect::from_center_size(pos2(r.max.x - 28.0 * s, r.center().y), Vec2::splat(24.0 * s));
                icons::paint(&clip, "check", cr, veil(if selected { k.accent_text } else { k.accent }, alpha));
            }
        }
        // What the selected row does; with none found, what to check first.
        let help = match list[sel] {
            _ if st.avr.found.is_empty() && st.avr.status != Status::Searching => "help.avr_pick_none",
            Row::Auto => "help.avr_pick_auto",
            Row::Found(_) => "help.avr_pick_found",
            Row::Typed => "help.avr_pick_typed",
        };
        self.help_line(p, Rect::from_min_max(pos2(area.min.x, area.max.y), pos2(area.max.x, area.max.y + help_h)), t(help), alpha);
        self.footer(p, rect, &[("▲ ▼", t("hint.choose")), (t("key.ok"), t("hint.confirm")), (t("key.back"), t("hint.back"))], alpha);
    }
}
