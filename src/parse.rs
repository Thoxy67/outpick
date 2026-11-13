//! What the TV's services answer, as sources: inputs from `eim/getAllInputStatus`, storage
//! and media servers from `attachedstoragemanager/listDevices`, the app in front from
//! `getForegroundAppInfo`. Plain JSON in, plain data out, so it is tested on any machine
//! with payloads captured from real TVs (tests/fixtures).

use crate::model::{Source, SourceKind, icon_from_tv};
use serde_json::{Value, json};

pub fn parse_inputs(v: &Value) -> Vec<Source> {
    let mut out = Vec::new();
    for d in v.get("devices").and_then(Value::as_array).into_iter().flatten() {
        let s = |k: &str| d.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let id = s("id");
        let app_id = s("appId");
        if id.is_empty() || app_id.is_empty() {
            continue;
        }
        let kind = if id.starts_with("HDMI") { SourceKind::Hdmi } else { SourceKind::Analog };
        let connected = d
            .get("hdmiPlugIn")
            .or_else(|| d.get("connected"))
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let label = Some(s("label")).filter(|l| !l.is_empty()).unwrap_or_else(|| id.replace('_', " "));
        let sub = d.get("subList").and_then(Value::as_array).cloned().unwrap_or_default();
        let entry = |kind: &str| sub.iter().find(|x| x.get("id").and_then(Value::as_str) == Some(kind)).cloned();
        let cec = entry("SIMPLINK");
        let urcu = entry("URCU");
        let text = |v: &Option<Value>, k: &str| {
            v.as_ref().and_then(|x| x.get(k)).and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty()).map(str::to_string)
        };
        // Model: the device's own CEC name. Maker: the universal-remote brand, else the CEC vendor id.
        // Without a CEC name, the HDMI info frame's product, unless it is a chip
        // maker's reference board ("Amlogic" / "MBox Meson Ref").
        let chip = |v: &str| {
            let v = v.to_ascii_lowercase();
            ["amlogic", "realtek", "mediatek", "rockchip", "allwinner", "hisilicon", "broadcom", "mstar", "novatek"].iter().any(|c| v.contains(c))
        };
        let spd_vendor = Some(s("spdVendorName")).filter(|v| !v.trim().is_empty() && !chip(v));
        let spd_product = Some(s("spdProductDescription"))
            .filter(|p| !p.trim().is_empty() && !p.contains(" Ref") && !chip(p) && !p.to_ascii_lowercase().contains("meson"));
        let device = text(&cec, "osdName").or_else(|| spd_product.clone());
        let brand = text(&urcu, "brandName")
            .or_else(|| cec.as_ref().and_then(|c| c.get("vendorId")).and_then(Value::as_u64).and_then(crate::model::cec_vendor).map(str::to_string))
            .or(spd_vendor);
        let signal = d.get("hdmiSignalExist").and_then(Value::as_bool);
        let label_custom = d.get("modified").and_then(Value::as_bool).unwrap_or(false);
        // The icon: HDMI-CEC first, then the info frame and LG's settings.
        let hints = crate::model::DeviceHints {
            cec_name: text(&cec, "osdName").unwrap_or_default(),
            cec_vendor: cec.as_ref().and_then(|c| c.get("vendorId")).and_then(Value::as_u64),
            cec_type: cec.as_ref().and_then(|c| c.get("cecpDevType")).and_then(Value::as_u64),
            spd_vendor: s("spdVendorName"),
            spd_product: s("spdProductDescription"),
            spd_type: s("spdSourceDeviceInfo"),
            lg_icon: s("icon"),
            label: label.clone(),
            remote_brand: text(&urcu, "brandName").unwrap_or_default(),
            remote_service: text(&urcu, "serviceType").unwrap_or_default(),
        };
        let tv_icon = crate::model::detect_icon(&hints, kind);
        let cec_power = cec.as_ref().and_then(|c| c.get("cecPower")).and_then(Value::as_str).and_then(|p| match p {
            "On" => Some(true),
            "Off" => Some(false),
            _ => None,
        });
        out.push(Source { tv_icon, id, app_id, label, kind, connected, device, params: None, brand, label_custom, signal, cec: cec.is_some(), cec_power });
    }
    out
}

