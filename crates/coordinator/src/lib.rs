pub mod flow;

use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use release_the_launcher_auth::AccountList;
use release_the_launcher_core::log::LogBuffer;
use release_the_launcher_core::settings::GlobalSettings;
use release_the_launcher_core::InstanceManager;

pub use release_the_launcher_core::log;

pub use flow::launch::{do_launch, extract_account_data, AccountData, LaunchParams};

pub type Queue = tokio::sync::mpsc::UnboundedSender<Event>;

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
    ModrinthInstallResult {
        instance_id: String,
        name: String,
        mc_version: String,
        loader: String,
    },
    ModUpdatesResult {
        instance_id: String,
        updates: Vec<release_the_launcher_mods::ModUpdate>,
    },
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
    let _ = queue.send(event);
}

/// Owns application state and drives every async flow. UI-agnostic.
pub struct Coordinator {
    pub instance_manager: InstanceManager,
    pub account_list: AccountList,
    pub global_settings: GlobalSettings,
    pub log_buffer: LogBuffer,
    pub settings_path: PathBuf,
    pub http_provider: reqwest::Client,
    queue: Queue,
    rx: Arc<Mutex<tokio::sync::mpsc::UnboundedReceiver<Event>>>,
    tokio_handle: Option<tokio::runtime::Handle>,
}

