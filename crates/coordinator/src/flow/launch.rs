use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;

use release_the_launcher_auth::AccountList;
use release_the_launcher_core::settings::ModLoader;
use release_the_launcher_launch::assets::AssetManager;
use release_the_launcher_launch::{
    assemble_launch_profile, build_command, ensure_fml_deobfuscation_data, AssetIndex,
    DependencyResolver, DownloadManager, LaunchProfile, PlayerAuth,
};

use crate::log::LogLevel;
use crate::{push_event, Event, Queue};

pub struct LaunchParams {
    pub queue: Queue,
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
    pub http: reqwest::Client,
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

pub fn send_log(queue: &Queue, level: LogLevel, message: impl Into<String>) {
    send_log_with_target(queue, level, message, "launcher");
}

pub fn send_log_with_target(
    queue: &Queue,
    level: LogLevel,
    message: impl Into<String>,
    target: impl Into<String>,
) {
    push_event(
        queue,
        Event::Log(crate::log::LogEntry {
            timestamp: chrono::Local::now()
                .format(release_the_launcher_constants::defaults::TIMESTAMP_FORMAT)
                .to_string(),
            level,
            message: message.into(),
            target: target.into(),
        }),
    );
}

pub async fn do_launch(params: LaunchParams) {
    let send = |msg: Event| push_event(&params.queue, msg);

    send_log(
        &params.queue,
        LogLevel::Info,
        format!(
            "=== Launching Instance: {} ===",
            params.instance_root.display()
        ),
    );

    let Some((ref player_name, ref player_uuid, ref access_token)) = params.account_data else {
        let err = "No active account. Add an account before launching.".to_string();
        send_log(&params.queue, LogLevel::Error, &err);
        send(Event::DownloadError(err));
        return;
    };

    send_log(
        &params.queue,
        LogLevel::Info,
        format!("Account: {player_name} (UUID: {player_uuid})"),
    );
    send_log(
        &params.queue,
        LogLevel::Info,
        format!(
            "Minecraft: {}, Loader: {}",
            params.mc_version, params.loader
        ),
    );

    if run_pre_launch(&params).await.is_err() {
        send_log(&params.queue, LogLevel::Error, "Pre-launch command failed");
        return;
    }

    download_modpack_mods(&params).await;

    let Ok(profile) = resolve_and_prepare_downloads(&params).await else {
        return;
    };

    send_log(&params.queue, LogLevel::Info, "Resolving Java runtime...");
    let Some(java_path) = resolve_java_path(
        &params.queue,
        params.java_path_override.as_deref(),
        &profile.compatible_java_majors,
    ) else {
        send_log(
            &params.queue,
            LogLevel::Error,
            "Failed to resolve Java path",
        );
        return;
    };

    let cmd = build_launch_command(
        &params,
        &profile,
        &java_path,
        player_name,
        player_uuid,
        access_token,
    );

    spawn_and_monitor_game(&params, cmd).await;
}

fn build_launch_command(
    params: &LaunchParams,
    profile: &LaunchProfile,
    java_path: &std::path::Path,
    player_name: &str,
    player_uuid: &str,
    access_token: &str,
) -> std::process::Command {
    send_log(
        &params.queue,
        LogLevel::Info,
        format!("Java executable: {}", java_path.display()),
    );

    let cmd = build_command(
        profile,
        &params.instance_root,
        java_path,
        &PlayerAuth {
            name: player_name.to_string(),
            uuid: player_uuid.to_string(),
            access_token: access_token.to_string(),
        },
        &params.memory_min,
        &params.memory_max,
    );

    send_log(
        &params.queue,
        LogLevel::Info,
        format!("Java Command Line: {cmd:?}"),
    );

    cmd
}

async fn resolve_and_prepare_downloads(params: &LaunchParams) -> Result<LaunchProfile, ()> {
    let send = |msg: Event| push_event(&params.queue, msg);
    send_log(
        &params.queue,
        LogLevel::Info,
        "Resolving version components & manifests...",
    );

    let components =
        match resolve_components(&params.queue, &params.loader, &params.mc_version).await {
            Ok(c) => c,
            Err(e) => {
                let msg = format!("Failed to resolve version components: {e}");
                send_log(&params.queue, LogLevel::Error, &msg);
                send(Event::DownloadError(msg));
                return Err(());
            }
        };

    send_log(
        &params.queue,
        LogLevel::Info,
        format!("Resolved {} component(s)", components.len()),
    );

    let profile = match assemble_launch_profile(&components) {
        Ok(p) => p,
        Err(e) => {
            let msg = format!("Failed to assemble profile: {e}");
            send_log(&params.queue, LogLevel::Error, &msg);
            send(Event::DownloadError(msg));
            return Err(());
        }
    };

    send_log(
        &params.queue,
        LogLevel::Info,
        format!(
            "Profile ready: MainClass='{}', {} libraries, {} native libraries",
            profile.main_class,
            profile.libraries.len(),
            profile.native_libraries.len()
        ),
    );

    send_log(
        &params.queue,
        LogLevel::Info,
        "Checking & downloading required game libraries...",
    );
    download_game_files(&params.queue, &params.instance_root, &profile).await;

    send_log(
        &params.queue,
        LogLevel::Info,
        "Checking legacy FML runtime libraries...",
    );
    if let Err(e) = ensure_fml_deobfuscation_data(&profile, &params.instance_root).await {
        send_log(
            &params.queue,
            LogLevel::Error,
            format!("Failed to prepare FML libraries: {e}"),
        );
    }

    send_log(
        &params.queue,
        LogLevel::Info,
        "Extracting native libraries...",
    );
    extract_natives_files(&params.queue, &params.instance_root, &profile);

    send_log(
        &params.queue,
        LogLevel::Info,
        "Checking & downloading game assets...",
    );
    download_assets(&params.queue, &params.instance_root, &profile.asset_index, &params.http).await;

    download_client_jar(params, &profile).await?;

    Ok(profile)
}

async fn download_client_jar(params: &LaunchParams, profile: &LaunchProfile) -> Result<(), ()> {
    let Some(client_dl) = profile.client_download.as_ref() else {
        return Ok(());
    };
    if client_dl.url.is_empty() {
        return Ok(());
    }
    let client_jar = params
        .instance_root
        .join("versions")
        .join(&profile.mc_version)
        .join(format!("{}.jar", profile.mc_version));
    if client_jar.exists() {
        return Ok(());
    }
    send_log(
        &params.queue,
        LogLevel::Info,
        format!("Downloading Minecraft client: {}.jar", profile.mc_version),
    );
    push_event(
        &params.queue,
        Event::Status(format!(
            "Downloading Minecraft {}.jar...",
            profile.mc_version
        )),
    );
    let dl_mgr = DownloadManager::new(params.instance_root.clone());
    if let Err(e) = dl_mgr
        .download_client_jar(&client_jar, &client_dl.url, client_dl.sha1.as_deref())
        .await
    {
        let msg = format!("Failed to download client.jar: {e}");
        send_log(&params.queue, LogLevel::Error, &msg);
        push_event(&params.queue, Event::DownloadError(msg));
        return Err(());
    }
    Ok(())
}

/// Spawns the game and streams its output. Reading happens on std threads:
/// tokio's child pipes on Windows lose everything but the first stderr line
/// when the process dies right after writing (javaw crash after ~67 of 950
/// bytes); sync reads drain the pipe before the OS cancels the pending I/O.
async fn spawn_and_monitor_game(params: &LaunchParams, cmd: std::process::Command) {
    push_event(
        &params.queue,
        Event::Status("Launching game...".to_string()),
    );

    let queue = params.queue.clone();
    let instance_id = params.instance_id.clone();
    let result =
        tokio::task::spawn_blocking(move || spawn_and_stream_output(cmd, &queue, &instance_id))
            .await
            .unwrap_or_else(|_| Err("Log streaming task failed".to_string()));

    match result {
        Ok(status) => {
            let msg = format!("Game process exited with status: {status}");
            let level = if status.success() {
                LogLevel::Info
            } else {
                LogLevel::Error
            };
            send_log(&params.queue, level, &msg);
            push_event(&params.queue, Event::DownloadComplete(msg));
        }
        Err(e) => {
            let msg = format!("Failed to run game process: {e}");
            send_log(&params.queue, LogLevel::Error, &msg);
            push_event(&params.queue, Event::DownloadError(msg));
        }
    }

    run_post_launch(params).await;

    if params.close_after_launch {
        push_event(&params.queue, Event::RequestClose);
    }
}

fn spawn_and_stream_output(
    mut cmd: std::process::Command,
    queue: &Queue,
    instance_id: &str,
) -> Result<std::process::ExitStatus, String> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;

