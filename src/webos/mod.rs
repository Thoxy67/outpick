//! The TV runner: an SDL-webOS window with a GLES 2 context (Luma's proven
//! stack, `lg-luma/src/main.rs`), egui painted by egui_glow, remote keys from
//! SDL events. The window is cleared to transparent so the picture can show
//! through where the compositor allows it.

pub mod capture;
mod compat;
mod luna;
mod overlay;
mod sdl_events;
pub mod takeover;
mod tv;

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
    static START: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    let start = START.get_or_init(|| {
        let on = std::env::var_os("OUTPICK_TRACE").is_some_and(|v| v == "1")
            || std::path::Path::new("/tmp/outpick.trace").exists();
        on.then(|| process_start_us().unwrap_or_else(now_us))
    });
    if let Some(start) = start {
        eprintln!("trace: {} ms  {step}", now_us().saturating_sub(*start) / 1000);
    }
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
    let mut traced = (false, false); // first swap, first frame with tiles

    let text_input = video.text_input();
    text_input.stop();
    let mut pump = sdl.event_pump()?;
    let start = Instant::now();
    let mut next_paint = Instant::now();
    let mut held_back: Option<Instant> = None;
    'main: loop {
        let mut keys = Vec::new();
        let timeout = next_paint.saturating_duration_since(Instant::now()).min(Duration::from_millis(500));
        let mut first = sdl_events::wait(&mut pump, timeout.as_millis() as u32, true);
        while let Some(ev) = first.take().or_else(|| sdl_events::poll(&mut pump, true)) {
            match ev {
                Event::Quit { .. } => break 'main,
                Event::Window { win_event: WindowEvent::Close, .. } => break 'main,
                Event::KeyDown { keycode: Some(k), repeat, .. } => {
                    let nav = match k {
                        Keycode::Up => Some(Nav::Up),
                        Keycode::Down => Some(Nav::Down),
                        Keycode::Left => Some(Nav::Left),
                        Keycode::Right => Some(Nav::Right),
                        Keycode::Return | Keycode::KpEnter | Keycode::Select => (!repeat).then_some(Nav::Ok),
                        Keycode::Backspace => Some(Nav::Erase),
                        // A further Source press while open (sent by our key filter).
                        Keycode::Tab => Some(Nav::Source),
                        Keycode::AcBack => {
                            held_back.get_or_insert_with(Instant::now);
                            None
                        }
                        k if i32::from(k) == sdl_events::WEBOS_EXIT_KEYCODE => Some(Nav::Exit),
                        Keycode::Escape => Some(Nav::Exit),
                        _ => None,
                    };
                    keys.extend(nav);
                }
                Event::KeyUp { keycode: Some(Keycode::AcBack), .. } => {
                    // A long Back (held 600 ms or more) closes from any screen.
                    if let Some(down) = held_back.take() {
                        keys.push(if down.elapsed() >= Duration::from_millis(600) { Nav::Exit } else { Nav::Back });
                    }
                }
                Event::TextInput { text, .. } if app.wants_text_input() => keys.push(Nav::Text(text)),
                // The Magic Remote's pointer, in window pixels; egui works in points.
                Event::MouseMotion { x, y, .. } => {
                    let ppp = (window.drawable_size().1 as f32 / 1080.0).max(0.25);
                    keys.push(Nav::Pointer(egui::pos2(x as f32, y as f32) / ppp));
                }
                Event::MouseButtonDown { mouse_btn: sdl2::mouse::MouseButton::Left, x, y, .. } => {
                    let ppp = (window.drawable_size().1 as f32 / 1080.0).max(0.25);
                    keys.push(Nav::Click(egui::pos2(x as f32, y as f32) / ppp));
                }
                Event::MouseWheel { y, .. } if y != 0 => keys.push(Nav::Scroll(y as f32)),
                _ => {}
            }
            next_paint = Instant::now();
        }

        // LG's on-screen keyboard follows SDL text input (SDL-webOS's OSK support).
        let typing = app.wants_text_input();
        if typing != text_input.is_active() {
            if typing { text_input.start() } else { text_input.stop() }
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
