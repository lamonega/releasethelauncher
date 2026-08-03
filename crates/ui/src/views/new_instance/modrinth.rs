use super::{LoaderFilter, NewInstanceState};
use crate::App;
use egui_extras::{Column, TableBuilder};
use release_the_launcher_mods::{ModVersion, ProjectSummary};

pub(crate) fn show_modrinth(app: &App, ui: &mut egui::Ui, state: &mut NewInstanceState) {
    ui.label("Search for modpacks on Modrinth:");

    ui.add_space(app.theme.spacing.sm);

    ui.horizontal(|ui| {
        ui.label("Search:");
        ui.text_edit_singleline(&mut state.modrinth_query);
    });

    ui.add_space(app.theme.spacing.sm);

    ui.label("MC Version:");
    if state.available_versions.is_empty() {
        ui.text_edit_singleline(&mut state.mc_version_filter);
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
        egui::ComboBox::from_id_source("modrinth_mc_version_filter")
            .selected_text(if state.mc_version_filter.is_empty() {
                "Any".to_string()
            } else {
                state.mc_version_filter.clone()
            })
            .width(220.0)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(state.mc_version_filter.is_empty(), "Any")
                    .clicked()
                {
                    state.mc_version_filter.clear();
                    app.log(
                        crate::log::LogLevel::Info,
                        "UI: Modrinth MC version filter changed to Any",
                    );
                }
                for (version, _) in &filtered_versions {
                    if ui
                        .selectable_label(state.mc_version_filter == *version, version)
                        .clicked()
                    {
                        state.mc_version_filter.clone_from(version);
                        app.log(
                            crate::log::LogLevel::Info,
                            &format!("UI: Modrinth MC version filter changed to {version}"),
                        );
                    }
                }
            });
    }

    if !state.available_versions.is_empty() {
        ui.add_space(app.theme.spacing.xs);
        ui.horizontal(|ui| {
            if ui
                .checkbox(&mut state.manual_show_types[0], "Releases")
                .changed()
                || ui
                    .checkbox(&mut state.manual_show_types[1], "Snapshots")
                    .changed()
                || ui
                    .checkbox(&mut state.manual_show_types[2], "Old Betas")
                    .changed()
                || ui
                    .checkbox(&mut state.manual_show_types[3], "Old Alphas")
                    .changed()
            {
                app.log(
                    crate::log::LogLevel::Info,
                    "UI: Modrinth MC version type filter changed",
                );
            }
        });
    }

    ui.add_space(app.theme.spacing.sm);

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

    ui.add_space(app.theme.spacing.sm);

    if ui
        .add(crate::widgets::icon_button(crate::icons::SEARCH, "Search"))
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

    show_modrinth_results(app, ui, state);
}

fn show_modrinth_results(app: &App, ui: &mut egui::Ui, state: &mut NewInstanceState) {
    if !state.modrinth_status.is_empty() {
        ui.colored_label(app.theme.text_secondary, &state.modrinth_status);
        ui.add_space(app.theme.spacing.sm);
    }

    if state.modrinth_results.is_empty() && state.modrinth_status.is_empty() {
        crate::empty_state(ui, &app.theme, &["Search for modpacks on Modrinth."]);
        return;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let results = state.modrinth_results.clone();
            for result in &results {
                show_modrinth_result_card(app, ui, state, result);
            }
        });
}

fn show_modrinth_result_card(
    app: &App,
    ui: &mut egui::Ui,
    state: &mut NewInstanceState,
    result: &ProjectSummary,
) {
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
        show_modrinth_result_actions(app, ui, state, result);
    });
}

