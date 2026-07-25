use std::path::Path;
use std::process::Command;
use crate::LaunchError;
use super::profile::LaunchProfile;

#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn build_command(
    profile: &LaunchProfile,
    instance_dir: &Path,
    java_path: &Path,
    player_name: &str,
    player_uuid: &str,
    access_token: &str,
    memory_min: &str,
    memory_max: &str,
) -> Command {
    let mut cmd = Command::new(java_path);

    let mut has_min_mem = false;
    let mut has_max_mem = false;

    for arg in &profile.jvm_args {
        if arg.starts_with("-Xms") { has_min_mem = true; }
        if arg.starts_with("-Xmx") { has_max_mem = true; }

        let mut processed = arg.replace("{auth_player_name}", player_name);
        processed = processed.replace("{auth_uuid}", player_uuid);
        processed = processed.replace("{auth_access_token}", access_token);
        processed = processed.replace("{user_properties}", "{}");
        processed = processed.replace("{client_id}", "");
        processed = processed.replace("{version_type}", &profile.mc_version_type);
        cmd.arg(&processed);
    }

    if !has_min_mem { cmd.arg(format!("-Xms{memory_min}")); }
    if !has_max_mem { cmd.arg(format!("-Xmx{memory_max}")); }

    let natives_dir = instance_dir.join("natives");
    cmd.arg(format!("-Djava.library.path={}", natives_dir.display()));

    let mut classpath = Vec::new();
    for lib in &profile.libraries {
        if lib.is_native { continue; }
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

    let mc_jar = format!("{}/versions/{}/{}.jar",
        instance_dir.display(), profile.mc_version, profile.mc_version);
    classpath.push(mc_jar);

    let cp_str = classpath.join(":");
    cmd.arg("-cp").arg(&cp_str);
    cmd.arg(&profile.main_class);

    let game_args = profile.game_args_template.clone();
    for arg in game_args.split_whitespace() {
        let mut processed = arg.to_string();
        processed = processed.replace("{auth_player_name}", player_name);
        processed = processed.replace("{auth_uuid}", player_uuid);
        processed = processed.replace("{auth_access_token}", access_token);
        processed = processed.replace("{user_properties}", "{}");
        processed = processed.replace("{client_id}", "");
        processed = processed.replace("{version_type}", &profile.mc_version_type);
        cmd.arg(&processed);
    }

    cmd.arg("--width").arg("854");
    cmd.arg("--height").arg("480");

    cmd
}

/// # Errors
/// Returns an error if the process fails to spawn or wait.
pub async fn launch_game(command: &mut Command) -> Result<std::process::ExitStatus, LaunchError> {
    command.spawn()
        .map_err(|e| LaunchError::Launch(e.to_string()))?
        .wait()
        .map_err(|e| LaunchError::Launch(e.to_string()))
}
