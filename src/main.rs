//! outpick: a modern, customizable native input picker for rooted LG webOS TVs.
//!
//! `--features webos` builds the TV app (SDL-webOS + GLES2 + luna-service2);
//! the default build is a desktop preview with mock inputs for designing the UI.
//! With neither (`--no-default-features`) it is only the host test build.
#![cfg_attr(not(any(feature = "desktop", feature = "webos")), allow(dead_code))]

mod app;
mod clock;
mod config;
mod denon;
mod i18n;
mod icons;
mod model;
// The TV's JSON answers: used by the TV build, tested on any.
#[cfg_attr(not(feature = "webos"), allow(dead_code))]
mod parse;
// The root helpers' pure logic: used by the TV build, tested on any.
#[cfg_attr(not(feature = "webos"), allow(dead_code))]
mod rootside;
mod theme;

#[cfg(feature = "webos")]
mod webos;

#[cfg(all(feature = "desktop", not(feature = "webos")))]
mod desktop;

fn main() {
    #[cfg(feature = "webos")]
    {
        let args: Vec<String> = std::env::args().collect();
        if let Some(i) = args.iter().position(|a| a == "--fix-permissions") {
            // The app passes its uid, to be handed its settings file.
            let uid = args.get(i + 1).and_then(|u| u.parse::<u32>().ok());
            std::process::exit(match webos::takeover::open_dir_for(uid) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("{e}");
                    1
                }
            });
        }
        if let Some(i) = args.iter().position(|a| a == "--capture-access") {
            std::process::exit(webos::capture::cli(args.get(i + 1).map(String::as_str).unwrap_or("status")));
        }
        if args.iter().any(|a| a == "--avr-scan") {
            // Diagnostics over ssh: what the receiver search finds from here.
            denon::scan_report();
            return;
        }
        if args.iter().any(|a| a == "--boot") {
            std::process::exit(webos::takeover::boot());
        }
        if args.iter().any(|a| a == "--uninstall") {
            std::process::exit(webos::takeover::uninstall());
        }
        if let Some(i) = args.iter().position(|a| a == "--takeover") {
            std::process::exit(webos::takeover::cli(args.get(i + 1).map(String::as_str).unwrap_or("status")));
        }
        // What webos::run sets otherwise, set here while still single-threaded.
        if std::env::var_os("APPID").is_none() {
            // SAFETY: no other thread exists yet.
            unsafe { std::env::set_var("APPID", webos::APP_ID) };
        }
        // Off the UI thread: a one-time move of an earlier build's key filter.
        std::thread::spawn(webos::takeover::migrate);
        if let Err(e) = webos::run() {
            eprintln!("outpick: {e}");
            std::process::exit(1);
        }
    }
    #[cfg(all(feature = "desktop", not(feature = "webos")))]
    desktop::run();
}
