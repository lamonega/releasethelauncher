use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use release_the_launcher_auth::AccountList;
use release_the_launcher_core::settings::ModLoader;
use release_the_launcher_launch::assets::AssetManager;
use release_the_launcher_launch::{
    assemble_launch_profile, build_command, AssetIndex, DependencyResolver, DownloadManager,
    PlayerAuth,
};

use crate::services::modrinth::send_msg;
use crate::{UiMessage, Queue};

pub struct LaunchParams {
    pub queue: Queue,
    pub ctx: egui::Context,
    pub account_data: Option<(String, String, String)>,
    pub instance_id: String,
    pub instance_root: std::path::PathBuf,
    pub mc_version: String,
    pub loader: ModLoader,
    pub java_path_override: Option<String>,
    pub memory_min: String,
    pub memory_max: String,
    pub pre_launch_command: String,
    pub post_launch_command: String,
    pub close_after_launch: bool,
}

pub fn extract_account_data(account_list: &AccountList) -> Option<(String, String, String)> {
    let active = account_list.active()?;
    let player_name = active.display_name().to_string();
    let player_uuid = active.internal_id.clone();
    let access_token = active
        .mc_token
        .as_ref()
        .map_or_else(String::new, |t| t.token.clone());
    Some((player_name, player_uuid, access_token))
}

pub fn send_log(
    queue: &Arc<Mutex<Vec<UiMessage>>>,
    ctx: &egui::Context,
    level: crate::log::LogLevel,
    message: impl Into<String>,
) {
    send_log_with_target(queue, ctx, level, message, "launcher");
}

pub fn send_log_with_target(
    queue: &Arc<Mutex<Vec<UiMessage>>>,
    ctx: &egui::Context,
    level: crate::log::LogLevel,
    message: impl Into<String>,
    target: impl Into<String>,
) {
    if let Ok(mut q) = queue.lock() {
        q.push(UiMessage::Log(crate::log::LogEntry {
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
            level,
            message: message.into(),
            target: target.into(),
        }));
    }
    ctx.request_repaint();
}

