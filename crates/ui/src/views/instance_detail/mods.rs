use super::{DetailTabState, ModFilter, ModUpdatesState};
use crate::{widgets, App, View};

pub fn show_mods(
    app: &mut App,
    ui: &mut egui::Ui,
    root_path: &std::path::Path,
    id: &str,
    tab_state: &mut DetailTabState,
) {
    handle_mod_messages(app, id, tab_state);
    show_mods_toolbar(app, ui, id, tab_state);
    show_mod_updates(app, ui, id, tab_state);
    show_mod_search_filter(app, ui, tab_state);
    ui.add_space(app.theme.spacing.xs);
    ui.separator();
    ui.add_space(app.theme.spacing.xs);
    show_mod_list(app, ui, root_path, id, tab_state);
}

fn handle_mod_messages(app: &mut App, id: &str, tab_state: &mut DetailTabState) {
    for msg in app.drain_ui_queue() {
        if let crate::UiMessage::ModUpdatesResult {
            instance_id: target_id,
            updates,
        } = msg
        {
            if target_id == id {
                tab_state.mod_updates = ModUpdatesState::Loaded(updates);
            }
        }
    }
}

fn show_mods_toolbar(app: &mut App, ui: &mut egui::Ui, id: &str, tab_state: &mut DetailTabState) {
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
                tab_state.mod_updates = ModUpdatesState::Checking;
                app.check_mod_updates(id.to_string());
            }
        });
    });
    ui.add_space(app.theme.spacing.xs);
}

fn show_mod_updates(app: &mut App, ui: &mut egui::Ui, id: &str, tab_state: &DetailTabState) {
    match &tab_state.mod_updates {
        ModUpdatesState::Checking => {
            ui.colored_label(app.theme.text_secondary, "Buscando actualizaciones...");
            ui.add_space(app.theme.spacing.xs);
        }
        ModUpdatesState::Loaded(updates) if updates.is_empty() => {
            ui.colored_label(
                app.theme.text_secondary,
                "Todos los mods están actualizados.",
            );
            ui.add_space(app.theme.spacing.xs);
        }
        ModUpdatesState::Loaded(updates) => {
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
            ui.add_space(app.theme.spacing.xs);
        }
        ModUpdatesState::Idle => {}
    }
}

fn show_mod_search_filter(app: &mut App, ui: &mut egui::Ui, tab_state: &mut DetailTabState) {
    ui.horizontal(|ui| {
        ui.label("Search:");
        ui.add(
            egui::TextEdit::singleline(&mut tab_state.mod_search_query)
                .hint_text("Filter by mod name...")
                .desired_width(180.0),
        );

        ui.add_space(app.theme.spacing.sm);
        ui.label("Filter:");
        let filter = &mut tab_state.mod_filter;
        if ui.radio_value(filter, ModFilter::All, "All").changed()
            || ui
                .radio_value(filter, ModFilter::EnabledOnly, "Active")
                .changed()
            || ui
                .radio_value(filter, ModFilter::DisabledOnly, "Inactive")
                .changed()
        {
            app.log(
                crate::log::LogLevel::Info,
                &format!("UI: Mod filter changed to {filter:?}"),
            );
        }
    });
}

struct ModEntry {
    name: String,
    path: std::path::PathBuf,
    enabled: bool,
    details: Option<release_the_launcher_mods::ModDetails>,
}

fn filter_mods(
    mods: &[release_the_launcher_mods::ModEntry],
    metadata_list: &[release_the_launcher_mods::ModDetails],
    tab_state: &DetailTabState,
) -> Vec<ModEntry> {
    let query = tab_state.mod_search_query.trim().to_lowercase();
    mods.iter()
        .filter(|m| match tab_state.mod_filter {
            ModFilter::EnabledOnly if !m.enabled => false,
            ModFilter::DisabledOnly if m.enabled => false,
            ModFilter::None => false,
            _ => query.is_empty() || m.name.to_lowercase().contains(&query),
        })
        .map(|m| {
            let details = metadata_list
                .iter()
                .find(|d| {
                    d.mod_id.eq_ignore_ascii_case(&m.name)
                        || m.name.to_lowercase().contains(&d.mod_id.to_lowercase())
                        || m.name.to_lowercase().contains(&d.name.to_lowercase())
                })
                .cloned()
                .or_else(|| release_the_launcher_mods::parser::parse_mod_metadata(&m.path).ok());
            ModEntry {
                name: m.name.clone(),
                path: m.path.clone(),
                enabled: m.enabled,
                details,
            }
        })
        .collect()
}

fn show_mod_list(
    app: &mut App,
    ui: &mut egui::Ui,
    root_path: &std::path::Path,
    id: &str,
    tab_state: &DetailTabState,
) {
    let mc_mods_dir = root_path.join(".minecraft").join("mods");
    let mods_dir = if mc_mods_dir.exists() {
        mc_mods_dir
    } else {
        root_path.join("mods")
    };
    let mods = release_the_launcher_mods::list_mods(&mods_dir);
    let metadata_list = app.coordinator.mods_metadata(id);

    if mods.is_empty() {
        ui.colored_label(app.theme.text_secondary, "No mods installed.");
        return;
    }

    let entries = filter_mods(&mods, &metadata_list, tab_state);

    if entries.is_empty() {
        ui.colored_label(app.theme.text_secondary, "No mods match search or filters.");
        return;
    }

    let mut toggle_path = None;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for entry in &entries {
                ui.horizontal(|ui| {
                    let mut enabled = entry.enabled;
                    if ui.checkbox(&mut enabled, &entry.name).changed() {
                        toggle_path = Some(entry.path.clone());
                    }

                    if let Some(details) = &entry.details {
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

                    if !entry.enabled {
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
