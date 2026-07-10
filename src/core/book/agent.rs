//! The agent client: spawns the `claude` CLI to generate book chapters, and parses its
//! output. Testable via a `CommandRunner` trait so unit tests and the smoke test inject a
//! fake `claude` and never touch the network or real subscription usage.
//!
//! Hard rules (spec): never set `ANTHROPIC_API_KEY` in the child env; never use `--bare`;
//! always keep runs on the subscription. Always pass `--plugin-dir <novelist>`,
//! `--tools ""`, `--permission-mode dontAsk`, `--setting-sources ""` (to avoid the user's
//! global hooks), and the full system prompt via `--append-system-prompt`.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::Value;

/// A friendly classification of a failed generation, mapped from `api_retry` error
/// categories and non-success result subtypes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenError {
    /// `claude` binary not found / could not spawn.
    NotFound,
    /// Not logged in.
    LoggedOut,
    /// Org policy blocks this OAuth org.
    OrgNotAllowed,
    /// Billing problem.
    Billing,
    /// Rate limited.
    RateLimited,
    /// Model overloaded.
    Overloaded,
    /// Bad request / model not found / invalid.
    InvalidRequest(String),
    /// Server error.
    ServerError,
    /// Ran out of turns (incomplete chapter, not a clean stop).
    MaxTurns,
    /// Hit the output token cap.
    MaxOutputTokens,
    /// Anything else.
    Other(String),
}

impl GenError {
    /// A user-facing message with actionable guidance.
    pub fn user_message(&self) -> String {
        match self {
            GenError::NotFound => {
                "Claude CLI not found. Install it and make sure `claude` is on your PATH \
(or set BOOKLEY_CLAUDE_BIN)."
                    .into()
            }
            GenError::LoggedOut => {
                "Claude isn't signed in. Run `claude auth login` in a terminal, then retry."
                    .into()
            }
            GenError::OrgNotAllowed => {
                "Your Claude organization isn't allowed to use this. Check your account, \
then retry."
                    .into()
            }
            GenError::Billing => {
                "Claude reported a billing problem. Check your subscription, then retry."
                    .into()
            }
            GenError::RateLimited => {
                "Claude is rate-limited right now. Wait a bit and retry; your chapters are \
saved.".into()
            }
            GenError::Overloaded => {
                "Claude is overloaded. Retry in a moment; your chapters are saved.".into()
            }
            GenError::InvalidRequest(s) => format!("Claude rejected the request: {s}"),
            GenError::ServerError => {
                "Claude had a server error. Retry in a moment; your chapters are saved."
                    .into()
            }
            GenError::MaxTurns => {
                "The chapter didn't finish (turn limit). Retry to regenerate it; nothing \
was lost.".into()
            }
            GenError::MaxOutputTokens => {
                "The chapter hit the length cap and may be truncated. Retry, or accept the \
partial chapter.".into()
            }
            GenError::Other(s) => format!("Book generation failed: {s}"),
        }
    }
}

/// Map an `api_retry` error category string to a `GenError`.
fn classify_category(cat: &str) -> GenError {
    match cat {
        "authentication_failed" => GenError::LoggedOut,
        "oauth_org_not_allowed" => GenError::OrgNotAllowed,
        "billing_error" => GenError::Billing,
        "rate_limit" => GenError::RateLimited,
        "overloaded" => GenError::Overloaded,
        "invalid_request" => GenError::InvalidRequest("invalid request".into()),
        "model_not_found" => GenError::InvalidRequest("model not found".into()),
        "server_error" => GenError::ServerError,
        "max_output_tokens" => GenError::MaxOutputTokens,
        other => GenError::Other(other.to_string()),
    }
}

/// A parsed successful generation.
#[derive(Debug, Clone)]
pub struct GenSuccess {
    /// The full assistant text (marker-delimited; caller parses with `prompt::parse_reply`).
    pub text: String,
    /// The session id for multi-turn continuity.
    pub session_id: Option<String>,
    /// The model that produced it (from the init event), if seen.
    pub model: Option<String>,
    /// Plugins loaded, from the init event (for verification/logging).
    pub plugins: Vec<String>,
    /// Whether any plugin_errors were reported.
    pub plugin_errors: bool,
}

pub type GenResult = Result<GenSuccess, GenError>;

