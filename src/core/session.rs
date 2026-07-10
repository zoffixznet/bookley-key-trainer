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

/// Pause bookkeeping over an injected wall clock: `raw_secs` is seconds since session
/// start by the wall; `active_secs` subtracts every paused gap, so paused time never
/// counts against WPM or consistency. Pure and fully testable.
#[derive(Debug, Clone, Default)]
pub struct PauseClock {
    paused_total: f64,
    pause_started: Option<f64>,
}

impl PauseClock {
    pub fn is_paused(&self) -> bool {
        self.pause_started.is_some()
    }

    pub fn pause(&mut self, raw_secs: f64) {
        if self.pause_started.is_none() {
            self.pause_started = Some(raw_secs);
        }
    }

    pub fn resume(&mut self, raw_secs: f64) {
        if let Some(p) = self.pause_started.take() {
            self.paused_total += (raw_secs - p).max(0.0);
        }
    }

    /// Active (unpaused) seconds elapsed, given the raw wall seconds.
    pub fn active_secs(&self, raw_secs: f64) -> f64 {
        let current_gap = self
            .pause_started
            .map(|p| (raw_secs - p).max(0.0))
            .unwrap_or(0.0);
        (raw_secs - self.paused_total - current_gap).max(0.0)
    }
}

pub struct Session {
    pub target: Target,
    pub status: Vec<CharStatus>,
    pub cursor: usize,
    pub metrics: Metrics,
    pub error_mode: ErrorMode,
    /// For timed drills: the session ends when active time reaches this many seconds.
    pub time_limit_secs: Option<f64>,
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
            time_limit_secs: None,
            word_start: 0,
            last_keystroke_secs: None,
            complete: n == 0,
        }
    }

    /// A timed drill: ends when `limit_secs` of active typing time have elapsed.
    pub fn with_time_limit(target: Target, error_mode: ErrorMode, limit_secs: f64) -> Self {
        let mut s = Self::new(target, error_mode);
        s.time_limit_secs = Some(limit_secs.max(1.0));
        s
    }

    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// Whether the drill timer has run out (always false for untimed sessions).
    pub fn time_up(&self, active_secs: f64) -> bool {
        self.time_limit_secs.is_some_and(|l| active_secs >= l)
    }

    /// Seconds left on the drill timer, if this session is timed.
    pub fn time_left(&self, active_secs: f64) -> Option<f64> {
        self.time_limit_secs.map(|l| (l - active_secs).max(0.0))
    }

    /// Append more items to the target (streaming word/key drills refill near the end).
    pub fn extend_target(&mut self, extra: Vec<Expected>) {
        if extra.is_empty() {
            return;
        }
        self.status
            .extend(std::iter::repeat_n(CharStatus::Pending, extra.len()));
        self.target.items.extend(extra);
        self.complete = false;
    }

    /// How many items remain after the cursor (for refill decisions).
    pub fn items_remaining(&self) -> usize {
        self.target.len().saturating_sub(self.cursor)
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

    /// Whether the cursor is on a word boundary (space / newline).
    fn at_word_boundary(&self) -> bool {
        matches!(
            self.target.items.get(self.cursor),
            Some(Expected::Char(' ')) | Some(Expected::Char('\n'))
        )
    }

    /// Whether the current word (word_start..cursor) contains any wrong positions.
    fn word_has_errors(&self) -> bool {
        self.status[self.word_start..self.cursor].contains(&CharStatus::Wrong)
    }

    fn apply(&mut self, correct: bool, phys: Option<Key>, now_secs: f64) -> Progress {
        // Stop-on-word: at a word boundary with unfixed errors in the word, block even a
        // correct boundary press; the user must backspace and fix the word first.
        let word_block =
            self.error_mode == ErrorMode::Word && self.at_word_boundary() && self.word_has_errors();
        let counted_correct = correct && !word_block;

        let latency = self.latency_ms(now_secs);
        self.metrics
            .record_keystroke(phys, counted_correct, latency);
        self.metrics.tick(now_secs);

        if word_block {
            return Progress::Blocked;
        }

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
                // Type-through, and stop-on-word within a word: mark wrong and advance.
                // (Word mode enforces correction at the word boundary above.)
                ErrorMode::Off | ErrorMode::Word => {
                    self.status[self.cursor] = CharStatus::Wrong;
                    self.advance();
                    if self.complete {
                        Progress::Complete
                    } else {
                        Progress::AdvancedWithError
                    }
                }
                ErrorMode::Letter => {
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

    /// Dev: complete the current "page" (a bounded chunk of the target) as correct.
    /// A page is ~200 items, roughly what the typing stage shows at once.
    pub fn dev_complete_page(&mut self, now_secs: f64) {
        let mut n = 0;
        while !self.complete && n < 200 {
            let phys = self.expected().and_then(|e| e.physical_key());
            self.apply(true, phys, now_secs);
            n += 1;
        }
    }

    /// Dev: complete the whole current target (chapter / full text) as correct.
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
    fn stop_on_word_blocks_boundary_until_fixed() {
        let mut s = Session::new(Target::from_text("ab cd", "t"), ErrorMode::Word);
        // Wrong first letter advances within the word (unlike Letter mode).
        assert_eq!(s.input_char('x', 0.1), Progress::AdvancedWithError);
        assert_eq!(s.input_char('b', 0.2), Progress::Advanced);
        // At the space boundary with an unfixed error: blocked, even though space is right.
        assert_eq!(s.input_char(' ', 0.3), Progress::Blocked);
        // Fix the word: backspace twice, retype correctly.
        s.backspace();
        s.backspace();
        assert_eq!(s.input_char('a', 0.4), Progress::Advanced);
        assert_eq!(s.input_char('b', 0.5), Progress::Advanced);
        assert_eq!(s.input_char(' ', 0.6), Progress::Advanced);
        assert_eq!(s.input_char('c', 0.7), Progress::Advanced);
        assert_eq!(s.input_char('d', 0.8), Progress::Complete);
    }

    #[test]
    fn dev_complete_page_is_bounded() {
        let text = "x".repeat(500);
        let mut s = Session::new(Target::from_text(&text, "t"), ErrorMode::Off);
        s.dev_complete_page(1.0);
        assert!(!s.is_complete());
        assert_eq!(s.cursor, 200);
        s.dev_complete_all(2.0);
        assert!(s.is_complete());
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
        let mut s = Session::new(
            Target::from_text("a long chapter of text", "t"),
            ErrorMode::Off,
        );
        s.dev_complete_all(1.0);
        assert!(s.is_complete());
        assert_eq!(
            s.metrics.correct_chars as usize,
            "a long chapter of text".len()
        );
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

    /// A pause must not change WPM, accuracy, or consistency: the paused gap is
    /// subtracted from the clock, so the metrics see identical active timestamps.
    #[test]
    fn pause_does_not_change_metrics() {
        let type_chars = |s: &mut Session, clock: &PauseClock, raws: &[f64]| {
            for &raw in raws {
                s.input_char('a', clock.active_secs(raw));
            }
        };

        // Control run: 10 keystrokes, one per second, no pause.
        let mut control = Session::new(Target::from_text(&"a".repeat(10), "t"), ErrorMode::Off);
        let idle = PauseClock::default();
        let raws: Vec<f64> = (1..=10).map(|i| i as f64).collect();
        type_chars(&mut control, &idle, &raws);
        control.metrics.tick(idle.active_secs(10.0));

        // Paused run: same cadence, but a 60-second pause after the 5th keystroke.
        let mut paused = Session::new(Target::from_text(&"a".repeat(10), "t"), ErrorMode::Off);
        let mut clock = PauseClock::default();
        type_chars(&mut paused, &clock, &[1.0, 2.0, 3.0, 4.0, 5.0]);
        clock.pause(5.2);
        // While paused, active time is frozen.
        assert!((clock.active_secs(30.0) - 5.2).abs() < 1e-9);
        assert!(clock.is_paused());
        clock.resume(65.2);
        assert!(!clock.is_paused());
        // Wall time is now shifted by exactly 60s; active time continues from 5.2.
        type_chars(&mut paused, &clock, &[66.0, 67.0, 68.0, 69.0, 70.0]);
        paused.metrics.tick(clock.active_secs(70.0));

        assert!((paused.metrics.elapsed_secs - control.metrics.elapsed_secs).abs() < 1e-9);
        assert!((paused.metrics.wpm() - control.metrics.wpm()).abs() < 1e-9);
        assert!((paused.metrics.consistency() - control.metrics.consistency()).abs() < 1e-9);
        assert_eq!(paused.metrics.correct_chars, control.metrics.correct_chars);
    }

    #[test]
    fn timed_session_reports_time_up_and_left() {
        let s = Session::with_time_limit(Target::from_text("abc", "t"), ErrorMode::Off, 30.0);
        assert!(!s.time_up(29.9));
        assert!(s.time_up(30.0));
        assert_eq!(s.time_left(10.0), Some(20.0));
        let untimed = Session::new(Target::from_text("abc", "t"), ErrorMode::Off);
        assert!(!untimed.time_up(1e9));
        assert_eq!(untimed.time_left(5.0), None);
    }

    #[test]
    fn extend_target_refills_a_stream() {
        let mut s = Session::new(Target::from_text("ab", "t"), ErrorMode::Off);
        assert_eq!(s.input_char('a', 0.1), Progress::Advanced);
        assert_eq!(s.items_remaining(), 1);
        s.extend_target("cd".chars().map(Expected::Char).collect());
        assert_eq!(s.items_remaining(), 3);
        assert_eq!(s.input_char('b', 0.2), Progress::Advanced);
        assert_eq!(s.input_char('c', 0.3), Progress::Advanced);
        assert_eq!(s.input_char('d', 0.4), Progress::Complete);
    }
}
