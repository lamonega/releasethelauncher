use crate::App;
use crate::View;
use release_the_launcher_core::ModLoader;

pub fn show(app: &mut App, ui: &mut egui::Ui, state: &mut NewInstanceState) {
    if ui.button("Back").clicked() {
        app.current_view = View::InstanceList;
        return;
    }

    ui.heading("New Instance");

    ui.label("Name:");
    ui.text_edit_singleline(&mut state.name);

    ui.label("Minecraft Version:");
    ui.text_edit_singleline(&mut state.mc_version);

    ui.label("Loader:");
    egui::ComboBox::from_label("Mod Loader")
        .selected_text(state.loader_type.as_str())
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut state.loader_type, LoaderType::Vanilla, "Vanilla");
            ui.selectable_value(&mut state.loader_type, LoaderType::Fabric, "Fabric");
            ui.selectable_value(&mut state.loader_type, LoaderType::Forge, "Forge");
            ui.selectable_value(&mut state.loader_type, LoaderType::NeoForge, "NeoForge");
        });

    if state.loader_type != LoaderType::Vanilla {
        ui.label("Loader Version:");
        ui.text_edit_singleline(&mut state.loader_version);
    }

    ui.separator();

    if ui.button("Create Instance").clicked()
        && !state.name.is_empty()
        && !state.mc_version.is_empty()
    {
        let loader = match state.loader_type {
            LoaderType::Vanilla => ModLoader::Vanilla,
            LoaderType::Fabric => ModLoader::Fabric { loader_version: state.loader_version.clone() },
            LoaderType::Forge => ModLoader::Forge { loader_version: state.loader_version.clone() },
            LoaderType::NeoForge => ModLoader::NeoForge { loader_version: state.loader_version.clone() },
        };

        let settings = release_the_launcher_core::InstanceSettings::new(
            state.name.clone(),
            state.mc_version.clone(),
            loader,
        );

        match app.instance_manager.create(&state.name, settings) {
            Ok(_) => {
                app.status_message = format!("Created instance: {}", state.name);
                app.current_view = View::InstanceList;
            }
            Err(e) => {
                app.status_message = format!("Error: {e}");
            }
        }
    }
}

pub struct NewInstanceState {
    pub name: String,
    pub mc_version: String,
    pub loader_type: LoaderType,
    pub loader_version: String,
}

impl Default for NewInstanceState {
    fn default() -> Self {
        Self {
            name: String::new(),
            mc_version: "1.21.1".to_string(),
            loader_type: LoaderType::Vanilla,
            loader_version: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LoaderType {
    Vanilla,
    Fabric,
    Forge,
    NeoForge,
}

impl LoaderType {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            LoaderType::Vanilla => "Vanilla",
            LoaderType::Fabric => "Fabric",
            LoaderType::Forge => "Forge",
            LoaderType::NeoForge => "NeoForge",
        }
    }
}
