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
use crate::core::session::{PauseClock, Progress, Session};
use crate::core::stats_store::Stats;
use crate::core::text_source::{PasteSource, RandomSource, Target, TextSource, WordSource};
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
    /// This generation was asked to conclude the book.
    pub conclude: bool,
    pub started: Instant,
}

/// State of a cover-design request in flight.
pub struct CoverGen {
    pub rx: Receiver<CoverEvent>,
    pub cancel: Arc<AtomicBool>,
    pub started: Instant,
    /// The book the cover is for.
    pub slug: String,
}

/// Outcome of a background cover run.
pub enum CoverEvent {
    Done { png: Vec<u8>, used_fallback: bool },
    Failed(GenError),
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
    /// "Make this chapter the last chapter of the book."
    pub make_last: bool,
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
    /// Press-Space-to-start gate: the session exists but the clock has not started.
    /// The gate press itself is consumed, never fed to the session.
    pub awaiting_start: bool,
    /// Pause bookkeeping: paused time never counts toward metrics.
    pub pause: PauseClock,
    pub last_result: Option<SessionResult>,
    pub last_was_pb: bool,
    /// When the results screen was entered (guards the Enter shortcut).
    pub results_at: Option<Instant>,

    // Text sources per mode (streaming drills refill from these).
    pub paste_input: String,
    word_src: WordSource,
    random_src: Option<RandomSource>,

    // Keyboard flash animation state.
    pub flash: FlashState,

    // Book mode.
    pub store: BookStore,
    pub agent: AgentClient,
    pub runner: Arc<dyn CommandRunner>,
    pub book_ui: BookUi,
    pub gen: Option<BookGen>,
    pub auth: AuthUi,
    /// A cover-design run in flight, if any.
    pub cover_gen: Option<CoverGen>,
    /// Cached cover texture keyed by slug + file mtime (reloaded when either changes).
    pub cover_tex: Option<(String, egui::TextureHandle)>,

    /// When the active session types a book chapter: (slug, chapter n). Used to persist
    /// typing progress continuously and crash-safely, whatever mode the UI is in now.
    session_book: Option<(String, usize)>,
    /// Offset of the session target within the full normalized chapter (resume rewind).
    book_resume_offset: usize,
    /// Full normalized chapter length in chars (whole-chapter progress display).
    book_chapter_len: usize,
    /// Last position written to disk (skip redundant writes).
    progress_saved_pos: usize,
    /// Throttle for the periodic progress save.
    last_progress_save: Instant,

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

        let store =
            BookStore::new(paths::books_dir().unwrap_or_else(|| std::path::PathBuf::from("books")));

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
            awaiting_start: false,
            pause: PauseClock::default(),
            last_result: None,
            last_was_pb: false,
            results_at: None,
            paste_input: String::new(),
            word_src: WordSource::new(),
            random_src: None,
            flash: FlashState::default(),
            store,
            agent,
            runner,
            book_ui: BookUi::default(),
            gen: None,
            auth: AuthUi::default(),
            cover_gen: None,
            cover_tex: None,
            session_book: None,
            book_resume_offset: 0,
            book_chapter_len: 0,
            progress_saved_pos: 0,
            last_progress_save: Instant::now(),
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
                self.auth.state =
                    ConnectUiState::Failed(format!("Could not start the sign-in flow: {e}"));
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

    /// Raw wall seconds since the current session started (0 if none).
    fn raw_secs(&self) -> f64 {
        self.session_started
            .map(|t| t.elapsed().as_secs_f64())
            .unwrap_or(0.0)
    }

    /// Active (unpaused) seconds of the current session; this is the metrics clock.
    pub fn session_secs(&self) -> f64 {
        self.pause.active_secs(self.raw_secs())
    }

    /// Public accessor for the keyboard flash clock.
    pub fn session_started_secs(&self) -> Option<f64> {
        self.session_started.map(|_| self.session_secs())
    }

    pub fn is_paused(&self) -> bool {
        self.pause.is_paused()
    }