impl Coordinator {
    #[must_use]
    pub fn new() -> Self {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".config"))
            .join(release_the_launcher_constants::paths::APP_DIR_NAME);

        let instances_dir =
            config_dir.join(release_the_launcher_constants::paths::INSTANCES_DIR_NAME);
        let accounts_path =
            config_dir.join(release_the_launcher_constants::paths::ACCOUNTS_FILE_NAME);
        let settings_path =
            config_dir.join(release_the_launcher_constants::paths::SETTINGS_FILE_NAME);

        std::fs::create_dir_all(&config_dir).ok();
        std::fs::create_dir_all(&instances_dir).ok();

        let instance_manager =
            InstanceManager::discover(instances_dir.clone()).unwrap_or_else(|e| {
                tracing::warn!("Failed to discover instances: {e}");
                let fallback = std::env::temp_dir().join(format!(
                    "{}-instances",
                    release_the_launcher_constants::paths::APP_DIR_NAME
                ));
                let _ = std::fs::create_dir_all(&fallback);
                InstanceManager::discover(fallback)
                    .unwrap_or_else(|_| InstanceManager::new(instances_dir))
            });

        let account_list = AccountList::load(&accounts_path);
        let global_settings = GlobalSettings::load(&settings_path);

        let log_file = config_dir.join(release_the_launcher_constants::paths::LOG_FILE_NAME);
        let log_buffer = LogBuffer::new();
        log_buffer.set_log_file_path(log_file.clone());
        log_buffer.push(log::LogEntry {
            timestamp: chrono::Local::now()
                .format(release_the_launcher_constants::defaults::TIMESTAMP_FORMAT)
                .to_string(),
            level: log::LogLevel::Info,
            message: format!(
                "Release The Launcher started. Log file: {}",
                log_file.display()
            ),
            target: "launcher".to_string(),
        });

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        Self {
            instance_manager,
            account_list,
            global_settings,
            log_buffer,
            settings_path,
            http_provider: release_the_launcher_net::default_client(),
            queue: tx,
            rx: Arc::new(Mutex::new(rx)),
            tokio_handle: None,
        }
    }

    pub fn attach_runtime(&mut self, handle: tokio::runtime::Handle) {
        self.tokio_handle = Some(handle);
    }

    #[must_use]
    pub fn queue(&self) -> Queue {
        self.queue.clone()
    }

    /// Drains all pending events.
    #[must_use]
    pub fn drain_events(&self) -> Vec<Event> {
        let mut events = Vec::new();
        if let Ok(mut rx) = self.rx.lock() {
            while let Ok(ev) = rx.try_recv() {
                events.push(ev);
            }
        }
        events
    }

    pub fn log(&self, level: log::LogLevel, message: &str) {
        let entry = log::LogEntry {
            timestamp: chrono::Local::now()
                .format(release_the_launcher_constants::defaults::TIMESTAMP_FORMAT)
                .to_string(),
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

    fn run_async<F: Future<Output = ()> + Send + 'static>(
        &self,
        f: impl FnOnce(Queue) -> F + Send + 'static,
    ) -> bool {
        let queue = self.queue();
        let Some(handle) = self.tokio_handle.clone() else {
            return false;
        };
        handle.spawn(f(queue));
        true
    }

    pub fn launch_instance(&self, instance_id: &str) {
        let account_data = extract_account_data(&self.account_list);
        let active_auth_account = self.account_list.active().cloned();
        let http_client = self.http_provider.clone();

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
                gs.memory_min_for(inst.settings.java.memory_min.as_deref()),
                gs.memory_max_for(inst.settings.java.memory_max.as_deref()),
                pre,
                post,
                close,
            )
        } else {
            push_event(
                &self.queue(),
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
        self.run_async(move |queue| {
            do_launch(LaunchParams {
                queue,
                account_data,
                active_auth_account,
                http_client,
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
        });
    }

    pub fn fetch_versions_list(&self) {
        self.run_async(flow::launch::fetch_versions_list);
    }

    pub fn fetch_loader_versions(&self, loader_type: &str, mc_version: &str) {
        let loader_type = loader_type.to_string();
        let mc_version = mc_version.to_string();
        self.run_async(move |queue| {
            flow::launch::fetch_loader_versions(queue, loader_type, mc_version)
        });
    }

    pub fn search_modpacks(&self, query: String, mc_version: String, loader: String) {
        let http = self.http_provider.clone();
        self.run_async(move |queue| {
            flow::modrinth::search_modpacks(queue, query, mc_version, loader, http)
        });
    }

    pub fn search_mods(&self, query: String, mc_version: String, loader_name: String) {
        let http = self.http_provider.clone();
        self.run_async(move |queue| {
            flow::modrinth::search_mods(queue, query, mc_version, loader_name, http)
        });
    }

    pub fn install_mod(
        &self,
        project_id: String,
        mods_dir: PathBuf,
        mc_version: Option<String>,
        loader_name: Option<String>,
    ) {
        let http = self.http_provider.clone();
        self.run_async(move |queue| {
            flow::modrinth::install_mod(queue, project_id, mods_dir, mc_version, loader_name, http)
        });
    }

    pub fn fetch_modpack_versions(&self, project_id: String) {
        self.run_async(move |queue| flow::modrinth::fetch_modpack_versions(queue, project_id));
    }

    pub fn install_modpack_as_instance(
        &self,
        project_id: String,
        version_id: Option<String>,
        instances_dir: PathBuf,
    ) {
        let http = self.http_provider.clone();
        self.run_async(move |queue| {
            flow::modrinth::install_modpack_as_instance(
                queue,
                project_id,
                version_id,
                instances_dir,
                http,
            )
        });
    }

    pub fn start_ms_login(&self) {
        self.run_async(flow::msa::start_login);
    }

    pub fn refresh_active_account(&self) {
        let mut active = self.account_list.active().cloned();
        let http = self.http_provider.clone();
        self.run_async(move |queue| async move {
            if let Some(ref mut account) = active {
                if release_the_launcher_auth::refresh::needs_refresh(account) {
                    let client_id = release_the_launcher_constants::urls::DEFAULT_MSA_CLIENT_ID;
                    match release_the_launcher_auth::refresh::try_refresh_if_needed(
                        account, &http, client_id,
                    )
                    .await
                    {
                        Ok(Some(refreshed)) => {
                            push_event(
                                &queue,
                                Event::MsLoginSuccess {
                                    account: Box::new(refreshed),
                                },
                            );
                            push_event(&queue, Event::Status("Account refreshed".to_string()));
                        }
                        Ok(None) => {}
                        Err(e) => {
                            push_event(&queue, Event::MsLoginError(e.to_string()));
                        }
                    }
                }
            }
        });
    }

    #[must_use]
    pub fn mods_metadata(&self, instance_id: &str) -> Vec<release_the_launcher_mods::ModDetails> {
        let Some(inst) = self.instance_manager.get(&instance_id.to_string()) else {
            return Vec::new();
        };
        let mods_dir = inst
            .root
            .join(release_the_launcher_constants::paths::MINECRAFT_DIR)
            .join(release_the_launcher_constants::paths::MODS_DIR);
        let entries = release_the_launcher_mods::list_mods(&mods_dir);
        let mut results = Vec::new();
        for entry in entries {
            if entry.enabled {
                if let Ok(details) =
                    release_the_launcher_mods::parser::parse_mod_metadata(&entry.path)
                {
                    results.push(details);
                }
            }
        }
        results
    }

    pub fn check_mod_updates(&self, instance_id: String) {
        let inst = self.instance_manager.get(&instance_id);
        let Some(inst) = inst else {
            return;
        };
        let mods_dir = inst
            .root
            .join(release_the_launcher_constants::paths::MINECRAFT_DIR)
            .join(release_the_launcher_constants::paths::MODS_DIR);
        let mc_version = inst.settings.minecraft_version.clone();
        let loader_str = inst.settings.loader_name().to_string();

        self.run_async(move |queue| async move {
            use release_the_launcher_mods::ModProvider;
            let entries = release_the_launcher_mods::list_mods(&mods_dir);
            let mut installed_mods = Vec::new();
            for entry in entries {
                if entry.enabled {
                    if let Ok(bytes) = std::fs::read(&entry.path) {
                        let hash = release_the_launcher_core::hash::compute_sha1_bytes(&bytes);
                        installed_mods.push(release_the_launcher_mods::InstalledMod {
                            path: entry.path,
                            hash,
                            hash_type: "sha1".to_string(),
                            project_id: None,
                            version_id: None,
                        });
                    }
                }
            }
            let provider = release_the_launcher_mods::ModrinthProvider::new(None);
            let updates = match provider
                .check_updates(&installed_mods, &[mc_version], &[loader_str])
                .await
            {
                Ok(u) => u,
                Err(e) => {
                    tracing::warn!("Failed to check mod updates: {e}");
                    Vec::new()
                }
            };
            push_event(
                &queue,
                Event::ModUpdatesResult {
                    instance_id,
                    updates,
                },
            );
        });
    }
}

impl Default for Coordinator {
    fn default() -> Self {
        Self::new()
    }
}
