pub mod config;
pub mod info;
pub mod logs;
pub mod mods;

pub use config::show_config;
pub use info::show_info;
pub use logs::show_logs;
pub use mods::show_mods;

use crate::{App, DetailTab, View};

pub struct InstanceView<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub mc_version: &'a str,
    pub loader_name: &'a str,
    pub root_display: &'a str,
    pub root_path: &'a std::path::Path,
    pub java_settings: &'a release_the_launcher_core::JavaSettings,
}

pub struct DetailTabState {
    pub log_scroll_to_end: bool,
    pub mod_search_query: String,
    pub mod_filter: ModFilter,
    pub config_instance_id: String,
    pub config_java_path: String,
    pub config_memory_min: String,
    pub config_memory_max: String,
    pub mod_updates: ModUpdatesState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModFilter {
    All,
    EnabledOnly,
    DisabledOnly,
    None,
}

#[derive(Debug, Clone, Default)]
pub enum ModUpdatesState {
    #[default]
    Idle,
    Checking,
    Loaded(Vec<release_the_launcher_mods::ModUpdate>),
}

impl Default for DetailTabState {
    fn default() -> Self {
        Self {
            log_scroll_to_end: false,
            mod_search_query: String::new(),
            mod_filter: ModFilter::All,
            config_instance_id: String::new(),
            config_java_path: String::new(),
            config_memory_min: String::new(),
            config_memory_max: String::new(),
            mod_updates: ModUpdatesState::default(),
        }
    }
}

pub fn show(
    app: &mut App,
    ui: &mut egui::Ui,
    instance_id: &str,
    tab: DetailTab,
    tab_state: &mut DetailTabState,
) {
    let id = instance_id.to_string();

    let Some(instance) = app.coordinator.instance_manager.get(&id) else {
        ui.label("Instance not found.");
        app.current_view = View::InstanceList;
        return;
    };

    let root_display = instance.root.display().to_string();
    let view = InstanceView {
        id: &id,
        name: &instance.settings.name,
        mc_version: &instance.settings.minecraft_version,
        loader_name: instance.settings.loader_name(),
        root_display: &root_display,
        root_path: &instance.root,
        java_settings: &instance.settings.java,
    };

    if tab_state.config_instance_id != id {
        tab_state.config_instance_id.clone_from(&id);
        tab_state.config_java_path = view.java_settings.path.clone().unwrap_or_default();
        tab_state.config_memory_min = view.java_settings.memory_min.clone().unwrap_or_default();
        tab_state.config_memory_max = view.java_settings.memory_max.clone().unwrap_or_default();
    }

    ui.heading(view.name);
    ui.colored_label(
        app.theme.text_secondary,
        format!("{} | {}", view.mc_version, view.loader_name),
    );

    ui.add_space(app.theme.spacing.sm);

    // view borrows from instance, but handle_actions needs &mut app.
    // We need to drop instance before calling handle_actions.
    let view_id = view.id.to_string();
    let view_name = view.name.to_string();
    let view_root_path = view.root_path.to_path_buf();
    let view_root_display = view.root_display.to_string();
    let view_mc_version = view.mc_version.to_string();
    let view_loader_name = view.loader_name.to_string();
    let view_java_settings = view.java_settings.clone();
    let _ = instance;

    let view = InstanceView {
        id: &view_id,
        name: &view_name,
        mc_version: &view_mc_version,
        loader_name: &view_loader_name,
        root_display: &view_root_display,
        root_path: &view_root_path,
        java_settings: &view_java_settings,
    };

    if handle_actions(app, ui, &view) {
        return;
    }

    ui.add_space(app.theme.spacing.sm);
    ui.separator();
    ui.add_space(app.theme.spacing.sm);

    let is_vanilla = view.loader_name == "Vanilla";
    show_tabs(app, ui, view.id, tab, is_vanilla);
    ui.separator();
    ui.add_space(app.theme.spacing.sm);

    show_tab_content(app, ui, tab, tab_state, &view);
}

fn handle_actions(app: &mut App, ui: &mut egui::Ui, view: &InstanceView<'_>) -> bool {
    let mut action = None;
    ui.horizontal(|ui| {
        if ui
            .add(crate::widgets::icon_button(crate::icons::LAUNCH, "Launch").fill(app.theme.accent))
            .clicked()
        {
            action = Some("launch");
        }
        if ui
            .add(crate::widgets::icon_button(
                crate::icons::FOLDER,
                "Open Folder",
            ))
            .clicked()
        {
            action = Some("open_folder");
        }
        if ui
            .add(crate::widgets::icon_button(crate::icons::DELETE, "Delete"))
            .clicked()
        {
            action = Some("delete");
        }
    });

    match action {
        Some("launch") => {
            app.log(
                crate::log::LogLevel::Info,
                &format!("Launching instance: {}", view.name),
            );
            app.status_message = format!("Launching {}...", view.name);
            app.launch_instance(view.id);
            false
        }
        Some("open_folder") => {
            app.log(
                crate::log::LogLevel::Info,
                &format!("UI: Open folder for instance '{}'", view.name),
            );
            let _ = open::that(view.root_path);
            false
        }
        Some("delete") => {
            app.log(
                crate::log::LogLevel::Info,
                &format!("UI: Deleted instance '{}'", view.name),
            );
            let _ = app
                .coordinator
                .instance_manager
                .delete(&view.id.to_string());
            app.current_view = View::InstanceList;
            true
        }
        _ => false,
    }
}

fn show_tab_content(
    app: &mut App,
    ui: &mut egui::Ui,
    tab: DetailTab,
    tab_state: &mut DetailTabState,
    view: &InstanceView<'_>,
) {
    match tab {
        DetailTab::Info => {
            show_info(
                app,
                ui,
                view.root_display,
                view.loader_name,
                view.mc_version,
                view.java_settings,
            );
        }
        DetailTab::Config => {
            show_config(app, ui, view.id, tab_state);
        }
        DetailTab::Logs => {
            show_logs(app, ui, view.id, view.root_path);
        }
        DetailTab::Mods => {
            if view.loader_name == "Vanilla" {
                show_info(
                    app,
                    ui,
                    view.root_display,
                    view.loader_name,
                    view.mc_version,
                    view.java_settings,
                );
            } else {
                show_mods(app, ui, view.root_path, view.id, tab_state);
            }
        }
    }
}

fn show_tabs(app: &mut App, ui: &mut egui::Ui, id: &str, tab: DetailTab, is_vanilla: bool) {
    let tabs: Vec<(DetailTab, &str)> = if is_vanilla {
        vec![
            (DetailTab::Info, "Info"),
            (DetailTab::Config, "Config"),
            (DetailTab::Logs, "Logs"),
        ]
    } else {
        vec![
            (DetailTab::Info, "Info"),
            (DetailTab::Config, "Config"),
            (DetailTab::Logs, "Logs"),
            (DetailTab::Mods, "Mods"),
        ]
    };

    if let Some(target_tab) = crate::widgets::tab_row(ui, &app.theme, &tab, &tabs) {
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
