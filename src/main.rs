//! Bookley Key Trainer entry point: CLI parsing, logging, and the eframe run loop with a
//! wgpu -> glow fallback so a broken GPU stack degrades instead of panicking.

use std::sync::Arc;

use clap::Parser;

use bookley::core::book::agent::ClaudeRunner;
use bookley::core::config::Config;
use bookley::ui::app::App;

#[derive(Parser, Debug)]
#[command(
    name = "bookley-key-trainer",
    about = "Bookley Key Trainer: type your way through a novel"
)]
struct Args {
    /// Developer mode: F9 auto-types the next expected key (hold to keep going),
    /// F10 completes the current page, F12 completes the whole chapter/text.
    #[arg(long, hide = true)]
    dev: bool,

    /// Headless end-to-end self-test; exits non-zero on failure.
    #[arg(long, hide = true)]
    smoke: bool,

    /// With --smoke: run one REAL generation against the real claude CLI
    /// (needs a connected Claude; consumes subscription usage).
    #[arg(long, hide = true)]
    live: bool,

    /// Boot the app, render a few frames, save a screenshot to PATH, and exit.
    #[arg(long, hide = true, value_name = "PATH")]
    screenshot: Option<std::path::PathBuf>,

    /// With --screenshot: which screen to capture (typing|books|settings|connect|results).
    #[arg(long, hide = true, value_name = "NAME")]
    screen: Option<String>,

    /// With --screenshot: force a theme (light|dark) for the capture.
    #[arg(long, hide = true, value_name = "THEME")]
    theme: Option<String>,

    /// With --screenshot: force a content mode (random|word|paste|book) for the capture.
    #[arg(long, hide = true, value_name = "MODE")]
    content: Option<String>,

    /// With --screenshot: pause the drill before capturing (verifies the pause overlay).
    #[arg(long, hide = true)]
    paused: bool,

    /// With --screenshot: keep the press-Space-to-start gate up instead of auto-starting.
    #[arg(long, hide = true)]
    gate: bool,

    /// With --screenshot: pre-type a scripted mix of right and wrong keys so error
    /// styling is visible in the capture.
    #[arg(long, hide = true)]
    demo_errors: bool,
}

fn main() {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    // Rename legacy "bookleykeytrainer" config/data dirs to the current
    // "bookley-key-trainer" names before anything reads them (books move with it).
    bookley::core::paths::migrate_legacy_dirs();

    if args.smoke {
        match bookley::smoke::run(args.live) {
            Ok(()) => std::process::exit(0),
            Err(e) => {
                eprintln!("smoke failed: {e}");
                std::process::exit(1);
            }
        }
    }

    let mut config = Config::load_from(&bookley::core::config::config_path());
    match args.theme.as_deref() {
        Some("light") => config.theme = bookley::core::config::Theme::Light,
        Some("dark") => config.theme = bookley::core::config::Theme::Dark,
        _ => {}
    }
    match args.content.as_deref() {
        Some("random") => config.content_mode = bookley::core::config::ContentMode::Random,
        Some("word") => config.content_mode = bookley::core::config::ContentMode::Word,
        Some("paste") => config.content_mode = bookley::core::config::ContentMode::Paste,
        Some("book") => config.content_mode = bookley::core::config::ContentMode::Book,
        _ => {}
    }
    let dev = args.dev;
    let shot = ShotOpts {
        path: args.screenshot.clone(),
        screen: args.screen.clone(),
        paused: args.paused,
        gate: args.gate,
        demo_errors: args.demo_errors,
    };

    // Try the default wgpu renderer first; fall back to glow (OpenGL) if the GPU stack
    // cannot initialize, and exit with a clear message if both fail.
    match run_gui(config.clone(), dev, shot.clone(), eframe::Renderer::Wgpu) {
        Ok(()) => {}
        Err(e) => {
            tracing::warn!("wgpu renderer failed ({e}); retrying with glow (OpenGL)");
            if let Err(e2) = run_gui(config, dev, shot, eframe::Renderer::Glow) {
                eprintln!(
                    "Could not initialize a GPU context (wgpu: {e}; glow: {e2}).\n\
Bookley needs OpenGL or Vulkan. On X11/Wayland, check your graphics drivers."
                );
                std::process::exit(1);
            }
        }
    }
}