    push_event(
        queue,
        Event::DownloadComplete("Game is running".to_string()),
    );
    send_log(
        queue,
        LogLevel::Info,
        "Game process started successfully. Streaming logs...",
    );

    let target = format!("instance:{instance_id}");
    let mut readers = Vec::new();
    if let Some(out) = child.stdout.take() {
        readers.push(spawn_line_reader(
            out,
            Arc::clone(queue),
            target.clone(),
            true,
        ));
    }
    if let Some(err) = child.stderr.take() {
        readers.push(spawn_line_reader(err, Arc::clone(queue), target, false));
    }

    let status = child.wait().map_err(|e| e.to_string())?;
    for reader in readers {
        let _ = reader.join();
    }
    Ok(status)
}

fn spawn_line_reader<R: std::io::Read + Send + 'static>(
    stream: R,
    queue: Queue,
    target: String,
    is_stdout: bool,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        use std::io::BufRead;
        for line in std::io::BufReader::new(stream).lines() {
            let Ok(line) = line else { break };
            let level = if is_stdout {
                LogLevel::Info
            } else {
                stderr_level(&line)
            };
            push_event(
                &queue,
                Event::Log(crate::log::LogEntry {
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                    level,
                    message: line,
                    target: target.clone(),
                }),
            );
        }
    })
}

