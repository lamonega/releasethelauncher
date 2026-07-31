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
/// 2. The `JAVA_HOME` environment variable (`bin/javaw.exe` / `bin/java.exe` / `bin/java`).
/// 3. Windows Registry entries for known Java vendors (including JDK 25+).
/// 4. Dev environment paths (`.jdks`, `.sdkman`, `.gradle`).
/// 5. Bundled Minecraft Java installations.
/// 6. `java`/`javaw` on `PATH`.
/// 7. Known system fallback installation paths.
///
/// Resolves a suitable Java executable path for launching Minecraft.
///
/// # Errors
///
/// Returns [`LaunchError::JavaNotFound`] if no suitable Java is found or the detected
/// version is incompatible with `compatible_java_majors`.
pub fn resolve_java(
    instance_java_path: Option<&str>,
    compatible_java_majors: &[u32],
) -> Result<PathBuf, LaunchError> {
    let mut last_incompatible_err = None;

    if let Some(path_str) = instance_java_path {
        let path = PathBuf::from(path_str);
        if path.exists() {
            return validate_java(&path, compatible_java_majors);
        }
    }

    if let Some(path) = find_system_java(compatible_java_majors, &mut last_incompatible_err) {
        return Ok(path);
    }

    last_incompatible_err.map_or_else(
        || {
            let req_info = if compatible_java_majors.is_empty() {
                String::new()
            } else {
                format!(" Required versions: {compatible_java_majors:?}.")
            };
            Err(LaunchError::JavaNotFound(format!(
                "No suitable Java installation found.{req_info} Set JAVA_HOME, install the required JDK, or add Java to PATH."
            )))
        },
        Err,
    )
}

fn find_system_java(
    compatible_java_majors: &[u32],
    last_incompatible_err: &mut Option<LaunchError>,
) -> Option<PathBuf> {
    let mut check_candidate = |path: &Path| -> Option<PathBuf> {
        if path.exists() {
            match validate_java(path, compatible_java_majors) {
                Ok(validated) => Some(validated),
                Err(err) => {
                    *last_incompatible_err = Some(err);
                    None
                }
            }
        } else {
            None
        }
    };

    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        let home_path = PathBuf::from(&java_home);
        let executables = if cfg!(windows) {
            vec!["javaw.exe", "java.exe"]
        } else {
            vec!["java"]
        };
        for exe in executables {
            let path = home_path.join("bin").join(exe);
            if let Some(valid) = check_candidate(&path) {
                return Some(valid);
            }
        }
    }

    if let Some(valid) = scan_common_java_paths(&mut check_candidate) {
        return Some(valid);
    }

    if let Some(path) = find_bundled_java(compatible_java_majors) {
        return Some(path);
    }

    let exe_names = if cfg!(windows) {
        vec!["javaw.exe", "java.exe"]
    } else {
        vec!["java"]
    };
    let find_cmd = if cfg!(windows) { "where" } else { "which" };
    for exe_name in exe_names {
        if let Ok(output) = quiet_command(find_cmd, &[exe_name]) {
            if output.status.success() {
                let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                for line in path_str.lines() {
                    let path = PathBuf::from(line.trim());
                    if let Some(valid) = check_candidate(&path) {
                        return Some(valid);
                    }
                }
            }
        }
    }

    None
}

