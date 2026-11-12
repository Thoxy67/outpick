//! lg-c1-source: a modern, customizable native input picker for rooted LG webOS TVs.
//!
//! `--features webos` builds the TV app (SDL-webOS + GLES2 + luna-service2);
//! the default build is a desktop preview with mock inputs for designing the UI.

mod app;
mod config;
mod i18n;
mod icons;
mod model;
mod theme;

#[cfg(feature = "webos")]
mod webos;

#[cfg(all(feature = "desktop", not(feature = "webos")))]
mod desktop;

fn main() {
    #[cfg(feature = "webos")]
    {
        let args: Vec<String> = std::env::args().collect();
        if args.iter().any(|a| a == "--fix-permissions") {
            std::process::exit(match webos::takeover::open_dir() {
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
        if let Some(i) = args.iter().position(|a| a == "--takeover") {
            std::process::exit(webos::takeover::cli(args.get(i + 1).map(String::as_str).unwrap_or("status")));
        }
        if let Err(e) = webos::run() {
            eprintln!("source-picker: {e}");
            std::process::exit(1);
        }
    }
    #[cfg(all(feature = "desktop", not(feature = "webos")))]
    desktop::run();
}
