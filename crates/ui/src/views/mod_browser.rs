use crate::{widgets, App, View};

#[derive(Default)]
pub struct ModBrowserState {
    pub current_instance_id: String,
    pub query: String,
    pub results: Vec<release_the_launcher_mods::ProjectSummary>,
    pub status: String,
    pub installing_mod_id: Option<String>,
    pub install_status: String,
}

pub fn show(app: &mut App, ui: &mut egui::Ui, instance_id: &str, state: &mut ModBrowserState) {
    let id = instance_id.to_string();
    process_messages(app, state);

    let (mc_version, loader_name) = app
        .coordinator
        .instance_summary(&id)
        .map(|summary| (summary.mc_version, summary.loader_name))
        .unwrap_or_default();

    if state.current_instance_id != id {
        state.current_instance_id.clone_from(&id);
        state.query.clear();
        state.results.clear();
        state.status = "Loading compatible mods...".to_string();
        trigger_search(app, "", &mc_version, &loader_name);
    }

    show_header(app, ui, &id, &mc_version, &loader_name);
    show_search(app, ui, state, &mc_version, &loader_name);
    ui.add_space(app.theme.spacing.sm);
    ui.separator();
    ui.add_space(app.theme.spacing.sm);
    show_results(app, ui, state, &id, &mc_version, &loader_name);
}

fn process_messages(app: &App, state: &mut ModBrowserState) {
    let messages = app.drain_view_events();
    for msg in messages {
        match msg {
            crate::UiMessage::ModrinthSearchResult(result) => match result {
                Ok(results) => {
                    state.status = format!("Found {} compatible mods", results.total_hits);
                    state.results = results.hits;
                }
                Err(e) => {
                    state.status = format!("Search failed: {e}");
                }
            },
            crate::UiMessage::ModrinthInstallResult { name, .. } => {
                state.status = format!("Installed: {name}");
                state.installing_mod_id = None;
            }
            crate::UiMessage::DownloadError(err) => {
                state.status = format!("Install failed: {err}");
                state.installing_mod_id = None;
            }
            _ => {}
        }
    }
}

fn show_header(app: &mut App, ui: &mut egui::Ui, id: &str, mc_version: &str, loader_name: &str) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.heading("Browse Mods (Modrinth)");
            ui.colored_label(
                app.theme.text_secondary,
                format!("Compatible with Minecraft {mc_version} ({loader_name})"),
            );
        });
        widgets::right_aligned(ui, |ui| {
            if ui
                .add(widgets::icon_button(crate::icons::BACK, "Back"))
                .clicked()
            {
                app.log(
                    crate::log::LogLevel::Info,
                    "UI: Navigated back from Mod Browser",
                );
                app.current_view = View::InstanceDetail {
                    id: id.to_string(),
                    tab: crate::DetailTab::Mods,
                };
            }
        });
    });
}

fn trigger_search(app: &App, query_str: &str, mc_version: &str, loader_name: &str) {
    app.search_mods(
        query_str.to_string(),
        mc_version.to_string(),
        loader_name.to_string(),
    );
}

fn show_search(
    app: &App,
    ui: &mut egui::Ui,
    state: &mut ModBrowserState,
    mc_version: &str,
    loader_name: &str,
) {
    ui.add_space(app.theme.spacing.sm);
    if widgets::search_bar(ui, &mut state.query) {
        app.log(
            crate::log::LogLevel::Info,
            &format!("UI: Searched mods for '{}'", state.query),
        );
        state.status = "Searching compatible mods...".to_string();
        state.results = Vec::new();
        let query = state.query.clone();
        trigger_search(app, &query, mc_version, loader_name);
    }
}

fn show_results(
    app: &App,
    ui: &mut egui::Ui,
    state: &mut ModBrowserState,
    id: &str,
    mc_version: &str,
    loader_name: &str,
) {
    if !state.status.is_empty() {
        ui.colored_label(app.theme.text_secondary, &state.status);
        ui.add_space(app.theme.spacing.sm);
    }

    if state.results.is_empty() && state.status.is_empty() {
        crate::empty_state(ui, &app.theme, &["Search for mods to install."]);
        return;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for result in &state.results {
                ui.group(|ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        if let Some(icon_url) = &result.icon_url {
                            ui.add(
                                egui::Image::new(icon_url.as_str())
                                    .max_size(egui::vec2(32.0, 32.0))
                                    .show_loading_spinner(true)
                                    .rounding(4.0),
                            );
                        }
                        ui.label(&result.name);
                        ui.colored_label(app.theme.text_secondary, format!("by {}", result.author));
                        ui.colored_label(
                            app.theme.text_secondary,
                            format!("({} downloads)", result.downloads),
                        );
                    });
                    ui.label(&result.description);
                    ui.add_space(app.theme.spacing.sm);
                    if state.installing_mod_id == Some(result.id.clone()) {
                        ui.colored_label(app.theme.text_secondary, &state.install_status);
                    } else if ui.button("Install").clicked() {
                        app.log(
                            crate::log::LogLevel::Info,
                            &format!("UI: Installing mod '{}' from Modrinth", result.name),
                        );
                        let project_id = result.id.clone();
                        state.installing_mod_id = Some(project_id.clone());
                        state.install_status = format!("Installing {}...", result.name);
                        let mods_dir = app.coordinator.instance_mods_dir(id).unwrap_or_default();

                        let loader_clean = loader_name
                            .to_lowercase()
                            .split_whitespace()
                            .next()
                            .unwrap_or("")
                            .to_string();

                        app.install_mod_from_modrinth(
                            project_id,
                            mods_dir,
                            Some(mc_version.to_string()),
                            Some(loader_clean),
                        );
                    }
                });
            }
        });
}
