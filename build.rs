//! Rasterizes the Lucide icons once, at build time, so the TV binary carries
//! no SVG renderer (resvg/usvg/tiny-skia stay build dependencies).
//!
//! Each `assets/icons/*.svg` becomes a 128×128 alpha mask, run-length encoded
//! as (count, value) byte pairs: Lucide glyphs are mostly empty, so a mask
//! shrinks to a few KiB. `src/icons.rs` decodes them into egui textures.

use std::{env, fmt::Write as _, fs, path::Path};

const PX: u32 = 128;

fn rle(alpha: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < alpha.len() {
        let v = alpha[i];
        let mut n = 1;
        while i + n < alpha.len() && alpha[i + n] == v && n < 255 {
            n += 1;
        }
        out.push(n as u8);
        out.push(v);
        i += n;
    }
    out
}

fn main() {
    println!("cargo:rerun-if-changed=assets/icons");
    let out = env::var("OUT_DIR").unwrap();
    let mut entries: Vec<_> = fs::read_dir("assets/icons")
        .expect("assets/icons")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "svg"))
        .collect();
    entries.sort();

    let mut code = format!("/// Side of every icon mask, in pixels.\npub const ICON_PX: usize = {PX};\n\n");
    code.push_str("/// (name, run-length encoded alpha mask) for every icon.\npub static ICONS: &[(&str, &[u8])] = &[\n");
    for path in entries {
        println!("cargo:rerun-if-changed={}", path.display());
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        // White strokes: the alpha channel is the whole glyph.
        let svg = fs::read_to_string(&path).unwrap().replace("currentColor", "#ffffff");
        let tree = resvg::usvg::Tree::from_data(svg.as_bytes(), &resvg::usvg::Options::default())
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let size = tree.size();
        let scale = PX as f32 / size.width().max(size.height());
        let mut pixmap = resvg::tiny_skia::Pixmap::new(PX, PX).unwrap();
        resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
        let alpha: Vec<u8> = pixmap.data().chunks_exact(4).map(|p| p[3]).collect();
        let file = Path::new(&out).join(format!("icon_{name}.rle"));
        fs::write(&file, rle(&alpha)).unwrap();
        writeln!(code, "    ({name:?}, include_bytes!(concat!(env!(\"OUT_DIR\"), \"/icon_{name}.rle\"))),").unwrap();
    }
    code.push_str("];\n");
    fs::write(Path::new(&out).join("icons.rs"), code).unwrap();
}
