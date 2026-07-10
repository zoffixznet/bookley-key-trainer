//! Typing-target normalization. Book files on disk and exports keep their original
//! Unicode; the TYPING TARGET (what the stage displays and compares keystrokes against)
//! is normalized so everything is reachable on a plain keyboard:
//!
//! - accented letters -> base ASCII letters (É -> E, é -> e)
//! - em/en dashes (and friends) -> "-"
//! - curly/angled quotes -> ' and "
//! - ellipsis -> "..."
//! - every exotic Unicode space (nbsp, thin, wide, ideographic, ...) and tab -> " "
//! - anything left with no sensible keyboard equivalent is dropped
//! - only space and newline survive as whitespace; doubled spaces are collapsed
//!
//! Implementation: an explicit punctuation/space map first, then NFKD decomposition with
//! combining marks and remaining non-ASCII dropped.

use unicode_normalization::UnicodeNormalization;

/// Normalize text into a plain-ASCII typing target.
pub fn normalize_target(input: &str) -> String {
    // Pass 1: explicit replacements for punctuation that NFKD does not fold to ASCII.
    let mut mapped = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            // Dashes and dash-like horizontals.
            '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}'
            | '\u{2212}' | '\u{FE58}' | '\u{FE63}' | '\u{FF0D}' => mapped.push('-'),
            // Single quotes / apostrophes.
            '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' | '\u{2039}' | '\u{203A}'
            | '\u{02BC}' | '\u{FF07}' => mapped.push('\''),
            // Double quotes.
            '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' | '\u{00AB}' | '\u{00BB}'
            | '\u{FF02}' => mapped.push('"'),
            // Ellipsis.
            '\u{2026}' => mapped.push_str("..."),
            // Every Unicode space separator, plus tab, becomes a regular space.
            '\t'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200A}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}' => mapped.push(' '),
            // Zero-width / joiner junk: drop.
            '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{FEFF}' | '\u{2060}' => {}
            // Line separators become newlines; carriage returns handled below.
            '\u{2028}' | '\u{2029}' => mapped.push('\n'),
            '\r' => {}
            _ => mapped.push(c),
        }
    }

    // Pass 2: NFKD-decompose (é -> e + combining acute), keep printable ASCII, drop
    // combining marks and any remaining unmappable character.
    let mut out = String::with_capacity(mapped.len());
    for c in mapped.nfkd() {
        if c == '\n' {
            out.push('\n');
        } else if (' '..='~').contains(&c) {
            out.push(c);
        }
        // Everything else (combining marks, emoji, symbols) is dropped.
    }

    // Pass 3: collapse runs of spaces (normalization can create doubles) and trim spaces
    // that ended up hugging newlines.
    let mut cleaned = String::with_capacity(out.len());
    let mut prev_space = false;
    for c in out.chars() {
        if c == ' ' {
            if !prev_space {
                cleaned.push(' ');
            }
            prev_space = true;
        } else {
            if c == '\n' {
                while cleaned.ends_with(' ') {
                    cleaned.pop();
                }
                prev_space = true; // also swallow spaces right after the newline
            } else {
                prev_space = false;
            }
            cleaned.push(c);
        }
    }
    cleaned.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn users_example() {
        assert_eq!(
            normalize_target("Émile heard it too — a third key"),
            "Emile heard it too - a third key"
        );
    }

    #[test]
    fn accents_fold_to_ascii() {
        assert_eq!(
            normalize_target("café naïve Übung señor"),
            "cafe naive Ubung senor"
        );
        assert_eq!(normalize_target("ÉÈÊËéèêë"), "EEEEeeee");
    }

    #[test]
    fn dashes_become_hyphen() {
        assert_eq!(normalize_target("a–b—c‒d―e−f"), "a-b-c-d-e-f");
    }

    #[test]
    fn quotes_become_straight() {
        assert_eq!(
            normalize_target("‘single’ “double” „low” «angle»"),
            "'single' \"double\" \"low\" \"angle\""
        );
        assert_eq!(normalize_target("it’s"), "it's");
    }

    #[test]
    fn ellipsis_expands() {
        assert_eq!(normalize_target("wait…"), "wait...");
    }

    #[test]
    fn exotic_spaces_become_space() {
        // nbsp, thin, em-space, ideographic, narrow nbsp
        assert_eq!(
            normalize_target("a\u{00A0}b\u{2009}c\u{2003}d\u{3000}e\u{202F}f"),
            "a b c d e f"
        );
    }

    #[test]
    fn tabs_become_space_and_doubles_collapse() {
        assert_eq!(normalize_target("a\tb"), "a b");
        assert_eq!(normalize_target("a — b"), "a - b"); // space, dash, space stays single
        assert_eq!(normalize_target("a\u{00A0} b"), "a b"); // nbsp+space collapses
        assert_eq!(normalize_target("x  y   z"), "x y z");
    }

    #[test]
    fn unmappable_is_dropped() {
        assert_eq!(normalize_target("fire 🔥 emoji"), "fire emoji");
        assert_eq!(normalize_target("a☃b"), "ab");
        assert_eq!(normalize_target("→ arrows ←"), "arrows");
    }

    #[test]
    fn only_space_and_newline_survive_as_whitespace() {
        let n = normalize_target("line one\r\nline\ttwo\u{2028}three");
        assert_eq!(n, "line one\nline two\nthree");
        assert!(n
            .chars()
            .all(|c| c == ' ' || c == '\n' || (' '..='~').contains(&c)));
    }

    #[test]
    fn spaces_around_newlines_are_trimmed() {
        assert_eq!(
            normalize_target("end of line \n  next"),
            "end of line\nnext"
        );
    }

    #[test]
    fn plain_ascii_untouched() {
        let s = "The quick brown fox; it jumped over 12 lazy dogs! (Really?)";
        assert_eq!(normalize_target(s), s);
    }
}
