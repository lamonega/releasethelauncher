use super::profile::LaunchProfile;
use crate::platform;
use crate::LaunchError;
use std::collections::HashSet;
use std::path::Path;
use std::process::ExitStatus;

#[derive(Debug, Clone)]
pub struct PlayerAuth {
    pub name: String,
    pub uuid: String,
    pub access_token: String,
}

pub fn clean_environment(cmd: &mut tokio::process::Command) {
    for var in [
        "_JAVA_OPTIONS",
        "_JAVA_TOOL_OPTIONS",
        "JAVA_TOOL_OPTIONS",
        "CLASSPATH",
        "LD_PRELOAD",
        "LD_LIBRARY_PATH",
    ] {
        cmd.env_remove(var);
    }
}

pub fn set_game_env(cmd: &mut tokio::process::Command, instance_root: &Path, mc_version: &str) {
    cmd.env("INST_DIR", instance_root.display().to_string());
    cmd.env(
        "INST_MC_DIR",
        instance_root.join(".minecraft").display().to_string(),
    );
    cmd.env("INST_ID", mc_version);
    cmd.env("INST_NAME", mc_version);
    cmd.env("INST_JAVA", "");
    cmd.env("NO_COLOR", "1");
}

fn replace_placeholders(
    raw: &str,
    profile: &LaunchProfile,
    instance_dir: &Path,
    player: &PlayerAuth,
    cp_str: &str,
) -> String {
    let mc_dir = instance_dir.join(".minecraft");
    let assets_dir = instance_dir.join("assets");
    let natives_dir = instance_dir.join("natives");
    let token = if player.access_token.is_empty() {
        "0"
    } else {
        &player.access_token
    };

    let mut res = raw.to_string();
    for (key, val) in [
        ("auth_player_name", player.name.as_str()),
        ("auth_uuid", player.uuid.as_str()),
        ("auth_access_token", token),
        ("user_type", "msa"),
        ("version_name", profile.mc_version.as_str()),
        ("version_type", profile.mc_version_type.as_str()),
        ("game_directory", mc_dir.to_str().unwrap_or("")),
        ("assets_root", assets_dir.to_str().unwrap_or("")),
        ("game_assets", assets_dir.to_str().unwrap_or("")),
        ("assets_index_name", profile.asset_index.id.as_str()),
        ("natives_directory", natives_dir.to_str().unwrap_or("")),
        ("classpath", cp_str),
        ("user_properties", "{}"),
        ("client_id", ""),
    ] {
        res = res.replace(&format!("${{{key}}}"), val);
        res = res.replace(&format!("{{{key}}}"), val);
    }
    res
}

#[must_use]
pub fn build_command(
    profile: &LaunchProfile,
    instance_dir: &Path,
    java_path: &Path,
    player: &PlayerAuth,
    memory_min: &str,
    memory_max: &str,
) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(java_path);

    let mc_dir = instance_dir.join(".minecraft");
    std::fs::create_dir_all(&mc_dir).ok();
    cmd.current_dir(&mc_dir);

    clean_environment(&mut cmd);
    set_game_env(&mut cmd, instance_dir, &profile.mc_version);

    let mut classpath = build_classpath(profile, instance_dir);
    let mc_jar = instance_dir
        .join("versions")
        .join(&profile.mc_version)
        .join(format!("{}.jar", profile.mc_version));
    classpath.push(mc_jar.display().to_string());
    let cp_str = classpath.join(platform::classpath_separator());

    let mut has_min_mem = false;
    let mut has_max_mem = false;

    for arg in &profile.jvm_args {
        if arg.starts_with("-Xms") {
            has_min_mem = true;
        }
        if arg.starts_with("-Xmx") {
            has_max_mem = true;
        }
        let processed = replace_placeholders(arg, profile, instance_dir, player, &cp_str);
        if processed.contains("sun-misc-unsafe-memory-access") {
            continue;
        }
        cmd.arg(&processed);
    }

    if !has_min_mem {
        cmd.arg(format!("-Xms{memory_min}"));
    }
    if !has_max_mem {
        cmd.arg(format!("-Xmx{memory_max}"));
    }

    let natives_dir = instance_dir.join("natives");
    cmd.arg(format!("-Djava.library.path={}", natives_dir.display()));

    if !profile.jvm_args.iter().any(|a| a.contains("-cp")) {
        cmd.arg("-cp").arg(&cp_str);
    }

    cmd.arg(&profile.main_class);

    cmd.arg("--add-opens").arg("java.base/java.net=ALL-UNNAMED");

    let game_args_raw = if profile.game_args_template.is_empty() {
        "--username {auth_player_name} --version {version_name} --gameDir {game_directory} --assetsDir {assets_root} --assetIndex {assets_index_name} --uuid {auth_uuid} --accessToken {auth_access_token} --userType msa".to_string()
    } else {
        profile.game_args_template.clone()
    };

    for arg in game_args_raw.split_whitespace() {
        let processed = replace_placeholders(arg, profile, instance_dir, player, &cp_str);
        if processed == "--demo" {
            continue;
        }
        cmd.arg(&processed);
    }

    cmd.arg("--width").arg("854");
    cmd.arg("--height").arg("480");

    cmd
}

