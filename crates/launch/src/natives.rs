use std::fs;
use std::path::Path;
use zip::ZipArchive;

use crate::download::DownloadManager;
use crate::{LaunchError, Library};

/// Helper to check if a file extension matches a dynamic library (.dll, .so, .dylib).
#[must_use]
pub fn is_native_binary(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            let ext_lower = ext.to_lowercase();
            ext_lower == "dll" || ext_lower == "so" || ext_lower == "dylib"
        })
}

/// Helper to count dynamic libraries in `natives_dir`.
#[must_use]
pub fn verify_natives_dir(natives_dir: &Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = fs::read_dir(natives_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && is_native_binary(&path) {
                count += 1;
            }
        }
    }
    count
}

/// # Errors
/// Returns an error if file system operations fail.
pub fn extract_natives(
    native_libraries: &[Library],
    libraries_dir: &Path,
    natives_dir: &Path,
) -> Result<(), LaunchError> {
    tracing::info!(
        natives_dir = ?natives_dir,
        count = native_libraries.len(),
        "Extracting native libraries"
    );

    if natives_dir.exists() {
        fs::remove_dir_all(natives_dir)?;
    }
    fs::create_dir_all(natives_dir)?;

    let mut total_extracted_files = 0;

    for lib in native_libraries {
        if let Some(relative) = DownloadManager::local_path_for_library(lib) {
            let jar_path = libraries_dir.join(&relative);
            if jar_path.exists() {
                tracing::info!(
                    library = %lib.name,
                    jar_path = ?jar_path,
                    "Extracting native JAR"
                );
                let excludes = lib
                    .extract
                    .as_ref()
                    .map_or(&[][..], |e| e.exclude.as_slice());
                let extracted = extract_jar_to_dir(&jar_path, natives_dir, excludes)?;
                total_extracted_files += extracted.len();
                let example_files: Vec<&str> =
                    extracted.iter().take(5).map(String::as_str).collect();
                tracing::info!(
                    library = %lib.name,
                    files_extracted = extracted.len(),
                    example_files = ?example_files,
                    "Extracted files from native JAR"
                );
            } else {
                tracing::warn!(
                    library = %lib.name,
                    jar_path = ?jar_path,
                    "Native JAR path does not exist on disk"
                );
            }
        } else {
            tracing::warn!(
                library = %lib.name,
                "Could not determine local path for native library"
            );
        }
    }

    let binary_count = verify_natives_dir(natives_dir);
    if binary_count == 0 {
        tracing::warn!(
            natives_dir = ?natives_dir,
            total_files = total_extracted_files,
            "No dynamic binary libraries (.dll, .so, .dylib) were found in natives directory after extraction!"
        );
    } else {
        tracing::info!(
            natives_dir = ?natives_dir,
            binary_count = binary_count,
            "Native dynamic libraries successfully extracted"
        );
    }

    Ok(())
}

fn extract_jar_to_dir(
    jar_path: &Path,
    target_dir: &Path,
    custom_excludes: &[String],
) -> Result<Vec<String>, LaunchError> {
    let file = fs::File::open(jar_path)?;
    let mut archive = ZipArchive::new(file)?;

    let default_exclude_dirs = ["META-INF/"];
    let mut extracted_names = Vec::new();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();

        if default_exclude_dirs.iter().any(|d| name.starts_with(d)) {
            continue;
        }

        if custom_excludes.iter().any(|ex| name.starts_with(ex)) {
            continue;
        }

        if entry.is_dir() {
            continue;
        }

        let outpath = target_dir.join(&name);

        if let Some(parent) = outpath.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut outfile = fs::File::create(&outpath)?;
        std::io::copy(&mut entry, &mut outfile)?;
        extracted_names.push(name.clone());

        // Support LWJGL 2.x/3.x nested native binaries:
        // If it's a dynamic binary (.dll, .so, .dylib) inside a subfolder,
        // also extract/copy it to target_dir root if target_dir/filename doesn't exist yet.
        let entry_path = Path::new(&name);
        if let Some(file_name) = entry_path.file_name() {
            if is_native_binary(entry_path) {
                let root_target = target_dir.join(file_name);
                if root_target != outpath && !root_target.exists() {
                    fs::copy(&outpath, &root_target)?;
                    tracing::debug!(
                        from = ?outpath,
                        to = ?root_target,
                        "Copied nested native binary to root of natives directory"
                    );
                }
            }
        }
    }

    Ok(extracted_names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::FileOptions;
    use zip::ZipWriter;

    #[test]
    fn test_is_native_binary() {
        assert!(is_native_binary(Path::new("lwjgl.dll")));
        assert!(is_native_binary(Path::new("lwjgl64.dll")));
        assert!(is_native_binary(Path::new("liblwjgl.so")));
        assert!(is_native_binary(Path::new("liblwjgl.dylib")));
        assert!(!is_native_binary(Path::new("lwjgl.jar")));
        assert!(!is_native_binary(Path::new("MANIFEST.MF")));
    }

    #[test]
    fn test_extract_natives_lwjgl2_and_3() {
        let temp_dir = std::env::temp_dir().join(format!(
            "rtl_test_natives_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let libraries_dir = temp_dir.join("libraries");
        let natives_dir = temp_dir.join("natives");

        let lib_dir = libraries_dir.join("org/lwjgl/lwjgl/lwjgl-platform/2.9.4");
        fs::create_dir_all(&lib_dir).unwrap();

        let jar_path = lib_dir.join("lwjgl-platform-2.9.4-natives-windows.jar");
        let file = fs::File::create(&jar_path).unwrap();
        let mut zip = ZipWriter::new(file);

        // LWJGL 2.x root files
        zip.start_file("lwjgl.dll", FileOptions::default()).unwrap();
        zip.write_all(b"dummy dll content").unwrap();

        zip.start_file("lwjgl64.dll", FileOptions::default())
            .unwrap();
        zip.write_all(b"dummy dll 64 content").unwrap();

        zip.start_file("OpenAL32.dll", FileOptions::default())
            .unwrap();
        zip.write_all(b"dummy openal 32 content").unwrap();

        zip.start_file("OpenAL64.dll", FileOptions::default())
            .unwrap();
        zip.write_all(b"dummy openal 64 content").unwrap();

        // META-INF should be excluded
        zip.start_file("META-INF/MANIFEST.MF", FileOptions::default())
            .unwrap();
        zip.write_all(b"Manifest-Version: 1.0").unwrap();

        // Nested native binary
        zip.start_file("x64/custom_native.dll", FileOptions::default())
            .unwrap();
        zip.write_all(b"dummy nested binary").unwrap();

        zip.finish().unwrap();

        let lib = Library {
            name: "org.lwjgl.lwjgl:lwjgl-platform:2.9.4:natives-windows".to_string(),
            url: None,
            sha1: None,
            size: None,
            is_native: true,
            rules: Vec::new(),
            extract: None,
        };

        extract_natives(&[lib], &libraries_dir, &natives_dir).unwrap();

        assert!(natives_dir.join("lwjgl.dll").exists());
        assert!(natives_dir.join("lwjgl64.dll").exists());
        assert!(natives_dir.join("OpenAL32.dll").exists());
        assert!(natives_dir.join("OpenAL64.dll").exists());
        assert!(!natives_dir.join("META-INF/MANIFEST.MF").exists());

        // Check nested binary extracted to root as well
        assert!(natives_dir.join("custom_native.dll").exists());
        assert_eq!(verify_natives_dir(&natives_dir), 5);

        let _ = fs::remove_dir_all(temp_dir);
    }
}
