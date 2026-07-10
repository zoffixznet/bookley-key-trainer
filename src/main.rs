//! Bookley Key Trainer entry point: CLI parsing, logging, and the eframe run loop with a
//! wgpu -> glow fallback so a broken GPU stack degrades instead of panicking.

use std::sync::Arc;

use clap::Parser;

use bookley::core::book::agent::ClaudeRunner;
use bookley::core::config::Config;
use bookley::ui::app::App;

#[derive(Parser, Debug)]
#[command(
    name = "bookley",
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
    let screenshot = args.screenshot.clone();
    let screen = args.screen.clone();
    let paused = args.paused;

    // Try the default wgpu renderer first; fall back to glow (OpenGL) if the GPU stack
    // cannot initialize, and exit with a clear message if both fail.
    match run_gui(
        config.clone(),
        dev,
        screenshot.clone(),
        screen.clone(),
        paused,
        eframe::Renderer::Wgpu,
    ) {
        Ok(()) => {}
        Err(e) => {
            tracing::warn!("wgpu renderer failed ({e}); retrying with glow (OpenGL)");
            if let Err(e2) = run_gui(
                config,
                dev,
                screenshot,
                screen,
                paused,
                eframe::Renderer::Glow,
            ) {
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
        m.record_keystroke(Some(key), !wrong, period * 1000.0);
        m.tick(t);
    }
    bookley::core::metrics::SessionResult::from_metrics(&m, "word")
}

fn run_gui(
    config: Config,
    dev: bool,
    screenshot: Option<std::path::PathBuf>,
    screen: Option<String>,
    paused: bool,
    renderer: eframe::Renderer,
) -> Result<(), eframe::Error> {
    let icon =
        eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon/bookley-256.png")).ok();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Bookley Key Trainer")
        .with_inner_size([1240.0, 900.0])
        .with_min_inner_size([860.0, 600.0])
        .with_app_id("bookley");
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
            app.screenshot_path = screenshot.clone();
            // Screenshot-mode conveniences: jump to a screen, open a book, or pause.
            if app.config.content_mode == bookley::core::config::ContentMode::Book
                && screenshot.is_some()
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
            match screen.as_deref() {
                Some("books") => app.screen = bookley::ui::app::Screen::Books,
                Some("settings") => app.screen = bookley::ui::app::Screen::Settings,
                Some("connect") => app.screen = bookley::ui::app::Screen::Connect,
                Some("results") => {
                    app.screen = bookley::ui::app::Screen::Results;
                    if app.last_result.is_none() && screenshot.is_some() {
                        app.last_result = Some(demo_result());
                        app.last_was_pb = true;
                    }
                }
                _ => {}
            }
            if paused {
                app.toggle_pause();
            }
            Ok(Box::new(app))
        }),
    )
}
