//! The eframe application: state, the update loop, input handling, and view dispatch.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Instant;

use egui::{Event, Key};

use crate::core::book::agent::{AgentClient, AgentEvent, CommandRunner, GenError};
use crate::core::book::store::{Book, BookStore};
use crate::core::claude_auth::{AuthCheck, ConnectEvent, ConnectFlow};
use crate::core::config::{Config, ContentMode, KeyboardMode};
use crate::core::keys;
use crate::core::metrics::SessionResult;
use crate::core::paths;
use crate::core::session::{Progress, Session};
use crate::core::stats_store::Stats;
use crate::core::text_source::{
    BookSource, PasteSource, RandomSource, Target, TextSource, WordSource,
};
use crate::ui::keyboard::FlashState;
use crate::ui::{books, connect, results, settings, stage, theme};

/// Which top-level screen is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Typing,
    Results,
    Books,
    Settings,
    Connect,
}

/// State of a book-generation request in flight.
pub struct BookGen {
    pub rx: Receiver<AgentEvent>,
    pub cancel: Arc<AtomicBool>,
    pub live_text: String,
    pub n: usize,
    pub is_rewrite: bool,
    pub started: Instant,
}

/// UI state of the Connect Claude flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectUiState {
    Idle,
    Starting,
    UrlShown { url: String, waiting_for_code: bool },
    Verifying,
    Failed(String),
}

/// Authentication state owned by the app: background checks + a live connect flow.
pub struct AuthUi {
    /// Last background check result (None while a check is in flight / not yet run).
    pub check: Option<AuthCheck>,
    pub check_rx: Option<Receiver<AuthCheck>>,
    /// A running PTY-driven connect flow, if any.
    pub flow: Option<ConnectFlow>,
    pub state: ConnectUiState,
    pub code_input: String,
}

impl Default for AuthUi {
    fn default() -> Self {
        AuthUi {
            check: None,
            check_rx: None,
            flow: None,
            state: ConnectUiState::Idle,
            code_input: String::new(),
        }
    }
}

/// Book-mode UI sub-state.
#[derive(Default)]
pub struct BookUi {
    /// The book currently open for typing/managing.
    pub open_slug: Option<String>,
    /// Draft fields for creating a new book.
    pub new_title: String,
    pub new_language: String,
    pub new_premise: String,
    pub show_create: bool,
    /// The single-line "how should the story continue" input.
    pub continuation: String,
    /// The clarifying-question round: questions from the agent + the user's answer.
    pub pending_questions: Option<String>,
    pub clarify_answer: String,
    /// A confirmation prompt for fully-AI-invented content when inputs are blank.
    pub confirm_blank: bool,
    /// Rewrite dialog: which chapter and the instruction.
    pub rewrite_n: Option<usize>,
    pub rewrite_instruction: String,
    /// Last status/error message to show.
    pub status: Option<String>,
    /// Which chapter is being typed (1-based).
    pub typing_chapter: Option<usize>,
    /// Slug pending delete confirmation.
    pub confirm_delete: Option<String>,
}

pub struct App {
    pub config: Config,
    pub stats: Stats,
    pub screen: Screen,
    pub dev_mode: bool,

    // Active typing session (None between sessions).
    pub session: Option<Session>,
    pub session_started: Option<Instant>,
    pub last_result: Option<SessionResult>,
    pub last_was_pb: bool,

    // Text sources per mode (recreated as needed).
    pub paste_input: String,

    // Keyboard flash animation state.
    pub flash: FlashState,

    // Book mode.
    pub store: BookStore,
    pub agent: AgentClient,
    pub runner: Arc<dyn CommandRunner>,
    pub book_ui: BookUi,
    pub gen: Option<BookGen>,
    pub auth: AuthUi,

    /// When set, save a screenshot here after a few frames and exit (verification mode).
    pub screenshot_path: Option<std::path::PathBuf>,
    frame_count: u64,

    // Whether a real embedded/managed session is running or we are idle.
    start_time: Instant,
}

