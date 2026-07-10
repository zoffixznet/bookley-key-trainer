//! In-app Claude authentication: the "Connect Claude" flow.
//!
//! The app never asks the user to run a terminal command. It PTY-drives Anthropic's own
//! `claude setup-token` login flow, scrapes the OAuth authorize URL, shows it in-app as a
//! clickable link, forwards the pasted authentication code back into the PTY, captures the
//! resulting long-lived OAuth token (`sk-ant-oat...`), stores it with mode 0600 under the
//! app's config dir, and passes it to every spawned `claude` child as
//! `CLAUDE_CODE_OAUTH_TOKEN` (subscription billing; never `ANTHROPIC_API_KEY`).
//!
//! CLI output is treated as an unstable interface: reads go through an ANSI-stripping
//! buffer, matching is defensive regex-free scanning, and every state has a timeout. The
//! whole flow is testable against a fake PTY script via `BOOKLEY_CLAUDE_BIN`.

use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

/// Where the captured OAuth token lives (mode 0600).
pub fn token_path() -> Option<PathBuf> {
    super::paths::config_dir().map(|d| d.join("claude-oauth-token"))
}

/// Load the stored OAuth token, if any.
pub fn load_token() -> Option<String> {
    let path = token_path()?;
    let tok = std::fs::read_to_string(path).ok()?;
    let tok = tok.trim().to_string();
    if tok.starts_with("sk-ant-oat") {
        Some(tok)
    } else {
        None
    }
}

/// Persist the OAuth token with restrictive permissions. Returns the path written.
pub fn save_token(token: &str) -> std::io::Result<PathBuf> {
    let path = token_path().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "no config dir available")
    })?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, token.trim())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(path)
}

/// Remove the stored token (disconnect).
pub fn delete_token() {
    if let Some(p) = token_path() {
        let _ = std::fs::remove_file(p);
    }
}

/// The resolved `claude` binary (env override for tests / power users).
pub fn claude_bin() -> String {
    std::env::var("BOOKLEY_CLAUDE_BIN").unwrap_or_else(|_| "claude".to_string())
}

/// How the app is (or is not) connected to Claude.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthCheck {
    /// A stored OAuth token from the Connect Claude flow.
    ConnectedToken,
    /// The CLI itself is logged in (e.g. a dev box); no in-app token needed.
    ConnectedCli,
    /// CLI present but not signed in; the Connect Claude flow is needed.
    NotConnected,
    /// The `claude` binary is not installed / not on PATH.
    CliMissing,
    /// The check itself failed (still allows trying to generate).
    Unknown(String),
}

impl AuthCheck {
    pub fn is_connected(&self) -> bool {
        matches!(self, AuthCheck::ConnectedToken | AuthCheck::ConnectedCli)
    }
}

/// Blocking auth check: stored token wins; otherwise ask `claude auth status`.
/// Run this off the UI thread; the CLI can take seconds to start.
pub fn check_auth_blocking() -> AuthCheck {
    if load_token().is_some() {
        return AuthCheck::ConnectedToken;
    }
    let bin = claude_bin();
    let mut cmd = std::process::Command::new(&bin);
    cmd.arg("auth")
        .arg("status")
        .arg("--json")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    cmd.env_remove("ANTHROPIC_API_KEY");
    cmd.env_remove("ANTHROPIC_AUTH_TOKEN");
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return AuthCheck::CliMissing,
        Err(e) => return AuthCheck::Unknown(e.to_string()),
    };
    // Bounded wait so a wedged CLI can never hang the caller thread forever.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if std::time::Instant::now() > deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return AuthCheck::Unknown("auth status timed out".into());
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(e) => return AuthCheck::Unknown(e.to_string()),
        }
    }
    let mut out = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.read_to_string(&mut out);
    }
    match serde_json::from_str::<serde_json::Value>(out.trim()) {
        Ok(v) => match v.get("loggedIn").and_then(|b| b.as_bool()) {
            Some(true) => AuthCheck::ConnectedCli,
            Some(false) => AuthCheck::NotConnected,
            None => AuthCheck::Unknown("no loggedIn field in auth status".into()),
        },
        Err(_) => {
            let low = out.to_lowercase();
            if low.contains("logged in") && !low.contains("not logged") {
                AuthCheck::ConnectedCli
            } else {
                AuthCheck::Unknown("unparseable auth status output".into())
            }
        }
    }
}

