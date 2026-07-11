//! Book-cover generation. Claude cannot paint rasters, but it can DESIGN: we ask it for
//! a single self-contained SVG based on the book's actual title and text so far, then
//! validate the SVG defensively and rasterize it with the existing resvg/usvg pipeline
//! into a PNG stored with the book (and used as page one of the PDF).
//!
//! Failure policy: on a parse/validate/render failure the run is retried once with the
//! error fed back; if that also fails, a clean locally-generated typographic cover is
//! rasterized instead, so the Generate-cover button always yields a cover. Auth/spawn
//! errors propagate with the same classification and handling as chapter generation.

use std::sync::atomic::AtomicBool;

use resvg::{tiny_skia, usvg};

use super::agent::{CommandRunner, GenError, GenRequest};
use super::store::{display_title, Book};

/// Raster size of the stored cover (matches the requested 1600x2560 viewBox, a standard
/// 1:1.6 portrait book cover).
pub const COVER_W: u32 = 1600;
pub const COVER_H: u32 = 2560;

/// System prompt for the design task. The novelist craft is irrelevant here; the whole
/// contract is "one self-contained SVG, nothing else".
const COVER_SYSTEM_PROMPT: &str = "You are a book cover designer. You design striking, \
professional covers and output them as a SINGLE self-contained SVG document. You return \
ONLY the SVG markup: no prose, no code fences, no commentary before or after it.";

/// Build the design prompt from the book's actual content. `previous_error` feeds the
/// retry after a failed parse/render.
pub fn cover_prompt(book: &Book, previous_error: Option<&str>) -> String {
    let title = display_title(&book.meta);
    let mut p = String::new();
    p.push_str(
        "Design the front cover of this book and return it as a SINGLE self-contained \
SVG document.\n\n",
    );
    p.push_str(&format!(
        "## Book title (must appear on the cover)\n{title}\n\n"
    ));
    let sample = story_sample(book);
    if sample.trim().is_empty() {
        p.push_str(
            "No chapters exist yet: design from the title alone (and pick imagery that \
could suit many stories bearing it).\n\n",
        );
    } else {
        p.push_str("## Text so far (for mood, imagery, and genre)\n");
        p.push_str(&sample);
        p.push_str("\n\n");
    }
    p.push_str(
        "## HARD CONSTRAINTS (violating any one makes the output unusable)\n\
- Return ONLY the SVG markup. No explanation, no markdown fences. The output must \
start with <svg and end with </svg>.\n\
- Root element: <svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1600 2560\"> \
(portrait book cover). Paint the full canvas; start with a background rect covering \
0 0 1600 2560.\n\
- Use ONLY these elements: svg, g, defs, rect, circle, ellipse, line, polyline, \
polygon, path, text, tspan, linearGradient, radialGradient, stop.\n\
- Absolutely NO external or exotic references: no image elements, no href or \
xlink:href attributes, no use elements, no style elements or CSS, no script, no \
filter elements or filter attributes, no masks or clipPaths, no foreignObject, no \
data: URIs, no embedded fonts or font-face. Gradients are referenced only as \
fill=\"url(#someId)\".\n\
- Text must use only generic font families (\"serif\", \"sans-serif\", or \
\"monospace\"), or be drawn as pure path geometry.\n\
- The exact book title must appear prominently and legibly. No author name, no \
publisher marks, no barcodes.\n\n\
## Design intent\n\
A cover a reader would pick up: one strong idea drawn from the story's imagery, a \
restrained palette, deliberate composition, high contrast between the title and its \
background. Prefer bold geometry and negative space over clutter.\n",
    );
    if let Some(err) = previous_error {
        p.push_str(&format!(
            "\n## Your previous attempt failed\nIt could not be used because: {err}\n\
Return a corrected SVG that honors every constraint above.\n"
        ));
    }
    p
}