    /// Pause or resume the active drill. Paused time never counts toward metrics.
    pub fn toggle_pause(&mut self) {
        if self.session.is_none() || self.awaiting_start {
            return;
        }
        let raw = self.raw_secs();
        if self.pause.is_paused() {
            self.pause.resume(raw);
        } else {
            self.pause.pause(raw);
        }
    }

    /// Progress through the current target, 0..1. For a resumed book chapter this is the
    /// whole-chapter progress (the rewound offset counts), not just the served remainder.
    pub fn session_progress_fraction(&self) -> f32 {
        let Some(s) = &self.session else {
            return 0.0;
        };
        if self.session_book.is_some() && self.book_chapter_len > 0 {
            ((self.book_resume_offset + s.cursor) as f32 / self.book_chapter_len as f32)
                .clamp(0.0, 1.0)
        } else {
            s.progress_fraction()
        }
    }

    /// Dismiss the press-Space-to-start gate: the clock starts now.
    pub fn begin_after_gate(&mut self) {
        if self.session.is_none() || !self.awaiting_start {
            return;
        }
        self.awaiting_start = false;
        self.session_started = Some(Instant::now());
        self.pause = PauseClock::default();
    }

    /// Zero the timer and every metric WITHOUT losing the typing position (Paste and
    /// Book modes: a mid-chapter distraction ruins the stats, not the progress). The
    /// press-Space gate re-arms so the fresh clock starts when the user is ready; any
    /// active pause is cleared.
    pub fn reset_session_stats(&mut self) {
        let Some(s) = self.session.as_mut() else {
            return;
        };
        s.reset_metrics();
        self.pause = PauseClock::default();
        self.session_started = None;
        self.awaiting_start = true;
    }

    /// Build a text source for the current content mode and start a fresh session.
    /// Word and Random are timed streaming drills; Paste and Book run to completion.
    pub fn start_session(&mut self) {
        // If a book chapter was in progress, persist its position before replacing the
        // session (mode switches must never lose progress).
        self.save_book_progress();
        self.session_book = None;
        let drill = self.config.drill_secs as f64;
        let error_mode = self.config.error_mode;
        let session = match self.config.content_mode {
            ContentMode::Random => {
                let mut src = RandomSource::new(120);
                if let Some(weights) = self.adaptive_weights(src.pool()) {
                    src.set_weights(weights);
                }
                let target = src.next_target();
                self.random_src = Some(src);
                Some(Session::with_time_limit(target, error_mode, drill))
            }
            ContentMode::Word => {
                let target = self.word_src.stream_target(120);
                Some(Session::with_time_limit(target, error_mode, drill))
            }
            ContentMode::Paste => {
                let text = self.paste_input.trim();
                if text.is_empty() {
                    None
                } else {
                    // Cap oversized pastes so the app stays responsive.
                    let capped: String = text.chars().take(20_000).collect();
                    Some(Session::new(
                        PasteSource::new(capped).next_target(),
                        error_mode,
                    ))
                }
            }
            ContentMode::Book => self.next_book_target().map(|t| Session::new(t, error_mode)),
        };
        if let Some(mut s) = session {
            s.metrics.tick(0.0);
            self.session = Some(s);
            // Every mode starts behind the press-Space gate: the clock starts on Space,
            // and that press is never counted as typing input.
            self.session_started = None;
            self.awaiting_start = true;
            self.pause = PauseClock::default();
            self.screen = Screen::Typing;
        }
    }

    /// Timed drills stream: refill the target near the end, and stop when time is up.
    fn tick_timed_drill(&mut self) {
        let now = self.session_secs();
        let mut time_up = false;
        if let Some(s) = self.session.as_mut() {
            if s.time_limit_secs.is_some() {
                if s.items_remaining() < 60 {
                    let extra = match self.config.content_mode {
                        ContentMode::Word => self.word_src.batch(60),
                        ContentMode::Random => self
                            .random_src
                            .as_ref()
                            .map(|src| {
                                src.batch(60)
                                    .into_iter()
                                    .map(crate::core::text_source::Expected::PhysicalKey)
                                    .collect()
                            })
                            .unwrap_or_default(),
                        _ => Vec::new(),
                    };
                    s.extend_target(extra);
                }
                if s.time_up(now) && !self.pause.is_paused() {
                    time_up = true;
                }
            }
        }
        if time_up {
            self.finish_session();
        }
    }

