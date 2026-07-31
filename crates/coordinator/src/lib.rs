
pub mod flow;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use release_the_launcher_auth::AccountList;
use release_the_launcher_core::log::LogBuffer;
use release_the_launcher_core::settings::GlobalSettings;
use release_the_launcher_core::InstanceManager;

pub use release_the_launcher_core::log;

pub use flow::launch::{do_launch, extract_account_data, LaunchParams};

pub type Queue = Arc<Mutex<Vec<Event>>>;

/// Events emitted by async flows and consumed by the UI.
#[derive(Debug, Clone)]
pub enum Event {
    Log(log::LogEntry),
    Status(String),
    DownloadProgress {
        message: String,
        done: u64,
        total: u64,
    },
    DownloadComplete(String),
    DownloadError(String),
    ModrinthSearchResult(Result<release_the_launcher_mods::SearchResults, String>),
    ModrinthVersionsResult {
        project_id: String,
        result: Result<Vec<release_the_launcher_mods::ModVersion>, String>,
    },
    ModrinthInstallResult(Result<String, String>),
    VersionListResult(Result<Vec<(String, String)>, String>),
    LoaderVersionsResult {
        loader_type: String,
        mc_version: String,
        result: Result<Vec<String>, String>,
    },
    MsDeviceCode {
        user_code: String,
        verification_uri: String,
        message: String,
    },
    MsLoginSuccess {
        account: Box<release_the_launcher_auth::AccountData>,
    },
    MsLoginError(String),
    /// Ask the UI to close its window (post-launch, when configured).
    RequestClose,
}

/// Queues an event for the UI to pick up.
pub fn push_event(queue: &Queue, event: Event) {
    if let Ok(mut q) = queue.lock() {
        q.push(event);
    }
}

/// Owns application state and drives every async flow. UI-agnostic.
pub struct Coordinator {
    pub instance_manager: InstanceManager,
    pub account_list: AccountList,
    pub global_settings: GlobalSettings,
    pub log_buffer: LogBuffer,
    pub settings_path: PathBuf,
    queue: Queue,
    tokio_handle: Option<tokio::runtime::Handle>,
}

