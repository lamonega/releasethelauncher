use std::path::Path;
use std::process::Stdio;

use release_the_launcher_auth::AccountList;
use release_the_launcher_core::settings::ModLoader;
use release_the_launcher_launch::assets::AssetManager;
use release_the_launcher_launch::{
    assemble_launch_profile, build_command, ensure_fml_deobfuscation_data, AssetIndex,
    DependencyResolver, DownloadManager, LaunchProfile, PlayerAuth,
};

use crate::log::LogLevel;
use crate::{push_event, Event, Queue};

#[derive(Clone, Debug)]
pub struct AccountData {
    pub name: String,
    pub uuid: String,
    pub token: String,
}

pub struct LaunchParams {
    pub queue: Queue,
    pub account_data: Option<AccountData>,
    pub active_auth_account: Option<release_the_launcher_auth::AccountData>,
    pub http_client: reqwest::Client,
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

pub fn extract_account_data(account_list: &AccountList) -> Option<AccountData> {
    let active = account_list.active()?;
    Some(AccountData {
        name: active.display_name().to_string(),
        uuid: active.internal_id.clone(),
        token: active
            .mc_token
            .as_ref()
            .map_or_else(String::new, |t| t.token.clone()),
    })
}

fn fail(queue: &Queue, msg: impl Into<String>) -> anyhow::Error {
    let message = msg.into();
    send_log(queue, LogLevel::Error, &message);
    anyhow::anyhow!(message)
}

fn emit_progress(queue: &Queue, message: String, done: u64, total: u64) {
    push_event(
        queue,
        Event::DownloadProgress {
            message,
            done,
            total,
        },
    );
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

pub async fn do_launch(mut params: LaunchParams) {
    send_log(
        &params.queue,
        LogLevel::Info,
        format!(
            "=== Launching Instance: {} ===",
            params.instance_root.display()
        ),
    );

    if let Err(err) = do_launch_pipeline(&mut params).await {
        push_event(&params.queue, Event::DownloadError(err.to_string()));
    }
}

async fn do_launch_pipeline(params: &mut LaunchParams) -> Result<(), anyhow::Error> {
    // 1. Refresh Microsoft token if near expiry
    if let Some(ref mut auth_account) = params.active_auth_account {
        if release_the_launcher_auth::refresh::needs_refresh(auth_account) {
            send_log(
                &params.queue,
                LogLevel::Info,
                "Refreshing Microsoft account session before launch...",
            );
            let client_id = release_the_launcher_constants::urls::DEFAULT_MSA_CLIENT_ID;
            match release_the_launcher_auth::refresh::try_refresh_if_needed(
                auth_account,
                &params.http_client,
                client_id,
            )
            .await
            {
                Ok(Some(refreshed)) => {
                    params.account_data = Some(AccountData {
                        name: refreshed.display_name().to_string(),
                        uuid: refreshed.internal_id.clone(),
                        token: refreshed
                            .mc_token
                            .as_ref()
                            .map_or_else(String::new, |t| t.token.clone()),
                    });
                    push_event(
                        &params.queue,
                        Event::MsLoginSuccess {
                            account: Box::new(refreshed),
                        },
                    );
                    push_event(
                        &params.queue,
                        Event::Status("Account refreshed".to_string()),
                    );
                }
                Ok(None) => {}
                Err(e) => {
                    return Err(fail(
                        &params.queue,
                        format!("Session expired: re-login required ({e})"),
                    ));
                }
            }
        }
    }

    // 2. Validate account data
    let account = params.account_data.as_ref().ok_or_else(|| {
        fail(
            &params.queue,
            "No active account. Add an account before launching.",
        )
    })?;

    let player_name = account.name.clone();
    let player_uuid = account.uuid.clone();
    let access_token = account.token.clone();

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

    // 3. Pre-launch memory check
    let memory_max_mb = parse_memory_mb(&params.memory_max);
    if !release_the_launcher_launch::memory::has_enough_memory(memory_max_mb) {
        send_log(
            &params.queue,
            LogLevel::Warn,
            format!("Low system memory warning: requested {memory_max_mb} MB but available physical RAM is lower."),
        );
        push_event(
            &params.queue,
            Event::Status("Warning: system memory may be insufficient".to_string()),
        );
    }

    // 4. Pre-launch command
    run_pre_launch(params).await?;

    // 5. Download modpack mods
    download_modpack_mods(params).await;

    // 6. Resolve version components & profile
    let profile = resolve_and_prepare_downloads(params).await?;

    // 7. Resolve Java runtime
    send_log(&params.queue, LogLevel::Info, "Resolving Java runtime...");
    let java_path = resolve_java_path(
        &params.queue,
        params.java_path_override.as_deref(),
        &profile.compatible_java_majors,
    )?;

    // 8. Build command
    let cmd = build_launch_command(
        params,
        &profile,
        &java_path,
        &player_name,
        &player_uuid,
        &access_token,
    );

    // 9. Spawn and monitor process
    spawn_and_monitor_game(params, cmd).await?;

    Ok(())
}

fn parse_memory_mb(mem_str: &str) -> u64 {
    let s = mem_str.trim();
    if s.ends_with('G') || s.ends_with('g') {
        s[..s.len() - 1].parse::<u64>().unwrap_or(2048) * 1024
    } else if s.ends_with('M') || s.ends_with('m') {
        s[..s.len() - 1].parse::<u64>().unwrap_or(2048)
    } else {
        s.parse::<u64>().unwrap_or(2048)
    }
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

async fn resolve_and_prepare_downloads(
    params: &LaunchParams,
) -> Result<LaunchProfile, anyhow::Error> {
    send_log(
        &params.queue,
        LogLevel::Info,
        "Resolving version components & manifests...",
    );

    let components = resolve_components(&params.queue, &params.loader, &params.mc_version).await?;

    send_log(
        &params.queue,
        LogLevel::Info,
        format!("Resolved {} component(s)", components.len()),
    );

    let profile = assemble_launch_profile(&components)
        .map_err(|e| fail(&params.queue, format!("Failed to assemble profile: {e}")))?;

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
    download_game_files(&params.queue, &params.instance_root, &profile).await?;

    send_log(
        &params.queue,
        LogLevel::Info,
        "Checking legacy FML runtime libraries...",
    );
    if let Err(e) = ensure_fml_deobfuscation_data(&profile, &params.instance_root).await {
        send_log(
            &params.queue,
            LogLevel::Warn,
            format!("Failed to prepare FML libraries: {e}"),
        );
    }

    send_log(
        &params.queue,
        LogLevel::Info,
        "Extracting native libraries...",
    );
    extract_natives_files(&params.queue, &params.instance_root, &profile)?;

    send_log(
        &params.queue,
        LogLevel::Info,
        "Checking & downloading game assets...",
    );
    download_assets(&params.queue, &params.instance_root, &profile.asset_index).await?;

    download_client_jar(params, &profile).await?;

    Ok(profile)
}

async fn download_client_jar(
    params: &LaunchParams,
    profile: &LaunchProfile,
) -> Result<(), anyhow::Error> {
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
    dl_mgr
        .download_client_jar(&client_jar, &client_dl.url, client_dl.sha1.as_deref())
        .await
        .map_err(|e| fail(&params.queue, format!("Failed to download client.jar: {e}")))
}

async fn spawn_and_monitor_game(
    params: &LaunchParams,
    cmd: std::process::Command,
) -> Result<(), anyhow::Error> {
    push_event(
        &params.queue,
        Event::Status("Launching game...".to_string()),
    );

    let queue = params.queue.clone();
    let instance_id = params.instance_id.clone();
    let result =
        tokio::task::spawn_blocking(move || spawn_and_stream_output(cmd, &queue, &instance_id))
            .await
            .map_err(|_| fail(&params.queue, "Log streaming task failed"))?;

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
            return Err(fail(
                &params.queue,
                format!("Failed to run game process: {e}"),
            ));
        }
    }

    run_post_launch(params).await;

    if params.close_after_launch {
        push_event(&params.queue, Event::RequestClose);
    }
    Ok(())
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
        readers.push(spawn_line_reader(out, queue.clone(), target.clone(), true));
    }
    if let Some(err) = child.stderr.take() {
        readers.push(spawn_line_reader(err, queue.clone(), target, false));
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
    let index_path = params
        .instance_root
        .join(release_the_launcher_constants::paths::MODRINTH_INDEX_FILE);
    if !index_path.exists() {
        return;
    }
    send_log(&params.queue, LogLevel::Info, "Downloading modpack mods...");
    let mod_manager =
        release_the_launcher_mods::ModrinthProvider::with_client(params.http_client.clone(), None);
    let progress_queue = params.queue.clone();
    if let Err(e) = mod_manager
        .download_modpack_files(&params.instance_root, move |done, total, mod_name| {
            emit_progress(
                &progress_queue,
                format!("Downloading mod: {mod_name}"),
                done,
                total,
            );
        })
        .await
    {
        send_log(
            &params.queue,
            LogLevel::Warn,
            format!("Modpack download encountered errors: {e}"),
        );
    }
}

async fn run_pre_launch(params: &LaunchParams) -> Result<(), anyhow::Error> {
    if params.pre_launch_command.is_empty() {
        return Ok(());
    }
    push_event(
        &params.queue,
        Event::Status("Running pre-launch command...".to_string()),
    );
    release_the_launcher_launch::run_pre_launch_command(
        &params.pre_launch_command,
        &params.instance_root,
    )
    .await
    .map_err(|e| fail(&params.queue, format!("Pre-launch command failed: {e}")))
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
        send_log(
            &params.queue,
            LogLevel::Error,
            format!("Post-launch command failed: {e}"),
        );
    }
}