    /// Weak-key weighting: keys with lower accuracy or higher latency get up-weighted.
    fn adaptive_weights(&self, pool: &[Key]) -> Option<Vec<f32>> {
        // Aggregate per-key error rate across recent history.
        let mut acc: std::collections::HashMap<String, (u32, u32)> =
            std::collections::HashMap::new();
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
        // The persisted position indexes the NORMALIZED target (what the session
        // consumes). Resume rewound to the previous paragraph boundary as a refresher.
        let full = crate::core::normalize::normalize_target(&plain);
        let resume = crate::core::book::store::resume_position(&full, chapter.typed_chars);
        let rest: String = full.chars().skip(resume).collect();
        self.book_resume_offset = resume;
        self.book_chapter_len = full.chars().count();
        self.progress_saved_pos = chapter.typed_chars;
        self.session_book = Some((slug.clone(), chapter.n));
        self.book_ui.typing_chapter = Some(chapter.n);
        if resume > 0 {
            tracing::info!(
                "resuming book={} chapter={} at {} of {} (rewound from {})",
                slug,
                chapter.n,
                resume,
                self.book_chapter_len,
                chapter.typed_chars
            );
        }
        let title = format!(
            "{} — Chapter {}",
            crate::core::book::store::display_title(&book.meta),
            chapter.n
        );
        Some(Target::from_text(&rest, title))
    }

    /// Persist the current book-chapter typing position (crash-safe resume). Progress is
    /// monotonic: rewound refresher typing never regresses the saved position, and a
    /// chapter already marked done is left alone.
    pub fn save_book_progress(&mut self) {
        let Some((slug, n)) = self.session_book.clone() else {
            return;
        };
        let Some(s) = &self.session else {
            return;
        };
        if s.is_complete() {
            return; // completion goes through finish_session, which marks it done
        }
        let pos = self.book_resume_offset + s.cursor;
        if pos <= self.progress_saved_pos {
            return;
        }
        if let Ok(mut book) = self.store.load(&slug) {
            let done_already = book.meta.chapters.iter().any(|c| c.n == n && c.done);
            let on_disk = book
                .meta
                .chapters
                .iter()
                .find(|c| c.n == n)
                .map(|c| c.typed_chars)
                .unwrap_or(0);
            if !done_already && pos > on_disk {
                book.set_typed_progress(n, pos, false);
            }
            self.progress_saved_pos = pos;
        }
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
        // Collect events first to avoid borrow conflicts.
        let events: Vec<Event> = ctx.input(|i| i.events.clone());

        // Paused: Space resumes (besides the Resume button); nothing else is fed.
        if self.is_paused() {
            if has_space_press(&events) {
                self.toggle_pause();
            }
            return;
        }
        // The press-Space-to-start gate: Space starts the clock; the gate press itself
        // is consumed here and never counted as typing input.
        if self.awaiting_start {
            if has_space_press(&events) {
                self.begin_after_gate();
            }
            return;
        }
        let now = self.session_secs();
        // Random mode drills Backspace as an ordinary key; the other modes use it to
        // correct mistakes.
        let backspace_is_input = self.config.content_mode == ContentMode::Random;
        let cursor_before = self.session.as_ref().map(|s| s.cursor).unwrap_or(0);
        let completed = match self.session.as_mut() {
            Some(s) => feed_session_events(
                s,
                &mut self.flash,
                &events,
                now,
                self.dev_mode,
                backspace_is_input,
            ),
            None => false,
        };

        if let Some(s) = self.session.as_mut() {
            s.metrics.tick(now);
        }

        if completed {
            self.finish_session();
        } else if self.session_book.is_some() {
            // Book chapters: persist the position whenever a paragraph boundary was
            // crossed this frame (plus the periodic save in `logic`).
            let crossed_paragraph = self
                .session
                .as_ref()
                .map(|s| {
                    (cursor_before..s.cursor).any(|i| {
                        matches!(
                            s.target.items.get(i),
                            Some(crate::core::text_source::Expected::Char('\n'))
                        )
                    })
                })
                .unwrap_or(false);
            if crossed_paragraph {
                self.save_book_progress();
            }
        }
    }

