use crate::{widgets, LauncherApp, View};

#[derive(Default)]
pub struct ModBrowserState {
    pub current_instance_id: String,
    pub query: String,
    pub results: Vec<release_the_launcher_mods::ProjectInfo>,
    pub status: String,
    pub installing_mod_id: Option<String>,
    pub install_status: String,
}

pub fn process_message(app: &mut crate::LauncherApp, msg: crate::UiMessage) {
    let state = &mut app.mod_browser_state;
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

pub fn show(app: &mut LauncherApp, ui: &mut egui::Ui, instance_id: &str) {
    let Some(instance) = app.coordinator.instance_summary(instance_id) else {
        ui.label("Instance not found.");
        return;
    };
    let mc_version = instance.mc_version.clone();
    let loader_name = instance.loader_name.clone();

    show_header(app, ui, instance_id, &mc_version, &loader_name);

    let mut state = std::mem::take(&mut app.mod_browser_state);
    show_search(app, ui, &mut state, &mc_version, &loader_name);
    show_results(app, ui, &mut state, instance_id, &mc_version, &loader_name);
    app.mod_browser_state = state;
}

fn show_header(
    app: &mut LauncherApp,
    ui: &mut egui::Ui,
    id: &str,
    mc_version: &str,
    loader_name: &str,
) {
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
                app.coordinator.log(
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

fn show_search(
    app: &LauncherApp,
    ui: &mut egui::Ui,
    state: &mut ModBrowserState,
    mc_version: &str,
    loader_name: &str,
) {
    ui.add_space(app.theme.spacing.sm);
    if widgets::search_bar(ui, &mut state.query) {
        app.coordinator.log(
            crate::log::LogLevel::Info,
            &format!("UI: Searched mods for '{}'", state.query),
        );
        state.status = "Searching compatible mods...".to_string();
        state.results = Vec::new();
        let query = state.query.clone();
        app.coordinator
            .search_mods(query, mc_version.to_string(), loader_name.to_string());
    }
}

fn show_results(
    app: &mut LauncherApp,
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
                        let authors_str = result.authors.join(", ");
                        ui.colored_label(app.theme.text_secondary, format!("by {authors_str}"));
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
                        app.coordinator.log(
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

                        app.coordinator.install_mod(
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
