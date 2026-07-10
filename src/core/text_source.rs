//! Text sources: where the text-to-type comes from. Random keys, single word, pasted
//! text, and a book chapter all reduce to a target the session consumes.
//!
//! Two shapes of target exist:
//!   - a plain string the user types character by character (Word / Paste / Book), and
//!   - a sequence of physical keys to press (Random keys, which includes non-character
//!     keys like arrows and function keys that have no character).
//!
//! `Target` unifies both: it is an ordered list of `Expected` items, each either a
//! character or a bare physical key.

use egui::Key;

use super::keys;

/// One expected item in a target sequence.
#[derive(Debug, Clone, PartialEq)]
pub enum Expected {
    /// A printable character to type. The physical key and shift are derived on demand.
    Char(char),
    /// A bare physical key to press (no character), e.g. arrows / function keys.
    PhysicalKey(Key),
}

impl Expected {
    /// The physical key that satisfies this item, if determinable.
    pub fn physical_key(&self) -> Option<Key> {
        match self {
            Expected::Char(c) => keys::char_to_physical(*c).map(|(k, _)| k),
            Expected::PhysicalKey(k) => Some(*k),
        }
    }

    /// Whether Shift is needed (only meaningful for characters).
    pub fn needs_shift(&self) -> bool {
        match self {
            Expected::Char(c) => keys::char_to_physical(*c).map(|(_, s)| s).unwrap_or(false),
            Expected::PhysicalKey(_) => false,
        }
    }

    /// A short label for the HUD / next-key prompt.
    pub fn label(&self) -> String {
        match self {
            Expected::Char(' ') => "␣".to_string(),
            Expected::Char('\n') => "⏎".to_string(),
            Expected::Char(c) => c.to_string(),
            Expected::PhysicalKey(k) => keys::display_name(*k),
        }
    }

    /// Whether this is a character item (contributes to the visible text strip).
    pub fn is_char(&self) -> bool {
        matches!(self, Expected::Char(_))
    }
}

/// A full target the session will make the user reproduce.
#[derive(Debug, Clone)]
pub struct Target {
    pub items: Vec<Expected>,
    /// A human-readable title for the HUD (e.g. the word, "Random keys", chapter title).
    pub title: String,
}

impl Target {
    pub fn from_text(text: &str, title: impl Into<String>) -> Self {
        let items = text.chars().map(Expected::Char).collect();
        Target {
            items,
            title: title.into(),
        }
    }

    pub fn from_keys(keys: Vec<Key>, title: impl Into<String>) -> Self {
        Target {
            items: keys.into_iter().map(Expected::PhysicalKey).collect(),
            title: title.into(),
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The visible text (characters only) for the typing strip.
    pub fn visible_text(&self) -> String {
        self.items
            .iter()
            .map(|e| match e {
                Expected::Char(c) => *c,
                Expected::PhysicalKey(_) => '\u{25A1}', // a box glyph placeholder
            })
            .collect()
    }
}

/// Something that yields targets. `next_target` produces the next chunk to type.
pub trait TextSource {
    fn next_target(&mut self) -> Target;
    /// Human label for stats / logs.
    fn mode_label(&self) -> &'static str;
}

/// Random-keys source. Draws from the full physical keyboard (letters, digits,
/// punctuation, arrows, function keys, nav cluster), excluding the dev shortcut keys.
pub struct RandomSource {
    pool: Vec<Key>,
    round_len: usize,
    /// Optional per-key weights for adaptive drilling (higher = more likely).
    weights: Option<Vec<f32>>,
}

impl RandomSource {
    pub fn new(round_len: usize) -> Self {
        let pool: Vec<Key> = keys::all_keys()
            .into_iter()
            .filter(|k| !keys::DEV_KEYS.contains(k))
            .collect();
        RandomSource {
            pool,
            round_len: round_len.max(1),
            weights: None,
        }
    }

    /// Set adaptive weights aligned to `pool` order. Weak keys should get higher weights.
    pub fn set_weights(&mut self, weights: Vec<f32>) {
        if weights.len() == self.pool.len() {
            self.weights = Some(weights);
        }
    }

    pub fn pool(&self) -> &[Key] {
        &self.pool
    }

    /// A fresh batch of `n` keys, honoring the adaptive weights (for stream refills).
    pub fn batch(&self, n: usize) -> Vec<Key> {
        let mut rng = rand::rng();
        (0..n).map(|_| self.pick(&mut rng)).collect()
    }

    fn pick(&self, rng: &mut impl rand::RngExt) -> Key {
        match &self.weights {
            Some(w) => {
                let total: f32 = w.iter().sum();
                if total <= 0.0 {
                    return self.pool[rng.random_range(0..self.pool.len())];
                }
                let mut r = rng.random_range(0.0..total);
                for (i, wi) in w.iter().enumerate() {
                    r -= *wi;
                    if r <= 0.0 {
                        return self.pool[i];
                    }
                }
                *self.pool.last().unwrap()
            }
            None => self.pool[rng.random_range(0..self.pool.len())],
        }
    }
}

impl TextSource for RandomSource {
    fn next_target(&mut self) -> Target {
        Target::from_keys(self.batch(self.round_len), "Random keys")
    }
    fn mode_label(&self) -> &'static str {
        "random"
    }
}

/// Word-drill source over the bundled list: yields a flowing stream of words.
pub struct WordSource {
    list: super::wordlist::WordList,
}

impl Default for WordSource {
    fn default() -> Self {
        Self::new()
    }
}

impl WordSource {
    pub fn new() -> Self {
        WordSource {
            list: super::wordlist::WordList::new(),
        }
    }