    fn finish_session(&mut self) {
        let Some(mut session) = self.session.take() else {
            return;
        };
        // Make sure the clock reflects the final active time (timed drills can end
        // between keystrokes).
        session
            .metrics
            .tick(self.pause.active_secs(self.raw_secs()));
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

        // Book chapters: mark the chapter fully typed.
        if let Some((slug, n)) = self.session_book.take() {
            if let Ok(mut book) = self.store.load(&slug) {
                book.set_typed_progress(n, self.book_chapter_len, true);
            }
        }
        self.pause = PauseClock::default();
        self.results_at = Some(Instant::now());
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
                        self.write_generated_chapter(
                            &mut book,
                            &gen,
                            &title,
                            &prose,
                            &bible,
                            done.session_id,
                        );
                    }
                    ParsedReply::Fallback(prose) => {
                        self.write_generated_chapter(
                            &mut book,
                            &gen,
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
        gen: &BookGen,
        title: &str,
        prose: &str,
        bible: &str,
        session_id: Option<String>,
    ) {
        let n = gen.n;
        if prose.trim().is_empty() {
            self.book_ui.status = Some("The author returned an empty chapter. Try again.".into());
            return;
        }
        if book.meta.session_id.is_none() {
            book.meta.session_id = session_id;
        }
        // 100%-AI books: adopt the title the author invented (BOOK-TITLE in the bible).
        if book.meta.title.trim().is_empty() {
            if let Some(t) = crate::core::book::prompt::book_title_from_bible(bible) {
                book.meta.title = t;
            }
        }
        // A generation that was asked to conclude the book marks it finished.
        if gen.conclude && !gen.is_rewrite {
            book.meta.concluded = true;
        }
        if let Err(e) = book.write_chapter(n, title, prose, bible) {
            self.book_ui.status = Some(format!("Failed to save the chapter: {e}"));
            return;
        }
        self.book_ui.pending_questions = None;
        self.book_ui.continuation.clear();
        self.book_ui.make_last = false;
        self.book_ui.status = Some(if book.meta.concluded {
            format!("Chapter {n} is ready, and it ends the book. Type it to bind it in.")
        } else {
            format!("Chapter {n} is ready. Type it to bind it in.")
        });
    }

    /// Kick off a chapter generation (or the clarifying turn / rewrite) on a background
    /// thread. `allow_clarify` and blank-confirm logic is handled by the caller;
    /// `conclude` marks this as the book's final chapter.
    pub fn start_generation(&mut self, n: usize, is_rewrite: bool, conclude: bool, prompt: String) {
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
            conclude,
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

    /// Kick off a cover design for the open book on a background thread: same claude
    /// plumbing, auth gating, and error handling as chapter generation.
    pub fn start_cover_generation(&mut self) {
        if self.cover_gen.is_some() {
            return;
        }
        let Some(slug) = self.book_ui.open_slug.clone() else {
            return;
        };
        let Ok(book) = self.store.load(&slug) else {
            self.book_ui.status = Some("Could not load the book.".into());
            return;
        };
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
        let runner = self.runner.clone();
        let model = self.config.book_model.clone();
        let plugin_dir = self.agent.plugin_dir.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel2 = cancel.clone();
        std::thread::spawn(move || {
            let res = crate::core::book::cover::generate_cover_blocking(
                &*runner, &book, &model, plugin_dir, 300, &cancel2,
            );
            let _ = tx.send(match res {
                Ok(o) => CoverEvent::Done {
                    png: o.png,
                    used_fallback: o.used_fallback,
                },
                Err(e) => CoverEvent::Failed(e),
            });
        });
        self.cover_gen = Some(CoverGen {
            rx,
            cancel,
            started: Instant::now(),
            slug,
        });
        self.book_ui.status = Some("Designing a cover...".into());
    }

    /// Cancel the in-flight cover design, if any.
    pub fn cancel_cover(&mut self) {
        if let Some(cg) = &self.cover_gen {
            cg.cancel.store(true, Ordering::SeqCst);
        }
    }

    /// Poll a running cover design; store the PNG with the book when it lands.
    fn poll_cover(&mut self, ctx: &egui::Context) {
        let Some(cg) = &self.cover_gen else {
            return;
        };
        let ev = match cg.rx.try_recv() {
            Ok(ev) => ev,
            Err(_) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
                return;
            }
        };
        let slug = cg.slug.clone();
        self.cover_gen = None;
        match ev {
            CoverEvent::Done { png, used_fallback } => match self.store.load(&slug) {
                Ok(book) => match std::fs::write(book.cover_path(), &png) {
                    Ok(()) => {
                        tracing::info!(
                            "cover saved book={} bytes={} fallback={}",
                            slug,
                            png.len(),
                            used_fallback
                        );
                        self.cover_tex = None; // force a texture reload
                        self.book_ui.status = Some(if used_fallback {
                            "Claude's design could not be rendered, so a clean typographic \
cover was generated instead. Regenerate to try again."
                                .into()
                        } else {
                            "Cover ready: it is page one of the PDF export. Regenerating \
replaces it."
                                .into()
                        });
                    }
                    Err(e) => {
                        self.book_ui.status = Some(format!("Could not save the cover: {e}"));
                    }
                },
                Err(e) => {
                    self.book_ui.status = Some(format!("Could not load the book: {e}"));
                }
            },
            CoverEvent::Failed(e) => {
                tracing::warn!("cover generation failed: {e:?}");
                self.book_ui.status = Some(e.user_message());
                if matches!(e, GenError::LoggedOut | GenError::OrgNotAllowed) {
                    self.auth.check = Some(AuthCheck::NotConnected);
                    self.auth.state = ConnectUiState::Idle;
                    self.screen = Screen::Connect;
                }
            }
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
        self.poll_cover(ctx);
        self.handle_typing_input(ctx);
        self.tick_timed_drill();

        // Crash-safe book progress: periodic save on top of the per-paragraph saves.
        if self.session_book.is_some() && self.last_progress_save.elapsed().as_secs() >= 3 {
            self.save_book_progress();
            self.last_progress_save = Instant::now();
        }

        // Prune old flashes and request repaints while animating.
        let now = self.session_secs();
        self.flash.prune(now, 1.0);
        if !self.config.reduced_motion && self.flash.is_animating(now, 0.5) {
            ctx.request_repaint();
        }
        if self.session.is_some() && self.screen == Screen::Typing && !self.is_paused() {
            // Keep the clock, drill timer, and consistency samples ticking.
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }

    /// Shutdown: persist any in-flight book-chapter typing position.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_book_progress();
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

/// Whether any event in the batch is a fresh (non-repeat) Space press.
fn has_space_press(events: &[Event]) -> bool {
    events.iter().any(|e| {
        matches!(
            e,
            Event::Key {
                key: Key::Space,
                pressed: true,
                repeat: false,
                ..
            }
        )
    })
}

/// Feed one frame's worth of keyboard events into the session. Pure with respect to the
/// app (only the session and the flash animation state are touched), so the exact
/// counting rules are unit-testable.
///
/// Counting rules:
/// - Exactly one keystroke per physical press. Key auto-repeat is ignored: repeated `Key`
///   events are dropped, and so is the `Text` event each of them produces (egui emits the
///   `Key` event first, then its `Text`, so a repeat flags the next `Text` for skipping).
/// - Character targets consume `Text` events; physical-key targets and the keys that
///   produce no text (Enter/Tab) consume `Key` events. Space produces a `Text(" ")`, so
///   it is never matched on the `Key` path (that would double-count it).
/// - `backspace_is_input` (Random mode): Backspace is an ordinary drillable key. In every
///   other mode it steps the cursor back to correct mistakes.
///
/// Returns whether the target was completed.
pub(crate) fn feed_session_events(
    session: &mut Session,
    flash: &mut FlashState,
    events: &[Event],
    now: f64,
    dev_mode: bool,
    backspace_is_input: bool,
) -> bool {
    let mut completed = false;
    // Set when the last Key event was an auto-repeat; the Text event it produced (which
    // immediately follows it in the same frame) is skipped and the flag cleared.
    let mut suppress_repeat_text = false;

    for ev in events {
        match ev {
            Event::Key {
                key,
                physical_key,
                pressed,
                repeat,
                modifiers,
                ..
            } => {
                let (key, physical_key) = (*key, *physical_key);
                if !pressed {
                    continue;
                }
                // Feedback flash for the Shift caps (egui reports the modifier state,
                // not the modifier keypress itself).
                if modifiers.shift {
                    flash.press_shift(now);
                }
                // Dev shortcuts.
                if dev_mode {
                    if key == keys::DEV_AUTOTYPE || physical_key == Some(keys::DEV_AUTOTYPE) {
                        if session.dev_autotype_next(now) == Progress::Complete {
                            completed = true;
                        }
                        continue;
                    }
                    if !repeat
                        && (key == keys::DEV_COMPLETE_PAGE
                            || physical_key == Some(keys::DEV_COMPLETE_PAGE))
                    {
                        session.dev_complete_page(now);
                        if session.is_complete() {
                            completed = true;
                        }
                        continue;
                    }
                    if !repeat
                        && (key == keys::DEV_COMPLETE_CHAPTER
                            || physical_key == Some(keys::DEV_COMPLETE_CHAPTER))
                    {
                        session.dev_complete_all(now);
                        completed = true;
                        continue;
                    }
                }
                suppress_repeat_text = *repeat;
                if *repeat {
                    continue;
                }
                // Feedback flash: record the just-pressed physical key.
                if let Some(pk) = physical_key {
                    flash.press(pk, now);
                }
                // Non-character targets (Random mode arrows/function keys) and special
                // keys (Enter/Tab, which produce no Text event) are matched by
                // physical key. Space is NOT special: it produces a Text(" ") event,
                // so handling it here too would double-count it.
                let expected_is_char = session.expected().map(|e| e.is_char()).unwrap_or(false);
                let is_special = matches!(key, Key::Enter | Key::Tab);
                if key == Key::Backspace && !backspace_is_input {
                    session.backspace();
                } else if !expected_is_char || is_special {
                    let pk = physical_key.unwrap_or(key);
                    if session.input_physical_key(pk, now) == Progress::Complete {
                        completed = true;
                    }
                }
                // Character targets are advanced by the Text event below.
            }
            Event::Text(t) => {
                if suppress_repeat_text {
                    suppress_repeat_text = false;
                    continue;
                }
                for c in t.chars() {
                    let expects_char = session.expected().map(|e| e.is_char()).unwrap_or(false);
                    if expects_char && session.input_char(c, now) == Progress::Complete {
                        completed = true;
                    }
                }
            }
            _ => {}
        }
    }
    completed
}

/// The top app bar: serif wordmark lockup, segmented content-mode tabs, and a tidy right
/// cluster (keyboard mode, Books, Settings).
fn top_bar(app: &mut App, ui: &mut egui::Ui) {
    let p = app.palette();
    egui::Panel::top("top_bar")
        .frame(
            egui::Frame::new()
                .fill(p.ink_850)
                .inner_margin(egui::Margin::symmetric(16, 10)),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Wordmark lockup: literary serif + quiet descriptor.
                ui.label(
                    egui::RichText::new("Bookley")
                        .font(theme::display_font(24.0))
                        .color(p.brass),
                );
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new("KEY TRAINER")
                        .color(p.ghost)
                        .size(10.5)
                        .extra_letter_spacing(1.6),
                );
                if app.dev_mode {
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(" DEV ")
                            .color(p.ink_850)
                            .background_color(p.ribbon)
                            .size(11.0)
                            .strong(),
                    );
                }
                ui.add_space(22.0);

                // Content-mode tabs.
                for mode in ContentMode::ALL {
                    let selected = app.config.content_mode == mode;
                    let text = if selected {
                        egui::RichText::new(mode.label())
                            .color(p.verdigris)
                            .strong()
                    } else {
                        egui::RichText::new(mode.label()).color(p.paper)
                    };
                    if ui.selectable_label(selected, text).clicked() && !selected {
                        app.config.content_mode = mode;
                        app.save_config();
                        if mode == ContentMode::Book {
                            app.screen = Screen::Books;
                        } else if mode == ContentMode::Paste {
                            // Paste needs input first; show the stage which prompts for
                            // it. Never lose an in-flight book chapter's position.
                            app.save_book_progress();
                            app.session = None;
                            app.session_book = None;
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
                    ui.add_space(10.0);
                    // Keyboard-mode switch.
                    for km in KeyboardMode::ALL.iter().rev() {
                        let selected = app.config.keyboard_mode == *km;
                        let text = if selected {
                            egui::RichText::new(km.label()).color(p.verdigris).strong()
                        } else {
                            egui::RichText::new(km.label()).color(p.paper)
                        };
                        if ui.selectable_label(selected, text).clicked() {
                            app.config.keyboard_mode = *km;
                            app.save_config();
                        }
                    }
                    ui.label(
                        egui::RichText::new("KEYBOARD")
                            .color(p.ghost)
                            .size(10.0)
                            .extra_letter_spacing(1.2),
                    );
                });
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::ErrorMode;
    use crate::core::text_source::Target;

    fn key_event(key: Key, repeat: bool) -> Event {
        Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat,
            modifiers: egui::Modifiers::default(),
        }
    }

    fn text_event(s: &str) -> Event {
        Event::Text(s.to_string())
    }

    fn feed(session: &mut Session, events: &[Event], backspace_is_input: bool) -> bool {
        let mut flash = FlashState::default();
        feed_session_events(session, &mut flash, events, 1.0, false, backspace_is_input)
    }

    /// One physical press produces a Key event AND a Text event; exactly one keystroke
    /// must be counted, attributed to the expected key.
    #[test]
    fn one_press_counts_exactly_once() {
        let mut s = Session::new(Target::from_text("ab", "t"), ErrorMode::Off);
        feed(&mut s, &[key_event(Key::X, false), text_event("x")], false);
        assert_eq!(s.metrics.total_keystrokes, 1, "no double counting");
        assert_eq!(s.metrics.error_keystrokes, 1);
        // Attributed to the expected key (A), not the pressed one (X).
        assert_eq!(s.metrics.per_key[&Key::A].errors, 1);
        assert!(!s.metrics.per_key.contains_key(&Key::X));
    }

    /// Key auto-repeat (held key) must not spam keystrokes: repeated Key events are
    /// dropped and so are the Text events they produce.
    #[test]
    fn key_repeat_is_not_counted() {
        let mut s = Session::new(Target::from_text("aaa", "t"), ErrorMode::Off);
        let events = vec![
            key_event(Key::A, false),
            text_event("a"),
            key_event(Key::A, true),
            text_event("a"),
            key_event(Key::A, true),
            text_event("a"),
        ];
        feed(&mut s, &events, false);
        assert_eq!(s.metrics.total_keystrokes, 1, "repeats must be ignored");
        assert_eq!(s.cursor, 1);
    }

    /// In Random mode Backspace is a drillable target: pressing it completes the item
    /// instead of stepping the cursor back.
    #[test]
    fn backspace_is_a_target_in_random_mode() {
        let mut s = Session::new(
            Target::from_keys(vec![Key::Backspace, Key::F1], "r"),
            ErrorMode::Letter,
        );
        let done = feed(&mut s, &[key_event(Key::Backspace, false)], true);
        assert!(!done);
        assert_eq!(s.cursor, 1, "Backspace completed its own item");
        assert_eq!(s.metrics.correct_chars, 1);
        assert_eq!(s.metrics.error_keystrokes, 0);
        assert!(feed(&mut s, &[key_event(Key::F1, false)], true));
    }

    /// Outside Random mode Backspace still corrects: it steps back without counting a
    /// keystroke.
    #[test]
    fn backspace_corrects_in_char_modes() {
        let mut s = Session::new(Target::from_text("ab", "t"), ErrorMode::Off);
        feed(&mut s, &[key_event(Key::X, false), text_event("x")], false);
        assert_eq!(s.cursor, 1);
        feed(&mut s, &[key_event(Key::Backspace, false)], false);
        assert_eq!(s.cursor, 0, "backspace stepped back");
        // The original error stays counted; backspace itself is not a keystroke.
        assert_eq!(s.metrics.total_keystrokes, 1);
    }
}
