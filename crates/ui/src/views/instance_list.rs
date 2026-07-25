use crate::App;
use crate::View;

/// # Panics
///
/// Panics if an instance ID in the list cannot be found in the instance manager.
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

    let instances: Vec<String> = app.instance_manager.list()
        .iter()
        .map(|i| i.id.clone())
        .collect();

    if instances.is_empty() {
        ui.label("No instances. Create one to get started.");
    } else {
        for id in &instances {
            let instance = app.instance_manager.get(id).unwrap();
            ui.horizontal(|ui| {
                let label = format!(
                    "{} - {} ({})",
                    instance.settings.name,
                    instance.settings.minecraft_version,
                    instance.settings.loader_name()
                );
                if ui.button(&label).clicked() {
                    app.current_view = View::InstanceDetail { id: id.clone() };
                }
            });
        }
    }

    if let Some((done, total)) = &app.download_progress {
        ui.separator();
        ui.label(format!("Downloads: {done}/{total}"));
        #[allow(clippy::cast_precision_loss)] // Download progress values are small enough for f64
        let progress = if *total > 0 { *done as f64 / *total as f64 } else { 0.0 };
        #[allow(clippy::cast_possible_truncation)] // Progress is always in [0.0, 1.0]
        ui.add(egui::ProgressBar::new(progress as f32));
    }

    if !app.status_message.is_empty() {
        ui.separator();
        ui.label(&app.status_message);
    }
}
