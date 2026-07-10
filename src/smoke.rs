//! `--smoke`: a scriptable end-to-end self-test that exercises the real core pipeline
//! headlessly and exits non-zero on any failure. The book path runs against a fake
//! `claude` (BOOKLEY_CLAUDE_BIN) so it never consumes subscription usage or needs auth;
//! `--smoke --live` opts into one real generation for a human-triggered check.
//!
//! Output lines are stable and assertable ("smoke: ... ok"), ending in "SMOKE OK".

use std::sync::atomic::AtomicBool;

use crate::core::book::agent::{ClaudeRunner, CommandRunner, GenError, GenRequest};
use crate::core::book::prompt::{self, ParsedReply};
use crate::core::book::store::BookStore;
use crate::core::config::ErrorMode;
use crate::core::session::Session;
use crate::core::text_source::{PasteSource, RandomSource, TextSource, WordSource};

macro_rules! step {
    ($name:expr, $body:expr) => {{
        match $body {
            Ok(()) => println!("smoke: {} ok", $name),
            Err(e) => {
                println!("smoke: {} FAILED: {}", $name, e);
                return Err(format!("{} failed: {}", $name, e));
            }
        }
    }};
}

/// Run the smoke suite. `live` swaps the fake `claude` for the real one (one tiny
/// generation; requires a connected Claude and consumes subscription usage).
pub fn run(live: bool) -> Result<(), String> {
    // Isolate all app state in a throwaway data dir unless the caller pinned one.
    if std::env::var("BOOKLEY_DATA_DIR").is_err() {
        let dir = std::env::temp_dir().join(format!("bookley-smoke-{}", std::process::id()));
        std::env::set_var("BOOKLEY_DATA_DIR", &dir);
    }
    let data = std::env::var("BOOKLEY_DATA_DIR").unwrap();
    std::fs::create_dir_all(&data).map_err(|e| format!("create data dir: {e}"))?;
    if !live {
        // Default the fake claude if the caller did not point elsewhere.
        if std::env::var("BOOKLEY_CLAUDE_BIN").is_err() {
            let fake = std::path::Path::new("tests/fake_claude.sh");
            let fake = fake
                .canonicalize()
                .map_err(|e| format!("fake claude not found at tests/fake_claude.sh: {e}"))?;
            std::env::set_var("BOOKLEY_CLAUDE_BIN", &fake);
        }
    }

    step!("typing session (random keys)", typing_random());
    step!("typing session (word)", typing_word());
    step!("typing session (paste)", typing_paste());

    if live {
        step!("book cycle (LIVE claude)", book_cycle(true));
    } else {
        step!("book cycle (fake claude)", book_cycle(false));
        step!("error classification", error_classification());
        step!("generation timeout kills the child", timeout_kills());
        step!("connect flow (fake pty)", connect_flow());
    }

    println!("SMOKE OK");
    Ok(())
}

fn typing_random() -> Result<(), String> {
    let mut src = RandomSource::new(40);
    let target = src.next_target();
    // Pool honored and dev keys excluded is asserted by unit tests; here we run the
    // real session pipeline via the dev auto-type path.
    let mut s = Session::new(target, ErrorMode::Letter);
    let mut t = 0.0;
    while !s.is_complete() {
        t += 0.05;
        s.dev_autotype_next(t);
    }
    let m = &s.metrics;
    if m.correct_chars != 40 || m.error_keystrokes != 0 {
        return Err(format!("unexpected metrics: {}", m.summary_line()));
    }
    if m.wpm() <= 0.0 || m.accuracy() != 1.0 {
        return Err(format!("bad wpm/accuracy: {}", m.summary_line()));
    }
    println!("smoke: session complete {}", m.summary_line());
    Ok(())
}

fn typing_word() -> Result<(), String> {
    let mut src = WordSource::new();
    let target = src.next_target();
    let n = target.len();
    let mut s = Session::new(target, ErrorMode::Off);
    s.dev_complete_all(2.0);
    if !s.is_complete() || s.metrics.correct_chars as usize != n {
        return Err(format!("word session incomplete: {}", s.summary()));
    }
    Ok(())
}

fn typing_paste() -> Result<(), String> {
    let text = "The quick brown fox jumps over the lazy dog, 42 times!";
    let mut src = PasteSource::new(text.to_string());
    let target = src.next_target();
    if target.visible_text() != text {
        return Err("paste target does not reproduce input".into());
    }
    let mut s = Session::new(target, ErrorMode::Off);
    s.dev_complete_all(3.0);
    if !s.is_complete() {
        return Err("paste session did not complete".into());
    }
    println!("smoke: session complete {}", s.summary());
    Ok(())
}

