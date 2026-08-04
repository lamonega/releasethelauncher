use super::DetailTabState;
use crate::LauncherApp;
use release_the_launcher_core::JavaSettings;

pub fn show_config(
    app: &mut LauncherApp,
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
            .add(crate::widgets::icon_button(crate::icons::FOLDER, "Browse"))
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
        .add(crate::widgets::icon_button(crate::icons::ADD, "Save Settings").fill(app.theme.accent))
        .clicked()
    {
        let java = JavaSettings {
            path: if tab_state.config_java_path.trim().is_empty() {
                None
            } else {
                Some(tab_state.config_java_path.trim().to_string())
            },
            memory_min: if tab_state.config_memory_min.trim().is_empty() {
                None
            } else {
                Some(tab_state.config_memory_min.trim().to_string())
            },
            memory_max: if tab_state.config_memory_max.trim().is_empty() {
                None
            } else {
                Some(tab_state.config_memory_max.trim().to_string())
            },
        };

        if let Err(e) = app.coordinator.update_instance_java_settings(id, &java) {
            app.coordinator.log(
                crate::log::LogLevel::Error,
                &format!("Failed to save instance settings for '{id}': {e}"),
            );
            app.status_message = format!("Failed to save settings: {e}");
        } else {
            app.coordinator.log(
                crate::log::LogLevel::Info,
                &format!("Saved custom settings for instance '{id}'"),
            );
            app.status_message = "Instance settings saved successfully!".to_string();
        }
    }
}
