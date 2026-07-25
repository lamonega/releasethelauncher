use crate::App;
use crate::View;
use release_the_launcher_core::ModLoader;
use release_the_launcher_mods::ProjectSummary;

pub fn show(app: &mut App, ui: &mut egui::Ui, state: &mut NewInstanceState) {
    let messages = app.drain_messages();
    for msg in messages {
        match msg {
            crate::UiMessage::ModrinthSearchResult(result) => match result {
                Ok(results) => {
                    state.modrinth_status = format!("Found {} modpacks", results.total_hits);
                    state.modrinth_results = results.hits;
                }
                Err(e) => {
                    state.modrinth_status = format!("Search failed: {e}");
                }
            },
            crate::UiMessage::ModrinthInstallResult(result) => match result {
                Ok(name) => {
                    state.modrinth_status = format!("Installed modpack: {name}");
                    state.installing_modpack_id = None;
                }
                Err(e) => {
                    state.modrinth_status = format!("Install failed: {e}");
                    state.installing_modpack_id = None;
                }
            },
            _ => {}
        }
    }

    ui.horizontal(|ui| {
        ui.heading("New Instance");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Back").clicked() {
                app.current_view = View::InstanceList;
            }
        });
    });

    ui.horizontal(|ui| {
        let manual_style = if state.tab == InstanceTab::Manual {
            egui::Button::new("Manual").fill(egui::Color32::from_rgb(60, 60, 80))
        } else {
            egui::Button::new("Manual")
        };
        if ui.add(manual_style).clicked() {
            state.tab = InstanceTab::Manual;
        }

        let modrinth_style = if state.tab == InstanceTab::Modrinth {
            egui::Button::new("From Modrinth").fill(egui::Color32::from_rgb(60, 60, 80))
        } else {
            egui::Button::new("From Modrinth")
        };
        if ui.add(modrinth_style).clicked() {
            state.tab = InstanceTab::Modrinth;
        }
    });

    ui.separator();

    match state.tab {
        InstanceTab::Manual => show_manual(app, ui, state),
        InstanceTab::Modrinth => show_modrinth(app, ui, state),
    }
}

fn show_manual(app: &mut App, ui: &mut egui::Ui, state: &mut NewInstanceState) {
    ui.label("Name:");
    ui.text_edit_singleline(&mut state.name);

    ui.label("Minecraft Version:");
    ui.text_edit_singleline(&mut state.mc_version);

    ui.label("Loader:");
    egui::ComboBox::from_label("Mod Loader")
        .selected_text(state.loader_type.as_str())
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut state.loader_type, LoaderType::Vanilla, "Vanilla");
            ui.selectable_value(&mut state.loader_type, LoaderType::Fabric, "Fabric");
            ui.selectable_value(&mut state.loader_type, LoaderType::Forge, "Forge");
            ui.selectable_value(&mut state.loader_type, LoaderType::NeoForge, "NeoForge");
        });

    if state.loader_type != LoaderType::Vanilla {
        ui.label("Loader Version:");
        ui.text_edit_singleline(&mut state.loader_version);
    }

    ui.separator();

    if ui.button("Create Instance").clicked()
        && !state.name.is_empty()
        && !state.mc_version.is_empty()
    {
        let loader = match state.loader_type {
            LoaderType::Vanilla => ModLoader::Vanilla,
            LoaderType::Fabric => ModLoader::Fabric {
                loader_version: state.loader_version.clone(),
            },
            LoaderType::Forge => ModLoader::Forge {
                loader_version: state.loader_version.clone(),
            },
            LoaderType::NeoForge => ModLoader::NeoForge {
                loader_version: state.loader_version.clone(),
            },
        };

        let settings = release_the_launcher_core::InstanceSettings::new(
            state.name.clone(),
            state.mc_version.clone(),
            loader,
        );

        match app.instance_manager.create(&state.name, settings) {
            Ok(_) => {
                app.log(
                    crate::log::LogLevel::Info,
                    &format!("Created instance: {}", state.name),
                );
                app.status_message = format!("Created instance: {}", state.name);
                app.current_view = View::InstanceList;
            }
            Err(e) => {
                app.log(
                    crate::log::LogLevel::Error,
                    &format!("Failed to create instance: {e}"),
                );
                app.status_message = format!("Error: {e}");
            }
        }
    }
}

