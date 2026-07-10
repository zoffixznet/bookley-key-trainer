//! Keyboard model: physical keys, layout rows, finger zones, dev keys, and the mapping
//! between a target character / expected key and the physical position to highlight.
//!
//! We highlight the on-screen keyboard by *physical* position (egui `physical_key`) so a
//! trainer teaches muscle memory regardless of the active keymap. Character targets map to
//! the physical key that produces them on a standard US QWERTY layout (which is the layout
//! we draw), plus a `shift` flag when the character needs Shift.

use egui::Key;

/// A logical finger, used for zone coloring and next-finger logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finger {
    LeftPinky,
    LeftRing,
    LeftMiddle,
    LeftIndex,
    LeftThumb,
    RightThumb,
    RightIndex,
    RightMiddle,
    RightRing,
    RightPinky,
}

impl Finger {
    /// A stable index 0..8 for the eight finger-zone hues (thumbs share one hue).
    pub fn zone(self) -> usize {
        match self {
            Finger::LeftPinky => 0,
            Finger::LeftRing => 1,
            Finger::LeftMiddle => 2,
            Finger::LeftIndex => 3,
            Finger::LeftThumb | Finger::RightThumb => 4,
            Finger::RightIndex => 5,
            Finger::RightMiddle => 6,
            Finger::RightRing => 7,
            Finger::RightPinky => 8,
        }
    }
}

/// A physical keyboard key we can draw and highlight.
#[derive(Debug, Clone)]
pub struct KeyCap {
    /// egui physical key this cap corresponds to (for highlight-by-physical-position).
    pub key: Key,
    /// Label drawn on the cap (unshifted).
    pub label: &'static str,
    /// Shifted label, if the key produces a different glyph with Shift.
    pub shifted: Option<&'static str>,
    /// Relative width in "units" (1.0 = a normal letter key).
    pub width: f32,
    /// Which finger this key belongs to.
    pub finger: Finger,
}

impl KeyCap {
    const fn new(
        key: Key,
        label: &'static str,
        shifted: Option<&'static str>,
        width: f32,
        finger: Finger,
    ) -> Self {
        KeyCap {
            key,
            label,
            shifted,
            width,
            finger,
        }
    }
}

use Finger::*;

