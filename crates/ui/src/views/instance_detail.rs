use crate::{App, DetailTab, View};

#[derive(Default)]
pub struct DetailTabState {
    pub log_scroll_to_end: bool,
}

pub fn show(
    app: &mut App,
    ui: &mut egui::Ui,
    instance_id: &str,
    tab: DetailTab,
    _state: &mut DetailTabState,
    open_mod_browser: &mut Option<String>,
) {
    let id = instance_id.to_string();

    let instance_data = app.instance_manager.get(&id).map(|instance| {
        (
            instance.settings.name.clone(),
            instance.settings.minecraft_version.clone(),
            instance.settings.loader_name().to_string(),
            instance.root.display().to_string(),
            instance.root.clone(),
        )
    });

    let Some((name, mc_version, loader_name, root_display, root_path)) = instance_data else {
        ui.label("Instance not found.");
        app.current_view = View::InstanceList;
        return;
    };

    ui.heading(&name);
    ui.colored_label(
        app.theme.text_secondary,
        format!("{mc_version} | {loader_name}"),
    );

    ui.add_space(app.theme.spacing.sm);

    let mut action = None;
    ui.horizontal(|ui| {
        if ui
            .add(
                egui::Button::new(format!(" {} Launch", crate::icons::LAUNCH))
                    .fill(app.theme.accent),
            )
            .clicked()
        {
            action = Some("launch");
        }
        if ui
            .button(format!(" {} Open Folder", crate::icons::FOLDER))
            .clicked()
        {
            action = Some("open_folder");
        }
        if ui
            .button(format!(" {} Delete", crate::icons::DELETE))
            .clicked()
        {
            action = Some("delete");
        }
    });

    match action {
        Some("launch") => {
            app.log(
                crate::log::LogLevel::Info,
                &format!("Launching instance: {name}"),
            );
            app.status_message = format!("Launching {name}...");
            app.launch_instance(&id);
        }
        Some("open_folder") => {
            app.log(
                crate::log::LogLevel::Info,
                &format!("UI: Open folder for instance '{name}'"),
            );
            let _ = open::that(&root_path);
        }
        Some("delete") => {
            app.log(
                crate::log::LogLevel::Info,
                &format!("UI: Deleted instance '{name}'"),
            );
            let _ = app.instance_manager.delete(&id);
            app.current_view = View::InstanceList;
            return;
        }
        _ => {}
    }

    let is_vanilla = loader_name == "Vanilla";

    ui.add_space(app.theme.spacing.sm);
    ui.separator();
    ui.add_space(app.theme.spacing.sm);

    show_tabs(app, ui, &id, tab, is_vanilla);
    ui.separator();
    ui.add_space(app.theme.spacing.sm);

    match tab {
        DetailTab::Info => {
            show_info(app, ui, &root_display, &loader_name, &mc_version);
        }
        DetailTab::Logs => {
            show_logs(app, ui, &id, &root_path);
        }
        DetailTab::Mods => {
            if is_vanilla {
                show_info(app, ui, &root_display, &loader_name, &mc_version);
            } else {
                show_mods(app, ui, &root_path, &id, open_mod_browser);
            }
        }
    }
}

fn show_tabs(app: &mut App, ui: &mut egui::Ui, id: &str, tab: DetailTab, is_vanilla: bool) {
    let tabs: Vec<(&str, DetailTab)> = if is_vanilla {
        vec![("Info", DetailTab::Info), ("Logs", DetailTab::Logs)]
    } else {
        vec![
            ("Info", DetailTab::Info),
            ("Logs", DetailTab::Logs),
            ("Mods", DetailTab::Mods),
        ]
    };

    ui.horizontal(|ui| {
        for (label, target_tab) in tabs {
            let style = if tab == target_tab {
                egui::Button::new(label).fill(app.theme.accent)
            } else {
                egui::Button::new(label)
            };
            if ui.add(style).clicked() {
                app.log(
                    crate::log::LogLevel::Info,
                    &format!("UI: Switched to {target_tab:?} tab on instance '{id}'"),
                );
                app.current_view = View::InstanceDetail {
                    id: id.to_string(),
                    tab: target_tab,
                };
            }
        }
    });
}

