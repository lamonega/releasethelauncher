//! Facade between the UI and the backend crates.
//!
//! `Coordinator` owns the shared state (instances, accounts, settings) and every
//! stateful/IO operation the UI can trigger: launching, mod and auth flows
//! (`flow`), plus the read snapshots and mutations exposed on [`Coordinator`]
//! itself. **The UI must not reach past this crate into `core`/`auth`/`mods`/
//! `launch`/`net` for logic — only for data types.** This crate also defines the
//! [`Event`] queue that async flows publish to and the [`dto`] view snapshots.
pub mod dto;
pub mod flow;

use std::future::Future;
use std::path::{Path, PathBuf};

use release_the_launcher_auth::AccountList;
use release_the_launcher_core::log::LogBuffer;
use release_the_launcher_core::settings::GlobalSettings;
use release_the_launcher_core::{InstanceManager, InstanceSettings, JavaSettings, ModLoader};

pub use release_the_launcher_core::log;

pub use flow::launch::{do_launch, extract_account_data, AccountData, LaunchParams};

pub use dto::{AccountSummary, InstalledModEntry, InstanceSummary};

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
        modpack_project_id: Option<String>,
        modpack_version_id: Option<String>,
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
    ModsMetadataResult {
        instance_id: String,
        mods: Vec<release_the_launcher_mods::ModDetails>,
    },
    /// Ask the UI to close its window (post-launch, when configured).
    RequestClose,
}

/// Queues an event for the UI to pick up.
pub fn push_event(queue: &Queue, event: Event) {
    let _ = queue.send(event);
}

/// Owns application state and drives every async flow. UI-agnostic.
pub struct Coordinator {
    instance_manager: InstanceManager,
    account_list: AccountList,
    global_settings: GlobalSettings,
    log_buffer: LogBuffer,
    settings_path: PathBuf,
    http_provider: reqwest::Client,
    queue: Queue,
    rx: tokio::sync::mpsc::UnboundedReceiver<Event>,
    tokio_handle: Option<tokio::runtime::Handle>,
}

impl Default for Coordinator {
    fn default() -> Self {
        Self::new()
    }
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

        let account_list = AccountList::load_or_default(&accounts_path);
        let global_settings = GlobalSettings::load(&settings_path).unwrap_or_else(|e| {
            tracing::warn!("Global settings corrupted or unreadable ({e}), using defaults");
            GlobalSettings::default()
        });

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

        let tokio_handle = tokio::runtime::Handle::try_current().ok();