async fn resolve_components(
    queue: &Queue,
    loader: &ModLoader,
    mc_version: &str,
) -> Result<Vec<release_the_launcher_launch::Component>, anyhow::Error> {
    let mut resolver = DependencyResolver::new();

    push_event(
        queue,
        Event::Status("Fetching version manifest...".to_string()),
    );
    resolver
        .fetch_manifest()
        .await
        .map_err(|e| fail(queue, format!("Failed to fetch version manifest: {e}")))?;

    let mut components = Vec::new();

    let vanilla_comp = resolver
        .fetch_vanilla_component(mc_version)
        .await
        .map_err(|e| fail(queue, format!("Failed to fetch Minecraft version: {e}")))?;
    components.push(vanilla_comp);

    let loader_comp = match loader {
        ModLoader::Fabric { loader_version } => resolver
            .fetch_fabric_component(mc_version, Some(loader_version.as_str()))
            .await
            .map(Some),
        ModLoader::Forge { loader_version } => resolver
            .fetch_forge_component(mc_version, loader_version)
            .await
            .map(Some),
        ModLoader::NeoForge { loader_version } => resolver
            .fetch_neoforge_component(mc_version, loader_version)
            .await
            .map(Some),
        ModLoader::Quilt { loader_version } => resolver
            .fetch_quilt_component(mc_version, Some(loader_version.as_str()))
            .await
            .map(Some),
        ModLoader::Vanilla => Ok(None),
    };

    if let Some(comp) =
        loader_comp.map_err(|e| fail(queue, format!("Failed to fetch {loader} loader: {e}")))?
    {
        components.push(comp);
    }

    push_event(
        queue,
        Event::Status("Resolving dependencies...".to_string()),
    );
    let merged =
        release_the_launcher_launch::resolve::resolve_dependencies(&mut resolver, components)
            .await
            .map_err(|e| fail(queue, format!("Failed to resolve dependencies: {e}")))?;

    push_event(queue, Event::Status("Components resolved.".to_string()));
    Ok(merged)
}

