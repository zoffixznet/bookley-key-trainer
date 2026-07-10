//! Integration tests that exercise the REAL process-spawning paths (generation and the
//! Connect Claude PTY flow) against tests/fake_claude.sh. They never touch the network,
//! real usage, or real auth.
//!
//! Everything lives in one #[test] because the paths are configured through process-wide
//! env vars (BOOKLEY_CLAUDE_BIN / BOOKLEY_DATA_DIR / FAKE_CLAUDE_MODE) and cargo runs
//! tests in parallel threads.

use std::sync::atomic::AtomicBool;

use bookley::core::book::agent::{ClaudeRunner, CommandRunner, GenError, GenRequest};
use bookley::core::book::prompt::{parse_reply, ParsedReply};
use bookley::core::claude_auth::{self, ConnectEvent, ConnectFlow};

fn fake_bin() -> String {
    format!("{}/tests/fake_claude.sh", env!("CARGO_MANIFEST_DIR"))
}

fn req(data: &std::path::Path, timeout_secs: u64) -> GenRequest {
    GenRequest {
        prompt: "write chapter 1".into(),
        system_prompt: "system".into(),
        model: "sonnet".into(),
        plugin_dir: data.to_path_buf(),
        cwd: data.to_path_buf(),
        resume_session: None,
        fork_session: false,
        stream: true,
        timeout_secs,
    }
}

