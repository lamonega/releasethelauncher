use crate::{App, DetailTab, View};

pub struct DetailTabState {
    pub log_scroll_to_end: bool,
    pub mod_search_query: String,
    pub show_enabled_mods: bool,
    pub show_disabled_mods: bool,
    pub config_instance_id: String,
    pub config_java_path: String,
    pub config_memory_min: String,
    pub config_memory_max: String,
}

impl Default for DetailTabState {
    fn default() -> Self {
        Self {
            log_scroll_to_end: false,
            mod_search_query: String::new(),
            show_enabled_mods: true,
            show_disabled_mods: true,
            config_instance_id: String::new(),
            config_java_path: String::new(),
            config_memory_min: String::new(),
            config_memory_max: String::new(),
        }
    }
}

pub fn show(
    app: &mut App,
    ui: &mut egui::Ui,
    instance_id: &str,
    tab: DetailTab,
    tab_state: &mut DetailTabState,
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
            instance.settings.java.clone(),
        )
    });

    let Some((name, mc_version, loader_name, root_display, root_path, java_settings)) = instance_data else {
        ui.label("Instance not found.");
        app.current_view = View::InstanceList;
        return;
    };

    if tab_state.config_instance_id != id {
        tab_state.config_instance_id = id.clone();
        tab_state.config_java_path = java_settings.path.clone().unwrap_or_default();
        tab_state.config_memory_min = java_settings.memory_min.clone().unwrap_or_default();
        tab_state.config_memory_max = java_settings.memory_max.clone().unwrap_or_default();
    }

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
            show_info(app, ui, &root_display, &loader_name, &mc_version, &java_settings);
        }
        DetailTab::Config => {
            show_config(app, ui, &id, tab_state);
        }
        DetailTab::Logs => {
            show_logs(app, ui, &id, &root_path);
        }
        DetailTab::Mods => {
            if is_vanilla {
                show_info(app, ui, &root_display, &loader_name, &mc_version, &java_settings);
            } else {
                show_mods(app, ui, &root_path, &id, tab_state, open_mod_browser);
            }
        }
    }
}

