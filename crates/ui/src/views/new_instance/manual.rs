use super::{LoaderType, NewInstanceState, VersionListState};
use crate::LauncherApp;
use crate::View;

pub(crate) fn show_manual(app: &mut LauncherApp, ui: &mut egui::Ui, state: &mut NewInstanceState) {
    ui.label("Name:");
    ui.text_edit_singleline(&mut state.name);

    ui.add_space(app.theme.spacing.sm);

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Filter Minecraft Versions:").strong());
        if ui
            .checkbox(&mut state.manual_show_types[0], "Releases")
            .changed()
        {
            app.coordinator.log(
                crate::log::LogLevel::Info,
                &format!(
                    "UI: MC version filter - Releases: {}",
                    state.manual_show_types[0]
                ),
            );
        }
        if ui
            .checkbox(&mut state.manual_show_types[1], "Snapshots")
            .changed()
        {
            app.coordinator.log(
                crate::log::LogLevel::Info,
                &format!(
                    "UI: MC version filter - Snapshots: {}",
                    state.manual_show_types[1]
                ),
            );
        }
        if ui
            .checkbox(&mut state.manual_show_types[2], "Old Betas")
            .changed()
        {
            app.coordinator.log(
                crate::log::LogLevel::Info,
                &format!(
                    "UI: MC version filter - Betas: {}",
                    state.manual_show_types[2]
                ),
            );
        }
        if ui
            .checkbox(&mut state.manual_show_types[3], "Old Alphas")
            .changed()
        {
            app.coordinator.log(
                crate::log::LogLevel::Info,
                &format!(
                    "UI: MC version filter - Alphas: {}",
                    state.manual_show_types[3]
                ),
            );
        }
    });
    ui.add_space(app.theme.spacing.xs);

    show_manual_version(app, ui, state);

    ui.add_space(app.theme.spacing.sm);

    ui.label("Loader:");
    egui::ComboBox::from_label("Mod Loader")
        .selected_text(state.loader_type.as_str())
        .show_ui(ui, |ui| {
            if ui
                .selectable_value(&mut state.loader_type, LoaderType::Vanilla, "Vanilla")
                .changed()
            {
                app.coordinator
                    .log(crate::log::LogLevel::Info, "UI: Loader changed to Vanilla");
            }
            if ui
                .selectable_value(&mut state.loader_type, LoaderType::Fabric, "Fabric")
                .changed()
            {
                app.coordinator
                    .log(crate::log::LogLevel::Info, "UI: Loader changed to Fabric");
            }
            if ui
                .selectable_value(&mut state.loader_type, LoaderType::Forge, "Forge")
                .changed()
            {
                app.coordinator
                    .log(crate::log::LogLevel::Info, "UI: Loader changed to Forge");
            }
            if ui
                .selectable_value(&mut state.loader_type, LoaderType::NeoForge, "NeoForge")
                .changed()
            {
                app.coordinator
                    .log(crate::log::LogLevel::Info, "UI: Loader changed to NeoForge");
            }
            if ui
                .selectable_value(&mut state.loader_type, LoaderType::Quilt, "Quilt")
                .changed()
            {
                app.coordinator
                    .log(crate::log::LogLevel::Info, "UI: Loader changed to Quilt");
            }
        });

    show_manual_loader(app, ui, state);

    ui.add_space(app.theme.spacing.sm);
    ui.separator();
    ui.add_space(app.theme.spacing.sm);

    show_manual_create(app, ui, state);
}

fn show_manual_version(app: &LauncherApp, ui: &mut egui::Ui, state: &mut NewInstanceState) {
    ui.label("Minecraft Version:");
    if state.available_versions.is_empty() {
        if state.version_list_state == VersionListState::Loading {
            crate::widgets::loading_row(ui, "Loading versions...");
        } else {
            ui.text_edit_singleline(&mut state.mc_version);
        }
    } else {
        let filtered_versions: Vec<&(String, String)> = state
            .available_versions
            .iter()
            .filter(|(_, ver_type)| match ver_type.as_str() {
                "release" => state.manual_show_types[0],
                "snapshot" => state.manual_show_types[1],
                "old_beta" => state.manual_show_types[2],
                "old_alpha" => state.manual_show_types[3],
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
                        app.coordinator.log(
                            crate::log::LogLevel::Info,
                            &format!("UI: MC version selected: {}", state.mc_version),
                        );
                    }
                }
            });
    }
}

fn show_manual_loader(app: &LauncherApp, ui: &mut egui::Ui, state: &mut NewInstanceState) {
    if state.loader_type != LoaderType::Vanilla && !state.mc_version.is_empty() {
        let current_key = (state.loader_type, state.mc_version.clone());
        if state.last_fetched_loader_key.as_ref() != Some(&current_key) {
            state.loader_versions_loading = true;
            state.loader_versions_error = None;
            state.loader_versions.clear();
            state.loader_version.clear();
            state.last_fetched_loader_key = Some(current_key);
            app.coordinator
                .fetch_loader_versions(state.loader_type.as_str(), &state.mc_version);
        }
    }

    if state.loader_type != LoaderType::Vanilla {
        ui.add_space(app.theme.spacing.sm);
        ui.label("Loader Version:");

        if state.loader_versions_loading {
            crate::widgets::loading_row(ui, "Loading loader versions...");
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
                            app.coordinator.log(
                                crate::log::LogLevel::Info,
                                &format!("UI: Loader version selected: {ver}"),
                            );
                        }
                    }
                });
        } else {
            if let Some(err) = &state.loader_versions_error {
                ui.colored_label(
                    app.theme.log_colors.warn,
                    format!("Could not fetch versions: {err}"),
                );
            }
            ui.text_edit_singleline(&mut state.loader_version);
        }
    }
}

fn show_manual_create(app: &mut LauncherApp, ui: &mut egui::Ui, state: &NewInstanceState) {
    if ui
        .add(
            crate::widgets::icon_button(crate::icons::ADD, "Create Instance")
                .fill(app.theme.accent),
        )
        .clicked()
        && !state.name.is_empty()
        && !state.mc_version.is_empty()
    {
        if state.loader_type != LoaderType::Vanilla && state.loader_version.trim().is_empty() {
            app.status_message =
                "Wait for loader versions to load or enter one manually.".to_string();
            return;
        }
        let loader = state
            .loader_type
            .into_mod_loader(state.loader_version.clone());

        match app.coordinator.create_instance(
            &state.name,
            state.mc_version.clone(),
            loader,
            None,
            None,
        ) {
            Ok(_) => {
                app.coordinator.log(
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
                app.coordinator.log(
                    crate::log::LogLevel::Error,
                    &format!("Failed to create instance: {e}"),
                );
                app.status_message = format!("Error: {e}");
            }
        }
    }
}