/// A bounded sample of the book's text so far: the opening of chapter 1 plus the tail of
/// the latest chapter.
fn story_sample(book: &Book) -> String {
    let mut out = String::new();
    let ns: Vec<usize> = book.meta.chapters.iter().map(|c| c.n).collect();
    if let Some(&first) = ns.first() {
        if let Ok(text) = book.read_chapter(first) {
            let head: String = text.chars().take(1200).collect();
            out.push_str(&head);
        }
    }
    if let Some(&last) = ns.last() {
        if last != *ns.first().unwrap_or(&last) {
            if let Ok(text) = book.read_chapter(last) {
                let chars: Vec<char> = text.chars().collect();
                let start = chars.len().saturating_sub(600);
                out.push_str("\n[...]\n");
                out.push_str(&chars[start..].iter().collect::<String>());
            }
        }
    }
    out
}

/// Pull the SVG document out of a possibly chatty reply: first `<svg` to last `</svg>`.
pub fn extract_svg(text: &str) -> Option<String> {
    let start = text.find("<svg")?;
    let end = text.rfind("</svg>")? + "</svg>".len();
    if end <= start {
        return None;
    }
    Some(text[start..end].to_string())
}

/// Defensive validation of a generated SVG before it goes anywhere near the renderer:
/// self-contained basic shapes/paths/text/gradients only, no external references, no
/// filters, no scripts, no embedded fonts.
pub fn validate_svg(svg: &str) -> Result<(), String> {
    if svg.len() > 300_000 {
        return Err(format!("SVG too large ({} bytes)", svg.len()));
    }
    let low = svg.to_lowercase();
    if !low.contains("viewbox") {
        return Err("missing viewBox".into());
    }
    const BANNED: [(&str, &str); 12] = [
        ("<script", "script element"),
        ("<image", "image element"),
        ("<use", "use element"),
        ("<style", "style element"),
        ("<filter", "filter element"),
        ("filter=", "filter attribute"),
        ("<foreignobject", "foreignObject element"),
        ("<mask", "mask element"),
        ("href", "href/xlink:href reference"),
        ("data:", "data: URI"),
        ("@font-face", "embedded font"),
        ("<iframe", "iframe element"),
    ];
    for (needle, what) in BANNED {
        if low.contains(needle) {
            return Err(format!("forbidden content: {what}"));
        }
    }
    // url(...) is only allowed for same-document gradient references: url(#id).
    let mut rest = low.as_str();
    while let Some(i) = rest.find("url(") {
        let after = &rest[i + 4..];
        if !after.starts_with('#') {
            return Err("url() reference that is not a local #id".into());
        }
        rest = after;
    }
    Ok(())
}

/// Rasterize a validated SVG to the cover PNG via usvg/resvg (tiny-skia), scaled to
/// cover the full canvas and centered. Fails if the SVG cannot be parsed, or renders to
/// a (near-)blank image.
pub fn rasterize_svg_to_png(svg: &str) -> Result<Vec<u8>, String> {
    let mut opt = usvg::Options::default();
    {
        let db = opt.fontdb_mut();
        // The embedded app fonts guarantee text renders even on a fontless system, and
        // map the generic families the prompt allows.
        db.load_font_data(include_bytes!("../../../assets/fonts/YoungSerif-Regular.ttf").to_vec());
        db.load_font_data(include_bytes!("../../../assets/fonts/IBMPlexSans-Regular.ttf").to_vec());
        db.load_font_data(include_bytes!("../../../assets/fonts/IBMPlexMono-Regular.ttf").to_vec());
        db.load_system_fonts();
        db.set_serif_family("Young Serif");
        db.set_sans_serif_family("IBM Plex Sans");
        db.set_monospace_family("IBM Plex Mono");
    }
    let tree = usvg::Tree::from_str(svg, &opt).map_err(|e| format!("SVG parse failed: {e}"))?;
    let size = tree.size();
    if size.width() < 1.0 || size.height() < 1.0 {
        return Err("SVG has a degenerate size".into());
    }
    let mut pixmap = tiny_skia::Pixmap::new(COVER_W, COVER_H).ok_or("pixmap alloc failed")?;
    let sx = COVER_W as f32 / size.width();
    let sy = COVER_H as f32 / size.height();
    let s = sx.max(sy); // cover the canvas; center the overflow
    let tx = (COVER_W as f32 - size.width() * s) / 2.0;
    let ty = (COVER_H as f32 - size.height() * s) / 2.0;
    let transform = tiny_skia::Transform::from_scale(s, s).post_translate(tx, ty);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    // Defensive: a cover that rendered to (almost) nothing is a failed render.
    let painted = pixmap.pixels().iter().filter(|px| px.alpha() > 0).count();
    if (painted as f64) < 0.5 * (COVER_W as f64 * COVER_H as f64) {
        return Err("SVG rendered mostly blank".into());
    }
    pixmap
        .encode_png()
        .map_err(|e| format!("PNG encode failed: {e}"))
}