fn scan_common_java_paths(
    check_candidate: &mut impl FnMut(&Path) -> Option<PathBuf>,
) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let candidates = find_java_from_registry();
        for path in candidates {
            if let Some(valid) = check_candidate(&path) {
                return Some(valid);
            }
        }
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
                        for exe in &["bin/javaw.exe", "bin/java.exe", "bin/java"] {
                            let candidate = path.join(exe);
                            if let Some(valid) = check_candidate(&candidate) {
                                return Some(valid);
                            }
                        }
                    }
                }
            }
        }
        let fallbacks = [
            r"C:\Program Files\Java\jdk-25\bin\javaw.exe",
            r"C:\Program Files\Java\jdk-25\bin\java.exe",
            r"C:\Program Files\Eclipse Adoptium\jdk-25\bin\javaw.exe",
            r"C:\Program Files\Microsoft\jdk-25\bin\javaw.exe",
            r"C:\Program Files\Java\jdk-24\bin\javaw.exe",
            r"C:\Program Files\Java\jdk-23\bin\javaw.exe",
            r"C:\Program Files\Java\jdk-22\bin\javaw.exe",
            r"C:\Program Files\Java\jdk-21\bin\javaw.exe",
            r"C:\Program Files\Eclipse Adoptium\jdk-21.0.11.10-hotspot\bin\javaw.exe",
            r"C:\Program Files\Java\jdk-21\bin\java.exe",
            r"C:\Program Files\Java\jdk-17\bin\javaw.exe",
            r"C:\Program Files\Java\jdk-17\bin\java.exe",
            r"C:\Program Files\Java\jre8\bin\javaw.exe",
            r"C:\Program Files\Java\jre1.8.0\bin\javaw.exe",
            r"C:\Program Files (x86)\Java\jre8\bin\javaw.exe",
        ];
        for fallback in &fallbacks {
            let path = PathBuf::from(fallback);
            if let Some(valid) = check_candidate(&path) {
                return Some(valid);
            }
        }
    }
    None
}

