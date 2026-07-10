//! The Books manager: a shelf of books with create / continue / generate / rewrite /
//! export actions, the one-clarifying-turn flow, the blank-input confirmation, and the
//! live "writing..." view during generation.
//!
//! The open book renders as a "book jacket": cover (or a typographic placeholder) beside
//! the title, status, and progress, with a quiet grouped toolbar under them, a chapters
//! table, and exactly one prominent next-step block (type the next chapter, generate the
//! next one, or the finished state), so actions never float around the page.

use crate::core::book::prompt;
use crate::core::config::ContentMode;
use crate::ui::app::{App, Screen};
use crate::ui::stage::COLUMN_W;
use crate::ui::theme;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let p = app.palette();

    // If a generation is in flight, show the live writing view.
    if app.gen.is_some() {
        writing_view(app, ui);
        return;
    }

    theme::page_scroll(ui, |ui| {
        theme::centered_column(ui, COLUMN_W, |ui| {
            ui.add_space(22.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Your books")
                        .font(theme::display_font(26.0))
                        .color(p.brass),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("New book").clicked() {
                        app.book_ui.show_create = true;
                        app.book_ui.new_language = app.config.default_language.clone();
                    }
                });
            });

            // Claude banner: books and typing always work; generation needs Claude.
            match &app.auth.check {
                Some(check) if !check.is_connected() => {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(
                                "Claude isn't connected yet; generating chapters needs it.",
                            )
                            .color(p.ghost),
                        );
                        if ui.button("Connect Claude").clicked() {
                            app.screen = Screen::Connect;
                        }
                    });
                }
                _ => {}
            }

            if let Some(status) = &app.book_ui.status {
                ui.add_space(6.0);
                ui.label(egui::RichText::new(status).color(p.verdigris));
            }

            ui.add_space(10.0);

            if app.book_ui.show_create {
                create_dialog(app, ui);
                ui.add_space(12.0);
            }

            // If a book is open, show its detail (chapters, continue, rewrite, export),
            // then a clear divider and heading so the tile grid below reads as a
            // separate "all books" list rather than part of the open book.
            if let Some(slug) = app.book_ui.open_slug.clone() {
                book_detail(app, ui, &slug);
                ui.add_space(20.0);
                ui.separator();
                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new("ALL BOOKS")
                        .color(p.ghost)
                        .size(10.5)
                        .extra_letter_spacing(1.4),
                );
                ui.add_space(2.0);
            }

            // The shelf.
            let books = app.store.list();
            if books.is_empty() && !app.book_ui.show_create {
                ui.add_space(24.0);
                ui.label(
                    egui::RichText::new(
                        "No books yet. Create one, then type its chapters to write it.",
                    )
                    .color(p.ghost),
                );
            }
            ui.horizontal_wrapped(|ui| {
                for book in &books {
                    shelf_card(app, ui, book);
                }
            });
            ui.add_space(24.0);
        });
    });
}