/// Escape text for embedding in SVG.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// The local fallback: a clean typographic cover (title on a themed background) in the
/// app's ink/brass/verdigris identity. Always renderable by our own pipeline.
pub fn fallback_cover_svg(title: &str) -> String {
    // Wrap the title into short centered lines.
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in title.split_whitespace() {
        if !current.is_empty() && current.chars().count() + 1 + word.chars().count() > 14 {
            lines.push(current.clone());
            current.clear();
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push("Untitled".to_string());
    }
    lines.truncate(6);
    let longest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(8);
    // Fit the longest line inside ~1360px of width; serif glyphs average ~0.62em.
    let font_size = (1360.0 / (longest as f32 * 0.62)).clamp(70.0, 210.0);
    let line_h = font_size * 1.18;
    let block_h = line_h * lines.len() as f32;
    let first_y = 1210.0 - block_h / 2.0 + line_h * 0.75;
    let text_lines: String = lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            format!(
                "<text x=\"800\" y=\"{:.0}\" text-anchor=\"middle\" \
font-family=\"serif\" font-size=\"{:.0}\" fill=\"#C9A24B\">{}</text>\n",
                first_y + i as f32 * line_h,
                font_size,
                xml_escape(l)
            )
        })
        .collect();
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1600 2560\">\n\
<rect x=\"0\" y=\"0\" width=\"1600\" height=\"2560\" fill=\"#14110E\"/>\n\
<rect x=\"70\" y=\"70\" width=\"1460\" height=\"2420\" fill=\"none\" \
stroke=\"#C9A24B\" stroke-width=\"6\"/>\n\
<rect x=\"100\" y=\"100\" width=\"1400\" height=\"2360\" fill=\"none\" \
stroke=\"#3E9C8C\" stroke-width=\"2\"/>\n\
<line x1=\"420\" y1=\"760\" x2=\"1180\" y2=\"760\" stroke=\"#3E9C8C\" stroke-width=\"5\"/>\n\
{text_lines}\
<line x1=\"420\" y1=\"1700\" x2=\"1180\" y2=\"1700\" stroke=\"#3E9C8C\" stroke-width=\"5\"/>\n\
<circle cx=\"800\" cy=\"2100\" r=\"46\" fill=\"none\" stroke=\"#C9A24B\" stroke-width=\"5\"/>\n\
<rect x=\"778\" y=\"2078\" width=\"44\" height=\"44\" fill=\"#3E9C8C\"/>\n\
</svg>\n"
    )
}

/// A finished cover: PNG bytes plus how it was obtained.
#[derive(Debug, Clone)]
pub struct CoverOutcome {
    pub png: Vec<u8>,
    /// True when both generation attempts failed and the local typographic cover was
    /// rasterized instead.
    pub used_fallback: bool,
    pub attempts: u32,
}