fn stderr_level(line: &str) -> LogLevel {
    if line.contains("ERROR")
        || line.contains("Error")
        || line.contains("EXCEPTION")
        || line.contains("Exception")
        || line.contains("FATAL")
        || line.contains("Fatal")
        || line.contains("SEVERE")
        || line.contains("Severe")
    {
        LogLevel::Error
    } else if line.contains("WARN") || line.contains("Warn") || line.contains("WARNING") {
        LogLevel::Warn
    } else {
        LogLevel::Info
    }
}

async fn download_modpack_mods(params: &LaunchParams) {
    let index_path = params.instance_root.join(release_the_launcher_constants::paths::MODRINTH_INDEX_FILE);
    if !index_path.exists() {
        return;
    }
    send_log(&params.queue, LogLevel::Info, "Downloading modpack mods...");
    let mod_manager = release_the_launcher_mods::ModrinthProvider::with_client(params.http.clone(), None);
    let progress_queue = params.queue.clone();
    if let Err(e) = mod_manager
        .download_modpack_files(&params.instance_root, move |done, total, mod_name| {
            let _ = progress_queue.lock().map(|mut q| {
                q.push(Event::DownloadProgress {
                    message: format!("Downloading mod: {mod_name}"),
                    done,
                    total,
                });
            });
        })
        .await
    {
        send_log(
            &params.queue,
            LogLevel::Warn,
            &format!("Modpack download encountered errors: {e}"),
        );
    }
}

async fn run_pre_launch(params: &LaunchParams) -> Result<(), ()> {
    if params.pre_launch_command.is_empty() {
        return Ok(());
    }
    push_event(
        &params.queue,
        Event::Status("Running pre-launch command...".to_string()),
    );
    match release_the_launcher_launch::run_pre_launch_command(
        &params.pre_launch_command,
        &params.instance_root,
    )
    .await
    {
        Ok(()) => Ok(()),
        Err(e) => {
            push_event(
                &params.queue,
                Event::DownloadError(format!("Pre-launch command failed: {e}")),
            );
            Err(())
        }
    }
}