fn build_classpath(profile: &LaunchProfile, instance_dir: &Path) -> Vec<String> {
    let mut classpath = Vec::new();
    let mut seen_keys: HashSet<String> = HashSet::new();
    let libraries_dir = instance_dir.join("libraries");
    for lib in profile.libraries.iter().chain(profile.native_libraries.iter()) {
        if !platform::should_include(&lib.rules) {
            continue;
        }
        let parts: Vec<&str> = lib.name.split(':').collect();
        if parts.len() >= 3 {
            let key = format!("{}:{}", parts[0], parts[1]);
            if !seen_keys.insert(key) {
                continue;
            }
            let path = parts[0].replace('.', "/");
            let artifact = parts[1];
            let version = parts[2];
            let classifier = parts.get(3);
            let filename = classifier.map_or_else(
                || format!("{artifact}-{version}.jar"),
                |cls| format!("{artifact}-{version}-{cls}.jar"),
            );
            let jar_path = libraries_dir
                .join(&path)
                .join(artifact)
                .join(version)
                .join(filename);
            classpath.push(jar_path.display().to_string());
        }
    }
    classpath
}

/// # Errors
/// Returns an error if the process fails to spawn or wait.
pub async fn launch_game(command: &mut tokio::process::Command) -> Result<ExitStatus, LaunchError> {
    command
        .spawn()
        .map_err(|e| LaunchError::Launch(e.to_string()))?
        .wait()
        .await
        .map_err(|e| LaunchError::Launch(e.to_string()))
}

/// # Errors
/// Returns an error if the pre-launch command fails.
pub async fn run_pre_launch_command(
    command: &str,
    instance_root: &Path,
) -> Result<(), LaunchError> {
    if command.is_empty() {
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    let status = tokio::process::Command::new("cmd")
        .arg("/C")
        .arg(command)
        .current_dir(instance_root)
        .status()
        .await
        .map_err(|e| LaunchError::Launch(e.to_string()))?;

    #[cfg(not(target_os = "windows"))]
    let status = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(instance_root)
        .status()
        .await
        .map_err(|e| LaunchError::Launch(e.to_string()))?;

    if status.success() {
        Ok(())
    } else {
        Err(LaunchError::Launch(format!(
            "Pre-launch command failed with status: {status}"
        )))
    }
}

/// # Errors
/// This function currently always returns Ok.
pub async fn run_post_launch_command(
    command: &str,
    instance_root: &Path,
) -> Result<(), LaunchError> {
    if command.is_empty() {
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        let _ = tokio::process::Command::new("cmd")
            .arg("/C")
            .arg(command)
            .current_dir(instance_root)
            .status()
            .await;
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(instance_root)
            .status()
            .await;
    }

    Ok(())
}
