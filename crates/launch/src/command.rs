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

pub fn clean_environment(cmd: &mut std::process::Command) {
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

pub fn set_game_env(cmd: &mut std::process::Command, instance_root: &Path, mc_version: &str) {
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
    let mc_dir_str = mc_dir.display().to_string();
    let assets_dir_str = assets_dir.display().to_string();
    let natives_dir_str = natives_dir.display().to_string();
    let is_offline = player.access_token.is_empty()
        || player.access_token == "0"
        || player.access_token == "offline";
    let token = if is_offline {
        "0"
    } else {
        &player.access_token
    };
    let user_type = if is_offline { "legacy" } else { "msa" };

    let mut res = raw.to_string();
    for (key, val) in [
        ("auth_player_name", player.name.as_str()),
        ("auth_uuid", player.uuid.as_str()),
        ("auth_access_token", token),
        ("auth_session_id", token),
        ("auth_session", token), // legacy: used in minecraftArguments (pre-1.13)
        ("user_type", user_type),
        ("version_name", profile.mc_version.as_str()),
        ("version_type", profile.mc_version_type.as_str()),
        ("game_directory", mc_dir_str.as_str()),
        ("assets_root", assets_dir_str.as_str()),
        ("game_assets", assets_dir_str.as_str()),
        ("assets_index_name", profile.asset_index.id.as_str()),
        ("natives_directory", natives_dir_str.as_str()),
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
#[allow(clippy::too_many_lines)] // flat java arg builder, splitting adds nothing
pub fn build_command(
    profile: &LaunchProfile,
    instance_dir: &Path,
    java_path: &Path,
    player: &PlayerAuth,
    memory_min: &str,
    memory_max: &str,
) -> std::process::Command {
    let mut cmd = std::process::Command::new(java_path);
    cmd.current_dir(instance_dir);

    clean_environment(&mut cmd);
    set_game_env(&mut cmd, instance_dir, &profile.mc_version);

    let mut classpath = build_classpath(profile, instance_dir);
    let mc_jar = instance_dir
        .join("versions")
        .join(&profile.mc_version)
        .join(format!("{}.jar", profile.mc_version));
    classpath.push(mc_jar.display().to_string());
    let cp_str = classpath.join(crate::platform::classpath_separator());

    let mut has_min_mem = false;
    let mut has_max_mem = false;
    let mut has_java_lib_path = false;
    let mut has_lwjgl_lib_path = false;

    for arg in &profile.jvm_args {
        if arg.starts_with("-Xms") {
            has_min_mem = true;
        }
        if arg.starts_with("-Xmx") {
            has_max_mem = true;
        }
        let processed = replace_placeholders(arg, profile, instance_dir, player, &cp_str);
        if processed.starts_with("-Djava.library.path=") {
            has_java_lib_path = true;
        }
        if processed.starts_with("-Dorg.lwjgl.librarypath=") {
            has_lwjgl_lib_path = true;
        }
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
    if !has_java_lib_path {
        cmd.arg(format!("-Djava.library.path={}", natives_dir.display()));
    }
    if !has_lwjgl_lib_path {
        cmd.arg(format!("-Dorg.lwjgl.librarypath={}", natives_dir.display()));
    }

    if !profile.jvm_args.iter().any(|a| a.contains("-cp")) {
        cmd.arg("-cp").arg(&cp_str);
    }
    let java_major = crate::java::detect_java_major_version(java_path).unwrap_or(8);
    if java_major >= 9
        && !profile
            .jvm_args
            .iter()
            .any(|a| a.contains("java.base/java.net"))
    {
        cmd.arg("--add-opens").arg("java.base/java.net=ALL-UNNAMED");
    }

    cmd.arg(&profile.main_class);

    let game_args_raw = if profile.game_args_template.is_empty() {
        "--username {auth_player_name} --version {version_name} --gameDir {game_directory} --assetsDir {assets_root} --assetIndex {assets_index_name} --uuid {auth_uuid} --accessToken {auth_access_token} --userType {user_type}".to_string()
    } else {
        profile.game_args_template.clone()
    };

    let mut has_tweak_class = game_args_raw.contains("--tweakClass");

    for arg in game_args_raw.split_whitespace() {
        let processed = replace_placeholders(arg, profile, instance_dir, player, &cp_str);
        if processed == "--demo"
            || processed.starts_with("--quickPlay")
            || processed.contains("${quickPlay")
            || (processed.starts_with("${") && processed.ends_with('}'))
        {
            continue;
        }
        if processed == "--tweakClass" {
            has_tweak_class = true;
        }
        cmd.arg(&processed);
    }

    if !has_tweak_class
        && (profile.main_class == "net.minecraft.launchwrapper.Launch"
            || profile.traits.iter().any(|t| t == "legacyFML"))
    {
        if profile.tweakers.is_empty() {
            let tweak = if profile.mc_version.starts_with("1.8")
                || profile.mc_version.starts_with("1.9")
                || profile.mc_version.starts_with("1.10")
                || profile.mc_version.starts_with("1.11")
                || profile.mc_version.starts_with("1.12")
            {
                "net.minecraftforge.fml.common.launcher.FMLTweaker"
            } else {
                "cpw.mods.fml.common.launcher.FMLTweaker"
            };
            cmd.arg("--tweakClass").arg(tweak);
        } else {
            for tweak in &profile.tweakers {
                cmd.arg("--tweakClass").arg(tweak);
            }
        }
    }

    cmd.arg("--width").arg("854");
    cmd.arg("--height").arg("480");

    cmd
}

fn build_classpath(profile: &LaunchProfile, instance_dir: &Path) -> Vec<String> {
    let mut classpath = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let libraries_dir = instance_dir.join("libraries");
    for lib in profile
        .libraries
        .iter()
        .chain(profile.native_libraries.iter())
    {
        if !platform::should_include_library(lib) {
            continue;
        }
        let parts: Vec<&str> = lib.name.split(':').collect();
        if parts.len() >= 3 {
            if !seen.insert(lib.name.clone()) {
                continue;
            }
            let group = parts[0].replace('.', "/");
            let artifact = parts[1];
            let version = parts[2];
            let filename = crate::download::library_filename(lib);

            let jar_path = libraries_dir
                .join(&group)
                .join(artifact)
                .join(version)
                .join(filename);

            let is_jarmod = lib.name.contains("@zip")
                || lib.name.contains("net.minecraftforge:forge")
                || std::path::Path::new(lib.url.as_deref().unwrap_or(""))
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"));
            if is_jarmod {
                classpath.insert(0, jar_path.display().to_string());
            } else {
                classpath.push(jar_path.display().to_string());
            }
        }
    }
    classpath
}

/// # Errors
/// Returns an error if the process fails to spawn or wait.
pub async fn launch_game(command: &mut std::process::Command) -> Result<ExitStatus, LaunchError> {
    let mut command = std::mem::replace(command, std::process::Command::new(""));
    tokio::task::spawn_blocking(move || {
        command
            .spawn()
            .map_err(|e| LaunchError::Launch(e.to_string()))?
            .wait()
            .map_err(|e| LaunchError::Launch(e.to_string()))
    })
    .await
    .map_err(|e| LaunchError::Launch(e.to_string()))?
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_build_command_natives_and_placeholders() {
        let profile = LaunchProfile {
            mc_version: "1.20.1".to_string(),
            mc_version_type: "release".to_string(),
            main_class: "net.minecraft.client.main.Main".to_string(),
            libraries: Vec::new(),
            native_libraries: Vec::new(),
            asset_index: crate::profile::AssetIndex {
                id: "1.20".to_string(),
                url: String::new(),
                sha1: None,
                size: 0,
            },
            client_download: None,
            jvm_args: vec![
                "-Djava.library.path=${natives_directory}".to_string(),
                "-Dcustom.prop={natives_directory}".to_string(),
            ],
            game_args_template: String::new(),
            traits: Vec::new(),
            tweakers: Vec::new(),
            compatible_java_majors: vec![17],
        };

        let instance_dir = PathBuf::from("/tmp/test_instance");
        let java_path = PathBuf::from("/usr/bin/java");
        let player = PlayerAuth {
            name: "TestUser".to_string(),
            uuid: "00000000-0000-0000-0000-000000000000".to_string(),
            access_token: "testtoken".to_string(),
        };

        let cmd = build_command(&profile, &instance_dir, &java_path, &player, "1G", "2G");
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();

        let expected_natives = instance_dir.join("natives").display().to_string();

        let java_lib_args: Vec<&String> = args
            .iter()
            .filter(|a| a.starts_with("-Djava.library.path="))
            .collect();
        assert_eq!(
            java_lib_args.len(),
            1,
            "Should have exactly one -Djava.library.path argument"
        );
        assert_eq!(
            java_lib_args[0],
            &format!("-Djava.library.path={expected_natives}")
        );

        let lwjgl_lib_args: Vec<&String> = args
            .iter()
            .filter(|a| a.starts_with("-Dorg.lwjgl.librarypath="))
            .collect();
        assert_eq!(
            lwjgl_lib_args.len(),
            1,
            "Should have exactly one -Dorg.lwjgl.librarypath argument"
        );
        assert_eq!(
            lwjgl_lib_args[0],
            &format!("-Dorg.lwjgl.librarypath={expected_natives}")
        );

        assert!(
            args.contains(&format!("-Dcustom.prop={expected_natives}")),
            "Placeholder {{natives_directory}} should be replaced correctly"
        );
    }
}