/// The full book path: create -> generate (spawn/parse) -> persist -> type -> export.
fn book_cycle(live: bool) -> Result<(), String> {
    let data = std::env::var("BOOKLEY_DATA_DIR").map_err(|e| e.to_string())?;
    let root = std::path::PathBuf::from(&data).join("data/books");
    let store = BookStore::new(root);
    let mut book = store
        .create(
            "Smoke Test",
            "English",
            if live {
                "A 60-second story about a lighthouse keeper. Keep it very short."
            } else {
                "A test premise."
            },
            false,
        )
        .map_err(|e| e.to_string())?;

    // Stage the bundled plugin exactly like the app does.
    let plugin_root = std::path::PathBuf::from(&data).join("data/plugin");
    let plugin_dir =
        crate::core::book::agent::stage_plugin(&plugin_root).map_err(|e| e.to_string())?;

    let runner = ClaudeRunner::new();
    let req = GenRequest {
        prompt: prompt::chapter_prompt(&book, 1, "", false, None, false),
        system_prompt: prompt::system_prompt(),
        model: if live { "opus".into() } else { "sonnet".into() },
        plugin_dir: plugin_dir.clone(),
        cwd: book.dir.clone(),
        resume_session: None,
        fork_session: false,
        stream: true,
        timeout_secs: if live { 600 } else { 60 },
    };
    let cancel = AtomicBool::new(false);
    let mut deltas = String::new();
    let done = runner
        .run(&req, &cancel, &mut |d| deltas.push_str(d))
        .map_err(|e| format!("generation failed: {e:?}"))?;

    if done.session_id.is_none() {
        return Err("no session_id captured".into());
    }
    if !live && !done.plugins.iter().any(|p| p == "novelist") {
        return Err(format!("novelist plugin not loaded: {:?}", done.plugins));
    }
    if done.plugin_errors {
        return Err("plugin_errors reported".into());
    }

    let (title, prose, bible) = match prompt::parse_reply(&done.text) {
        ParsedReply::Chapter {
            title,
            prose,
            bible,
        } => (title, prose, bible),
        other => return Err(format!("expected a chapter reply, got {other:?}")),
    };
    if prose.split_whitespace().count() < 10 {
        return Err("chapter suspiciously short".into());
    }
    book.meta.session_id = done.session_id.clone();
    book.write_chapter(1, &title, &prose, &bible)
        .map_err(|e| e.to_string())?;
    println!("smoke: chapter generated book={} n=1", book.meta.slug);

    // Type the chapter through the real session pipeline via the dev path.
    let plain = crate::core::book::export::markdown_to_plain(&prose);
    let mut s = Session::new(
        crate::core::text_source::Target::from_text(&plain, "ch1"),
        ErrorMode::Off,
    );
    s.dev_complete_all(60.0);
    if !s.is_complete() {
        return Err("chapter typing did not complete".into());
    }
    book.set_typed_progress(1, plain.chars().count(), true);
    let reloaded = store.load(&book.meta.slug).map_err(|e| e.to_string())?;
    if !reloaded.all_chapters_typed() {
        return Err("typed progress did not persist".into());
    }
    println!("smoke: session complete {}", s.summary());

    // Exports.
    let md = crate::core::book::export::export_markdown(&reloaded);
    if !md.contains("Chapter 1") {
        return Err("markdown export missing chapter heading".into());
    }
    let pdf = crate::core::book::export::export_pdf(&reloaded)?;
    if !pdf.starts_with(b"%PDF") || pdf.len() < 1000 {
        return Err(format!("bad pdf output ({} bytes)", pdf.len()));
    }
    println!(
        "smoke: exports ok md={}B pdf={}B live={}",
        md.len(),
        pdf.len(),
        live
    );
    // Exports carry book content only: no premise/language prompt material. (Fake path
    // only: a live chapter's prose could legitimately echo premise words.)
    if !live && (md.contains("A test premise") || md.contains("*Language:")) {
        return Err("markdown export leaked prompt material".into());
    }

    // Cover: claude designs a self-contained SVG which is validated, rasterized to PNG,
    // stored with the book, and becomes page one of the PDF.
    if !live {
        std::env::set_var("FAKE_CLAUDE_MODE", "cover");
    }
    let model = if live { "opus" } else { "sonnet" };
    let cancel = AtomicBool::new(false);
    let cover = crate::core::book::cover::generate_cover_blocking(
        &runner,
        &reloaded,
        model,
        plugin_dir.clone(),
        if live { 600 } else { 60 },
        &cancel,
    )
    .map_err(|e| format!("cover generation failed: {e:?}"))?;
    if !live {
        std::env::remove_var("FAKE_CLAUDE_MODE");
    }
    if !cover.png.starts_with(b"\x89PNG") {
        return Err("cover is not a PNG".into());
    }
    if !live && cover.used_fallback {
        return Err("the canned SVG must render without the fallback".into());
    }
    std::fs::write(reloaded.cover_path(), &cover.png).map_err(|e| e.to_string())?;
    let pdf_cover = crate::core::book::export::export_pdf(&reloaded)?;
    if !pdf_cover.starts_with(b"%PDF") || pdf_cover.len() <= pdf.len() {
        return Err(format!(
            "cover did not land in the PDF ({} -> {} bytes)",
            pdf.len(),
            pdf_cover.len()
        ));
    }
    println!(
        "smoke: cover ok png={}B pdf={}B fallback={}",
        cover.png.len(),
        pdf_cover.len(),
        cover.used_fallback
    );

    if !live {
        // An unusable design (twice) must still yield a cover via the local fallback.
        std::env::set_var("FAKE_CLAUDE_MODE", "cover_invalid");
        let fb = crate::core::book::cover::generate_cover_blocking(
            &runner,
            &reloaded,
            model,
            plugin_dir,
            60,
            &AtomicBool::new(false),
        )
        .map_err(|e| format!("fallback cover failed: {e:?}"))?;
        std::env::remove_var("FAKE_CLAUDE_MODE");
        if !fb.used_fallback {
            return Err("an invalid SVG must hit the typographic fallback".into());
        }
        if !fb.png.starts_with(b"\x89PNG") {
            return Err("fallback cover is not a PNG".into());
        }
        println!("smoke: cover fallback ok png={}B", fb.png.len());
    }
    Ok(())
}

