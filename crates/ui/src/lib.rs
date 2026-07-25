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
use release_the_launcher_core::{InstanceManager, ModLoader};
use release_the_launcher_core::settings::GlobalSettings;
use release_the_launcher_launch::assets::AssetManager;
use release_the_launcher_launch::{
    assemble_launch_profile, build_command, launch_game, AssetIndex, DependencyResolver,
    DownloadManager, PlayerAuth,
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
        done: usize,
        total: usize,
    },
    DownloadComplete(String),
    DownloadError(String),
    ModrinthSearchResult(Result<SearchResults, String>),
    ModrinthInstallResult(Result<String, String>),
    VersionListResult(Result<Vec<String>, String>),
    MsDeviceCode {
        user_code: String,
        verification_uri: String,
        message: String,
    },
    MsLoginSuccess {
        display_name: String,
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
        let settings_path = config_dir.join("settings.toml");

        std::fs::create_dir_all(&config_dir).ok();
        std::fs::create_dir_all(&instances_dir).ok();

        let instance_manager = InstanceManager::discover(instances_dir).unwrap_or_else(|e| {
            tracing::warn!("Failed to discover instances: {e}");
            InstanceManager::discover(std::path::PathBuf::from("/dev/null")).unwrap()
        });

        let account_list = AccountList::load(&accounts_path);
        let global_settings = GlobalSettings::load(&settings_path);

        let theme = Theme::default();

        Self {
            instance_manager,
            account_list,
            current_view: View::InstanceList,
            log_buffer: LogBuffer::new(),
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
                    let versions: Vec<String> = resolver.available_versions();
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

async fn do_launch(params: LaunchParams) {
    let send = |msg: UiMessage| send_msg(&params.queue, &params.ctx, msg);

    let Some((player_name, player_uuid, access_token)) = params.account_data else {
        send(UiMessage::DownloadError(
            "No active account. Add an account before launching.".to_string(),
        ));
        return;
    };

    if !params.pre_launch_command.is_empty() {
        send(UiMessage::Status("Running pre-launch command...".to_string()));
        if let Err(e) = release_the_launcher_launch::run_pre_launch_command(
            &params.pre_launch_command,
            &params.instance_root,
        )
        .await
        {
            send(UiMessage::DownloadError(format!(
                "Pre-launch command failed: {e}"
            )));
            return;
        }
    }

    let Ok(components) =
        resolve_components(&params.queue, &params.ctx, &params.loader, &params.mc_version).await
    else {
        return;
    };

    let profile = match assemble_launch_profile(&components) {
        Ok(p) => p,
        Err(e) => {
            send(UiMessage::DownloadError(format!(
                "Failed to assemble profile: {e}"
            )));
            return;
        }
    };

    let asset_index = profile.asset_index.clone();

    download_game_files(&params.queue, &params.ctx, &params.instance_root, &profile).await;
    extract_natives_files(&params.queue, &params.ctx, &params.instance_root, &profile);
    download_assets(&params.queue, &params.ctx, &params.instance_root, &asset_index).await;

    let Some(java_path) = resolve_java_path(
        &params.queue,
        &params.ctx,
        params.java_path_override.as_deref(),
        &profile.compatible_java_majors,
    ) else {
        return;
    };

    let mut cmd = build_command(
        &profile,
        &params.instance_root,
        &java_path,
        &PlayerAuth {
            name: player_name,
            uuid: player_uuid,
            access_token,
        },
        &params.memory_min,
        &params.memory_max,
    );

    send(UiMessage::Status("Launching game...".to_string()));
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    match launch_game(&mut cmd).await {
        Ok(status) => {
            send(UiMessage::DownloadComplete(format!(
                "Game exited with status: {status}"
            )));
        }
        Err(e) => {
            send(UiMessage::DownloadError(format!(
                "Failed to launch game: {e}"
            )));
        }
    }

    if !params.post_launch_command.is_empty() {
        send(UiMessage::Status("Running post-launch command...".to_string()));
        if let Err(e) = release_the_launcher_launch::run_post_launch_command(
            &params.post_launch_command,
            &params.instance_root,
        )
        .await
        {
            send(UiMessage::DownloadError(format!(
                "Post-launch command failed: {e}"
            )));
        }
    }

    if params.close_after_launch {
        params.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
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

    let total = profile.libraries.len();
    send(UiMessage::DownloadProgress {
        message: format!("Downloading {total} libraries..."),
        done: 0,
        total,
    });

    let progress_queue = queue.clone();
    let progress_ctx = ctx.clone();
    if let Err(e) = dl_manager
        .download_libraries(&profile.libraries, move |done, lib_total| {
            let _ = progress_queue.lock().map(|mut q| {
                q.push(UiMessage::DownloadProgress {
                    message: "Downloading libraries...".to_string(),
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
            if let Err(e) = dl_manager.download_asset_objects(&http, &index_path).await {
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
