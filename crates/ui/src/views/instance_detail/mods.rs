use super::DetailTabState;
use crate::{widgets, App, View};

pub fn show_mods(
    app: &mut App,
    ui: &mut egui::Ui,
    _root_path: &std::path::Path,
    id: &str,
    tab_state: &mut DetailTabState,
) {
    for msg in app.drain_ui_queue() {
        if let crate::UiMessage::ModUpdatesResult {
            instance_id: target_id,
            updates,
        } = msg
        {
            if target_id == id {
                tab_state.checking_mod_updates = false;
                tab_state.mod_updates = Some(updates);
            }
        }
    }

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Installed mods:").strong());
        widgets::right_aligned(ui, |ui| {
            if ui
                .add(widgets::icon_button(crate::icons::ADD, "Add mods"))
                .clicked()
            {
                app.log(
                    crate::log::LogLevel::Info,
                    &format!("UI: Opened Mod Browser for instance '{id}'"),
                );
                app.current_view = View::ModBrowser {
                    instance_id: id.to_string(),
                };
            }
            if ui
                .add(widgets::icon_button(
                    crate::icons::SEARCH,
                    "Buscar actualizaciones",
                ))
                .clicked()
            {
                app.log(
                    crate::log::LogLevel::Info,
                    &format!("UI: Checking mod updates for instance '{id}'"),
                );
                tab_state.checking_mod_updates = true;
                app.check_mod_updates(id.to_string());
            }
        });
    });

    ui.add_space(app.theme.spacing.xs);

    if tab_state.checking_mod_updates {
        ui.colored_label(app.theme.text_secondary, "Buscando actualizaciones...");
        ui.add_space(app.theme.spacing.xs);
    } else if let Some(updates) = &tab_state.mod_updates {
        if updates.is_empty() {
            ui.colored_label(app.theme.text_secondary, "Todos los mods están actualizados.");
        } else {
            ui.collapsing(
                format!("Actualizaciones disponibles ({})", updates.len()),
                |ui| {
                    for update in updates {
                        ui.horizontal(|ui| {
                            ui.label(format!(
                                "{} → {}",
                                update.latest.name, update.latest.version_number
                            ));
                            if ui.button("Actualizar").clicked() {
                                if let Some(inst) =
                                    app.coordinator.instance_manager.get(&id.to_string())
                                {
                                    let mods_dir = inst.root.join("mods");
                                    let mc_ver = Some(inst.settings.minecraft_version.clone());
                                    let loader = Some(inst.settings.loader_name().to_string());
                                    app.install_mod_from_modrinth(
                                        update.latest.project_id.clone(),
                                        mods_dir,
                                        mc_ver,
                                        loader,
                                    );
                                }
                            }
                        });
                    }
                },
            );
        }
        ui.add_space(app.theme.spacing.xs);
    }

    ui.horizontal(|ui| {
        ui.label("Search:");
        ui.add(
            egui::TextEdit::singleline(&mut tab_state.mod_search_query)
                .hint_text("Filter by mod name...")
                .desired_width(180.0),
        );

        ui.add_space(app.theme.spacing.sm);
        ui.label("Filter:");
        ui.checkbox(&mut tab_state.show_enabled_mods, "Active");
        ui.checkbox(&mut tab_state.show_disabled_mods, "Inactive");
    });

    ui.add_space(app.theme.spacing.xs);
    ui.separator();
    ui.add_space(app.theme.spacing.xs);

    let mc_mods_dir = _root_path.join(".minecraft").join("mods");
    let mods_dir = if mc_mods_dir.exists() {
        mc_mods_dir
    } else {
        _root_path.join("mods")
    };
    let mods = release_the_launcher_mods::list_mods(&mods_dir);
    let metadata_list = app.coordinator.mods_metadata(id);

    let query = tab_state.mod_search_query.trim().to_lowercase();
    let filtered_mods: Vec<_> = mods
        .iter()
        .filter(|m| {
            if m.enabled && !tab_state.show_enabled_mods {
                return false;
            }
            if !m.enabled && !tab_state.show_disabled_mods {
                return false;
            }
            if !query.is_empty() && !m.name.to_lowercase().contains(&query) {
                return false;
            }
            true
        })
        .collect();

    if mods.is_empty() {
        ui.colored_label(app.theme.text_secondary, "No mods installed.");
    } else if filtered_mods.is_empty() {
        ui.colored_label(app.theme.text_secondary, "No mods match search or filters.");
    } else {
        let mut toggle_path = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for m in filtered_mods {
                    ui.horizontal(|ui| {
                        let mut enabled = m.enabled;
                        if ui.checkbox(&mut enabled, &m.name).changed() {
                            toggle_path = Some(m.path.clone());
                        }

                        let meta = metadata_list
                            .iter()
                            .find(|d| {
                                d.mod_id.eq_ignore_ascii_case(&m.name)
                                    || m.name.to_lowercase().contains(&d.mod_id.to_lowercase())
                                    || m.name.to_lowercase().contains(&d.name.to_lowercase())
                            })
                            .cloned()
                            .or_else(|| {
                                release_the_launcher_mods::parser::parse_mod_metadata(&m.path).ok()
                            });

                        if let Some(details) = meta {
                            ui.label(format!("- {} (v{})", details.name, details.version));
                            if !details.description.is_empty() {
                                ui.colored_label(
                                    app.theme.text_secondary,
                                    format!(": {}", details.description),
                                );
                            }
                        } else {
                            ui.colored_label(app.theme.text_secondary, "(sin metadata)");
                        }

                        if !m.enabled {
                            ui.colored_label(app.theme.text_secondary, "(Disabled)");
                        }
                    });
                }
            });

        if let Some(path) = toggle_path {
            if let Some(entry) = mods.iter().find(|m| m.path == path) {
                let action = if entry.enabled { "disabled" } else { "enabled" };
                app.log(
                    crate::log::LogLevel::Info,
                    &format!("UI: Mod '{}' {action}", entry.name),
                );
                let result = if entry.enabled {
                    release_the_launcher_mods::disable_mod(&path)
                } else {
                    release_the_launcher_mods::enable_mod(&path)
                };
                if let Err(e) = result {
                    app.log(
                        crate::log::LogLevel::Error,
                        &format!("Failed to toggle mod: {e}"),
                    );
                }
            }
        }
    }
}
