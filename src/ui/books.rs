//! The Books manager: a shelf of books with create / continue / generate / rewrite /
//! export actions, the one-clarifying-turn flow, the blank-input confirmation, and the
//! live "writing..." view during generation.

use crate::core::book::prompt;
use crate::core::config::ContentMode;
use crate::ui::app::{App, Screen};

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let p = app.palette();

    // If a generation is in flight, show the live writing view.
    if app.gen.is_some() {
        writing_view(app, ui);
        return;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Your books").color(p.brass).size(24.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("New book").clicked() {
                    app.book_ui.show_create = true;
                    app.book_ui.new_language = app.config.default_language.clone();
                }
            });
        });

        if let Some(status) = &app.book_ui.status {
            ui.add_space(6.0);
            ui.label(egui::RichText::new(status).color(p.verdigris));
        }

        ui.add_space(8.0);

        if app.book_ui.show_create {
            create_dialog(app, ui);
            ui.separator();
        }

        // If a book is open, show its detail (chapters, continue, rewrite, export).
        if let Some(slug) = app.book_ui.open_slug.clone() {
            book_detail(app, ui, &slug);
            ui.separator();
        }

        // The shelf.
        let books = app.store.list();
        if books.is_empty() && !app.book_ui.show_create {
            ui.add_space(20.0);
            ui.label(
                egui::RichText::new(
                    "No books yet. Create one, then type its chapters to write it.",
                )
                .color(p.ghost),
            );
        }
        ui.horizontal_wrapped(|ui| {
            for book in &books {
                let title = crate::core::book::store::display_title(&book.meta);
                let done = book.meta.chapters.iter().filter(|c| c.done).count();
                let total = book.meta.chapters.len();
                let group = ui.group(|ui| {
                    ui.set_width(220.0);
                    ui.label(egui::RichText::new(&title).color(p.brass).size(16.0).strong());
                    ui.label(
                        egui::RichText::new(if book.meta.language.is_empty() {
                            "".to_string()
                        } else {
                            book.meta.language.clone()
                        })
                        .color(p.ghost)
                        .size(12.0),
                    );
                    ui.label(
                        egui::RichText::new(format!("{done}/{total} chapters typed"))
                            .color(p.ghost)
                            .size(12.0),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Open").clicked() {
                            app.book_ui.open_slug = Some(book.meta.slug.clone());
                            app.book_ui.status = None;
                        }
                    });
                });
                let _ = group;
            }
        });
        ui.add_space(20.0);
    });
}

fn create_dialog(app: &mut App, ui: &mut egui::Ui) {
    let p = app.palette();
    ui.group(|ui| {
        ui.label(egui::RichText::new("New book").color(p.brass).size(18.0));
        ui.label(
            egui::RichText::new("All fields may be left blank; the author will invent them.")
                .color(p.ghost)
                .size(12.0),
        );
        ui.horizontal(|ui| {
            ui.label("Title:");
            ui.text_edit_singleline(&mut app.book_ui.new_title);
        });
        ui.horizontal(|ui| {
            ui.label("Language:");
            ui.text_edit_singleline(&mut app.book_ui.new_language);
        });
        ui.label("What should the story involve?");
        ui.add(
            egui::TextEdit::multiline(&mut app.book_ui.new_premise)
                .desired_rows(4)
                .desired_width(f32::INFINITY)
                .hint_text("Genre, characters, premise, tone... or leave blank."),
        );
        ui.horizontal(|ui| {
            if ui.button("Create").clicked() {
                let title = app.book_ui.new_title.clone();
                let lang = if app.book_ui.new_language.trim().is_empty() {
                    app.config.default_language.clone()
                } else {
                    app.book_ui.new_language.clone()
                };
                let premise = app.book_ui.new_premise.clone();
                match app.store.create(&title, &lang, &premise, false) {
                    Ok(book) => {
                        app.book_ui.open_slug = Some(book.meta.slug.clone());
                        app.book_ui.show_create = false;
                        app.book_ui.new_title.clear();
                        app.book_ui.new_premise.clear();
                        app.book_ui.status =
                            Some("Book created. Generate the first chapter.".into());
                    }
                    Err(e) => app.book_ui.status = Some(format!("Could not create book: {e}")),
                }
            }
            if ui.button("Cancel").clicked() {
                app.book_ui.show_create = false;
            }
        });
    });
}

