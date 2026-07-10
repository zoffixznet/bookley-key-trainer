//! Bundled word list for Single-word mode. Uses the EFF long wordlist (CC BY 3.0 US),
//! embedded at compile time. Attribution is recorded in NOTICE at the repo root.

use rand::seq::IndexedRandom;

const WORDLIST: &str = include_str!("../../assets/wordlist.txt");

/// Provider of single words drawn from the bundled list.
pub struct WordList {
    words: Vec<&'static str>,
}

impl Default for WordList {
    fn default() -> Self {
        Self::new()
    }
}

impl WordList {
    pub fn new() -> Self {
        let words: Vec<&'static str> = WORDLIST
            .lines()
            .map(str::trim)
            .filter(|w| !w.is_empty())
            .collect();
        WordList { words }
    }

    pub fn len(&self) -> usize {
        self.words.len()
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// Draw a random word.
    pub fn random(&self) -> &'static str {
        let mut rng = rand::rng();
        self.words.choose(&mut rng).copied().unwrap_or("bookley")
    }

    /// All words (for tests / verification).
    pub fn all(&self) -> &[&'static str] {
        &self.words
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_loads_and_is_large() {
        let wl = WordList::new();
        assert!(wl.len() > 5000, "expected a large list, got {}", wl.len());
    }

    #[test]
    fn random_word_is_from_the_list() {
        let wl = WordList::new();
        for _ in 0..50 {
            let w = wl.random();
            assert!(wl.all().contains(&w));
            assert!(w.chars().all(|c| c.is_ascii_lowercase()));
        }
    }
}
