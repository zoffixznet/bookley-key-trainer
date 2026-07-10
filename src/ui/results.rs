//! Results overlay: WPM-over-time line, per-key error/slowness feedback, accuracy,
//! consistency, and personal-best tracking with a "drill your weak keys" affordance.

use egui::{Color32, Sense, Stroke, Vec2};

use crate::core::config::ContentMode;
use crate::ui::app::{App, Screen};

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let p = app.palette();
    let Some(result) = app.last_result.clone() else {
        ui.label("No results yet.");
        return;
    };

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Results")
                    .color(p.brass)
                    .size(24.0)
                    .strong(),
            );
            if app.last_was_pb {
                ui.label(
                    egui::RichText::new("  Personal best  ")
                        .color(p.ink_950)
                        .background_color(p.brass)
                        .strong(),
                );
            }
        });
        ui.add_space(12.0);

        // Big numbers.
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
        });
        if let Some(best) = app.stats.best_for(&result.mode) {
            ui.label(
                egui::RichText::new(format!("Best {} WPM: {:.0}", result.mode, best))
                    .color(p.brass),
            );
        }

        ui.add_space(16.0);
        ui.label(
            egui::RichText::new("WPM over time")
                .color(p.ghost)
                .size(14.0),
        );
        wpm_graph(ui, &p, &result);

        ui.add_space(16.0);
        ui.label(
            egui::RichText::new("Slow and error-prone keys")
                .color(p.ghost)
                .size(14.0),
        );
        heatmap(ui, &p, &result);

        ui.add_space(20.0);
        // Primary action: a proper big button, also triggered by Enter. Book chapters do
        // not get a "type again"; the next step there is the continue-the-book flow.
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
                    .size(18.0)
                    .strong()
                    .color(p.ink_950),
            )
            .fill(p.verdigris)
            .min_size(egui::vec2(260.0, 44.0))
            .corner_radius(egui::CornerRadius::same(8));
            if ui.add(btn).clicked() {
                go = true;
            }
            if !is_book && ui.button("Drill weak keys (Random)").clicked() {
                app.config.content_mode = ContentMode::Random;
                app.save_config();
                app.start_session();
            }
        });
        // Enter shortcut, guarded so the keystroke that finished the session (or held
        // Enter auto-repeat) cannot immediately restart it.
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
        ui.add_space(20.0);
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

fn big_stat(
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
                .size(40.0)
                .monospace()
                .strong(),
        );
        ui.label(egui::RichText::new(label).color(p.ghost).size(12.0));
    });
    ui.add_space(24.0);
}

fn wpm_graph(
    ui: &mut egui::Ui,
    p: &crate::ui::theme::Palette,
    r: &crate::core::metrics::SessionResult,
) {
    let w = ui.available_width().min(860.0);
    let h = 160.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, egui::CornerRadius::same(8), p.ink_850);
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
    let pad = 12.0;
    let max_wpm = r
        .samples
        .iter()
        .map(|s| s.raw.max(s.wpm))
        .fold(1.0, f64::max)
        .max(10.0);
    let max_t = r.samples.last().unwrap().t.max(0.001);
    let plot = egui::Rect::from_min_max(
        egui::pos2(rect.left() + pad, rect.top() + pad),
        egui::pos2(rect.right() - pad, rect.bottom() - pad),
    );
    let map = |t: f64, v: f64| {
        egui::pos2(
            plot.left() + (t / max_t) as f32 * plot.width(),
            plot.bottom() - (v / max_wpm) as f32 * plot.height(),
        )
    };
    // Raw line (ghost) then wpm line (verdigris).
    for (key, color) in [("raw", p.ghost), ("wpm", p.verdigris)] {
        let pts: Vec<egui::Pos2> = r
            .samples
            .iter()
            .map(|s| map(s.t, if key == "raw" { s.raw } else { s.wpm }))
            .collect();
        for w2 in pts.windows(2) {
            painter.line_segment([w2[0], w2[1]], Stroke::new(2.0, color));
        }
    }
}

fn heatmap(
    ui: &mut egui::Ui,
    p: &crate::ui::theme::Palette,
    r: &crate::core::metrics::SessionResult,
) {
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
            let color = if err_rate > 0.0 {
                lerp(p.ink_850, p.ribbon, err_rate.min(1.0))
            } else if *lat > 250.0 {
                lerp(p.ink_850, p.brass, ((*lat as f32 - 250.0) / 500.0).min(1.0))
            } else {
                p.ink_850
            };
            let text = format!("{label}  {errors}e {lat:.0}ms");
            ui.label(
                egui::RichText::new(text)
                    .color(p.paper)
                    .background_color(color)
                    .monospace(),
            );
        }
    });
}

fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()))
}
