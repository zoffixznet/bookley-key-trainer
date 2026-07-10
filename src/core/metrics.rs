//! Typing metrics: WPM (Monkeytype 5-char convention), raw WPM, accuracy, consistency
//! (coefficient of variation of instantaneous WPM samples), per-key stats and a
//! WPM-over-time series for the results graph.
//!
//! Definitions (Monkeytype / Wikipedia standard):
//!   1 word = 5 characters (including spaces)
//!   wpm      = (correct_chars / 5) / minutes
//!   raw      = (all_typed_chars / 5) / minutes
//!   accuracy = correct_keystrokes / total_keystrokes
//!   consistency = 100 * (1 - coefficient_of_variation(instantaneous_wpm_samples)),
//!                 clamped to 0..100. CoV = stddev / mean.

use std::collections::HashMap;
use std::time::Duration;

use egui::Key;

/// A single instantaneous WPM sample taken at a moment in the session.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct WpmSample {
    /// Seconds since session start.
    pub t: f64,
    /// Instantaneous WPM at this moment (correct chars so far / 5 / minutes so far).
    pub wpm: f64,
    /// Raw WPM so far.
    pub raw: f64,
}

/// Per-key accuracy and latency accumulator, used for the heatmap and adaptive weighting.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct KeyStat {
    pub presses: u32,
    pub errors: u32,
    /// Sum of inter-keystroke latency in milliseconds (only for correct presses).
    pub total_latency_ms: f64,
    pub latency_samples: u32,
}

impl KeyStat {
    pub fn accuracy(&self) -> f64 {
        if self.presses == 0 {
            1.0
        } else {
            (self.presses - self.errors) as f64 / self.presses as f64
        }
    }
    pub fn avg_latency_ms(&self) -> f64 {
        if self.latency_samples == 0 {
            0.0
        } else {
            self.total_latency_ms / self.latency_samples as f64
        }
    }
}

/// The live metrics engine. Feed it keystroke events as they happen; read derived values
/// at any time. Time is passed in explicitly so it is fully testable without a clock.
#[derive(Debug, Clone, Default)]
pub struct Metrics {
    /// Correctly typed characters (advance the target).
    pub correct_chars: u32,
    /// Every keystroke that counts toward the target attempt (correct + incorrect).
    pub total_keystrokes: u32,
    /// Incorrect keystrokes.
    pub error_keystrokes: u32,
    /// All characters the user produced toward the target, including corrected ones.
    pub typed_chars: u32,
    /// Elapsed seconds of active typing (set by the session).
    pub elapsed_secs: f64,
    /// Instantaneous WPM samples over time.
    pub samples: Vec<WpmSample>,
    /// Per physical key stats.
    pub per_key: HashMap<Key, KeyStat>,
    last_sample_t: f64,
}

impl Metrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a keystroke against an expected physical key.
    ///
    /// `correct` = whether it matched the expected character/key.
    /// `key` = the physical key pressed (for per-key stats); `latency_ms` = time since the
    /// previous keystroke (used for per-key latency of correct presses).
    pub fn record_keystroke(&mut self, key: Option<Key>, correct: bool, latency_ms: f64) {
        self.total_keystrokes += 1;
        self.typed_chars += 1;
        if correct {
            self.correct_chars += 1;
        } else {
            self.error_keystrokes += 1;
        }
        if let Some(k) = key {
            let e = self.per_key.entry(k).or_default();
            e.presses += 1;
            if !correct {
                e.errors += 1;
            } else {
                e.total_latency_ms += latency_ms;
                e.latency_samples += 1;
            }
        }
    }

    /// Set the elapsed time and, if enough time has passed since the last sample, append a
    /// WPM-over-time sample. Call this ~1/sec and on each keystroke.
    pub fn tick(&mut self, elapsed_secs: f64) {
        self.elapsed_secs = elapsed_secs;
        // Sample at most ~5x/sec to keep the series bounded but smooth.
        if elapsed_secs - self.last_sample_t >= 0.2 || self.samples.is_empty() {
            self.last_sample_t = elapsed_secs;
            self.samples.push(WpmSample {
                t: elapsed_secs,
                wpm: self.wpm(),
                raw: self.raw_wpm(),
            });
        }
    }

    fn minutes(&self) -> f64 {
        (self.elapsed_secs / 60.0).max(1e-9)
    }

    /// Monkeytype WPM: correct chars / 5 / minutes.
    pub fn wpm(&self) -> f64 {
        (self.correct_chars as f64 / 5.0) / self.minutes()
    }

    /// Raw WPM: all typed chars / 5 / minutes.
    pub fn raw_wpm(&self) -> f64 {
        (self.typed_chars as f64 / 5.0) / self.minutes()
    }

    /// Accuracy as a fraction 0..1.
    pub fn accuracy(&self) -> f64 {
        if self.total_keystrokes == 0 {
            1.0
        } else {
            (self.total_keystrokes - self.error_keystrokes) as f64 / self.total_keystrokes as f64
        }
    }

    /// Consistency 0..100 from the coefficient of variation of instantaneous WPM samples.
    /// Uses only samples after the first second so warm-up noise does not dominate.
    pub fn consistency(&self) -> f64 {
        let vals: Vec<f64> = self
            .samples
            .iter()
            .filter(|s| s.t >= 1.0)
            .map(|s| s.wpm)
            .filter(|w| *w > 0.0)
            .collect();
        if vals.len() < 2 {
            return 100.0;
        }
        let mean = vals.iter().sum::<f64>() / vals.len() as f64;
        if mean <= 0.0 {
            return 0.0;
        }
        let var = vals.iter().map(|w| (w - mean).powi(2)).sum::<f64>() / vals.len() as f64;
        let cov = var.sqrt() / mean;
        (100.0 * (1.0 - cov)).clamp(0.0, 100.0)
    }

    /// Net WPM (exam-prep style): gross minus uncorrected errors per minute. We treat all
    /// counted error keystrokes as uncorrected for this classic figure; the primary metric
    /// stays the Monkeytype model above.
    pub fn net_wpm(&self) -> f64 {
        let gross = self.raw_wpm();
        let err_per_min = self.error_keystrokes as f64 / self.minutes();
        (gross - err_per_min).max(0.0)
    }

    /// Convenience for the smoke test / logs.
    pub fn summary_line(&self) -> String {
        format!(
            "wpm={:.1} raw={:.1} acc={:.3} consistency={:.1} chars={} errors={} secs={:.1}",
            self.wpm(),
            self.raw_wpm(),
            self.accuracy(),
            self.consistency(),
            self.correct_chars,
            self.error_keystrokes,
            self.elapsed_secs
        )
    }
}

