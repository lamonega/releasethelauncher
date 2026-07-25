use crate::App;
use crate::View;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.heading("Settings");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(format!(" {} Back", crate::icons::BACK)).clicked() {
                app.current_view = View::InstanceList;
            }
        });
    });

    ui.add_space(app.theme.spacing.sm);

    ui.horizontal(|ui| {
        if ui
            .add(
                egui::Button::new(format!(" {} Save", crate::icons::ADD))
                    .fill(app.theme.accent),
            )
            .clicked()
        {
            if let Err(e) = app.save_global_settings() {
                app.status_message = format!("Failed to save settings: {e}");
            } else {
                app.status_message = "Settings saved.".to_string();
            }
        }
    });

    ui.add_space(app.theme.spacing.md);

    ui.heading("Java");
    ui.add_space(app.theme.spacing.sm);

    ui.label("Java Path (optional, leave empty for auto-detect):");
    let mut java_path = app.global_settings.java_path.clone().unwrap_or_default();
    if ui.text_edit_singleline(&mut java_path).changed() {
        app.global_settings.java_path = if java_path.is_empty() {
            None
        } else {
            Some(java_path)
        };
    }

    ui.add_space(app.theme.spacing.sm);

    ui.label("Memory Min:");
    let mut mem_min = app
        .global_settings
        .memory_min
        .clone()
        .unwrap_or_else(|| "1G".to_string());
    if ui.text_edit_singleline(&mut mem_min).changed() {
        app.global_settings.memory_min = Some(mem_min);
    }

    ui.label("Memory Max:");
    let mut mem_max = app
        .global_settings
        .memory_max
        .clone()
        .unwrap_or_else(|| "2G".to_string());
    if ui.text_edit_singleline(&mut mem_max).changed() {
        app.global_settings.memory_max = Some(mem_max);
    }

    ui.add_space(app.theme.spacing.md);

    ui.heading("Launch");
    ui.add_space(app.theme.spacing.sm);

    ui.checkbox(&mut app.global_settings.close_after_launch, "Close launcher after game starts");

    ui.add_space(app.theme.spacing.sm);

    ui.label("Pre-launch command:");
    let mut pre = app.global_settings.pre_launch_command.clone();
    if ui.text_edit_singleline(&mut pre).changed() {
        app.global_settings.pre_launch_command = pre;
    }

    ui.label("Post-launch command:");
    let mut post = app.global_settings.post_launch_command.clone();
    if ui.text_edit_singleline(&mut post).changed() {
        app.global_settings.post_launch_command = post;
    }

    if !app.status_message.is_empty() {
        ui.add_space(app.theme.spacing.md);
        ui.colored_label(app.theme.text_secondary, &app.status_message);
    }
}