fn show_modrinth_result_actions(
    app: &App,
    ui: &mut egui::Ui,
    state: &mut NewInstanceState,
    result: &ProjectSummary,
) {
    ui.horizontal(|ui| {
        if ui
            .add(crate::widgets::icon_button(
                crate::icons::ADD,
                "Install Latest",
            ))
            .clicked()
        {
            app.log(
                crate::log::LogLevel::Info,
                &format!("UI: Installing modpack '{}' (latest)", result.name),
            );
            state.installing_modpack_id = Some(result.id.clone());
            state.modrinth_status = format!("Installing {}...", result.name);
            app.install_modpack_as_instance(result.id.clone(), None);
        }

        let is_expanded = state.expanded_project_id.as_deref() == Some(&result.id);
        let expand_text = if is_expanded {
            "Hide Versions"
        } else {
            "Choose Version"
        };

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
        let versions = state.modpack_versions.get(&result.id).cloned();
        let is_loading = state.loading_versions_for_project.as_deref() == Some(&result.id);
        ui.indent("modpack_versions", |ui| {
            if is_loading {
                crate::widgets::loading_row(ui, "Loading available versions...");
            } else if let Some(v) = versions {
                show_modrinth_version_list(app, ui, state, result, &v);
            }
        });
    }
}

fn show_modrinth_version_list(
    app: &App,
    ui: &mut egui::Ui,
    state: &mut NewInstanceState,
    result: &ProjectSummary,
    versions: &[ModVersion],
) {
    if versions.is_empty() {
        ui.label(egui::RichText::new("No versions found.").size(14.0));
        return;
    }

    show_version_filter(ui, state);
    ui.add_space(app.theme.spacing.xs);

    let filtered = filter_versions(versions, state);

    if filtered.is_empty() {
        ui.label("No matching versions found.");
        return;
    }

    let install_target = show_version_table(app, ui, &filtered);

    if let Some((ver_number, ver_id)) = install_target {
        app.log(
            crate::log::LogLevel::Info,
            &format!(
                "UI: Installing modpack '{}' version '{}'",
                result.name, ver_number
            ),
        );
        state.installing_modpack_id = Some(result.id.clone());
        state.modrinth_status = format!("Installing {} ({})...", result.name, ver_number);
        app.install_modpack_as_instance(result.id.clone(), Some(ver_id));
    }
}

fn show_version_filter(ui: &mut egui::Ui, state: &mut NewInstanceState) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Filter:").strong());
        ui.checkbox(&mut state.filter_types[0], "Releases");
        ui.checkbox(&mut state.filter_types[1], "Betas");
        ui.checkbox(&mut state.filter_types[2], "Alphas");
        ui.add_space(10.0);
        ui.add(
            egui::TextEdit::singleline(&mut state.version_search_query)
                .hint_text("Search version...")
                .desired_width(120.0),
        );
    });
}

fn filter_versions<'a>(
    versions: &'a [ModVersion],
    state: &NewInstanceState,
) -> Vec<&'a ModVersion> {
    let search_q = state.version_search_query.to_lowercase();
    versions
        .iter()
        .filter(|ver| {
            let type_match = match ver.release_type {
                release_the_launcher_mods::ReleaseType::Release => state.filter_types[0],
                release_the_launcher_mods::ReleaseType::Beta => state.filter_types[1],
                release_the_launcher_mods::ReleaseType::Alpha => state.filter_types[2],
            };
            if !type_match {
                return false;
            }
            if search_q.is_empty() {
                return true;
            }
            let matches_ver = ver.version_number.to_lowercase().contains(&search_q);
            let matches_mc = ver
                .mc_versions
                .iter()
                .any(|m| m.to_lowercase().contains(&search_q));
            matches_ver || matches_mc
        })
        .collect()
}

fn show_version_table(
    app: &App,
    ui: &mut egui::Ui,
    filtered_versions: &[&ModVersion],
) -> Option<(String, String)> {
    let mut install_target = None;

    TableBuilder::new(ui)
        .striped(true)
        .max_scroll_height(280.0)
        .column(Column::initial(140.0).resizable(true))
        .column(Column::initial(140.0).resizable(true))
        .column(Column::initial(100.0).resizable(true))
        .column(Column::initial(80.0).resizable(true))
        .column(Column::remainder())
        .header(22.0, |mut header| {
            header.col(|ui| {
                ui.label(egui::RichText::new("Modpack Version").strong().size(15.0));
            });
            header.col(|ui| {
                ui.label(egui::RichText::new("Minecraft Version").strong().size(15.0));
            });
            header.col(|ui| {
                ui.label(egui::RichText::new("Loader").strong().size(15.0));
            });
            header.col(|ui| {
                ui.label(egui::RichText::new("Type").strong().size(15.0));
            });
            header.col(|ui| {
                ui.label(egui::RichText::new("Action").strong().size(15.0));
            });
        })
        .body(|mut body| {
            for ver in filtered_versions {
                body.row(24.0, |mut row| {
                    row.col(|ui| {
                        ui.label(egui::RichText::new(&ver.version_number).strong().size(14.0));
                    });
                    row.col(|ui| {
                        ui.label(egui::RichText::new(ver.mc_versions.join(", ")).size(13.0));
                    });
                    row.col(|ui| {
                        ui.label(egui::RichText::new(ver.loaders.join(", ")).size(13.0));
                    });
                    row.col(|ui| {
                        ui.label(egui::RichText::new(ver.release_type.as_str()).size(13.0));
                    });
                    row.col(|ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("Install Version").strong().size(13.0),
                                )
                                .fill(app.theme.accent),
                            )
                            .clicked()
                        {
                            install_target = Some((ver.version_number.clone(), ver.id.clone()));
                        }
                    });
                });
            }
        });

    install_target
}
