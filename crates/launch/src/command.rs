use super::profile::LaunchProfile;
use crate::platform;
use crate::LaunchError;
use std::path::Path;
use std::process::ExitStatus;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

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

    clean_environment(&mut cmd);
    set_game_env(&mut cmd, instance_dir, &profile.mc_version);

    let mut has_min_mem = false;
    let mut has_max_mem = false;

    for arg in &profile.jvm_args {
        if arg.starts_with("-Xms") {
            has_min_mem = true;
        }
        if arg.starts_with("-Xmx") {
            has_max_mem = true;
        }

        let mut processed = arg.replace("{auth_player_name}", &player.name);
        processed = processed.replace("{auth_uuid}", &player.uuid);
        processed = processed.replace("{auth_access_token}", &player.access_token);
        processed = processed.replace("{user_properties}", "{}");
        processed = processed.replace("{client_id}", "");
        processed = processed.replace("{version_type}", &profile.mc_version_type);
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

    let mut classpath = build_classpath(profile);
    let mc_jar = format!(
        "{}/versions/{}/{}.jar",
        instance_dir.display(),
        profile.mc_version,
        profile.mc_version
    );
    classpath.push(mc_jar);

    let cp_str = classpath.join(platform::classpath_separator());
    cmd.arg("-cp").arg(&cp_str);
    cmd.arg(&profile.main_class);

    cmd.arg("--add-opens")
        .arg("java.base/java.net=ALL-UNNAMED");

    let game_args = profile.game_args_template.clone();
    for arg in game_args.split_whitespace() {
        let mut processed = arg.to_string();
        processed = processed.replace("{auth_player_name}", &player.name);
        processed = processed.replace("{auth_uuid}", &player.uuid);
        processed = processed.replace("{auth_access_token}", &player.access_token);
        processed = processed.replace("{user_properties}", "{}");
        processed = processed.replace("{client_id}", "");
        processed = processed.replace("{version_type}", &profile.mc_version_type);
        cmd.arg(&processed);
    }

    cmd.arg("--width").arg("854");
    cmd.arg("--height").arg("480");

    cmd
}

fn build_classpath(profile: &LaunchProfile) -> Vec<String> {
    let mut classpath = Vec::new();
    for lib in &profile.libraries {
        if lib.is_native {
            continue;
        }
        if !platform::should_include(&lib.rules) {
            continue;
        }
        let parts: Vec<&str> = lib.name.split(':').collect();
        if parts.len() >= 3 {
            let path = parts[0].replace('.', "/");
            let artifact = parts[1];
            let version = parts[2];
            let filename = format!("{artifact}-{version}.jar");
            let jar_path = format!("{path}/{artifact}/{version}/{filename}");
            classpath.push(jar_path);
        }
    }
    classpath
}

/// # Errors
/// Returns an error if the process fails to spawn or wait.
pub async fn launch_game(command: &mut tokio::process::Command) -> Result<ExitStatus, LaunchError> {
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);

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
