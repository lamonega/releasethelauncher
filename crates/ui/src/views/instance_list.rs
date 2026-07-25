use crate::App;
use crate::View;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    ui.heading("Instances");

    ui.horizontal(|ui| {
        if ui.button("New Instance").clicked() {
            app.current_view = View::NewInstance;
        }
        if ui.button("Accounts").clicked() {
            app.current_view = View::AccountList;
        }
    });

    ui.separator();

    let instances: Vec<String> = app
        .instance_manager
        .list()
        .iter()
        .map(|i| i.id.clone())
        .collect();

    if instances.is_empty() {
        ui.label("No instances. Create one to get started.");
    } else {
        for id in &instances {
            if let Some(instance) = app.instance_manager.get(id) {
                let label = format!(
                    "{} - {} ({})",
                    instance.settings.name,
                    instance.settings.minecraft_version,
                    instance.settings.loader_name()
                );
                if ui.button(&label).clicked() {
                    app.current_view = View::InstanceDetail {
                        id: id.clone(),
                        tab: crate::DetailTab::Info,
                    };
                }
            }
        }
    }

    if !app.status_message.is_empty() {
        ui.separator();
        ui.label(&app.status_message);
    }
}