        Self {
            instance_manager,
            account_list,
            global_settings,
            log_buffer,
            settings_path,
            http_provider: release_the_launcher_net::default_client(),
            queue: tx,
            rx,
            tokio_handle,
        }
    }

    pub fn attach_runtime(&mut self, handle: tokio::runtime::Handle) {
        self.tokio_handle = Some(handle);
    }

    #[must_use]
    pub const fn log_buffer(&self) -> &LogBuffer {
        &self.log_buffer
    }

    #[must_use]
    pub fn queue(&self) -> Queue {
        self.queue.clone()
    }

    /// Drains all pending events.
    #[must_use]
    pub fn drain_events(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        while let Ok(ev) = self.rx.try_recv() {
            events.push(ev);
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

    // ----------------------------------------------------------------------
    // Facade: the only API surface the UI uses for stateful/IO work.
    // ----------------------------------------------------------------------

    /// Returns a lightweight snapshot of the instance, or `None` if it does not
    /// exist.
    #[must_use]
    pub fn instance_summary(&self, id: &str) -> Option<InstanceSummary> {
        self.instance_manager
            .get(&id.to_string())
            .map(|inst| InstanceSummary {
                id: inst.id.clone(),
                name: inst.settings.name.clone(),
                mc_version: inst.settings.minecraft_version.clone(),
                loader_name: inst.settings.loader_name().to_string(),
                root: inst.root.clone(),
                java: inst.settings.java.clone(),
            })
    }

    /// Returns the ids of all discovered instances.
    #[must_use]
    pub fn instance_ids(&self) -> Vec<String> {
        self.instance_manager
            .list()
            .iter()
            .map(|inst| inst.id.clone())
            .collect()
    }

    /// Lists the installed mods of an instance, including parsed metadata when
    /// available.
    #[must_use]
    pub fn list_instance_mods(&self, id: &str) -> Vec<InstalledModEntry> {
        let Some(inst) = self.instance_manager.get(&id.to_string()) else {
            return Vec::new();
        };
        release_the_launcher_mods::list_mods(&inst.mods_dir())
            .into_iter()
            .map(|entry| {
                let details =
                    release_the_launcher_mods::parser::parse_mod_metadata(&entry.path).ok();
                InstalledModEntry {
                    name: entry.name,
                    path: entry.path,
                    enabled: entry.enabled,
                    details,
                }
            })
            .collect()
    }

    /// Returns the mods directory for an instance, or `None` if it does not
    /// exist.
    #[must_use]
    pub fn instance_mods_dir(&self, id: &str) -> Option<PathBuf> {
        self.instance_manager.get_mods_dir(&id.to_string())
    }

    /// Resolves the `latest.log` path for an instance, preferring the
    /// `.minecraft/logs` location and falling back to `logs` next to the
    /// instance root. The caller owns the disk read and any mtime/size caching.
    #[must_use]
    pub fn instance_log_path(&self, id: &str) -> Option<PathBuf> {
        let inst = self.instance_manager.get(&id.to_string())?;
        let mc_log_path = inst.minecraft_dir().join("logs").join("latest.log");
        let alt_log_path = inst.root.join("logs").join("latest.log");
        if mc_log_path.exists() {
            Some(mc_log_path)
        } else if alt_log_path.exists() {
            Some(alt_log_path)
        } else {
            None
        }
    }

    /// Returns a snapshot of every account for rendering.
    #[must_use]
    pub fn accounts(&self) -> Vec<AccountSummary> {
        self.account_list
            .accounts
            .iter()
            .enumerate()
            .map(|(i, account)| AccountSummary {
                name: account.display_name().to_string(),
                account_type: account.account_type.clone(),
                auth_state: account.auth_state(),
                skin_url: account.skin_texture_url(),
                is_active: Some(i) == self.account_list.active_index,
            })
            .collect()
    }

    /// Returns a clone of the current global settings.
    #[must_use]
    pub fn settings(&self) -> GlobalSettings {
        self.global_settings.clone()
    }

    /// Deletes the instance with the given id, removing it from disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the instance does not exist or its directory cannot
    /// be removed.
    pub fn delete_instance(&mut self, id: &str) -> Result<(), String> {
        self.instance_manager
            .delete(&id.to_string())
            .map_err(|e| e.to_string())
    }

    /// Creates a new instance and returns its id.
    ///
    /// # Errors
    ///
    /// Returns an error if an instance with the same name already exists or
    /// writing the instance config fails.
    pub fn create_instance(
        &mut self,
        name: &str,
        mc_version: String,
        loader: ModLoader,
        modpack_project_id: Option<String>,
        modpack_version_id: Option<String>,
    ) -> Result<String, String> {
        let mut settings = InstanceSettings::new(name.to_string(), mc_version, loader);
        settings.modpack_project_id = modpack_project_id;
        settings.modpack_version_id = modpack_version_id;
        self.instance_manager
            .create(name, settings)
            .map(|inst| inst.id.clone())
            .map_err(|e| e.to_string())
    }

    /// Updates and persists an instance's Java path and memory settings.
    ///
    /// # Errors
    ///
    /// Returns an error if the instance is not found or saving fails.
    pub fn update_instance_java_settings(
        &mut self,
        id: &str,
        java: &JavaSettings,
    ) -> Result<(), String> {
        self.instance_manager
            .update_instance_java_settings(id, java)
            .map_err(|e| e.to_string())
    }

    /// Enables or disables the mod at `mod_path` according to its current state.
    ///
    /// # Errors
    ///
    /// Returns an error if the path is not a recognised mod file or the rename
    /// fails.
    pub fn toggle_mod(&mut self, mod_path: &Path) -> Result<(), String> {
        let result = if mod_path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().ends_with(".disabled"))
        {
            release_the_launcher_mods::enable_mod(mod_path)
        } else {
            release_the_launcher_mods::disable_mod(mod_path)
        };
        result.map_err(|e| e.to_string())
    }

    /// Adds an offline account and persists the account list.
    ///
    /// # Errors
    ///
    /// Returns an error if saving the account list fails.
    pub fn add_offline_account(&mut self, username: &str) -> Result<(), String> {
        self.add_account(release_the_launcher_auth::AccountData::offline(username))
    }

    /// Adds an account and persists the account list.
    ///
    /// # Errors
    ///
    /// Returns an error if saving the account list fails.
    pub fn add_account(
        &mut self,
        account: release_the_launcher_auth::AccountData,
    ) -> Result<(), String> {
        self.account_list.add(account);
        self.persist_accounts()
    }

    /// Marks the account at `index` as active and persists the account list.
    ///
    /// # Errors
    ///
    /// Returns an error if `index` is out of bounds or saving fails.
    pub fn set_active_account(&mut self, index: usize) -> Result<(), String> {
        if !self.account_list.set_active(index) {
            return Err(format!("No account at index {index}"));
        }
        self.persist_accounts()
    }

    /// Removes the account at `index` and persists the account list.
    ///
    /// # Errors
    ///
    /// Returns an error if `index` is out of bounds or saving fails.
    pub fn remove_account(&mut self, index: usize) -> Result<(), String> {
        if index >= self.account_list.accounts.len() {
            return Err(format!("No account at index {index}"));
        }
        self.account_list.remove(index);
        self.persist_accounts()
    }

    fn persist_accounts(&self) -> Result<(), String> {
        self.account_list.save().map_err(|e| e.to_string())
    }

    /// Replaces the global settings in memory and persists them to disk.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization or file writing fails.
    pub fn update_settings(&mut self, settings: GlobalSettings) -> Result<(), String> {
        self.global_settings = settings;
        self.global_settings
            .save(&self.settings_path)
            .map_err(|e| e.to_string())
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
        let Some(inst) = self.instance_manager.get(&instance_id.to_string()) else {
            push_event(
                &self.queue(),
                Event::DownloadError(format!("Instance '{instance_id}' not found")),
            );
            return;
        };

        let gs = &self.global_settings;
        let params = LaunchParams {
            queue: self.queue(),
            account_data: extract_account_data(&self.account_list),
            active_auth_account: self.account_list.active().cloned(),
            http_client: self.http_provider.clone(),
            instance_id: instance_id.to_string(),
            instance_root: inst.root.clone(),
            mc_version: inst.settings.minecraft_version.clone(),
            loader: inst.settings.loader.clone(),
            modpack_project_id: inst.settings.modpack_project_id.clone(),
            modpack_version_id: inst.settings.modpack_version_id.clone(),
            java_path_override: gs.java_path_for(inst.settings.java.path.as_deref()),
            memory_min: gs.memory_min_for(inst.settings.java.memory_min.as_deref()),
            memory_max: gs.memory_max_for(inst.settings.java.memory_max.as_deref()),
            pre_launch_command: if inst.settings.pre_launch_command.is_empty() {
                gs.pre_launch_command.clone()
            } else {
                inst.settings.pre_launch_command.clone()
            },
            post_launch_command: if inst.settings.post_launch_command.is_empty() {
                gs.post_launch_command.clone()
            } else {
                inst.settings.post_launch_command.clone()
            },
            close_after_launch: inst.settings.close_after_launch || gs.close_after_launch,
        };

        self.run_async(move |queue| async move {
            do_launch(LaunchParams { queue, ..params }).await;
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

    pub fn install_modpack_as_instance(&self, project_id: String, version_id: Option<String>) {
        let http = self.http_provider.clone();
        self.run_async(move |queue| {
            flow::modrinth::resolve_modpack_as_instance(queue, project_id, version_id, http)
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
        let mods_dir = inst.mods_dir();
        release_the_launcher_mods::list_mods(&mods_dir)
            .into_iter()
            .filter(|e| e.enabled)
            .filter_map(|e| release_the_launcher_mods::parser::parse_mod_metadata(&e.path).ok())
            .collect()
    }

    pub fn request_mods_metadata(&self, instance_id: String) {
        let Some(inst) = self.instance_manager.get(&instance_id) else {
            return;
        };
        let mods_dir = inst.mods_dir();
        let id = instance_id;

        self.run_async(move |queue| async move {
            let mods = tokio::task::spawn_blocking(move || {
                release_the_launcher_mods::list_mods(&mods_dir)
                    .into_iter()
                    .filter(|e| e.enabled)
                    .filter_map(|e| {
                        release_the_launcher_mods::parser::parse_mod_metadata(&e.path).ok()
                    })
                    .collect()
            })
            .await
            .unwrap_or_default();

            push_event(
                &queue,
                Event::ModsMetadataResult {
                    instance_id: id,
                    mods,
                },
            );
        });
    }

    pub fn check_mod_updates(&self, instance_id: String) {
        let Some(summary) = self.instance_summary(&instance_id) else {
            return;
        };
        let Some(mods_dir) = self.instance_mods_dir(&instance_id) else {
            return;
        };
        let mc_version = summary.mc_version;
        let loader_str = summary.loader_name;

        self.run_async(move |queue| async move {
            use release_the_launcher_mods::ModProvider;
            let installed_mods: Vec<_> = release_the_launcher_mods::list_mods(&mods_dir)
                .into_iter()
                .filter(|e| e.enabled)
                .filter_map(|e| {
                    let hash = release_the_launcher_core::hash::compute_sha1_file(&e.path).ok()?;
                    Some(release_the_launcher_mods::InstalledMod {
                        path: e.path,
                        hash,
                        hash_type: "sha1".to_string(),
                        project_id: None,
                        version_id: None,
                    })
                })
                .collect();
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