fn show_info(
    app: &App,
    ui: &mut egui::Ui,
    root_display: &str,
    loader_name: &str,
    mc_version: &str,
) {
    ui.colored_label(app.theme.text_secondary, "Instance folder:");
    ui.monospace(root_display);
    ui.colored_label(app.theme.text_secondary, format!("Loader: {loader_name}"));
    ui.colored_label(app.theme.text_secondary, format!("Minecraft: {mc_version}"));
}

fn show_logs(app: &App, ui: &mut egui::Ui, instance_id: &str, root_path: &std::path::Path) {
    let mc_log_path = root_path.join(".minecraft").join("logs").join("latest.log");
    let alt_log_path = root_path.join("logs").join("latest.log");

    let log_file_path = if mc_log_path.exists() {
        Some(mc_log_path)
    } else if alt_log_path.exists() {
        Some(alt_log_path)
    } else {
        None
    };

    let target_key = format!("instance:{instance_id}");
    let buffer_entries: Vec<_> = app
        .log_buffer
        .entries()
        .into_iter()
        .filter(|e| e.target == target_key || e.target == instance_id)
        .collect();

    let disk_content = log_file_path.and_then(|p| std::fs::read_to_string(p).ok());
    let has_disk_logs = disk_content
        .as_ref()
        .map_or(false, |c| !c.trim().is_empty());
    let has_buffer_logs = !buffer_entries.is_empty();

    if !has_disk_logs && !has_buffer_logs {
        ui.colored_label(
            app.theme.text_secondary,
            "No log entries yet for this instance.",
        );
        return;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            ui.style_mut().override_text_style = Some(egui::TextStyle::Monospace);

            if has_buffer_logs {
                for entry in &buffer_entries {
                    let color = match entry.level {
                        crate::log::LogLevel::Error => app.theme.log_colors.error,
                        crate::log::LogLevel::Warn => app.theme.log_colors.warn,
                        crate::log::LogLevel::Info => app.theme.log_colors.info,
                        crate::log::LogLevel::Debug => app.theme.log_colors.debug,
                        crate::log::LogLevel::Trace => app.theme.log_colors.trace,
                    };
                    let text = format!(
                        "[{}] [{}] {}",
                        entry.timestamp,
                        entry.level.as_str(),
                        entry.message
                    );
                    ui.colored_label(color, text);
                }
            }

            if let Some(content) = disk_content {
                for line in content.lines() {
                    let color = if line.contains("/ERROR")
                        || line.contains("ERROR")
                        || line.contains("FATAL")
                    {
                        app.theme.log_colors.error
                    } else if line.contains("/WARN")
                        || line.contains("WARN")
                        || line.contains("WARNING")
                    {
                        app.theme.log_colors.warn
                    } else if line.contains("/INFO") || line.contains("INFO") {
                        app.theme.log_colors.info
                    } else if line.contains("/DEBUG") || line.contains("DEBUG") {
                        app.theme.log_colors.debug
                    } else {
                        app.theme.text_secondary
                    };
                    ui.colored_label(color, line);
                }
            }
        });
}

fn show_mods(
    app: &App,
    ui: &mut egui::Ui,
    root_path: &std::path::Path,
    id: &str,
    open_mod_browser: &mut Option<String>,
) {
    ui.label("Installed mods:");
    let mods_dir = root_path.join(".minecraft").join("mods");
    let mods = release_the_launcher_mods::list_mods(&mods_dir);

    if mods.is_empty() {
        ui.colored_label(app.theme.text_secondary, "No mods installed.");
    } else {
        let mut toggle_path = None;
        for m in &mods {
            let mut enabled = m.enabled;
            if ui.checkbox(&mut enabled, &m.name).changed() {
                toggle_path = Some(m.path.clone());
            }
        }
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
    ui.separator();
    if ui.button("Browse Mods (Modrinth)").clicked() {
        app.log(
            crate::log::LogLevel::Info,
            &format!("UI: Opened Mod Browser for instance '{id}'"),
        );
        *open_mod_browser = Some(id.to_string());
    }
}