#[test]
fn spawn_paths_against_fake_claude() {
    let data = std::env::temp_dir().join(format!("bookley-it-{}", std::process::id()));
    std::fs::create_dir_all(&data).unwrap();
    std::env::set_var("BOOKLEY_CLAUDE_BIN", fake_bin());
    std::env::set_var("BOOKLEY_DATA_DIR", &data);
    std::env::remove_var("FAKE_CLAUDE_MODE");

    // --- successful generation: deltas stream, chapter parses, session id captured ---
    let runner = ClaudeRunner::new();
    let cancel = AtomicBool::new(false);
    let mut deltas = String::new();
    let ok = runner
        .run(&req(&data, 60), &cancel, &mut |d| deltas.push_str(d))
        .expect("generation should succeed");
    assert_eq!(ok.session_id.as_deref(), Some("fake-session-0001"));
    assert!(ok.plugins.iter().any(|p| p == "novelist"));
    assert!(!ok.plugin_errors);
    assert!(
        deltas.contains("===CHAPTER==="),
        "live deltas should stream"
    );
    match parse_reply(&ok.text) {
        ParsedReply::Chapter {
            title,
            prose,
            bible,
        } => {
            assert_eq!(title, "The Fake Chapter");
            assert!(prose.contains("smoke-test window"));
            assert!(bible.contains("VOICE"));
        }
        other => panic!("expected chapter, got {other:?}"),
    }

    // --- error classification through the real spawn path ---
    for (mode, want) in [
        ("rate_limit", GenError::RateLimited),
        ("logged_out", GenError::LoggedOut),
        ("max_turns", GenError::MaxTurns),
    ] {
        std::env::set_var("FAKE_CLAUDE_MODE", mode);
        let e = runner
            .run(&req(&data, 30), &AtomicBool::new(false), &mut |_| {})
            .expect_err("must fail");
        assert_eq!(e, want, "mode {mode}");
    }
    std::env::remove_var("FAKE_CLAUDE_MODE");

    // --- auth status via the CLI ---
    assert!(runner.auth_status().unwrap());
    std::env::set_var("FAKE_CLAUDE_MODE", "logged_out");
    assert!(!runner.auth_status().unwrap());
    std::env::remove_var("FAKE_CLAUDE_MODE");

    // --- the whole Connect Claude flow over a fake PTY ---
    claude_auth::delete_token();
    let mut flow = ConnectFlow::start().expect("flow starts");
    let t = std::time::Duration::from_secs(20);
    match flow.events.recv_timeout(t).unwrap() {
        ConnectEvent::Url(url) => assert!(url.contains("oauth")),
        other => panic!("expected Url, got {other:?}"),
    }
    assert_eq!(
        flow.events.recv_timeout(t).unwrap(),
        ConnectEvent::WaitingForCode
    );
    flow.submit_code("a-code-from-the-browser");
    assert_eq!(
        flow.events.recv_timeout(t).unwrap(),
        ConnectEvent::TokenStored
    );
    let tok = claude_auth::load_token().expect("token stored");
    assert!(tok.starts_with("sk-ant-oat"));
    // With a stored token, auth counts as connected without asking the CLI.
    assert!(runner.auth_status().unwrap());
    assert_eq!(
        claude_auth::check_auth_blocking(),
        claude_auth::AuthCheck::ConnectedToken
    );

    // --- a rejected code surfaces a Failed event, not a hang ---
    claude_auth::delete_token();
    std::env::set_var("FAKE_CLAUDE_MODE", "badcode");
    let mut flow = ConnectFlow::start().expect("flow starts");
    loop {
        match flow.events.recv_timeout(t).unwrap() {
            ConnectEvent::WaitingForCode => break,
            ConnectEvent::Url(_) => {}
            other => panic!("unexpected {other:?}"),
        }
    }
    flow.submit_code("whatever");
    match flow.events.recv_timeout(t).unwrap() {
        ConnectEvent::Failed(msg) => assert!(msg.to_lowercase().contains("code")),
        other => panic!("expected Failed, got {other:?}"),
    }
    std::env::remove_var("FAKE_CLAUDE_MODE");
    assert_eq!(claude_auth::load_token(), None);

    // --- the last-chapter conclude directive reaches the agent's prompt ---
    let store = bookley::core::book::store::BookStore::new(data.join("books"));
    let book = store
        .create("End Test", "English", "a short tale", false)
        .unwrap();
    let conclude_prompt =
        bookley::core::book::prompt::chapter_prompt(&book, 2, "finish it", false, None, true);
    assert!(conclude_prompt.contains(bookley::core::book::prompt::CONCLUDE_DIRECTIVE));
    let dump = data.join("prompt-dump.txt");
    std::env::set_var("FAKE_CLAUDE_DUMP_PROMPT", &dump);
    let mut creq = req(&data, 60);
    creq.prompt = conclude_prompt;
    runner
        .run(&creq, &AtomicBool::new(false), &mut |_| {})
        .expect("conclude generation succeeds");
    std::env::remove_var("FAKE_CLAUDE_DUMP_PROMPT");
    let sent = std::fs::read_to_string(&dump).expect("prompt dump written");
    assert!(
        sent.contains("Conclude the story IN THIS CHAPTER"),
        "the conclude directive must reach the spawned agent"
    );
    assert!(sent.contains("not a cliffhanger"));

    // --- cover generation through the real spawn path ---
    // Canned SVG: validated, rasterized, no fallback.
    std::env::set_var("FAKE_CLAUDE_MODE", "cover");
    let cover_book = store.create("Cover Test", "English", "", false).unwrap();
    let out = bookley::core::book::cover::generate_cover_blocking(
        &runner,
        &cover_book,
        "sonnet",
        data.clone(),
        60,
        &AtomicBool::new(false),
    )
    .expect("canned SVG cover");
    assert!(out.png.starts_with(b"\x89PNG"));
    assert!(!out.used_fallback, "canned SVG must render directly");
    // Unusable reply (twice): the local typographic fallback still yields a cover.
    std::env::set_var("FAKE_CLAUDE_MODE", "cover_invalid");
    let out = bookley::core::book::cover::generate_cover_blocking(
        &runner,
        &cover_book,
        "sonnet",
        data.clone(),
        60,
        &AtomicBool::new(false),
    )
    .expect("fallback cover");
    assert!(out.used_fallback, "invalid SVG must hit the fallback");
    assert!(out.png.starts_with(b"\x89PNG"));
    // A hard CLI failure surfaces like chapter generation does (no silent fallback).
    std::env::set_var("FAKE_CLAUDE_MODE", "logged_out");
    let e = bookley::core::book::cover::generate_cover_blocking(
        &runner,
        &cover_book,
        "sonnet",
        data.clone(),
        60,
        &AtomicBool::new(false),
    )
    .expect_err("auth failure must propagate");
    assert_eq!(e, GenError::LoggedOut);
    std::env::remove_var("FAKE_CLAUDE_MODE");

    let _ = std::fs::remove_dir_all(&data);
}
