//! The typing stage: a centered play column holding the stats HUD, the manuscript panel
//! (typed text bright, upcoming ghosted, errors ribbon-underlined, an unmissable current
//! glyph), and the full-size keyboard docked below. Random-keys targets render as
//! per-token chips instead of glyph cells.

use egui::{Align2, Color32, CornerRadius, FontId, Rect, Sense, Stroke, Vec2};

use crate::core::config::{CaretStyle, ContentMode, KeyboardMode};
use crate::core::session::{CharStatus, Session};
use crate::core::text_source::Expected;
use crate::ui::app::{App, Screen};
use crate::ui::keyboard;
use crate::ui::theme::{self, Palette};

/// The shared width of the play column.
pub const COLUMN_W: f32 = 960.0;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let p = app.palette();

    // Paste mode with no session yet: show the paste box.
    if app.config.content_mode == ContentMode::Paste && app.session.is_none() {
        theme::centered_column(ui, COLUMN_W, |ui| paste_entry(app, ui));
        return;
    }

    if app.session.is_none() {
        ui.vertical_centered(|ui| {
            ui.add_space(100.0);
            if app.config.content_mode == ContentMode::Book {
                ui.label(
                    egui::RichText::new("Open a book from the Books shelf to start typing.")
                        .color(p.ghost)
                        .size(16.0),
                );
                ui.add_space(8.0);
                if ui.button("Go to Books").clicked() {
                    app.screen = Screen::Books;
                }
            } else {
                ui.label(egui::RichText::new("Ready.").color(p.ghost).size(16.0));
                ui.add_space(8.0);
                if ui.button("Start").clicked() {
                    app.start_session();
                }
            }
        });
        return;
    }

    // Keyboard docked at the bottom first (so the central column gets the remainder).
    let next_key = app.session.as_ref().and_then(|s| s.next_physical_key());
    let needs_shift = app
        .session
        .as_ref()
        .and_then(|s| s.expected().map(|e| e.needs_shift()))
        .unwrap_or(false);
    let now = app.session_started_secs().unwrap_or(0.0);
    if app.config.keyboard_mode != KeyboardMode::Hidden {
        egui::Panel::bottom("keyboard_dock")
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(p.ink_950))
            .show(ui, |ui| {
                ui.add_space(2.0);
                theme::centered_column(ui, 1160.0, |ui| {
                    keyboard::draw(
                        ui,
                        &p,
                        app.config.keyboard_mode,
                        next_key,
                        needs_shift,
                        &app.flash,
                        app.config.reduced_motion,
                        now,
                        app.config.show_numpad,
                    );
                });
                ui.add_space(10.0);
            });
    }

    theme::centered_column(ui, COLUMN_W, |ui| {
        ui.add_space(18.0);
        hud(app, ui);
        ui.add_space(10.0);

        // The manuscript panel, sized to its content.
        let paused = app.is_paused();
        theme::card(&p).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let is_keys_target = app
                .session
                .as_ref()
                .map(|s| s.target.items.iter().any(|e| !e.is_char()))
                .unwrap_or(false);
            if let Some(session) = &app.session {
                if is_keys_target {
                    chips_panel(ui, &p, session, paused);
                } else {
                    manuscript_panel(ui, &p, session, app.config.caret, paused);
                }
            }
        });

        // Book mode: a thin spine fills as the chapter is typed (whole-chapter progress,
        // including any resumed portion).
        if app.config.content_mode == ContentMode::Book && app.session.is_some() {
            ui.add_space(8.0);
            spine(ui, &p, app.session_progress_fraction());
        }

        // Paused overlay controls.
        if paused {
            ui.add_space(14.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new("Paused")
                        .font(theme::display_font(26.0))
                        .color(p.brass),
                );
                ui.label(
                    egui::RichText::new(
                        "The clock is stopped; paused time never counts. Press Space to resume.",
                    )
                    .color(p.ghost)
                    .size(13.0),
                );
                ui.add_space(6.0);
                let btn = egui::Button::new(
                    egui::RichText::new("Resume (Space)")
                        .size(16.0)
                        .strong()
                        .color(p.ink_850),
                )
                .fill(p.verdigris)
                .min_size(egui::vec2(180.0, 40.0))
                .corner_radius(CornerRadius::same(8));
                if ui.add(btn).clicked() {
                    app.toggle_pause();
                }
            });
        }

        // Press-Space-to-start gate: shown until the first Space starts the clock.
        if app.awaiting_start && !paused {
            start_gate(app, ui);
        }
    });
}