async fn run_post_launch(params: &LaunchParams) {
    if params.post_launch_command.is_empty() {
        return;
    }
    push_event(
        &params.queue,
        Event::Status("Running post-launch command...".to_string()),
    );
    if let Err(e) = release_the_launcher_launch::run_post_launch_command(
        &params.post_launch_command,
        &params.instance_root,
    )
    .await
    {
        push_event(
            &params.queue,
            Event::DownloadError(format!("Post-launch command failed: {e}")),
        );
    }
}

async fn resolve_components(
    queue: &Queue,
    loader: &ModLoader,
    mc_version: &str,
) -> Result<Vec<release_the_launcher_launch::Component>, String> {
    let send = |msg: Event| push_event(queue, msg);
    let mut resolver = DependencyResolver::new();

    send(Event::Status("Fetching version manifest...".to_string()));
    if let Err(e) = resolver.fetch_manifest().await {
        let err_msg = format!("Failed to fetch version manifest: {e}");
        send(Event::DownloadError(err_msg.clone()));
        return Err(err_msg);
    }

    let mut components = Vec::new();

    match resolver.fetch_vanilla_component(mc_version).await {
        Ok(comp) => components.push(comp),
        Err(e) => {
            let err_msg = format!("Failed to fetch Minecraft version: {e}");
            send(Event::DownloadError(err_msg.clone()));
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
                    send(Event::DownloadError(err_msg.clone()));
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
                    send(Event::DownloadError(err_msg.clone()));
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
                    send(Event::DownloadError(err_msg.clone()));
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
                    send(Event::DownloadError(err_msg.clone()));
                    return Err(err_msg);
                }
            }
        }
        ModLoader::Vanilla => {}
    }

    send(Event::Status("Resolving dependencies...".to_string()));
    let merged =
        release_the_launcher_launch::resolve::resolve_dependencies(&mut resolver, components)
            .await
            .map_err(|e| {
                let err_msg = format!("Failed to resolve dependencies: {e}");
                send(Event::DownloadError(err_msg.clone()));
                err_msg
            })?;

    send(Event::Status("Components resolved.".to_string()));
    Ok(merged)
}

async fn download_game_files(queue: &Queue, instance_root: &Path, profile: &LaunchProfile) {
    let send = |msg: Event| push_event(queue, msg);
    let dl_manager = DownloadManager::new(instance_root.to_path_buf());

    let mut all_libraries = profile.libraries.clone();
    all_libraries.extend(profile.native_libraries.clone());

    send(Event::DownloadProgress {
        message: format!("Preparing {} libraries...", all_libraries.len()),
        done: 0,
        total: 0,
    });

    let progress_queue = Arc::clone(queue);
    if let Err(e) = dl_manager
        .download_libraries(&all_libraries, move |done, lib_total, lib_name| {
            let _ = progress_queue.lock().map(|mut q| {
                q.push(Event::DownloadProgress {
                    message: format!("Downloading library: {lib_name}"),
                    done,
                    total: lib_total,
                });
            });
        })
        .await
    {
        send(Event::DownloadError(format!(
            "Failed to download libraries: {e}"
        )));
    }
}

fn extract_natives_files(queue: &Queue, instance_root: &Path, profile: &LaunchProfile) {
    let send = |msg: Event| push_event(queue, msg);
    let send_log = |level: LogLevel, msg: String| {
        push_event(
            queue,
            Event::Log(crate::log::LogEntry {
                timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                level,
                message: msg,
                target: "launcher".to_string(),
            }),
        );
    };
    let libraries_dir = instance_root.join("libraries");
    let natives_dir = instance_root.join("natives");

    send_log(
        LogLevel::Info,
        format!(
            "Extracting {} native libraries",
            profile.native_libraries.len()
        ),
    );
    for lib in &profile.native_libraries {
        send_log(
            LogLevel::Info,
            format!("  native: {} url={:?}", lib.name, lib.url),
        );
    }

    if let Err(e) = release_the_launcher_launch::natives::extract_natives(
        &profile.native_libraries,
        &libraries_dir,
        &natives_dir,
    ) {
        send(Event::DownloadError(format!(
            "Failed to extract natives: {e}"
        )));
    } else {
        let count = release_the_launcher_launch::natives::verify_natives_dir(&natives_dir);
        send_log(
            LogLevel::Info,
            format!(
                "Extracted {count} native dynamic library binaries to {}",
                natives_dir.display()
            ),
        );
    }
}

