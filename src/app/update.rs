//! The Update screen: what changed in every version since this one (the changelog's
//! sections), scrolled with Up/Down, and the install through the Homebrew Channel, with its
//! progress. Opening it is the confirmation: OK installs.

use super::settings::Item;
use super::widgets::{fit, gradient, text_width};
use super::{App, Nav, Screen};
use crate::i18n::{t, tf};
use crate::icons;
use crate::model::{Command, Install, Notes, TvState};
use crate::theme::{regular, semibold, veil};
use crate::update::{INSTALLED, Offer, printable};
use egui::text::{LayoutJob, TextFormat};
use egui::{Align2, Color32, Rect, Vec2, pos2, vec2};

/// A line of the notes as drawn, its text already reduced to what the font has.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum NoteLine {
    Version { version: String, date: String },
    Title(String),
    Bullet { area: Option<String>, text: String },
    /// The notes were cut (`update::NOTES_MAX`).
    More,
}

/// The Update row's value: checking, up to date, the version found, or why not.
pub(super) fn status_text(st: &TvState) -> String {
    let u = &st.update;
    if u.checking {
        return t("update.checking").into();
    }
    if let Some(o) = &u.offer {
        return tf("update.available", &[("version", &o.version)]);
    }
    match &u.checked {
        None => t("update.never").into(),
        Some(Ok(())) => tf("update.up_to_date", &[("version", INSTALLED)]),
        Some(Err(e)) => tf("update.failed", &[("error", e)]),
    }
}

/// A note line laid out: the gap above it, its indent, its text, the date on its right
/// (versions), whether it is a bullet.
type Laid = (f32, f32, std::sync::Arc<egui::Galley>, Option<std::sync::Arc<egui::Galley>>, bool);

/// Points scrolled by one press of Up or Down (× scale).
const STEP: f32 = 120.0;

