//! "Manuscript & Ribbon" visual identity: palette, embedded fonts, and egui style setup.
//!
//! The light "foolscap paper" theme is the flagship (and the default); the dark ink
//! theme mirrors it. Fonts: IBM Plex Sans (UI), IBM Plex Mono (typing targets, stats),
//! Young Serif (the wordmark and book titles), all under the SIL Open Font License and
//! recorded in NOTICE.

use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke};

use crate::core::config::Theme;

/// The named palette. Two themes share the same accent semantics.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// Window ground.
    pub ink_950: Color32,
    /// Cards / panels sitting on the ground.
    pub ink_850: Color32,
    /// Primary text / correctly-typed glyphs.
    pub paper: Color32,
    /// Untyped / upcoming text, secondary labels.
    pub ghost: Color32,
    /// Signature interactive accent: caret, next-key ring, primary buttons.
    pub verdigris: Color32,
    /// Secondary accent: wordmark, titles, personal bests, press-flash.
    pub brass: Color32,
    /// Errors and destructive actions only.
    pub ribbon: Color32,
    /// Mistyped-glyph ink: brighter and higher-contrast than `ribbon`, tuned so the
    /// error/correct difference survives grayscale (colorblind-safe together with the
    /// underline + background tint, which carry the state without hue).
    pub error_ink: Color32,
    /// Subtle background tint behind mistyped glyphs (luminance cue, not just hue).
    pub error_bg: Color32,
    /// Hairlines / borders on cards.
    pub edge: Color32,
    /// Keycap face, top highlight, bottom lip, edge stroke, and label.
    pub key_face: Color32,
    pub key_top: Color32,
    pub key_lip: Color32,
    pub key_edge: Color32,
    pub key_text: Color32,
    /// Eight muted finger-zone hues (index 0..8, thumbs share index 4).
    pub finger: [Color32; 9],
}

impl Palette {
    /// Flagship: warm foolscap paper, ink text, patina accents.
    pub fn light() -> Self {
        Palette {
            ink_950: Color32::from_rgb(0xF1, 0xEB, 0xDD), // foolscap ground
            ink_850: Color32::from_rgb(0xFA, 0xF6, 0xEC), // paper card
            paper: Color32::from_rgb(0x2B, 0x25, 0x1C),   // ink text
            ghost: Color32::from_rgb(0xA3, 0x97, 0x83),
            verdigris: Color32::from_rgb(0x1F, 0x6F, 0x62),
            brass: Color32::from_rgb(0x8F, 0x6D, 0x1F),
            ribbon: Color32::from_rgb(0xB0, 0x35, 0x2C),
            // Noticeably lighter than the near-black ink text in grayscale, on a light
            // red wash that darkens the cell against the paper card.
            error_ink: Color32::from_rgb(0xE0, 0x38, 0x1F),
            error_bg: Color32::from_rgb(0xF6, 0xD2, 0xC9),
            edge: Color32::from_rgb(0xDC, 0xD3, 0xC0),
            key_face: Color32::from_rgb(0xFB, 0xF8, 0xF0),
            key_top: Color32::from_rgb(0xFF, 0xFD, 0xF7),
            key_lip: Color32::from_rgb(0xC9, 0xBE, 0xA9),
            key_edge: Color32::from_rgb(0xCF, 0xC5, 0xB0),
            key_text: Color32::from_rgb(0x4A, 0x42, 0x36),
            finger: FINGER_LIGHT,
        }
    }

    /// Dark ink workspace for long sessions.
    pub fn dark() -> Self {
        Palette {
            ink_950: Color32::from_rgb(0x15, 0x12, 0x0E),
            ink_850: Color32::from_rgb(0x21, 0x1C, 0x16),
            paper: Color32::from_rgb(0xEA, 0xE3, 0xD5),
            ghost: Color32::from_rgb(0x85, 0x7A, 0x6A),
            verdigris: Color32::from_rgb(0x49, 0xA8, 0x97),
            brass: Color32::from_rgb(0xCF, 0xA9, 0x54),
            ribbon: Color32::from_rgb(0xD2, 0x4A, 0x41),
            // Noticeably darker than the warm paper text in grayscale, on a dark red
            // wash that lifts the cell off the ink ground.
            error_ink: Color32::from_rgb(0xFF, 0x72, 0x5C),
            error_bg: Color32::from_rgb(0x52, 0x22, 0x1B),
            edge: Color32::from_rgb(0x33, 0x2C, 0x23),
            key_face: Color32::from_rgb(0x2A, 0x24, 0x1D),
            key_top: Color32::from_rgb(0x32, 0x2B, 0x23),
            key_lip: Color32::from_rgb(0x0C, 0x0A, 0x08),
            key_edge: Color32::from_rgb(0x3E, 0x36, 0x2B),
            key_text: Color32::from_rgb(0xD8, 0xD0, 0xC0),
            finger: FINGER_DARK,
        }
    }