fn show_tabs(app: &mut App, ui: &mut egui::Ui, id: &str, tab: DetailTab, is_vanilla: bool) {
    let tabs: Vec<(&str, DetailTab)> = if is_vanilla {
        vec![
            ("Info", DetailTab::Info),
            ("Config", DetailTab::Config),
            ("Logs", DetailTab::Logs),
        ]
    } else {
        vec![
            ("Info", DetailTab::Info),
            ("Config", DetailTab::Config),
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
    java_settings: &release_the_launcher_core::JavaSettings,
) {
    ui.colored_label(app.theme.text_secondary, "Instance folder:");
    ui.monospace(root_display);

    ui.add_space(app.theme.spacing.xs);
    ui.colored_label(app.theme.text_secondary, format!("Loader: {loader_name}"));
    ui.colored_label(app.theme.text_secondary, format!("Minecraft: {mc_version}"));

    let gs = &app.global_settings;

    let java_display = match &java_settings.path {
        Some(p) if !p.trim().is_empty() => format!("{p} (Custom)"),
        _ => {
            let global_p = gs.java_path.as_deref().unwrap_or("System Default");
            format!("{global_p} (Global Default)")
        }
    };
    ui.horizontal(|ui| {
        ui.colored_label(app.theme.text_secondary, "Java Path:");
        ui.monospace(java_display);
    });

    let min_display = match &java_settings.memory_min {
        Some(m) if !m.trim().is_empty() => format!("{m} (Custom)"),
        _ => {
            let global_m = gs.memory_min.as_deref().unwrap_or("1G");
            format!("{global_m} (Global Default)")
        }
    };
    ui.horizontal(|ui| {
        ui.colored_label(app.theme.text_secondary, "Min Memory:");
        ui.label(min_display);
    });

    let max_display = match &java_settings.memory_max {
        Some(m) if !m.trim().is_empty() => format!("{m} (Custom)"),
        _ => {
            let global_m = gs.memory_max.as_deref().unwrap_or("2G");
            format!("{global_m} (Global Default)")
        }
    };
    ui.horizontal(|ui| {
        ui.colored_label(app.theme.text_secondary, "Max Memory:");
        ui.label(max_display);
    });
}

fn show_config(
    app: &mut App,
    ui: &mut egui::Ui,
    id: &str,
    tab_state: &mut DetailTabState,
) {
    ui.label(egui::RichText::new("Instance Java & Memory Settings:").strong());
    ui.colored_label(
        app.theme.text_secondary,
        "Configure custom Java and Memory settings for this instance. Leave empty to use global defaults.",
    );

    ui.add_space(app.theme.spacing.md);

    ui.label("Java Path (custom executable):");
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut tab_state.config_java_path)
                .hint_text("Default (uses global Java path)")
                .desired_width(320.0),
        );
        if ui
            .button(format!(" {} Browse", crate::icons::FOLDER))
            .clicked()
        {
            if let Some(path) = rfd::FileDialog::new().pick_file() {
                tab_state.config_java_path = path.display().to_string();
            }
        }
    });

    ui.add_space(app.theme.spacing.sm);

    ui.label("Min Memory (e.g. 1G, 1024M):");
    ui.add(
        egui::TextEdit::singleline(&mut tab_state.config_memory_min)
            .hint_text("Default (uses global min memory)")
            .desired_width(180.0),
    );

    ui.add_space(app.theme.spacing.sm);

    ui.label("Max Memory (e.g. 4G, 4096M):");
    ui.add(
        egui::TextEdit::singleline(&mut tab_state.config_memory_max)
            .hint_text("Default (uses global max memory)")
            .desired_width(180.0),
    );

    ui.add_space(app.theme.spacing.md);

    if ui
        .add(
            egui::Button::new(format!(" {} Save Settings", crate::icons::ADD))
                .fill(app.theme.accent),
        )
        .clicked()
    {
        let java_path = if tab_state.config_java_path.trim().is_empty() {
            None
        } else {
            Some(tab_state.config_java_path.trim().to_string())
        };
        let memory_min = if tab_state.config_memory_min.trim().is_empty() {
            None
        } else {
            Some(tab_state.config_memory_min.trim().to_string())
        };
        let memory_max = if tab_state.config_memory_max.trim().is_empty() {
            None
        } else {
            Some(tab_state.config_memory_max.trim().to_string())
        };

        if let Err(e) = app.instance_manager.update_instance_java_settings(
            id,
            java_path,
            memory_min,
            memory_max,
        ) {
            app.log(
                crate::log::LogLevel::Error,
                &format!("Failed to save instance settings for '{id}': {e}"),
            );
            app.status_message = format!("Failed to save settings: {e}");
        } else {
            app.log(
                crate::log::LogLevel::Info,
                &format!("Saved custom settings for instance '{id}'"),
            );
            app.status_message = "Instance settings saved successfully!".to_string();
        }
    }
}

fn show_logs(app: &mut App, ui: &mut egui::Ui, instance_id: &str, root_path: &std::path::Path) {
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

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Instance logs:").strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if (has_disk_logs || has_buffer_logs)
                && ui
                    .button(format!(" {} Copy Logs", crate::icons::COPY))
                    .clicked()
            {
                let mut full_logs = String::new();
                for entry in &buffer_entries {
                    full_logs.push_str(&format!(
                        "[{}] [{}] {}\n",
                        entry.timestamp,
                        entry.level.as_str(),
                        entry.message
                    ));
                }
                if let Some(ref content) = disk_content {
                    full_logs.push_str(content);
                }
                ui.output_mut(|o| o.copied_text = full_logs);
                app.status_message = "Logs copied to clipboard!".to_string();
                app.log(
                    crate::log::LogLevel::Info,
                    &format!("UI: Copied logs for instance '{instance_id}' to clipboard"),
                );
            }
        });
    });

    ui.add_space(app.theme.spacing.xs);

    if !has_disk_logs && !has_buffer_logs {
        ui.colored_label(
            app.theme.text_secondary,
            "No log entries yet for this instance.",
        );
        return;
    }

    egui::ScrollArea::both()
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
    tab_state: &mut DetailTabState,
    open_mod_browser: &mut Option<String>,
) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Installed mods:").strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button(format!(" {} Add mods", crate::icons::ADD))
                .clicked()
            {
                app.log(
                    crate::log::LogLevel::Info,
                    &format!("UI: Opened Mod Browser for instance '{id}'"),
                );
                *open_mod_browser = Some(id.to_string());
            }
        });
    });

    ui.add_space(app.theme.spacing.xs);

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

    let mods_dir = root_path.join(".minecraft").join("mods");
    let mods = release_the_launcher_mods::list_mods(&mods_dir);

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
        ui.colored_label(
            app.theme.text_secondary,
            "No mods match search or filters.",
        );
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