async fn download_game_files(
    queue: &Queue,
    instance_root: &Path,
    profile: &LaunchProfile,
) -> Result<(), anyhow::Error> {
    let dl_manager = DownloadManager::new(instance_root.to_path_buf());

    let mut all_libraries = profile.libraries.clone();
    all_libraries.extend(profile.native_libraries.clone());

    emit_progress(
        queue,
        format!("Preparing {} libraries...", all_libraries.len()),
        0,
        0,
    );

    let progress_queue = queue.clone();
    dl_manager
        .download_libraries(&all_libraries, move |done, lib_total, lib_name| {
            emit_progress(
                &progress_queue,
                format!("Downloading library: {lib_name}"),
                done,
                lib_total,
            );
        })
        .await
        .map_err(|e| fail(queue, format!("Failed to download libraries: {e}")))
}

fn extract_natives_files(
    queue: &Queue,
    instance_root: &Path,
    profile: &LaunchProfile,
) -> Result<(), anyhow::Error> {
    let libraries_dir = instance_root.join("libraries");
    let natives_dir = instance_root.join("natives");

    send_log(
        queue,
        LogLevel::Info,
        format!(
            "Extracting {} native libraries",
            profile.native_libraries.len()
        ),
    );
    for lib in &profile.native_libraries {
        send_log(
            queue,
            LogLevel::Info,
            format!("  native: {} url={:?}", lib.name, lib.url),
        );
    }

    if let Err(e) = release_the_launcher_launch::natives::extract_natives(
        &profile.native_libraries,
        &libraries_dir,
        &natives_dir,
    ) {
        Err(fail(queue, format!("Failed to extract natives: {e}")))
    } else {
        let count = release_the_launcher_launch::natives::verify_natives_dir(&natives_dir);
        send_log(
            queue,
            LogLevel::Info,
            format!(
                "Extracted {count} native dynamic library binaries to {}",
                natives_dir.display()
            ),
        );
        Ok(())
    }
}

