//! The central typing stage: the manuscript strip (upcoming text ghosted, typed text
//! bright, errors ribbon+underline, a verdigris caret), a quiet monospace HUD above, and
//! the on-screen keyboard docked below.

use egui::{Align2, Color32, FontId, Sense, Stroke, Vec2};

use crate::core::config::{CaretStyle, ContentMode};
use crate::core::session::CharStatus;
use crate::core::text_source::Expected;
use crate::ui::app::{App, Screen};
use crate::ui::keyboard;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let p = app.palette();

    // Paste mode with no session yet: show the paste box.
    if app.config.content_mode == ContentMode::Paste && app.session.is_none() {
        paste_entry(app, ui);
        return;
    }

    if app.session.is_none() {
        ui.vertical_centered(|ui| {
            ui.add_space(80.0);
            if app.config.content_mode == ContentMode::Book {
                ui.label(
                    egui::RichText::new("Open a book from the Books shelf to start typing.")
                        .color(p.ghost)
                        .size(16.0),
                );
                if ui.button("Go to Books").clicked() {
                    app.screen = Screen::Books;
                }
            } else {
                ui.label(egui::RichText::new("Ready.").color(p.ghost).size(16.0));
                if ui.button("Start").clicked() {
                    app.start_session();
                }
            }
        });
        return;
    }

    // HUD.
    hud(app, ui);
    ui.add_space(8.0);

    // Manuscript strip.
    manuscript(app, ui);

    // Spine progress in Book mode.
    if app.config.content_mode == ContentMode::Book {
        if let Some(s) = &app.session {
            ui.add_space(6.0);
            spine(ui, &p, s.progress_fraction());
        }
    }

    // Dock the keyboard at the bottom.
    let next_key = app.session.as_ref().and_then(|s| s.next_physical_key());
    let now = app.session_started_secs().unwrap_or(0.0);
    egui::Panel::bottom("keyboard_dock")
        .show_separator_line(false)
        .frame(egui::Frame::NONE.fill(p.ink_950))
        .show(ui, |ui| {
            ui.add_space(6.0);
            keyboard::draw(
                ui,
                &p,
                app.config.keyboard_mode,
                next_key,
                &app.flash,
                app.config.reduced_motion,
                now,
            );
            ui.add_space(6.0);
        });
}

fn hud(app: &App, ui: &mut egui::Ui) {
    let p = app.palette();
    let (wpm, raw, acc, cons, secs, frac) = match &app.session {
        Some(s) => (
            s.metrics.wpm(),
            s.metrics.raw_wpm(),
            s.metrics.accuracy() * 100.0,
            s.metrics.consistency(),
            s.metrics.elapsed_secs,
            s.progress_fraction() * 100.0,
        ),
        None => (0.0, 0.0, 100.0, 100.0, 0.0, 0.0),
    };
    ui.horizontal(|ui| {
        stat(ui, &p, "wpm", &format!("{wpm:.0}"), p.verdigris);
        stat(ui, &p, "raw", &format!("{raw:.0}"), p.ghost);
        stat(ui, &p, "acc", &format!("{acc:.0}%"), p.paper);
        stat(ui, &p, "consistency", &format!("{cons:.0}"), p.paper);
        stat(ui, &p, "time", &format!("{secs:.0}s"), p.ghost);
        stat(ui, &p, "progress", &format!("{frac:.0}%"), p.brass);
        if let Some(s) = &app.session {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(&s.target.title)
                        .color(p.brass)
                        .italics(),
                );
            });
        }
    });
}

fn stat(
    ui: &mut egui::Ui,
    p: &crate::ui::theme::Palette,
    label: &str,
    value: &str,
    color: Color32,
) {
    ui.vertical(|ui| {
        ui.label(
            egui::RichText::new(value)
                .color(color)
                .size(22.0)
                .monospace()
                .strong(),
        );
        ui.label(egui::RichText::new(label).color(p.ghost).size(11.0));
    });
    ui.add_space(18.0);
}