/// USB storage and DLNA servers, as LG's picker lists them
/// (`InputPicker/Controllers/ConnectionsInterfaces/Interfaces.qml`), opened in
/// the media player with `{"requestDevice": deviceId}`.
pub fn parse_storage(v: &Value) -> Vec<Source> {
    let mut out = Vec::new();
    for d in v.get("devices").and_then(Value::as_array).into_iter().flatten() {
        let s = |k: &str| d.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let id = s("deviceId");
        if id.is_empty() {
            continue;
        }
        let (kind, prefix, device) = match s("deviceType").as_str() {
            "usb" => {
                let port = d.get("portNum").and_then(Value::as_u64).map(|p| if p < 0x10 { p } else { p / 0x10 });
                let ejecting = matches!(s("deviceStatus").as_str(), "EJECTING" | "HIGHPRIORITY_EJECTING");
                let detail = if ejecting { "ejecting".to_string() } else { port.map(|p| p.to_string()).unwrap_or_default() };
                (SourceKind::Usb, "USB", Some(detail))
            }
            "dms" => (SourceKind::MediaServer, "DLNA", None),
            _ => continue,
        };
        let name = Some(s("deviceName")).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| prefix.to_string());
        out.push(Source {
            id: format!("{prefix}:{id}"),
            app_id: "com.webos.app.mediadiscovery".into(),
            label: name,
            kind,
            connected: true,
            device,
            tv_icon: icon_from_tv("", kind),
            params: Some(json!({ "requestDevice": id })),
            brand: None,
            label_custom: false,
            signal: None,
            cec: false,
            cec_power: None,
        });
    }
    out
}

/// The addresses of the Denon and Marantz receivers among the media servers the TV sees
/// (their HEOS media server): where to look for them when SSDP gets no answer.
pub fn receiver_addresses(v: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for d in v.get("devices").and_then(Value::as_array).into_iter().flatten() {
        let s = |k: &str| d.get(k).and_then(Value::as_str).unwrap_or("").to_ascii_lowercase();
        let maker = format!("{} {}", s("manufacturer"), s("deviceName"));
        let ip = d.get("ipAddress").and_then(Value::as_str).unwrap_or("").trim();
        if (maker.contains("denon") || maker.contains("marantz")) && ip.parse::<std::net::Ipv4Addr>().is_ok() && !out.iter().any(|o| o == ip) {
            out.push(ip.to_string());
        }
    }
    out
}