    pub fn for_theme(theme: Theme) -> Self {
        match theme {
            Theme::Dark => Self::dark(),
            Theme::Light => Self::light(),
        }
    }
}

/// Muted finger-zone hues used as small accents on the keycap lips.
const FINGER_LIGHT: [Color32; 9] = [
    Color32::from_rgb(0xB5, 0x6B, 0x8A), // left pinky  - plum
    Color32::from_rgb(0xA8, 0x8A, 0x3A), // left ring   - olive
    Color32::from_rgb(0x3E, 0x8F, 0xA0), // left middle - teal
    Color32::from_rgb(0x63, 0x9B, 0x4C), // left index  - moss
    Color32::from_rgb(0x9A, 0x8B, 0x74), // thumbs      - stone
    Color32::from_rgb(0xC0, 0x7A, 0x36), // right index - amber
    Color32::from_rgb(0x54, 0x7E, 0xC0), // right middle- steel blue
    Color32::from_rgb(0xB0, 0x64, 0x40), // right ring  - umber
    Color32::from_rgb(0x8F, 0x62, 0xA8), // right pinky - mauve
];

const FINGER_DARK: [Color32; 9] = [
    Color32::from_rgb(0xC8, 0x84, 0x9E),
    Color32::from_rgb(0xBB, 0xA0, 0x55),
    Color32::from_rgb(0x5E, 0xA8, 0xB8),
    Color32::from_rgb(0x7F, 0xB2, 0x69),
    Color32::from_rgb(0xA8, 0x9B, 0x86),
    Color32::from_rgb(0xD0, 0x92, 0x52),
    Color32::from_rgb(0x74, 0x97, 0xD4),
    Color32::from_rgb(0xC4, 0x7E, 0x5C),
    Color32::from_rgb(0xA8, 0x7E, 0xBE),
];

/// Font role helpers.
pub fn mono_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}
pub fn ui_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}
/// The literary display face (Young Serif), for the wordmark and book titles.
pub fn display_family() -> FontFamily {
    FontFamily::Name("display".into())
}
pub fn display_font(size: f32) -> FontId {
    FontId::new(size, display_family())
}

