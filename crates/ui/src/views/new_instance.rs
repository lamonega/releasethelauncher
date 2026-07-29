use crate::App;
use crate::View;
use release_the_launcher_core::ModLoader;
use release_the_launcher_mods::{ModVersion, ProjectSummary};

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
            crate::UiMessage::ModrinthVersionsResult { project_id, result } => match result {
                Ok(versions) => {
                    state.modpack_versions.insert(project_id, versions);
                    state.loading_versions_for_project = None;
                }
                Err(e) => {
                    state.modrinth_status = format!("Failed to load versions: {e}");
                    state.loading_versions_for_project = None;
                }
            },
            crate::UiMessage::ModrinthInstallResult(result) => match result {
                Ok(info) => {
                    let parts: Vec<&str> = info.split('|').collect();
                    if parts.len() == 3 {
                        let name = parts[0];
                        let mc_version = parts[1];
                        let loader_raw = parts[2];
                        let loader = if loader_raw.starts_with("Fabric") {
                            ModLoader::Fabric {
                                loader_version: loader_raw
                                    .strip_prefix("Fabric:")
                                    .unwrap_or("")
                                    .to_string(),
                            }
                        } else if loader_raw.starts_with("Forge") {
                            ModLoader::Forge {
                                loader_version: loader_raw
                                    .strip_prefix("Forge:")
                                    .unwrap_or("")
                                    .to_string(),
                            }
                        } else if loader_raw.starts_with("NeoForge") {
                            ModLoader::NeoForge {
                                loader_version: loader_raw
                                    .strip_prefix("NeoForge:")
                                    .unwrap_or("")
                                    .to_string(),
                            }
                        } else if loader_raw.starts_with("Quilt") {
                            ModLoader::Quilt {
                                loader_version: loader_raw
                                    .strip_prefix("Quilt:")
                                    .unwrap_or("")
                                    .to_string(),
                            }
                        } else {
                            ModLoader::Vanilla
                        };
                        let settings = release_the_launcher_core::InstanceSettings::new(
                            name.to_string(),
                            mc_version.to_string(),
                            loader,
                        );
                        let _ = app.instance_manager.create(name, settings);
                        app.status_message = format!("Installed modpack instance: {name}");
                        app.current_view = View::InstanceList;
                    } else {
                        state.modrinth_status = format!("Installed: {info}");
                    }
                    state.installing_modpack_id = None;
                }
                Err(e) => {
                    state.modrinth_status = format!("Install failed: {e}");
                    state.installing_modpack_id = None;
                }
            },
            crate::UiMessage::VersionListResult(result) => match result {
                Ok(versions) => {
                    if let Some((latest, _)) = versions.iter().find(|(_, t)| t == "release").or_else(|| versions.first()) {
                        state.mc_version = latest.clone();
                    }
                    state.available_versions = versions;
                    state.version_list_loaded = true;
                }
                Err(e) => {
                    state.modrinth_status = format!("Failed to load versions: {e}");
                }
            },
            crate::UiMessage::LoaderVersionsResult {
                loader_type,
                mc_version,
                result,
            } => {
                if state.loader_type.as_str() == loader_type && state.mc_version == mc_version {
                    match result {
                        Ok(versions) => {
                            state.loader_versions = versions;
                            state.loader_versions_loading = false;
                            state.loader_versions_error = None;
                            if !state.loader_versions.is_empty()
                                && (state.loader_version.is_empty()
                                    || !state.loader_versions.contains(&state.loader_version))
                            {
                                state.loader_version = state.loader_versions[0].clone();
                            }
                        }
                        Err(e) => {
                            state.loader_versions_loading = false;
                            state.loader_versions_error = Some(e);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Trigger version list fetch on first open if not yet loaded
    if !state.version_list_loaded && !state.version_list_loading {
        state.version_list_loading = true;
        app.fetch_versions_list();
    }

    ui.horizontal(|ui| {
        ui.heading("New Instance");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(format!(" {} Back", crate::icons::BACK)).clicked() {
                app.log(
                    crate::log::LogLevel::Info,
                    "UI: Navigated back from New Instance",
                );
                app.current_view = View::InstanceList;
            }
        });
    });

    ui.add_space(app.theme.spacing.sm);

    ui.horizontal(|ui| {
        let manual_style = if state.tab == InstanceTab::Manual {
            egui::Button::new("Manual").fill(app.theme.accent)
        } else {
            egui::Button::new("Manual")
        };
        if ui.add(manual_style).clicked() {
            app.log(crate::log::LogLevel::Info, "UI: Switched to Manual tab");
            state.tab = InstanceTab::Manual;
        }

        let modrinth_style = if state.tab == InstanceTab::Modrinth {
            egui::Button::new("From Modrinth").fill(app.theme.accent)
        } else {
            egui::Button::new("From Modrinth")
        };
        if ui.add(modrinth_style).clicked() {
            app.log(
                crate::log::LogLevel::Info,
                "UI: Switched to From Modrinth tab",
            );
            state.tab = InstanceTab::Modrinth;
        }
    });

    ui.add_space(app.theme.spacing.sm);
    ui.separator();
    ui.add_space(app.theme.spacing.sm);

    match state.tab {
        InstanceTab::Manual => show_manual(app, ui, state),
        InstanceTab::Modrinth => show_modrinth(app, ui, state),
    }
}

fn show_manual(app: &mut App, ui: &mut egui::Ui, state: &mut NewInstanceState) {
    ui.label("Name:");
    ui.text_edit_singleline(&mut state.name);

    ui.add_space(app.theme.spacing.sm);

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Filter Minecraft Versions:").strong());
        if ui
            .checkbox(&mut state.manual_show_releases, "Releases")
            .changed()
        {
            app.log(
                crate::log::LogLevel::Info,
                &format!(
                    "UI: MC version filter - Releases: {}",
                    state.manual_show_releases
                ),
            );
        }
        if ui
            .checkbox(&mut state.manual_show_snapshots, "Snapshots")
            .changed()
        {
            app.log(
                crate::log::LogLevel::Info,
                &format!(
                    "UI: MC version filter - Snapshots: {}",
                    state.manual_show_snapshots
                ),
            );
        }
        if ui
            .checkbox(&mut state.manual_show_betas, "Old Betas")
            .changed()
        {
            app.log(
                crate::log::LogLevel::Info,
                &format!("UI: MC version filter - Betas: {}", state.manual_show_betas),
            );
        }
        if ui
            .checkbox(&mut state.manual_show_alphas, "Old Alphas")
            .changed()
        {
            app.log(
                crate::log::LogLevel::Info,
                &format!(
                    "UI: MC version filter - Alphas: {}",
                    state.manual_show_alphas
                ),
            );
        }
    });
    ui.add_space(app.theme.spacing.xs);

    ui.label("Minecraft Version:");
    if state.available_versions.is_empty() {
        if state.version_list_loading {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Loading versions...");
            });
        } else {
            ui.text_edit_singleline(&mut state.mc_version);
        }
    } else {
        let filtered_versions: Vec<&(String, String)> = state
            .available_versions
            .iter()
            .filter(|(_, ver_type)| match ver_type.as_str() {
                "release" => state.manual_show_releases,
                "snapshot" => state.manual_show_snapshots,
                "old_beta" => state.manual_show_betas,
                "old_alpha" => state.manual_show_alphas,
                _ => false,
            })
            .collect();

        egui::ComboBox::from_label("Minecraft Version")
            .selected_text(&state.mc_version)
            .width(220.0)
            .show_ui(ui, |ui| {
                for (version, _) in filtered_versions {
                    if ui
                        .selectable_value(&mut state.mc_version, version.clone(), version)
                        .changed()
                    {
                        app.log(
                            crate::log::LogLevel::Info,
                            &format!("UI: MC version selected: {}", state.mc_version),
                        );
                    }
                }
            });
    }

    ui.add_space(app.theme.spacing.sm);

    ui.label("Loader:");
    egui::ComboBox::from_label("Mod Loader")
        .selected_text(state.loader_type.as_str())
        .show_ui(ui, |ui| {
            if ui
                .selectable_value(&mut state.loader_type, LoaderType::Vanilla, "Vanilla")
                .changed()
            {
                app.log(crate::log::LogLevel::Info, "UI: Loader changed to Vanilla");
            }
            if ui
                .selectable_value(&mut state.loader_type, LoaderType::Fabric, "Fabric")
                .changed()
            {
                app.log(crate::log::LogLevel::Info, "UI: Loader changed to Fabric");
            }
            if ui
                .selectable_value(&mut state.loader_type, LoaderType::Forge, "Forge")
                .changed()
            {
                app.log(crate::log::LogLevel::Info, "UI: Loader changed to Forge");
            }
            if ui
                .selectable_value(&mut state.loader_type, LoaderType::NeoForge, "NeoForge")
                .changed()
            {
                app.log(crate::log::LogLevel::Info, "UI: Loader changed to NeoForge");
            }
            if ui
                .selectable_value(&mut state.loader_type, LoaderType::Quilt, "Quilt")
                .changed()
            {
                app.log(crate::log::LogLevel::Info, "UI: Loader changed to Quilt");
            }
        });

    if state.loader_type != LoaderType::Vanilla && !state.mc_version.is_empty() {
        let current_key = (state.loader_type.clone(), state.mc_version.clone());
        if state.last_fetched_loader_key.as_ref() != Some(&current_key) {
            state.loader_versions_loading = true;
            state.loader_versions_error = None;
            state.loader_versions.clear();
            state.last_fetched_loader_key = Some(current_key.clone());
            app.fetch_loader_versions(
                state.loader_type.as_str().to_string(),
                state.mc_version.clone(),
            );
        }
    }

    if state.loader_type != LoaderType::Vanilla {
        ui.add_space(app.theme.spacing.sm);
        ui.label("Loader Version:");

        if state.loader_versions_loading {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Loading loader versions...");
            });
        } else if !state.loader_versions.is_empty() {
            egui::ComboBox::from_label("Loader Version")
                .selected_text(&state.loader_version)
                .width(220.0)
                .show_ui(ui, |ui| {
                    for ver in &state.loader_versions {
                        if ui
                            .selectable_value(&mut state.loader_version, ver.clone(), ver)
                            .changed()
                        {
                            app.log(
                                crate::log::LogLevel::Info,
                                &format!("UI: Loader version selected: {}", state.loader_version),
                            );
                        }
                    }
                });
        } else {
            if let Some(err) = &state.loader_versions_error {
                ui.label(
                    egui::RichText::new(format!("Failed to load versions: {err}"))
                        .color(app.theme.log_colors.error),
                );
            }
            ui.text_edit_singleline(&mut state.loader_version);
        }
    }

    ui.add_space(app.theme.spacing.sm);
    ui.separator();
    ui.add_space(app.theme.spacing.sm);

    if ui
        .add(
            egui::Button::new(format!(" {} Create Instance", crate::icons::ADD))
                .fill(app.theme.accent),
        )
        .clicked()
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
            LoaderType::Quilt => ModLoader::Quilt {
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
                    &format!(
                        "UI: Created instance '{}' (MC {}, loader {})",
                        state.name,
                        state.mc_version,
                        state.loader_type.as_str()
                    ),
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

fn show_modrinth(app: &App, ui: &mut egui::Ui, state: &mut NewInstanceState) {
    ui.label("Search for modpacks on Modrinth:");

    ui.add_space(app.theme.spacing.sm);

    ui.horizontal(|ui| {
        ui.label("Search:");
        ui.text_edit_singleline(&mut state.modrinth_query);
    });

    ui.add_space(app.theme.spacing.sm);

    ui.horizontal(|ui| {
        ui.label("MC Version:");
        ui.text_edit_singleline(&mut state.mc_version_filter);
        ui.label("Loader:");
        egui::ComboBox::from_id_source("modrinth_loader_filter")
            .selected_text(state.loader_filter.as_str())
            .show_ui(ui, |ui| {
                if ui
                    .selectable_value(&mut state.loader_filter, LoaderFilter::Any, "Any")
                    .changed()
                {
                    app.log(
                        crate::log::LogLevel::Info,
                        "UI: Modrinth loader filter changed to Any",
                    );
                }
                if ui
                    .selectable_value(&mut state.loader_filter, LoaderFilter::Fabric, "Fabric")
                    .changed()
                {
                    app.log(
                        crate::log::LogLevel::Info,
                        "UI: Modrinth loader filter changed to Fabric",
                    );
                }
                if ui
                    .selectable_value(&mut state.loader_filter, LoaderFilter::Forge, "Forge")
                    .changed()
                {
                    app.log(
                        crate::log::LogLevel::Info,
                        "UI: Modrinth loader filter changed to Forge",
                    );
                }
                if ui
                    .selectable_value(&mut state.loader_filter, LoaderFilter::NeoForge, "NeoForge")
                    .changed()
                {
                    app.log(
                        crate::log::LogLevel::Info,
                        "UI: Modrinth loader filter changed to NeoForge",
                    );
                }
                if ui
                    .selectable_value(&mut state.loader_filter, LoaderFilter::Quilt, "Quilt")
                    .changed()
                {
                    app.log(
                        crate::log::LogLevel::Info,
                        "UI: Modrinth loader filter changed to Quilt",
                    );
                }
            });
    });

    ui.add_space(app.theme.spacing.sm);

    if ui
        .button(format!(" {} Search", crate::icons::SEARCH))
        .clicked()
        && !state.modrinth_query.is_empty()
    {
        app.log(
            crate::log::LogLevel::Info,
            &format!("UI: Searched Modrinth for '{}'", state.modrinth_query),
        );
        state.modrinth_status = "Searching...".to_string();
        state.modrinth_results = Vec::new();
        let loader = match state.loader_filter {
            LoaderFilter::Any => String::new(),
            LoaderFilter::Fabric => "fabric".to_string(),
            LoaderFilter::Forge => "forge".to_string(),
            LoaderFilter::NeoForge => "neoforge".to_string(),
            LoaderFilter::Quilt => "quilt".to_string(),
        };
        app.search_modrinth_modpacks(
            state.modrinth_query.clone(),
            state.mc_version_filter.clone(),
            loader,
        );
    }

    ui.add_space(app.theme.spacing.sm);
    ui.separator();
    ui.add_space(app.theme.spacing.sm);

    if !state.modrinth_status.is_empty() {
        ui.label(&state.modrinth_status);
    }

    if state.modrinth_results.is_empty() && state.modrinth_status.is_empty() {
        crate::empty_state(ui, &app.theme, &["Search for modpacks to install."]);
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
                        } else {
                            ui.horizontal(|ui| {
                                if ui
                                    .button(format!(" {} Install Latest", crate::icons::ADD))
                                    .clicked()
                                {
                                    app.log(
                                        crate::log::LogLevel::Info,
                                        &format!("UI: Installing modpack '{}' (latest)", result.name),
                                    );
                                    state.installing_modpack_id = Some(result.id.clone());
                                    state.modrinth_status = format!("Installing {}...", result.name);
                                    let base_dir = dirs::config_dir()
                                        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".config"))
                                        .join("release-the-launcher")
                                        .join("instances");
                                    app.install_modpack_as_instance(result.id.clone(), None, base_dir);
                                }

                                let is_expanded = state.expanded_project_id.as_deref() == Some(&result.id);
                                let expand_text = if is_expanded { "Hide Versions" } else { "Choose Version" };

                                if ui.button(expand_text).clicked() {
                                    if is_expanded {
                                        app.log(
                                            crate::log::LogLevel::Info,
                                            &format!("UI: Hid versions for '{}'", result.name),
                                        );
                                        state.expanded_project_id = None;
                                    } else {
                                        app.log(
                                            crate::log::LogLevel::Info,
                                            &format!("UI: Showed versions for '{}'", result.name),
                                        );
                                        state.expanded_project_id = Some(result.id.clone());
                                        if !state.modpack_versions.contains_key(&result.id) {
                                            state.loading_versions_for_project = Some(result.id.clone());
                                            app.fetch_modpack_versions(result.id.clone());
                                        }
                                    }
                                }
                            });

                            if state.expanded_project_id.as_deref() == Some(&result.id) {
                                ui.add_space(app.theme.spacing.sm);
                                ui.indent("modpack_versions", |ui| {
                                    if state.loading_versions_for_project.as_deref() == Some(&result.id) {
                                        ui.horizontal(|ui| {
                                            ui.spinner();
                                            ui.label(egui::RichText::new("Loading available versions...").size(14.0));
                                        });
                                    } else if let Some(versions) = state.modpack_versions.get(&result.id) {
                                        if versions.is_empty() {
                                            ui.label(egui::RichText::new("No versions found.").size(14.0));
                                        } else {
                                            ui.horizontal(|ui| {
                                                ui.label(egui::RichText::new("Filter:").strong());
                                                ui.checkbox(&mut state.filter_releases, "Releases");
                                                ui.checkbox(&mut state.filter_betas, "Betas");
                                                ui.checkbox(&mut state.filter_alphas, "Alphas");
                                                ui.add_space(10.0);
                                                ui.add(egui::TextEdit::singleline(&mut state.version_search_query).hint_text("Search version...").desired_width(120.0));
                                            });
                                            ui.add_space(app.theme.spacing.xs);

                                            let search_q = state.version_search_query.to_lowercase();
                                            let filtered_versions: Vec<_> = versions.iter().filter(|ver| {
                                                let type_match = match ver.release_type {
                                                    release_the_launcher_mods::ReleaseType::Release => state.filter_releases,
                                                    release_the_launcher_mods::ReleaseType::Beta => state.filter_betas,
                                                    release_the_launcher_mods::ReleaseType::Alpha => state.filter_alphas,
                                                };
                                                if !type_match {
                                                    return false;
                                                }
                                                if !search_q.is_empty() {
                                                    let matches_ver = ver.version_number.to_lowercase().contains(&search_q);
                                                    let matches_mc = ver.mc_versions.iter().any(|m| m.to_lowercase().contains(&search_q));
                                                    return matches_ver || matches_mc;
                                                }
                                                true
                                            }).collect();

                                            if filtered_versions.is_empty() {
                                                ui.label("No matching versions found.");
                                            } else {
                                                egui::ScrollArea::vertical()
                                                    .max_height(280.0)
                                                    .auto_shrink([false, true])
                                                    .show(ui, |ui| {
                                                        ui.spacing_mut().item_spacing = egui::vec2(20.0, 10.0);
                                                        egui::Grid::new(format!("ver_grid_{}", result.id))
                                                            .striped(true)
                                                            .show(ui, |ui| {
                                                                ui.label(egui::RichText::new("Modpack Version").strong().size(15.0));
                                                                ui.label(egui::RichText::new("Minecraft Version").strong().size(15.0));
                                                                ui.label(egui::RichText::new("Loader").strong().size(15.0));
                                                                ui.label(egui::RichText::new("Type").strong().size(15.0));
                                                                ui.label(egui::RichText::new("Action").strong().size(15.0));
                                                                ui.end_row();

                                                                for ver in filtered_versions {
                                                                    ui.label(egui::RichText::new(&ver.version_number).strong().size(14.0));
                                                                    ui.label(egui::RichText::new(ver.mc_versions.join(", ")).size(13.0));
                                                                    ui.label(egui::RichText::new(ver.loaders.join(", ")).size(13.0));
                                                                    ui.label(egui::RichText::new(ver.release_type.as_str()).size(13.0));
                                                                    if ui.add(egui::Button::new(egui::RichText::new("Install Version").strong().size(13.0)).fill(app.theme.accent)).clicked() {
                                                                        app.log(
                                                                            crate::log::LogLevel::Info,
                                                                            &format!("UI: Installing modpack '{}' version '{}'", result.name, ver.version_number),
                                                                        );
                                                                        state.installing_modpack_id = Some(result.id.clone());
                                                                        state.modrinth_status = format!(
                                                                            "Installing {} ({})...",
                                                                            result.name, ver.version_number
                                                                        );
                                                                        let base_dir = dirs::config_dir()
                                                                            .unwrap_or_else(|| {
                                                                                dirs::home_dir()
                                                                                    .unwrap_or_default()
                                                                                    .join(".config")
                                                                            })
                                                                            .join("release-the-launcher")
                                                                            .join("instances");
                                                                        app.install_modpack_as_instance(
                                                                            result.id.clone(),
                                                                            Some(ver.id.clone()),
                                                                            base_dir,
                                                                        );
                                                                    }
                                                                    ui.end_row();
                                                                }
                                                            });
                                                    });
                                            }
                                        }
                                    }
                                });
                            }
                        }
                    });
                }
            });
    }
}

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
    pub available_versions: Vec<(String, String)>,
    pub version_list_loaded: bool,
    pub version_list_loading: bool,
    pub modpack_versions: std::collections::HashMap<String, Vec<ModVersion>>,
    pub loading_versions_for_project: Option<String>,
    pub expanded_project_id: Option<String>,
    pub filter_releases: bool,
    pub filter_betas: bool,
    pub filter_alphas: bool,
    pub version_search_query: String,
    pub manual_show_releases: bool,
    pub manual_show_snapshots: bool,
    pub manual_show_betas: bool,
    pub manual_show_alphas: bool,
    pub loader_versions: Vec<String>,
    pub loader_versions_loading: bool,
    pub loader_versions_error: Option<String>,
    pub last_fetched_loader_key: Option<(LoaderType, String)>,
}

