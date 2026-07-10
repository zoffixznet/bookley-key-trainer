//! Results: WPM-over-time line, per-key error/slowness feedback, accuracy, consistency,
//! and personal-best tracking, laid out on the shared centered column.

use egui::{Color32, CornerRadius, Sense, Stroke, Vec2};

use crate::core::config::ContentMode;
use crate::ui::app::{App, Screen};
use crate::ui::stage::COLUMN_W;
use crate::ui::theme::{self, Palette};

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let p = app.palette();
    let Some(result) = app.last_result.clone() else {
        ui.vertical_centered(|ui| {
            ui.add_space(80.0);
            ui.label(egui::RichText::new("No results yet.").color(p.ghost));
        });
        return;
    };

    egui::ScrollArea::vertical().show(ui, |ui| {
        theme::centered_column(ui, COLUMN_W, |ui| {
            ui.add_space(22.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Results")
                        .font(theme::display_font(26.0))
                        .color(p.brass),
                );
                if app.last_was_pb {
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new("  PERSONAL BEST  ")
                            .color(p.ink_850)
                            .background_color(p.brass)
                            .size(11.0)
                            .strong(),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(best) = app.stats.best_for(&result.mode) {
                        ui.label(
                            egui::RichText::new(format!("best {} wpm: {:.0}", result.mode, best))
                                .color(p.ghost)
                                .size(12.5),
                        );
                    }
                });
            });
            ui.add_space(14.0);

            // Big numbers on one baseline.
            ui.horizontal(|ui| {
                big_stat(ui, &p, "wpm", &format!("{:.0}", result.wpm), p.verdigris);
                big_stat(ui, &p, "raw", &format!("{:.0}", result.raw_wpm), p.ghost);
                big_stat(
                    ui,
                    &p,
                    "accuracy",
                    &format!("{:.0}%", result.accuracy * 100.0),
                    p.paper,
                );
                big_stat(
                    ui,
                    &p,
                    "consistency",
                    &format!("{:.0}", result.consistency),
                    p.paper,
                );
                big_stat(
                    ui,
                    &p,
                    "net wpm",
                    &format!("{:.0}", result.net_wpm),
                    p.ghost,
                );
                big_stat(
                    ui,
                    &p,
                    "time",
                    &format!(
                        "{}:{:02}",
                        result.elapsed_secs as u64 / 60,
                        result.elapsed_secs as u64 % 60
                    ),
                    p.brass,
                );
            });

            ui.add_space(18.0);
            section_label(ui, &p, "WPM OVER TIME");
            theme::card(&p).show(ui, |ui| {
                ui.set_width(ui.available_width());
                wpm_graph(ui, &p, &result);
            });

            ui.add_space(14.0);
            section_label(ui, &p, "SLOW AND ERROR-PRONE KEYS");
            heatmap(ui, &p, &result);

            ui.add_space(22.0);
            // Primary action: a proper big button, also triggered by Enter. Book chapters
            // do not get a "type again"; there the next step is continuing the book.
            let is_book = app.config.content_mode == ContentMode::Book;
            let label = if is_book {
                "Continue the book (Enter)"
            } else {
                "Type again (Enter)"
            };
            let mut go = false;
            ui.horizontal(|ui| {
                let btn = egui::Button::new(
                    egui::RichText::new(label)
                        .size(17.0)
                        .strong()
                        .color(p.ink_850),
                )
                .fill(p.verdigris)
                .min_size(egui::vec2(270.0, 46.0))
                .corner_radius(CornerRadius::same(9));
                if ui.add(btn).clicked() {
                    go = true;
                }
                ui.add_space(4.0);
                if !is_book && ui.button("Drill weak keys (Random)").clicked() {
                    app.config.content_mode = ContentMode::Random;
                    app.save_config();
                    app.start_session();
                }
            });
            // Enter shortcut, guarded so the keystroke that finished the session (or a
            // held Enter auto-repeat) cannot immediately restart it.
            let settled = app
                .results_at
                .map(|t| t.elapsed().as_millis() > 400)
                .unwrap_or(true);
            if settled && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                go = true;
            }
            if go {
                next_action(app);
            }
            ui.add_space(24.0);
        });
    });
}

fn next_action(app: &mut App) {
    if app.config.content_mode == ContentMode::Book {
        // Go back to books to continue the next chapter (or generate one).
        app.screen = Screen::Books;
    } else {
        app.start_session();
    }
}