fn find_bundled_java(compatible_java_majors: &[u32]) -> Option<PathBuf> {
    let local_app_data = std::env::var("LOCALAPPDATA").ok()?;
    let mc_runtime_dir = PathBuf::from(local_app_data)
        .join("Packages")
        .join("Microsoft.4297127926708_8wekyb3d8bbwe")
        .join("LocalCache")
        .join("Local")
        .join("runtime");

    if !mc_runtime_dir.exists() {
        return None;
    }

    let entries = std::fs::read_dir(mc_runtime_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let javaw = path.join("windows-x64").join("bin").join("javaw.exe");
        let java = path.join("windows-x64").join("bin").join("java.exe");
        let candidate = if javaw.exists() {
            javaw
        } else if java.exists() {
            java
        } else {
            continue;
        };

        if validate_java(&candidate, compatible_java_majors).is_ok() {
            return Some(candidate);
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
                    for value_name in &value_names {
                        if let Ok(java_home) = subkey.get_value::<String, _>(value_name) {
                            add_java_bin_candidates(&java_home, &mut candidates);
                        }
                    }

                    for version_name in subkey.enum_keys().filter_map(std::result::Result::ok) {
                        if let Ok(version_key) = subkey.open_subkey_with_flags(&version_name, *view)
                        {
                            for value_name in &value_names {
                                if let Ok(java_home) =
                                    version_key.get_value::<String, _>(value_name)
                                {
                                    add_java_bin_candidates(&java_home, &mut candidates);
                                }
                            }

                            for sub_version_name in
                                version_key.enum_keys().filter_map(std::result::Result::ok)
                            {
                                if let Ok(nested_key) =
                                    version_key.open_subkey_with_flags(&sub_version_name, *view)
                                {
                                    for value_name in &value_names {
                                        if let Ok(java_home) =
                                            nested_key.get_value::<String, _>(value_name)
                                        {
                                            add_java_bin_candidates(&java_home, &mut candidates);
                                        }
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

#[cfg(target_os = "windows")]
fn add_java_bin_candidates(java_home: &str, candidates: &mut Vec<PathBuf>) {
    let base = PathBuf::from(java_home.trim());
    for exe in &["javaw.exe", "java.exe"] {
        let candidate = if base.ends_with("bin") {
            base.join(exe)
        } else {
            base.join("bin").join(exe)
        };
        if !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    }
}

/// Validates a Java executable path against compatible Java major versions.
///
/// # Errors
///
/// Returns [`LaunchError::JavaNotFound`] if the version cannot be determined or is incompatible.
pub fn validate_java(
    java_path: &Path,
    compatible_java_majors: &[u32],
) -> Result<PathBuf, LaunchError> {
    detect_java_major_version(java_path).map_or_else(
        || {
            Err(LaunchError::JavaNotFound(format!(
                "Could not determine Java version at: {}",
                java_path.display()
            )))
        },
        |major| check_version_compatibility(major, java_path, compatible_java_majors),
    )
}

fn check_version_compatibility(
    major: u32,
    java_path: &Path,
    compatible_java_majors: &[u32],
) -> Result<PathBuf, LaunchError> {
    if compatible_java_majors.is_empty() {
        Ok(java_path.to_path_buf())
    } else {
        let min_required = compatible_java_majors.iter().copied().min().unwrap_or(8);
        if major >= min_required {
            Ok(java_path.to_path_buf())
        } else {
            let compatible = compatible_java_majors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            Err(LaunchError::JavaNotFound(format!(
                "Minecraft requires one of Java versions [{compatible}] (found Java {major} at {})",
                java_path.display()
            )))
        }
    }
}

#[must_use]
pub fn detect_java_major_version(java_path: &Path) -> Option<u32> {
    let output = quiet_command(java_path.to_str()?, &["-version"]).ok()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    parse_java_version_output(&stderr)
}

#[must_use]
pub fn parse_java_version_output(output: &str) -> Option<u32> {
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(idx) = line.find("version ") {
            let after_version = &line[idx + "version ".len()..];
            let ver_str = after_version.strip_prefix('"').map_or_else(
                || {
                    after_version
                        .split_whitespace()
                        .next()
                        .unwrap_or(after_version)
                },
                |inside| inside.split('"').next().unwrap_or(inside),
            );
            if let Some(major) = extract_major_version(ver_str) {
                return Some(major);
            }
        }

        if let (Some(start), Some(end)) = (line.find('"'), line.rfind('"')) {
            if start < end {
                let ver_str = &line[start + 1..end];
                if let Some(major) = extract_major_version(ver_str) {
                    return Some(major);
                }
            }
        }

        for token in line.split_whitespace() {
            let clean_token = token.trim_matches('"');
            if clean_token.starts_with(|c: char| c.is_ascii_digit()) {
                if let Some(major) = extract_major_version(clean_token) {
                    return Some(major);
                }
            }
        }
    }
    None
}

fn extract_major_version(ver_str: &str) -> Option<u32> {
    let ver_str = ver_str.trim_matches('"').trim();
    if ver_str.is_empty() {
        return None;
    }

    let target_part = ver_str.strip_prefix("1.").unwrap_or(ver_str);

    let digits: String = target_part
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
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
    fn parse_java_25_ea() {
        assert_eq!(
            parse_java_version_output("openjdk version \"25-ea\" 2025-09-16"),
            Some(25)
        );
    }

    #[test]
    fn parse_java_25_plain() {
        assert_eq!(parse_java_version_output("java version \"25\""), Some(25));
    }

    #[test]
    fn parse_old_style_version() {
        assert_eq!(
            parse_java_version_output("java version \"1.8.0_362\""),
            Some(8)
        );
    }

    #[test]
    fn parse_with_leading_stdout_stderr_junk() {
        let sample = "Picked up _JAVA_OPTIONS: -Dsomething=true\nopenjdk version \"21.0.3\" 2024-04-16\nOpenJDK Runtime Environment";
        assert_eq!(parse_java_version_output(sample), Some(21));
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

    #[test]
    fn test_check_version_compatibility() {
        let dummy_path = Path::new("/usr/bin/java");

        // Equal or higher version
        assert!(check_version_compatibility(21, dummy_path, &[17]).is_ok());
        assert!(check_version_compatibility(25, dummy_path, &[25]).is_ok());
        assert!(check_version_compatibility(8, dummy_path, &[8]).is_ok());

        // Incompatible version lower than required
        let err = check_version_compatibility(21, dummy_path, &[25]).unwrap_err();
        assert!(err.to_string().contains(
            "Minecraft requires one of Java versions [25] (found Java 21 at /usr/bin/java)"
        ));

        let err8 = check_version_compatibility(8, dummy_path, &[17]).unwrap_err();
        assert!(err8.to_string().contains(
            "Minecraft requires one of Java versions [17] (found Java 8 at /usr/bin/java)"
        ));
    }
}
