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

    ui.horizontal(|ui| {
        ui.heading(&name);
        ui.label(format!("| {mc_version} | {loader_name}"));
    });

    let mut action = None;
    ui.horizontal(|ui| {
        if ui.button("Launch").clicked() {
            action = Some("launch");
        }
        if ui.button("Open Folder").clicked() {
            action = Some("open_folder");
        }
        if ui.button("Delete").clicked() {
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
        }
        Some("open_folder") => {
            let _ = open::that(&root_path);
        }
        Some("delete") => {
            let _ = app.instance_manager.delete(&id);
            app.current_view = View::InstanceList;
            return;
        }
        _ => {}
    }

    ui.separator();

    show_tabs(app, ui, &id, tab);
    ui.separator();

    match tab {
        DetailTab::Info => {
            show_info(ui, &root_display, &loader_name, &mc_version);
        }
        DetailTab::Logs => {
            show_logs(app, ui);
        }
        DetailTab::Mods => {
            show_mods(app, ui, &root_path, &id, open_mod_browser);
        }
    }
}

fn show_tabs(app: &mut App, ui: &mut egui::Ui, id: &str, tab: DetailTab) {
    ui.horizontal(|ui| {
        for (label, target_tab) in [
            ("Info", DetailTab::Info),
            ("Logs", DetailTab::Logs),
            ("Mods", DetailTab::Mods),
        ] {
            let style = if tab == target_tab {
                egui::Button::new(label).fill(egui::Color32::from_rgb(60, 60, 80))
            } else {
                egui::Button::new(label)
            };
            if ui.add(style).clicked() {
                app.current_view = View::InstanceDetail {
                    id: id.to_string(),
                    tab: target_tab,
                };
            }
        }
    });
}

fn show_info(ui: &mut egui::Ui, root_display: &str, loader_name: &str, mc_version: &str) {
    ui.label("Instance folder:");
    ui.monospace(root_display);
    ui.label(format!("Loader: {loader_name}"));
    ui.label(format!("Minecraft: {mc_version}"));
}

fn show_logs(app: &App, ui: &mut egui::Ui) {
    let entries = app.log_buffer.entries();
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            ui.style_mut().override_text_style = Some(egui::TextStyle::Monospace);
            for entry in &entries {
                let color = match entry.level {
                    crate::log::LogLevel::Error => egui::Color32::RED,
                    crate::log::LogLevel::Warn => egui::Color32::YELLOW,
                    crate::log::LogLevel::Info => egui::Color32::LIGHT_GRAY,
                    crate::log::LogLevel::Debug => egui::Color32::GRAY,
                    crate::log::LogLevel::Trace => egui::Color32::DARK_GRAY,
                };
                let text = format!(
                    "[{}] {} {}",
                    entry.timestamp,
                    entry.level.as_str(),
                    entry.message
                );
                ui.colored_label(color, text);
            }
            if entries.is_empty() {
                ui.label("No log entries yet.");
            }
        });
}

fn show_mods(
    _app: &mut App,
    ui: &mut egui::Ui,
    root_path: &std::path::Path,
    id: &str,
    open_mod_browser: &mut Option<String>,
) {
    ui.label("Installed mods:");
    let mods_dir = root_path.join(".minecraft").join("mods");
    if mods_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&mods_dir) {
            let mut found_mods = false;
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if std::path::Path::new(&name)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("jar"))
                {
                    found_mods = true;
                    ui.label(&name);
                }
            }
            if !found_mods {
                ui.label("No mods installed.");
            }
        }
    } else {
        ui.label("No mods directory.");
    }
    ui.separator();
    if ui.button("Browse Mods (Modrinth)").clicked() {
        *open_mod_browser = Some(id.to_string());
    }
}