/// Parse a stream of `claude --output-format stream-json` lines (one JSON object per line)
/// into a `GenResult`. Also used to feed a live text callback for the "writing..." view.
pub fn parse_stream_lines<F: FnMut(&str)>(
    lines: impl Iterator<Item = String>,
    mut on_delta: F,
) -> GenResult {
    let mut session_id = None;
    let mut model = None;
    let mut plugins = Vec::new();
    let mut plugin_errors = false;
    let mut assistant_text = String::new();
    let mut result_text: Option<String> = None;
    let mut pending_error: Option<GenError> = None;

    for line in lines {
        let line = line.trim();
        if !line.starts_with('{') {
            continue;
        }
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match ty {
            "system" => {
                let sub = v.get("subtype").and_then(|s| s.as_str()).unwrap_or("");
                if sub == "init" {
                    session_id = v
                        .get("session_id")
                        .and_then(|s| s.as_str())
                        .map(String::from);
                    model = v.get("model").and_then(|s| s.as_str()).map(String::from);
                    if let Some(arr) = v.get("plugins").and_then(|p| p.as_array()) {
                        for p in arr {
                            if let Some(n) = p.get("name").and_then(|n| n.as_str()) {
                                plugins.push(n.to_string());
                            }
                        }
                    }
                    if let Some(pe) = v.get("plugin_errors") {
                        plugin_errors = match pe {
                            Value::Array(a) => !a.is_empty(),
                            Value::Object(o) => !o.is_empty(),
                            Value::Null => false,
                            _ => false,
                        };
                    }
                }
            }
            "api_retry" => {
                if let Some(cat) = v.get("error").and_then(|e| e.as_str()) {
                    pending_error = Some(classify_category(cat));
                } else if let Some(cat) = v
                    .get("error")
                    .and_then(|e| e.get("category"))
                    .and_then(|c| c.as_str())
                {
                    pending_error = Some(classify_category(cat));
                }
            }
            "stream_event" => {
                if let Some(ev) = v.get("event") {
                    let et = ev.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    if et == "content_block_delta" {
                        if let Some(d) = ev.get("delta") {
                            if d.get("type").and_then(|t| t.as_str()) == Some("text_delta") {
                                if let Some(txt) = d.get("text").and_then(|t| t.as_str()) {
                                    on_delta(txt);
                                }
                            }
                        }
                    }
                }
            }
            "assistant" => {
                if let Some(content) = v
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_array())
                {
                    for block in content {
                        if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                            if let Some(txt) = block.get("text").and_then(|t| t.as_str()) {
                                assistant_text.push_str(txt);
                            }
                        }
                    }
                }
            }
            "result" => {
                let sub = v.get("subtype").and_then(|s| s.as_str()).unwrap_or("");
                let is_error = v.get("is_error").and_then(|b| b.as_bool()).unwrap_or(false);
                if session_id.is_none() {
                    session_id = v
                        .get("session_id")
                        .and_then(|s| s.as_str())
                        .map(String::from);
                }
                match sub {
                    "success" if !is_error => {
                        result_text = v
                            .get("result")
                            .and_then(|r| r.as_str())
                            .map(String::from);
                    }
                    "error_max_turns" => return Err(GenError::MaxTurns),
                    "error_during_execution" => {
                        return Err(pending_error
                            .clone()
                            .unwrap_or_else(|| GenError::Other("execution error".into())))
                    }
                    _ => {
                        return Err(pending_error
                            .clone()
                            .unwrap_or_else(|| GenError::Other(format!("result subtype {sub}"))))
                    }
                }
            }
            _ => {}
        }
    }

    // Prefer the final `result` text; fall back to accumulated assistant text.
    let text = result_text
        .filter(|t| !t.trim().is_empty())
        .or_else(|| {
            if assistant_text.trim().is_empty() {
                None
            } else {
                Some(assistant_text.clone())
            }
        });

    match text {
        Some(t) => Ok(GenSuccess {
            text: t,
            session_id,
            model,
            plugins,
            plugin_errors,
        }),
        None => Err(pending_error.unwrap_or_else(|| GenError::Other("no result produced".into()))),
    }
}

