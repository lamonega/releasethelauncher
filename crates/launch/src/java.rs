use std::path::{Path, PathBuf};

use crate::LaunchError;

/// Detects a Java installation and validates its major version against the compatible list.
///
/// Checks, in order:
/// 1. The per-instance `java_path` override from settings.
/// 2. The `JAVA_HOME` environment variable.
/// 3. `java` on `PATH`.
///
/// Returns the path to a valid `java` executable, or a [`LaunchError::JavaNotFound`].
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

    let exe_name = java_executable_name();
    if let Ok(output) = std::process::Command::new("which").arg(exe_name).output() {
        if output.status.success() {
            let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path_str.is_empty() {
                let path = PathBuf::from(&path_str);
                if path.exists() {
                    return validate_java(&path, compatible_java_majors);
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
        "java.exe"
    } else {
        "java"
    }
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
    let output = std::process::Command::new(java_path)
        .arg("-version")
        .output()
        .ok()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    parse_java_version_output(&stderr)
}

fn parse_java_version_output(output: &str) -> Option<u32> {
    let version_line = output.lines().next()?;
    // Handle both old-style: "1.8.0_xxx" and new-style: "17.0.x", "21.0.x"
    let version_str = version_line
        .trim_start_matches("java version \"")
        .trim_start_matches("openjdk version \"")
        .trim_matches('"');

    let first_part: &str = version_str.split('.').next()?;
    let major: u32 = first_part.parse().ok()?;

    // Old-style versioning: 1.8 = Java 8
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