impl App {
    pub fn new(
        ctx: &egui::Context,
        config: Config,
        dev_mode: bool,
        runner: Arc<dyn CommandRunner>,
    ) -> Self {
        theme::install_fonts(ctx);
        theme::apply_style(ctx, config.theme);

        let stats = paths::stats_path()
            .map(|p| Stats::load_from(&p))
            .unwrap_or_default();

        let store = BookStore::new(
            paths::books_dir().unwrap_or_else(|| std::path::PathBuf::from("books")),
        );

        // Stage the bundled plugin so --plugin-dir works from an installed binary.
        let plugin_root = paths::plugin_dir().unwrap_or_else(|| std::path::PathBuf::from("plugin"));
        let plugin_dir = crate::core::book::agent::stage_plugin(&plugin_root)
            .unwrap_or_else(|_| plugin_root.join("novelist"));
        let agent = AgentClient::new(runner.clone(), plugin_dir);

        let mut app = App {
            config,
            stats,
            screen: Screen::Typing,
            dev_mode,
            session: None,
            session_started: None,
            last_result: None,
            last_was_pb: false,
            paste_input: String::new(),
            flash: FlashState::default(),
            store,
            agent,
            runner,
            book_ui: BookUi::default(),
            gen: None,
            auth: AuthUi::default(),
            screenshot_path: None,
            frame_count: 0,
            start_time: Instant::now(),
        };
        // Kick off a background auth check so Book mode knows its state without blocking.
        app.refresh_auth();
        // Start an initial session for the default content mode (except Book, which needs
        // a book selected first).
        if app.config.content_mode != ContentMode::Book {
            app.start_session();
        }
        app
    }