/// Build a snapshot used by the results screen and persisted to stats.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionResult {
    pub wpm: f64,
    pub raw_wpm: f64,
    pub accuracy: f64,
    pub consistency: f64,
    pub net_wpm: f64,
    pub elapsed_secs: f64,
    pub correct_chars: u32,
    pub error_keystrokes: u32,
    pub samples: Vec<WpmSample>,
    /// Per-key errors and average latency for the heatmap, keyed by a stable label.
    pub per_key: Vec<(String, u32, u32, f64)>, // (label, presses, errors, avg_latency_ms)
    pub when: String,
    pub mode: String,
}

impl SessionResult {
    pub fn from_metrics(m: &Metrics, mode: &str) -> Self {
        let mut per_key: Vec<(String, u32, u32, f64)> = m
            .per_key
            .iter()
            .map(|(k, s)| {
                (
                    super::keys::display_name(*k),
                    s.presses,
                    s.errors,
                    s.avg_latency_ms(),
                )
            })
            .collect();
        per_key.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
        SessionResult {
            wpm: m.wpm(),
            raw_wpm: m.raw_wpm(),
            accuracy: m.accuracy(),
            consistency: m.consistency(),
            net_wpm: m.net_wpm(),
            elapsed_secs: m.elapsed_secs,
            correct_chars: m.correct_chars,
            error_keystrokes: m.error_keystrokes,
            samples: m.samples.clone(),
            per_key,
            when: now_iso(),
            mode: mode.to_string(),
        }
    }
}

pub fn now_iso() -> String {
    // Minimal ISO-ish timestamp without pulling a date crate: seconds since epoch.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs();
    format!("epoch:{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wpm_five_char_convention() {
        // 25 correct chars in 60 seconds = 25/5 / 1 min = 5 wpm.
        let mut m = Metrics::new();
        for _ in 0..25 {
            m.record_keystroke(Some(Key::A), true, 100.0);
        }
        m.tick(60.0);
        assert!((m.wpm() - 5.0).abs() < 1e-6, "wpm was {}", m.wpm());
    }

    #[test]
    fn raw_counts_all_typed() {
        let mut m = Metrics::new();
        for _ in 0..20 {
            m.record_keystroke(Some(Key::A), true, 50.0);
        }
        for _ in 0..10 {
            m.record_keystroke(Some(Key::B), false, 50.0);
        }
        m.tick(60.0);
        // correct = 20 -> wpm 4; typed = 30 -> raw 6.
        assert!((m.wpm() - 4.0).abs() < 1e-6);
        assert!((m.raw_wpm() - 6.0).abs() < 1e-6);
    }

    #[test]
    fn accuracy_all_errors() {
        let mut m = Metrics::new();
        for _ in 0..10 {
            m.record_keystroke(Some(Key::A), false, 50.0);
        }
        m.tick(30.0);
        assert_eq!(m.accuracy(), 0.0);
        assert_eq!(m.wpm(), 0.0);
        assert!(m.raw_wpm() > 0.0);
    }

    #[test]
    fn accuracy_half() {
        let mut m = Metrics::new();
        for _ in 0..5 {
            m.record_keystroke(Some(Key::A), true, 50.0);
        }
        for _ in 0..5 {
            m.record_keystroke(Some(Key::B), false, 50.0);
        }
        m.tick(30.0);
        assert!((m.accuracy() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn consistency_perfectly_even_is_high() {
        let mut m = Metrics::new();
        // Feed a steady stream: same chars per equal window.
        for step in 1..=30 {
            m.record_keystroke(Some(Key::A), true, 100.0);
            m.record_keystroke(Some(Key::A), true, 100.0);
            m.tick(step as f64); // 2 chars per second, steady
        }
        // A perfectly steady rate should score high consistency.
        assert!(m.consistency() > 80.0, "consistency={}", m.consistency());
    }

    #[test]
    fn per_key_tracks_errors_and_latency() {
        let mut m = Metrics::new();
        m.record_keystroke(Some(Key::E), true, 120.0);
        m.record_keystroke(Some(Key::E), false, 500.0);
        let s = &m.per_key[&Key::E];
        assert_eq!(s.presses, 2);
        assert_eq!(s.errors, 1);
        assert!((s.accuracy() - 0.5).abs() < 1e-9);
        assert!((s.avg_latency_ms() - 120.0).abs() < 1e-6);
    }
}