/// Parse a single `--output-format json` object into a `GenResult`.
pub fn parse_json_result(raw: &str) -> GenResult {
    let v: Value = serde_json::from_str(raw.trim())
        .map_err(|e| GenError::Other(format!("bad json: {e}")))?;
    let sub = v.get("subtype").and_then(|s| s.as_str()).unwrap_or("");
    let is_error = v.get("is_error").and_then(|b| b.as_bool()).unwrap_or(false);
    let session_id = v
        .get("session_id")
        .and_then(|s| s.as_str())
        .map(String::from);
    match sub {
        "success" if !is_error => {
            let text = v
                .get("result")
                .and_then(|r| r.as_str())
                .unwrap_or("")
                .to_string();
            if text.trim().is_empty() {
                return Err(GenError::Other("empty result".into()));
            }
            Ok(GenSuccess {
                text,
                session_id,
                model: None,
                plugins: Vec::new(),
                plugin_errors: false,
            })
        }
        "error_max_turns" => Err(GenError::MaxTurns),
        _ => Err(GenError::Other(format!("result subtype {sub}"))),
    }
}

/// Options controlling one generation call.
#[derive(Debug, Clone)]
pub struct GenRequest {
    pub prompt: String,
    pub system_prompt: String,
    pub model: String,
    pub plugin_dir: PathBuf,
    /// Working directory: the book's dir, so `--resume` session lookup (cwd-scoped) works.
    pub cwd: PathBuf,
    /// Resume this session id if set (later chapters).
    pub resume_session: Option<String>,
    /// Fork the session (for alternate drafts / rewrites) instead of mutating it.
    pub fork_session: bool,
    /// Whether to request partial messages for a live view.
    pub stream: bool,
}

/// Abstraction over running the `claude` process so tests can inject a fake.
pub trait CommandRunner: Send + Sync {
    /// Run the request, invoking `on_delta` for streamed text and returning the result.
    fn run(&self, req: &GenRequest, on_delta: &mut dyn FnMut(&str)) -> GenResult;
    /// Check login status. Returns Ok(true) if logged in, Ok(false) if logged out.
    fn auth_status(&self) -> Result<bool, GenError>;
}

/// The real runner: spawns the `claude` binary (path from BOOKLEY_CLAUDE_BIN or "claude").
pub struct ClaudeRunner {
    pub bin: String,
}

impl Default for ClaudeRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaudeRunner {
    pub fn new() -> Self {
        let bin = std::env::var("BOOKLEY_CLAUDE_BIN").unwrap_or_else(|_| "claude".to_string());
        ClaudeRunner { bin }
    }

    /// Build the base command with the sanitized env and the shared flags.
    fn base_command(&self) -> Command {
        let mut cmd = Command::new(&self.bin);
        // Sanitize: never leak ANTHROPIC_API_KEY into the child (would flip to API billing).
        cmd.env_remove("ANTHROPIC_API_KEY");
        cmd.env_remove("ANTHROPIC_AUTH_TOKEN");
        cmd
    }
}

impl CommandRunner for ClaudeRunner {
    fn auth_status(&self) -> Result<bool, GenError> {
        let out = self
            .base_command()
            .arg("auth")
            .arg("status")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();
        let out = match out {
            Ok(o) => o,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(GenError::NotFound),
            Err(e) => return Err(GenError::Other(e.to_string())),
        };
        let text = String::from_utf8_lossy(&out.stdout);
        // Output is JSON with a `loggedIn` boolean; fall back to substring if needed.
        if let Ok(v) = serde_json::from_str::<Value>(text.trim()) {
            if let Some(b) = v.get("loggedIn").and_then(|b| b.as_bool()) {
                return Ok(b);
            }
        }
        Ok(text.to_lowercase().contains("logged in") && !text.to_lowercase().contains("not logged"))
    }