fn book_detail(app: &mut App, ui: &mut egui::Ui, slug: &str) {
    let p = app.palette();
    let Ok(book) = app.store.load(slug) else {
        ui.label("Could not load this book.");
        return;
    };
    let title = crate::core::book::store::display_title(&book.meta);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(&title).color(p.brass).size(20.0).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Close").clicked() {
                app.book_ui.open_slug = None;
            }
            if ui.button("Export PDF").clicked() {
                export_pdf(app, &book);
            }
            if ui.button("Export Markdown").clicked() {
                export_markdown(app, &book);
            }
        });
    });

    let all_typed = book.all_chapters_typed();
    let n_chapters = book.chapter_count();

    // Chapter list with per-chapter type/rewrite.
    for c in &book.meta.chapters {
        ui.horizontal(|ui| {
            let heading = if c.title.trim().is_empty() {
                format!("Chapter {}", c.n)
            } else {
                format!("Chapter {}: {}", c.n, c.title)
            };
            let color = if c.done { p.paper } else { p.brass };
            ui.label(egui::RichText::new(&heading).color(color));
            ui.label(
                egui::RichText::new(format!("({} words)", c.words))
                    .color(p.ghost)
                    .size(11.0),
            );
            if c.done {
                ui.label(egui::RichText::new("typed").color(p.verdigris).size(11.0));
            } else if ui.button("Type this chapter").clicked() {
                app.config.content_mode = ContentMode::Book;
                app.book_ui.open_slug = Some(slug.to_string());
                app.save_config();
                app.start_session();
            }
            if ui.button("Rewrite").clicked() {
                app.book_ui.rewrite_n = Some(c.n);
                app.book_ui.rewrite_instruction.clear();
            }
        });
    }

    // Rewrite dialog.
    if let Some(rn) = app.book_ui.rewrite_n {
        ui.group(|ui| {
            ui.label(
                egui::RichText::new(format!("Rewrite chapter {rn}"))
                    .color(p.brass),
            );
            ui.label(
                egui::RichText::new(
                    "Later chapters stay as they are; this rewrite must still fit the book. \
Rewriting resets this chapter's typing progress.",
                )
                .color(p.ghost)
                .size(12.0),
            );
            ui.text_edit_singleline(&mut app.book_ui.rewrite_instruction);
            ui.horizontal(|ui| {
                if ui.button("Rewrite now").clicked() {
                    let prompt = prompt::rewrite_prompt(&book, rn, &app.book_ui.rewrite_instruction);
                    app.book_ui.rewrite_n = None;
                    app.start_generation(rn, true, prompt);
                }
                if ui.button("Cancel").clicked() {
                    app.book_ui.rewrite_n = None;
                }
            });
        });
    }

    ui.add_space(8.0);

    // The clarifying-turn flow.
    if let Some(questions) = app.book_ui.pending_questions.clone() {
        ui.group(|ui| {
            ui.label(egui::RichText::new("The author asks:").color(p.brass));
            ui.label(egui::RichText::new(&questions).color(p.paper));
            ui.label("Your answer:");
            ui.text_edit_singleline(&mut app.book_ui.clarify_answer);
            if ui.button("Send answer and write").clicked() {
                let n = n_chapters + 1;
                let answer = app.book_ui.clarify_answer.clone();
                // Re-issue with the answer as continuation, clarifying disabled now.
                let combined = format!(
                    "{}\n\nYour earlier questions and my answers:\n{}",
                    app.book_ui.continuation, answer
                );
                let prompt = prompt::chapter_prompt(&book, n, &combined, false, None);
                app.book_ui.pending_questions = None;
                app.book_ui.clarify_answer.clear();
                app.start_generation(n, false, prompt);
            }
        });
        return;
    }

    // The blank-input confirmation.
    if app.book_ui.confirm_blank {
        ui.group(|ui| {
            ui.label(
                egui::RichText::new(
                    "You left the direction blank. Let the author invent everything for \
this chapter?",
                )
                .color(p.brass),
            );
            ui.horizontal(|ui| {
                if ui.button("Yes, invent it").clicked() {
                    let n = n_chapters + 1;
                    // Blank confirmed: clarifying disabled.
                    let prompt = prompt::chapter_prompt(&book, n, "", false, None);
                    app.book_ui.confirm_blank = false;
                    app.start_generation(n, false, prompt);
                }
                if ui.button("No, let me add direction").clicked() {
                    app.book_ui.confirm_blank = false;
                }
            });
        });
        return;
    }

    // Generate-next-chapter block (gated on all chapters being typed).
    if n_chapters == 0 || all_typed {
        ui.group(|ui| {
            let next_n = n_chapters + 1;
            ui.label(
                egui::RichText::new(if next_n == 1 {
                    "Generate the first chapter".to_string()
                } else {
                    format!("Generate chapter {next_n}")
                })
                .color(p.brass),
            );
            ui.horizontal(|ui| {
                ui.label("How should the story continue?");
                ui.add(
                    egui::TextEdit::singleline(&mut app.book_ui.continuation)
                        .desired_width(360.0)
                        .hint_text("One line, or leave blank for the author to decide."),
                );
            });
            if ui.button("Generate next chapter").clicked() {
                let cont = app.book_ui.continuation.trim().to_string();
                if cont.is_empty() {
                    // Blank input: confirm they want fully AI-invented content.
                    app.book_ui.confirm_blank = true;
                } else {
                    // Allow one clarifying turn on the first chapter with real input.
                    let allow_clarify = next_n == 1;
                    let prompt = prompt::chapter_prompt(&book, next_n, &cont, allow_clarify, None);
                    app.start_generation(next_n, false, prompt);
                }
            }
        });
    } else {
        ui.label(
            egui::RichText::new(
                "Finish typing every generated chapter to unlock the next one.",
            )
            .color(p.ghost),
        );
    }
}

