//! An on-screen clock, placed anywhere and sized freely, in five styles.
//! Local time comes from libc's `localtime_r` (the TV's /etc/localtime points
//! at its time-zone setting).

use crate::config::{ClockConfig, ClockStyle};
use crate::i18n::t;
use crate::theme::{Tokens, medium, mix, regular, semibold, veil};
use egui::{Align2, Color32, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2, pos2, vec2};

pub struct Now {
    pub hour: u32,
    pub min: u32,
    pub sec: u32,
    pub weekday: usize,
    pub day: u32,
    pub month: usize,
}

pub fn now() -> Now {
    // SAFETY: time and localtime_r only write into the tm we own.
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        Now {
            hour: tm.tm_hour as u32,
            min: tm.tm_min as u32,
            sec: tm.tm_sec as u32,
            weekday: tm.tm_wday.clamp(0, 6) as usize,
            day: tm.tm_mday as u32,
            month: tm.tm_mon.clamp(0, 11) as usize,
        }
    }
}

fn time_text(n: &Now, c: &ClockConfig) -> (String, Option<&'static str>) {
    let (h, ampm) = if c.hour24 {
        (n.hour, None)
    } else {
        let h = n.hour % 12;
        (if h == 0 { 12 } else { h }, Some(if n.hour < 12 { "AM" } else { "PM" }))
    };
    let mut s = if c.hour24 { format!("{h:02}:{:02}", n.min) } else { format!("{h}:{:02}", n.min) };
    if c.seconds {
        s.push_str(&format!(":{:02}", n.sec));
    }
    (s, ampm)
}

fn date_text(n: &Now) -> String {
    const DAYS: [&str; 7] = ["day.0", "day.1", "day.2", "day.3", "day.4", "day.5", "day.6"];
    const MONTHS: [&str; 12] = [
        "month.0", "month.1", "month.2", "month.3", "month.4", "month.5", "month.6", "month.7", "month.8", "month.9", "month.10", "month.11",
    ];
    crate::i18n::tf("clock.date", &[("day", t(DAYS[n.weekday])), ("n", &n.day.to_string()), ("month", t(MONTHS[n.month]))])
}

/// Text with a soft shadow, so it reads over any picture.
fn shadowed(p: &Painter, at: Pos2, anchor: Align2, text: &str, font: egui::FontId, color: Color32, alpha: f32) -> Rect {
    let off = vec2(0.0, font.size * 0.04 + 1.0);
    p.text(at + off, anchor, text, font.clone(), Color32::from_black_alpha((150.0 * alpha) as u8));
    p.text(at, anchor, text, font, veil(color, alpha))
}

/// Lit segments (a b c d e f g) of each digit.
const SEGMENTS: [[bool; 7]; 10] = [
    [true, true, true, true, true, true, false],
    [false, true, true, false, false, false, false],
    [true, true, false, true, true, false, true],
    [true, true, true, true, false, false, true],
    [false, true, true, false, false, true, true],
    [true, false, true, true, false, true, true],
    [true, false, true, true, true, true, true],
    [true, true, true, false, false, false, false],
    [true, true, true, true, true, true, true],
    [true, true, true, true, false, true, true],
];

/// One seven-segment digit in `r`: lit segments in `on`, the rest faint.
fn seven_segment(p: &Painter, r: Rect, digit: u32, on: Color32, off: Color32) {
    let (w, h) = (r.width(), r.height());
    let t = w * 0.18; // segment thickness
    let lit = SEGMENTS[(digit % 10) as usize];
    let horiz = |y: f32| Rect::from_min_size(pos2(r.min.x + t * 0.6, y - t / 2.0), vec2(w - t * 1.2, t));
    let vert = |x: f32, y0: f32, y1: f32| Rect::from_min_max(pos2(x - t / 2.0, y0 + t * 0.6), pos2(x + t / 2.0, y1 - t * 0.6));
    let (top, mid, bot) = (r.min.y + t / 2.0, r.center().y, r.max.y - t / 2.0);
    let (l, rr) = (r.min.x + t / 2.0, r.max.x - t / 2.0);
    let segs = [horiz(top), vert(rr, top, mid), vert(rr, mid, bot), horiz(bot), vert(l, mid, bot), vert(l, top, mid), horiz(mid)];
    for (i, seg) in segs.iter().enumerate() {
        p.rect_filled(*seg, t / 2.0, if lit[i] { on } else { off });
    }
    let _ = h;
}