/// Install the embedded fonts: Plex Sans for UI, Plex Mono for typing/stats, Young Serif
/// as the named "display" family. egui's built-ins stay as fallbacks (emoji coverage).
pub fn install_fonts(ctx: &egui::Context) {
    use egui::epaint::text::{FontData, FontInsert, FontPriority, InsertFontFamily};

    let inserts = [
        (
            "plex-sans",
            &include_bytes!("../../assets/fonts/IBMPlexSans-Regular.ttf")[..],
            vec![InsertFontFamily {
                family: FontFamily::Proportional,
                priority: FontPriority::Highest,
            }],
        ),
        (
            "plex-mono",
            &include_bytes!("../../assets/fonts/IBMPlexMono-Regular.ttf")[..],
            vec![InsertFontFamily {
                family: FontFamily::Monospace,
                priority: FontPriority::Highest,
            }],
        ),
        (
            "young-serif",
            &include_bytes!("../../assets/fonts/YoungSerif-Regular.ttf")[..],
            vec![InsertFontFamily {
                family: display_family(),
                priority: FontPriority::Highest,
            }],
        ),
    ];
    for (name, bytes, families) in inserts {
        ctx.add_font(FontInsert::new(
            name,
            FontData::from_static(bytes),
            families,
        ));
    }
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
    visuals.selection.bg_fill = p.verdigris.linear_multiply(0.35);
    visuals.selection.stroke = Stroke::new(1.0, p.verdigris);
    visuals.widgets.noninteractive.bg_fill = p.ink_850;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.edge);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.paper);
    // Idle small controls (checkbox boxes, slider rails) are drawn with bg_fill and
    // bg_stroke and usually sit on ink_850 cards: with bg_fill = ink_850 and the faint
    // `edge` stroke an unchecked checkbox was nearly invisible. Fill them with the
    // window ground (a subtle well against the card) and outline them with `ghost`,
    // which reads clearly on both themes. Buttons keep their key_face weak_bg_fill;
    // only the outline color changes, never the stroke width, so the
    // no-shift-on-hover invariant below still holds.
    visuals.widgets.inactive.bg_fill = p.ink_950;
    visuals.widgets.inactive.weak_bg_fill = p.key_face;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, p.ghost);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, p.paper);
    visuals.widgets.hovered.bg_fill = p.verdigris.linear_multiply(0.18);
    visuals.widgets.hovered.weak_bg_fill = p.verdigris.linear_multiply(0.14);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, p.verdigris);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, p.paper);
    visuals.widgets.active.bg_fill = p.verdigris.linear_multiply(0.4);
    visuals.widgets.active.weak_bg_fill = p.verdigris.linear_multiply(0.3);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, p.verdigris);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, p.paper);
    visuals.widgets.open.bg_fill = p.ink_850;
    visuals.widgets.open.weak_bg_fill = p.ink_850;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, p.edge);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, p.paper);
    // Labels must never move on hover. egui sizes a button's frame as
    // button_padding + expansion - bg_stroke.width (the stroke is then added back to the
    // total margin when the frame is drawn), but an UNSELECTED selectable label draws no
    // frame at all while idle, so its stroke never gets added back. Its idle padding is
    // therefore short by (bg_stroke.width - expansion); on hover the frame appears and
    // the label shifts by exactly that much. Keeping every interactive state at the SAME
    // stroke width and the SAME expansion, with expansion == stroke width, makes the
    // idle and hovered layouts identical, so nothing shifts, app-wide.
    for w in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        w.expansion = w.bg_stroke.width;
    }
    let radius = CornerRadius::same(6);
    visuals.widgets.noninteractive.corner_radius = radius;
    visuals.widgets.inactive.corner_radius = radius;
    visuals.widgets.hovered.corner_radius = radius;
    visuals.widgets.active.corner_radius = radius;
    visuals.widgets.open.corner_radius = radius;
    visuals.window_corner_radius = CornerRadius::same(10);
    ctx.all_styles_mut(|style| {
        style.visuals = visuals.clone();
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 7.0);
        // Scrollbars: solid (they reserve their own lane instead of floating over the
        // content), wide enough to grab, and high-contrast (the handle uses the text
        // color, so it reads on both themes).
        style.spacing.scroll = egui::style::ScrollStyle::solid();
        style.spacing.scroll.bar_width = 12.0;
        style.spacing.scroll.handle_min_length = 24.0;
        style.spacing.scroll.bar_inner_margin = 6.0;
        style.spacing.scroll.foreground_color = true;
        // A slightly larger, calmer type scale.
        style
            .text_styles
            .insert(egui::TextStyle::Body, ui_font(14.5));
        style
            .text_styles
            .insert(egui::TextStyle::Button, ui_font(14.5));
        style
            .text_styles
            .insert(egui::TextStyle::Small, ui_font(11.5));
        style
            .text_styles
            .insert(egui::TextStyle::Heading, ui_font(22.0));
        style
            .text_styles
            .insert(egui::TextStyle::Monospace, mono_font(14.0));
    });
}

/// Plain-language explanations for every stat, shared by the results screen and the live
/// HUD so hovering any number tells the user what it means.
pub mod stat_tips {
    pub const WPM: &str = "Words per minute: correctly typed characters, five per word, \
per minute of active typing.";
    pub const RAW: &str = "Raw WPM: every keystroke counted, right or wrong, five per \
word per minute. Speed ignoring accuracy.";
    pub const ACCURACY: &str = "Correct keystrokes as a share of all keystrokes.";
    pub const CONSISTENCY: &str =
        "How steady your pace was, 0-100; higher means fewer bursts and stalls.";
    pub const NET_WPM: &str =
        "The classic exam formula: gross WPM minus one per uncorrected error per minute.";
    pub const TIME: &str = "Active typing time; pauses do not count.";
    pub const TIME_LIVE: &str =
        "Active typing time; pauses do not count. Timed drills show the time left.";
    pub const PROGRESS: &str = "How much of the text you have typed so far.";
}