/// A plausible fabricated result so the results screen can be screenshot-verified
/// without typing a whole drill. Only used in screenshot mode.
fn demo_result() -> bookley::core::metrics::SessionResult {
    use bookley::core::metrics::Metrics;
    let mut m = Metrics::new();
    let keys = [
        egui::Key::E,
        egui::Key::T,
        egui::Key::A,
        egui::Key::O,
        egui::Key::I,
        egui::Key::N,
        egui::Key::S,
        egui::Key::R,
    ];
    let mut t = 0.0;
    for k in 0..420u32 {
        // A wavy pace with occasional errors on a couple of keys.
        let period = 0.14 + 0.05 * ((k as f64 / 30.0).sin().abs());
        t += period;
        let key = keys[(k as usize) % keys.len()];
        let wrong = k % 23 == 0 && (key == egui::Key::R || key == egui::Key::I);
        let latency = if k == 0 { None } else { Some(period * 1000.0) };
        m.record_keystroke(Some(key), !wrong, latency);
        m.tick(t);
    }
    bookley::core::metrics::SessionResult::from_metrics(&m, "word")
}

/// Screenshot-mode options bundled for the two renderer attempts.
#[derive(Clone, Default)]
struct ShotOpts {
    path: Option<std::path::PathBuf>,
    screen: Option<String>,
    paused: bool,
    gate: bool,
    demo_errors: bool,
}

fn run_gui(
    config: Config,
    dev: bool,
    shot: ShotOpts,
    renderer: eframe::Renderer,
) -> Result<(), eframe::Error> {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!(
        "../assets/icon/bookley-key-trainer-256.png"
    ))
    .ok();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Bookley Key Trainer")
        .with_inner_size([1240.0, 900.0])
        // Wide enough that the top bar's tabs and right cluster can never collide
        // (the bar also drops its eyebrow labels below ~1235px).
        .with_min_inner_size([1120.0, 640.0])
        .with_app_id("bookley-key-trainer");
    if let Some(icon) = icon {
        viewport = viewport.with_icon(Arc::new(icon));
    }

    let options = eframe::NativeOptions {
        viewport,
        renderer,
        ..Default::default()
    };

    eframe::run_native(
        "Bookley Key Trainer",
        options,
        Box::new(move |cc| {
            let runner = Arc::new(ClaudeRunner::new());
            let mut app = App::new(&cc.egui_ctx, config, dev, runner);
            app.screenshot_path = shot.path.clone();
            // Screenshot-mode conveniences: jump to a screen, open a book, or pause.
            if app.config.content_mode == bookley::core::config::ContentMode::Book
                && shot.path.is_some()
            {
                if let Some(b) = app
                    .store
                    .list()
                    .into_iter()
                    .find(|b| !b.all_chapters_typed())
                {
                    app.book_ui.open_slug = Some(b.meta.slug.clone());
                    app.start_session();
                }
            }
            match shot.screen.as_deref() {
                Some("books") => app.screen = bookley::ui::app::Screen::Books,
                Some("settings") => app.screen = bookley::ui::app::Screen::Settings,
                Some("connect") => app.screen = bookley::ui::app::Screen::Connect,
                Some("results") => {
                    app.screen = bookley::ui::app::Screen::Results;
                    if app.last_result.is_none() && shot.path.is_some() {
                        app.last_result = Some(demo_result());
                        app.last_was_pb = true;
                    }
                }
                _ => {}
            }
            // Screenshots capture a running session unless the gate itself is wanted.
            if shot.path.is_some() && !shot.gate {
                app.begin_after_gate();
            }
            if shot.demo_errors {
                if let Some(s) = app.session.as_mut() {
                    demo_type_with_errors(s);
                }
            }
            if shot.paused {
                app.toggle_pause();
            }
            Ok(Box::new(app))
        }),
    )
}

/// Screenshot verification: feed a scripted mix of correct and wrong keystrokes through
/// the real session pipeline so typed/error/pending styling is all visible at once.
fn demo_type_with_errors(s: &mut bookley::core::session::Session) {
    let mut t = 0.0;
    for i in 0..60usize {
        t += 0.18;
        let Some(expected) = s.expected().cloned() else {
            break;
        };
        // Every 4th keystroke is wrong.
        if i % 4 == 3 {
            match expected {
                bookley::core::text_source::Expected::Char(_) => {
                    s.input_char('\u{0}', t);
                }
                bookley::core::text_source::Expected::PhysicalKey(_) => {
                    s.input_physical_key(egui::Key::F20, t);
                }
            }
        } else {
            s.dev_autotype_next(t);
        }
    }
}