    /// `n` random words joined by single spaces, as extendable target items.
    pub fn batch(&self, n: usize) -> Vec<Expected> {
        let mut items = Vec::new();
        for i in 0..n.max(1) {
            if i > 0 {
                items.push(Expected::Char(' '));
            }
            items.extend(self.list.random().chars().map(Expected::Char));
        }
        items
    }

    /// A stream target of `n` words for the timed drill.
    pub fn stream_target(&self, n: usize) -> Target {
        Target {
            items: self.batch(n),
            title: "Word drill".to_string(),
        }
    }
}

impl TextSource for WordSource {
    fn next_target(&mut self) -> Target {
        self.stream_target(200)
    }
    fn mode_label(&self) -> &'static str {
        "word"
    }
}

/// Paste source: reproduces the text the user provided, normalized to a plain-ASCII
/// typing target (accents folded, smart punctuation straightened, exotic spaces mapped).
pub struct PasteSource {
    text: String,
    served: bool,
}

impl PasteSource {
    pub fn new(text: String) -> Self {
        PasteSource {
            text: super::normalize::normalize_target(&text),
            served: false,
        }
    }
}

impl TextSource for PasteSource {
    fn next_target(&mut self) -> Target {
        self.served = true;
        Target::from_text(&self.text, "Pasted text")
    }
    fn mode_label(&self) -> &'static str {
        "paste"
    }
}

/// Book source: serves a single chapter's plain text as the target.
pub struct BookSource {
    text: String,
    title: String,
}

impl BookSource {
    /// The chapter file on disk keeps its original Unicode; the typing target is
    /// normalized so every character is reachable on a plain keyboard.
    pub fn new(text: String, title: String) -> Self {
        BookSource {
            text: super::normalize::normalize_target(&text),
            title,
        }
    }
}

impl TextSource for BookSource {
    fn next_target(&mut self) -> Target {
        Target::from_text(&self.text, self.title.clone())
    }
    fn mode_label(&self) -> &'static str {
        "book"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_excludes_dev_keys() {
        let src = RandomSource::new(10);
        for d in keys::DEV_KEYS {
            assert!(!src.pool().contains(&d), "dev key {d:?} leaked into pool");
        }
        // Pool still has plenty of keys including non-character ones. Backspace is an
        // ordinary drillable key in Random mode.
        assert!(src.pool().contains(&Key::A));
        assert!(src.pool().contains(&Key::ArrowUp));
        assert!(src.pool().contains(&Key::Backspace));
    }

    #[test]
    fn random_honors_round_len_and_pool() {
        let mut src = RandomSource::new(25);
        let t = src.next_target();
        assert_eq!(t.len(), 25);
        for item in &t.items {
            let k = item.physical_key().unwrap();
            assert!(src.pool().contains(&k));
        }
    }

    #[test]
    fn word_source_streams_many_words() {
        let src = WordSource::new();
        let t = src.stream_target(50);
        assert!(t.items.iter().all(|e| e.is_char()));
        let text = t.visible_text();
        assert_eq!(text.split(' ').count(), 50);
        assert!(!text.contains("  "), "single spaces between words");
        // Batches are extendable stream chunks.
        let extra = src.batch(10);
        assert!(!extra.is_empty());
    }

    #[test]
    fn paste_reproduces_ascii_exactly_and_normalizes_unicode() {
        let input = "Hello, World! 123";
        let mut src = PasteSource::new(input.to_string());
        assert_eq!(src.next_target().visible_text(), input);

        // Unicode input is normalized into a typeable target.
        let mut src = PasteSource::new("Émile heard it too — a third key".to_string());
        assert_eq!(
            src.next_target().visible_text(),
            "Emile heard it too - a third key"
        );
    }

    #[test]
    fn book_source_normalizes_target() {
        let mut src = BookSource::new("“Café—now…”".to_string(), "t".into());
        assert_eq!(src.next_target().visible_text(), "\"Cafe-now...\"");
    }

    #[test]
    fn adaptive_weight_bias() {
        // With a weight of 0 for all but the first key, the first key dominates.
        let mut src = RandomSource::new(200);
        let mut w = vec![0.0f32; src.pool().len()];
        w[0] = 1.0;
        let first = src.pool()[0];
        src.set_weights(w);
        let t = src.next_target();
        let hits = t
            .items
            .iter()
            .filter(|e| e.physical_key() == Some(first))
            .count();
        assert_eq!(hits, 200, "weighted-only key should always be chosen");
    }
}