    /// Run a Claude auth check on a background thread; result lands in `self.auth.check`.
    pub fn refresh_auth(&mut self) {
        let (tx, rx) = std::sync::mpsc::channel();
        self.auth.check_rx = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(crate::core::claude_auth::check_auth_blocking());
        });
    }

    /// Begin the PTY-driven Connect Claude flow.
    pub fn start_connect_flow(&mut self) {
        match ConnectFlow::start() {
            Ok(flow) => {
                self.auth.flow = Some(flow);
                self.auth.state = ConnectUiState::Starting;
                self.auth.code_input.clear();
            }
            Err(e) => {
                self.auth.state = ConnectUiState::Failed(format!(
                    "Could not start the sign-in flow: {e}"
                ));
            }
        }
    }

    /// Poll the background auth check and any live connect flow.
    fn poll_auth(&mut self) {
        if let Some(rx) = &self.auth.check_rx {
            if let Ok(check) = rx.try_recv() {
                tracing::info!("claude auth: {:?}", check);
                self.auth.check = Some(check);
                self.auth.check_rx = None;
            }
        }
        let mut finish_flow = false;
        if let Some(flow) = &self.auth.flow {
            while let Ok(ev) = flow.events.try_recv() {
                match ev {
                    ConnectEvent::Url(url) => {
                        self.auth.state = ConnectUiState::UrlShown {
                            url,
                            waiting_for_code: false,
                        };
                    }
                    ConnectEvent::WaitingForCode => {
                        if let ConnectUiState::UrlShown { url, .. } = &self.auth.state {
                            self.auth.state = ConnectUiState::UrlShown {
                                url: url.clone(),
                                waiting_for_code: true,
                            };
                        }
                    }
                    ConnectEvent::TokenStored => {
                        self.auth.check = Some(AuthCheck::ConnectedToken);
                        self.auth.state = ConnectUiState::Idle;
                        self.book_ui.status =
                            Some("Claude is connected. Book mode is ready.".into());
                        finish_flow = true;
                    }
                    ConnectEvent::Failed(msg) => {
                        self.auth.state = ConnectUiState::Failed(msg);
                        finish_flow = true;
                    }
                }
            }
        }
        if finish_flow {
            self.auth.flow = None;
        }
    }

    /// Seconds since the current session started (0 if none).
    fn session_secs(&self) -> f64 {
        self.session_started
            .map(|t| t.elapsed().as_secs_f64())
            .unwrap_or(0.0)
    }

    /// Public accessor for the keyboard flash clock.
    pub fn session_started_secs(&self) -> Option<f64> {
        self.session_started.map(|t| t.elapsed().as_secs_f64())
    }

    /// Build a text source for the current content mode and start a fresh session.
    pub fn start_session(&mut self) {
        let target = self.next_target_for_mode();
        if let Some(target) = target {
            let mut s = Session::new(target, self.config.error_mode);
            s.metrics.tick(0.0);
            self.session = Some(s);
            self.session_started = Some(Instant::now());
            self.screen = Screen::Typing;
        }
    }

    fn next_target_for_mode(&mut self) -> Option<Target> {
        match self.config.content_mode {
            ContentMode::Random => {
                let mut src = RandomSource::new(self.config.random_round_len);
                // Adaptive weighting toward weak keys from stored stats.
                if let Some(weights) = self.adaptive_weights(src.pool()) {
                    src.set_weights(weights);
                }
                Some(src.next_target())
            }
            ContentMode::Word => Some(WordSource::new().next_target()),
            ContentMode::Paste => {
                let text = self.paste_input.trim();
                if text.is_empty() {
                    None
                } else {
                    // Cap oversized pastes so the app stays responsive.
                    let capped: String = text.chars().take(20_000).collect();
                    Some(PasteSource::new(capped).next_target())
                }
            }
            ContentMode::Book => self.next_book_target(),
        }
    }

    /// Weak-key weighting: keys with lower accuracy or higher latency get up-weighted.
    fn adaptive_weights(&self, pool: &[Key]) -> Option<Vec<f32>> {
        // Aggregate per-key error rate across recent history.
        let mut acc: std::collections::HashMap<String, (u32, u32)> = std::collections::HashMap::new();
        for r in self.stats.history.iter().rev().take(20) {
            for (label, presses, errors, _lat) in &r.per_key {
                let e = acc.entry(label.clone()).or_insert((0, 0));
                e.0 += presses;
                e.1 += errors;
            }
        }
        if acc.is_empty() {
            return None;
        }
        let weights = pool
            .iter()
            .map(|k| {
                let label = keys::display_name(*k);
                match acc.get(&label) {
                    Some((p, e)) if *p > 0 => 1.0 + 4.0 * (*e as f32 / *p as f32),
                    _ => 1.0,
                }
            })
            .collect();
        Some(weights)
    }

    fn next_book_target(&mut self) -> Option<Target> {
        let slug = self.book_ui.open_slug.clone()?;
        let book = self.store.load(&slug).ok()?;
        // Serve the first not-yet-done chapter.
        let chapter = book.meta.chapters.iter().find(|c| !c.done)?;
        let md = book.read_chapter(chapter.n).ok()?;
        let plain = crate::core::book::export::markdown_to_plain(&md);
        self.book_ui.typing_chapter = Some(chapter.n);
        let title = format!(
            "{} — Chapter {}",
            crate::core::book::store::display_title(&book.meta),
            chapter.n
        );
        Some(BookSource::new(plain, title).next_target())
    }

    fn mode_label(&self) -> &'static str {
        match self.config.content_mode {
            ContentMode::Random => "random",
            ContentMode::Word => "word",
            ContentMode::Paste => "paste",
            ContentMode::Book => "book",
        }
    }

    /// Persist config to disk.
    pub fn save_config(&self) {
        if let Err(e) = self.config.save_to(&crate::core::config::config_path()) {
            tracing::warn!("failed to save config: {e}");
        }
    }

    fn save_stats(&self) {
        if let Some(p) = paths::stats_path() {
            if let Err(e) = self.stats.save_to(&p) {
                tracing::warn!("failed to save stats: {e}");
            }
        }
    }

    /// Handle keyboard input for the active typing session.
    fn handle_typing_input(&mut self, ctx: &egui::Context) {
        if self.session.is_none() || self.screen != Screen::Typing {
            return;
        }
        let now = self.session_secs();

        // Collect events first to avoid borrow conflicts.
        let events: Vec<Event> = ctx.input(|i| i.events.clone());
        let mut completed = false;

        for ev in events {
            match ev {
                Event::Key {
                    key,
                    physical_key,
                    pressed,
                    repeat,
                    ..
                } => {
                    if !pressed {
                        continue;
                    }
                    // Dev shortcuts.
                    if self.dev_mode {
                        if key == keys::DEV_AUTOTYPE || physical_key == Some(keys::DEV_AUTOTYPE) {
                            if let Some(s) = self.session.as_mut() {
                                if s.dev_autotype_next(now) == Progress::Complete {
                                    completed = true;
                                }
                            }
                            continue;
                        }
                        if !repeat
                            && (key == keys::DEV_COMPLETE_PAGE
                                || physical_key == Some(keys::DEV_COMPLETE_PAGE))
                        {
                            if let Some(s) = self.session.as_mut() {
                                s.dev_complete_page(now);
                                if s.is_complete() {
                                    completed = true;
                                }
                            }
                            continue;
                        }
                        if !repeat
                            && (key == keys::DEV_COMPLETE_CHAPTER
                                || physical_key == Some(keys::DEV_COMPLETE_CHAPTER))
                        {
                            if let Some(s) = self.session.as_mut() {
                                s.dev_complete_all(now);
                                completed = true;
                            }
                            continue;
                        }
                    }
                    if repeat {
                        continue;
                    }
                    // Feedback flash: record the just-pressed physical key.
                    if let Some(pk) = physical_key {
                        self.flash.press(pk, now);
                    }
                    // Non-character targets (Random mode arrows/function keys) and special
                    // keys (Enter/Tab, which produce no Text event) are matched by
                    // physical key. Space is NOT special: it produces a Text(" ") event,
                    // so handling it here too would double-count it.
                    if let Some(s) = self.session.as_mut() {
                        let expected_is_char = s.expected().map(|e| e.is_char()).unwrap_or(false);
                        let is_special = matches!(key, Key::Enter | Key::Tab);
                        if key == Key::Backspace {
                            s.backspace();
                        } else if !expected_is_char || is_special {
                            let pk = physical_key.unwrap_or(key);
                            if s.input_physical_key(pk, now) == Progress::Complete {
                                completed = true;
                            }
                        }
                        // Character targets are advanced by the Text event below.
                    }
                }
                Event::Text(t) => {
                    for c in t.chars() {
                        if let Some(s) = self.session.as_mut() {
                            let expects_char = s.expected().map(|e| e.is_char()).unwrap_or(false);
                            if expects_char && s.input_char(c, now) == Progress::Complete {
                                completed = true;
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        if let Some(s) = self.session.as_mut() {
            s.metrics.tick(now);
        }

        if completed {
            self.finish_session();
        }
    }

    fn finish_session(&mut self) {
        let Some(session) = self.session.take() else {
            return;
        };
        let result = SessionResult::from_metrics(&session.metrics, self.mode_label());
        tracing::info!(
            "session complete mode={} {}",
            self.mode_label(),
            session.metrics.summary_line()
        );
        let is_pb = self.stats.record(result.clone());
        self.save_stats();
        self.last_result = Some(result);
        self.last_was_pb = is_pb;

        // Book mode: mark the chapter typed and advance.
        if self.config.content_mode == ContentMode::Book {
            if let (Some(slug), Some(n)) =
                (self.book_ui.open_slug.clone(), self.book_ui.typing_chapter)
            {
                if let Ok(mut book) = self.store.load(&slug) {
                    let typed = book
                        .meta
                        .chapters
                        .iter()
                        .find(|c| c.n == n)
                        .map(|c| c.words)
                        .unwrap_or(0);
                    book.set_typed_progress(n, typed, true);
                }
            }
        }
        self.screen = Screen::Results;
    }

    /// Poll a running book generation for events.
    fn poll_gen(&mut self, ctx: &egui::Context) {
        let mut finished: Option<Result<crate::core::book::agent::GenDone, GenError>> = None;
        if let Some(gen) = self.gen.as_mut() {
            while let Ok(ev) = gen.rx.try_recv() {
                match ev {
                    AgentEvent::Delta(d) => gen.live_text.push_str(&d),
                    AgentEvent::Done(done) => {
                        finished = Some(Ok(*done));
                        break;
                    }
                    AgentEvent::Failed(e) => {
                        finished = Some(Err(e));
                        break;
                    }
                }
            }
            ctx.request_repaint();
        }

        if let Some(outcome) = finished {
            let gen = self.gen.take().unwrap();
            self.apply_generation(gen, outcome);
        }
    }

    fn apply_generation(
        &mut self,
        gen: BookGen,
        outcome: Result<crate::core::book::agent::GenDone, GenError>,
    ) {
        use crate::core::book::prompt::{parse_reply, ParsedReply};
        let Some(slug) = self.book_ui.open_slug.clone() else {
            return;
        };
        let Ok(mut book) = self.store.load(&slug) else {
            self.book_ui.status = Some("Could not load the book from disk.".into());
            return;
        };
        match outcome {
            Ok(done) => {
                if done.plugin_errors {
                    tracing::warn!("book gen reported plugin_errors");
                }
                tracing::info!(
                    "chapter generated book={} n={} plugins={:?}",
                    slug,
                    gen.n,
                    done.plugins
                );
                match parse_reply(&done.text) {
                    ParsedReply::Questions(q) => {
                        self.book_ui.pending_questions = Some(q);
                        self.book_ui.status =
                            Some("The author has a question before writing.".into());
                    }
                    ParsedReply::Chapter {
                        title,
                        prose,
                        bible,
                    } => {
                        self.write_generated_chapter(&mut book, gen.n, &title, &prose, &bible, done.session_id);
                    }
                    ParsedReply::Fallback(prose) => {
                        self.write_generated_chapter(
                            &mut book,
                            gen.n,
                            "",
                            &prose,
                            "",
                            done.session_id,
                        );
                    }
                }
            }
            Err(e) => {
                tracing::warn!("book gen failed: {e:?}");
                self.book_ui.status = Some(e.user_message());
                // Auth-shaped failures reopen the Connect Claude flow; chapters are safe
                // on disk, and retrying after connecting picks up where we left off.
                if matches!(e, GenError::LoggedOut | GenError::OrgNotAllowed) {
                    self.auth.check = Some(AuthCheck::NotConnected);
                    self.auth.state = ConnectUiState::Idle;
                    self.screen = Screen::Connect;
                }
            }
        }
    }

    fn write_generated_chapter(
        &mut self,
        book: &mut Book,
        n: usize,
        title: &str,
        prose: &str,
        bible: &str,
        session_id: Option<String>,
    ) {
        if prose.trim().is_empty() {
            self.book_ui.status = Some("The author returned an empty chapter. Try again.".into());
            return;
        }
        if book.meta.session_id.is_none() {
            book.meta.session_id = session_id;
        }
        if let Err(e) = book.write_chapter(n, title, prose, bible) {
            self.book_ui.status = Some(format!("Failed to save the chapter: {e}"));
            return;
        }
        self.book_ui.pending_questions = None;
        self.book_ui.continuation.clear();
        self.book_ui.status = Some(format!("Chapter {n} is ready. Type it to bind it in."));
    }

    /// Kick off a chapter generation (or the clarifying turn / rewrite) on a background
    /// thread. `allow_clarify` and blank-confirm logic is handled by the caller.
    pub fn start_generation(&mut self, n: usize, is_rewrite: bool, prompt: String) {
        let Some(slug) = self.book_ui.open_slug.clone() else {
            return;
        };
        let Ok(book) = self.store.load(&slug) else {
            self.book_ui.status = Some("Could not load the book.".into());
            return;
        };
        // Gate on the cached auth state: clear guidance instead of a doomed run. An
        // Unknown/None state proceeds; the run itself will classify any failure.
        match &self.auth.check {
            Some(AuthCheck::CliMissing) => {
                self.book_ui.status = Some(GenError::NotFound.user_message());
                return;
            }
            Some(AuthCheck::NotConnected) => {
                self.book_ui.status = Some(GenError::LoggedOut.user_message());
                self.screen = Screen::Connect;
                return;
            }
            _ => {}
        }
        let system_prompt = crate::core::book::prompt::system_prompt();
        let model = self.config.book_model.clone();
        let cwd = book.dir.clone();
        let resume = book.meta.session_id.clone();
        let (rx, cancel) = self.agent.generate(
            prompt,
            system_prompt,
            model,
            cwd,
            resume,
            is_rewrite, // fork the session for rewrites so the main thread is untouched
        );
        self.gen = Some(BookGen {
            rx,
            cancel,
            live_text: String::new(),
            n,
            is_rewrite,
            started: Instant::now(),
        });
        self.book_ui.status = Some("Writing...".into());
    }

    /// Cancel the in-flight generation, if any.
    pub fn cancel_generation(&mut self) {
        if let Some(gen) = &self.gen {
            gen.cancel.store(true, Ordering::SeqCst);
        }
    }

    pub fn palette(&self) -> theme::Palette {
        theme::Palette::for_theme(self.config.theme)
    }

    /// Verification mode: render a few frames, request a screenshot, save it, exit.
    fn handle_screenshot_mode(&mut self, ctx: &egui::Context) {
        let Some(path) = self.screenshot_path.clone() else {
            return;
        };
        self.frame_count += 1;
        ctx.request_repaint();
        if self.frame_count == 5 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        if self.frame_count > 300 {
            tracing::error!("screenshot never arrived; giving up");
            std::process::exit(1);
        }
        let shot: Option<std::sync::Arc<egui::ColorImage>> = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = shot {
            let [w, h] = img.size;
            let mut out = image::RgbaImage::new(w as u32, h as u32);
            for (i, px) in img.pixels.iter().enumerate() {
                let x = (i % w) as u32;
                let y = (i / w) as u32;
                out.put_pixel(x, y, image::Rgba(px.to_array()));
            }
            match out.save(&path) {
                Ok(()) => {
                    tracing::info!("screenshot saved to {}", path.display());
                    std::process::exit(0);
                }
                Err(e) => {
                    tracing::error!("screenshot save failed: {e}");
                    std::process::exit(1);
                }
            }
        }
    }
}

impl eframe::App for App {
    /// Non-UI work: poll generation, handle typing input, manage repaints.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let _ = self.start_time; // keep field alive for potential uptime logging

        self.handle_screenshot_mode(ctx);
        self.poll_auth();
        self.poll_gen(ctx);
        self.handle_typing_input(ctx);

        // Prune old flashes and request repaints while animating.
        let now = self.session_secs();
        self.flash.prune(now, 1.0);
        if !self.config.reduced_motion && self.flash.is_animating(now, 0.5) {
            ctx.request_repaint();
        }
        if self.session.is_some() && self.screen == Screen::Typing {
            // Keep the clock and consistency samples ticking.
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }

    /// Rendering: the top bar and the current screen.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        top_bar(self, ui);

        egui::CentralPanel::default().show(ui, |ui| match self.screen {
            Screen::Typing => stage::show(self, ui),
            Screen::Results => results::show(self, ui),
            Screen::Books => books::show(self, ui),
            Screen::Settings => settings::show(self, ui),
            Screen::Connect => connect::show(self, ui),
        });
    }
}

/// The slim top app bar: wordmark, content-mode tabs, keyboard-mode switch, nav, DEV badge.
fn top_bar(app: &mut App, ui: &mut egui::Ui) {
    let p = app.palette();
    egui::Panel::top("top_bar").show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Bookley")
                    .color(p.brass)
                    .size(20.0)
                    .strong(),
            );
            ui.label(
                egui::RichText::new("Key Trainer")
                    .color(p.ghost)
                    .size(13.0),
            );
            if app.dev_mode {
                ui.label(
                    egui::RichText::new(" DEV ")
                        .color(p.ink_950)
                        .background_color(p.ribbon)
                        .strong(),
                );
            }
            ui.add_space(12.0);

            // Content-mode tabs.
            for mode in ContentMode::ALL {
                let selected = app.config.content_mode == mode;
                let mut text = egui::RichText::new(mode.label());
                if selected {
                    text = text.color(p.verdigris).strong();
                }
                if ui.selectable_label(selected, text).clicked() && !selected {
                    app.config.content_mode = mode;
                    app.save_config();
                    if mode == ContentMode::Book {
                        app.screen = Screen::Books;
                    } else if mode == ContentMode::Paste {
                        // Paste needs input first; show the stage which prompts for it.
                        app.session = None;
                        app.screen = Screen::Typing;
                    } else {
                        app.start_session();
                    }
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Settings").clicked() {
                    app.screen = Screen::Settings;
                }
                if ui.button("Books").clicked() {
                    app.screen = Screen::Books;
                }
                // Keyboard-mode switch.
                for km in KeyboardMode::ALL.iter().rev() {
                    let selected = app.config.keyboard_mode == *km;
                    let mut text = egui::RichText::new(km.label());
                    if selected {
                        text = text.color(p.verdigris).strong();
                    }
                    if ui.selectable_label(selected, text).clicked() {
                        app.config.keyboard_mode = *km;
                        app.save_config();
                    }
                }
                ui.label(egui::RichText::new("Keyboard:").color(p.ghost).size(12.0));
            });
        });
        ui.add_space(2.0);
    });
}
