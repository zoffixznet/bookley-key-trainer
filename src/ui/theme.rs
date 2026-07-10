//! "Manuscript & Ribbon" visual identity: palette, fonts, and egui style setup.

use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke};

use crate::core::config::Theme;

/// The named palette. Two themes share the same accent semantics.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// Base background.
    pub ink_950: Color32,
    /// Panels / cards.
    pub ink_850: Color32,
    /// Primary foreground / correctly-typed text.
    pub paper: Color32,
    /// Untyped / upcoming text (dim), dividers.
    pub ghost: Color32,
    /// Signature interactive accent (caret, next-key ring, active nav, primary buttons).
    pub verdigris: Color32,
    /// Secondary accent (wordmark, titles, chapter headings, PB medals).
    pub brass: Color32,
    /// Error / destructive.
    pub ribbon: Color32,
    /// Eight finger-zone hues (index 0..8, thumbs share index 4).
    pub finger: [Color32; 9],
}

impl Palette {
    pub fn dark() -> Self {
        Palette {
            ink_950: Color32::from_rgb(0x14, 0x11, 0x0E),
            ink_850: Color32::from_rgb(0x20, 0x1B, 0x16),
            paper: Color32::from_rgb(0xE8, 0xE1, 0xD3),
            ghost: Color32::from_rgb(0x7C, 0x72, 0x63),
            verdigris: Color32::from_rgb(0x3E, 0x9C, 0x8C),
            brass: Color32::from_rgb(0xC9, 0xA2, 0x4B),
            ribbon: Color32::from_rgb(0xC4, 0x36, 0x2F),
            finger: FINGER_DARK,
        }
    }

    pub fn light() -> Self {
        // Foolscap paper theme: warm off-white surfaces, ink-dark text, same accents.
        Palette {
            ink_950: Color32::from_rgb(0xEC, 0xE6, 0xD8),
            ink_850: Color32::from_rgb(0xDD, 0xD5, 0xC3),
            paper: Color32::from_rgb(0x23, 0x1E, 0x18),
            ghost: Color32::from_rgb(0x9A, 0x8F, 0x7C),
            verdigris: Color32::from_rgb(0x2C, 0x7A, 0x6D),
            brass: Color32::from_rgb(0x9A, 0x77, 0x22),
            ribbon: Color32::from_rgb(0xA8, 0x2A, 0x24),
            finger: FINGER_LIGHT,
        }
    }

    pub fn for_theme(theme: Theme) -> Self {
        match theme {
            Theme::Dark => Self::dark(),
            Theme::Light => Self::light(),
        }
    }
}

/// Eight muted, dark-friendly finger-zone hues (index 8 duplicated for symmetry / pinky).
const FINGER_DARK: [Color32; 9] = [
    Color32::from_rgb(0x5A, 0x3E, 0x4A), // left pinky  - muted plum
    Color32::from_rgb(0x4A, 0x40, 0x2E), // left ring   - olive
    Color32::from_rgb(0x2E, 0x44, 0x4A), // left middle - teal-slate
    Color32::from_rgb(0x3A, 0x4A, 0x35), // left index  - moss
    Color32::from_rgb(0x38, 0x33, 0x2A), // thumbs      - warm stone
    Color32::from_rgb(0x40, 0x3A, 0x2E), // right index - bronze
    Color32::from_rgb(0x2E, 0x3E, 0x4E), // right middle- steel blue
    Color32::from_rgb(0x4A, 0x38, 0x2E), // right ring  - umber
    Color32::from_rgb(0x4A, 0x30, 0x40), // right pinky - mauve
];

const FINGER_LIGHT: [Color32; 9] = [
    Color32::from_rgb(0xD8, 0xC0, 0xC8),
    Color32::from_rgb(0xD2, 0xCB, 0xB0),
    Color32::from_rgb(0xBF, 0xD0, 0xD2),
    Color32::from_rgb(0xC6, 0xD2, 0xBD),
    Color32::from_rgb(0xD6, 0xCE, 0xBE),
    Color32::from_rgb(0xD8, 0xCC, 0xB6),
    Color32::from_rgb(0xC0, 0xCE, 0xDA),
    Color32::from_rgb(0xD8, 0xC6, 0xB4),
    Color32::from_rgb(0xD8, 0xC2, 0xCE),
];

/// Font role sizes used across the UI.
pub fn mono_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}
pub fn ui_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

/// Apply the palette to egui's visuals for a consistent look. Called on theme change.
pub fn apply_style(ctx: &egui::Context, theme: Theme) {
    let p = Palette::for_theme(theme);
    let mut visuals = match theme {
        Theme::Dark => egui::Visuals::dark(),
        Theme::Light => egui::Visuals::light(),
    };
    visuals.panel_fill = p.ink_950;
    visuals.window_fill = p.ink_850;
    visuals.extreme_bg_color = p.ink_850;
    visuals.faint_bg_color = p.ink_850;
    visuals.override_text_color = Some(p.paper);
    visuals.hyperlink_color = p.verdigris;
    visuals.selection.bg_fill = p.verdigris.linear_multiply(0.4);
    visuals.selection.stroke = Stroke::new(1.0, p.verdigris);
    visuals.widgets.noninteractive.bg_fill = p.ink_850;
    visuals.widgets.inactive.bg_fill = p.ink_850;
    visuals.widgets.inactive.weak_bg_fill = p.ink_850;
    visuals.widgets.hovered.bg_fill = p.ink_850.linear_multiply(1.3);
    visuals.widgets.active.bg_fill = p.verdigris.linear_multiply(0.5);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.ghost.linear_multiply(0.5));
    let radius = CornerRadius::same(4);
    visuals.widgets.noninteractive.corner_radius = radius;
    visuals.widgets.inactive.corner_radius = radius;
    visuals.widgets.hovered.corner_radius = radius;
    visuals.widgets.active.corner_radius = radius;
    visuals.window_corner_radius = CornerRadius::same(8);
    ctx.all_styles_mut(|style| {
        style.visuals = visuals.clone();
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(10.0, 6.0);
    });
}

/// Install embedded fonts if any are staged under assets/fonts, otherwise keep egui's
/// defaults (which still render the identity via palette + sizing). Fonts are optional so
/// the repo stays self-contained without shipping large binaries we lack licenses for.
pub fn install_fonts(ctx: &egui::Context) {
    // No fonts are bundled by default (see NOTICE / DECISIONS). If a future build adds
    // .ttf/.otf under assets/fonts and wires them here, record the license in NOTICE.
    let _ = ctx;
}
