#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines,
    clippy::option_if_let_else,
    clippy::module_name_repetitions,
    clippy::struct_field_names,
    clippy::similar_names,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::redundant_closure_for_method_calls,
    clippy::map_unwrap_or,
    clippy::inconsistent_struct_constructor,
    clippy::doc_markdown,
    clippy::single_match_else,
    clippy::use_self,
    clippy::uninlined_format_args,
    clippy::format_in_format_args,
    clippy::redundant_clone,
    clippy::needless_pass_by_value,
    clippy::match_wildcard_for_single_variants,
    clippy::assigning_clones,
    clippy::unnecessary_map_or,
    clippy::format_push_string,
    clippy::collapsible_match,
    clippy::struct_excessive_bools,
    clippy::large_enum_variant,
    clippy::too_many_arguments
)]

pub mod layout;
pub mod log;
pub mod services;
pub mod theme;
pub mod views;

pub use layout::LauncherApp;
pub use theme::icons;

/// Renders a centered empty-state label in muted text using the given theme.
pub fn empty_state(ui: &mut egui::Ui, theme: &Theme, messages: &[&str]) {
    ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
        for msg in messages {
            ui.colored_label(theme.text_secondary, *msg);
        }
    });
}

use std::sync::{Arc, Mutex};

use log::{LogBuffer, LogEntry, LogLevel};
use release_the_launcher_auth::AccountList;
use release_the_launcher_core::settings::GlobalSettings;
use release_the_launcher_core::InstanceManager;
use services::launcher::{extract_account_data, do_launch, LaunchParams};
use services::modrinth::send_msg;
use theme::Theme;

type Queue = Arc<Mutex<Vec<UiMessage>>>;

pub struct App {
    pub instance_manager: InstanceManager,
    pub account_list: AccountList,
    pub current_view: View,
    pub log_buffer: LogBuffer,
    pub status_message: String,
    pub download_state: DownloadState,
    pub ui_queue: Queue,
    pub theme: Theme,
    pub tokio_handle: Option<tokio::runtime::Handle>,
    pub ctx: Option<egui::Context>,
    pub global_settings: GlobalSettings,
    pub settings_path: std::path::PathBuf,
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

#[derive(Debug, Clone)]
pub enum UiMessage {
    Log(LogEntry),
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
        account: release_the_launcher_auth::AccountData,
    },
    MsLoginError(String),
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
        Self::new()
    }
}

impl App {
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
            InstanceManager::discover(fallback).unwrap_or_else(|_| {
                InstanceManager::new(instances_dir)
            })
        });

        let account_list = AccountList::load(&accounts_path);
        let global_settings = GlobalSettings::load(&settings_path);

        let log_file = config_dir.join("launcher.log");
        let log_buffer = LogBuffer::new();
        log_buffer.set_log_file_path(log_file.clone());
        log_buffer.push(LogEntry {
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
            level: LogLevel::Info,
            message: format!(
                "Release The Launcher started. Log file: {}",
                log_file.display()
            ),
            target: "launcher".to_string(),
        });
        let theme = Theme::default();

        Self {
            instance_manager,
            account_list,
            current_view: View::InstanceList,
            log_buffer,
            status_message: String::new(),
            download_state: DownloadState::default(),
            ui_queue: Arc::new(Mutex::new(Vec::new())),
            theme,
            tokio_handle: None,
            ctx: None,
            global_settings,
            settings_path,
        }
    }

    pub fn push_message(&self, msg: UiMessage) {
        if let Ok(mut queue) = self.ui_queue.lock() {
            queue.push(msg);
        }
    }

    /// Saves global settings to the settings file.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization or file writing fails.
    pub fn save_global_settings(&self) -> Result<(), std::io::Error> {
        self.global_settings.save(&self.settings_path)
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
        let Some(ctx) = self.ctx.clone() else { return };
        let Some(handle) = self.tokio_handle.clone() else { return };
        services::modrinth::search_modpacks(queue, ctx, handle, query, mc_version, loader);
    }

    pub fn install_mod_from_modrinth(
        &self,
        project_id: String,
        mods_dir: std::path::PathBuf,
        mc_version: Option<String>,
        loader_name: Option<String>,
    ) {
        let queue = self.ui_queue.clone();
        let Some(ctx) = self.ctx.clone() else { return };
        let Some(handle) = self.tokio_handle.clone() else { return };
        services::modrinth::install_mod(queue, ctx, handle, project_id, mods_dir, mc_version, loader_name);
    }

    pub fn fetch_modpack_versions(&self, project_id: String) {
        let queue = self.ui_queue.clone();
        let Some(ctx) = self.ctx.clone() else { return };
        let Some(handle) = self.tokio_handle.clone() else { return };
        services::modrinth::fetch_modpack_versions(queue, ctx, handle, project_id);
    }

    pub fn install_modpack_as_instance(
        &self,
        project_id: String,
        version_id: Option<String>,
        instances_dir: std::path::PathBuf,
    ) {
        let queue = self.ui_queue.clone();
        let Some(ctx) = self.ctx.clone() else { return };
        let Some(handle) = self.tokio_handle.clone() else { return };
        services::modrinth::install_modpack_as_instance(queue, ctx, handle, project_id, version_id, instances_dir);
    }

    pub fn drain_messages(&mut self) -> Vec<UiMessage> {
        self.ui_queue
            .lock()
            .map(|mut q| std::mem::take(&mut *q))
            .unwrap_or_default()
    }

    pub fn launch_instance(&self, instance_id: &str) {
        let queue = self.ui_queue.clone();
        let Some(ctx) = self.ctx.clone() else { return };
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
            send_msg(
                &queue,
                &ctx,
                UiMessage::DownloadError(format!("Instance '{instance_id}' not found")),
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
                ctx,
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
        let queue = self.ui_queue.clone();
        let Some(ctx) = self.ctx.clone() else { return };
        let Some(handle) = self.tokio_handle.clone() else { return };
        services::launcher::fetch_versions_list(queue, ctx, handle);
    }

    pub fn fetch_loader_versions(&self, loader_type: String, mc_version: String) {
        let queue = self.ui_queue.clone();
        let Some(ctx) = self.ctx.clone() else { return };
        let Some(handle) = self.tokio_handle.clone() else { return };
        services::launcher::fetch_loader_versions(queue, ctx, handle, loader_type, mc_version);
    }
}