fn section_label(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .color(p.ghost)
            .size(10.5)
            .extra_letter_spacing(1.4),
    );
    ui.add_space(2.0);
}

fn big_stat(ui: &mut egui::Ui, p: &Palette, label: &str, value: &str, color: Color32) {
    ui.vertical(|ui| {
        ui.label(
            egui::RichText::new(value)
                .color(color)
                .font(theme::mono_font(38.0)),
        );
        ui.label(
            egui::RichText::new(label)
                .color(p.ghost)
                .size(11.0)
                .extra_letter_spacing(0.8),
        );
    });
    ui.add_space(26.0);
}

fn wpm_graph(ui: &mut egui::Ui, p: &Palette, r: &crate::core::metrics::SessionResult) {
    let w = ui.available_width();
    let h = 150.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
    let painter = ui.painter_at(rect);
    if r.samples.len() < 2 {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Not enough data",
            egui::FontId::proportional(13.0),
            p.ghost,
        );
        return;
    }
    let pad = 8.0;
    let max_wpm = r
        .samples
        .iter()
        .map(|s| s.raw.max(s.wpm))
        .fold(1.0, f64::max)
        .max(10.0);
    let max_t = r.samples.last().unwrap().t.max(0.001);
    let plot = egui::Rect::from_min_max(
        egui::pos2(rect.left() + pad + 26.0, rect.top() + pad),
        egui::pos2(rect.right() - pad, rect.bottom() - pad - 4.0),
    );
    // Light gridlines + axis figures at 0 / mid / max.
    for frac in [0.0f32, 0.5, 1.0] {
        let y = plot.bottom() - frac * plot.height();
        painter.line_segment(
            [egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)],
            Stroke::new(1.0, p.edge),
        );
        painter.text(
            egui::pos2(plot.left() - 6.0, y),
            egui::Align2::RIGHT_CENTER,
            format!("{:.0}", max_wpm * frac as f64),
            egui::FontId::monospace(10.0),
            p.ghost,
        );
    }
    let map = |t: f64, v: f64| {
        egui::pos2(
            plot.left() + (t / max_t) as f32 * plot.width(),
            plot.bottom() - (v / max_wpm) as f32 * plot.height(),
        )
    };
    // Raw line (ghost) then wpm line (verdigris, heavier).
    for (key, color, width) in [("raw", p.ghost, 1.5), ("wpm", p.verdigris, 2.5)] {
        let pts: Vec<egui::Pos2> = r
            .samples
            .iter()
            .map(|s| map(s.t, if key == "raw" { s.raw } else { s.wpm }))
            .collect();
        for w2 in pts.windows(2) {
            painter.line_segment([w2[0], w2[1]], Stroke::new(width, color));
        }
    }
}

fn heatmap(ui: &mut egui::Ui, p: &Palette, r: &crate::core::metrics::SessionResult) {
    // Show up to the 12 worst keys (by errors then latency).
    let mut keys: Vec<&(String, u32, u32, f64)> = r.per_key.iter().collect();
    keys.sort_by(|a, b| {
        b.2.cmp(&a.2)
            .then(b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal))
    });
    let worst: Vec<_> = keys.into_iter().take(12).collect();
    if worst.is_empty() {
        ui.label(egui::RichText::new("Clean run: no problem keys.").color(p.ghost));
        return;
    }
    ui.horizontal_wrapped(|ui| {
        for (label, presses, errors, lat) in worst {
            let err_rate = if *presses > 0 {
                *errors as f32 / *presses as f32
            } else {
                0.0
            };
            let (fill, stroke) = if err_rate > 0.0 {
                (
                    p.ribbon.linear_multiply(0.10 + 0.25 * err_rate.min(1.0)),
                    p.ribbon,
                )
            } else if *lat > 250.0 {
                (p.brass.linear_multiply(0.15), p.brass)
            } else {
                (p.ink_850, p.edge)
            };
            egui::Frame::new()
                .fill(fill)
                .stroke(Stroke::new(1.0, stroke))
                .corner_radius(CornerRadius::same(7))
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(label.to_string())
                                .font(theme::mono_font(14.0))
                                .strong()
                                .color(p.paper),
                        );
                        ui.label(
                            egui::RichText::new(format!("{errors} err \u{00B7} {lat:.0}ms"))
                                .font(theme::mono_font(11.0))
                                .color(p.ghost),
                        );
                    });
                });
        }
    });
}
