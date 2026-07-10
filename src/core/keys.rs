//! Keyboard model: a real full-size (104-key) board with proper geometry, finger zones,
//! dev keys, and the mapping between a target character and the physical position to
//! highlight.
//!
//! We highlight the on-screen keyboard by *physical* position (egui `physical_key`) so a
//! trainer teaches muscle memory regardless of the active keymap. Character targets map to
//! the physical key that produces them on a standard US QWERTY layout (which is the layout
//! we draw), plus a `shift` flag when the character needs Shift; Guide mode also
//! highlights the Shift keys for shifted targets.

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

/// Identity of a drawn keycap. `K(Key)` caps are matchable against egui `physical_key`
/// events and can be typing targets; the rest (modifiers, numpad, PrtSc cluster) are
/// drawn for realism, flashed where possible, but never targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapId {
    K(Key),
    ShiftL,
    ShiftR,
    Caps,
    CtrlL,
    CtrlR,
    AltL,
    AltR,
    SuperL,
    SuperR,
    Menu,
    PrtSc,
    ScrLk,
    PauseBrk,
    NumLock,
    NpDiv,
    NpMul,
    NpSub,
    NpAdd,
    NpEnter,
    NpDot,
    Np(u8),
}

/// A physical keyboard key we can draw and highlight.
#[derive(Debug, Clone)]
pub struct KeyCap {
    pub id: CapId,
    /// Label drawn on the cap (unshifted).
    pub label: &'static str,
    /// Shifted label, if the key produces a different glyph with Shift.
    pub shifted: Option<&'static str>,
    /// Width in "units" (1.0 = a normal letter key).
    pub width: f32,
    /// Height in row units (2.0 = spans two rows, e.g. numpad + / Enter).
    pub height: f32,
    /// Which finger this key belongs to.
    pub finger: Finger,
}

/// One slot in a board row: a cap or a horizontal gap (in units).
#[derive(Debug, Clone)]
pub enum Slot {
    Cap(KeyCap),
    Gap(f32),
}

/// The whole drawn board.
#[derive(Debug, Clone)]
pub struct Board {
    pub rows: Vec<Vec<Slot>>,
    pub units_wide: f32,
}

use Finger::*;

fn cap(id: CapId, label: &'static str, w: f32, finger: Finger) -> Slot {
    Slot::Cap(KeyCap {
        id,
        label,
        shifted: None,
        width: w,
        height: 1.0,
        finger,
    })
}

fn cap_sh(id: CapId, label: &'static str, shifted: &'static str, finger: Finger) -> Slot {
    Slot::Cap(KeyCap {
        id,
        label,
        shifted: Some(shifted),
        width: 1.0,
        height: 1.0,
        finger,
    })
}

fn tall(id: CapId, label: &'static str, finger: Finger) -> Slot {
    Slot::Cap(KeyCap {
        id,
        label,
        shifted: None,
        width: 1.0,
        height: 2.0,
        finger,
    })
}

fn k(key: Key, label: &'static str, w: f32, finger: Finger) -> Slot {
    cap(CapId::K(key), label, w, finger)
}

fn k_sh(key: Key, label: &'static str, shifted: &'static str, finger: Finger) -> Slot {
    cap_sh(CapId::K(key), label, shifted, finger)
}

fn gap(units: f32) -> Slot {
    Slot::Gap(units)
}

/// Gutter between the main block, nav cluster, and numpad.
const GUTTER: f32 = 0.5;