/// The app-wide full-page scroll area. It spans the whole window (no auto-shrink) so the
/// always-visible scrollbar hugs the window edge instead of floating mid-page; the
/// content centers itself inside it.
pub fn page_scroll(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .show(ui, |ui| add(ui));
}

/// A centered content column: everything on screen lives on this grid.
pub fn centered_column<R>(
    ui: &mut egui::Ui,
    max_w: f32,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let w = ui.available_width().min(max_w);
    let pad = ((ui.available_width() - w) / 2.0).max(0.0);
    let mut result = None;
    ui.horizontal(|ui| {
        ui.add_space(pad);
        ui.vertical(|ui| {
            ui.set_width(w);
            result = Some(add(ui));
        });
    });
    result.unwrap()
}

/// A "Label: [controls]" row with the label vertically centered against the controls.
/// Plain `ui.horizontal` centers the first-added short label against a provisional row
/// height, so labels sit a few pixels above taller widgets (buttons/selectables); a
/// zero-width spacer establishes the real control height first.
pub fn control_row<R>(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.horizontal(|ui| {
        let h =
            ui.text_style_height(&egui::TextStyle::Button) + 2.0 * ui.spacing().button_padding.y;
        ui.allocate_exact_size(egui::vec2(0.0, h), egui::Sense::hover());
        ui.add_space(-ui.spacing().item_spacing.x);
        ui.label(label);
        add(ui)
    })
    .inner
}

/// A fixed-size action button with its label properly centered (a Button given only
/// min_size leaves the label left-aligned with lopsided padding on some backends).
/// `accent` fills it as the primary action; plain ones read as secondary.
pub fn action_button(
    ui: &mut egui::Ui,
    p: &Palette,
    label: &str,
    text_size: f32,
    size: egui::Vec2,
    accent: bool,
) -> egui::Response {
    let text = if accent {
        egui::RichText::new(label)
            .size(text_size)
            .strong()
            .color(p.ink_850)
    } else {
        egui::RichText::new(label).size(text_size)
    };
    let mut button = egui::Button::new(text).corner_radius(CornerRadius::same(8));
    if accent {
        button = button.fill(p.verdigris);
    }
    ui.add_sized(size, button)
}

/// A paper card frame sitting on the ground.
pub fn card(p: &Palette) -> egui::Frame {
    egui::Frame::new()
        .fill(p.ink_850)
        .stroke(Stroke::new(1.0, p.edge))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(18, 14))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hovering must never move a label: every interactive widget state shares one
    /// bg-stroke width and one expansion, and expansion equals the stroke width (the
    /// frameless-idle selectable-label case sizes as padding + expansion - stroke_width,
    /// while framed states size as exactly padding; equality keeps them identical).
    #[test]
    fn widget_states_never_shift_on_hover() {
        for theme in [Theme::Light, Theme::Dark] {
            let ctx = egui::Context::default();
            apply_style(&ctx, theme);
            // apply_style writes the same visuals into every egui style slot.
            let style = ctx.style_of(egui::Theme::Dark);
            let w = &style.visuals.widgets;
            let states = [
                ("inactive", &w.inactive),
                ("hovered", &w.hovered),
                ("active", &w.active),
                ("open", &w.open),
            ];
            let width = w.inactive.bg_stroke.width;
            let expansion = w.inactive.expansion;
            for (name, s) in states {
                assert_eq!(
                    s.bg_stroke.width, width,
                    "{theme:?}/{name}: bg stroke width differs across states"
                );
                assert_eq!(
                    s.expansion, expansion,
                    "{theme:?}/{name}: expansion differs across states"
                );
                assert_eq!(
                    s.expansion, s.bg_stroke.width,
                    "{theme:?}/{name}: expansion must equal stroke width or idle \
selectable labels shift on hover"
                );
            }
        }
    }
}