impl Default for NewInstanceState {
    fn default() -> Self {
        Self {
            name: String::new(),
            mc_version: String::new(),
            loader_type: LoaderType::default(),
            loader_version: String::new(),
            tab: InstanceTab::default(),
            modrinth_query: String::new(),
            mc_version_filter: String::new(),
            loader_filter: LoaderFilter::default(),
            modrinth_status: String::new(),
            modrinth_results: Vec::new(),
            installing_modpack_id: None,
            available_versions: Vec::new(),
            version_list_loaded: false,
            version_list_loading: false,
            modpack_versions: std::collections::HashMap::new(),
            loading_versions_for_project: None,
            expanded_project_id: None,
            filter_releases: true,
            filter_betas: false,
            filter_alphas: false,
            version_search_query: String::new(),
            manual_show_releases: true,
            manual_show_snapshots: false,
            manual_show_betas: false,
            manual_show_alphas: false,
            loader_versions: Vec::new(),
            loader_versions_loading: false,
            loader_versions_error: None,
            last_fetched_loader_key: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum InstanceTab {
    #[default]
    Manual,
    Modrinth,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum LoaderType {
    #[default]
    Vanilla,
    Fabric,
    Forge,
    NeoForge,
    Quilt,
}

impl LoaderType {
    #[must_use]
    pub const fn as_str(&self) -> &str {
        match self {
            Self::Vanilla => "Vanilla",
            Self::Fabric => "Fabric",
            Self::Forge => "Forge",
            Self::NeoForge => "NeoForge",
            Self::Quilt => "Quilt",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum LoaderFilter {
    #[default]
    Any,
    Fabric,
    Forge,
    NeoForge,
    Quilt,
}

impl LoaderFilter {
    #[must_use]
    pub const fn as_str(&self) -> &str {
        match self {
            Self::Any => "Any",
            Self::Fabric => "Fabric",
            Self::Forge => "Forge",
            Self::NeoForge => "NeoForge",
            Self::Quilt => "Quilt",
        }
    }
}