/// The full keyboard layout, row by row, as we draw it. US QWERTY plus a nav cluster and a
/// function-key row so Random mode can exercise the whole board.
pub fn layout() -> Vec<Vec<KeyCap>> {
    vec![
        // Function row
        vec![
            KeyCap::new(Key::Escape, "Esc", None, 1.3, LeftPinky),
            KeyCap::new(Key::F1, "F1", None, 1.0, LeftPinky),
            KeyCap::new(Key::F2, "F2", None, 1.0, LeftRing),
            KeyCap::new(Key::F3, "F3", None, 1.0, LeftMiddle),
            KeyCap::new(Key::F4, "F4", None, 1.0, LeftIndex),
            KeyCap::new(Key::F5, "F5", None, 1.0, LeftIndex),
            KeyCap::new(Key::F6, "F6", None, 1.0, RightIndex),
            KeyCap::new(Key::F7, "F7", None, 1.0, RightIndex),
            KeyCap::new(Key::F8, "F8", None, 1.0, RightMiddle),
            KeyCap::new(Key::F9, "F9", None, 1.0, RightRing),
            KeyCap::new(Key::F10, "F10", None, 1.0, RightPinky),
            KeyCap::new(Key::F11, "F11", None, 1.0, RightPinky),
            KeyCap::new(Key::F12, "F12", None, 1.0, RightPinky),
        ],
        // Number row
        vec![
            KeyCap::new(Key::Backtick, "`", Some("~"), 1.0, LeftPinky),
            KeyCap::new(Key::Num1, "1", Some("!"), 1.0, LeftPinky),
            KeyCap::new(Key::Num2, "2", Some("@"), 1.0, LeftRing),
            KeyCap::new(Key::Num3, "3", Some("#"), 1.0, LeftMiddle),
            KeyCap::new(Key::Num4, "4", Some("$"), 1.0, LeftIndex),
            KeyCap::new(Key::Num5, "5", Some("%"), 1.0, LeftIndex),
            KeyCap::new(Key::Num6, "6", Some("^"), 1.0, RightIndex),
            KeyCap::new(Key::Num7, "7", Some("&"), 1.0, RightIndex),
            KeyCap::new(Key::Num8, "8", Some("*"), 1.0, RightMiddle),
            KeyCap::new(Key::Num9, "9", Some("("), 1.0, RightRing),
            KeyCap::new(Key::Num0, "0", Some(")"), 1.0, RightPinky),
            KeyCap::new(Key::Minus, "-", Some("_"), 1.0, RightPinky),
            KeyCap::new(Key::Equals, "=", Some("+"), 1.0, RightPinky),
            KeyCap::new(Key::Backspace, "Bksp", None, 1.8, RightPinky),
        ],
        // Top letter row
        vec![
            KeyCap::new(Key::Tab, "Tab", None, 1.5, LeftPinky),
            KeyCap::new(Key::Q, "Q", None, 1.0, LeftPinky),
            KeyCap::new(Key::W, "W", None, 1.0, LeftRing),
            KeyCap::new(Key::E, "E", None, 1.0, LeftMiddle),
            KeyCap::new(Key::R, "R", None, 1.0, LeftIndex),
            KeyCap::new(Key::T, "T", None, 1.0, LeftIndex),
            KeyCap::new(Key::Y, "Y", None, 1.0, RightIndex),
            KeyCap::new(Key::U, "U", None, 1.0, RightIndex),
            KeyCap::new(Key::I, "I", None, 1.0, RightMiddle),
            KeyCap::new(Key::O, "O", None, 1.0, RightRing),
            KeyCap::new(Key::P, "P", None, 1.0, RightPinky),
            KeyCap::new(Key::OpenBracket, "[", Some("{"), 1.0, RightPinky),
            KeyCap::new(Key::CloseBracket, "]", Some("}"), 1.0, RightPinky),
            KeyCap::new(Key::Backslash, "\\", Some("|"), 1.5, RightPinky),
        ],
        // Home row (CapsLock is not a typing target and egui has no CapsLock variant, so
        // we start the drawn row at A; the home-row bumps live on F and J.)
        vec![
            KeyCap::new(Key::A, "A", None, 1.0, LeftPinky),
            KeyCap::new(Key::S, "S", None, 1.0, LeftRing),
            KeyCap::new(Key::D, "D", None, 1.0, LeftMiddle),
            KeyCap::new(Key::F, "F", None, 1.0, LeftIndex),
            KeyCap::new(Key::G, "G", None, 1.0, LeftIndex),
            KeyCap::new(Key::H, "H", None, 1.0, RightIndex),
            KeyCap::new(Key::J, "J", None, 1.0, RightIndex),
            KeyCap::new(Key::K, "K", None, 1.0, RightMiddle),
            KeyCap::new(Key::L, "L", None, 1.0, RightRing),
            KeyCap::new(Key::Semicolon, ";", Some(":"), 1.0, RightPinky),
            KeyCap::new(Key::Quote, "'", Some("\""), 1.0, RightPinky),
            KeyCap::new(Key::Enter, "Enter", None, 2.0, RightPinky),
        ],
        // Bottom letter row
        vec![
            KeyCap::new(Key::Comma, ",", Some("<"), 1.0, RightMiddle), // placeholder replaced below
        ],
        // Space row
        vec![
            KeyCap::new(Key::Space, "Space", None, 6.0, LeftThumb),
        ],
        // Navigation cluster / arrows (drawn as a compact block)
        vec![
            KeyCap::new(Key::Insert, "Ins", None, 1.0, RightPinky),
            KeyCap::new(Key::Home, "Home", None, 1.0, RightPinky),
            KeyCap::new(Key::PageUp, "PgUp", None, 1.0, RightPinky),
            KeyCap::new(Key::Delete, "Del", None, 1.0, RightPinky),
            KeyCap::new(Key::End, "End", None, 1.0, RightPinky),
            KeyCap::new(Key::PageDown, "PgDn", None, 1.0, RightPinky),
            KeyCap::new(Key::ArrowLeft, "\u{2190}", None, 1.0, RightIndex),
            KeyCap::new(Key::ArrowDown, "\u{2193}", None, 1.0, RightMiddle),
            KeyCap::new(Key::ArrowUp, "\u{2191}", None, 1.0, RightMiddle),
            KeyCap::new(Key::ArrowRight, "\u{2192}", None, 1.0, RightRing),
        ],
    ]
    .into_iter()
    .enumerate()
    .map(|(i, row)| if i == 4 { bottom_row() } else { row })
    .collect()
}

fn bottom_row() -> Vec<KeyCap> {
    vec![
        KeyCap::new(Key::Z, "Z", None, 1.0, LeftPinky),
        KeyCap::new(Key::X, "X", None, 1.0, LeftRing),
        KeyCap::new(Key::C, "C", None, 1.0, LeftMiddle),
        KeyCap::new(Key::V, "V", None, 1.0, LeftIndex),
        KeyCap::new(Key::B, "B", None, 1.0, LeftIndex),
        KeyCap::new(Key::N, "N", None, 1.0, RightIndex),
        KeyCap::new(Key::M, "M", None, 1.0, RightIndex),
        KeyCap::new(Key::Comma, ",", Some("<"), 1.0, RightMiddle),
        KeyCap::new(Key::Period, ".", Some(">"), 1.0, RightRing),
        KeyCap::new(Key::Slash, "/", Some("?"), 1.0, RightPinky),
    ]
}

