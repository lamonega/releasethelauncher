use crate::{widgets, App, View};

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    show_header(app, ui);
    ui.add_space(app.theme.spacing.md);
    show_java_settings(app, ui);
    ui.add_space(app.theme.spacing.md);
    show_launch_settings(app, ui);

    if !app.status_message.is_empty() {
        ui.add_space(app.theme.spacing.md);
        ui.colored_label(app.theme.text_secondary, &app.status_message);
    }
}

fn show_header(app: &mut App, ui: &mut egui::Ui) {
    if widgets::page_header(ui, app, "Settings", Some(View::InstanceList)) {
        return;
    }

    ui.horizontal(|ui| {
        if ui
            .add(widgets::icon_button(crate::icons::ADD, "Save").fill(app.theme.accent))
            .clicked()
        {
            app.log(crate::log::LogLevel::Info, "UI: Settings saved");
            if let Err(e) = app.save_global_settings() {
                app.status_message = format!("Failed to save settings: {e}");
            } else {
                app.status_message = "Settings saved.".to_string();
            }
        }
    });
}

fn show_java_settings(app: &mut App, ui: &mut egui::Ui) {
    ui.heading("Java");
    ui.add_space(app.theme.spacing.sm);

    let mut java_path = app
        .coordinator
        .global_settings()
        .java
        .path
        .clone()
        .unwrap_or_default();
    if widgets::settings_field(
        ui,
        "Java Path (optional, leave empty for auto-detect):",
        &mut java_path,
    ) {
        app.log(
            crate::log::LogLevel::Info,
            &format!("UI: Java path changed to '{java_path}'"),
        );
        app.coordinator.global_settings_mut().java.path = if java_path.is_empty() {
            None
        } else {
            Some(java_path)
        };
    }

    ui.add_space(app.theme.spacing.sm);

    let mut mem_min = app
        .coordinator
        .global_settings()
        .java
        .memory_min
        .clone()
        .unwrap_or_else(|| "1G".to_string());
    if widgets::settings_field(ui, "Memory Min:", &mut mem_min) {
        app.log(
            crate::log::LogLevel::Info,
            &format!("UI: Memory min changed to '{mem_min}'"),
        );
        app.coordinator.global_settings_mut().java.memory_min = Some(mem_min);
    }

    let mut mem_max = app
        .coordinator
        .global_settings()
        .java
        .memory_max
        .clone()
        .unwrap_or_else(|| "2G".to_string());
    if widgets::settings_field(ui, "Memory Max:", &mut mem_max) {
        app.log(
            crate::log::LogLevel::Info,
            &format!("UI: Memory max changed to '{mem_max}'"),
        );
        app.coordinator.global_settings_mut().java.memory_max = Some(mem_max);
    }
}

fn show_launch_settings(app: &mut App, ui: &mut egui::Ui) {
    ui.heading("Launch");
    ui.add_space(app.theme.spacing.sm);

    if ui
        .checkbox(
            &mut app.coordinator.global_settings_mut().close_after_launch,
            "Close launcher after game starts",
        )
        .changed()
    {
        let close_after = app.coordinator.global_settings().close_after_launch;
        app.log(
            crate::log::LogLevel::Info,
            &format!("UI: Close after launch toggled to {close_after}"),
        );
    }

    ui.add_space(app.theme.spacing.sm);

    let mut pre = app.coordinator.global_settings().pre_launch_command.clone();
    if widgets::settings_field(ui, "Pre-launch command:", &mut pre) {
        app.log(
            crate::log::LogLevel::Info,
            &format!("UI: Pre-launch command changed to '{pre}'"),
        );
        app.coordinator.global_settings_mut().pre_launch_command = pre;
    }

    let mut post = app.coordinator.global_settings().post_launch_command.clone();
    if widgets::settings_field(ui, "Post-launch command:", &mut post) {
        app.log(
            crate::log::LogLevel::Info,
            &format!("UI: Post-launch command changed to '{post}'"),
        );
        app.coordinator.global_settings_mut().post_launch_command = post;
    }
}
