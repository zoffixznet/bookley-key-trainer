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

    /// Inter-keystroke interval in ms, or `None` for the first keystroke of the session.
    /// `now_secs` is active (pause-adjusted) time, so paused gaps never inflate latency.
    fn latency_ms(&mut self, now_secs: f64) -> Option<f64> {
        let dt = self
            .last_keystroke_secs
            .map(|prev| ((now_secs - prev) * 1000.0).max(0.0));
        self.last_keystroke_secs = Some(now_secs);
        dt
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
        // Stats are attributed to the key that was EXPECTED at this moment, same as the
        // char path, so "errors on E" means "errors made while E was expected".
        self.apply(correct, expected.physical_key(), now_secs)
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
        // Stop-on-word: at a word boundary, block any press that is not a clean, correct
        // boundary keystroke — a wrong key must never slip past the boundary (it would
        // land an uncorrectable error there), and with unfixed errors in the word even a
        // correct boundary press blocks until the user backspaces and fixes the word.
        let word_block = self.error_mode == ErrorMode::Word
            && self.at_word_boundary()
            && (self.word_has_errors() || !correct);
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
        // Track word boundaries for stop-on-word: both boundary characters (space and
        // newline) start a fresh word, matching `at_word_boundary`.
        if matches!(
            self.target.items.get(self.cursor.wrapping_sub(1)),
            Some(Expected::Char(' ')) | Some(Expected::Char('\n'))
        ) {
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

    /// Reset the timer-dependent state (metrics, latency anchor) WITHOUT touching the
    /// typing position: cursor, per-position status, and the target stay exactly as they
    /// are. Used by the "Reset stats" control when a drill got interrupted mid-text.
    pub fn reset_metrics(&mut self) {
        self.metrics = Metrics::new();
        self.last_keystroke_secs = None;
        self.metrics.tick(0.0);
    }

    // ------- Dev-mode shortcuts -------

    /// Dev input means "register as correctly typed": clear any unfixed errors in the
    /// current word first, so stop-on-word can never block a dev keystroke (a Blocked
    /// result makes zero progress and would spin `dev_complete_all` forever).
    fn dev_clear_word_errors(&mut self) {
        for st in &mut self.status[self.word_start..self.cursor] {
            if *st == CharStatus::Wrong {
                *st = CharStatus::Correct;
            }
        }
    }

    /// Dev: register the currently-expected item as correctly typed and advance.
    pub fn dev_autotype_next(&mut self, now_secs: f64) -> Progress {
        if self.complete {
            return Progress::Complete;
        }
        self.dev_clear_word_errors();
        let phys = self.expected().and_then(|e| e.physical_key());
        self.apply(true, phys, now_secs)
    }

    /// Dev: complete the current "page" (a bounded chunk of the target) as correct.
    /// A page is ~200 items, roughly what the typing stage shows at once.
    pub fn dev_complete_page(&mut self, now_secs: f64) {
        let mut n = 0;
        while !self.complete && n < 200 {
            self.dev_autotype_next(now_secs);
            n += 1;
        }
    }

    /// Dev: complete the whole current target (chapter / full text) as correct.
    pub fn dev_complete_all(&mut self, now_secs: f64) {
        while !self.complete {
            self.dev_autotype_next(now_secs);
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

    /// A wrong keystroke where the SPACE itself is expected must block (not advance):
    /// letting it through would land an error on the boundary that backspace can never
    /// reach once word_start moves past it.
    #[test]
    fn stop_on_word_blocks_wrong_key_at_the_boundary() {
        let mut s = Session::new(Target::from_text("ab cd", "t"), ErrorMode::Word);
        assert_eq!(s.input_char('a', 0.1), Progress::Advanced);
        assert_eq!(s.input_char('b', 0.2), Progress::Advanced);
        // Word is clean; a wrong key at the expected space is blocked, not advanced.
        assert_eq!(s.input_char('x', 0.3), Progress::Blocked);
        assert_eq!(s.cursor, 2, "cursor must not cross the boundary");
        // The correct space then advances normally.
        assert_eq!(s.input_char(' ', 0.4), Progress::Advanced);
        assert_eq!(s.input_char('c', 0.5), Progress::Advanced);
        assert_eq!(s.input_char('d', 0.6), Progress::Complete);
    }

    /// Newlines are word boundaries too: an error on the previous line must not bleed
    /// into (and block) a cleanly typed word on the next line.
    #[test]
    fn stop_on_word_resets_word_start_at_newlines() {
        let mut s = Session::new(Target::from_text("ab\ncd ef", "t"), ErrorMode::Word);
        assert_eq!(s.input_char('a', 0.1), Progress::Advanced);
        assert_eq!(s.input_char('b', 0.2), Progress::Advanced);
        assert_eq!(s.input_char('\n', 0.3), Progress::Advanced);
        // An error inside "cd" is correctable without crossing the newline...
        assert_eq!(s.input_char('x', 0.4), Progress::AdvancedWithError);
        s.backspace();
        assert_eq!(s.cursor, 3, "backspace stops at the newline word start");
        // ...and a cleanly typed "cd" passes its trailing space unblocked.
        assert_eq!(s.input_char('c', 0.5), Progress::Advanced);
        assert_eq!(s.input_char('d', 0.6), Progress::Advanced);
        assert_eq!(s.input_char(' ', 0.7), Progress::Advanced);
        assert_eq!(s.input_char('e', 0.8), Progress::Advanced);
        assert_eq!(s.input_char('f', 0.9), Progress::Complete);
    }

    /// F12 (complete chapter) in stop-on-word mode with an unfixed error must terminate:
    /// the dev path registers everything as correctly typed instead of spinning on the
    /// word-boundary block forever (a UI-thread hang).
    #[test]
    fn dev_complete_all_finishes_past_a_word_block() {
        let mut s = Session::new(Target::from_text("ab cd", "t"), ErrorMode::Word);
        assert_eq!(s.input_char('x', 0.1), Progress::AdvancedWithError);
        assert_eq!(s.input_char('b', 0.2), Progress::Advanced);
        s.dev_complete_all(1.0);
        assert!(s.is_complete(), "dev complete must not spin forever");
        assert!(!s.status.contains(&CharStatus::Wrong));
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

    /// A known keystroke timeline in, expected per-key stats out:
    ///   t=1.0  'a' correct   (first keystroke: no latency sample)
    ///   t=1.4  'z' wrong     (expected 'b': error booked on B, no latency)
    ///   t=1.9  'b' correct   (latency 500ms from the previous keystroke, booked on B)
    #[test]
    fn per_key_stats_from_keystroke_timeline() {
        let mut s = Session::new(Target::from_text("ab", "t"), ErrorMode::Letter);
        assert_eq!(s.input_char('a', 1.0), Progress::Advanced);
        assert_eq!(s.input_char('z', 1.4), Progress::Blocked);
        assert_eq!(s.input_char('b', 1.9), Progress::Complete);

        let a = &s.metrics.per_key[&Key::A];
        assert_eq!((a.presses, a.errors, a.latency_samples), (1, 0, 0));
        assert_eq!(a.avg_latency_ms(), None, "first keystroke has no interval");

        let b = &s.metrics.per_key[&Key::B];
        assert_eq!((b.presses, b.errors), (2, 1), "one error, exactly once");
        assert_eq!(b.latency_samples, 1, "only the correct press measures");
        assert!((b.avg_latency_ms().unwrap() - 500.0).abs() < 1e-6);

        // The wrong press was attributed to the EXPECTED key (B), not the pressed key (Z).
        assert!(!s.metrics.per_key.contains_key(&Key::Z));
        assert_eq!(s.metrics.error_keystrokes, 1);
        assert_eq!(s.metrics.total_keystrokes, 3);
    }

    /// Same attribution rule on the physical-key path (Random mode): a wrong press is an
    /// error on the key that was expected, and its correct press carries the latency.
    #[test]
    fn physical_key_stats_attributed_to_expected() {
        let mut s = Session::new(
            Target::from_keys(vec![Key::ArrowUp, Key::PageDown], "r"),
            ErrorMode::Letter,
        );
        assert_eq!(s.input_physical_key(Key::ArrowUp, 1.0), Progress::Advanced);
        // Wrong key while PageDown is expected: error on PageDown, none on Escape.
        assert_eq!(s.input_physical_key(Key::Escape, 1.3), Progress::Blocked);
        assert_eq!(s.input_physical_key(Key::PageDown, 1.8), Progress::Complete);
        let pd = &s.metrics.per_key[&Key::PageDown];
        assert_eq!((pd.presses, pd.errors, pd.latency_samples), (2, 1, 1));
        assert!((pd.avg_latency_ms().unwrap() - 500.0).abs() < 1e-6);
        assert!(!s.metrics.per_key.contains_key(&Key::Escape));
    }

    /// Backspace is an ordinary drillable target on the physical-key path (Random mode).
    #[test]
    fn backspace_is_a_drillable_target() {
        let mut s = Session::new(
            Target::from_keys(vec![Key::Backspace, Key::A], "r"),
            ErrorMode::Letter,
        );
        assert_eq!(
            s.input_physical_key(Key::Backspace, 0.5),
            Progress::Advanced
        );
        assert_eq!(s.input_physical_key(Key::A, 1.0), Progress::Complete);
        assert_eq!(s.metrics.error_keystrokes, 0);
    }

    /// Reset stats zeroes the metrics but never the typing position.
    #[test]
    fn reset_metrics_keeps_position() {
        let mut s = Session::new(Target::from_text("abcd", "t"), ErrorMode::Off);
        s.input_char('a', 1.0);
        s.input_char('x', 2.0); // wrong at 'b'
        assert_eq!(s.cursor, 2);
        assert_eq!(s.metrics.error_keystrokes, 1);

        s.reset_metrics();
        assert_eq!(s.cursor, 2, "position survives the reset");
        assert_eq!(s.status[0], CharStatus::Correct);
        assert_eq!(s.status[1], CharStatus::Wrong);
        assert_eq!(s.metrics.total_keystrokes, 0);
        assert_eq!(s.metrics.error_keystrokes, 0);
        assert!(s.metrics.per_key.is_empty());

        // Typing continues from the same spot; the first post-reset keystroke has no
        // latency sample (the timing anchor was reset too).
        assert_eq!(s.input_char('c', 3.0), Progress::Advanced);
        assert_eq!(s.metrics.per_key[&Key::C].latency_samples, 0);
        assert_eq!(s.input_char('d', 3.5), Progress::Complete);
        assert_eq!(s.metrics.correct_chars, 2);
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
