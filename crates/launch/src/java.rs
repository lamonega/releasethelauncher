use std::path::{Path, PathBuf};

use crate::LaunchError;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Detects a Java installation and validates its major version against the compatible list.
///
/// Checks, in order:
/// 1. The per-instance `java_path` override from settings.
/// 2. The `JAVA_HOME` environment variable.
/// 3. Windows Registry entries for known Java vendors.
/// 4. `java`/`javaw` on `PATH`.
///
/// Returns the path to a valid Java executable, or a [`LaunchError::JavaNotFound`].
///
/// # Errors
///
/// Returns [`LaunchError::JavaNotFound`] if no suitable Java is found or the detected
/// version is incompatible with `compatible_java_majors`.
pub fn resolve_java(
    instance_java_path: Option<&str>,
    compatible_java_majors: &[u32],
) -> Result<PathBuf, LaunchError> {
    if let Some(path_str) = instance_java_path {
        let path = PathBuf::from(path_str);
        if path.exists() {
            return validate_java(&path, compatible_java_majors);
        }
    }

    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        let exe_name = java_executable_name();
        let path = PathBuf::from(&java_home).join("bin").join(exe_name);
        if path.exists() {
            return validate_java(&path, compatible_java_majors);
        }
    }

    #[cfg(target_os = "windows")]
    {
        let candidates = find_java_from_registry();
        for path in candidates {
            if path.exists() {
                if let Ok(validated) = validate_java(&path, compatible_java_majors) {
                    return Ok(validated);
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let home = dirs::home_dir().unwrap_or_default();
        let dev_paths = [
            home.join(".jdks"),
            home.join(".sdkman/candidates/java"),
            home.join(".gradle/jdks"),
        ];
        for base in &dev_paths {
            if base.exists() {
                if let Ok(entries) = std::fs::read_dir(base) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        let exe = path.join("bin").join(java_executable_name());
                        if exe.exists() {
                            if let Ok(validated) = validate_java(&exe, compatible_java_majors) {
                                return Ok(validated);
                            }
                        }
                    }
                }
            }
        }
    }

    {
        if let Some(path) = find_bundled_java(compatible_java_majors) {
            return Ok(path);
        }
    }

    let exe_name = java_executable_name();
    let find_cmd = if cfg!(windows) { "where" } else { "which" };
    if let Ok(output) = quiet_command(find_cmd, &[exe_name]) {
        if output.status.success() {
            let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            for line in path_str.lines() {
                let path = PathBuf::from(line.trim());
                if path.exists() {
                    if let Ok(validated) = validate_java(&path, compatible_java_majors) {
                        return Ok(validated);
                    }
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let fallbacks = [
            r"C:\Program Files\Java\jre8\bin\javaw.exe",
            r"C:\Program Files\Java\jre1.8.0\bin\javaw.exe",
            r"C:\Program Files (x86)\Java\jre8\bin\javaw.exe",
            r"C:\Program Files\Java\jdk-17\bin\java.exe",
            r"C:\Program Files\Java\jdk-21\bin\java.exe",
        ];
        for fallback in &fallbacks {
            let path = PathBuf::from(fallback);
            if path.exists() {
                if let Ok(validated) = validate_java(&path, compatible_java_majors) {
                    return Ok(validated);
                }
            }
        }
    }

    Err(LaunchError::JavaNotFound(
        "No Java installation found. Set JAVA_HOME or add java to PATH.".to_string(),
    ))
}

const fn java_executable_name() -> &'static str {
    if cfg!(windows) {
        "javaw.exe"
    } else {
        "java"
    }
}

fn find_bundled_java(compatible_java_majors: &[u32]) -> Option<PathBuf> {
    let mc_runtimes = if cfg!(windows) {
        let local = dirs::data_local_dir().unwrap_or_default();
        vec![
            local.join("Packages/Microsoft.4297127D64EC6_8wekyb3d8bbwe/LocalCache/Local/packages/Microsoft.MinecraftUWP_8wekyb3d8bbwe/LocalState/runtime"),
            dirs::home_dir().unwrap_or_default().join(".minecraft/runtime"),
        ]
    } else if cfg!(target_os = "macos") {
        vec![dirs::home_dir()
            .unwrap_or_default()
            .join("Library/Application Support/minecraft/runtime")]
    } else {
        vec![dirs::home_dir()
            .unwrap_or_default()
            .join(".minecraft/runtime")]
    };

    for runtime_dir in &mc_runtimes {
        if runtime_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(runtime_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let exe = if cfg!(windows) {
                        path.join("bin/javaw.exe")
                    } else {
                        path.join("bin/java")
                    };
                    if exe.exists() {
                        if let Ok(validated) = validate_java(&exe, compatible_java_majors) {
                            return Some(validated);
                        }
                    }
                }
            }
        }
    }
    None
}