/// The pre-drill gate: a clear "Press Space to start" plus, for the timed drills, a quick
/// duration picker so drill length is not buried in Settings. Picking a duration applies
/// to this (not yet started) drill and persists as the new default.
fn start_gate(app: &mut App, ui: &mut egui::Ui) {
    let p = app.palette();
    ui.add_space(14.0);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new("Press Space to start")
                .font(theme::display_font(26.0))
                .color(p.verdigris),
        );
        ui.label(
            egui::RichText::new("The clock starts on Space; that press isn't counted as typing.")
                .color(p.ghost)
                .size(13.0),
        );
        let timed = matches!(
            app.config.content_mode,
            ContentMode::Random | ContentMode::Word
        );
        if timed {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                // Center the segmented control on the column.
                let seg_w = 5.0 * 46.0 + 90.0;
                ui.add_space(((ui.available_width() - seg_w) / 2.0).max(0.0));
                ui.label(egui::RichText::new("Duration:").color(p.ghost).size(12.5));
                for (secs, label) in crate::core::config::DRILL_PRESETS {
                    if ui
                        .selectable_label(app.config.drill_secs == secs, label)
                        .clicked()
                    {
                        app.config.drill_secs = secs;
                        app.save_config();
                        // The drill has not started yet: apply to it directly.
                        if let Some(s) = app.session.as_mut() {
                            s.time_limit_secs = Some(secs as f64);
                        }
                    }
                }
            });
        }
    });
}

/// The stats HUD: one aligned baseline of tabular numbers on the play column.
fn hud(app: &mut App, ui: &mut egui::Ui) {
    let p = app.palette();
    let (wpm, raw, acc, cons, time_str, right_str) = match &app.session {
        Some(s) => {
            let active = app.session_secs();
            let time_str = match s.time_left(active) {
                Some(left) => format_clock(left),
                None => format_clock(active),
            };
            let right = match app.config.content_mode {
                ContentMode::Book | ContentMode::Paste => {
                    format!("{:.0}%", app.session_progress_fraction() * 100.0)
                }
                _ => String::new(),
            };
            (
                s.metrics.wpm(),
                s.metrics.raw_wpm(),
                s.metrics.accuracy() * 100.0,
                s.metrics.consistency(),
                time_str,
                right,
            )
        }
        None => (0.0, 0.0, 100.0, 100.0, "0:00".to_string(), String::new()),
    };

    ui.horizontal(|ui| {
        stat(ui, &p, &format!("{wpm:.0}"), "wpm", p.verdigris);
        stat(ui, &p, &format!("{raw:.0}"), "raw", p.ghost);
        stat(ui, &p, &format!("{acc:.0}%"), "accuracy", p.paper);
        stat(ui, &p, &format!("{cons:.0}"), "consistency", p.paper);
        stat(ui, &p, &time_str, "time", p.brass);
        if !right_str.is_empty() {
            stat(ui, &p, &right_str, "progress", p.ghost);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Pause / resume control (meaningless before the clock starts).
            if !app.awaiting_start {
                let label = if app.is_paused() { "Resume" } else { "Pause" };
                if ui.button(label).clicked() {
                    app.toggle_pause();
                }
                // Reset stats: zero the clock and metrics, keep the typing position
                // (useful when a distraction ruined a chapter's numbers mid-way).
                if matches!(
                    app.config.content_mode,
                    ContentMode::Paste | ContentMode::Book
                ) && ui
                    .button("Reset stats")
                    .on_hover_text("Zero the timer and metrics; your position in the text is kept.")
                    .clicked()
                {
                    app.reset_session_stats();
                }
            }
            // The quiet context label: what is being typed.
            let context = match app.config.content_mode {
                ContentMode::Word => "word drill".to_string(),
                ContentMode::Random => "random keys".to_string(),
                ContentMode::Paste => "pasted text".to_string(),
                ContentMode::Book => app
                    .session
                    .as_ref()
                    .map(|s| s.target.title.clone())
                    .unwrap_or_default(),
            };
            ui.label(egui::RichText::new(context).color(p.ghost).size(12.5));
        });
    });
}