/// One book on the shelf: a spine-like card with title, language, progress, actions.
/// The currently open book's tile keeps identical geometry to every other tile; its
/// state is carried by a slightly darker background and the disabled "Opened" button
/// (a luminance + text cue, never hue alone and never a layout change).
fn shelf_card(app: &mut App, ui: &mut egui::Ui, book: &crate::core::book::store::Book) {
    let p = app.palette();
    let title = crate::core::book::store::display_title(&book.meta);
    let done = book.meta.chapters.iter().filter(|c| c.done).count();
    let total = book.meta.chapters.len();
    let is_open = app.book_ui.open_slug.as_deref() == Some(book.meta.slug.as_str());
    // The open book's tile: a slightly darker background plus the disabled "Opened"
    // button, and NOTHING that changes the tile's geometry. Earlier cues (a badge and a
    // thicker border/spine) altered the layout, re-wrapped titles, and misaligned the
    // whole grid.
    let frame = if is_open {
        theme::card(&p).fill(p.ink_950)
    } else {
        theme::card(&p)
    };
    frame.show(ui, |ui| {
        ui.vertical(|ui| {
            ui.set_width(268.0);
            ui.set_min_height(112.0);
            // Spine accent along the top of the card (identical on every tile).
            let (rule, _) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), 3.0), egui::Sense::hover());
            ui.painter()
                .rect_filled(rule, egui::CornerRadius::same(2), p.brass);
            ui.add_space(6.0);
            // The title must wrap inside the fixed card width. A Label in a horizontal
            // layout never wraps (it extends), so a long AI-generated title would blow
            // the tile out to the full row width; keep the title an explicitly wrapping
            // label.
            ui.add(
                egui::Label::new(
                    egui::RichText::new(&title)
                        .font(theme::display_font(17.0))
                        .color(p.paper),
                )
                .wrap(),
            );
            ui.add_space(2.0);
            let mut meta = format!("{done}/{total} chapters typed");
            if !book.meta.language.is_empty() {
                meta = format!("{}  \u{00B7}  {meta}", book.meta.language);
            }
            if book.meta.concluded {
                meta.push_str("  \u{00B7}  finished");
            }
            ui.label(egui::RichText::new(meta).color(p.ghost).size(12.0));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if is_open {
                    // Already open: a dead "Open" button confused users, so make the
                    // state explicit and unclickable.
                    ui.add_enabled(false, egui::Button::new("Opened"))
                        .on_disabled_hover_text("This book is already open above.");
                } else if ui.button("Open").clicked() {
                    app.book_ui.open_slug = Some(book.meta.slug.clone());
                    app.book_ui.status = None;
                    // Remember the book so the next launch reopens it directly.
                    app.config.last_book = Some(book.meta.slug.clone());
                    app.save_config();
                }
                if app.book_ui.confirm_delete.as_deref() == Some(&book.meta.slug) {
                    if ui
                        .button(egui::RichText::new("Really delete").color(p.ribbon))
                        .clicked()
                    {
                        if let Err(e) = app.store.delete(&book.meta.slug) {
                            app.book_ui.status = Some(format!("Could not delete the book: {e}"));
                        }
                        if app.book_ui.open_slug.as_deref() == Some(&book.meta.slug) {
                            app.book_ui.open_slug = None;
                        }
                        if app.config.last_book.as_deref() == Some(&book.meta.slug) {
                            app.config.last_book = None;
                            app.save_config();
                        }
                        // A live session typing this book would be a ghost: its saves
                        // and completion would fail silently. Drop it with the book.
                        app.invalidate_book_session(&book.meta.slug, None);
                        app.book_ui.confirm_delete = None;
                    }
                    if ui.button("Keep").clicked() {
                        app.book_ui.confirm_delete = None;
                    }
                } else if ui
                    .button(egui::RichText::new("Delete").color(p.ghost))
                    .clicked()
                {
                    app.book_ui.confirm_delete = Some(book.meta.slug.clone());
                }
            });
        });
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
        theme::control_row(ui, "Title:", |ui| {
            ui.text_edit_singleline(&mut app.book_ui.new_title);
        });
        theme::control_row(ui, "Language:", |ui| {
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
                        app.config.last_book = Some(book.meta.slug.clone());
                        app.save_config();
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

/// Start typing the book's next untyped chapter (the session always serves the first
/// one that is not done). Remembers the book for the next launch.
fn start_typing(app: &mut App, slug: &str) {
    app.config.content_mode = ContentMode::Book;
    app.book_ui.open_slug = Some(slug.to_string());
    app.config.last_book = Some(slug.to_string());
    app.save_config();
    app.start_session();
}

/// The open book as a "book jacket" card: cover | title/status/progress, a quiet
/// toolbar, the chapters table, and one prominent next-step block.
fn book_detail(app: &mut App, ui: &mut egui::Ui, slug: &str) {
    let p = app.palette();
    let Ok(book) = app.store.load(slug) else {
        ui.label("Could not load this book.");
        return;
    };
    let title = crate::core::book::store::display_title(&book.meta);
    let cover_tex = cover_texture(app, ui, &book);
    let cover_designing = app
        .cover_gen
        .as_ref()
        .map(|c| c.slug == *slug)
        .unwrap_or(false);
    let all_typed = book.all_chapters_typed();
    let n_chapters = book.chapter_count();
    let done = book.meta.chapters.iter().filter(|c| c.done).count();
    let first_untyped = book.meta.chapters.iter().find(|c| !c.done).map(|c| c.n);

    theme::card(&p).show(ui, |ui| {
        ui.set_width(ui.available_width());

        // Jacket header: cover on the left, identity column on the right.
        const COVER_SIZE: egui::Vec2 = egui::Vec2::new(120.0, 192.0);
        ui.horizontal_top(|ui| {
            if let Some(tex) = &cover_tex {
                ui.add(egui::Image::new(tex).fit_to_exact_size(COVER_SIZE));
            } else {
                cover_placeholder(ui, &p, &title, COVER_SIZE);
            }
            ui.add_space(14.0);
            ui.vertical(|ui| {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(&title)
                            .font(theme::display_font(24.0))
                            .color(p.brass),
                    )
                    .wrap(),
                );
                ui.add_space(6.0);
                // Status chip (the word itself is the cue, never hue alone) + meta.
                ui.horizontal(|ui| {
                    let (chip, chip_bg) = if book.meta.concluded {
                        (" FINISHED ", p.verdigris)
                    } else {
                        (" WRITING ", p.brass)
                    };
                    ui.label(
                        egui::RichText::new(chip)
                            .color(p.ink_850)
                            .background_color(chip_bg)
                            .size(10.5)
                            .strong(),
                    );
                    let mut meta = format!("{done}/{n_chapters} chapters typed");
                    if !book.meta.language.is_empty() {
                        meta = format!("{}  \u{00B7}  {meta}", book.meta.language);
                    }
                    ui.label(egui::RichText::new(meta).color(p.ghost).size(12.5));
                });
                ui.add_space(10.0);
                // Binding progress: chapters typed over chapters written.
                let frac = if n_chapters == 0 {
                    0.0
                } else {
                    done as f32 / n_chapters as f32
                };
                let (bar, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width().min(360.0), 6.0),
                    egui::Sense::hover(),
                );
                ui.painter()
                    .rect_filled(bar, egui::CornerRadius::same(3), p.edge);
                if frac > 0.0 {
                    let mut fill = bar;
                    fill.set_width(bar.width() * frac);
                    ui.painter()
                        .rect_filled(fill, egui::CornerRadius::same(3), p.verdigris);
                }
                ui.add_space(12.0);
                // Quiet grouped toolbar: management actions live here, nowhere else.
                ui.horizontal_wrapped(|ui| {
                    if cover_designing {
                        if ui.button("Cancel cover").clicked() {
                            app.cancel_cover();
                        }
                        ui.spinner();
                    } else {
                        let label = if book.has_cover() {
                            "Regenerate cover"
                        } else {
                            "Generate cover"
                        };
                        if ui.button(label).clicked() {
                            app.start_cover_generation();
                        }
                    }
                    if ui.button("Export Markdown").clicked() {
                        export_markdown(app, &book);
                    }
                    if ui.button("Export PDF").clicked() {
                        export_pdf(app, &book);
                    }
                    if ui
                        .button(egui::RichText::new("Close").color(p.ghost))
                        .clicked()
                    {
                        app.book_ui.open_slug = None;
                    }
                });
            });
        });

        // Chapters table.
        if !book.meta.chapters.is_empty() {
            ui.add_space(14.0);
            ui.separator();
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("CHAPTERS")
                    .color(p.ghost)
                    .size(10.5)
                    .extra_letter_spacing(1.4),
            );
            ui.add_space(4.0);
            egui::Grid::new("chapters")
                .num_columns(4)
                .spacing([14.0, 8.0])
                .show(ui, |ui| {
                    for c in &book.meta.chapters {
                        ui.label(
                            egui::RichText::new(format!("{:>2}", c.n))
                                .font(theme::mono_font(13.0))
                                .color(p.ghost),
                        );
                        let heading = if c.title.trim().is_empty() {
                            format!("Chapter {}", c.n)
                        } else {
                            c.title.clone()
                        };
                        ui.scope(|ui| {
                            ui.set_max_width(380.0);
                            ui.add(
                                egui::Label::new(egui::RichText::new(heading).color(if c.done {
                                    p.paper
                                } else {
                                    p.brass
                                }))
                                .wrap(),
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!("{} words", c.words))
                                    .color(p.ghost)
                                    .size(11.5),
                            );
                            let (status, color) = if c.done {
                                ("typed \u{2713}", p.verdigris)
                            } else if c.typed_chars > 0 {
                                ("in progress", p.brass)
                            } else {
                                ("not typed", p.ghost)
                            };
                            ui.label(egui::RichText::new(status).color(color).size(11.5));
                        });
                        ui.horizontal(|ui| {
                            if first_untyped == Some(c.n) {
                                // The session always serves the first untyped chapter,
                                // so only that row gets a Type button.
                                let b = egui::Button::new(
                                    egui::RichText::new("Type")
                                        .size(12.5)
                                        .strong()
                                        .color(p.ink_850),
                                )
                                .fill(p.verdigris);
                                if ui.add(b).clicked() {
                                    start_typing(app, slug);
                                }
                            }
                            if ui
                                .button(egui::RichText::new("Rewrite").color(p.ghost).size(12.0))
                                .clicked()
                            {
                                app.book_ui.rewrite_n = Some(c.n);
                                app.book_ui.rewrite_instruction.clear();
                            }
                        });
                        ui.end_row();
                    }
                });
        }

        // Rewrite dialog (independent of the next-step block).
        if let Some(rn) = app.book_ui.rewrite_n {
            ui.add_space(8.0);
            ui.group(|ui| {
                ui.label(egui::RichText::new(format!("Rewrite chapter {rn}")).color(p.brass));
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
                        let prompt =
                            prompt::rewrite_prompt(&book, rn, &app.book_ui.rewrite_instruction);
                        app.book_ui.rewrite_n = None;
                        app.start_generation(rn, true, false, prompt);
                    }
                    if ui.button("Cancel").clicked() {
                        app.book_ui.rewrite_n = None;
                    }
                });
            });
        }

        ui.add_space(14.0);
        ui.separator();
        ui.add_space(8.0);

        // Exactly one next-step block: the clarifying turn, the blank-input
        // confirmation, the finished state, the generate block, or the type prompt.
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
                    let conclude = app.book_ui.make_last;
                    let prompt = prompt::chapter_prompt(&book, n, &combined, false, None, conclude);
                    app.book_ui.pending_questions = None;
                    app.book_ui.clarify_answer.clear();
                    app.start_generation(n, false, conclude, prompt);
                }
            });
        } else if app.book_ui.confirm_blank {
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
                        let conclude = app.book_ui.make_last;
                        let prompt = prompt::chapter_prompt(&book, n, "", false, None, conclude);
                        app.book_ui.confirm_blank = false;
                        app.start_generation(n, false, conclude, prompt);
                    }
                    if ui.button("No, let me add direction").clicked() {
                        app.book_ui.confirm_blank = false;
                    }
                });
            });
        } else if book.meta.concluded {
            ui.label(
                egui::RichText::new("This book is finished.")
                    .color(p.brass)
                    .strong(),
            );
            ui.label(
                egui::RichText::new(if all_typed {
                    "Every chapter is typed. Export it, or rewrite any chapter if you like."
                } else {
                    "The final chapter is written; type it to bind the book. Rewrites are \
still possible."
                })
                .color(p.ghost),
            );
            if !all_typed {
                if let Some(n) = first_untyped {
                    ui.add_space(6.0);
                    let b = egui::Button::new(
                        egui::RichText::new(format!("Type chapter {n}"))
                            .size(15.0)
                            .strong()
                            .color(p.ink_850),
                    )
                    .fill(p.verdigris)
                    .min_size(egui::vec2(210.0, 40.0));
                    if ui.add(b).clicked() {
                        start_typing(app, slug);
                    }
                }
            }
        } else if n_chapters == 0 || all_typed {
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
            ui.checkbox(
                &mut app.book_ui.make_last,
                "Make this chapter the last chapter of the book",
            );
            ui.add_space(4.0);
            let b = egui::Button::new(
                egui::RichText::new("Generate next chapter")
                    .size(15.0)
                    .strong()
                    .color(p.ink_850),
            )
            .fill(p.verdigris)
            .min_size(egui::vec2(210.0, 40.0));
            if ui.add(b).clicked() {
                let cont = app.book_ui.continuation.trim().to_string();
                if cont.is_empty() {
                    // Blank input: confirm they want fully AI-invented content.
                    app.book_ui.confirm_blank = true;
                } else {
                    // The author gets at most one clarifying turn per generation.
                    let conclude = app.book_ui.make_last;
                    let prompt = prompt::chapter_prompt(&book, next_n, &cont, true, None, conclude);
                    app.start_generation(next_n, false, conclude, prompt);
                }
            }
        } else if let Some(n) = first_untyped {
            let b = egui::Button::new(
                egui::RichText::new(format!("Type chapter {n}"))
                    .size(15.0)
                    .strong()
                    .color(p.ink_850),
            )
            .fill(p.verdigris)
            .min_size(egui::vec2(210.0, 40.0));
            ui.horizontal(|ui| {
                if ui.add(b).clicked() {
                    start_typing(app, slug);
                }
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(
                        "Finish typing every generated chapter to unlock the next one.",
                    )
                    .color(p.ghost),
                );
            });
        }
    });
}

