//! Settings view: persisted preferences across the two axes plus visual/behavior options.

use crate::core::config::{CaretStyle, ContentMode, ErrorMode, KeyboardMode, Theme};
use crate::ui::app::App;
use crate::ui::theme;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let p = app.palette();
    let mut changed = false;
    let mut theme_changed = false;

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new("Settings")
                .color(p.brass)
                .size(24.0)
                .strong(),
        );
        ui.add_space(12.0);

        ui.group(|ui| {
            ui.label(egui::RichText::new("Keyboard display").color(p.ghost));
            ui.horizontal(|ui| {
                for km in KeyboardMode::ALL {
                    if ui
                        .selectable_label(app.config.keyboard_mode == km, km.label())
                        .clicked()
                    {
                        app.config.keyboard_mode = km;
                        changed = true;
                    }
                }
            });
        });

        ui.group(|ui| {
            ui.label(egui::RichText::new("Content mode").color(p.ghost));
            ui.horizontal(|ui| {
                for cm in ContentMode::ALL {
                    if ui
                        .selectable_label(app.config.content_mode == cm, cm.label())
                        .clicked()
                    {
                        app.config.content_mode = cm;
                        changed = true;
                    }
                }
            });
        });

        ui.group(|ui| {
            ui.label(egui::RichText::new("Error handling").color(p.ghost));
            ui.horizontal(|ui| {
                for em in ErrorMode::ALL {
                    if ui
                        .selectable_label(app.config.error_mode == em, em.label())
                        .clicked()
                    {
                        app.config.error_mode = em;
                        changed = true;
                    }
                }
            });
        });

        ui.group(|ui| {
            ui.label(egui::RichText::new("Appearance").color(p.ghost));
            ui.horizontal(|ui| {
                ui.label("Theme:");
                for t in [Theme::Dark, Theme::Light] {
                    if ui
                        .selectable_label(app.config.theme == t, t.label())
                        .clicked()
                    {
                        app.config.theme = t;
                        changed = true;
                        theme_changed = true;
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("Caret:");
                for c in [CaretStyle::Block, CaretStyle::Bar, CaretStyle::Underline] {
                    if ui
                        .selectable_label(app.config.caret == c, c.label())
                        .clicked()
                    {
                        app.config.caret = c;
                        changed = true;
                    }
                }
            });
            if ui
                .checkbox(
                    &mut app.config.reduced_motion,
                    "Reduced motion (disable flashes/animations)",
                )
                .changed()
            {
                changed = true;
            }
        });

        ui.group(|ui| {
            ui.label(egui::RichText::new("Drills").color(p.ghost));
            ui.horizontal(|ui| {
                ui.label("Drill duration:");
                for (secs, label) in crate::core::config::DRILL_PRESETS {
                    if ui
                        .selectable_label(app.config.drill_secs == secs, label)
                        .clicked()
                    {
                        app.config.drill_secs = secs;
                        changed = true;
                    }
                }
            });
            ui.label(
                egui::RichText::new(
                    "Word and Random drills run for this long; results appear when time \
is up.",
                )
                .color(p.ghost)
                .size(12.0),
            );
            if ui
                .checkbox(
                    &mut app.config.show_numpad,
                    "Show the numpad on the keyboard",
                )
                .changed()
            {
                changed = true;
            }
        });

        ui.group(|ui| {
            ui.label(egui::RichText::new("Book generation").color(p.ghost));
            ui.horizontal(|ui| {
                ui.label("Default language:");
                if ui
                    .text_edit_singleline(&mut app.config.default_language)
                    .changed()
                {
                    changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Model:");
                for m in ["opus", "sonnet", "haiku", "fable"] {
                    let label = if m == "opus" { "opus (default)" } else { m };
                    if ui
                        .selectable_label(app.config.book_model == m, label)
                        .clicked()
                    {
                        app.config.book_model = m.to_string();
                        changed = true;
                    }
                }
            });
            ui.label(
                egui::RichText::new(
                    "Book mode uses your Claude subscription via the claude CLI. It never \
uses an API key.",
                )
                .color(p.ghost)
                .size(12.0),
            );
        });

        ui.add_space(8.0);
        if ui.button("Back to typing").clicked() {
            app.screen = crate::ui::app::Screen::Typing;
        }
        ui.add_space(20.0);
    });

    if theme_changed {
        theme::apply_style(ui.ctx(), app.config.theme);
    }
    if changed {
        app.save_config();
    }
}