/// The app in front, other than `own` (the picker itself): the card (a full-screen app)
/// when there is one, as overlays such as the screen saver are listed too, else the first.
pub fn foreground_of(v: &Value, own: &str) -> Option<String> {
    let mine = |id: &&str| !id.is_empty() && *id != own;
    v.get("foregroundAppInfo")
        .and_then(Value::as_array)
        .and_then(|l| {
            let card = l
                .iter()
                .filter(|a| a.get("windowType").and_then(Value::as_str) == Some("_WEBOS_WINDOW_TYPE_CARD"))
                .filter_map(|a| a.get("appId").and_then(Value::as_str))
                .find(mine);
            card.or_else(|| l.iter().filter_map(|a| a.get("appId").and_then(Value::as_str)).find(mine))
        })
        .map(str::to_string)
        .or_else(|| v.get("appId").and_then(Value::as_str).filter(|id| *id != own).map(str::to_string))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons::SourceIcon;

    fn fixture(name: &str) -> Value {
        let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap()
    }

    /// (id, label, connected, device, brand, icon) of each input.
    type Row<'a> = (&'a str, &'a str, bool, Option<&'a str>, Option<&'a str>, SourceIcon);

    fn rows(s: &[Source]) -> Vec<Row<'_>> {
        s.iter().map(|s| (s.id.as_str(), s.label.as_str(), s.connected, s.device.as_deref(), s.brand.as_deref(), s.tv_icon)).collect()
    }

    #[test]
    fn c1_inputs() {
        let s = parse_inputs(&fixture("c1-getAllInputStatus.json"));
        assert_eq!(
            rows(&s),
            [
                ("HDMI_1", "Mecool OTT", true, Some("TV Thoxy"), Some("Mecool"), SourceIcon::Android),
                ("HDMI_2", "Denon Sound Bar", true, Some("AVC-S660H"), Some("Denon"), SourceIcon::Speaker),
                ("HDMI_3", "HDMI 3", false, None, None, SourceIcon::Cable),
                ("HDMI_4", "HDMI 4", false, None, None, SourceIcon::Cable),
            ]
        );
        assert!(s.iter().all(|s| s.kind == SourceKind::Hdmi && !s.label_custom && s.signal.is_none()));
        assert_eq!(s[0].app_id, "com.webos.app.hdmi1");
        assert_eq!(s[1].device_name(), "Denon AVC-S660H");
    }

    #[test]
    fn qned_inputs() {
        let s = parse_inputs(&fixture("qned-getAllInputStatus.json"));
        assert_eq!(
            rows(&s),
            [
                ("HDMI_1", "Salon", true, Some("Salon"), None, SourceIcon::Android),
                ("HDMI_2", "HDMI 2", false, None, None, SourceIcon::Cable),
                ("HDMI_3", "Denon Barre de son", true, Some("AVC-S670H"), Some("Denon"), SourceIcon::Speaker),
                ("HDMI_4", "HDMI 4", false, None, None, SourceIcon::Cable),
            ]
        );
        assert!(s.iter().all(|s| s.signal == Some(false)));
        assert_eq!(s[2].device_name(), "Denon AVC-S670H");
    }

    #[test]
    fn inputs_without_id_or_app_are_skipped() {
        let v = json!({"devices": [{"id": "HDMI_1"}, {"appId": "x"}, {"id": "AV_1", "appId": "com.webos.app.externalinput.av1"}]});
        let s = parse_inputs(&v);
        assert_eq!(s.len(), 1);
        assert_eq!((s[0].kind, s[0].label.as_str(), s[0].connected), (SourceKind::Analog, "AV 1", true));
        assert!(parse_inputs(&json!({"returnValue": false})).is_empty());
    }

    #[test]
    fn c1_media_servers() {
        let s = parse_storage(&fixture("c1-listDevices.json"));
        let names: Vec<(&str, &str)> = s.iter().map(|s| (s.id.as_str(), s.label.as_str())).collect();
        assert_eq!(
            names,
            [
                ("DLNA:e3881190-ac55-7bd3-3e9b-c0685d9439fc", "Denon AVC-S660H"),
                ("DLNA:561b5616-6924-fc40-9077-0f72e08f3212", "Denon AVC-S670H"),
                ("DLNA:5238d0e8-aebc-3102-a0ee-cb00731d79ce", "Freebox Server"),
            ]
        );
        // Both receivers, by their media server (the capture's addresses are all 192.0.2.10).
        assert_eq!(receiver_addresses(&fixture("c1-listDevices.json")), ["192.0.2.10"]);
        let v = json!({"devices": [
            {"manufacturer": "Denon", "ipAddress": "192.168.1.70"},
            {"manufacturer": "DENON", "ipAddress": "192.168.1.58"},
            {"deviceName": "Marantz SR6015", "ipAddress": "192.168.1.60"},
            {"manufacturer": "Freebox SAS", "ipAddress": "192.168.1.254"},
            {"manufacturer": "Denon", "ipAddress": "nope"}
        ]});
        assert_eq!(receiver_addresses(&v), ["192.168.1.70", "192.168.1.58", "192.168.1.60"]);
        for m in &s {
            assert_eq!(m.kind, SourceKind::MediaServer);
            assert_eq!(m.app_id, "com.webos.app.mediadiscovery");
            assert_eq!(m.params, Some(json!({ "requestDevice": m.id.trim_start_matches("DLNA:") })));
        }
    }

    #[test]
    fn usb_storage() {
        let v = json!({"devices": [
            {"deviceId": "sda", "deviceType": "usb", "deviceName": "KINGSTON", "portNum": 32},
            {"deviceId": "sdb", "deviceType": "usb", "deviceName": " ", "portNum": 1, "deviceStatus": "EJECTING"},
            {"deviceId": "x", "deviceType": "phone"},
            {"deviceType": "usb"}
        ]});
        let s = parse_storage(&v);
        let got: Vec<(&str, &str, Option<&str>)> = s.iter().map(|s| (s.id.as_str(), s.label.as_str(), s.device.as_deref())).collect();
        assert_eq!(got, [("USB:sda", "KINGSTON", Some("2")), ("USB:sdb", "USB", Some("ejecting"))]);
    }

    #[test]
    fn foreground() {
        // QNED with the screen saver up over the home screen.
        let v = fixture("qned-foreground.json");
        assert_eq!(foreground_of(&v, "com.thoxy.outpick").as_deref(), Some("com.webos.app.home"));
        // The card wins wherever it is listed, and the picker never counts.
        let v = json!({"foregroundAppInfo": [
            {"appId": "com.webos.app.screensaver", "windowType": "_WEBOS_WINDOW_TYPE_SCREENSAVER"},
            {"appId": "com.thoxy.outpick", "windowType": "_WEBOS_WINDOW_TYPE_CARD"},
            {"appId": "com.webos.app.hdmi2", "windowType": "_WEBOS_WINDOW_TYPE_CARD"}
        ]});
        assert_eq!(foreground_of(&v, "com.thoxy.outpick").as_deref(), Some("com.webos.app.hdmi2"));
        // No card: the first that is not ours.
        let v = json!({"foregroundAppInfo": [{"appId": "com.thoxy.outpick"}, {"appId": ""}, {"appId": "com.webos.app.livetv"}]});
        assert_eq!(foreground_of(&v, "com.thoxy.outpick").as_deref(), Some("com.webos.app.livetv"));
        // Older answer shape.
        assert_eq!(foreground_of(&json!({"appId": "com.webos.app.hdmi1"}), "com.thoxy.outpick").as_deref(), Some("com.webos.app.hdmi1"));
        assert_eq!(foreground_of(&json!({"appId": "com.thoxy.outpick"}), "com.thoxy.outpick"), None);
    }
}
