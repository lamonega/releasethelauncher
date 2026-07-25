use std::fs;
use std::path::Path;
use zip::ZipArchive;

use crate::{Library, LaunchError};

/// # Errors
/// Returns an error if file system operations fail.
pub fn extract_natives(
    native_libraries: &[Library],
    libraries_dir: &Path,
    natives_dir: &Path,
) -> Result<(), LaunchError> {
    if natives_dir.exists() {
        fs::remove_dir_all(natives_dir)?;
    }
    fs::create_dir_all(natives_dir)?;

    for lib in native_libraries {
        let parts: Vec<&str> = lib.name.split(':').collect();
        if parts.len() >= 3 {
            let path = parts[0].replace('.', "/");
            let artifact = parts[1];
            let version = parts[2];
            let filename = format!("{artifact}-{version}-natives.jar");
            let jar_path = libraries_dir
                .join(&path)
                .join(artifact)
                .join(version)
                .join(&filename);

            if jar_path.exists() {
                extract_jar_to_dir(&jar_path, natives_dir)?;
            }
        }
    }

    Ok(())
}

fn extract_jar_to_dir(jar_path: &Path, target_dir: &Path) -> Result<(), LaunchError> {
    let file = fs::File::open(jar_path)?;
    let mut archive = ZipArchive::new(file)?;

    let exclude_dirs = ["META-INF/"];

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();

        if exclude_dirs.iter().any(|d| name.starts_with(d)) {
            continue;
        }

        let outpath = target_dir.join(&name);

        if let Some(parent) = outpath.parent() {
            fs::create_dir_all(parent)?;
        }

        if !entry.is_dir() {
            let mut outfile = fs::File::create(&outpath)?;
            std::io::copy(&mut entry, &mut outfile)?;
        }
    }

    Ok(())
}