    fn run(&self, req: &GenRequest, on_delta: &mut dyn FnMut(&str)) -> GenResult {
        let mut cmd = self.base_command();
        cmd.current_dir(&req.cwd);
        cmd.arg("-p");
        cmd.arg("--output-format").arg("stream-json");
        cmd.arg("--verbose");
        if req.stream {
            cmd.arg("--include-partial-messages");
        }
        cmd.arg("--tools").arg("");
        cmd.arg("--permission-mode").arg("dontAsk");
        // Do not load the user's global hooks/skills; add our plugin explicitly.
        cmd.arg("--setting-sources").arg("");
        cmd.arg("--plugin-dir").arg(&req.plugin_dir);
        cmd.arg("--append-system-prompt").arg(&req.system_prompt);
        cmd.arg("--model").arg(&req.model);
        if let Some(sid) = &req.resume_session {
            cmd.arg("--resume").arg(sid);
            if req.fork_session {
                cmd.arg("--fork-session");
            }
        }
        // The prompt goes on stdin to avoid arg length limits.
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(GenError::NotFound),
            Err(e) => return Err(GenError::Other(e.to_string())),
        };

        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(req.prompt.as_bytes());
            // dropping stdin closes it
        }

        let stdout = child.stdout.take();
        let result = if let Some(stdout) = stdout {
            let reader = BufReader::new(stdout);
            let lines = reader.lines().map_while(Result::ok);
            parse_stream_lines(lines, |d| on_delta(d))
        } else {
            Err(GenError::Other("no stdout".into()))
        };

        // Reap the child and consider its exit status for error subtypes.
        let status = child.wait();
        match (&result, status) {
            (Ok(_), _) => result,
            (Err(_), Ok(st)) if !st.success() => result, // already classified
            (Err(_), _) => result,
        }
    }
}

/// A convenience wrapper the UI uses: build the request and run it on a background thread.
/// Returns a receiver of `AgentEvent`s.
pub struct AgentClient {
    pub runner: std::sync::Arc<dyn CommandRunner>,
    pub plugin_dir: PathBuf,
}

/// Events streamed back from a running generation.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// A chunk of live text (for the "writing..." view).
    Delta(String),
    /// Generation finished successfully.
    Done(Box<GenDone>),
    /// Generation failed.
    Failed(GenError),
}

#[derive(Debug, Clone)]
pub struct GenDone {
    pub text: String,
    pub session_id: Option<String>,
    pub plugins: Vec<String>,
    pub plugin_errors: bool,
}

impl AgentClient {
    pub fn new(runner: std::sync::Arc<dyn CommandRunner>, plugin_dir: PathBuf) -> Self {
        AgentClient { runner, plugin_dir }
    }

    /// Spawn a generation on a background thread; events arrive on the returned receiver.
    pub fn generate(
        &self,
        prompt: String,
        system_prompt: String,
        model: String,
        cwd: PathBuf,
        resume_session: Option<String>,
        fork_session: bool,
    ) -> std::sync::mpsc::Receiver<AgentEvent> {
        let (tx, rx) = std::sync::mpsc::channel();
        let runner = self.runner.clone();
        let plugin_dir = self.plugin_dir.clone();
        std::thread::spawn(move || {
            let req = GenRequest {
                prompt,
                system_prompt,
                model,
                plugin_dir,
                cwd,
                resume_session,
                fork_session,
                stream: true,
            };
            let tx_delta = tx.clone();
            let mut on_delta = move |d: &str| {
                let _ = tx_delta.send(AgentEvent::Delta(d.to_string()));
            };
            match runner.run(&req, &mut on_delta) {
                Ok(s) => {
                    let _ = tx.send(AgentEvent::Done(Box::new(GenDone {
                        text: s.text,
                        session_id: s.session_id,
                        plugins: s.plugins,
                        plugin_errors: s.plugin_errors,
                    })));
                }
                Err(e) => {
                    let _ = tx.send(AgentEvent::Failed(e));
                }
            }
        });
        rx
    }
}

/// Stage the compiled-in plugin files into a writable directory so `--plugin-dir` works
/// from an installed binary. Idempotent. Returns the plugin root to pass to `--plugin-dir`.
pub fn stage_plugin(dest_root: &std::path::Path) -> std::io::Result<PathBuf> {
    // The plugin files are embedded at compile time via a small manifest below.
    let files: &[(&str, &str)] = &[
        (
            "novelist/.claude-plugin/plugin.json",
            include_str!("../../../assets/plugin/novelist/.claude-plugin/plugin.json"),
        ),
        (
            "novelist/skills/write-chapter/SKILL.md",
            include_str!("../../../assets/plugin/novelist/skills/write-chapter/SKILL.md"),
        ),
    ];
    for (rel, content) in files {
        let path = dest_root.join(rel);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, content)?;
    }
    Ok(dest_root.join("novelist"))
}