/// Draw the manuscript strip: a wrapped monospace rendering of the target with per-char
/// status coloring and a caret at the cursor.
fn manuscript(app: &App, ui: &mut egui::Ui) {
    let p = app.palette();
    let Some(session) = &app.session else {
        return;
    };
    let font_size = 26.0;
    let font = FontId::monospace(font_size);
    let char_w = font_size * 0.62;
    let line_h = font_size * 1.5;

    let avail_w = ui.available_width().min(900.0);
    let cols = ((avail_w / char_w).floor() as usize).max(10);

    // Wrap items into lines, breaking on spaces where possible.
    let items = &session.target.items;
    let mut lines: Vec<Vec<usize>> = vec![Vec::new()];
    let mut col = 0;
    for (i, item) in items.iter().enumerate() {
        if col >= cols {
            lines.push(Vec::new());
            col = 0;
        }
        lines.last_mut().unwrap().push(i);
        col += 1;
        // Prefer to wrap after spaces near the edge.
        if matches!(item, Expected::Char(' ')) && col > cols.saturating_sub(6) {
            lines.push(Vec::new());
            col = 0;
        }
    }

    // Show a window of lines around the caret so long texts scroll.
    let caret = session.cursor.min(items.len().saturating_sub(1));
    let caret_line = lines.iter().position(|l| l.contains(&caret)).unwrap_or(0);
    let max_lines = 6usize;
    let start_line = caret_line
        .saturating_sub(2)
        .min(lines.len().saturating_sub(max_lines));
    let end_line = (start_line + max_lines).min(lines.len());

    let strip_h = line_h * max_lines as f32 + 24.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(avail_w, strip_h), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, egui::CornerRadius::same(8), p.ink_850);

    let pad = 16.0;
    let mut y = rect.top() + pad + font_size * 0.5;
    for line in lines.iter().take(end_line).skip(start_line) {
        let mut x = rect.left() + pad;
        for &i in line {
            let item = &items[i];
            let status = session.status[i];
            let ch = match item {
                Expected::Char(c) => *c,
                Expected::PhysicalKey(_) => '\u{25A1}',
            };
            let (color, underline) = match status {
                CharStatus::Pending => (p.ghost, false),
                CharStatus::Correct => (p.paper, false),
                CharStatus::Wrong => (p.ribbon, true),
            };
            // Caret.
            if i == session.cursor {
                draw_caret(&painter, &p, app.config.caret, x, y, char_w, font_size);
            }
            // For physical-key targets, draw the key label instead of the box glyph.
            if let Expected::PhysicalKey(_) = item {
                painter.text(
                    egui::pos2(x, y),
                    Align2::LEFT_CENTER,
                    item.label(),
                    FontId::monospace(font_size * 0.5),
                    color,
                );
            } else {
                let display = if ch == ' ' { ' ' } else { ch };
                painter.text(
                    egui::pos2(x, y),
                    Align2::LEFT_CENTER,
                    display,
                    font.clone(),
                    color,
                );
                if underline {
                    painter.line_segment(
                        [
                            egui::pos2(x, y + font_size * 0.55),
                            egui::pos2(x + char_w, y + font_size * 0.55),
                        ],
                        Stroke::new(2.0, p.ribbon),
                    );
                }
            }
            x += char_w;
        }
        // Caret at end of the target (after last char).
        if session.cursor == items.len() && line == lines.last().unwrap() {
            draw_caret(&painter, &p, app.config.caret, x, y, char_w, font_size);
        }
        y += line_h;
    }
}

fn draw_caret(
    painter: &egui::Painter,
    p: &crate::ui::theme::Palette,
    style: CaretStyle,
    x: f32,
    y: f32,
    char_w: f32,
    font_size: f32,
) {
    match style {
        CaretStyle::Block => {
            let r = egui::Rect::from_min_size(
                egui::pos2(x, y - font_size * 0.6),
                Vec2::new(char_w, font_size * 1.15),
            );
            painter.rect_filled(
                r,
                egui::CornerRadius::same(2),
                p.verdigris.linear_multiply(0.55),
            );
        }
        CaretStyle::Bar => {
            painter.line_segment(
                [
                    egui::pos2(x, y - font_size * 0.6),
                    egui::pos2(x, y + font_size * 0.55),
                ],
                Stroke::new(2.5, p.verdigris),
            );
        }
        CaretStyle::Underline => {
            painter.line_segment(
                [
                    egui::pos2(x, y + font_size * 0.6),
                    egui::pos2(x + char_w, y + font_size * 0.6),
                ],
                Stroke::new(3.0, p.verdigris),
            );
        }
    }
}

/// A thin spine/progress bar that fills as the chapter is typed (Book mode).
fn spine(ui: &mut egui::Ui, p: &crate::ui::theme::Palette, frac: f32) {
    let w = ui.available_width().min(900.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 8.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, egui::CornerRadius::same(4), p.ink_850);
    let fill = egui::Rect::from_min_size(rect.min, Vec2::new(w * frac.clamp(0.0, 1.0), 8.0));
    painter.rect_filled(fill, egui::CornerRadius::same(4), p.brass);
}

fn paste_entry(app: &mut App, ui: &mut egui::Ui) {
    let p = app.palette();
    ui.add_space(20.0);
    ui.label(
        egui::RichText::new("Paste the text you want to type")
            .color(p.brass)
            .size(18.0),
    );
    ui.add_space(8.0);
    ui.add(
        egui::TextEdit::multiline(&mut app.paste_input)
            .desired_rows(8)
            .desired_width(f32::INFINITY)
            .hint_text("Paste or type any text here, then press Start."),
    );
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        let empty = app.paste_input.trim().is_empty();
        if ui
            .add_enabled(!empty, egui::Button::new("Start typing this"))
            .clicked()
        {
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