fn stat(ui: &mut egui::Ui, p: &Palette, value: &str, label: &str, color: Color32) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(value)
                .color(color)
                .font(theme::mono_font(21.0))
                .strong(),
        );
        ui.label(egui::RichText::new(label).color(p.ghost).size(11.5));
    });
    ui.add_space(10.0);
}

fn format_clock(secs: f64) -> String {
    let s = secs.max(0.0).round() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Character targets (word / paste / book): a wrapped monospace manuscript with per-char
/// status coloring and a high-contrast current glyph.
fn manuscript_panel(
    ui: &mut egui::Ui,
    p: &Palette,
    session: &Session,
    caret: CaretStyle,
    paused: bool,
) {
    let font_size = 25.0;
    let font = FontId::monospace(font_size);
    let char_w = font_size * 0.601; // Plex Mono advance
    let line_h = font_size * 1.55;

    let avail_w = ui.available_width();
    let cols = ((avail_w / char_w).floor() as usize).max(10);

    // Word-aware wrap: whole words move to the next line instead of breaking mid-word;
    // newlines force a wrap. Words longer than a line hard-break.
    let items = &session.target.items;
    let mut lines: Vec<Vec<usize>> = vec![Vec::new()];
    let mut col = 0usize;
    let mut i = 0usize;
    while i < items.len() {
        match &items[i] {
            Expected::Char('\n') => {
                lines.last_mut().unwrap().push(i);
                lines.push(Vec::new());
                col = 0;
                i += 1;
            }
            Expected::Char(' ') => {
                lines.last_mut().unwrap().push(i);
                col += 1;
                if col >= cols {
                    lines.push(Vec::new());
                    col = 0;
                }
                i += 1;
            }
            _ => {
                // Measure the word (up to the next space/newline).
                let mut j = i;
                while j < items.len()
                    && !matches!(items[j], Expected::Char(' ') | Expected::Char('\n'))
                {
                    j += 1;
                }
                let word_len = j - i;
                if col > 0 && col + word_len > cols && word_len <= cols {
                    lines.push(Vec::new());
                    col = 0;
                }
                for idx in i..j {
                    if col >= cols {
                        lines.push(Vec::new());
                        col = 0;
                    }
                    lines.last_mut().unwrap().push(idx);
                    col += 1;
                }
                i = j;
            }
        }
    }

    // A window of lines around the caret; the panel sizes to what it shows.
    let caret_i = session.cursor.min(items.len().saturating_sub(1));
    let caret_line = lines.iter().position(|l| l.contains(&caret_i)).unwrap_or(0);
    let max_lines = 4usize;
    let shown = lines.len().min(max_lines);
    let start_line = caret_line
        .saturating_sub(1)
        .min(lines.len().saturating_sub(shown));
    let end_line = (start_line + shown).min(lines.len());

    let strip_h = line_h * shown as f32 + 10.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(avail_w, strip_h), Sense::hover());
    let painter = ui.painter_at(rect);

    let mut y = rect.top() + 8.0 + font_size * 0.55;
    for line in lines.iter().take(end_line).skip(start_line) {
        let mut x = rect.left();
        for &i in line {
            let item = &items[i];
            let status = session.status[i];
            let ch = match item {
                Expected::Char(c) => *c,
                Expected::PhysicalKey(_) => ' ',
            };
            let is_current = i == session.cursor && !session.is_complete();
            let (mut color, underline) = match status {
                CharStatus::Pending => (p.ghost, false),
                CharStatus::Correct => (p.paper, false),
                CharStatus::Wrong => (p.ribbon, true),
            };

            // The current glyph is the most prominent thing on screen: a solid accent
            // block with a background-colored glyph (block caret), or a strong under-bar
            // with a brightened glyph.
            if is_current {
                match caret {
                    CaretStyle::Block => {
                        let r = Rect::from_min_size(
                            egui::pos2(x - 1.0, y - font_size * 0.62),
                            Vec2::new(char_w + 2.0, font_size * 1.24),
                        );
                        painter.rect_filled(r, CornerRadius::same(3), p.verdigris);
                        color = p.ink_850; // paper-on-accent: maximum contrast
                    }
                    CaretStyle::Bar => {
                        painter.line_segment(
                            [
                                egui::pos2(x - 1.5, y - font_size * 0.62),
                                egui::pos2(x - 1.5, y + font_size * 0.55),
                            ],
                            Stroke::new(2.5, p.verdigris),
                        );
                        color = p.paper;
                    }
                    CaretStyle::Underline => {
                        painter.line_segment(
                            [
                                egui::pos2(x, y + font_size * 0.58),
                                egui::pos2(x + char_w, y + font_size * 0.58),
                            ],
                            Stroke::new(3.0, p.verdigris),
                        );
                        color = p.paper;
                    }
                }
            }

            // Show newlines as a dim pilcrow so the user knows to press Enter.
            let display = if ch == '\n' { '\u{00B6}' } else { ch };
            let color = if ch == '\n' && !is_current {
                color.linear_multiply(0.55)
            } else {
                color
            };
            painter.text(
                egui::pos2(x, y),
                Align2::LEFT_CENTER,
                display,
                font.clone(),
                color,
            );
            if underline && !is_current {
                painter.line_segment(
                    [
                        egui::pos2(x, y + font_size * 0.58),
                        egui::pos2(x + char_w, y + font_size * 0.58),
                    ],
                    Stroke::new(2.0, p.ribbon),
                );
            }
            x += char_w;
        }
        y += line_h;
    }

    if paused {
        blur_overlay(&painter, rect, p);
    }
}