async fn download_assets(queue: &Queue, instance_root: &Path, asset_index: &AssetIndex, http: &reqwest::Client) {
    let send = |msg: Event| push_event(queue, msg);

    if asset_index.url.is_empty() {
        send_log(
            queue,
            LogLevel::Warn,
            "Asset index URL is empty, skipping asset download",
        );
        return;
    }

    let asset_mgr = AssetManager::new(instance_root);

    send(Event::Status("Downloading asset index...".to_string()));
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
            send(Event::Status("Downloading assets...".to_string()));
            let dl_manager = DownloadManager::new(instance_root.to_path_buf());
            let progress_queue = Arc::clone(queue);
            if let Err(e) = dl_manager
                .download_asset_objects(&http, &index_path, move |done, total, asset_name| {
                    let _ = progress_queue.lock().map(|mut q| {
                        q.push(Event::DownloadProgress {
                            message: format!("Downloading asset: {asset_name}"),
                            done,
                            total,
                        });
                    });
                })
                .await
            {
                send(Event::DownloadError(format!(
                    "Failed to download assets: {e}"
                )));
                return;
            }

            send(Event::Status(
                "Reconstructing virtual assets...".to_string(),
            ));
            match asset_mgr.parse_asset_index(&index_path) {
                Ok(parsed_index) => {
                    let mc_dir = instance_root.join(".minecraft");
                    if let Err(e) = asset_mgr.reconstruct_virtual_assets(&mc_dir, &parsed_index) {
                        send_log(
                            queue,
                            LogLevel::Warn,
                            format!("Failed to reconstruct virtual assets: {e}"),
                        );
                    }
                }
                Err(e) => {
                    send_log(
                        queue,
                        LogLevel::Warn,
                        format!("Failed to parse asset index for virtual reconstruction: {e}"),
                    );
                }
            }
        }
        Err(e) => {
            send(Event::DownloadError(format!(
                "Failed to download asset index: {e}"
            )));
        }
    }
}

fn resolve_java_path(
    queue: &Queue,
    java_path_override: Option<&str>,
    compatible_java_majors: &[u32],
) -> Option<std::path::PathBuf> {
    let send = |msg: Event| push_event(queue, msg);

    match release_the_launcher_launch::java::resolve_java(
        java_path_override,
        compatible_java_majors,
    ) {
        Ok(path) => {
            send(Event::Status(format!("Using Java: {}", path.display())));
            Some(path)
        }
        Err(e) => {
            send(Event::DownloadError(format!("Java resolution failed: {e}")));
            None
        }
    }
}

pub fn fetch_versions_list(queue: &Queue, handle: &tokio::runtime::Handle) {
    let queue = Arc::clone(queue);
    handle.spawn(async move {
        let mut resolver = DependencyResolver::new();
        let result = match resolver.fetch_manifest().await {
            Ok(()) => {
                let versions: Vec<(String, String)> = resolver.available_versions_with_types();
                Ok(versions)
            }
            Err(e) => Err(e.to_string()),
        };
        push_event(&queue, Event::VersionListResult(result));
    });
}

pub fn fetch_loader_versions(
    queue: &Queue,
    handle: &tokio::runtime::Handle,
    loader_type: &str,
    mc_version: &str,
) {
    let queue = Arc::clone(queue);
    let loader_type = loader_type.to_string();
    let mc_version = mc_version.to_string();
    handle.spawn(async move {
        let resolver = DependencyResolver::new();
        let result = resolver
            .fetch_loader_versions(&loader_type, &mc_version)
            .await
            .map_err(|e| e.to_string());
        push_event(
            &queue,
            Event::LoaderVersionsResult {
                loader_type,
                mc_version,
                result,
            },
        );
    });
}