/// A stable summary of what the client will pass, for logging.
pub fn describe_flags() -> BTreeMap<&'static str, &'static str> {
    let mut m = BTreeMap::new();
    m.insert("output-format", "stream-json");
    m.insert("tools", "\"\" (all disabled)");
    m.insert("permission-mode", "dontAsk");
    m.insert("setting-sources", "\"\" (no user hooks)");
    m.insert("api-key", "removed from env (subscription only)");
    m.insert("bare", "never used");
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &str) -> impl Iterator<Item = String> + '_ {
        s.lines().map(|l| l.to_string())
    }

    #[test]
    fn parse_stream_success_extracts_text_and_session() {
        let s = r#"{"type":"system","subtype":"init","model":"claude-x","session_id":"abc-123","plugins":[{"name":"novelist","path":"/p"}],"plugin_errors":[]}
{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"===CHAPTER==="}}}
{"type":"assistant","message":{"content":[{"type":"text","text":"===TITLE===\nOne\n===CHAPTER===\nProse here.\n===BIBLE===\nCAST: A\n===END==="}]}}
{"type":"result","subtype":"success","is_error":false,"result":"===TITLE===\nOne\n===CHAPTER===\nProse here.\n===BIBLE===\nCAST: A\n===END===","session_id":"abc-123"}"#;
        let mut deltas = String::new();
        let r = parse_stream_lines(lines(s), |d| deltas.push_str(d)).unwrap();
        assert_eq!(r.session_id.as_deref(), Some("abc-123"));
        assert_eq!(r.model.as_deref(), Some("claude-x"));
        assert_eq!(r.plugins, vec!["novelist".to_string()]);
        assert!(!r.plugin_errors);
        assert!(r.text.contains("Prose here."));
        assert!(deltas.contains("===CHAPTER==="));
    }

    #[test]
    fn parse_stream_falls_back_to_assistant_text() {
        // No result line; assistant text is used.
        let s = r#"{"type":"system","subtype":"init","session_id":"s1"}
{"type":"assistant","message":{"content":[{"type":"text","text":"only assistant prose"}]}}"#;
        let r = parse_stream_lines(lines(s), |_| {}).unwrap();
        assert_eq!(r.text, "only assistant prose");
        assert_eq!(r.session_id.as_deref(), Some("s1"));
    }

    #[test]
    fn parse_stream_classifies_max_turns() {
        let s = r#"{"type":"system","subtype":"init","session_id":"s"}
{"type":"result","subtype":"error_max_turns","is_error":true,"session_id":"s"}"#;
        let e = parse_stream_lines(lines(s), |_| {}).unwrap_err();
        assert_eq!(e, GenError::MaxTurns);
    }

    #[test]
    fn parse_stream_classifies_rate_limit_from_api_retry() {
        let s = r#"{"type":"system","subtype":"init","session_id":"s"}
{"type":"api_retry","error":"rate_limit","attempt":1,"max_retries":5,"retry_delay_ms":1000}
{"type":"result","subtype":"error_during_execution","is_error":true,"session_id":"s"}"#;
        let e = parse_stream_lines(lines(s), |_| {}).unwrap_err();
        assert_eq!(e, GenError::RateLimited);
    }

    #[test]
    fn parse_stream_classifies_logged_out() {
        let s = r#"{"type":"system","subtype":"init","session_id":"s"}
{"type":"api_retry","error":"authentication_failed"}
{"type":"result","subtype":"error_during_execution","is_error":true}"#;
        let e = parse_stream_lines(lines(s), |_| {}).unwrap_err();
        assert_eq!(e, GenError::LoggedOut);
    }

    #[test]
    fn parse_json_result_success_and_error() {
        let ok = r#"{"type":"result","subtype":"success","is_error":false,"result":"hello","session_id":"z"}"#;
        let r = parse_json_result(ok).unwrap();
        assert_eq!(r.text, "hello");
        assert_eq!(r.session_id.as_deref(), Some("z"));

        let bad = r#"{"type":"result","subtype":"error_max_turns","is_error":true}"#;
        assert_eq!(parse_json_result(bad).unwrap_err(), GenError::MaxTurns);
    }

    #[test]
    fn stage_plugin_writes_files() {
        let dir = std::env::temp_dir().join(format!("bookley-plug-{}", std::process::id()));
        let root = stage_plugin(&dir).unwrap();
        assert!(root.join(".claude-plugin/plugin.json").exists());
        assert!(root.join("skills/write-chapter/SKILL.md").exists());
        let skill = std::fs::read_to_string(root.join("skills/write-chapter/SKILL.md")).unwrap();
        assert!(skill.contains("Continuity bible"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
