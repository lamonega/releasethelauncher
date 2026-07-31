use crate::App;
use crate::View;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    ui.heading("Instances");

    ui.add_space(app.theme.spacing.sm);
    ui.horizontal(|ui| {
        if ui
            .add(
                egui::Button::new(format!(" {} New Instance", crate::icons::ADD))
                    .fill(app.theme.accent),
            )
            .clicked()
        {
            app.current_view = View::NewInstance;
        }
        if ui.button("Accounts").clicked() {
            app.current_view = View::AccountList;
        }
    });

    ui.add_space(app.theme.spacing.sm);
    ui.separator();
    ui.add_space(app.theme.spacing.sm);

    let instances: Vec<String> = app
        .coordinator
        .instance_manager
        .list()
        .iter()
        .map(|i| i.id.clone())
        .collect();

    if instances.is_empty() {
        crate::empty_state(
            ui,
            &app.theme,
            &["No instances.", "Create one to get started."],
        );
    } else {
        for id in &instances {
            if let Some(instance) = app.coordinator.instance_manager.get(id) {
                ui.horizontal(|ui| {
                    if ui
                        .button(format!(
                            "{}  {}",
                            crate::icons::FOLDER,
                            instance.settings.name,
                        ))
                        .clicked()
                    {
                        app.current_view = View::InstanceDetail {
                            id: id.clone(),
                            tab: crate::DetailTab::Info,
                        };
                    }
                    ui.colored_label(
                        app.theme.text_secondary,
                        format!(
                            "{} / {}",
                            instance.settings.minecraft_version,
                            instance.settings.loader_name()
                        ),
                    );
                });
            }
        }
    }

    if !app.status_message.is_empty() {
        ui.add_space(app.theme.spacing.sm);
        ui.separator();
        ui.add_space(app.theme.spacing.sm);
        ui.colored_label(app.theme.text_secondary, &app.status_message);
    }
}