/// Build the full-size board. The main block is 15u wide; the nav cluster 3u; the numpad
/// 4u. `show_numpad` controls whether the numpad section is included.
pub fn board(show_numpad: bool) -> Board {
    use CapId::*;
    use Key as EK;

    let mut rows: Vec<Vec<Slot>> = Vec::new();

    // Function row: Esc, 1u gap, F1-F4, 0.5 gap, F5-F8, 0.5 gap, F9-F12 (= 15u).
    let mut r = vec![
        k(EK::Escape, "Esc", 1.0, LeftPinky),
        gap(1.0),
        k(EK::F1, "F1", 1.0, LeftPinky),
        k(EK::F2, "F2", 1.0, LeftRing),
        k(EK::F3, "F3", 1.0, LeftMiddle),
        k(EK::F4, "F4", 1.0, LeftIndex),
        gap(0.5),
        k(EK::F5, "F5", 1.0, LeftIndex),
        k(EK::F6, "F6", 1.0, RightIndex),
        k(EK::F7, "F7", 1.0, RightIndex),
        k(EK::F8, "F8", 1.0, RightMiddle),
        gap(0.5),
        k(EK::F9, "F9", 1.0, RightRing),
        k(EK::F10, "F10", 1.0, RightPinky),
        k(EK::F11, "F11", 1.0, RightPinky),
        k(EK::F12, "F12", 1.0, RightPinky),
        gap(GUTTER),
        cap(PrtSc, "PrtSc", 1.0, RightPinky),
        cap(ScrLk, "ScrLk", 1.0, RightPinky),
        cap(PauseBrk, "Pause", 1.0, RightPinky),
    ];
    if show_numpad {
        r.push(gap(GUTTER + 4.0));
    }
    rows.push(r);

    // Number row: 13 keys + 2u Backspace (= 15u).
    let mut r = vec![
        k_sh(EK::Backtick, "`", "~", LeftPinky),
        k_sh(EK::Num1, "1", "!", LeftPinky),
        k_sh(EK::Num2, "2", "@", LeftRing),
        k_sh(EK::Num3, "3", "#", LeftMiddle),
        k_sh(EK::Num4, "4", "$", LeftIndex),
        k_sh(EK::Num5, "5", "%", LeftIndex),
        k_sh(EK::Num6, "6", "^", RightIndex),
        k_sh(EK::Num7, "7", "&", RightIndex),
        k_sh(EK::Num8, "8", "*", RightMiddle),
        k_sh(EK::Num9, "9", "(", RightRing),
        k_sh(EK::Num0, "0", ")", RightPinky),
        k_sh(EK::Minus, "-", "_", RightPinky),
        k_sh(EK::Equals, "=", "+", RightPinky),
        k(EK::Backspace, "Backspace", 2.0, RightPinky),
        gap(GUTTER),
        k(EK::Insert, "Ins", 1.0, RightPinky),
        k(EK::Home, "Home", 1.0, RightPinky),
        k(EK::PageUp, "PgUp", 1.0, RightPinky),
    ];
    if show_numpad {
        r.extend([
            gap(GUTTER),
            cap(NumLock, "Num", 1.0, RightIndex),
            cap(NpDiv, "/", 1.0, RightMiddle),
            cap(NpMul, "*", 1.0, RightRing),
            cap(NpSub, "-", 1.0, RightPinky),
        ]);
    }
    rows.push(r);

    // Top letter row: Tab 1.5 + 12 keys + 1.5 backslash (= 15u).
    let mut r = vec![
        k(EK::Tab, "Tab", 1.5, LeftPinky),
        k(EK::Q, "Q", 1.0, LeftPinky),
        k(EK::W, "W", 1.0, LeftRing),
        k(EK::E, "E", 1.0, LeftMiddle),
        k(EK::R, "R", 1.0, LeftIndex),
        k(EK::T, "T", 1.0, LeftIndex),
        k(EK::Y, "Y", 1.0, RightIndex),
        k(EK::U, "U", 1.0, RightIndex),
        k(EK::I, "I", 1.0, RightMiddle),
        k(EK::O, "O", 1.0, RightRing),
        k(EK::P, "P", 1.0, RightPinky),
        k_sh(EK::OpenBracket, "[", "{", RightPinky),
        k_sh(EK::CloseBracket, "]", "}", RightPinky),
        k(EK::Backslash, "\\", 1.5, RightPinky),
        gap(GUTTER),
        k(EK::Delete, "Del", 1.0, RightPinky),
        k(EK::End, "End", 1.0, RightPinky),
        k(EK::PageDown, "PgDn", 1.0, RightPinky),
    ];
    if show_numpad {
        r.extend([
            gap(GUTTER),
            cap(Np(7), "7", 1.0, RightIndex),
            cap(Np(8), "8", 1.0, RightMiddle),
            cap(Np(9), "9", 1.0, RightRing),
            tall(NpAdd, "+", RightPinky),
        ]);
    }
    rows.push(r);

    // Home row: Caps 1.75 + 11 keys + Enter 2.25 (= 15u).
    let mut r = vec![
        cap(Caps, "Caps", 1.75, LeftPinky),
        k(EK::A, "A", 1.0, LeftPinky),
        k(EK::S, "S", 1.0, LeftRing),
        k(EK::D, "D", 1.0, LeftMiddle),
        k(EK::F, "F", 1.0, LeftIndex),
        k(EK::G, "G", 1.0, LeftIndex),
        k(EK::H, "H", 1.0, RightIndex),
        k(EK::J, "J", 1.0, RightIndex),
        k(EK::K, "K", 1.0, RightMiddle),
        k(EK::L, "L", 1.0, RightRing),
        k_sh(EK::Semicolon, ";", ":", RightPinky),
        k_sh(EK::Quote, "'", "\"", RightPinky),
        k(EK::Enter, "Enter", 2.25, RightPinky),
        gap(GUTTER + 3.0),
    ];
    if show_numpad {
        r.extend([
            gap(GUTTER),
            cap(Np(4), "4", 1.0, RightIndex),
            cap(Np(5), "5", 1.0, RightMiddle),
            cap(Np(6), "6", 1.0, RightRing),
            gap(1.0), // occupied by the tall + above
        ]);
    }
    rows.push(r);

    // Bottom letter row: Shift 2.25 + 10 keys + Shift 2.75 (= 15u).
    let mut r = vec![
        cap(ShiftL, "Shift", 2.25, LeftPinky),
        k(EK::Z, "Z", 1.0, LeftPinky),
        k(EK::X, "X", 1.0, LeftRing),
        k(EK::C, "C", 1.0, LeftMiddle),
        k(EK::V, "V", 1.0, LeftIndex),
        k(EK::B, "B", 1.0, LeftIndex),
        k(EK::N, "N", 1.0, RightIndex),
        k(EK::M, "M", 1.0, RightIndex),
        k_sh(EK::Comma, ",", "<", RightMiddle),
        k_sh(EK::Period, ".", ">", RightRing),
        k_sh(EK::Slash, "/", "?", RightPinky),
        cap(ShiftR, "Shift", 2.75, RightPinky),
        gap(GUTTER + 1.0),
        k(EK::ArrowUp, "\u{2191}", 1.0, RightMiddle),
        gap(1.0),
    ];
    if show_numpad {
        r.extend([
            gap(GUTTER),
            cap(Np(1), "1", 1.0, RightIndex),
            cap(Np(2), "2", 1.0, RightMiddle),
            cap(Np(3), "3", 1.0, RightRing),
            tall(NpEnter, "Ent", RightPinky),
        ]);
    }
    rows.push(r);

    // Modifier row: 3x1.25 + 6.25 space + 4x1.25 (= 15u).
    let mut r = vec![
        cap(CtrlL, "Ctrl", 1.25, LeftPinky),
        cap(SuperL, "Super", 1.25, LeftThumb),
        cap(AltL, "Alt", 1.25, LeftThumb),
        k(EK::Space, "", 6.25, RightThumb),
        cap(AltR, "AltGr", 1.25, RightThumb),
        cap(SuperR, "Super", 1.25, RightThumb),
        cap(Menu, "Menu", 1.25, RightPinky),
        cap(CtrlR, "Ctrl", 1.25, RightPinky),
        gap(GUTTER),
        k(EK::ArrowLeft, "\u{2190}", 1.0, RightIndex),
        k(EK::ArrowDown, "\u{2193}", 1.0, RightMiddle),
        k(EK::ArrowRight, "\u{2192}", 1.0, RightRing),
    ];
    if show_numpad {
        r.extend([
            gap(GUTTER),
            cap(Np(0), "0", 2.0, RightThumb),
            cap(NpDot, ".", 1.0, RightRing),
            gap(1.0), // occupied by the tall numpad Enter
        ]);
    }
    rows.push(r);

    let units_wide = if show_numpad {
        15.0 + GUTTER + 3.0 + GUTTER + 4.0
    } else {
        15.0 + GUTTER + 3.0
    };
    Board { rows, units_wide }
}