fn writing_view(app: &mut App, ui: &mut egui::Ui) {
    let p = app.palette();
    ui.add_space(30.0);
    ui.vertical_centered(|ui| {
        ui.label(egui::RichText::new("Writing...").color(p.brass).size(22.0));
        ui.add_space(4.0);
        if let Some(gen) = &app.gen {
            let secs = gen.started.elapsed().as_secs();
            ui.label(
                egui::RichText::new(format!("{}s elapsed", secs))
                    .color(p.ghost)
                    .size(12.0),
            );
        }
    });
    ui.add_space(12.0);
    egui::ScrollArea::vertical()
        .max_height(360.0)
        .stick_to_bottom(true)
        .show(ui, |ui| {
            if let Some(gen) = &app.gen {
                // Strip the markers from the live view for readability.
                let shown = gen
                    .live_text
                    .replace("===TITLE===", "")
                    .replace("===CHAPTER===", "")
                    .replace("===BIBLE===", "\n\n[continuity notes...]")
                    .replace("===END===", "");
                ui.label(egui::RichText::new(shown).color(p.paper).monospace());
            }
        });
    ui.ctx().request_repaint();
}

fn export_markdown(app: &mut App, book: &crate::core::book::store::Book) {
    let md = crate::core::book::export::export_markdown(book);
    let name = format!("{}.md", book.meta.slug);
    match write_download(&name, md.as_bytes()) {
        Ok(path) => app.book_ui.status = Some(format!("Saved Markdown to {}", path.display())),
        Err(e) => app.book_ui.status = Some(format!("Markdown export failed: {e}")),
    }
}

fn export_pdf(app: &mut App, book: &crate::core::book::store::Book) {
    match crate::core::book::export::export_pdf(book) {
        Ok(bytes) => {
            let name = format!("{}.pdf", book.meta.slug);
            match write_download(&name, &bytes) {
                Ok(path) => app.book_ui.status = Some(format!("Saved PDF to {}", path.display())),
                Err(e) => app.book_ui.status = Some(format!("PDF export failed: {e}")),
            }
        }
        Err(e) => app.book_ui.status = Some(format!("PDF export failed: {e}")),
    }
}

/// Write an export to the user's downloads-ish location (data_dir/exports), returning path.
fn write_download(name: &str, bytes: &[u8]) -> std::io::Result<std::path::PathBuf> {
    let dir = crate::core::paths::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("exports");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(name);
    std::fs::write(&path, bytes)?;
    Ok(path)
}

/// Ensure the Books tab keeps content mode aligned when opened directly.
pub fn ensure_screen(app: &mut App) {
    app.screen = Screen::Books;
}