/// Run the whole cover pipeline synchronously (callers put it on a worker thread):
/// generate -> extract -> validate -> rasterize, retry once feeding the error back,
/// then fall back to the local typographic cover. Spawn/auth/rate-limit errors from the
/// CLI propagate unchanged (same classification and handling as chapter generation).
/// Turn a user-chosen image file into the book's cover PNG: decode (PNG/JPEG/WebP),
/// bound the dimensions to the cover canvas (never upscales), and re-encode as PNG for
/// the same on-disk format and PDF pipeline as generated covers.
pub fn process_uploaded_cover(path: &std::path::Path) -> Result<Vec<u8>, String> {
    const MAX_BYTES: u64 = 50 * 1024 * 1024;
    let meta = std::fs::metadata(path).map_err(|e| format!("could not read the file: {e}"))?;
    if meta.len() > MAX_BYTES {
        return Err("the file is larger than 50 MB".into());
    }
    let bytes = std::fs::read(path).map_err(|e| format!("could not read the file: {e}"))?;
    let img = image::load_from_memory(&bytes)
        .map_err(|e| format!("not a usable image (PNG, JPEG, or WebP): {e}"))?;
    // Bound to the cover canvas, preserving aspect. thumbnail() fits the bounds in
    // BOTH directions (it upscales small images), so only apply it when the image is
    // actually larger than the canvas.
    let img = if img.width() > 1600 || img.height() > 2560 {
        img.thumbnail(1600, 2560)
    } else {
        img
    };
    let mut png = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| format!("could not encode the cover: {e}"))?;
    Ok(png)
}

pub fn generate_cover_blocking(
    runner: &dyn CommandRunner,
    book: &Book,
    model: &str,
    plugin_dir: std::path::PathBuf,
    timeout_secs: u64,
    cancel: &AtomicBool,
) -> Result<CoverOutcome, GenError> {
    let mut last_err: Option<String> = None;
    for attempt in 1..=2u32 {
        let req = GenRequest {
            prompt: cover_prompt(book, last_err.as_deref()),
            system_prompt: COVER_SYSTEM_PROMPT.to_string(),
            model: model.to_string(),
            plugin_dir: plugin_dir.clone(),
            cwd: book.dir.clone(),
            // A design task, deliberately not resumed into the novel's session.
            resume_session: None,
            fork_session: false,
            stream: true,
            timeout_secs,
        };
        // A CLI failure (auth, rate limit, cancel...) propagates like chapters do; only
        // an unusable DESIGN is retried and eventually falls back.
        let done = runner.run(&req, cancel, &mut |_| {})?;
        match svg_reply_to_png(&done.text) {
            Ok(png) => {
                tracing::info!("cover generated attempt={attempt} bytes={}", png.len());
                return Ok(CoverOutcome {
                    png,
                    used_fallback: false,
                    attempts: attempt,
                });
            }
            Err(e) => {
                tracing::warn!("cover attempt {attempt} unusable: {e}");
                last_err = Some(e);
            }
        }
    }
    tracing::warn!("cover generation failed twice; using the local typographic fallback");
    let png = rasterize_svg_to_png(&fallback_cover_svg(&display_title(&book.meta)))
        .map_err(GenError::Other)?;
    Ok(CoverOutcome {
        png,
        used_fallback: true,
        attempts: 2,
    })
}