/// A typographic placeholder where the cover goes before one is generated: the book's
/// initial on an ink field with a brass rule, so the jacket layout holds its shape.
fn cover_placeholder(ui: &mut egui::Ui, p: &theme::Palette, title: &str, size: egui::Vec2) {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, egui::CornerRadius::same(4), p.ink_950);
    painter.rect_stroke(
        rect,
        egui::CornerRadius::same(4),
        egui::Stroke::new(1.0, p.edge),
        egui::StrokeKind::Inside,
    );
    let rule = egui::Rect::from_min_size(
        rect.min + egui::vec2(14.0, 24.0),
        egui::vec2(rect.width() - 28.0, 2.0),
    );
    painter.rect_filled(rule, egui::CornerRadius::same(1), p.brass);
    let initial = title
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "B".to_string());
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        initial,
        theme::display_font(54.0),
        p.brass,
    );
    painter.text(
        egui::pos2(rect.center().x, rect.max.y - 16.0),
        egui::Align2::CENTER_CENTER,
        "no cover yet",
        theme::ui_font(10.0),
        p.ghost,
    );
}

fn writing_view(app: &mut App, ui: &mut egui::Ui) {
    let p = app.palette();
    ui.add_space(36.0);
    let mut cancel_clicked = false;
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new("Writing...")
                .font(theme::display_font(26.0))
                .color(p.brass),
        );
        ui.add_space(4.0);
        if let Some(gen) = &app.gen {
            let secs = gen.started.elapsed().as_secs();
            ui.label(
                egui::RichText::new(format!("{}s elapsed", secs))
                    .color(p.ghost)
                    .size(12.0),
            );
        }
        ui.add_space(4.0);
        if ui.button("Cancel").clicked() {
            cancel_clicked = true;
        }
    });
    if cancel_clicked {
        app.cancel_generation();
    }
    ui.add_space(12.0);
    theme::centered_column(ui, 760.0, |ui| {
        theme::card(&p).show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::ScrollArea::vertical()
                .max_height(380.0)
                .stick_to_bottom(true)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                .show(ui, |ui| {
                    if let Some(gen) = &app.gen {
                        // Strip the markers from the live view for readability.
                        let shown = gen
                            .live_text
                            .replace("===TITLE===", "")
                            .replace("===CHAPTER===", "")
                            .replace("===BIBLE===", "\n\n[continuity notes...]")
                            .replace("===END===", "");
                        ui.label(egui::RichText::new(shown).color(p.paper).size(14.5));
                    }
                });
        });
    });
    ui.ctx().request_repaint();
}