/// A flat list of every physical key in the layout (deduplicated), used by Random mode.
pub fn all_keys() -> Vec<Key> {
    let mut seen = Vec::new();
    for row in layout() {
        for cap in row {
            if !seen.contains(&cap.key) {
                seen.push(cap.key);
            }
        }
    }
    seen
}

/// Dev shortcut keys, excluded from the Random pool so they never clash with typing.
pub const DEV_KEYS: [Key; 3] = [Key::F9, Key::F10, Key::F12];

pub const DEV_AUTOTYPE: Key = Key::F9;
pub const DEV_COMPLETE_PAGE: Key = Key::F10;
pub const DEV_COMPLETE_CHAPTER: Key = Key::F12;

/// Human-friendly name for a key, used in Random-mode prompts ("press: Page Up").
pub fn display_name(key: Key) -> String {
    // Prefer the printable label from the layout, else a descriptive name.
    if let Some(cap) = find_cap(key) {
        return cap.label.to_string();
    }
    format!("{key:?}")
}

/// Find the keycap for a physical key.
pub fn find_cap(key: Key) -> Option<KeyCap> {
    for row in layout() {
        for cap in row {
            if cap.key == key {
                return Some(cap);
            }
        }
    }
    None
}

/// The finger assigned to a physical key, if known.
pub fn finger_for(key: Key) -> Option<Finger> {
    find_cap(key).map(|c| c.finger)
}

/// Map a target character to the physical key that produces it on US QWERTY, plus whether
/// Shift is needed. Returns `None` for characters we don't draw (rare).
pub fn char_to_physical(c: char) -> Option<(Key, bool)> {
    // Letters: physical key is the uppercase letter; shift needed for uppercase.
    if c.is_ascii_alphabetic() {
        let upper = c.to_ascii_uppercase();
        let key = letter_key(upper)?;
        return Some((key, c.is_ascii_uppercase()));
    }
    if c == ' ' {
        return Some((Key::Space, false));
    }
    if c == '\n' || c == '\r' {
        return Some((Key::Enter, false));
    }
    if c == '\t' {
        return Some((Key::Tab, false));
    }
    // Search the layout for unshifted then shifted glyphs.
    let s = c.to_string();
    for row in layout() {
        for cap in row {
            if cap.label == s {
                return Some((cap.key, false));
            }
            if cap.shifted == Some(s.as_str()) {
                return Some((cap.key, true));
            }
        }
    }
    None
}

fn letter_key(upper: char) -> Option<Key> {
    Some(match upper {
        'A' => Key::A,
        'B' => Key::B,
        'C' => Key::C,
        'D' => Key::D,
        'E' => Key::E,
        'F' => Key::F,
        'G' => Key::G,
        'H' => Key::H,
        'I' => Key::I,
        'J' => Key::J,
        'K' => Key::K,
        'L' => Key::L,
        'M' => Key::M,
        'N' => Key::N,
        'O' => Key::O,
        'P' => Key::P,
        'Q' => Key::Q,
        'R' => Key::R,
        'S' => Key::S,
        'T' => Key::T,
        'U' => Key::U,
        'V' => Key::V,
        'W' => Key::W,
        'X' => Key::X,
        'Y' => Key::Y,
        'Z' => Key::Z,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_keys_includes_letters_digits_and_non_char() {
        let keys = all_keys();
        assert!(keys.contains(&Key::A));
        assert!(keys.contains(&Key::Num5));
        assert!(keys.contains(&Key::ArrowUp));
        assert!(keys.contains(&Key::PageUp));
        assert!(keys.contains(&Key::F1));
        assert!(keys.contains(&Key::Enter));
        assert!(keys.contains(&Key::Escape));
    }

    #[test]
    fn dev_keys_are_in_the_layout() {
        let keys = all_keys();
        for d in DEV_KEYS {
            assert!(keys.contains(&d), "dev key {d:?} must exist to exclude it");
        }
    }

    #[test]
    fn char_to_physical_letters() {
        assert_eq!(char_to_physical('a'), Some((Key::A, false)));
        assert_eq!(char_to_physical('A'), Some((Key::A, true)));
        assert_eq!(char_to_physical(' '), Some((Key::Space, false)));
    }

    #[test]
    fn char_to_physical_symbols() {
        assert_eq!(char_to_physical('!'), Some((Key::Num1, true)));
        assert_eq!(char_to_physical('1'), Some((Key::Num1, false)));
        assert_eq!(char_to_physical('.'), Some((Key::Period, false)));
        assert_eq!(char_to_physical('?'), Some((Key::Slash, true)));
    }

    #[test]
    fn every_cap_has_a_finger_zone_under_9() {
        for row in layout() {
            for cap in row {
                assert!(cap.finger.zone() <= 8);
            }
        }
    }
}
