use crate::App;
use crate::View;
use release_the_launcher_mods::{ModProvider, ModrinthProvider, SearchArgs, SortOrder};

#[derive(Default)]
pub struct ModBrowserState {
    pub query: String,
    pub results: Vec<release_the_launcher_mods::ProjectSummary>,
    pub status: String,
    pub installing_mod_id: Option<String>,
    pub install_status: String,
}

pub fn show(app: &mut App, ui: &mut egui::Ui, instance_id: &str, state: &mut ModBrowserState) {
    let id = instance_id.to_string();
    process_messages(app, state);
    show_header(app, ui, &id);
    show_search(app, ui, state);
    ui.add_space(app.theme.spacing.sm);
    ui.separator();
    ui.add_space(app.theme.spacing.sm);
    show_results(app, ui, state, &id);
}

fn process_messages(app: &mut App, state: &mut ModBrowserState) {
    let messages = app.drain_messages();
    for msg in messages {
        match msg {
            crate::UiMessage::ModrinthSearchResult(result) => match result {
                Ok(results) => {
                    state.status = format!("Found {} mods", results.total_hits);
                    state.results = results.hits;
                }
                Err(e) => {
                    state.status = format!("Search failed: {e}");
                }
            },
            crate::UiMessage::ModrinthInstallResult(result) => match result {
                Ok(name) => {
                    state.status = format!("Installed mod: {name}");
                    state.installing_mod_id = None;
                }
                Err(e) => {
                    state.status = format!("Install failed: {e}");
                    state.installing_mod_id = None;
                }
            },
            _ => {}
        }
    }
}

fn show_header(app: &mut App, ui: &mut egui::Ui, id: &str) {
    ui.horizontal(|ui| {
        ui.heading("Mod Browser (Modrinth)");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(format!(" {} Back", crate::icons::BACK)).clicked() {
                app.current_view = View::InstanceDetail {
                    id: id.to_string(),
                    tab: crate::DetailTab::Mods,
                };
            }
        });
    });
}

fn show_search(app: &App, ui: &mut egui::Ui, state: &mut ModBrowserState) {
    ui.add_space(app.theme.spacing.sm);
    ui.horizontal(|ui| {
        ui.label("Search:");
        ui.text_edit_singleline(&mut state.query);
        if ui
            .button(format!(" {} Search", crate::icons::SEARCH))
            .clicked()
            && !state.query.is_empty()
        {
            state.status = "Searching...".to_string();
            state.results = Vec::new();
            let query = state.query.clone();
            let queue = app.ui_queue.clone();
            let ctx = app.ctx.clone().expect("egui context not set");
            let Some(handle) = app.tokio_handle.clone() else {
                return;
            };
            handle.spawn(async move {
                let provider = ModrinthProvider::new(None);
                let args = SearchArgs {
                    query,
                    offset: 0,
                    limit: 20,
                    loaders: vec![],
                    mc_versions: vec![],
                    categories: vec![],
                    sort: SortOrder::Downloads,
                };
                let result = match provider.search(args).await {
                    Ok(results) => crate::UiMessage::ModrinthSearchResult(Ok(results)),
                    Err(e) => crate::UiMessage::ModrinthSearchResult(Err(e.to_string())),
                };
                if let Ok(mut q) = queue.lock() {
                    q.push(result);
                }
                ctx.request_repaint();
            });
        }
    });
}

fn show_results(app: &App, ui: &mut egui::Ui, state: &mut ModBrowserState, id: &str) {
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
                        let project_id = result.id.clone();
                        state.installing_mod_id = Some(project_id.clone());
                        state.install_status = format!("Installing {}...", result.name);
                        let mods_dir = app
                            .instance_manager
                            .get_mods_dir(&id.to_string())
                            .unwrap_or_default();
                        app.install_mod_from_modrinth(project_id, mods_dir);
                    }
                });
            }
        });
}