/// A flat list of every typeable physical key (deduplicated): the `K(Key)` caps.
/// Modifiers, the PrtSc cluster, and the numpad are drawn but are not typing targets.
pub fn all_keys() -> Vec<Key> {
    let mut seen = Vec::new();
    for row in board(false).rows {
        for slot in row {
            if let Slot::Cap(c) = slot {
                if let CapId::K(key) = c.id {
                    if !seen.contains(&key) {
                        seen.push(key);
                    }
                }
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

/// Human-friendly name for a key, used in Random-mode prompts and stats.
pub fn display_name(key: Key) -> String {
    if key == Key::Space {
        return "Space".to_string();
    }
    if let Some(cap) = find_cap(key) {
        return cap.label.to_string();
    }
    format!("{key:?}")
}

/// Find the keycap for a typeable physical key.
pub fn find_cap(key: Key) -> Option<KeyCap> {
    for row in board(false).rows {
        for slot in row {
            if let Slot::Cap(c) = slot {
                if c.id == CapId::K(key) {
                    return Some(c);
                }
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
    // Search the board for unshifted then shifted glyphs.
    let s = c.to_string();
    for row in board(false).rows {
        for slot in row {
            if let Slot::Cap(cap) = slot {
                if let CapId::K(key) = cap.id {
                    if cap.label == s {
                        return Some((key, false));
                    }
                    if cap.shifted == Some(s.as_str()) {
                        return Some((key, true));
                    }
                }
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
        assert!(keys.contains(&Key::Insert));
        assert!(keys.contains(&Key::Delete));
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
        for row in board(true).rows {
            for slot in row {
                if let Slot::Cap(cap) = slot {
                    assert!(cap.finger.zone() <= 8);
                }
            }
        }
    }

    /// Every row of every board variant must line up to the same total width.
    #[test]
    fn rows_align_to_the_board_width() {
        for show_numpad in [false, true] {
            let b = board(show_numpad);
            for (i, row) in b.rows.iter().enumerate() {
                let w: f32 = row
                    .iter()
                    .map(|s| match s {
                        Slot::Cap(c) => c.width,
                        Slot::Gap(g) => *g,
                    })
                    .sum();
                assert!(
                    (w - b.units_wide).abs() < 0.01,
                    "row {i} is {w}u, board is {}u (numpad={show_numpad})",
                    b.units_wide
                );
            }
        }
    }

    /// The board carries the full-size anatomy: both shifts, caps, modifier row, nav
    /// cluster, inverted-T arrows, and (when shown) the numpad.
    #[test]
    fn full_size_anatomy_present() {
        let b = board(true);
        let ids: Vec<CapId> = b
            .rows
            .iter()
            .flatten()
            .filter_map(|s| match s {
                Slot::Cap(c) => Some(c.id),
                _ => None,
            })
            .collect();
        for want in [
            CapId::ShiftL,
            CapId::ShiftR,
            CapId::Caps,
            CapId::CtrlL,
            CapId::CtrlR,
            CapId::AltL,
            CapId::AltR,
            CapId::SuperL,
            CapId::Menu,
            CapId::NumLock,
            CapId::NpEnter,
            CapId::Np(0),
            CapId::Np(9),
            CapId::K(Key::ArrowUp),
            CapId::K(Key::ArrowLeft),
            CapId::K(Key::Insert),
            CapId::K(Key::PageDown),
        ] {
            assert!(ids.contains(&want), "missing {want:?}");
        }
        // Numpad hidden variant drops the numpad but keeps the nav cluster.
        let b2 = board(false);
        let ids2: Vec<CapId> = b2
            .rows
            .iter()
            .flatten()
            .filter_map(|s| match s {
                Slot::Cap(c) => Some(c.id),
                _ => None,
            })
            .collect();
        assert!(!ids2.contains(&CapId::NumLock));
        assert!(ids2.contains(&CapId::K(Key::ArrowUp)));
    }

    #[test]
    fn key_widths_match_real_geometry() {
        let b = board(false);
        let find = |id: CapId| -> f32 {
            b.rows
                .iter()
                .flatten()
                .find_map(|s| match s {
                    Slot::Cap(c) if c.id == id => Some(c.width),
                    _ => None,
                })
                .unwrap()
        };
        assert_eq!(find(CapId::K(Key::Tab)), 1.5);
        assert_eq!(find(CapId::Caps), 1.75);
        assert_eq!(find(CapId::ShiftL), 2.25);
        assert_eq!(find(CapId::ShiftR), 2.75);
        assert_eq!(find(CapId::K(Key::Backspace)), 2.0);
        assert_eq!(find(CapId::K(Key::Space)), 6.25);
        assert_eq!(find(CapId::K(Key::Enter)), 2.25);
    }
}
