use std::fs;
use std::io::{self, Read};
use std::path::Path;
use thiserror::Error;
use zip::ZipArchive;

#[derive(Error, Debug)]
pub enum ArchiveError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("Path traversal detected: {0}")]
    PathTraversal(String),
    #[error("Entry not found: {0}")]
    NotFound(String),
}

/// Extracts a ZIP archive to the target directory, skipping entries for which `should_exclude` returns true.
///
/// # Errors
///
/// Returns [`ArchiveError`] if extraction fails or path traversal is detected.
pub fn extract_zip_with_filter<F>(
    zip_path: &Path,
    target_dir: &Path,
    should_exclude: F,
) -> Result<Vec<String>, ArchiveError>
where
    F: Fn(&str) -> bool,
{
    let file = fs::File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;
    let mut extracted = Vec::new();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();

        if should_exclude(&name) {
            continue;
        }

        let outpath = match entry.enclosed_name() {
            Some(path) => target_dir.join(path),
            None => return Err(ArchiveError::PathTraversal(name)),
        };

        if entry.is_dir() {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut outfile = fs::File::create(&outpath)?;
            io::copy(&mut entry, &mut outfile)?;
            extracted.push(name);
        }
    }

    Ok(extracted)
}

pub fn read_zip_entry_bytes(zip_path: &Path, entry_name: &str) -> Result<Vec<u8>, ArchiveError> {
    let file = fs::File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;
    let mut entry = archive
        .by_name(entry_name)
        .map_err(|_| ArchiveError::NotFound(entry_name.to_string()))?;
    let mut buffer = Vec::new();
    entry.read_to_end(&mut buffer)?;
    Ok(buffer)
}
