//! The TV runner: an SDL-webOS window with a GLES 2 context (Luma's proven
//! stack, `lg-luma/src/main.rs`), egui painted by egui_glow, remote keys from
//! SDL events. The window is cleared to transparent so the picture can show
//! through where the compositor allows it.

pub mod apps;
pub mod capture;
mod compat;
mod luna;
mod overlay;
mod sdl_events;
pub mod takeover;
mod tv;
pub use tv::preview_probe;
pub mod update;

use crate::app::{App, Nav, Outcome};
use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::Keycode;
use sdl2::video::GLProfile;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const APP_ID: &str = "com.thoxy.outpick";

/// Startup trace (`OUTPICK_TRACE=1`, or the file /tmp/outpick.trace):
/// prints the milliseconds since the process started at each step.
pub(crate) fn trace(step: &str) {
    if let Some(start) = trace_start() {
        eprintln!("trace: {} ms  {step}", now_us().saturating_sub(start) / 1000);
    }
}

/// When the process started, if tracing is on.
fn trace_start() -> Option<u64> {
    static START: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    *START.get_or_init(|| {
        let on = std::env::var_os("OUTPICK_TRACE").is_some_and(|v| v == "1")
            || std::path::Path::new("/tmp/outpick.trace").exists();
        on.then(|| process_start_us().unwrap_or_else(now_us))
    })
}

/// Tracing is on: diagnostics may also go to files in /tmp (the app's stderr is /dev/null).
pub(crate) fn tracing() -> bool {
    trace_start().is_some()
}

/// An OK press: the key and, while the Magic Remote's pointer shows, its mouse button.
struct OkPress {
    since: Instant,
    /// It has acted (or fired its long press): the rest of the press does nothing.
    done: bool,
    /// Each half: None until it goes down, then whether it is still down.
    key: Option<bool>,
    button: Option<bool>,
    /// Where the button went down, in points.
    click: Option<egui::Pos2>,
}

/// CLOCK_BOOTTIME in µs, the clock /proc/<pid>/stat's start time counts in.
fn now_us() -> u64 {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: a valid out pointer.
    unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut ts) };
    ts.tv_sec as u64 * 1_000_000 + ts.tv_nsec as u64 / 1000
}

/// When this process was started (exec), from /proc/self/stat (10 ms resolution).
fn process_start_us() -> Option<u64> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    // Field 22 (starttime, in clock ticks since boot), counted after the ")" ending the name.
    let ticks: u64 = stat.rsplit_once(')')?.1.split_whitespace().nth(19)?.parse().ok()?;
    // SAFETY: sysconf has no preconditions.
    let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as u64;
    Some(ticks * 1_000_000 / hz)
}

fn bytes(t: &str) -> String {
    t.bytes().map(|b| b.to_string()).collect::<Vec<_>>().join(",")
}

/// One line per SDL event in /tmp/outpick-keys.log (key debugging on the TV).
fn log_event(ev: &Event, text_active: bool) {
    let line = match ev {
        Event::KeyDown { keycode, scancode, repeat, .. } => {
            format!("down key={} scan={} repeat={repeat}", keycode.map(i32::from).unwrap_or(-1), scancode.map(|s| s as i32).unwrap_or(-1))
        }
        Event::KeyUp { keycode, scancode, .. } => format!("up key={} scan={}", keycode.map(i32::from).unwrap_or(-1), scancode.map(|s| s as i32).unwrap_or(-1)),
        // Bytes, not {:?}: the TV build has no Debug formatting.
        Event::TextInput { text, .. } => format!("text {}", bytes(text)),
        Event::TextEditing { text, .. } => format!("editing {}", bytes(text)),
        Event::Window { win_event, .. } => format!("window {}", match win_event {
            WindowEvent::FocusGained => "focus-gained",
            WindowEvent::FocusLost => "focus-lost",
            WindowEvent::Shown => "shown",
            WindowEvent::Hidden => "hidden",
            _ => "other",
        }),
        Event::MouseButtonDown { .. } => "click".into(),
        Event::MouseMotion { .. } => return,
        _ => "other".into(),
    };
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/outpick-keys.log") {
        let _ = writeln!(f, "{line} text_active={text_active}");
    }
}