async fn download_assets(
    queue: &Queue,
    instance_root: &Path,
    asset_index: &AssetIndex,
) -> Result<(), anyhow::Error> {
    if asset_index.url.is_empty() {
        send_log(
            queue,
            LogLevel::Warn,
            "Asset index URL is empty, skipping asset download",
        );
        return Ok(());
    }

    let asset_mgr = AssetManager::new(instance_root);
    let http = release_the_launcher_net::default_client();

    push_event(
        queue,
        Event::Status("Downloading asset index...".to_string()),
    );
    let index_path = asset_mgr
        .download_asset_index(
            &http,
            &asset_index.id,
            &asset_index.url,
            asset_index.sha1.as_deref(),
        )
        .await
        .map_err(|e| fail(queue, format!("Failed to download asset index: {e}")))?;

    push_event(queue, Event::Status("Downloading assets...".to_string()));
    let dl_manager = DownloadManager::new(instance_root.to_path_buf());
    let progress_queue = queue.clone();
    dl_manager
        .download_asset_objects(&http, &index_path, move |done, total, asset_name| {
            emit_progress(
                &progress_queue,
                format!("Downloading asset: {asset_name}"),
                done,
                total,
            );
        })
        .await
        .map_err(|e| fail(queue, format!("Failed to download assets: {e}")))?;

    push_event(
        queue,
        Event::Status("Reconstructing virtual assets...".to_string()),
    );
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
    Ok(())
}

fn resolve_java_path(
    queue: &Queue,
    java_path_override: Option<&str>,
    compatible_java_majors: &[u32],
) -> Result<std::path::PathBuf, anyhow::Error> {
    match release_the_launcher_launch::java::resolve_java(
        java_path_override,
        compatible_java_majors,
    ) {
        Ok(path) => {
            push_event(
                queue,
                Event::Status(format!("Using Java: {}", path.display())),
            );
            Ok(path)
        }
        Err(e) => Err(fail(queue, format!("Java resolution failed: {e}"))),
    }
}

pub async fn fetch_versions_list(queue: Queue) {
    let mut resolver = DependencyResolver::new();
    let result = match resolver.fetch_manifest().await {
        Ok(()) => {
            let versions: Vec<(String, String)> = resolver.available_versions_with_types();
            Ok(versions)
        }
        Err(e) => Err(e.to_string()),
    };
    push_event(&queue, Event::VersionListResult(result));
}

pub async fn fetch_loader_versions(queue: Queue, loader_type: String, mc_version: String) {
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
}