fn show_modrinth(app: &mut App, ui: &mut egui::Ui, state: &mut NewInstanceState) {
    ui.label("Search for modpacks on Modrinth:");

    ui.horizontal(|ui| {
        ui.label("Search:");
        ui.text_edit_singleline(&mut state.modrinth_query);
    });

    ui.horizontal(|ui| {
        ui.label("MC Version:");
        ui.text_edit_singleline(&mut state.mc_version_filter);
        ui.label("Loader:");
        egui::ComboBox::from_id_source("modrinth_loader_filter")
            .selected_text(state.loader_filter.as_str())
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut state.loader_filter, LoaderFilter::Any, "Any");
                ui.selectable_value(&mut state.loader_filter, LoaderFilter::Fabric, "Fabric");
                ui.selectable_value(&mut state.loader_filter, LoaderFilter::Forge, "Forge");
                ui.selectable_value(&mut state.loader_filter, LoaderFilter::NeoForge, "NeoForge");
            });
    });

    if ui.button("Search").clicked() && !state.modrinth_query.is_empty() {
        state.modrinth_status = "Searching...".to_string();
        state.modrinth_results = Vec::new();
        let loader = match state.loader_filter {
            LoaderFilter::Any => String::new(),
            LoaderFilter::Fabric => "fabric".to_string(),
            LoaderFilter::Forge => "forge".to_string(),
            LoaderFilter::NeoForge => "neoforge".to_string(),
        };
        app.search_modrinth_modpacks(
            state.modrinth_query.clone(),
            state.mc_version_filter.clone(),
            loader,
        );
    }

    ui.separator();

    if !state.modrinth_status.is_empty() {
        ui.label(&state.modrinth_status);
    }

    if state.modrinth_results.is_empty() && state.modrinth_status.is_empty() {
        ui.label("Search for modpacks to install.");
    } else {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for result in &state.modrinth_results {
                    ui.group(|ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(&result.name);
                            ui.label(format!("by {}", result.author));
                            ui.label(format!("({} downloads)", result.downloads));
                        });
                        ui.label(&result.description);

                        if state.installing_modpack_id == Some(result.id.clone()) {
                            ui.label(&state.modrinth_status);
                        } else if ui.button("Install as New Instance").clicked() {
                            state.installing_modpack_id = Some(result.id.clone());
                            state.modrinth_status = format!("Installing {}...", result.name);
                        }
                    });
                }
            });
    }
}

#[derive(Default)]
pub struct NewInstanceState {
    pub name: String,
    pub mc_version: String,
    pub loader_type: LoaderType,
    pub loader_version: String,
    pub tab: InstanceTab,
    pub modrinth_query: String,
    pub mc_version_filter: String,
    pub loader_filter: LoaderFilter,
    pub modrinth_status: String,
    pub modrinth_results: Vec<ProjectSummary>,
    pub installing_modpack_id: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum InstanceTab {
    #[default]
    Manual,
    Modrinth,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum LoaderType {
    #[default]
    Vanilla,
    Fabric,
    Forge,
    NeoForge,
}

impl LoaderType {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            LoaderType::Vanilla => "Vanilla",
            LoaderType::Fabric => "Fabric",
            LoaderType::Forge => "Forge",
            LoaderType::NeoForge => "NeoForge",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum LoaderFilter {
    #[default]
    Any,
    Fabric,
    Forge,
    NeoForge,
}

impl LoaderFilter {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            LoaderFilter::Any => "Any",
            LoaderFilter::Fabric => "Fabric",
            LoaderFilter::Forge => "Forge",
            LoaderFilter::NeoForge => "NeoForge",
        }
    }
}