impl App {
    /// The offer to mention outside Customize's Update row (the Customize tile, the General
    /// section's summary): only while the checks are on.
    pub(super) fn update_notice<'a>(&self, st: &'a TvState) -> Option<&'a Offer> {
        st.update.offer.as_ref().filter(|_| self.cfg.check_updates)
    }

    /// Opens the Update screen at the top, and has its notes read (again, if they failed).
    pub(super) fn open_update(&mut self, st: &TvState) {
        self.update_scroll = 0.0;
        if matches!(st.update.notes, None | Some(Notes::Failed(_))) {
            let _ = self.backend.commands.send(Command::UpdateNotes);
        }
        self.screen = Screen::Update;
    }

    /// New notes: their text as the font draws it, once.
    pub(super) fn sync_update_notes(&mut self, ctx: &egui::Context, st: &TvState) {
        if self.update_notes_seq == st.update.notes_seq {
            return;
        }
        self.update_notes_seq = st.update.notes_seq;
        self.update_lines.clear();
        let Some(Notes::Ready(notes)) = &st.update.notes else { return };
        let font = semibold(20.0);
        self.update_lines = ctx.fonts_mut(|f| {
            let mut has = |c: char| f.has_glyph(&font, c);
            let mut lines = Vec::new();
            for r in &notes.releases {
                lines.push(NoteLine::Version { version: printable(&r.version, &mut has), date: printable(&r.date, &mut has) });
                for e in &r.entries {
                    let title = printable(&e.title, &mut has);
                    if !title.is_empty() {
                        lines.push(NoteLine::Title(title));
                    }
                    for b in &e.bullets {
                        let text = printable(&b.text, &mut has);
                        let area = b.area.as_deref().map(|a| printable(a, &mut has)).filter(|a| !a.is_empty());
                        if !text.is_empty() || area.is_some() {
                            lines.push(NoteLine::Bullet { area, text });
                        }
                    }
                }
            }
            if notes.truncated {
                lines.push(NoteLine::More);
            }
            lines
        });
    }

    /// Up/Down scroll (stopping at the ends), OK installs (again after a failure; closes
    /// once installed), Back or Left go back to Customize. Back during an install only
    /// leaves the screen: the Homebrew Channel goes on.
    pub(super) fn update_key(&mut self, k: &Nav, st: &TvState, now: f64) {
        let step = STEP * self.cfg.scale;
        match k {
            Nav::Up => self.update_scroll = (self.update_scroll - step).max(0.0),
            Nav::Down => self.update_scroll = (self.update_scroll + step).min(self.update_max),
            Nav::Left | Nav::Right if self.held => {}
            Nav::Back | Nav::Erase | Nav::Left => self.screen = self.settings_at(Item::Update),
            Nav::Source => {
                self.persist();
                self.screen = Screen::Picker;
            }
            Nav::Ok => match &st.update.install {
                None | Some(Install::Failed(_)) if st.update.offer.is_some() => {
                    // Shown at once; the backend keeps a second press from starting another.
                    if let Ok(mut shared) = self.backend.state.lock() {
                        shared.update.install = Some(Install::Running { status: String::new(), progress: None });
                    }
                    let _ = self.backend.commands.send(Command::InstallUpdate);
                }
                Some(Install::Done) => self.close(now),
                _ => {}
            },
            _ => {}
        }
    }

    pub(super) fn draw_update(&mut self, ui: &egui::Ui, p: &egui::Painter, st: &TvState, screen: Rect, offset: Vec2, alpha: f32) {
        // The offer went (a check found nothing newer) before anything was asked of it.
        if st.update.offer.is_none() && st.update.install.is_none() {
            self.screen = self.settings_at(Item::Update);
        }
        let s = self.cfg.scale;
        let k = self.t;
        let pad = 28.0 * s;
        let head = pad + 38.0 * s + 18.0 * s;
        let foot = 72.0 * s;
        let status_h = 92.0 * s;
        // As tall as the screen allows: the notes are what this screen is for.
        let rect = self.panel_rect(screen, vec2(940.0 * s, 860.0 * s)).translate(offset);
        self.panel(p, rect, alpha);
        let to = st.update.offer.as_ref().map(|o| o.version.clone()).unwrap_or_default();
        self.header(p, rect, Some("download"), t("update.title"), &format!("{INSTALLED} → {to}"), alpha);
        let area = Rect::from_min_max(pos2(rect.min.x + pad, rect.min.y + head), pos2(rect.max.x - pad, rect.max.y - foot - status_h));

        // The notes: loading, unavailable (why, and that the install is still possible), none,
        // or scrolled.
        let message: Option<(&str, String, &str)> = match &st.update.notes {
            Some(Notes::Ready(_)) if !self.update_lines.is_empty() => None,
            Some(Notes::Ready(_)) => Some((t("update.notes_empty"), String::new(), "")),
            Some(Notes::Failed(e)) => Some((t("update.notes_error"), e.clone(), t("update.notes_error_hint"))),
            _ => Some((t("update.notes_loading"), String::new(), "")),
        };
        self.update_max = 0.0;
        if let Some((title, detail, hint)) = message {
            let c = area.center();
            let room = area.width() - 40.0 * s;
            let lines = [(title, semibold(21.0 * s), k.text), (detail.as_str(), regular(16.5 * s), k.text_faint), (hint, regular(17.0 * s), k.text_dim)];
            let shown: Vec<_> = lines.into_iter().filter(|(text, ..)| !text.is_empty()).collect();
            let gap = 30.0 * s;
            let mut y = c.y - gap * (shown.len() as f32 - 1.0) / 2.0;
            for (text, font, ink) in shown {
                p.text(pos2(c.x, y), Align2::CENTER_CENTER, fit(p, text, font.clone(), room), font, veil(ink, alpha));
                y += gap;
            }
        } else {
            self.draw_notes(ui, p, area, alpha);
        }

        // Under the notes: what OK does, or how the install goes.
        let band = Rect::from_min_max(pos2(rect.min.x + pad * 0.6, area.max.y + 4.0 * s), pos2(rect.max.x - pad * 0.6, area.max.y + status_h));
        let version = [("version", to.as_str())];
        match &st.update.install {
            None => self.help_line(p, band, &tf("update.install_help", &version), alpha),
            Some(Install::Done) => self.help_line(p, band, &tf("update.install_done", &version), alpha),
            Some(Install::Failed(e)) => self.help_line(p, band, &tf("update.install_failed", &[("error", e)]), alpha),
            Some(Install::Running { status, progress }) => self.draw_install_progress(p, band, status, *progress, alpha),
        }

        let scroll = ("▲ ▼", t("hint.scroll"));
        let back = (t("key.back"), t("hint.back"));
        let mut hints: Vec<(&str, &str)> = match &st.update.install {
            None => vec![scroll, (t("key.ok"), t("hint.install")), (t("key.back"), t("hint.cancel"))],
            Some(Install::Running { .. }) => vec![scroll, back],
            Some(Install::Failed(_)) => vec![scroll, (t("key.ok"), t("hint.retry")), back],
            Some(Install::Done) => vec![(t("key.ok"), t("hint.close")), back],
        };
        // Scrolling is offered only when there is something to scroll.
        if self.update_max <= 0.0 {
            hints.retain(|h| *h != scroll);
        }
        self.footer(p, rect, &hints, alpha);
    }

    /// The notes in `area`, from `update_scroll` (kept within the notes' height).
    fn draw_notes(&mut self, ui: &egui::Ui, p: &egui::Painter, area: Rect, alpha: f32) {
        let s = self.cfg.scale;
        let k = self.t;
        let bar = 14.0 * s;
        let width = area.width() - bar;
        let indent = 22.0 * s;
        // Each line laid out once a frame (egui caches the layouts), with the gap above it.
        let mut laid: Vec<Laid> = Vec::new();
        for (i, line) in self.update_lines.iter().enumerate() {
            let (gap, x, galley, right, bullet) = match line {
                NoteLine::Version { version, date } => {
                    let g = p.layout_no_wrap(tf("update.version", &[("version", version)]), semibold(23.0 * s), veil(k.accent_text, alpha));
                    let d = (!date.is_empty()).then(|| p.layout_no_wrap(date.clone(), regular(16.5 * s), veil(k.text_faint, alpha)));
                    (if i == 0 { 0.0 } else { 30.0 * s }, 0.0, g, d, false)
                }
                NoteLine::Title(text) => (14.0 * s, 0.0, p.layout(text.clone(), semibold(18.5 * s), veil(k.text, alpha), width), None, false),
                NoteLine::Bullet { area: label, text } => {
                    let mut job = LayoutJob::default();
                    if let Some(label) = label {
                        job.append(&format!("{label}: "), 0.0, TextFormat::simple(semibold(16.5 * s), veil(k.text, alpha)));
                    }
                    job.append(text, 0.0, TextFormat::simple(regular(16.5 * s), veil(k.text_dim, alpha)));
                    job.wrap.max_width = width - indent;
                    (7.0 * s, indent, p.layout_job(job), None, true)
                }
                NoteLine::More => (16.0 * s, 0.0, p.layout(t("update.notes_more").to_string(), regular(16.5 * s), veil(k.text_faint, alpha), width), None, false),
            };
            laid.push((gap, x, galley, right, bullet));
        }
        let total: f32 = laid.iter().map(|(gap, _, g, ..)| gap + g.size().y).sum::<f32>() + 8.0 * s;
        self.update_max = (total - area.height()).max(0.0);
        self.update_scroll = self.update_scroll.clamp(0.0, self.update_max);
        let y0 = self.anim(ui, "update-scroll", self.update_scroll, 0.15);
        let clip = p.with_clip_rect(area);
        let mut y = area.min.y - y0;
        for (i, (gap, x, galley, right, bullet)) in laid.into_iter().enumerate() {
            y += gap;
            let h = galley.size().y;
            if y + h >= area.min.y && y <= area.max.y {
                if matches!(self.update_lines[i], NoteLine::Version { .. }) && i > 0 {
                    // A hairline between versions.
                    clip.rect_filled(Rect::from_min_size(pos2(area.min.x, y - 15.0 * s), vec2(width, 1.0 * s)), 0.0, veil(k.separator, alpha));
                }
                if bullet {
                    let row = regular(16.5 * s).size * 0.62;
                    clip.circle_filled(pos2(area.min.x + 8.0 * s, y + row), 2.6 * s, veil(k.text_faint, alpha));
                }
                if let Some(d) = right {
                    clip.galley(pos2(area.min.x + width - d.size().x, y + (h - d.size().y) / 2.0), d, veil(k.text_faint, alpha));
                }
                clip.galley(pos2(area.min.x + x, y), galley, veil(k.text, alpha));
            }
            y += h;
        }
        if self.update_max <= 0.0 {
            return;
        }
        // Where the view is in the notes, on the right.
        let track = Rect::from_min_max(pos2(area.max.x - 4.0 * s, area.min.y), pos2(area.max.x, area.max.y));
        clip.rect_filled(track, 2.0 * s, veil(k.separator, alpha));
        let thumb_h = (track.height() * area.height() / total).max(28.0 * s);
        let at = track.min.y + (track.height() - thumb_h) * (y0 / self.update_max).clamp(0.0, 1.0);
        clip.rect_filled(Rect::from_min_size(pos2(track.min.x, at), vec2(track.width(), thumb_h)), 2.0 * s, veil(k.text_faint, alpha));
        // What the scroll cuts fades out at that edge, as the Apps list's does (not on glass).
        if self.cfg.panel_opacity >= 0.85 && !k.glass {
            let edge = 36.0 * s;
            let (fade_c, clear) = (veil(k.card, alpha * self.cfg.panel_opacity), Color32::TRANSPARENT);
            let left = area.min.x - 4.0 * s;
            let right = track.min.x - 4.0 * s;
            if y0 > 0.5 {
                gradient(p, Rect::from_min_max(pos2(left, area.min.y), pos2(right, area.min.y + edge)), fade_c, clear, true);
            }
            if y0 < self.update_max - 0.5 {
                gradient(p, Rect::from_min_max(pos2(left, area.max.y - edge), pos2(right, area.max.y)), clear, fade_c, true);
            }
        }
    }

    /// The install going on: its stage, a bar while downloading, and that leaving does not
    /// stop it.
    fn draw_install_progress(&self, p: &egui::Painter, band: Rect, status: &str, progress: Option<f32>, alpha: f32) {
        let s = self.cfg.scale;
        let k = &self.t;
        let r = Rect::from_min_max(pos2(band.min.x + 20.0 * s, band.min.y + 8.0 * s), pos2(band.max.x - 20.0 * s, band.max.y));
        p.rect_filled(Rect::from_min_size(pos2(band.min.x + 14.0 * s, r.min.y), vec2(band.width() - 28.0 * s, 1.0 * s)), 0.0, veil(k.separator, alpha));
        // The Homebrew Channel's own stages, in this language.
        let stage = if status.starts_with("Downloading") {
            t("update.downloading").to_string()
        } else if status.starts_with("Verifying") {
            t("update.verifying").to_string()
        } else if status.starts_with("Installing") {
            t("update.installing").to_string()
        } else if status.is_empty() {
            t("update.starting").to_string()
        } else {
            status.chars().filter(|c| c.is_ascii_graphic() || *c == ' ').collect()
        };
        let cy = r.min.y + 24.0 * s;
        let ir = Rect::from_center_size(pos2(r.min.x + 9.0 * s, cy), Vec2::splat(18.0 * s));
        icons::paint(p, "download", ir, veil(k.accent_text, alpha));
        let f = semibold(17.0 * s);
        let text_x = r.min.x + 28.0 * s;
        let label = match progress {
            Some(f) => format!("{stage} {:.0} %", f * 100.0),
            None => stage,
        };
        let tw = text_width(p, &label, f.clone());
        p.text(pos2(text_x, cy), Align2::LEFT_CENTER, &label, f, veil(k.text, alpha));
        // The bar: filled while downloading, a full faint track for the other stages.
        let track = Rect::from_min_max(pos2(text_x + tw + 18.0 * s, cy - 3.0 * s), pos2(r.max.x, cy + 3.0 * s));
        if track.width() > 40.0 * s {
            p.rect_filled(track, 3.0 * s, veil(k.field_border, alpha));
            let frac = progress.unwrap_or(1.0).clamp(0.0, 1.0);
            let ink = if progress.is_some() { k.accent } else { k.text_faint };
            p.rect_filled(Rect::from_min_max(track.min, pos2(track.min.x + track.width() * frac, track.max.y)), 3.0 * s, veil(ink, alpha));
        }
        let note = t("update.running_note");
        let g = p.layout(note.to_string(), regular(16.5 * s), veil(k.text_dim, alpha), r.max.x - text_x);
        p.with_clip_rect(r).galley(pos2(text_x, cy + 16.0 * s), g, veil(k.text_dim, alpha));
    }
}
