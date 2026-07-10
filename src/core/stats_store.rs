//! Personal-best and session-history persistence (JSON under the data dir). Kept small:
//! a rolling history plus a per-mode best WPM.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::metrics::SessionResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Stats {
    /// Best WPM per mode label.
    pub best_wpm: HashMap<String, f64>,
    /// Recent sessions (most recent last), capped.
    pub history: Vec<SessionResult>,
}

impl Stats {
    pub fn load_from(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Stats::default(),
        }
    }

    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let s = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, s)
    }

    /// Record a finished session. Returns true if it set a new personal best for its mode.
    pub fn record(&mut self, result: SessionResult) -> bool {
        let is_pb = self
            .best_wpm
            .get(&result.mode)
            .map(|b| result.wpm > *b)
            .unwrap_or(true);
        if is_pb {
            self.best_wpm.insert(result.mode.clone(), result.wpm);
        }
        self.history.push(result);
        // Cap history to a sane size.
        let cap = 200;
        if self.history.len() > cap {
            let excess = self.history.len() - cap;
            self.history.drain(0..excess);
        }
        is_pb
    }

    pub fn best_for(&self, mode: &str) -> Option<f64> {
        self.best_wpm.get(mode).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::metrics::Metrics;
    use egui::Key;

    fn result(wpm_chars: u32, mode: &str) -> SessionResult {
        let mut m = Metrics::new();
        for _ in 0..wpm_chars {
            m.record_keystroke(Some(Key::A), true, 100.0);
        }
        m.tick(60.0);
        SessionResult::from_metrics(&m, mode)
    }

    #[test]
    fn tracks_personal_best() {
        let mut s = Stats::default();
        assert!(s.record(result(50, "word")));
        assert!(!s.record(result(25, "word"))); // lower, not a PB
        assert!(s.record(result(100, "word"))); // higher, PB
        assert!((s.best_for("word").unwrap() - (100.0 / 5.0)).abs() < 1e-6);
    }

    #[test]
    fn roundtrip_json() {
        let mut s = Stats::default();
        s.record(result(30, "random"));
        let dir = std::env::temp_dir().join(format!("bookley-stats-{}", std::process::id()));
        let path = dir.join("stats.json");
        s.save_to(&path).unwrap();
        let back = Stats::load_from(&path);
        assert_eq!(back.history.len(), 1);
        assert!(back.best_for("random").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
