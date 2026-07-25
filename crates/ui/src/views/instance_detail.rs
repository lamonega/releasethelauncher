use crate::App;
use crate::View;

pub fn show(app: &mut App, ui: &mut egui::Ui, instance_id: &str) {
    let id = instance_id.to_string();

    if ui.button("Back").clicked() {
        app.current_view = View::InstanceList;
        return;
    }

    if let Some(instance) = app.instance_manager.get(&id) {
        let name = instance.settings.name.clone();
        let mc_version = instance.settings.minecraft_version.clone();
        let loader_name = instance.settings.loader_name().to_string();
        let root_display = instance.root.display().to_string();

        ui.heading(&name);

        ui.group(|ui| {
            ui.label(format!("Minecraft Version: {mc_version}"));
            ui.label(format!("Loader: {loader_name}"));
        });

        ui.separator();

        let mut action = None;
        ui.horizontal(|ui| {
            if ui.button("Launch").clicked() {
                action = Some("launch");
            }
            if ui.button("Mods").clicked() {
                action = Some("mods");
            }
            if ui.button("Delete").clicked() {
                action = Some("delete");
            }
        });

        match action {
            Some("launch") => {
                app.status_message = format!("Launching {name}...");
            }
            Some("mods") => {
                app.current_view = View::ModBrowser { instance_id: id };
                return;
            }
            Some("delete") => {
                let _ = app.instance_manager.delete(&id);
                app.current_view = View::InstanceList;
                return;
            }
            _ => {}
        }

        ui.separator();
        ui.label("Instance folder:");
        ui.monospace(root_display);
    } else {
        ui.label("Instance not found.");
        app.current_view = View::InstanceList;
    }
}
