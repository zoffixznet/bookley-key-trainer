//! The typing session state machine. Holds a `Target`, a cursor, per-position status,
//! the metrics engine, the error-handling policy, and the dev-mode shortcut behaviors.
//!
//! Time is injected (seconds since start) so the whole thing is testable without a clock.

use egui::Key;

use super::config::ErrorMode;
use super::metrics::Metrics;
use super::text_source::{Expected, Target};

/// Status of one target position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharStatus {
    Pending,
    Correct,
    /// Typed wrong (in type-through mode we advance but remember it was wrong).
    Wrong,
}

/// Result of feeding a key/char to the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// Advanced to the next position.
    Advanced,
    /// Wrong input, blocked here (stop-on-error).
    Blocked,
    /// Wrong input, marked and advanced (type-through).
    AdvancedWithError,
    /// The whole target is complete.
    Complete,
    /// Input was ignored (e.g. a modifier alone).
    Ignored,
}

pub struct Session {
    pub target: Target,
    pub status: Vec<CharStatus>,
    pub cursor: usize,
    pub metrics: Metrics,
    pub error_mode: ErrorMode,
    /// Start of the current word (for stop-on-word correction).
    word_start: usize,
    /// Seconds since session start at the previous keystroke, for latency.
    last_keystroke_secs: Option<f64>,
    /// Whether the target has been completed.
    complete: bool,
}

impl Session {
    pub fn new(target: Target, error_mode: ErrorMode) -> Self {
        let n = target.len();
        Session {
            target,
            status: vec![CharStatus::Pending; n],
            cursor: 0,
            metrics: Metrics::new(),
            error_mode,
            word_start: 0,
            last_keystroke_secs: None,
            complete: n == 0,
        }
    }

    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// The current expected item, if any.
    pub fn expected(&self) -> Option<&Expected> {
        self.target.items.get(self.cursor)
    }

    /// The physical key to highlight next (Guide mode).
    pub fn next_physical_key(&self) -> Option<Key> {
        self.expected().and_then(|e| e.physical_key())
    }

    /// Fraction of the target completed, 0..1.
    pub fn progress_fraction(&self) -> f32 {
        if self.target.is_empty() {
            return 1.0;
        }
        self.cursor as f32 / self.target.len() as f32
    }

    fn latency_ms(&mut self, now_secs: f64) -> f64 {
        let dt = match self.last_keystroke_secs {
            Some(prev) => (now_secs - prev) * 1000.0,
            None => 0.0,
        };
        self.last_keystroke_secs = Some(now_secs);
        dt.max(0.0)
    }

    /// Feed a produced character (from egui `Event::Text` or a physical char). `now_secs`
    /// is the elapsed session time. Returns how progress changed.
    pub fn input_char(&mut self, c: char, now_secs: f64) -> Progress {
        if self.complete {
            return Progress::Complete;
        }
        let expected = match self.target.items.get(self.cursor) {
            Some(e) => e.clone(),
            None => return Progress::Complete,
        };
        let correct = matches!(&expected, Expected::Char(ec) if *ec == c);
        let phys = expected.physical_key();
        self.apply(correct, phys, now_secs)
    }

    /// Feed a physical key press (for non-character targets in Random mode, and as a
    /// fallback for character targets when no Text event is produced, e.g. Enter/Tab).
    pub fn input_physical_key(&mut self, key: Key, now_secs: f64) -> Progress {
        if self.complete {
            return Progress::Complete;
        }
        let expected = match self.target.items.get(self.cursor) {
            Some(e) => e.clone(),
            None => return Progress::Complete,
        };
        // Only meaningful when the expected item is a physical key, or a char whose only
        // sensible input is a bare key (space/enter/tab handled by callers too).
        let correct = expected.physical_key() == Some(key);
        self.apply(correct, Some(key), now_secs)
    }

    fn apply(&mut self, correct: bool, phys: Option<Key>, now_secs: f64) -> Progress {
        let latency = self.latency_ms(now_secs);
        self.metrics.record_keystroke(phys, correct, latency);
        self.metrics.tick(now_secs);

        if correct {
            self.status[self.cursor] = CharStatus::Correct;
            self.advance();
            if self.complete {
                Progress::Complete
            } else {
                Progress::Advanced
            }
        } else {
            match self.error_mode {
                ErrorMode::Off => {
                    self.status[self.cursor] = CharStatus::Wrong;
                    self.advance();
                    if self.complete {
                        Progress::Complete
                    } else {
                        Progress::AdvancedWithError
                    }
                }
                ErrorMode::Letter | ErrorMode::Word => {
                    // Mark wrong but do not advance; user must produce the right key.
                    self.status[self.cursor] = CharStatus::Wrong;
                    Progress::Blocked
                }
            }
        }
    }