/// Draws the clock centred on its configured place; returns the area it covers.
pub fn draw(p: &Painter, screen: Rect, c: &ClockConfig, k: &Tokens, alpha: f32) -> Rect {
    let n = now();
    let z = c.size;
    let center = pos2(screen.min.x + screen.width() * c.x, screen.min.y + screen.height() * c.y);
    let (time, ampm) = time_text(&n, c);
    let big = semibold(64.0 * z);
    let width = |s: &str, f: egui::FontId| p.layout_no_wrap(s.to_string(), f, Color32::WHITE).size().x;
    match c.style {
        ClockStyle::Minimal => {
            let mut r = shadowed(p, center, Align2::CENTER_CENTER, &time, big, Color32::WHITE, alpha);
            if let Some(m) = ampm {
                let mr = shadowed(p, pos2(r.max.x + 8.0 * z, r.max.y - 10.0 * z), Align2::LEFT_BOTTOM, m, medium(22.0 * z), Color32::WHITE, alpha);
                r = r.union(mr);
            }
            r
        }
        ClockStyle::Pill => {
            let f = semibold(44.0 * z);
            let label = match ampm {
                Some(m) => format!("{time} {m}"),
                None => time,
            };
            let w = width(&label, f.clone()) + 48.0 * z;
            let r = Rect::from_center_size(center, vec2(w, 76.0 * z));
            p.rect(r, r.height() / 2.0, veil(k.card, 0.88 * alpha), Stroke::new(1.0, veil(k.card_border, alpha)), StrokeKind::Inside);
            p.text(r.center(), Align2::CENTER_CENTER, label, f, veil(k.text, alpha));
            r
        }
        ClockStyle::Stacked => {
            let tr = shadowed(p, center - vec2(0.0, 14.0 * z), Align2::CENTER_CENTER, &time, semibold(72.0 * z), Color32::WHITE, alpha);
            let mut r = tr;
            if let Some(m) = ampm {
                r = r.union(shadowed(p, pos2(tr.max.x + 8.0 * z, tr.max.y - 12.0 * z), Align2::LEFT_BOTTOM, m, medium(22.0 * z), Color32::WHITE, alpha));
            }
            let dr = shadowed(p, pos2(center.x, tr.max.y + 4.0 * z), Align2::CENTER_TOP, &date_text(&n), regular(24.0 * z), mix(Color32::WHITE, k.accent_text, 0.35), alpha);
            r.union(dr)
        }
        ClockStyle::Analog => {
            let radius = 70.0 * z;
            p.circle(center, radius, veil(k.card, 0.88 * alpha), Stroke::new(2.0 * z, veil(k.card_border, alpha)));
            for i in 0..12 {
                let a = i as f32 / 12.0 * std::f32::consts::TAU;
                let dir = Vec2::angled(a - std::f32::consts::FRAC_PI_2);
                let len = if i % 3 == 0 { 12.0 } else { 6.0 } * z;
                p.line_segment([center + dir * (radius - 6.0 * z - len), center + dir * (radius - 6.0 * z)], Stroke::new(2.0 * z, veil(k.text_dim, alpha)));
            }
            let hand = |frac: f32, len: f32, width: f32, color: Color32| {
                let dir = Vec2::angled(frac * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2);
                p.line_segment([center - dir * (len * 0.15), center + dir * len], Stroke::new(width, veil(color, alpha)));
            };
            let (h, m, s) = (n.hour % 12, n.min, n.sec);
            hand((h as f32 + m as f32 / 60.0) / 12.0, radius * 0.5, 5.0 * z, k.text);
            hand((m as f32 + s as f32 / 60.0) / 60.0, radius * 0.75, 3.5 * z, k.text);
            if c.seconds {
                hand(s as f32 / 60.0, radius * 0.82, 1.6 * z, k.accent);
            }
            p.circle_filled(center, 4.0 * z, veil(k.accent, alpha));
            Rect::from_center_size(center, Vec2::splat(radius * 2.0))
        }
        ClockStyle::Tiles => {
            // Each digit on its own tile, as a flip clock.
            let (tw, th, gap, colon) = (46.0 * z, 70.0 * z, 6.0 * z, 18.0 * z);
            let groups: Vec<String> = time.split(':').map(str::to_string).collect();
            let digits: usize = groups.iter().map(|g| g.chars().count()).sum();
            let total = digits as f32 * tw + (digits as f32 - groups.len() as f32) * gap + (groups.len() as f32 - 1.0) * colon
                + if ampm.is_some() { 56.0 * z } else { 0.0 };
            let mut x = center.x - total / 2.0;
            let top = center.y - th / 2.0;
            let f = semibold(46.0 * z);
            for (gi, g) in groups.iter().enumerate() {
                if gi > 0 {
                    let cx = x + colon / 2.0;
                    for dy in [-12.0, 12.0] {
                        p.circle_filled(pos2(cx, center.y + dy * z), 3.5 * z, veil(Color32::WHITE, alpha));
                    }
                    x += colon;
                }
                for (ci, ch) in g.chars().enumerate() {
                    if ci > 0 {
                        x += gap;
                    }
                    let r = Rect::from_min_size(pos2(x, top), vec2(tw, th));
                    p.rect(r, 8.0 * z, veil(k.card, 0.9 * alpha), Stroke::new(1.0, veil(k.card_border, alpha)), StrokeKind::Inside);
                    p.line_segment([pos2(r.min.x, r.center().y), pos2(r.max.x, r.center().y)], Stroke::new(1.5 * z, veil(Color32::BLACK, 0.5 * alpha)));
                    p.text(r.center(), Align2::CENTER_CENTER, ch, f.clone(), veil(k.text, alpha));
                    x += tw;
                }
            }
            if let Some(m) = ampm {
                p.text(pos2(x + 10.0 * z, center.y), Align2::LEFT_CENTER, m, semibold(22.0 * z), veil(k.accent_text, alpha));
            }
            Rect::from_center_size(center, vec2(total, th))
        }
        ClockStyle::SevenSegment => {
            // LED digits in the accent colour on a dark plate, unlit segments faint.
            let (dw, dh, gap, colon) = (40.0 * z, 72.0 * z, 10.0 * z, 20.0 * z);
            let groups: Vec<&str> = time.split(':').collect();
            let digits: usize = groups.iter().map(|g| g.len()).sum();
            let inner = digits as f32 * dw + (digits as f32 - groups.len() as f32) * gap + (groups.len() as f32 - 1.0) * colon;
            let extra = if ampm.is_some() { 52.0 * z } else { 0.0 };
            let plate = Rect::from_center_size(center, vec2(inner + extra + 40.0 * z, dh + 32.0 * z));
            p.rect(plate, 12.0 * z, veil(Color32::from_rgb(8, 8, 10), 0.9 * alpha), Stroke::new(1.0, veil(k.card_border, alpha)), StrokeKind::Inside);
            let on = veil(k.accent, alpha);
            let off = veil(k.accent, 0.08 * alpha);
            let mut x = plate.min.x + 20.0 * z;
            for (gi, g) in groups.iter().enumerate() {
                if gi > 0 {
                    for dy in [-14.0, 14.0] {
                        p.rect_filled(Rect::from_center_size(pos2(x + colon / 2.0, center.y + dy * z), Vec2::splat(6.0 * z)), 1.5 * z, on);
                    }
                    x += colon;
                }
                for (ci, ch) in g.chars().enumerate() {
                    if ci > 0 {
                        x += gap;
                    }
                    let d = ch.to_digit(10).unwrap_or(8);
                    seven_segment(p, Rect::from_min_size(pos2(x, center.y - dh / 2.0), vec2(dw, dh)), d, on, off);
                    x += dw;
                }
            }
            if let Some(m) = ampm {
                p.text(pos2(x + 12.0 * z, center.y + dh / 2.0), Align2::LEFT_BOTTOM, m, semibold(20.0 * z), on);
            }
            plate
        }
        ClockStyle::OnePlus => {
            // Two tight lines of heavy digits: hours in the accent colour, minutes in white.
            let f = semibold(92.0 * z);
            let (hh, mm) = time.split_once(':').unwrap_or((time.as_str(), ""));
            let mm = mm.split(':').next().unwrap_or("");
            let line = 80.0 * z;
            let hr = shadowed(p, pos2(center.x, center.y - line / 2.0), Align2::CENTER_CENTER, hh, f.clone(), k.accent, alpha);
            let mr = shadowed(p, pos2(center.x, center.y + line / 2.0), Align2::CENTER_CENTER, mm, f, Color32::WHITE, alpha);
            let mut r = hr.union(mr);
            if c.seconds {
                let sr = shadowed(p, pos2(mr.max.x + 6.0 * z, mr.max.y - 14.0 * z), Align2::LEFT_BOTTOM, &format!("{:02}", n.sec), medium(26.0 * z), k.accent_text, alpha);
                r = r.union(sr);
            }
            if let Some(m) = ampm {
                r = r.union(shadowed(p, pos2(hr.max.x + 6.0 * z, hr.min.y + 16.0 * z), Align2::LEFT_TOP, m, medium(22.0 * z), Color32::WHITE, alpha));
            }
            r
        }
    }
}