/// Failure classification against the fake claude's error modes.
fn error_classification() -> Result<(), String> {
    for (mode, want) in [
        ("rate_limit", GenError::RateLimited),
        ("logged_out", GenError::LoggedOut),
        ("max_turns", GenError::MaxTurns),
    ] {
        std::env::set_var("FAKE_CLAUDE_MODE", mode);
        let runner = ClaudeRunner::new();
        let req = minimal_req(30);
        let cancel = AtomicBool::new(false);
        let got = runner.run(&req, &cancel, &mut |_| {});
        std::env::remove_var("FAKE_CLAUDE_MODE");
        match got {
            Err(e) if e == want => {}
            other => return Err(format!("mode {mode}: expected {want:?}, got {other:?}")),
        }
    }
    Ok(())
}

/// A hanging child must be killed by the app-side timeout, never hanging the app.
fn timeout_kills() -> Result<(), String> {
    std::env::set_var("FAKE_CLAUDE_MODE", "hang");
    let runner = ClaudeRunner::new();
    let req = minimal_req(2);
    let cancel = AtomicBool::new(false);
    let started = std::time::Instant::now();
    let got = runner.run(&req, &cancel, &mut |_| {});
    std::env::remove_var("FAKE_CLAUDE_MODE");
    if started.elapsed() > std::time::Duration::from_secs(15) {
        return Err("timeout did not fire in time".into());
    }
    match got {
        Err(GenError::Cancelled) => Ok(()),
        other => Err(format!("expected Cancelled, got {other:?}")),
    }
}

/// Drive the whole Connect Claude flow against the fake PTY script: URL scraped, code
/// submitted, token captured and stored 0600.
fn connect_flow() -> Result<(), String> {
    use crate::core::claude_auth::{self, ConnectEvent};

    claude_auth::delete_token();
    let mut flow = claude_auth::ConnectFlow::start().map_err(|e| e.to_string())?;
    let deadline = std::time::Duration::from_secs(20);

    // Expect the URL first.
    match flow.events.recv_timeout(deadline) {
        Ok(ConnectEvent::Url(url)) => {
            if !url.contains("oauth") {
                return Err(format!("scraped a non-oauth url: {url}"));
            }
            println!("smoke: claude auth url scraped");
        }
        other => return Err(format!("expected Url event, got {other:?}")),
    }
    // Then the code prompt.
    match flow.events.recv_timeout(deadline) {
        Ok(ConnectEvent::WaitingForCode) => {}
        other => return Err(format!("expected WaitingForCode, got {other:?}")),
    }
    flow.submit_code("fake-authentication-code");
    match flow.events.recv_timeout(deadline) {
        Ok(ConnectEvent::TokenStored) => {}
        other => return Err(format!("expected TokenStored, got {other:?}")),
    }
    let tok = claude_auth::load_token().ok_or("token not stored")?;
    if !tok.starts_with("sk-ant-oat") {
        return Err("stored token has wrong shape".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = claude_auth::token_path().ok_or("no token path")?;
        let mode = std::fs::metadata(&path)
            .map_err(|e| e.to_string())?
            .permissions()
            .mode();
        if mode & 0o777 != 0o600 {
            return Err(format!("token file mode is {:o}, want 600", mode & 0o777));
        }
    }
    claude_auth::delete_token();
    println!("smoke: claude auth token captured and stored");
    Ok(())
}

fn minimal_req(timeout_secs: u64) -> GenRequest {
    let data = std::env::var("BOOKLEY_DATA_DIR").unwrap_or_else(|_| "/tmp".into());
    GenRequest {
        prompt: "test".into(),
        system_prompt: "test".into(),
        model: "sonnet".into(),
        plugin_dir: std::path::PathBuf::from(&data),
        cwd: std::path::PathBuf::from(&data),
        resume_session: None,
        fork_session: false,
        stream: false,
        timeout_secs,
    }
}