fn quiet_command(program: &str, args: &[&str]) -> Result<std::process::Output, std::io::Error> {
    let mut cmd = std::process::Command::new(program);
    cmd.args(args);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.output()
}

#[cfg(target_os = "windows")]
fn find_java_from_registry() -> Vec<PathBuf> {
    use winreg::enums::{
        HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
    };
    use winreg::RegKey;

    let mut candidates = Vec::new();

    let subkeys = [
        r"SOFTWARE\JavaSoft\Java Runtime Environment",
        r"SOFTWARE\JavaSoft\Java Development Kit",
        r"SOFTWARE\JavaSoft\JRE",
        r"SOFTWARE\JavaSoft\JDK",
        r"SOFTWARE\Eclipse Adoptium\JRE",
        r"SOFTWARE\Eclipse Adoptium\JDK",
        r"SOFTWARE\Eclipse Foundation\JDK",
        r"SOFTWARE\AdoptOpenJDK\JRE",
        r"SOFTWARE\AdoptOpenJDK\JDK",
        r"SOFTWARE\Microsoft\JDK",
        r"SOFTWARE\Azul Systems\Zulu",
        r"SOFTWARE\BellSoft\Liberica",
        r"SOFTWARE\Semeru\JRE",
        r"SOFTWARE\Semeru\JDK",
    ];

    let value_names = ["JavaHome", "Path", "InstallationPath"];
    let hives = [
        RegKey::predef(HKEY_LOCAL_MACHINE),
        RegKey::predef(HKEY_CURRENT_USER),
    ];
    let views = [KEY_READ | KEY_WOW64_64KEY, KEY_READ | KEY_WOW64_32KEY];

    for hive in &hives {
        for view in &views {
            for subkey_path in &subkeys {
                if let Ok(subkey) = hive.open_subkey_with_flags(subkey_path, *view) {
                    for version_name in subkey.enum_keys().filter_map(std::result::Result::ok) {
                        if let Ok(version_key) = subkey.open_subkey(&version_name) {
                            for value_name in &value_names {
                                if let Ok(java_home) =
                                    version_key.get_value::<String, _>(value_name)
                                {
                                    let bin_dir = PathBuf::from(&java_home).join("bin");
                                    let exe = java_executable_name();
                                    let candidate = bin_dir.join(exe);
                                    if !candidates.contains(&candidate) {
                                        candidates.push(candidate);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    candidates
}

fn validate_java(java_path: &Path, compatible_java_majors: &[u32]) -> Result<PathBuf, LaunchError> {
    detect_java_major_version(java_path).map_or_else(
        || {
            Err(LaunchError::JavaNotFound(format!(
                "Could not determine Java version at: {}",
                java_path.display()
            )))
        },
        |major| {
            if compatible_java_majors.is_empty() || compatible_java_majors.contains(&major) {
                Ok(java_path.to_path_buf())
            } else {
                Err(LaunchError::JavaNotFound(format!(
                    "Found Java {major} but this version requires one of: {compatible_java_majors:?}",
                )))
            }
        },
    )
}

fn detect_java_major_version(java_path: &Path) -> Option<u32> {
    let output = quiet_command(java_path.to_str()?, &["-version"]).ok()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    parse_java_version_output(&stderr)
}

fn parse_java_version_output(output: &str) -> Option<u32> {
    let version_line = output.lines().next()?;
    let version_str = version_line
        .trim_start_matches("java version \"")
        .trim_start_matches("openjdk version \"")
        .trim_matches('"');

    let first_part: &str = version_str.split('.').next()?;
    let major: u32 = first_part.parse().ok()?;

    if major == 1 {
        let second = version_str.split('.').nth(1)?;
        second.parse().ok()
    } else {
        Some(major)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_new_style_version() {
        assert_eq!(
            parse_java_version_output("openjdk version \"17.0.2\" 2022-07-19"),
            Some(17)
        );
    }

    #[test]
    fn parse_java_21() {
        assert_eq!(
            parse_java_version_output("openjdk version \"21.0.3\" 2024-04-16"),
            Some(21)
        );
    }

    #[test]
    fn parse_old_style_version() {
        assert_eq!(
            parse_java_version_output("java version \"1.8.0_362\""),
            Some(8)
        );
    }

    #[test]
    fn parse_empty_string() {
        assert_eq!(parse_java_version_output(""), None);
    }

    #[test]
    fn parse_non_java_output() {
        assert_eq!(
            parse_java_version_output("Error: could not find java"),
            None
        );
    }
}
