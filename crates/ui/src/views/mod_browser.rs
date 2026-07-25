use crate::App;
use crate::View;

pub fn show(app: &mut App, ui: &mut egui::Ui, instance_id: &str, state: &mut ModBrowserState) {
    let id = instance_id.to_string();

    if ui.button("Back").clicked() {
        app.current_view = View::InstanceDetail { id };
        return;
    }

    ui.heading("Mod Browser (Modrinth)");

    ui.horizontal(|ui| {
        ui.label("Search:");
        ui.text_edit_singleline(&mut state.query);
        if ui.button("Search").clicked() && !state.query.is_empty() {
            state.status = "Searching...".to_string();
        }
    });

    ui.separator();

    if !state.status.is_empty() {
        ui.label(&state.status);
    }

    if state.results.is_empty() && state.status.is_empty() {
        ui.label("Search for mods to install.");
    } else {
        for result in &state.results {
            ui.group(|ui| {
                ui.label(format!("{} by {}", result.name, result.author));
                ui.label(&result.description);
                ui.horizontal(|ui| {
                    if ui.button("Install").clicked() {
                        state.status = format!("Installing {}...", result.name);
                    }
                    if ui.button("Details").clicked() {
                        state.status = format!("Details: {}", result.id);
                    }
                });
            });
        }
    }
}

#[derive(Default)]
pub struct ModBrowserState {
    pub query: String,
    pub results: Vec<ModResult>,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct ModResult {
    pub id: String,
    pub name: String,
    pub author: String,
    pub description: String,
}
