//! The picker's entries: every source with the config applied.

use super::App;
use crate::config::Naming;
use crate::i18n::{t, tf};
use crate::icons::SourceIcon;
use crate::model::{Source, SourceKind, TvState};

/// One tile/row of the picker.
#[derive(Clone)]
pub(super) struct Entry {
    pub(super) source: Source,
    pub(super) name: String,
    pub(super) sub: Option<String>,
    pub(super) icon: SourceIcon,
    pub(super) current: bool,
    pub(super) hidden: bool,
    /// The network receiver is behind this input: its inputs are in the options.
    pub(super) avr: bool,
}

impl App {
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

    pub(super) fn tv_name(&self, s: &Source) -> String {
        Self::tv_name_with(s, self.cfg.naming)
    }

    /// All sources with the config applied, in display order (hidden ones included).
    pub(super) fn all_entries(&self, st: &TvState) -> Vec<Entry> {
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
                } else if let Some(input) = st.avr.active_input().filter(|_| st.avr.serves(&s.id)) {
                    // The receiver's input being shown through it: "Game · HDMI 3".
                    Some(format!("{} · {}", input.name, s.port_name()))
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
                let avr = st.avr.serves(&s.id);
                Entry { hidden: ov.is_some_and(|o| o.hidden), name, sub, icon, current, avr, source: s }
            })
            .collect()
    }

    pub(super) fn visible_entries(&self, st: &TvState) -> Vec<Entry> {
        self.all_entries(st)
            .into_iter()
            .filter(|e| !e.hidden && (self.cfg.show_disconnected || e.source.connected || e.current))
            // Home only when away from it (on an input or in another app).
            .filter(|e| !(e.source.kind == SourceKind::Home && e.current))
            .collect()
    }
}
