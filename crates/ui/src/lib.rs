pub mod log;
pub mod views;

use std::sync::{Arc, Mutex};

use log::{LogBuffer, LogEntry, LogLevel};
use release_the_launcher_auth::AccountList;
use release_the_launcher_core::InstanceManager;
use release_the_launcher_mods::{ModProvider, ModrinthProvider, SearchArgs, SearchResults};

pub struct App {
    pub instance_manager: InstanceManager,
    pub account_list: AccountList,
    pub current_view: View,
    pub log_buffer: LogBuffer,
    pub status_message: String,
    pub download_state: DownloadState,
    pub ui_queue: Arc<Mutex<Vec<UiMessage>>>,
    pub tokio_handle: Option<tokio::runtime::Handle>,
}

#[derive(Debug, Clone)]
pub enum View {
    InstanceList,
    InstanceDetail { id: String, tab: DetailTab },
    AccountList,
    AccountLogin,
    NewInstance,
    ModBrowser { instance_id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailTab {
    Info,
    Logs,
    Mods,
}

#[derive(Debug, Clone)]
pub enum UiMessage {
    Log(LogEntry),
    Status(String),
    DownloadProgress {
        message: String,
        done: usize,
        total: usize,
    },
    DownloadComplete(String),
    DownloadError(String),
    ModrinthSearchResult(Result<SearchResults, String>),
    ModrinthInstallResult(Result<String, String>),
}

#[derive(Debug, Clone, Default, PartialEq)]
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
    pub completed: usize,
    pub total: usize,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    /// # Panics
    ///
    /// Panics if the `/dev/null` fallback path cannot be discovered.
    #[must_use]
    pub fn new() -> Self {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".config"))
            .join("release-the-launcher");

        let instances_dir = config_dir.join("instances");
        let accounts_path = config_dir.join("accounts.json");

        std::fs::create_dir_all(&config_dir).ok();
        std::fs::create_dir_all(&instances_dir).ok();

        let instance_manager = InstanceManager::discover(instances_dir).unwrap_or_else(|e| {
            tracing::warn!("Failed to discover instances: {e}");
            InstanceManager::discover(std::path::PathBuf::from("/dev/null")).unwrap()
        });

        let account_list = AccountList::load(&accounts_path);

        Self {
            instance_manager,
            account_list,
            current_view: View::InstanceList,
            log_buffer: LogBuffer::new(),
            status_message: String::new(),
            download_state: DownloadState::default(),
            ui_queue: Arc::new(Mutex::new(Vec::new())),
            tokio_handle: None,
        }
    }

    pub fn push_message(&self, msg: UiMessage) {
        if let Ok(mut queue) = self.ui_queue.lock() {
            queue.push(msg);
        }
    }

    pub fn log(&self, level: LogLevel, message: &str) {
        let entry = LogEntry {
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
            level,
            message: message.to_string(),
            target: String::new(),
        };
        self.log_buffer.push(entry);
    }

    pub fn search_modrinth_modpacks(&self, query: String, mc_version: String, loader: String) {
        let queue = self.ui_queue.clone();
        let handle = match &self.tokio_handle {
            Some(h) => h.clone(),
            None => return,
        };
        handle.spawn(async move {
            let provider = ModrinthProvider::new(None);
            let args = SearchArgs {
                query,
                offset: 0,
                limit: 20,
                loaders: if loader.is_empty() {
                    vec![]
                } else {
                    vec![loader]
                },
                mc_versions: if mc_version.is_empty() {
                    vec![]
                } else {
                    vec![mc_version]
                },
                categories: vec![],
                sort: release_the_launcher_mods::SortOrder::Downloads,
            };
            let result = match provider.search_modpacks(&args).await {
                Ok(results) => UiMessage::ModrinthSearchResult(Ok(results)),
                Err(e) => UiMessage::ModrinthSearchResult(Err(e.to_string())),
            };
            if let Ok(mut q) = queue.lock() {
                q.push(result);
            }
        });
    }

    pub fn install_mod_from_modrinth(&self, project_id: String, mods_dir: std::path::PathBuf) {
        let queue = self.ui_queue.clone();
        let handle = match &self.tokio_handle {
            Some(h) => h.clone(),
            None => return,
        };
        handle.spawn(async move {
            let provider = ModrinthProvider::new(None);
            let result = match provider.get_versions(&project_id, &[], &[]).await {
                Ok(versions) => {
                    if let Some(version) = versions.first() {
                        match provider.download_mod(version, &mods_dir).await {
                            Ok(path) => UiMessage::ModrinthInstallResult(Ok(path
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_default())),
                            Err(e) => UiMessage::ModrinthInstallResult(Err(e.to_string())),
                        }
                    } else {
                        UiMessage::ModrinthInstallResult(Err("No versions found".into()))
                    }
                }
                Err(e) => UiMessage::ModrinthInstallResult(Err(e.to_string())),
            };
            if let Ok(mut q) = queue.lock() {
                q.push(result);
            }
        });
    }

    pub fn drain_messages(&mut self) -> Vec<UiMessage> {
        self.ui_queue
            .lock()
            .map(|mut q| std::mem::take(&mut *q))
            .unwrap_or_default()
    }
}