/// Events streamed from a running Connect Claude flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectEvent {
    /// The OAuth authorize URL was scraped; show it as a clickable link.
    Url(String),
    /// The CLI is waiting for the authentication code to be pasted.
    WaitingForCode,
    /// The long-lived token was captured and stored.
    TokenStored,
    /// The flow failed; human-readable reason.
    Failed(String),
}

/// A running `claude setup-token` flow under a PTY.
pub struct ConnectFlow {
    pub events: Receiver<ConnectEvent>,
    writer: Box<dyn std::io::Write + Send>,
    cancel: Arc<AtomicBool>,
    killer: Box<dyn portable_pty::ChildKiller + Send + Sync>,
}

impl ConnectFlow {
    /// Spawn `claude setup-token` under a wide PTY (so the URL is never line-wrapped) and
    /// start scraping its output on a background thread.
    pub fn start() -> Result<ConnectFlow, String> {
        let bin = claude_bin();
        let pty_system = portable_pty::native_pty_system();
        let pair = pty_system
            .openpty(portable_pty::PtySize {
                rows: 50,
                cols: 500,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("could not open a pty: {e}"))?;

        let mut cmd = portable_pty::CommandBuilder::new(&bin);
        cmd.arg("setup-token");
        // Same sanitation as generation children: keep everything on the subscription.
        cmd.env_remove("ANTHROPIC_API_KEY");
        cmd.env_remove("ANTHROPIC_AUTH_TOKEN");
        if let Ok(cwd) = std::env::current_dir() {
            cmd.cwd(cwd);
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("could not start the Claude sign-in flow: {e}"))?;
        drop(pair.slave);

        let killer = child.clone_killer();
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("pty reader: {e}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| format!("pty writer: {e}"))?;

        let (tx, rx) = std::sync::mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel2 = cancel.clone();
        std::thread::spawn(move || {
            // Keep master + child alive for the duration of the scrape.
            let _master = pair.master;
            let mut child = child;
            scrape_pty(reader, tx, cancel2);
            let _ = child.wait();
        });

        Ok(ConnectFlow {
            events: rx,
            writer,
            cancel,
            killer,
        })
    }

    /// Forward the authentication code the user pasted in the app to the CLI.
    pub fn submit_code(&mut self, code: &str) {
        let _ = self.writer.write_all(code.trim().as_bytes());
        let _ = self.writer.write_all(b"\r");
        let _ = self.writer.flush();
    }

    /// Abort the flow and kill the child.
    pub fn cancel(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
        let _ = self.killer.kill();
    }
}

impl Drop for ConnectFlow {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Read the PTY output, strip ANSI, and emit `ConnectEvent`s as milestones appear.
fn scrape_pty(
    mut reader: Box<dyn std::io::Read + Send>,
    tx: Sender<ConnectEvent>,
    cancel: Arc<AtomicBool>,
) {
    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    let mut sent_url = false;
    let mut sent_waiting = false;
    let mut done = false;
    let started = std::time::Instant::now();
    // Generous overall timeout: the user has to click through a browser.
    let timeout = std::time::Duration::from_secs(15 * 60);

    loop {
        if cancel.load(Ordering::SeqCst) {
            let _ = tx.send(ConnectEvent::Failed("Sign-in cancelled.".into()));
            return;
        }
        if started.elapsed() > timeout {
            let _ = tx.send(ConnectEvent::Failed(
                "The sign-in flow timed out. Try connecting again.".into(),
            ));
            return;
        }
        let n = match reader.read(&mut buf) {
            Ok(0) => break, // EOF: child exited
            Ok(n) => n,
            Err(_) => break,
        };
        raw.extend_from_slice(&buf[..n]);
        let text = strip_ansi(&String::from_utf8_lossy(&raw));

        if !sent_url {
            if let Some(url) = find_oauth_url(&text) {
                sent_url = true;
                let _ = tx.send(ConnectEvent::Url(url));
            }
        }
        if !sent_waiting && text.to_lowercase().contains("paste code here") {
            sent_waiting = true;
            let _ = tx.send(ConnectEvent::WaitingForCode);
        }
        if !done {
            if let Some(tok) = find_oauth_token(&text) {
                done = true;
                match save_token(&tok) {
                    Ok(path) => {
                        tracing::info!("claude auth: token stored at {}", path.display());
                        let _ = tx.send(ConnectEvent::TokenStored);
                    }
                    Err(e) => {
                        let _ = tx
                            .send(ConnectEvent::Failed(format!("Could not save the token: {e}")));
                    }
                }
            }
        }
    }

    if !done {
        // Child exited without producing a token.
        let text = strip_ansi(&String::from_utf8_lossy(&raw));
        let hint = if text.to_lowercase().contains("invalid") {
            "The code was not accepted. Start over and paste the fresh code."
        } else if sent_url {
            "The sign-in flow ended before a token was issued. Try connecting again."
        } else {
            "The Claude sign-in flow did not start properly. Try connecting again."
        };
        let _ = tx.send(ConnectEvent::Failed(hint.to_string()));
    }
}

/// Remove ANSI escape sequences (CSI and OSC) and carriage returns from terminal output.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            match chars.peek() {
                Some('[') => {
                    // CSI: ESC [ params final-byte (@..~)
                    chars.next();
                    for c2 in chars.by_ref() {
                        if ('\u{40}'..='\u{7e}').contains(&c2) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    // OSC: ESC ] ... (BEL or ESC \)
                    chars.next();
                    while let Some(c2) = chars.next() {
                        if c2 == '\u{07}' {
                            break;
                        }
                        if c2 == '\u{1b}' {
                            if chars.peek() == Some(&'\\') {
                                chars.next();
                            }
                            break;
                        }
                    }
                }
                _ => {
                    // Two-char escape like ESC ( B
                    chars.next();
                }
            }
        } else if c != '\r' {
            out.push(c);
        }
    }
    out
}