/// Reply text -> validated, rasterized PNG.
fn svg_reply_to_png(raw: &str) -> Result<Vec<u8>, String> {
    let svg = extract_svg(raw).ok_or("the reply contains no <svg>...</svg> document")?;
    validate_svg(&svg)?;
    rasterize_svg_to_png(&svg)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD_SVG: &str = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 1600 2560'>\
<defs><linearGradient id='sky'><stop offset='0' stop-color='#223'/>\
<stop offset='1' stop-color='#667'/></linearGradient></defs>\
<rect x='0' y='0' width='1600' height='2560' fill='url(#sky)'/>\
<circle cx='800' cy='700' r='300' fill='#C9A24B'/>\
<text x='800' y='1600' text-anchor='middle' font-family='serif' font-size='160' \
fill='#fff'>The Test</text></svg>";

    #[test]
    fn extract_svg_from_noisy_reply() {
        let raw = format!("Sure! Here is the cover:\n\n{GOOD_SVG}\n\nHope you like it!");
        let svg = extract_svg(&raw).unwrap();
        assert!(svg.starts_with("<svg"));
        assert!(svg.ends_with("</svg>"));
        assert!(!svg.contains("Hope you like it"));
        assert!(extract_svg("no vector art here").is_none());
    }

    #[test]
    fn validate_accepts_basic_shapes_and_local_gradients() {
        assert!(validate_svg(GOOD_SVG).is_ok());
    }

    #[test]
    fn validate_rejects_external_and_exotic_content() {
        for bad in [
            "<svg viewBox='0 0 1 1'><image href='x.png'/></svg>",
            "<svg viewBox='0 0 1 1'><script>alert(1)</script></svg>",
            "<svg viewBox='0 0 1 1'><use xlink:href='#a'/></svg>",
            "<svg viewBox='0 0 1 1'><filter id='f'/></svg>",
            "<svg viewBox='0 0 1 1'><rect filter='url(#f)'/></svg>",
            "<svg viewBox='0 0 1 1'><style>@font-face{}</style></svg>",
            "<svg viewBox='0 0 1 1'><rect fill='url(http://evil)'/></svg>",
            "<svg viewBox='0 0 1 1'><foreignObject/></svg>",
            "<svg><rect/></svg>", // no viewBox
        ] {
            assert!(validate_svg(bad).is_err(), "should reject: {bad}");
        }
    }

    #[test]
    fn rasterize_produces_a_real_png() {
        let png = rasterize_svg_to_png(GOOD_SVG).unwrap();
        assert!(png.starts_with(b"\x89PNG"), "missing PNG magic");
        assert!(png.len() > 5_000, "png suspiciously small: {}", png.len());
    }

    #[test]
    fn rasterize_rejects_blank_renders() {
        // Parses fine but paints nothing: must be treated as a failed render.
        let blank = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 1600 2560'></svg>";
        assert!(rasterize_svg_to_png(blank).is_err());
    }

    #[test]
    fn fallback_cover_always_renders() {
        for title in [
            "Salt",
            "The Extraordinarily Long and Winding Title of Doom",
            "Ampersands & <Angles> \"Quoted\"",
            "",
        ] {
            let svg = fallback_cover_svg(title);
            assert!(validate_svg(&svg).is_ok(), "fallback must pass validation");
            let png = rasterize_svg_to_png(&svg).unwrap();
            assert!(png.starts_with(b"\x89PNG"));
            assert!(png.len() > 5_000);
        }
    }

    #[test]
    fn prompt_carries_title_constraints_and_retry_error() {
        let root = std::env::temp_dir().join(format!("bookley-cover-{}", std::process::id()));
        let store = super::super::store::BookStore::new(root.clone());
        let mut book = store.create("The Salt Road", "English", "", false).unwrap();
        book.write_chapter(1, "One", "A long road of salt.", "")
            .unwrap();
        let p = cover_prompt(&book, None);
        assert!(p.contains("The Salt Road"));
        assert!(p.contains("viewBox=\"0 0 1600 2560\""));
        assert!(p.contains("A long road of salt."));
        assert!(p.contains("NO external"));
        let retry = cover_prompt(&book, Some("SVG parse failed: boom"));
        assert!(retry.contains("previous attempt failed"));
        assert!(retry.contains("boom"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn uploaded_cover_roundtrips_and_bounds() {
        let dir = std::env::temp_dir().join(format!("bookley-cover-up-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // A small JPEG becomes a PNG cover without upscaling.
        let jpg_path = dir.join("c.jpg");
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            40,
            64,
            image::Rgb([180, 120, 40]),
        ))
        .save(&jpg_path)
        .unwrap();
        let png = process_uploaded_cover(&jpg_path).expect("jpeg accepted");
        assert!(png.starts_with(b"\x89PNG"), "re-encoded as png");
        let out = image::load_from_memory(&png).unwrap();
        assert_eq!((out.width(), out.height()), (40, 64), "no upscaling");
        // An oversized image is bounded to the cover canvas.
        let big_path = dir.join("big.png");
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            3200,
            3200,
            image::Rgb([10, 10, 10]),
        ))
        .save(&big_path)
        .unwrap();
        let png = process_uploaded_cover(&big_path).expect("big image accepted");
        let out = image::load_from_memory(&png).unwrap();
        assert!(out.width() <= 1600 && out.height() <= 2560, "bounded");
        // Garbage is rejected with a message, not a panic.
        let junk = dir.join("junk.webp");
        std::fs::write(&junk, b"not an image").unwrap();
        assert!(process_uploaded_cover(&junk).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
