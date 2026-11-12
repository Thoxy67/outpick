//! Desktop preview (`cargo run`): the overlay over a stand-in "TV picture",
//! with mock inputs, laid out at TV scale (1080 points tall).
//!
//! Keys: arrows, Enter = OK, Escape = Back, Backspace = erase/back, Tab = the Source key,
//! F5 reopens the picker after it closed.
//!
//! Scripted screenshots for checking the UI without a TV:
//!   cargo run -- --screenshot shot.png [--keys right,right,ok] [--size 1920x1080] [--config cfg.json]
//! presses the keys one per frame once the inputs are loaded, waits for the
//! animations to settle, saves the window as PNG and exits.

use crate::app::{App, Nav, Outcome};
use crate::model::mock_backend;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

struct Script {
    path: String,
    keys: Vec<Nav>,
    frame: u32,
    requested: bool,
    done: Arc<AtomicBool>,
    started: Option<f64>,
}

struct Preview {
    app: App,
    closed: bool,
    script: Option<Script>,
}

fn parse_key(s: &str) -> Option<Nav> {
    if let Some(text) = s.strip_prefix("text:") {
        return Some(Nav::Text(text.to_string()));
    }
    // move:x;y and click:x;y in screen points (1920×1080).
    let point = |v: &str| {
        let (x, y) = v.split_once(';')?;
        Some(egui::pos2(x.parse().ok()?, y.parse().ok()?))
    };
    if let Some(v) = s.strip_prefix("move:") {
        return point(v).map(Nav::Pointer);
    }
    if let Some(v) = s.strip_prefix("click:") {
        return point(v).map(Nav::Click);
    }
    Some(match s.trim().to_ascii_lowercase().as_str() {
        "up" => Nav::Up,
        "down" => Nav::Down,
        "left" => Nav::Left,
        "right" => Nav::Right,
        "ok" | "enter" => Nav::Ok,
        "back" => Nav::Back,
        "source" | "input" => Nav::Source,
        "erase" => Nav::Erase,
        _ => return None,
    })
}

pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    if let Some(cfg) = arg("--config") {
        // SAFETY: before any thread is started.
        unsafe { std::env::set_var("LG_SOURCE_CONFIG", cfg) };
    }
    let size = arg("--size")
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some([w.parse().ok()?, h.parse().ok()?])
        })
        .unwrap_or([1280.0f32, 720.0]);
    let script = arg("--screenshot").map(|path| Script {
        path,
        keys: arg("--keys").map(|k| k.split(',').filter_map(parse_key).collect()).unwrap_or_default(),
        frame: 0,
        requested: false,
        done: Arc::new(AtomicBool::new(false)),
        started: None,
    });
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(size)
            .with_resizable(script.is_none())
            .with_title("LG source picker — preview"),
        ..Default::default()
    };
    let _ = eframe::run_native(
        "lg-c1-source",
        options,
        Box::new(move |cc| {
            let ctx = cc.egui_ctx.clone();
            let backend = mock_backend(move || ctx.request_repaint());
            let app = App::new(&cc.egui_ctx, backend);
            Ok(Box::new(Preview { app, closed: false, script }))
        }),
    );
}

impl eframe::App for Preview {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // Lay the UI out at TV scale: 1080 points tall, whatever the window.
        let px_h = ctx.content_rect().height() * ctx.zoom_factor();
        ctx.set_zoom_factor((px_h / 1080.0).max(0.3));
        let screen = ui.max_rect();
        fake_picture(ui, screen);

        let typing = self.app.wants_text_input();
        let events = ui.input(|i| i.events.clone());
        let mut keys: Vec<Nav> = Vec::new();
        for e in events {
            match e {
                egui::Event::Text(s) if typing => keys.push(Nav::Text(s)),
                egui::Event::PointerMoved(pos) => keys.push(Nav::Pointer(pos)),
                egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, .. } => keys.push(Nav::Click(pos)),
                egui::Event::MouseWheel { delta, .. } if delta.y != 0.0 => keys.push(Nav::Scroll(delta.y)),
                egui::Event::Key { key, pressed: true, .. } => keys.extend(match key {
                    egui::Key::ArrowUp => Some(Nav::Up),
                    egui::Key::ArrowDown => Some(Nav::Down),
                    egui::Key::ArrowLeft => Some(Nav::Left),
                    egui::Key::ArrowRight => Some(Nav::Right),
                    egui::Key::Enter => Some(Nav::Ok),
                    egui::Key::Space if !typing => Some(Nav::Ok),
                    egui::Key::Escape => Some(Nav::Back),
                    egui::Key::Backspace => Some(Nav::Erase),
                    egui::Key::Tab => Some(Nav::Source),
                    egui::Key::F5 => {
                        self.closed = false;
                        self.app.reopen();
                        None
                    }
                    _ => None,
                }),
                _ => {}
            }
        }

        // A scripted capture ignores the real mouse (moving it over the window
        // would hover or close the picker); the script's own pointer events stay.
        if self.script.is_some() {
            keys.retain(|k| !matches!(k, Nav::Pointer(_) | Nav::Click(_) | Nav::Scroll(_)));
        }
        if !keys.is_empty() && std::env::var_os("LG_DEBUG").is_some() {
            eprintln!("keys: {keys:?}");
        }
        // Scripted run: feed keys after a few frames, then capture once settled.
        if let Some(s) = &mut self.script {
            s.frame += 1;
            let now = ui.input(|i| i.time);
            if s.frame > 5 {
                if let Some(k) = (!s.keys.is_empty()).then(|| s.keys.remove(0)) {
                    keys.push(k);
                    s.started = None;
                } else {
                    let since = *s.started.get_or_insert(now);
                    if now - since > 0.8 && !s.requested {
                        s.requested = true;
                        let path = s.path.clone();
                        let done = s.done.clone();
                        let c2 = ctx.clone();
                        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |img| {
                            match save_png(&path, &img) {
                                Ok(()) => println!("saved {path} ({}x{})", img.size[0], img.size[1]),
                                Err(e) => eprintln!("screenshot: {e}"),
                            }
                            done.store(true, Ordering::SeqCst);
                            c2.request_repaint();
                        })));
                    }
                }
            }
            if s.done.load(Ordering::SeqCst) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ctx.request_repaint();
        }

        if self.closed {
            ui.painter().text(screen.center(), egui::Align2::CENTER_CENTER, "Closed — press F5 to open the picker again", egui::FontId::proportional(28.0), egui::Color32::WHITE);
            return;
        }
        if self.app.ui(ui, &keys) == Outcome::Close {
            self.closed = true;
        }
    }
}

fn save_png(path: &str, img: &egui::ColorImage) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), img.size[0] as u32, img.size[1] as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().map_err(|e| e.to_string())?;
    let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
    w.write_image_data(&bytes).map_err(|e| e.to_string())
}

/// Something picture-like behind the overlay, to judge transparency.
fn fake_picture(ui: &egui::Ui, r: egui::Rect) {
    let p = ui.painter();
    let mut mesh = egui::Mesh::default();
    let c = [
        egui::Color32::from_rgb(40, 90, 140),
        egui::Color32::from_rgb(200, 120, 70),
        egui::Color32::from_rgb(30, 40, 40),
        egui::Color32::from_rgb(20, 60, 50),
    ];
    for (pos, col) in [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()].into_iter().zip(c) {
        mesh.colored_vertex(pos, col);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(egui::Shape::mesh(mesh));
    p.circle_filled(r.center() + egui::vec2(-r.width() * 0.2, -r.height() * 0.15), r.height() * 0.12, egui::Color32::from_rgb(250, 220, 150));
}
