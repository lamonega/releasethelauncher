use std::path::{Path, PathBuf};

use crate::LaunchError;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

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

    Err(LaunchError::JavaNotFound(
        "No Java installation found. Set JAVA_HOME or add java to PATH.".to_string(),
    ))
}

fn java_executable_name() -> &'static str {
    if cfg!(windows) {
        "javaw.exe"
    } else {
        "java"
    }
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
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
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

    for subkey_path in &subkeys {
        if let Ok(subkey) = hklm.open_subkey(subkey_path) {
            for version_name in subkey.enum_keys().filter_map(|k| k.ok()) {
                if let Ok(version_key) = subkey.open_subkey(&version_name) {
                    if let Ok(java_home) = version_key.get_value::<String, _>("JavaHome") {
                        let bin_dir = PathBuf::from(&java_home).join("bin");
                        let exe = java_executable_name();
                        candidates.push(bin_dir.join(exe));
                    }
                }
            }
        }
    }

    candidates
}

fn validate_java(java_path: &Path, compatible_java_majors: &[u32]) -> Result<PathBuf, LaunchError> {
    if let Some(major) = detect_java_major_version(java_path) {
        if compatible_java_majors.is_empty() || compatible_java_majors.contains(&major) {
            Ok(java_path.to_path_buf())
        } else {
            Err(LaunchError::JavaNotFound(format!(
                "Found Java {major} but this version requires one of: {compatible_java_majors:?}",
            )))
        }
    } else {
        Err(LaunchError::JavaNotFound(format!(
            "Could not determine Java version at: {}",
            java_path.display()
        )))
    }
}

fn detect_java_major_version(java_path: &Path) -> Option<u32> {
    let output = quiet_command(
        java_path.to_str()?,
        &["-version"],
    )
    .ok()?;
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
