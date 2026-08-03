pub mod layout;
pub mod theme;
pub mod views;
pub mod widgets;

pub use layout::LauncherApp;
pub use release_the_launcher_coordinator::Event as UiMessage;
pub use release_the_launcher_core::log;
pub use theme::icons;

use release_the_launcher_coordinator::log::LogLevel;
use release_the_launcher_coordinator::{Coordinator, Queue};

/// Renders a centered empty-state label in muted text using the given theme.
pub fn empty_state(ui: &mut egui::Ui, theme: &Theme, messages: &[&str]) {
    ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
        for msg in messages {
            ui.colored_label(theme.text_secondary, *msg);
        }
    });
}

use theme::Theme;

pub struct App {
    pub coordinator: Coordinator,
    pub current_view: View,
    pub status_message: String,
    pub download_state: DownloadState,
    pub theme: Theme,
    pub ctx: Option<egui::Context>,
    /// Re-push target for view-result events (see [`layout::drain_ui_messages`]).
    pub ui_queue: Queue,
}

#[derive(Debug, Clone)]
pub enum View {
    InstanceList,
    InstanceDetail { id: String, tab: DetailTab },
    AccountList,
    AccountLogin,
    NewInstance,
    ModBrowser { instance_id: String },
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailTab {
    Info,
    Logs,
    Mods,
    Config,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum DownloadPhase {
    #[default]
    Idle,
    Resolving,
    Downloading {
        message: String,
    },
}

#[derive(Default)]
pub struct DownloadState {
    pub phase: DownloadPhase,
    pub completed: u64,
    pub total: u64,
}

impl Default for App {
    fn default() -> Self {
        Self::new(Coordinator::new(), Theme::default(), None)
    }
}

impl App {
    #[must_use]
    pub fn new(coordinator: Coordinator, theme: Theme, ctx: Option<egui::Context>) -> Self {
        let ui_queue = coordinator.queue();
        Self {
            coordinator,
            current_view: View::InstanceList,
            status_message: String::new(),
            download_state: DownloadState::default(),
            theme,
            ctx,
            ui_queue,
        }
    }

    #[must_use]
    pub fn drain_messages(&self) -> Vec<UiMessage> {
        self.coordinator.drain_events()
    }

    #[must_use]
    pub fn drain_ui_queue(&self) -> Vec<UiMessage> {
        self.coordinator.drain_events()
    }

    #[must_use]
    pub fn instance_ids(&self) -> Vec<String> {
        self.coordinator
            .instance_manager
            .list()
            .iter()
            .map(|i| i.id.clone())
            .collect()
    }

    pub fn log(&self, level: LogLevel, message: &str) {
        self.coordinator.log(level, message);
    }

    /// Saves global settings to the settings file.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization or file writing fails.
    pub fn save_global_settings(&self) -> Result<(), std::io::Error> {
        self.coordinator.save_global_settings()
    }

    pub fn launch_instance(&self, instance_id: &str) {
        self.coordinator.launch_instance(instance_id);
    }

    pub fn fetch_versions_list(&self) {
        self.coordinator.fetch_versions_list();
    }

    pub fn fetch_loader_versions(&self, loader_type: &str, mc_version: &str) {
        self.coordinator
            .fetch_loader_versions(loader_type, mc_version);
    }

    pub fn search_modrinth_modpacks(&self, query: String, mc_version: String, loader: String) {
        self.coordinator.search_modpacks(query, mc_version, loader);
    }

    pub fn search_mods(&self, query: String, mc_version: String, loader_name: String) {
        self.coordinator.search_mods(query, mc_version, loader_name);
    }

    pub fn install_mod_from_modrinth(
        &self,
        project_id: String,
        mods_dir: std::path::PathBuf,
        mc_version: Option<String>,
        loader_name: Option<String>,
    ) {
        self.coordinator
            .install_mod(project_id, mods_dir, mc_version, loader_name);
    }

    pub fn fetch_modpack_versions(&self, project_id: String) {
        self.coordinator.fetch_modpack_versions(project_id);
    }

    pub fn install_modpack_as_instance(&self, project_id: String, version_id: Option<String>) {
        self.coordinator
            .install_modpack_as_instance(project_id, version_id);
    }

    pub fn check_mod_updates(&self, instance_id: String) {
        self.coordinator.check_mod_updates(instance_id);
    }

    pub fn start_ms_login(&self) {
        self.coordinator.start_ms_login();
    }
}
