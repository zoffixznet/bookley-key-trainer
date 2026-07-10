//! Render the source SVG icon to the standard PNG sizes and write them into the hicolor
//! icon theme layout under assets/icon/hicolor. Run via `make icons`.

use resvg::tiny_skia;
use resvg::usvg;

const SVG: &str = include_str!("../../assets/icon/bookley-key-trainer.svg");
const SIZES: [u32; 6] = [16, 32, 48, 64, 128, 256];

fn main() {
    let out_root = std::path::Path::new("assets/icon/hicolor");
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_str(SVG, &opt).expect("parse svg");
    let svg_size = tree.size();

    for size in SIZES {
        let mut pixmap = tiny_skia::Pixmap::new(size, size).expect("pixmap");
        let scale = size as f32 / svg_size.width().max(svg_size.height());
        let transform = tiny_skia::Transform::from_scale(scale, scale);
        resvg::render(&tree, transform, &mut pixmap.as_mut());

        let dir = out_root.join(format!("{size}x{size}")).join("apps");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("bookley-key-trainer.png");
        pixmap.save_png(&path).expect("save png");
        println!("wrote {}", path.display());
    }

    // Also write a top-level 256 PNG the app loads at runtime for the window icon.
    let mut pixmap = tiny_skia::Pixmap::new(256, 256).expect("pixmap");
    let scale = 256.0 / svg_size.width().max(svg_size.height());
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap
        .save_png("assets/icon/bookley-key-trainer-256.png")
        .expect("save window icon");
    println!("wrote assets/icon/bookley-key-trainer-256.png");
}