impl Coordinator {
    #[must_use]
    pub fn new() -> Self {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".config"))
            .join("release-the-launcher");

        let instances_dir = config_dir.join("instances");
        let accounts_path = config_dir.join("accounts.json");
        let settings_path = config_dir.join("settings.toml");

        std::fs::create_dir_all(&config_dir).ok();
        std::fs::create_dir_all(&instances_dir).ok();

        let instance_manager = InstanceManager::discover(instances_dir.clone()).unwrap_or_else(|e| {
            tracing::warn!("Failed to discover instances: {e}");
            let fallback = std::env::temp_dir().join("release-the-launcher-instances");
            let _ = std::fs::create_dir_all(&fallback);
            InstanceManager::discover(fallback).unwrap_or_else(|_| InstanceManager::new(instances_dir))
        });

        let account_list = AccountList::load(&accounts_path);
        let global_settings = GlobalSettings::load(&settings_path);

        let log_file = config_dir.join("launcher.log");
        let log_buffer = LogBuffer::new();
        log_buffer.set_log_file_path(log_file.clone());
        log_buffer.push(log::LogEntry {
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
            level: log::LogLevel::Info,
            message: format!(
                "Release The Launcher started. Log file: {}",
                log_file.display()
            ),
            target: "launcher".to_string(),
        });

        Self {
            instance_manager,
            account_list,
            global_settings,
            log_buffer,
            settings_path,
            queue: Arc::new(Mutex::new(Vec::new())),
            tokio_handle: None,
        }
    }

    pub fn attach_runtime(&mut self, handle: tokio::runtime::Handle) {
        self.tokio_handle = Some(handle);
    }

    #[must_use]
    pub fn queue(&self) -> Queue {
        Arc::clone(&self.queue)
    }

    /// Drains all pending events.
    #[must_use]
    pub fn drain_events(&self) -> Vec<Event> {
        self.queue
            .lock()
            .map(|mut q| std::mem::take(&mut *q))
            .unwrap_or_default()
    }

    pub fn log(&self, level: log::LogLevel, message: &str) {
        let entry = log::LogEntry {
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
            level,
            message: message.to_string(),
            target: String::new(),
        };
        self.log_buffer.push(entry);
    }

    /// Saves global settings to the settings file.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization or file writing fails.
    pub fn save_global_settings(&self) -> Result<(), std::io::Error> {
        self.global_settings.save(&self.settings_path)
    }

    pub fn launch_instance(&self, instance_id: &str) {
        let queue = self.queue();
        let Some(handle) = self.tokio_handle.clone() else { return };

        let account_data = extract_account_data(&self.account_list);

        let inst = if let Some(inst) = self.instance_manager.get(&instance_id.to_string()) {
            let gs = &self.global_settings;
            let pre = if inst.settings.pre_launch_command.is_empty() {
                gs.pre_launch_command.clone()
            } else {
                inst.settings.pre_launch_command.clone()
            };
            let post = if inst.settings.post_launch_command.is_empty() {
                gs.post_launch_command.clone()
            } else {
                inst.settings.post_launch_command.clone()
            };
            let close = inst.settings.close_after_launch || gs.close_after_launch;
            (
                inst.root.clone(),
                inst.settings.minecraft_version.clone(),
                inst.settings.loader.clone(),
                gs.java_path_for(inst.settings.java.path.as_deref()),
                gs.memory_min_for(&inst.settings.java.memory_min),
                gs.memory_max_for(&inst.settings.java.memory_max),
                pre,
                post,
                close,
            )
        } else {
            push_event(
                &queue,
                Event::DownloadError(format!("Instance '{instance_id}' not found")),
            );
            return;
        };
        let (
            instance_root,
            mc_version,
            loader,
            java_path_override,
            memory_min,
            memory_max,
            pre_launch_command,
            post_launch_command,
            close_after_launch,
        ) = inst;

        let id_str = instance_id.to_string();
        handle.spawn(async move {
            do_launch(LaunchParams {
                queue,
                account_data,
                instance_id: id_str,
                instance_root,
                mc_version,
                loader,
                java_path_override,
                memory_min,
                memory_max,
                pre_launch_command,
                post_launch_command,
                close_after_launch,
            })
            .await;
        });
    }

    pub fn fetch_versions_list(&self) {
        let queue = self.queue();
        let Some(handle) = self.tokio_handle.clone() else { return };
        flow::launch::fetch_versions_list(&queue, &handle);
    }

    pub fn fetch_loader_versions(&self, loader_type: &str, mc_version: &str) {
        let queue = self.queue();
        let Some(handle) = self.tokio_handle.clone() else { return };
        flow::launch::fetch_loader_versions(&queue, &handle, loader_type, mc_version);
    }

    pub fn search_modpacks(&self, query: String, mc_version: String, loader: String) {
        let queue = self.queue();
        let Some(handle) = self.tokio_handle.clone() else { return };
        flow::modrinth::search_modpacks(&queue, &handle, query, mc_version, loader);
    }

    pub fn search_mods(&self, query: String, mc_version: String, loader_name: String) {
        let queue = self.queue();
        let Some(handle) = self.tokio_handle.clone() else { return };
        flow::modrinth::search_mods(&queue, &handle, query, mc_version, loader_name);
    }

    pub fn install_mod(
        &self,
        project_id: String,
        mods_dir: PathBuf,
        mc_version: Option<String>,
        loader_name: Option<String>,
    ) {
        let queue = self.queue();
        let Some(handle) = self.tokio_handle.clone() else { return };
        flow::modrinth::install_mod(&queue, &handle, project_id, mods_dir, mc_version, loader_name);
    }

    pub fn fetch_modpack_versions(&self, project_id: String) {
        let queue = self.queue();
        let Some(handle) = self.tokio_handle.clone() else { return };
        flow::modrinth::fetch_modpack_versions(&queue, &handle, project_id);
    }

    pub fn install_modpack_as_instance(
        &self,
        project_id: String,
        version_id: Option<String>,
        instances_dir: PathBuf,
    ) {
        let queue = self.queue();
        let Some(handle) = self.tokio_handle.clone() else { return };
        flow::modrinth::install_modpack_as_instance(&queue, &handle, project_id, version_id, instances_dir);
    }

    pub fn start_ms_login(&self) {
        let queue = self.queue();
        let Some(handle) = self.tokio_handle.clone() else { return };
        flow::msa::start_login(&queue, &handle);
    }
}

impl Default for Coordinator {
    fn default() -> Self {
        Self::new()
    }
}