pub fn run() -> Result<(), String> {
    trace("main entry");
    if std::env::var_os("APPID").is_none() {
        // SAFETY: single-threaded at this point; SDL-webOS reads it for the shell surface.
        unsafe { std::env::set_var("APPID", APP_ID) };
    }
    sdl2::hint::set("SDL_WEBOS_ACCESS_POLICY_KEYS_BACK", "true");
    sdl2::hint::set("SDL_WEBOS_ACCESS_POLICY_KEYS_EXIT", "true");
    // Keep the screen saver away while the picker is open.
    sdl2::hint::set("SDL_VIDEO_ALLOW_SCREENSAVER", "0");

    let sdl = sdl2::init()?;
    let video = sdl.video()?;
    {
        let gl = video.gl_attr();
        gl.set_context_profile(GLProfile::GLES);
        gl.set_context_version(2, 0);
        gl.set_red_size(8);
        gl.set_green_size(8);
        gl.set_blue_size(8);
        gl.set_alpha_size(8);
        gl.set_double_buffer(true);
    }
    let mut window = video
        .window("Outpick", 1920, 1080)
        .opengl()
        .fullscreen_desktop()
        .build()
        .map_err(|e| e.to_string())?;
    trace("window created");
    // Before the first buffer: float over the app in front instead of replacing it.
    if let Err(e) = overlay::make_overlay(&window) {
        eprintln!("{e}; showing as a normal window");
    }
    let gl_context = window.gl_create_context()?;
    let _ = video.gl_set_swap_interval(1);
    // SAFETY: the context above is current on this thread for the whole run.
    let gl = Arc::new(unsafe { glow::Context::from_loader_function(|s| video.gl_get_proc_address(s) as *const _) });
    let mut painter = egui_glow::Painter::new(gl.clone(), "", None, false).map_err(|e| e.to_string())?;
    trace("GL ready");

    let ctx = egui::Context::default();
    let wake = sdl.event()?.event_sender();
    let user_event = sdl.event()?;
    let wake_type = unsafe { user_event.register_event() }.map_err(|e| e.to_string())?;
    // Wakes the SDL loop from any thread. egui's repaint callback must only do
    // this: calling back into the context from it deadlocks on egui's lock.
    let wake = {
        let wake = Arc::new(std::sync::Mutex::new(wake));
        move || {
            if let Ok(w) = wake.lock() {
                let _ = w.push_event(Event::User {
                    timestamp: 0,
                    window_id: 0,
                    type_: wake_type,
                    code: 0,
                    data1: std::ptr::null_mut(),
                    data2: std::ptr::null_mut(),
                });
            }
        }
    };
    {
        let w = wake.clone();
        ctx.set_request_repaint_callback(move |_| w());
    }
    let repaint = {
        let ctx = ctx.clone();
        move || ctx.request_repaint()
    };
    let (backend, worker) = tv::start(repaint);
    let shared = backend.state.clone();
    let mut app = App::new(&ctx, backend);
    // Diagnostics only, with tracing on (a file root puts in the app's /tmp): a launch with
    // {"outpickOpen":"apps"} opens the Apps list at once, to try it without a remote.
    if tracing() && std::env::args().nth(1).is_some_and(|a| a.contains("\"outpickOpen\":\"apps\"")) {
        app.open_apps_for_trace();
    }
    let mut traced = (false, false); // first swap, first frame with tiles

    let text_input = video.text_input();
    text_input.stop();
    let mut pump = sdl.event_pump()?;
    let start = Instant::now();
    let mut next_paint = Instant::now();
    // Back held: since when, and whether its long press (close from anywhere) has fired yet.
    let mut held_back: Option<(Instant, bool)> = None;
    // The OK press in progress. Where a long OK means something (`wants_long_ok`), OK is
    // sent on release unless held, and at the threshold the long press fires. Elsewhere it is
    // sent on the press itself, and the rest of the press does nothing. The Magic Remote's OK
    // also comes as a click while its pointer shows, in either order: the same press, so it
    // gives one action, the click if it is there by then.
    let mut ok: Option<OkPress> = None;
    // LG's keyboard is up because we asked for it (to tell when it is closed by the user).
    let mut keyboard_up = false;
    // Key events traced to /tmp/outpick-keys.log while /tmp/outpick.keys exists.
    let key_log = std::path::Path::new("/tmp/outpick.keys").exists();
    'main: loop {
        let mut keys = Vec::new();
        let mut timeout = next_paint.saturating_duration_since(Instant::now()).min(Duration::from_millis(500));
        if ok.as_ref().is_some_and(|p| !p.done) || held_back.is_some_and(|(_, fired)| !fired) {
            timeout = timeout.min(Duration::from_millis(50)); // to fire the long press on time
        }
        let mut first = sdl_events::wait(&mut pump, timeout.as_millis() as u32, true);
        while let Some(ev) = first.take().or_else(|| sdl_events::poll(&mut pump, true)) {
            if key_log {
                log_event(&ev, text_input.is_active());
            }
            let typing = app.wants_text_input();
            match ev {
                Event::Quit { .. } => break 'main,
                Event::Window { win_event: WindowEvent::Close, .. } => break 'main,
                // Its key-up may never come: no long press fires, and the next OK is not lost.
                Event::Window { win_event: WindowEvent::FocusLost, .. } => {
                    ok = None;
                    held_back = None;
                }
                Event::KeyDown { keycode: Some(k), repeat, .. } => {
                    let nav = match k {
                        Keycode::Up => Some(Nav::Up),
                        Keycode::Down => Some(Nav::Down),
                        Keycode::Left => Some(Nav::Left),
                        Keycode::Right => Some(Nav::Right),
                        // The key of a press whose click came first: that press is handled.
                        Keycode::Return | Keycode::KpEnter | Keycode::Select if !repeat && ok.as_ref().is_some_and(|p| p.key.is_none()) => {
                            if let Some(p) = &mut ok {
                                p.key = Some(true);
                            }
                            None
                        }
                        // In a text field, the keyboard's Enter validates at once.
                        Keycode::Return | Keycode::KpEnter if typing => (!repeat && !keys.contains(&Nav::Ok)).then_some(Nav::Ok),
                        Keycode::Return | Keycode::KpEnter | Keycode::Select => {
                            // A key already seen in the press going down again: its release was lost.
                            if !repeat && ok.as_ref().is_none_or(|p| p.key.is_some()) {
                                let at_once = !app.wants_long_ok();
                                ok = Some(OkPress { since: Instant::now(), done: at_once, key: Some(true), button: None, click: None });
                                if at_once {
                                    keys.push(Nav::Ok);
                                }
                            }
                            None
                        }
                        Keycode::Backspace => Some(Nav::Erase),
                        // A further Source press while open (sent by our key filter).
                        Keycode::Tab => Some(Nav::Source),
                        Keycode::AcBack => {
                            if !repeat && held_back.is_none() {
                                held_back = Some((Instant::now(), false));
                            }
                            None
                        }
                        k if i32::from(k) == sdl_events::WEBOS_EXIT_KEYCODE => Some(Nav::Exit),
                        Keycode::Escape => Some(Nav::Exit),
                        _ => None,
                    };
                    keys.extend(nav);
                }
                // Waiting for the release, the first of the key or the button going up sends it;
                // the press lasts until each half that went down is up.
                Event::KeyUp { keycode: Some(Keycode::Return | Keycode::KpEnter | Keycode::Select), .. }
                | Event::MouseButtonUp { mouse_btn: sdl2::mouse::MouseButton::Left, .. } => {
                    if let Some(p) = &mut ok {
                        if matches!(ev, Event::KeyUp { .. }) {
                            p.key = p.key.map(|_| false);
                        } else {
                            p.button = p.button.map(|_| false);
                        }
                        if !p.done {
                            p.done = true;
                            keys.push(p.click.map_or(Nav::Ok, Nav::Click));
                        }
                        if p.key != Some(true) && p.button != Some(true) {
                            ok = None;
                        }
                    }
                }
                Event::KeyUp { keycode: Some(Keycode::AcBack), .. } => {
                    // Back is sent on release, unless it was held (its long press fired below).
                    if let Some((_, fired)) = held_back.take()
                        && !fired
                    {
                        keys.push(Nav::Back);
                    }
                }
                // Enter may also come as text from the keyboard.
                // (Once: it may come both as this key and as text in the same batch.)
                Event::TextInput { text, .. } if typing && text.contains(['\n', '\r']) => {
                    if !keys.contains(&Nav::Ok) {
                        keys.push(Nav::Ok);
                    }
                }
                Event::TextInput { text, .. } if typing => keys.push(Nav::Text(text)),
                // The Magic Remote's pointer, in window pixels; egui works in points.
                Event::MouseMotion { x, y, .. } => {
                    let ppp = (window.drawable_size().1 as f32 / 1080.0).max(0.25);
                    keys.push(Nav::Pointer(egui::pos2(x as f32, y as f32) / ppp));
                }
                Event::MouseButtonDown { mouse_btn: sdl2::mouse::MouseButton::Left, x, y, .. } => {
                    let ppp = (window.drawable_size().1 as f32 / 1080.0).max(0.25);
                    let pos = egui::pos2(x as f32, y as f32) / ppp;
                    match &mut ok {
                        // The click of a press whose key came first.
                        Some(p) if p.button.is_none() => {
                            p.button = Some(true);
                            p.click = Some(pos);
                        }
                        // A new press (or the button's release was lost).
                        _ => {
                            let at_once = !app.wants_long_ok();
                            ok = Some(OkPress { since: Instant::now(), done: at_once, key: None, button: Some(true), click: Some(pos) });
                            if at_once {
                                keys.push(Nav::Click(pos));
                            }
                        }
                    }
                }
                Event::MouseWheel { y, .. } if y != 0 => keys.push(Nav::Scroll(y as f32)),
                _ => {}
            }
            next_paint = Instant::now();
        }
        // A long OK opens the selected source's options, while still held.
        let long = app.long_press();
        if let Some(p) = &mut ok
            && !p.done
            && p.since.elapsed() >= long
        {
            p.done = true;
            keys.push(Nav::Menu);
        }

        // A long Back closes from any screen, while still held.
        if let Some((down, fired)) = &mut held_back
            && !*fired
            && down.elapsed() >= long
        {
            *fired = true;
            keys.push(Nav::Exit);
        }

        // LG's on-screen keyboard follows SDL text input (SDL-webOS's OSK support). Closed by
        // the user (its Back), it is not raised again: that is Back for the field.
        let typing = app.wants_text_input();
        if typing && keyboard_up && !text_input.is_active() {
            keyboard_up = false;
            // Not when its Enter closed it: that validated the field already.
            if !keys.contains(&Nav::Ok) {
                keys.push(Nav::Back);
            }
        } else if typing && !text_input.is_active() {
            // The press that opened the field must not become a long press, a click or an OK
            // in it: the rest of it (a late click, its release) still belongs to it.
            if let Some(p) = &mut ok {
                p.done = true;
            }
            text_input.start();
            keyboard_up = true;
        } else if !typing {
            if text_input.is_active() {
                text_input.stop();
            }
            keyboard_up = false;
        }
        if Instant::now() < next_paint && keys.is_empty() {
            continue;
        }
        let (w, h) = window.drawable_size();
        let ppp = (h as f32 / 1080.0).max(0.25);
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w as f32, h as f32) / ppp)),
            time: Some(start.elapsed().as_secs_f64()),
            focused: true,
            ..Default::default()
        };
        ctx.set_pixels_per_point(ppp);
        let tiles = !traced.1 && shared.lock().is_ok_and(|st| st.loaded && !st.sources.is_empty());
        let mut outcome = Outcome::Continue;
        let out = ctx.run_ui(raw, |ui| outcome = app.ui(ui, &keys));
        let prims = ctx.tessellate(out.shapes, out.pixels_per_point);
        let mut textures = out.textures_delta;
        // SAFETY: plain GL state calls on the current context.
        unsafe {
            use glow::HasContext;
            gl.viewport(0, 0, w as i32, h as i32);
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
        }
        painter.paint_and_update_textures([w, h], out.pixels_per_point, &prims, &mut textures);
        window.gl_swap_window();
        if !traced.0 {
            traced.0 = true;
            trace("first swap");
        }
        if tiles {
            traced.1 = true;
            trace("first frame with tiles");
        }
        if outcome == Outcome::Close {
            break;
        }
        let delay = out
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map(|v| v.repaint_delay)
            .unwrap_or(Duration::MAX);
        next_paint = Instant::now().checked_add(delay).unwrap_or_else(|| Instant::now() + Duration::from_secs(3600));
    }
    // Off the screen at once, before saving and waiting for the worker (a CEC key waits
    // for its input to come up): with the window gone the remote goes back to the app in front.
    window.hide();
    painter.destroy();
    drop(painter);
    drop(gl);
    drop(gl_context);
    text_input.stop();
    drop(window);
    app.shutdown(); // every way out (closed, Quit, window closed) saves the settings
    tv::flush_preview();
    drop(app); // drops the command sender: the worker finishes a pending switch, then ends
    let _ = worker.join();
    Ok(())
}