fn export_markdown(app: &mut App, book: &crate::core::book::store::Book) {
    let md = crate::core::book::export::export_markdown(book);
    let name = format!("{}.md", book.meta.slug);
    match write_download(&name, md.as_bytes()) {
        Ok(path) => app.book_ui.status = Some(finish_export(&path)),
        Err(e) => app.book_ui.status = Some(format!("Markdown export failed: {e}")),
    }
}

fn export_pdf(app: &mut App, book: &crate::core::book::store::Book) {
    match crate::core::book::export::export_pdf(book) {
        Ok(bytes) => {
            let name = format!("{}.pdf", book.meta.slug);
            match write_download(&name, &bytes) {
                Ok(path) => app.book_ui.status = Some(finish_export(&path)),
                Err(e) => app.book_ui.status = Some(format!("PDF export failed: {e}")),
            }
        }
        Err(e) => app.book_ui.status = Some(format!("PDF export failed: {e}")),
    }
}

/// Open the exported file with the system default viewer (detached, non-blocking) and
/// build the status line. A failed open degrades to just showing the path.
fn finish_export(path: &std::path::Path) -> String {
    match open_with_default_app(path) {
        Ok(()) => format!("Saved and opened {}", path.display()),
        Err(e) => format!("Saved to {} (could not open a viewer: {e})", path.display()),
    }
}