/// Physical-key targets (Random keys): each key name is its own measured chip.
fn chips_panel(ui: &mut egui::Ui, p: &Palette, session: &Session, paused: bool) {
    let font = FontId::monospace(15.0);
    let pad_x = 10.0;
    let chip_h = 30.0;
    let gap = 8.0;
    let row_gap = 10.0;

    let avail_w = ui.available_width();
    let items = &session.target.items;

    // Measure every chip, then flow them into rows.
    let widths: Vec<f32> = ui.fonts_mut(|f| {
        items
            .iter()
            .map(|e| {
                let label = e.label();
                let galley = f.layout_no_wrap(label, font.clone(), Color32::WHITE);
                galley.size().x + pad_x * 2.0
            })
            .collect()
    });
    let mut rows: Vec<Vec<usize>> = vec![Vec::new()];
    let mut x = 0.0;
    for (i, w) in widths.iter().enumerate() {
        if x + w > avail_w && !rows.last().unwrap().is_empty() {
            rows.push(Vec::new());
            x = 0.0;
        }
        rows.last_mut().unwrap().push(i);
        x += w + gap;
    }

    // Window of rows around the cursor.
    let cur = session.cursor.min(items.len().saturating_sub(1));
    let cur_row = rows.iter().position(|r| r.contains(&cur)).unwrap_or(0);
    let max_rows = 4usize;
    let shown = rows.len().min(max_rows);
    let start = cur_row
        .saturating_sub(1)
        .min(rows.len().saturating_sub(shown));
    let end = (start + shown).min(rows.len());

    let panel_h = shown as f32 * (chip_h + row_gap) + 6.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(avail_w, panel_h), Sense::hover());
    let painter = ui.painter_at(rect);

    let mut y = rect.top() + 4.0;
    for row in rows.iter().take(end).skip(start) {
        let mut x = rect.left();
        for &i in row {
            let w = widths[i];
            let r = Rect::from_min_size(egui::pos2(x, y), Vec2::new(w, chip_h));
            let status = session.status[i];
            let is_current = i == session.cursor && !session.is_complete();
            let (fill, text_c, stroke) = if is_current {
                (p.verdigris, p.ink_850, Stroke::new(1.5, p.verdigris))
            } else {
                match status {
                    CharStatus::Pending => (p.ink_850, p.ghost, Stroke::new(1.0, p.edge)),
                    CharStatus::Correct => (
                        p.verdigris.linear_multiply(0.12),
                        p.paper,
                        Stroke::new(1.0, p.edge),
                    ),
                    CharStatus::Wrong => (
                        p.ribbon.linear_multiply(0.15),
                        p.ribbon,
                        Stroke::new(1.2, p.ribbon),
                    ),
                }
            };
            painter.rect_filled(r, CornerRadius::same(7), fill);
            painter.rect_stroke(r, CornerRadius::same(7), stroke, egui::StrokeKind::Inside);
            painter.text(
                r.center(),
                Align2::CENTER_CENTER,
                items[i].label(),
                font.clone(),
                text_c,
            );
            x += w + gap;
        }
        y += chip_h + row_gap;
    }

    if paused {
        blur_overlay(&painter, rect, p);
    }
}

