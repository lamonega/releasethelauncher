use crate::{widgets, LauncherApp, View};
use release_the_launcher_core::GlobalSettings;

pub fn show(app: &mut LauncherApp, ui: &mut egui::Ui) {
    let mut settings = local_settings(ui).unwrap_or_else(|| app.coordinator.settings().clone());

    show_header(app, ui, &mut settings);
    ui.add_space(app.theme.spacing.md);
    show_java_settings(app, ui, &mut settings);
    ui.add_space(app.theme.spacing.md);
    show_launch_settings(app, ui, &mut settings);

    set_local_settings(ui, &settings);

    if !app.status_message.is_empty() {
        ui.add_space(app.theme.spacing.md);
        ui.colored_label(app.theme.text_secondary, &app.status_message);
    }
}

fn local_settings(ui: &egui::Ui) -> Option<GlobalSettings> {
    let id = ui.id();
    ui.data(|data| data.get_temp::<GlobalSettings>(id))
}

fn set_local_settings(ui: &egui::Ui, settings: &GlobalSettings) {
    let id = ui.id();
    ui.data_mut(|data| data.insert_temp(id, settings.clone()));
}

fn show_header(app: &mut LauncherApp, ui: &mut egui::Ui, settings: &mut GlobalSettings) {
    if widgets::page_header(ui, app, "Settings", Some(View::InstanceList)) {
        return;
    }

    ui.horizontal(|ui| {
        if ui
            .add(widgets::icon_button(crate::icons::ADD, "Save").fill(app.theme.accent))
            .clicked()
        {
            app.coordinator
                .log(crate::log::LogLevel::Info, "UI: Settings saved");
            if let Err(e) = app.coordinator.update_settings(settings.clone()) {
                app.status_message = format!("Failed to save settings: {e}");
            } else {
                app.status_message = "Settings saved.".to_string();
            }
        }
    });
}

fn show_java_settings(app: &mut LauncherApp, ui: &mut egui::Ui, settings: &mut GlobalSettings) {
    ui.heading("Java");
    ui.add_space(app.theme.spacing.sm);

    let mut java_path = settings.java.path.clone().unwrap_or_default();
    if widgets::settings_field(
        ui,
        "Java Path (optional, leave empty for auto-detect):",
        &mut java_path,
    ) {
        app.coordinator.log(
            crate::log::LogLevel::Info,
            &format!("UI: Java path changed to '{java_path}'"),
        );
        settings.java.path = if java_path.is_empty() {
            None
        } else {
            Some(java_path)
        };
    }

    ui.add_space(app.theme.spacing.sm);

    let mut mem_min = settings
        .java
        .memory_min
        .clone()
        .unwrap_or_else(|| "1G".to_string());
    if widgets::settings_field(ui, "Memory Min:", &mut mem_min) {
        app.coordinator.log(
            crate::log::LogLevel::Info,
            &format!("UI: Memory min changed to '{mem_min}'"),
        );
        settings.java.memory_min = Some(mem_min);
    }

    let mut mem_max = settings
        .java
        .memory_max
        .clone()
        .unwrap_or_else(|| "2G".to_string());
    if widgets::settings_field(ui, "Memory Max:", &mut mem_max) {
        app.coordinator.log(
            crate::log::LogLevel::Info,
            &format!("UI: Memory max changed to '{mem_max}'"),
        );
        settings.java.memory_max = Some(mem_max);
    }
}

fn show_launch_settings(app: &mut LauncherApp, ui: &mut egui::Ui, settings: &mut GlobalSettings) {
    ui.heading("Launch");
    ui.add_space(app.theme.spacing.sm);

    if ui
        .checkbox(
            &mut settings.close_after_launch,
            "Close launcher after game starts",
        )
        .changed()
    {
        let close_after = settings.close_after_launch;
        app.coordinator.log(
            crate::log::LogLevel::Info,
            &format!("UI: Close after launch toggled to {close_after}"),
        );
    }

    ui.add_space(app.theme.spacing.sm);

    let mut pre = settings.pre_launch_command.clone();
    if widgets::settings_field(ui, "Pre-launch command:", &mut pre) {
        app.coordinator.log(
            crate::log::LogLevel::Info,
            &format!("UI: Pre-launch command changed to '{pre}'"),
        );
        settings.pre_launch_command = pre;
    }

    let mut post = settings.post_launch_command.clone();
    if widgets::settings_field(ui, "Post-launch command:", &mut post) {
        app.coordinator.log(
            crate::log::LogLevel::Info,
            &format!("UI: Post-launch command changed to '{post}'"),
        );
        settings.post_launch_command = post;
    }
}
