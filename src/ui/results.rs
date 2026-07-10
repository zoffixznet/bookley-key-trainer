//! Results: WPM-over-time line, per-key error/slowness feedback, accuracy, consistency,
//! and personal-best tracking, laid out on the shared centered column.
//!
//! Colorblind rule: nothing on this screen conveys state through hue alone. The two graph
//! lines differ in dash pattern and weight (plus a labeled legend), and the weak-key
//! pills are neutral chips whose information is carried by text.

use egui::{Align2, Color32, CornerRadius, FontId, Sense, Stroke, Vec2};

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

    theme::page_scroll(ui, |ui| {
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
                // The personal best sits in the header flow, not floating at the edge.
                if let Some(best) = app.stats.best_for(&result.mode) {
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new(format!("best {} wpm: {:.0}", result.mode, best))
                            .color(p.ghost)
                            .size(12.5),
                    );
                }
            });
            ui.add_space(14.0);

            // The big stats: one tidy block per stat (number over its label, both
            // center-aligned in the block's column), every block the same height, so
            // all numbers share a single baseline and all labels line up. Hovering a
            // block explains the stat in plain language.
            use theme::stat_tips as tips;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 30.0;
                big_stat(
                    ui,
                    &p,
                    "wpm",
                    &format!("{:.0}", result.wpm),
                    p.verdigris,
                    tips::WPM,
                );
                big_stat(
                    ui,
                    &p,
                    "raw",
                    &format!("{:.0}", result.raw_wpm),
                    p.ghost,
                    tips::RAW,
                );
                big_stat(
                    ui,
                    &p,
                    "accuracy",
                    &format!("{:.0}%", result.accuracy * 100.0),
                    p.paper,
                    tips::ACCURACY,
                );
                big_stat(
                    ui,
                    &p,
                    "consistency",
                    &format!("{:.0}", result.consistency),
                    p.paper,
                    tips::CONSISTENCY,
                );
                big_stat(
                    ui,
                    &p,
                    "net wpm",
                    &format!("{:.0}", result.net_wpm),
                    p.ghost,
                    tips::NET_WPM,
                );
                big_stat(
                    ui,
                    &p,
                    "time",
                    &mmss(result.elapsed_secs),
                    p.brass,
                    tips::TIME,
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
            ui.label(
                egui::RichText::new(
                    "errors made while the key was expected \u{00B7} average time to reach it",
                )
                .color(p.ghost)
                .size(11.0),
            );
            ui.add_space(6.0);
            weak_key_pills(ui, &p, &result);

            ui.add_space(22.0);
            // Two equal-size primary actions; "Type again" keeps the accent. Book
            // chapters do not get a "type again"; there the next step is continuing.
            let is_book = app.config.content_mode == ContentMode::Book;
            let label = if is_book {
                "Continue the book (Enter)"
            } else {
                "Type again (Enter)"
            };
            let mut go = false;
            ui.horizontal(|ui| {
                let size = egui::vec2(270.0, 46.0);
                let primary = egui::Button::new(
                    egui::RichText::new(label)
                        .size(17.0)
                        .strong()
                        .color(p.ink_850),
                )
                .fill(p.verdigris)
                .min_size(size)
                .corner_radius(CornerRadius::same(9));
                if ui.add(primary).clicked() {
                    go = true;
                }
                if !is_book {
                    ui.add_space(4.0);
                    let secondary = egui::Button::new(
                        egui::RichText::new("Drill weak keys (Random)")
                            .size(17.0)
                            .strong()
                            .color(p.paper),
                    )
                    .fill(p.ink_850)
                    .stroke(Stroke::new(1.0, p.edge))
                    .min_size(size)
                    .corner_radius(CornerRadius::same(9));
                    if ui.add(secondary).clicked() {
                        app.config.content_mode = ContentMode::Random;
                        app.save_config();
                        app.start_session();
                    }
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

/// One stat block: the number and its label center-aligned in one column, painted onto
/// an allocated rect. Every block uses the same fonts and the same vertical geometry, so
/// a horizontal row of them puts all numbers on one shared baseline with the labels
/// evenly aligned beneath. The whole block answers hover with a plain-language tip.
fn big_stat(ui: &mut egui::Ui, p: &Palette, label: &str, value: &str, color: Color32, tip: &str) {
    let value_font = theme::mono_font(38.0);
    let label_font = theme::ui_font(11.0);
    let gap = 4.0;
    let (v_size, l_size) = ui.fonts_mut(|f| {
        (
            f.layout_no_wrap(value.to_string(), value_font.clone(), color)
                .size(),
            f.layout_no_wrap(label.to_string(), label_font.clone(), p.ghost)
                .size(),
        )
    });
    let w = v_size.x.max(l_size.x);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(w, v_size.y + gap + l_size.y), Sense::hover());
    let painter = ui.painter();
    let cx = rect.center().x;
    // The value fonts are identical across blocks, so anchoring every number's bottom to
    // the same offset from the block top yields one shared baseline.
    painter.text(
        egui::pos2(cx, rect.top() + v_size.y),
        Align2::CENTER_BOTTOM,
        value,
        value_font,
        color,
    );
    painter.text(
        egui::pos2(cx, rect.top() + v_size.y + gap),
        Align2::CENTER_TOP,
        label,
        label_font,
        p.ghost,
    );
    response.on_hover_text(tip);
}

fn mmss(secs: f64) -> String {
    let s = secs.max(0.0).round() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// A tick spacing (seconds) that yields readable mm:ss labels for the drill length.
fn time_tick_step(max_t: f64) -> f64 {
    if max_t <= 45.0 {
        10.0
    } else if max_t <= 100.0 {
        15.0
    } else if max_t <= 200.0 {
        30.0
    } else if max_t <= 420.0 {
        60.0
    } else {
        120.0
    }
}

fn wpm_graph(ui: &mut egui::Ui, p: &Palette, r: &crate::core::metrics::SessionResult) {
    let w = ui.available_width();
    let h = 170.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
    let painter = ui.painter_at(rect);
    if r.samples.len() < 2 {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "Not enough data",
            FontId::proportional(13.0),
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
    let max_t = r.samples.last().unwrap().t.max(1.0);
    // Leave room for y figures on the left and mm:ss tick labels below.
    let plot = egui::Rect::from_min_max(
        egui::pos2(rect.left() + pad + 30.0, rect.top() + pad),
        egui::pos2(rect.right() - pad, rect.bottom() - pad - 18.0),
    );
    // Light horizontal gridlines + y figures at 0 / mid / max.
    for frac in [0.0f32, 0.5, 1.0] {
        let y = plot.bottom() - frac * plot.height();
        painter.line_segment(
            [egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)],
            Stroke::new(1.0, p.edge),
        );
        painter.text(
            egui::pos2(plot.left() - 6.0, y),
            Align2::RIGHT_CENTER,
            format!("{:.0}", max_wpm * frac as f64),
            FontId::monospace(10.0),
            p.ghost,
        );
    }
    // X axis: time, with mm:ss ticks scaled to the drill length.
    let step = time_tick_step(max_t);
    let mut t = 0.0;
    while t <= max_t + 0.5 {
        let x = plot.left() + (t / max_t).min(1.0) as f32 * plot.width();
        painter.line_segment(
            [
                egui::pos2(x, plot.bottom()),
                egui::pos2(x, plot.bottom() + 4.0),
            ],
            Stroke::new(1.0, p.ghost),
        );
        painter.text(
            egui::pos2(x, plot.bottom() + 6.0),
            Align2::CENTER_TOP,
            mmss(t),
            FontId::monospace(10.0),
            p.ghost,
        );
        t += step;
    }
    painter.text(
        egui::pos2(plot.right(), plot.bottom() + 6.0),
        Align2::RIGHT_TOP,
        "time",
        FontId::proportional(10.0),
        p.ghost,
    );

    let map = |t: f64, v: f64| {
        egui::pos2(
            plot.left() + (t / max_t) as f32 * plot.width(),
            plot.bottom() - (v / max_wpm) as f32 * plot.height(),
        )
    };
    // Raw: thin dashed ghost line. Dash pattern + weight distinguish it from the wpm
    // line even in grayscale.
    let raw_pts: Vec<egui::Pos2> = r.samples.iter().map(|s| map(s.t, s.raw)).collect();
    painter.add(egui::Shape::dashed_line(
        &raw_pts,
        Stroke::new(1.4, p.ghost),
        6.0,
        4.0,
    ));
    // WPM: heavier solid accent line.
    let wpm_pts: Vec<egui::Pos2> = r.samples.iter().map(|s| map(s.t, s.wpm)).collect();
    for w2 in wpm_pts.windows(2) {
        painter.line_segment([w2[0], w2[1]], Stroke::new(2.5, p.verdigris));
    }

    // Legend, top-right inside the plot: swatch lines + labels, on a small backdrop so
    // it stays readable when the data runs underneath it.
    let lx = plot.right() - 96.0;
    let mut ly = plot.top() + 10.0;
    let legend_bg = egui::Rect::from_min_max(
        egui::pos2(lx - 10.0, plot.top() + 1.0),
        egui::pos2(plot.right() - 2.0, plot.top() + 35.0),
    );
    painter.rect_filled(legend_bg, CornerRadius::same(5), p.ink_850);
    painter.rect_stroke(
        legend_bg,
        CornerRadius::same(5),
        Stroke::new(1.0, p.edge),
        egui::StrokeKind::Inside,
    );
    painter.line_segment(
        [egui::pos2(lx, ly), egui::pos2(lx + 22.0, ly)],
        Stroke::new(2.5, p.verdigris),
    );
    painter.text(
        egui::pos2(lx + 28.0, ly),
        Align2::LEFT_CENTER,
        "wpm",
        FontId::proportional(11.5),
        p.paper,
    );
    ly += 16.0;
    painter.add(egui::Shape::dashed_line(
        &[egui::pos2(lx, ly), egui::pos2(lx + 22.0, ly)],
        Stroke::new(1.4, p.ghost),
        5.0,
        3.0,
    ));
    painter.text(
        egui::pos2(lx + 28.0, ly),
        Align2::LEFT_CENTER,
        "raw",
        FontId::proportional(11.5),
        p.paper,
    );
}

/// The weak-key pills: neutral chips (no red-on-red), a bold key glyph, plain-ink counts,
/// self-explanatory copy, wrapped onto as many rows as the column needs.
fn weak_key_pills(ui: &mut egui::Ui, p: &Palette, r: &crate::core::metrics::SessionResult) {
    // Worst keys first: by errors, then by latency; keep keys that actually have a
    // problem (an error, or a notably slow average).
    let mut keys: Vec<&(String, u32, u32, Option<f64>)> = r
        .per_key
        .iter()
        .filter(|(_, _, errors, lat)| *errors > 0 || lat.map(|l| l > 400.0).unwrap_or(false))
        .collect();
    keys.sort_by(|a, b| {
        b.2.cmp(&a.2)
            .then(b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal))
    });
    let worst: Vec<_> = keys.into_iter().take(12).collect();
    if worst.is_empty() {
        ui.label(egui::RichText::new("Clean run: no problem keys.").color(p.ghost));
        return;
    }

    let key_font = theme::mono_font(14.5);
    let detail_font = theme::mono_font(11.5);
    let pills: Vec<(String, String)> = worst
        .iter()
        .map(|(label, _presses, errors, lat)| {
            let plural = if *errors == 1 { "error" } else { "errors" };
            let lat_str = match lat {
                Some(l) => format!("{l:.0} ms avg"),
                None => "\u{2013}".to_string(),
            };
            (
                label.clone(),
                format!("{errors} {plural} \u{00B7} {lat_str}"),
            )
        })
        .collect();

    // Measure each pill, then flow them into rows ourselves: dynamically-sized frames do
    // not wrap in egui's horizontal_wrapped, which is how the row used to run offscreen.
    let pad_x = 12.0;
    let inner_gap = 8.0;
    let widths: Vec<f32> = ui.fonts_mut(|f| {
        pills
            .iter()
            .map(|(k, d)| {
                let wk = f
                    .layout_no_wrap(k.clone(), key_font.clone(), Color32::WHITE)
                    .size()
                    .x;
                let wd = f
                    .layout_no_wrap(d.clone(), detail_font.clone(), Color32::WHITE)
                    .size()
                    .x;
                wk + inner_gap + wd + pad_x * 2.0
            })
            .collect()
    });
    let spacing = ui.spacing().item_spacing.x;
    let avail = ui.available_width();
    let mut rows: Vec<Vec<usize>> = vec![Vec::new()];
    let mut x = 0.0;
    for (i, w) in widths.iter().enumerate() {
        if x + w > avail && !rows.last().unwrap().is_empty() {
            rows.push(Vec::new());
            x = 0.0;
        }
        rows.last_mut().unwrap().push(i);
        x += w + spacing;
    }
    for row in rows {
        ui.horizontal(|ui| {
            for i in row {
                let (key, detail) = &pills[i];
                egui::Frame::new()
                    .fill(p.ink_850)
                    .stroke(Stroke::new(1.0, p.edge))
                    .corner_radius(CornerRadius::same(7))
                    .inner_margin(egui::Margin::symmetric(pad_x as i8, 6))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = inner_gap;
                        ui.label(
                            egui::RichText::new(key)
                                .font(key_font.clone())
                                .strong()
                                .color(p.paper),
                        );
                        ui.label(
                            egui::RichText::new(detail)
                                .font(detail_font.clone())
                                .color(p.ghost),
                        );
                    });
            }
        });
    }
}