fn open_with_default_app(path: &std::path::Path) -> std::io::Result<()> {
    let mut child = std::process::Command::new("xdg-open")
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    // Reap on a background thread so the viewer never blocks the UI and no zombie
    // lingers; the viewer itself keeps running independently.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
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

/// Load (and cache) the cover PNG as an egui texture. The cache key includes the file
/// mtime so a regenerated cover replaces the old texture; misses decode a downscaled
/// thumbnail to keep GPU memory small.
fn cover_texture(
    app: &mut App,
    ui: &egui::Ui,
    book: &crate::core::book::store::Book,
) -> Option<egui::TextureHandle> {
    let path = book.cover_path();
    let mtime = std::fs::metadata(&path).ok()?.modified().ok()?;
    let key = format!("{}:{:?}", book.meta.slug, mtime);
    if let Some((k, tex)) = &app.cover_tex {
        if *k == key {
            return Some(tex.clone());
        }
    }
    let bytes = std::fs::read(&path).ok()?;
    let img = image::load_from_memory(&bytes).ok()?.thumbnail(320, 512);
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let ci = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba.as_raw());
    let tex = ui.ctx().load_texture(
        format!("cover-{}", book.meta.slug),
        ci,
        egui::TextureOptions::LINEAR,
    );
    app.cover_tex = Some((key, tex.clone()));
    Some(tex)
}

/// Ensure the Books tab keeps content mode aligned when opened directly.
pub fn ensure_screen(app: &mut App) {
    app.screen = Screen::Books;
}
