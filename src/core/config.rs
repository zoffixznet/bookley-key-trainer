//! Persisted user settings (TOML under the XDG config dir) plus the runtime enums that
//! define a session's two orthogonal axes: keyboard display mode and content mode.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// How the on-screen keyboard behaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyboardMode {
    /// Draw the keyboard and highlight the next key to press.
    Guide,
    /// Draw the keyboard, no hint, briefly flash just-pressed keys.
    Feedback,
    /// Do not draw the keyboard.
    Hidden,
}

impl KeyboardMode {
    pub fn label(self) -> &'static str {
        match self {
            KeyboardMode::Guide => "Guide",
            KeyboardMode::Feedback => "Feedback",
            KeyboardMode::Hidden => "Hidden",
        }
    }
    pub const ALL: [KeyboardMode; 3] = [
        KeyboardMode::Guide,
        KeyboardMode::Feedback,
        KeyboardMode::Hidden,
    ];
}

/// Where the text-to-type comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentMode {
    Random,
    Word,
    Paste,
    Book,
}

impl ContentMode {
    pub fn label(self) -> &'static str {
        match self {
            ContentMode::Random => "Random keys",
            ContentMode::Word => "Single word",
            ContentMode::Paste => "Paste text",
            ContentMode::Book => "Book",
        }
    }
    pub const ALL: [ContentMode; 4] = [
        ContentMode::Random,
        ContentMode::Word,
        ContentMode::Paste,
        ContentMode::Book,
    ];
}

/// Error-handling spectrum, matching Monkeytype's `stopOnError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorMode {
    /// Type through, mark wrong chars, keep going.
    Off,
    /// Block on a wrong letter until it is corrected.
    Letter,
    /// Must fix the whole word before advancing.
    Word,
}

impl ErrorMode {
    pub fn label(self) -> &'static str {
        match self {
            ErrorMode::Off => "Type through",
            ErrorMode::Letter => "Stop on letter",
            ErrorMode::Word => "Stop on word",
        }
    }
    pub const ALL: [ErrorMode; 3] = [ErrorMode::Off, ErrorMode::Letter, ErrorMode::Word];
}

/// Visual theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    Dark,
    Light,
}

impl Theme {
    pub fn label(self) -> &'static str {
        match self {
            Theme::Dark => "Dark (ink)",
            Theme::Light => "Light (foolscap)",
        }
    }
}

/// Caret style for the typing stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaretStyle {
    Block,
    Underline,
    Bar,
}

impl CaretStyle {
    pub fn label(self) -> &'static str {
        match self {
            CaretStyle::Block => "Block",
            CaretStyle::Underline => "Underline",
            CaretStyle::Bar => "Bar",
        }
    }
}

/// Everything that persists across runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub keyboard_mode: KeyboardMode,
    pub content_mode: ContentMode,
    pub error_mode: ErrorMode,
    pub theme: Theme,
    pub caret: CaretStyle,
    /// Typewriter key sound at launch (the top-bar toggle controls the running session).
    /// Renamed from the unused legacy `sound` field so existing configs pick up the
    /// on-by-default behavior; the old key is ignored on load.
    #[serde(default = "default_true")]
    pub key_sound: bool,
    pub reduced_motion: bool,
    /// Default target language for new books (free text).
    pub default_language: String,
    /// Number of keys to request in a Random-mode round (legacy; drills are timed now).
    pub random_round_len: usize,
    /// Model alias for book generation.
    pub book_model: String,
    /// Timed-drill duration in seconds (Word and Random modes).
    #[serde(default = "default_drill_secs")]
    pub drill_secs: u64,
    /// Draw the numpad section of the on-screen keyboard.
    #[serde(default = "default_true")]
    pub show_numpad: bool,
}

fn default_drill_secs() -> u64 {
    120
}
fn default_true() -> bool {
    true
}

/// The drill-duration presets offered in Settings: (seconds, label).
pub const DRILL_PRESETS: [(u64, &str); 5] = [
    (30, "30s"),
    (60, "1m"),
    (120, "2m"),
    (180, "3m"),
    (300, "5m"),
];

impl Default for Config {
    fn default() -> Self {
        Config {
            keyboard_mode: KeyboardMode::Guide,
            content_mode: ContentMode::Word,
            error_mode: ErrorMode::Off,
            // The light "foolscap" theme is the flagship default; a saved choice wins.
            theme: Theme::Light,
            caret: CaretStyle::Block,
            key_sound: true,
            reduced_motion: false,
            default_language: "English".to_string(),
            random_round_len: 30,
            // Spec: novel generation defaults to Opus; changeable in Settings.
            book_model: "opus".to_string(),
            drill_secs: default_drill_secs(),
            show_numpad: true,
        }
    }
}

impl Config {
    /// Load config from `path`, falling back to defaults on any error.
    pub fn load_from(path: &std::path::Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(s) => match toml::from_str(&s) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("config parse failed, using defaults: {e}");
                    Config::default()
                }
            },
            Err(_) => Config::default(),
        }
    }

    pub fn save_to(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let s = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, s)
    }
}

/// Resolve the XDG config file path for settings.
pub fn config_path() -> PathBuf {
    if let Some(dir) = crate::core::paths::config_dir() {
        dir.join("config.toml")
    } else {
        PathBuf::from("bookley-config.toml")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_toml() {
        let c = Config {
            keyboard_mode: KeyboardMode::Feedback,
            content_mode: ContentMode::Book,
            default_language: "Deutsch".into(),
            ..Config::default()
        };
        let dir = std::env::temp_dir().join(format!("bookley-cfg-{}", std::process::id()));
        let path = dir.join("config.toml");
        c.save_to(&path).unwrap();
        let back = Config::load_from(&path);
        assert_eq!(back.keyboard_mode, KeyboardMode::Feedback);
        assert_eq!(back.content_mode, ContentMode::Book);
        assert_eq!(back.default_language, "Deutsch");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Configs saved before the sound feature (including ones carrying the unused
    /// legacy `sound = false` stub) must come up with the key sound ON.
    #[test]
    fn key_sound_defaults_on_and_ignores_legacy_field() {
        let full = toml::to_string(&Config::default()).unwrap();
        let without: String = full
            .lines()
            .filter(|l| !l.starts_with("key_sound"))
            .collect::<Vec<_>>()
            .join("\n");
        let legacy = format!("{without}\nsound = false\n");
        let c: Config = toml::from_str(&legacy).unwrap();
        assert!(c.key_sound, "missing key_sound must default to on");
    }

    #[test]
    fn missing_file_is_defaults() {
        let c = Config::load_from(std::path::Path::new("/nonexistent/bookley/none.toml"));
        assert_eq!(c.keyboard_mode, KeyboardMode::Guide);
    }
}
