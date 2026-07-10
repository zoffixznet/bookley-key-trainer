//! Settings view: persisted preferences across the two axes plus visual/behavior options,
//! laid out as titled cards on a centered column.

use crate::core::config::{CaretStyle, ContentMode, ErrorMode, KeyboardMode, Theme};
use crate::ui::app::App;
use crate::ui::theme;

/// A titled settings card at a consistent width.
fn section<R>(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    title: &str,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.label(
        egui::RichText::new(title.to_uppercase())
            .color(p.ghost)
            .size(10.5)
            .extra_letter_spacing(1.4),
    );
    ui.add_space(2.0);
    let r = theme::card(p)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner;
    ui.add_space(12.0);
    r
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let p = app.palette();
    let mut changed = false;
    let mut theme_changed = false;

    theme::page_scroll(ui, |ui| {
        theme::centered_column(ui, 680.0, |ui| {
            ui.add_space(22.0);
            ui.label(
                egui::RichText::new("Settings")
                    .font(theme::display_font(26.0))
                    .color(p.brass),
            );
            ui.add_space(14.0);

            section(ui, &p, "Drills", |ui| {
                theme::control_row(ui, "Default drill duration:", |ui| {
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
                        "Word and Random drills run for this long; results appear when \
time is up. The drill screen's start gate has the same picker.",
                    )
                    .color(p.ghost)
                    .size(12.0),
                );
            });

            section(ui, &p, "Keyboard display", |ui| {
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

            section(ui, &p, "Content mode", |ui| {
                ui.horizontal(|ui| {
                    for cm in ContentMode::ALL {
                        if ui
                            .selectable_label(app.config.content_mode == cm, cm.label())
                            .clicked()
                        {
                            // Full transition (same as the top-bar tabs): saves the
                            // config itself and never leaks the old mode's session.
                            app.set_content_mode(cm);
                        }
                    }
                });
            });

            section(ui, &p, "Error handling", |ui| {
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

            section(ui, &p, "Book generation", |ui| {
                theme::control_row(ui, "Default language:", |ui| {
                    if ui
                        .text_edit_singleline(&mut app.config.default_language)
                        .changed()
                    {
                        changed = true;
                    }
                });
                theme::control_row(ui, "Model:", |ui| {
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
                        "Book mode uses your Claude subscription via the claude CLI. It \
never uses an API key.",
                    )
                    .color(p.ghost)
                    .size(12.0),
                );
            });

            section(ui, &p, "Appearance", |ui| {
                theme::control_row(ui, "Theme:", |ui| {
                    for t in [Theme::Light, Theme::Dark] {
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
                theme::control_row(ui, "Caret:", |ui| {
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
                theme::control_row(ui, "Key sound on launch:", |ui| {
                    for (val, label) in [(true, "On"), (false, "Off")] {
                        if ui
                            .selectable_label(app.config.key_sound == val, label)
                            .clicked()
                        {
                            app.config.key_sound = val;
                            changed = true;
                        }
                    }
                });
                ui.label(
                    egui::RichText::new(
                        "The top-bar Sound switch controls the running session; this \
sets how it starts.",
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

            if ui.button("Back to typing").clicked() {
                app.screen = crate::ui::app::Screen::Typing;
            }
            ui.add_space(24.0);
        });
    });

    if theme_changed {
        theme::apply_style(ui.ctx(), app.config.theme);
    }
    if changed {
        app.save_config();
    }
}
