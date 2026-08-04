use super::{DetailTabState, ModFilter, ModUpdatesState};
use crate::{widgets, LauncherApp, View};

pub fn show_mods(
    app: &mut LauncherApp,
    ui: &mut egui::Ui,
    id: &str,
    tab_state: &mut DetailTabState,
) {
    show_mods_toolbar(app, ui, id, tab_state);
    show_mod_updates(app, ui, id, tab_state);
    show_mod_search_filter(app, ui, tab_state);
    ui.add_space(app.theme.spacing.xs);
    ui.separator();
    ui.add_space(app.theme.spacing.xs);
    show_mod_list(app, ui, id, tab_state);
}

fn show_mods_toolbar(
    app: &mut LauncherApp,
    ui: &mut egui::Ui,
    id: &str,
    tab_state: &mut DetailTabState,
) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Installed mods:").strong());
        widgets::right_aligned(ui, |ui| {
            if ui
                .add(widgets::icon_button(crate::icons::ADD, "Add mods"))
                .clicked()
            {
                app.coordinator.log(
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
                    "Check for Updates",
                ))
                .clicked()
            {
                app.coordinator.log(
                    crate::log::LogLevel::Info,
                    &format!("UI: Checking mod updates for instance '{id}'"),
                );
                tab_state.mod_updates = ModUpdatesState::Checking;
                app.coordinator.check_mod_updates(id.to_string());
            }
        });
    });
    ui.add_space(app.theme.spacing.xs);
}

fn show_mod_updates(
    app: &mut LauncherApp,
    ui: &mut egui::Ui,
    id: &str,
    tab_state: &DetailTabState,
) {
    match &tab_state.mod_updates {
        ModUpdatesState::Checking => {
            ui.colored_label(app.theme.text_secondary, "Checking for updates...");
            ui.add_space(app.theme.spacing.xs);
        }
        ModUpdatesState::Loaded(updates) if updates.is_empty() => {
            ui.colored_label(app.theme.text_secondary, "All mods are up to date.");
            ui.add_space(app.theme.spacing.xs);
        }
        ModUpdatesState::Loaded(updates) => {
            ui.collapsing(format!("Available updates ({})", updates.len()), |ui| {
                for update in updates {
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "{} → {}",
                            update.latest.name, update.latest.version_number
                        ));
                        if ui.button("Update").clicked() {
                            if let (Some(mods_dir), Some(summary)) = (
                                app.coordinator.instance_mods_dir(id),
                                app.coordinator.instance_summary(id),
                            ) {
                                app.coordinator.install_mod(
                                    update.latest.project_id.clone(),
                                    mods_dir,
                                    Some(summary.mc_version),
                                    Some(summary.loader_name),
                                );
                            }
                        }
                    });
                }
            });
            ui.add_space(app.theme.spacing.xs);
        }
        ModUpdatesState::Idle => {}
    }
}

fn show_mod_search_filter(
    app: &mut LauncherApp,
    ui: &mut egui::Ui,
    tab_state: &mut DetailTabState,
) {
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
            app.coordinator.log(
                crate::log::LogLevel::Info,
                &format!("UI: Mod filter changed to {filter:?}"),
            );
        }
    });
}

use super::CachedModEntry;

fn show_mod_list(
    app: &mut LauncherApp,
    ui: &mut egui::Ui,
    id: &str,
    tab_state: &mut DetailTabState,
) {
    let cache = &mut tab_state.mods_cache;

    // Refresh cache if instance changed or cache marked dirty
    if cache.instance_id != id || cache.dirty || cache.mods.is_empty() {
        let mods = app.coordinator.list_instance_mods(id);

        cache.mods = mods
            .into_iter()
            .map(|m| CachedModEntry {
                name: m.name,
                path: m.path,
                enabled: m.enabled,
                details: m.details,
            })
            .collect();
        cache.instance_id = id.to_string();
        cache.dirty = false;
    }

    if cache.mods.is_empty() {
        ui.colored_label(app.theme.text_secondary, "No mods installed.");
        return;
    }

    let query = tab_state.mod_search_query.trim().to_lowercase();
    let filter = tab_state.mod_filter;

    let matching_indices: Vec<usize> = cache
        .mods
        .iter()
        .enumerate()
        .filter(|(_, m)| match filter {
            ModFilter::EnabledOnly if !m.enabled => false,
            ModFilter::DisabledOnly if m.enabled => false,
            _ => query.is_empty() || m.name.to_lowercase().contains(&query),
        })
        .map(|(i, _)| i)
        .collect();

    if matching_indices.is_empty() {
        ui.colored_label(app.theme.text_secondary, "No mods match search or filters.");
        return;
    }

    let mut toggle_idx = None;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for &idx in &matching_indices {
                let entry = &cache.mods[idx];
                ui.horizontal(|ui| {
                    let mut enabled = entry.enabled;
                    if ui.checkbox(&mut enabled, &entry.name).changed() {
                        toggle_idx = Some(idx);
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
                        ui.colored_label(app.theme.text_secondary, "(no metadata)");
                    }

                    if !entry.enabled {
                        ui.colored_label(app.theme.text_secondary, "(Disabled)");
                    }
                });
            }
        });

    if let Some(idx) = toggle_idx {
        let entry = &mut cache.mods[idx];
        let action = if entry.enabled { "disabled" } else { "enabled" };
        app.coordinator.log(
            crate::log::LogLevel::Info,
            &format!("UI: Mod '{}' {action}", entry.name),
        );
        let result = app.coordinator.toggle_mod(&entry.path);
        if let Err(e) = result {
            app.coordinator.log(
                crate::log::LogLevel::Error,
                &format!("Failed to toggle mod: {e}"),
            );
        } else {
            cache.dirty = true;
        }
    }
}