/// Find the OAuth authorize URL in scraped output. Tolerates arbitrary surrounding text;
/// requires an https URL that looks like an authorize link.
pub fn find_oauth_url(text: &str) -> Option<String> {
    for (i, _) in text.match_indices("https://") {
        let tail = &text[i..];
        let end = tail
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
            .unwrap_or(tail.len());
        let url = &tail[..end];
        if url.contains("oauth") || url.contains("authorize") {
            return Some(url.to_string());
        }
    }
    None
}

/// Find a long-lived OAuth token (`sk-ant-oat...`) in scraped output.
pub fn find_oauth_token(text: &str) -> Option<String> {
    let i = text.find("sk-ant-oat")?;
    let tail = &text[i..];
    let end = tail
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .unwrap_or(tail.len());
    let tok = &tail[..end];
    // A real token is long; ignore accidental short matches.
    if tok.len() > 20 {
        Some(tok.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_csi_and_osc() {
        let s = "\u{1b}[2K\u{1b}[1mBold\u{1b}[0m and \u{1b}]0;title\u{07}plain\r\n";
        assert_eq!(strip_ansi(s), "Bold and plain\n");
    }

    #[test]
    fn finds_wrapped_oauth_url() {
        let text = "Browser didn't open? Use the url below to sign in (c to copy)\n\
https://claude.com/cai/oauth/authorize?code=true&client_id=abc&state=xyz\n\nPaste code here if prompted >";
        let url = find_oauth_url(text).unwrap();
        assert!(url.starts_with("https://claude.com/cai/oauth/authorize"));
        assert!(url.ends_with("state=xyz"));
    }

    #[test]
    fn ignores_non_oauth_urls() {
        assert_eq!(find_oauth_url("see https://example.com/docs for info"), None);
    }

    #[test]
    fn finds_token_and_rejects_short_matches() {
        let text = "Your token:\nsk-ant-oat01-AbCdEf123456789_xyz-987654321\nDone.";
        assert_eq!(
            find_oauth_token(text).unwrap(),
            "sk-ant-oat01-AbCdEf123456789_xyz-987654321"
        );
        assert_eq!(find_oauth_token("sk-ant-oat "), None);
    }

    #[test]
    fn token_roundtrip_with_0600() {
        let dir = std::env::temp_dir().join(format!("bookley-auth-{}", std::process::id()));
        // Isolate paths through the env override.
        std::env::set_var("BOOKLEY_DATA_DIR", &dir);
        let tok = "sk-ant-oat01-TESTTOKEN1234567890abcdef";
        let path = save_token(tok).unwrap();
        assert_eq!(load_token().as_deref(), Some(tok));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "token file must be 0600");
        }
        delete_token();
        assert_eq!(load_token(), None);
        std::env::remove_var("BOOKLEY_DATA_DIR");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
