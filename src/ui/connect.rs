//! The Connect Claude screen: the whole authentication flow lives in the app. The user
//! clicks Connect, gets a clickable sign-in link (their logged-in claude.ai browser
//! session), authorizes, pastes the short code back into the app, and the app stores the
//! resulting token. No terminal, ever.

use crate::core::claude_auth::AuthCheck;
use crate::ui::app::{App, ConnectUiState, Screen};

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let p = app.palette();

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.add_space(16.0);
        ui.label(
            egui::RichText::new("Connect Claude")
                .color(p.brass)
                .size(24.0)
                .strong(),
        );
        ui.label(
            egui::RichText::new(
                "Book mode writes your novel with Claude, using your Claude subscription. \
Connect once; the other practice modes never need it.",
            )
            .color(p.ghost),
        );
        ui.add_space(12.0);

        // A connect flow in progress takes priority over the passive check states.
        match app.auth.state.clone() {
            ConnectUiState::Idle => idle_view(app, ui),
            ConnectUiState::Starting => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Starting Anthropic's sign-in flow...");
                });
                cancel_button(app, ui);
            }
            ConnectUiState::UrlShown {
                url,
                waiting_for_code,
            } => {
                ui.group(|ui| {
                    ui.label(
                        egui::RichText::new("Step 1 - Authorize in your browser")
                            .color(p.brass)
                            .strong(),
                    );
                    ui.label(
                        "A browser tab should have opened. If not, click this link \
(it uses your existing claude.ai sign-in):",
                    );
                    ui.horizontal_wrapped(|ui| {
                        ui.hyperlink_to("Open the Claude sign-in page", url.clone());
                        if ui.button("Copy link").clicked() {
                            ui.ctx().copy_text(url.clone());
                        }
                    });
                });
                ui.add_space(8.0);
                ui.group(|ui| {
                    ui.label(
                        egui::RichText::new("Step 2 - Paste the code here")
                            .color(p.brass)
                            .strong(),
                    );
                    ui.label(
                        "After you click Authorize, the page shows a short code. \
Paste it below.",
                    );
                    if !waiting_for_code {
                        ui.label(
                            egui::RichText::new("(waiting for the sign-in flow to ask for it...)")
                                .color(p.ghost)
                                .size(12.0),
                        );
                    }
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut app.auth.code_input)
                                .desired_width(420.0)
                                .hint_text("Paste the authentication code"),
                        );
                        let can_submit = !app.auth.code_input.trim().is_empty();
                        if ui
                            .add_enabled(can_submit, egui::Button::new("Connect"))
                            .clicked()
                        {
                            let code = app.auth.code_input.trim().to_string();
                            if let Some(flow) = app.auth.flow.as_mut() {
                                flow.submit_code(&code);
                            }
                            app.auth.code_input.clear();
                            app.auth.state = ConnectUiState::Verifying;
                        }
                    });
                });
                cancel_button(app, ui);
            }
            ConnectUiState::Verifying => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Checking the code and storing your connection...");
                });
                cancel_button(app, ui);
            }
            ConnectUiState::Failed(msg) => {
                ui.label(egui::RichText::new(&msg).color(p.ribbon));
                ui.add_space(6.0);
                if ui.button("Try connecting again").clicked() {
                    app.start_connect_flow();
                }
                back_button(app, ui);
            }
        }
        ui.add_space(20.0);
    });

    // Keep polling while a flow is active.
    if app.auth.flow.is_some() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
    }
}

fn idle_view(app: &mut App, ui: &mut egui::Ui) {
    let p = app.palette();
    match app.auth.check.clone() {
        None => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Checking your Claude connection...");
            });
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
        }
        Some(AuthCheck::ConnectedToken) => {
            ui.label(
                egui::RichText::new("Connected. Book mode is ready.").color(p.verdigris),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Go to Books").clicked() {
                    app.screen = Screen::Books;
                }
                if ui.button("Disconnect").clicked() {
                    crate::core::claude_auth::delete_token();
                    app.auth.check = None;
                    app.refresh_auth();
                }
            });
        }
        Some(AuthCheck::ConnectedCli) => {
            ui.label(
                egui::RichText::new("Connected (Claude is already signed in on this computer).")
                    .color(p.verdigris),
            );
            ui.add_space(6.0);
            if ui.button("Go to Books").clicked() {
                app.screen = Screen::Books;
            }
        }
        Some(AuthCheck::CliMissing) => {
            ui.label(
                egui::RichText::new(
                    "Claude Code isn't installed on this computer, so Book mode can't run. \
The other practice modes work fine without it.",
                )
                .color(p.ribbon),
            );
            ui.label(
                egui::RichText::new(
                    "Installing it is a one-time step described in the README \
(make install-claude on Debian/Ubuntu).",
                )
                .color(p.ghost),
            );
            ui.add_space(6.0);
            if ui.button("Check again").clicked() {
                app.auth.check = None;
                app.refresh_auth();
            }
        }
        Some(AuthCheck::NotConnected) | Some(AuthCheck::Unknown(_)) => {
            if let Some(AuthCheck::Unknown(e)) = &app.auth.check {
                ui.label(
                    egui::RichText::new(format!("Could not verify the connection ({e})."))
                        .color(p.ghost)
                        .size(12.0),
                );
            }
            ui.label("You'll need your claude.ai account signed in, in your browser.");
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .button(egui::RichText::new("Connect Claude").strong())
                    .clicked()
                {
                    app.start_connect_flow();
                }
                if ui.button("Check again").clicked() {
                    app.auth.check = None;
                    app.refresh_auth();
                }
            });
        }
    }
}

fn cancel_button(app: &mut App, ui: &mut egui::Ui) {
    ui.add_space(6.0);
    if ui.button("Cancel").clicked() {
        if let Some(mut flow) = app.auth.flow.take() {
            flow.cancel();
        }
        app.auth.state = ConnectUiState::Idle;
        app.auth.check = None;
        app.refresh_auth();
    }
}

fn back_button(app: &mut App, ui: &mut egui::Ui) {
    if ui.button("Back to Books").clicked() {
        app.auth.state = ConnectUiState::Idle;
        app.screen = Screen::Books;
    }
}
