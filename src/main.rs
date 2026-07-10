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

    let config = Config::load_from(&bookley::core::config::config_path());
    let dev = args.dev;
    let screenshot = args.screenshot.clone();

    // Try the default wgpu renderer first; fall back to glow (OpenGL) if the GPU stack
    // cannot initialize, and exit with a clear message if both fail.
    match run_gui(
        config.clone(),
        dev,
        screenshot.clone(),
        eframe::Renderer::Wgpu,
    ) {
        Ok(()) => {}
        Err(e) => {
            tracing::warn!("wgpu renderer failed ({e}); retrying with glow (OpenGL)");
            if let Err(e2) = run_gui(config, dev, screenshot, eframe::Renderer::Glow) {
                eprintln!(
                    "Could not initialize a GPU context (wgpu: {e}; glow: {e2}).\n\
Bookley needs OpenGL or Vulkan. On X11/Wayland, check your graphics drivers."
                );
                std::process::exit(1);
            }
        }
    }
}

fn run_gui(
    config: Config,
    dev: bool,
    screenshot: Option<std::path::PathBuf>,
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
            app.screenshot_path = screenshot;
            Ok(Box::new(app))
        }),
    )
}