/// Dim/veil the target so it cannot be read ahead while paused.
fn blur_overlay(painter: &egui::Painter, rect: Rect, p: &Palette) {
    painter.rect_filled(
        rect.expand(6.0),
        CornerRadius::same(8),
        p.ink_850.gamma_multiply(0.93),
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        "\u{2016}",
        FontId::monospace(30.0),
        p.ghost,
    );
}

/// A thin spine/progress bar that fills as the chapter is typed (Book mode).
fn spine(ui: &mut egui::Ui, p: &Palette, frac: f32) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 6.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(3), p.edge);
    let fill = Rect::from_min_size(rect.min, Vec2::new(w * frac.clamp(0.0, 1.0), 6.0));
    painter.rect_filled(fill, CornerRadius::same(3), p.brass);
}

fn paste_entry(app: &mut App, ui: &mut egui::Ui) {
    let p = app.palette();
    ui.add_space(26.0);
    ui.label(
        egui::RichText::new("Paste the text you want to type")
            .font(theme::display_font(22.0))
            .color(p.brass),
    );
    ui.label(
        egui::RichText::new("Fancy punctuation and accents are simplified into plain keystrokes.")
            .color(p.ghost)
            .size(12.5),
    );
    ui.add_space(8.0);
    theme::card(&p).show(ui, |ui| {
        ui.add(
            egui::TextEdit::multiline(&mut app.paste_input)
                .desired_rows(8)
                .desired_width(f32::INFINITY)
                .frame(egui::Frame::new())
                .hint_text("Paste or type any text here, then press Start."),
        );
    });
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        let empty = app.paste_input.trim().is_empty();
        let btn = egui::Button::new(
            egui::RichText::new("Start typing this")
                .size(15.0)
                .strong()
                .color(p.ink_850),
        )
        .fill(p.verdigris)
        .min_size(egui::vec2(180.0, 38.0))
        .corner_radius(CornerRadius::same(8));
        if ui.add_enabled(!empty, btn).clicked() {
            app.start_session();
        }
        if app.paste_input.chars().count() > 20_000 {
            ui.label(
                egui::RichText::new("Note: only the first 20,000 characters will be used.")
                    .color(p.ribbon),
            );
        }
    });
}