pub async fn do_launch(params: LaunchParams) {
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
        format!("Account: {player_name} (UUID: {player_uuid})"),
    );
    send_log(
        &params.queue,
        &params.ctx,
        crate::log::LogLevel::Info,
        format!(
            "Minecraft: {}, Loader: {}",
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

    let components = match resolve_components(
        &params.queue,
        &params.ctx,
        &params.loader,
        &params.mc_version,
    )
    .await
    {
        Ok(c) => c,
        Err(e) => {
            let msg = format!("Failed to resolve version components: {e}");
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
            "Profile ready: MainClass='{}', {} libraries, {} native libraries",
            profile.main_class,
            profile.libraries.len(),
            profile.native_libraries.len()
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

    let instance_target = format!("instance:{}", params.instance_id);
    let queue_out = params.queue.clone();
    let ctx_out = params.ctx.clone();
    let target_out = instance_target.clone();
    let out_handle = stdout.map(|stdout| {
        tokio::spawn(async move {
            use tokio::io::AsyncBufReadExt;
            let mut reader = tokio::io::BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if let Ok(mut q) = queue_out.lock() {
                    q.push(UiMessage::Log(crate::log::LogEntry {
                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                        level: crate::log::LogLevel::Info,
                        message: line,
                        target: target_out.clone(),
                    }));
                }
                ctx_out.request_repaint();
            }
        })
    });

    let queue_err = params.queue.clone();
    let ctx_err = params.ctx.clone();
    let target_err = instance_target;
    let err_handle = stderr.map(|stderr| {
        tokio::spawn(async move {
            use tokio::io::AsyncBufReadExt;
            let mut reader = tokio::io::BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let level = if line.contains("ERROR")
                    || line.contains("Error")
                    || line.contains("EXCEPTION")
                    || line.contains("Exception")
                    || line.contains("FATAL")
                    || line.contains("Fatal")
                    || line.contains("SEVERE")
                    || line.contains("Severe")
                {
                    crate::log::LogLevel::Error
                } else if line.contains("WARN") || line.contains("Warn") || line.contains("WARNING") {
                    crate::log::LogLevel::Warn
                } else {
                    crate::log::LogLevel::Info
                };
                if let Ok(mut q) = queue_err.lock() {
                    q.push(UiMessage::Log(crate::log::LogEntry {
                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                        level,
                        message: line,
                        target: target_err.clone(),
                    }));
                }
                ctx_err.request_repaint();
            }
        })
    });

    let status = child.wait().await;
    if let Some(h) = out_handle {
        let _ = h.await;
    }
    if let Some(h) = err_handle {
        let _ = h.await;
    }

    match status {
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

async fn resolve_components(
    queue: &Queue,
    ctx: &egui::Context,
    loader: &ModLoader,
    mc_version: &str,
) -> Result<Vec<release_the_launcher_launch::Component>, String> {
    let send = |msg: UiMessage| send_msg(queue, ctx, msg);
    let mut resolver = DependencyResolver::new();

    send(UiMessage::Status(
        "Fetching version manifest...".to_string(),
    ));
    if let Err(e) = resolver.fetch_manifest().await {
        let err_msg = format!("Failed to fetch version manifest: {e}");
        send(UiMessage::DownloadError(err_msg.clone()));
        return Err(err_msg);
    }

    let mut components = Vec::new();

    match resolver.fetch_vanilla_component(mc_version).await {
        Ok(comp) => components.push(comp),
        Err(e) => {
            let err_msg = format!("Failed to fetch Minecraft version: {e}");
            send(UiMessage::DownloadError(err_msg.clone()));
            return Err(err_msg);
        }
    }

    match loader {
        ModLoader::Fabric { loader_version } => {
            let lv = loader_version.as_str();
            match resolver.fetch_fabric_component(mc_version, Some(lv)).await {
                Ok(comp) => components.push(comp),
                Err(e) => {
                    let err_msg = format!("Failed to fetch Fabric loader: {e}");
                    send(UiMessage::DownloadError(err_msg.clone()));
                    return Err(err_msg);
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
                    let err_msg = format!("Failed to fetch Forge loader: {e}");
                    send(UiMessage::DownloadError(err_msg.clone()));
                    return Err(err_msg);
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
                    let err_msg = format!("Failed to fetch NeoForge loader: {e}");
                    send(UiMessage::DownloadError(err_msg.clone()));
                    return Err(err_msg);
                }
            }
        }
        ModLoader::Quilt { loader_version } => {
            let lv = loader_version.as_str();
            match resolver.fetch_quilt_component(mc_version, Some(lv)).await {
                Ok(comp) => components.push(comp),
                Err(e) => {
                    let err_msg = format!("Failed to fetch Quilt loader: {e}");
                    send(UiMessage::DownloadError(err_msg.clone()));
                    return Err(err_msg);
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
                let err_msg = format!("Failed to resolve dependencies: {e}");
                send(UiMessage::DownloadError(err_msg.clone()));
                err_msg
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

    let mut all_libraries = profile.libraries.clone();
    all_libraries.extend(profile.native_libraries.clone());

    send(UiMessage::DownloadProgress {
        message: format!("Preparing {} libraries...", all_libraries.len()),
        done: 0,
        total: 0,
    });

    let progress_queue = queue.clone();
    let progress_ctx = ctx.clone();
    if let Err(e) = dl_manager
        .download_libraries(&all_libraries, move |done, lib_total, lib_name| {
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
    let send_log = |level: crate::log::LogLevel, msg: String| send_msg(queue, ctx, UiMessage::Log(crate::log::LogEntry {
        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
        level,
        message: msg,
        target: "launcher".to_string(),
    }));
    let libraries_dir = instance_root.join("libraries");
    let natives_dir = instance_root.join("natives");

    send_log(crate::log::LogLevel::Info, format!("Extracting {} native libraries", profile.native_libraries.len()));
    for lib in &profile.native_libraries {
        send_log(crate::log::LogLevel::Info, format!("  native: {} url={:?}", lib.name, lib.url));
    }

    if let Err(e) = release_the_launcher_launch::natives::extract_natives(
        &profile.native_libraries,
        &libraries_dir,
        &natives_dir,
    ) {
        send(UiMessage::DownloadError(format!(
            "Failed to extract natives: {e}"
        )));
    } else {
        let count = release_the_launcher_launch::natives::verify_natives_dir(&natives_dir);
        send_log(
            crate::log::LogLevel::Info,
            format!("Extracted {count} native dynamic library binaries to {}", natives_dir.display()),
        );
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
        send_log(
            queue,
            ctx,
            crate::log::LogLevel::Warn,
            "Asset index URL is empty, skipping asset download",
        );
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
                return;
            }

            send(UiMessage::Status("Reconstructing virtual assets...".to_string()));
            match asset_mgr.parse_asset_index(&index_path) {
                Ok(parsed_index) => {
                    let mc_dir = instance_root.join(".minecraft");
                    if let Err(e) = asset_mgr.reconstruct_virtual_assets(&mc_dir, &parsed_index) {
                        send_log(
                            queue,
                            ctx,
                            crate::log::LogLevel::Warn,
                            format!("Failed to reconstruct virtual assets: {e}"),
                        );
                    }
                }
                Err(e) => {
                    send_log(
                        queue,
                        ctx,
                        crate::log::LogLevel::Warn,
                        format!("Failed to parse asset index for virtual reconstruction: {e}"),
                    );
                }
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

pub fn fetch_versions_list(
    queue: Queue,
    ctx: egui::Context,
    handle: tokio::runtime::Handle,
) {
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

pub fn fetch_loader_versions(
    queue: Queue,
    ctx: egui::Context,
    handle: tokio::runtime::Handle,
    loader_type: String,
    mc_version: String,
) {
    handle.spawn(async move {
        let resolver = DependencyResolver::new();
        let result = resolver
            .fetch_loader_versions(&loader_type, &mc_version)
            .await
            .map_err(|e| e.to_string());
        send_msg(
            &queue,
            &ctx,
            UiMessage::LoaderVersionsResult {
                loader_type,
                mc_version,
                result,
            },
        );
    });
}