    fn advance(&mut self) {
        self.cursor += 1;
        // Track word boundaries for stop-on-word (space or physical-key boundary).
        if let Some(Expected::Char(' ')) = self.target.items.get(self.cursor.wrapping_sub(1)) {
            self.word_start = self.cursor;
        }
        if self.cursor >= self.target.len() {
            self.complete = true;
        }
    }

    /// Backspace: step back one position if allowed. Used for corrections in stop-on-word
    /// and generally to let users fix mistakes. Does not un-count keystrokes (metrics keep
    /// the attempt) but resets the position status.
    pub fn backspace(&mut self) {
        if self.cursor > self.word_start && self.cursor > 0 {
            self.cursor -= 1;
            self.status[self.cursor] = CharStatus::Pending;
            self.complete = false;
        }
    }

    // ------- Dev-mode shortcuts -------

    /// Dev: register the currently-expected item as correctly typed and advance.
    pub fn dev_autotype_next(&mut self, now_secs: f64) -> Progress {
        if self.complete {
            return Progress::Complete;
        }
        let phys = self.expected().and_then(|e| e.physical_key());
        self.apply(true, phys, now_secs)
    }

    /// Dev: complete the current "page" (here: the whole current target) as correct.
    /// For the app, one target is a page; the chapter completion below handles multi-page.
    pub fn dev_complete_all(&mut self, now_secs: f64) {
        while !self.complete {
            let phys = self.expected().and_then(|e| e.physical_key());
            self.apply(true, phys, now_secs);
        }
    }

    /// A quick assertable summary.
    pub fn summary(&self) -> String {
        self.metrics.summary_line()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::text_source::Target;

    #[test]
    fn type_through_marks_errors_and_advances() {
        let mut s = Session::new(Target::from_text("ab", "t"), ErrorMode::Off);
        assert_eq!(s.input_char('x', 0.1), Progress::AdvancedWithError);
        assert_eq!(s.status[0], CharStatus::Wrong);
        assert_eq!(s.input_char('b', 0.2), Progress::Complete);
        assert_eq!(s.metrics.error_keystrokes, 1);
        assert_eq!(s.metrics.correct_chars, 1);
    }

    #[test]
    fn stop_on_letter_blocks_until_correct() {
        let mut s = Session::new(Target::from_text("ab", "t"), ErrorMode::Letter);
        assert_eq!(s.input_char('x', 0.1), Progress::Blocked);
        assert_eq!(s.cursor, 0);
        assert_eq!(s.input_char('a', 0.2), Progress::Advanced);
        assert_eq!(s.cursor, 1);
    }

    #[test]
    fn dev_autotype_advances_correctly() {
        let mut s = Session::new(Target::from_text("hello", "t"), ErrorMode::Letter);
        for i in 0..5 {
            let p = s.dev_autotype_next(i as f64 * 0.1);
            if i < 4 {
                assert_eq!(p, Progress::Advanced);
            } else {
                assert_eq!(p, Progress::Complete);
            }
        }
        assert!(s.is_complete());
        assert_eq!(s.metrics.correct_chars, 5);
        assert_eq!(s.metrics.error_keystrokes, 0);
    }

    #[test]
    fn dev_complete_all_finishes_everything() {
        let mut s = Session::new(Target::from_text("a long chapter of text", "t"), ErrorMode::Off);
        s.dev_complete_all(1.0);
        assert!(s.is_complete());
        assert_eq!(s.metrics.correct_chars as usize, "a long chapter of text".len());
    }

    #[test]
    fn physical_key_target_random() {
        let mut s = Session::new(
            Target::from_keys(vec![Key::ArrowUp, Key::F1], "r"),
            ErrorMode::Letter,
        );
        assert_eq!(s.input_physical_key(Key::ArrowDown, 0.1), Progress::Blocked);
        assert_eq!(s.input_physical_key(Key::ArrowUp, 0.2), Progress::Advanced);
        assert_eq!(s.input_physical_key(Key::F1, 0.3), Progress::Complete);
    }

    #[test]
    fn empty_target_is_immediately_complete() {
        let s = Session::new(Target::from_text("", "t"), ErrorMode::Off);
        assert!(s.is_complete());
    }
}
