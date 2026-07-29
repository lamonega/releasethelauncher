pub mod log;
pub mod theme;
pub mod views;

/// Renders a centered empty-state label in muted text using the given theme.
pub fn empty_state(ui: &mut egui::Ui, theme: &Theme, messages: &[&str]) {
    ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
        for msg in messages {
            ui.colored_label(theme.text_secondary, *msg);
        }
    });
}

use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use log::{LogBuffer, LogEntry, LogLevel};
use release_the_launcher_auth::AccountList;
use release_the_launcher_core::settings::GlobalSettings;
use release_the_launcher_core::{InstanceManager, ModLoader};
use release_the_launcher_launch::assets::AssetManager;
use release_the_launcher_launch::{
    assemble_launch_profile, build_command, AssetIndex, DependencyResolver, DownloadManager,
    PlayerAuth,
};
use release_the_launcher_mods::{ModProvider, ModrinthProvider, SearchArgs, SearchResults};
use theme::{icons, Theme};

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
    ModrinthSearchResult(Result<SearchResults, String>),
    ModrinthVersionsResult {
        project_id: String,
        result: Result<Vec<release_the_launcher_mods::ModVersion>, String>,
    },
    ModrinthInstallResult(Result<String, String>),
    VersionListResult(Result<Vec<(String, String)>, String>),
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
        let settings_path = config_dir.join("settings.toml");

        std::fs::create_dir_all(&config_dir).ok();
        std::fs::create_dir_all(&instances_dir).ok();

        let instance_manager = InstanceManager::discover(instances_dir).unwrap_or_else(|e| {
            tracing::warn!("Failed to discover instances: {e}");
            InstanceManager::discover(std::path::PathBuf::from("/dev/null")).unwrap()
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

    /// # Panics
    ///
    /// Panics if `self.ctx` is `None` (it is always set by `main.rs`).
    pub fn search_modrinth_modpacks(&self, query: String, mc_version: String, loader: String) {
        let queue = self.ui_queue.clone();
        let ctx = self.ctx.clone().expect("egui context not set");
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
            send_msg(&queue, &ctx, result);
        });
    }

    /// # Panics
    ///
    /// Panics if `self.ctx` is `None` (it is always set by `main.rs`).
    pub fn install_mod_from_modrinth(&self, project_id: String, mods_dir: std::path::PathBuf) {
        let queue = self.ui_queue.clone();
        let ctx = self.ctx.clone().expect("egui context not set");
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
            send_msg(&queue, &ctx, result);
        });
    }

    pub fn fetch_modpack_versions(&self, project_id: String) {
        let queue = self.ui_queue.clone();
        let ctx = self.ctx.clone().expect("egui context not set");
        let handle = match &self.tokio_handle {
            Some(h) => h.clone(),
            None => return,
        };
        let pid = project_id.clone();
        handle.spawn(async move {
            let provider = ModrinthProvider::new(None);
            let result = match provider.get_versions(&pid, &[], &[]).await {
                Ok(versions) => UiMessage::ModrinthVersionsResult {
                    project_id: pid,
                    result: Ok(versions),
                },
                Err(e) => UiMessage::ModrinthVersionsResult {
                    project_id: pid,
                    result: Err(e.to_string()),
                },
            };
            send_msg(&queue, &ctx, result);
        });
    }

    /// # Panics
    ///
    /// Panics if `self.ctx` is `None` (it is always set by `main.rs`).
    pub fn install_modpack_as_instance(
        &self,
        project_id: String,
        version_id: Option<String>,
        instances_dir: std::path::PathBuf,
    ) {
        let queue = self.ui_queue.clone();
        let ctx = self.ctx.clone().expect("egui context not set");
        let handle = match &self.tokio_handle {
            Some(h) => h.clone(),
            None => return,
        };
        handle.spawn(async move {
            let provider = ModrinthProvider::new(None);
            let result = match provider
                .install_modpack_as_instance(&project_id, version_id.as_deref(), &instances_dir)
                .await
            {
                Ok((name, mc_ver, loader_str)) => {
                    UiMessage::ModrinthInstallResult(Ok(format!("{name}|{mc_ver}|{loader_str}")))
                }
                Err(e) => UiMessage::ModrinthInstallResult(Err(e.to_string())),
            };
            send_msg(&queue, &ctx, result);
        });
    }

    pub fn drain_messages(&mut self) -> Vec<UiMessage> {
        self.ui_queue
            .lock()
            .map(|mut q| std::mem::take(&mut *q))
            .unwrap_or_default()
    }

    /// # Panics
    ///
    /// Panics if `self.ctx` is `None` (it is always set by `main.rs`).
    pub fn launch_instance(&self, instance_id: &str) {
        let queue = self.ui_queue.clone();
        let ctx = self.ctx.clone().expect("egui context not set");
        let Some(handle) = self.tokio_handle.clone() else {
            return;
        };

        let account_data = extract_account_data(&self.account_list);

        // Set up censor filters on the app's log buffer (PrismLauncher pattern)
        self.log_buffer.clear_censor_filters();
        if let Some((_, ref uuid, ref token)) = account_data {
            if !token.is_empty() && token != "0" {
                self.log_buffer
                    .add_censor_filter(token.clone(), "<ACCESS TOKEN>".into());
            }
            if !uuid.is_empty() {
                self.log_buffer
                    .add_censor_filter(uuid.clone(), "<PROFILE ID>".into());
            }
        }

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

        handle.spawn(async move {
            do_launch(LaunchParams {
                queue,
                ctx,
                account_data,
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

    /// # Panics
    ///
    /// Panics if `self.ctx` is `None` (it is always set by `main.rs`).
    pub fn fetch_versions_list(&self) {
        let queue = self.ui_queue.clone();
        let ctx = self.ctx.clone().expect("egui context not set");
        let Some(handle) = self.tokio_handle.clone() else {
            return;
        };
        handle.spawn(async move {
            let mut resolver = DependencyResolver::new();
            let result = match resolver.fetch_manifest().await {
                Ok(()) => {
                    let versions: Vec<(String, String)> = resolver.available_versions_with_types();
                    Ok(versions)
                }
                Err(e) => Err(e.to_string()),
            };
            send_msg(&queue, &ctx, UiMessage::VersionListResult(result));
        });
    }
}

struct LaunchParams {
    queue: Queue,
    ctx: egui::Context,
    account_data: Option<(String, String, String)>,
    instance_root: std::path::PathBuf,
    mc_version: String,
    loader: ModLoader,
    java_path_override: Option<String>,
    memory_min: String,
    memory_max: String,
    pre_launch_command: String,
    post_launch_command: String,
    close_after_launch: bool,
}

fn send_log(
    queue: &Arc<Mutex<Vec<UiMessage>>>,
    ctx: &egui::Context,
    level: crate::log::LogLevel,
    message: impl Into<String>,
) {
    if let Ok(mut q) = queue.lock() {
        q.push(UiMessage::Log(crate::log::LogEntry {
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
            level,
            message: message.into(),
            target: "launcher".to_string(),
        }));
    }
    ctx.request_repaint();
}

async fn do_launch(params: LaunchParams) {
    let send = |msg: UiMessage| send_msg(&params.queue, &params.ctx, msg);

    send_log(
        &params.queue,
        &params.ctx,
        crate::log::LogLevel::Info,
        format!(
            "=== Launching Instance: {} ===",
            params.instance_root.display()
        ),
    );

    let Some((ref player_name, ref player_uuid, ref access_token)) = params.account_data else {
        let err = "No active account. Add an account before launching.".to_string();
        send_log(
            &params.queue,
            &params.ctx,
            crate::log::LogLevel::Error,
            &err,
        );
        send(UiMessage::DownloadError(err));
        return;
    };

    send_log(
        &params.queue,
        &params.ctx,
        crate::log::LogLevel::Info,
        format!("Account: {} (UUID: {})", player_name, player_uuid),
    );
    send_log(
        &params.queue,
        &params.ctx,
        crate::log::LogLevel::Info,
        format!(
            "Minecraft: {}, Loader: {:?}",
            params.mc_version, params.loader
        ),
    );

    if run_pre_launch(&params).await.is_err() {
        send_log(
            &params.queue,
            &params.ctx,
            crate::log::LogLevel::Error,
            "Pre-launch command failed",
        );
        return;
    }

    let index_path = params.instance_root.join("modrinth.index.json");
    if index_path.exists() {
        send_log(
            &params.queue,
            &params.ctx,
            crate::log::LogLevel::Info,
            "Downloading modpack mods...",
        );
        let mod_manager = release_the_launcher_mods::ModrinthProvider::new(None);
        let progress_queue = params.queue.clone();
        let progress_ctx = params.ctx.clone();
        let _ = mod_manager
            .download_modpack_files(&params.instance_root, move |done, total, mod_name| {
                let _ = progress_queue.lock().map(|mut q| {
                    q.push(UiMessage::DownloadProgress {
                        message: format!("Downloading mod: {mod_name}"),
                        done,
                        total,
                    });
                });
                progress_ctx.request_repaint();
            })
            .await;
    }

    send_log(
        &params.queue,
        &params.ctx,
        crate::log::LogLevel::Info,
        "Resolving version components & manifests...",
    );

    let Ok(components) = resolve_components(
        &params.queue,
        &params.ctx,
        &params.loader,
        &params.mc_version,
    )
    .await
    else {
        send_log(
            &params.queue,
            &params.ctx,
            crate::log::LogLevel::Error,
            "Failed to resolve version components",
        );
        return;
    };

    send_log(
        &params.queue,
        &params.ctx,
        crate::log::LogLevel::Info,
        format!("Resolved {} component(s)", components.len()),
    );

    let profile = match assemble_launch_profile(&components) {
        Ok(p) => p,
        Err(e) => {
            let msg = format!("Failed to assemble profile: {e}");
            send_log(
                &params.queue,
                &params.ctx,
                crate::log::LogLevel::Error,
                &msg,
            );
            send(UiMessage::DownloadError(msg));
            return;
        }
    };

    send_log(
        &params.queue,
        &params.ctx,
        crate::log::LogLevel::Info,
        format!(
            "Profile ready: MainClass='{}', {} libraries",
            profile.main_class,
            profile.libraries.len()
        ),
    );

    send_log(
        &params.queue,
        &params.ctx,
        crate::log::LogLevel::Info,
        "Checking & downloading required game libraries...",
    );
    download_game_files(&params.queue, &params.ctx, &params.instance_root, &profile).await;

    send_log(
        &params.queue,
        &params.ctx,
        crate::log::LogLevel::Info,
        "Extracting native libraries...",
    );
    extract_natives_files(&params.queue, &params.ctx, &params.instance_root, &profile);

    send_log(
        &params.queue,
        &params.ctx,
        crate::log::LogLevel::Info,
        "Checking & downloading game assets...",
    );
    download_assets(
        &params.queue,
        &params.ctx,
        &params.instance_root,
        &profile.asset_index,
    )
    .await;

    if let Some(ref client_dl) = profile.client_download {
        if !client_dl.url.is_empty() {
            let client_jar = params
                .instance_root
                .join("versions")
                .join(&profile.mc_version)
                .join(format!("{}.jar", profile.mc_version));
            if !client_jar.exists() {
                send_log(
                    &params.queue,
                    &params.ctx,
                    crate::log::LogLevel::Info,
                    format!("Downloading Minecraft client: {}.jar", profile.mc_version),
                );
                send(UiMessage::Status(format!(
                    "Downloading Minecraft {}.jar...",
                    profile.mc_version
                )));
                let dl_mgr = DownloadManager::new(params.instance_root.clone());
                if let Err(e) = dl_mgr
                    .download_client_jar(&client_jar, &client_dl.url, client_dl.sha1.as_deref())
                    .await
                {
                    let msg = format!("Failed to download client.jar: {e}");
                    send_log(
                        &params.queue,
                        &params.ctx,
                        crate::log::LogLevel::Error,
                        &msg,
                    );
                    send(UiMessage::DownloadError(msg));
                    return;
                }
            }
        }
    }

    send_log(
        &params.queue,
        &params.ctx,
        crate::log::LogLevel::Info,
        "Resolving Java runtime...",
    );
    let Some(java_path) = resolve_java_path(
        &params.queue,
        &params.ctx,
        params.java_path_override.as_deref(),
        &profile.compatible_java_majors,
    ) else {
        send_log(
            &params.queue,
            &params.ctx,
            crate::log::LogLevel::Error,
            "Failed to resolve Java path",
        );
        return;
    };

    send_log(
        &params.queue,
        &params.ctx,
        crate::log::LogLevel::Info,
        format!("Java executable: {}", java_path.display()),
    );

    let mut cmd = build_command(
        &profile,
        &params.instance_root,
        &java_path,
        &PlayerAuth {
            name: player_name.clone(),
            uuid: player_uuid.clone(),
            access_token: access_token.clone(),
        },
        &params.memory_min,
        &params.memory_max,
    );

    send_log(
        &params.queue,
        &params.ctx,
        crate::log::LogLevel::Info,
        format!("Java Command Line: {:?}", cmd.as_std()),
    );

    send(UiMessage::Status("Launching game...".to_string()));
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let msg = format!("Failed to spawn process: {e}");
            send_log(
                &params.queue,
                &params.ctx,
                crate::log::LogLevel::Error,
                &msg,
            );
            send(UiMessage::DownloadError(msg));
            return;
        }
    };

    send(UiMessage::DownloadComplete("Game is running".to_string()));
    send_log(
        &params.queue,
        &params.ctx,
        crate::log::LogLevel::Info,
        "Game process started successfully. Streaming logs...",
    );

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let queue_out = params.queue.clone();
    let ctx_out = params.ctx.clone();
    if let Some(stdout) = stdout {
        tokio::spawn(async move {
            use tokio::io::AsyncBufReadExt;
            let mut reader = tokio::io::BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if let Ok(mut q) = queue_out.lock() {
                    q.push(UiMessage::Log(crate::log::LogEntry {
                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                        level: crate::log::LogLevel::Info,
                        message: line,
                        target: "game".to_string(),
                    }));
                }
                ctx_out.request_repaint();
            }
        });
    }

    let queue_err = params.queue.clone();
    let ctx_err = params.ctx.clone();
    if let Some(stderr) = stderr {
        tokio::spawn(async move {
            use tokio::io::AsyncBufReadExt;
            let mut reader = tokio::io::BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if let Ok(mut q) = queue_err.lock() {
                    q.push(UiMessage::Log(crate::log::LogEntry {
                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                        level: crate::log::LogLevel::Error,
                        message: line,
                        target: "game".to_string(),
                    }));
                }
                ctx_err.request_repaint();
            }
        });
    }

    match child.wait().await {
        Ok(status) => {
            let msg = format!("Game process exited with status: {status}");
            let level = if status.success() {
                crate::log::LogLevel::Info
            } else {
                crate::log::LogLevel::Error
            };
            send_log(&params.queue, &params.ctx, level, &msg);
            send(UiMessage::DownloadComplete(msg));
        }
        Err(e) => {
            let msg = format!("Failed to wait for game process: {e}");
            send_log(
                &params.queue,
                &params.ctx,
                crate::log::LogLevel::Error,
                &msg,
            );
            send(UiMessage::DownloadError(msg));
        }
    }

    run_post_launch(&params).await;

    if params.close_after_launch {
        params.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

async fn run_pre_launch(params: &LaunchParams) -> Result<(), ()> {
    if params.pre_launch_command.is_empty() {
        return Ok(());
    }
    send_msg(
        &params.queue,
        &params.ctx,
        UiMessage::Status("Running pre-launch command...".to_string()),
    );
    match release_the_launcher_launch::run_pre_launch_command(
        &params.pre_launch_command,
        &params.instance_root,
    )
    .await
    {
        Ok(()) => Ok(()),
        Err(e) => {
            send_msg(
                &params.queue,
                &params.ctx,
                UiMessage::DownloadError(format!("Pre-launch command failed: {e}")),
            );
            Err(())
        }
    }
}

async fn run_post_launch(params: &LaunchParams) {
    if params.post_launch_command.is_empty() {
        return;
    }
    send_msg(
        &params.queue,
        &params.ctx,
        UiMessage::Status("Running post-launch command...".to_string()),
    );
    if let Err(e) = release_the_launcher_launch::run_post_launch_command(
        &params.post_launch_command,
        &params.instance_root,
    )
    .await
    {
        send_msg(
            &params.queue,
            &params.ctx,
            UiMessage::DownloadError(format!("Post-launch command failed: {e}")),
        );
    }
}

fn send_msg(queue: &Queue, ctx: &egui::Context, msg: UiMessage) {
    if let Ok(mut q) = queue.lock() {
        q.push(msg);
    }
    ctx.request_repaint();
}

fn extract_account_data(account_list: &AccountList) -> Option<(String, String, String)> {
    let active = account_list.active()?;
    let player_name = active.display_name().to_string();
    let player_uuid = active.internal_id.clone();
    let access_token = active
        .mc_token
        .as_ref()
        .map_or(String::new(), |t| t.token.clone());
    Some((player_name, player_uuid, access_token))
}

async fn resolve_components(
    queue: &Queue,
    ctx: &egui::Context,
    loader: &ModLoader,
    mc_version: &str,
) -> Result<Vec<release_the_launcher_launch::Component>, ()> {
    let send = |msg: UiMessage| send_msg(queue, ctx, msg);
    let mut resolver = DependencyResolver::new();

    send(UiMessage::Status(
        "Fetching version manifest...".to_string(),
    ));
    if let Err(e) = resolver.fetch_manifest().await {
        send(UiMessage::DownloadError(format!(
            "Failed to fetch version manifest: {e}"
        )));
        return Err(());
    }

    let mut components = Vec::new();

    match resolver.fetch_vanilla_component(mc_version).await {
        Ok(comp) => components.push(comp),
        Err(e) => {
            send(UiMessage::DownloadError(format!(
                "Failed to fetch Minecraft version: {e}"
            )));
            return Err(());
        }
    }

    match loader {
        ModLoader::Fabric { loader_version } => {
            let lv = loader_version.as_str();
            match resolver.fetch_fabric_component(mc_version, Some(lv)).await {
                Ok(comp) => components.push(comp),
                Err(e) => {
                    send(UiMessage::DownloadError(format!(
                        "Failed to fetch Fabric loader: {e}"
                    )));
                    return Err(());
                }
            }
        }
        ModLoader::Forge { loader_version } => {
            match resolver
                .fetch_forge_component(mc_version, loader_version)
                .await
            {
                Ok(comp) => components.push(comp),
                Err(e) => {
                    send(UiMessage::DownloadError(format!(
                        "Failed to fetch Forge loader: {e}"
                    )));
                    return Err(());
                }
            }
        }
        ModLoader::NeoForge { loader_version } => {
            match resolver
                .fetch_neoforge_component(mc_version, loader_version)
                .await
            {
                Ok(comp) => components.push(comp),
                Err(e) => {
                    send(UiMessage::DownloadError(format!(
                        "Failed to fetch NeoForge loader: {e}"
                    )));
                    return Err(());
                }
            }
        }
        ModLoader::Quilt { loader_version } => {
            let lv = loader_version.as_str();
            match resolver.fetch_quilt_component(mc_version, Some(lv)).await {
                Ok(comp) => components.push(comp),
                Err(e) => {
                    send(UiMessage::DownloadError(format!(
                        "Failed to fetch Quilt loader: {e}"
                    )));
                    return Err(());
                }
            }
        }
        ModLoader::Vanilla => {}
    }

    send(UiMessage::Status("Resolving dependencies...".to_string()));
    let merged =
        release_the_launcher_launch::resolve::resolve_dependencies(&mut resolver, components)
            .await
            .map_err(|e| {
                send(UiMessage::DownloadError(format!(
                    "Failed to resolve dependencies: {e}"
                )));
            })?;

    send(UiMessage::Status("Components resolved.".to_string()));
    Ok(merged)
}

async fn download_game_files(
    queue: &Queue,
    ctx: &egui::Context,
    instance_root: &Path,
    profile: &release_the_launcher_launch::LaunchProfile,
) {
    let send = |msg: UiMessage| send_msg(queue, ctx, msg);
    let dl_manager = DownloadManager::new(instance_root.to_path_buf());

    send(UiMessage::DownloadProgress {
        message: format!("Preparing {} libraries...", profile.libraries.len()),
        done: 0,
        total: 0,
    });

    let progress_queue = queue.clone();
    let progress_ctx = ctx.clone();
    if let Err(e) = dl_manager
        .download_libraries(&profile.libraries, move |done, lib_total, lib_name| {
            let _ = progress_queue.lock().map(|mut q| {
                q.push(UiMessage::DownloadProgress {
                    message: format!("Downloading library: {lib_name}"),
                    done,
                    total: lib_total,
                });
            });
            progress_ctx.request_repaint();
        })
        .await
    {
        send(UiMessage::DownloadError(format!(
            "Failed to download libraries: {e}"
        )));
    }
}

fn extract_natives_files(
    queue: &Queue,
    ctx: &egui::Context,
    instance_root: &Path,
    profile: &release_the_launcher_launch::LaunchProfile,
) {
    let send = |msg: UiMessage| send_msg(queue, ctx, msg);
    let libraries_dir = instance_root.join("libraries");
    let natives_dir = instance_root.join("natives");

    if let Err(e) = release_the_launcher_launch::natives::extract_natives(
        &profile.native_libraries,
        &libraries_dir,
        &natives_dir,
    ) {
        send(UiMessage::DownloadError(format!(
            "Failed to extract natives: {e}"
        )));
    }
}

async fn download_assets(
    queue: &Queue,
    ctx: &egui::Context,
    instance_root: &Path,
    asset_index: &AssetIndex,
) {
    let send = |msg: UiMessage| send_msg(queue, ctx, msg);

    if asset_index.url.is_empty() {
        return;
    }

    let asset_mgr = AssetManager::new(instance_root);
    let http = reqwest::Client::new();

    send(UiMessage::Status("Downloading asset index...".to_string()));
    match asset_mgr
        .download_asset_index(
            &http,
            &asset_index.id,
            &asset_index.url,
            asset_index.sha1.as_deref(),
        )
        .await
    {
        Ok(index_path) => {
            send(UiMessage::Status("Downloading assets...".to_string()));
            let dl_manager = DownloadManager::new(instance_root.to_path_buf());
            let progress_queue = queue.clone();
            let progress_ctx = ctx.clone();
            if let Err(e) = dl_manager
                .download_asset_objects(&http, &index_path, move |done, total, asset_name| {
                    let _ = progress_queue.lock().map(|mut q| {
                        q.push(UiMessage::DownloadProgress {
                            message: format!("Downloading asset: {asset_name}"),
                            done,
                            total,
                        });
                    });
                    progress_ctx.request_repaint();
                })
                .await
            {
                send(UiMessage::DownloadError(format!(
                    "Failed to download assets: {e}"
                )));
            }
        }
        Err(e) => {
            send(UiMessage::DownloadError(format!(
                "Failed to download asset index: {e}"
            )));
        }
    }
}

fn resolve_java_path(
    queue: &Queue,
    ctx: &egui::Context,
    java_path_override: Option<&str>,
    compatible_java_majors: &[u32],
) -> Option<std::path::PathBuf> {
    let send = |msg: UiMessage| send_msg(queue, ctx, msg);

    match release_the_launcher_launch::java::resolve_java(
        java_path_override,
        compatible_java_majors,
    ) {
        Ok(path) => {
            send(UiMessage::Status(format!("Using Java: {}", path.display())));
            Some(path)
        }
        Err(e) => {
            send(UiMessage::DownloadError(format!(
                "Java resolution failed: {e}"
            )));
            None
        }
    }
}
