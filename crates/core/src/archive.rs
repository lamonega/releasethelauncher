use std::fs;
use std::io;
use std::path::Path;
use thiserror::Error;
use zip::read::ZipArchive;

#[derive(Error, Debug)]
pub enum ArchiveError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("Path traversal detected: {0}")]
    PathTraversal(String),
}

/// Extracts a ZIP archive to the specified target directory.
///
/// # Errors
///
/// Returns [`ArchiveError::Io`] if file I/O operations fail,
/// [`ArchiveError::Zip`] if reading the ZIP archive fails,
/// or [`ArchiveError::PathTraversal`] if a ZIP entry attempts directory traversal.
pub fn extract_zip_to_dir(zip_path: &Path, target_dir: &Path) -> Result<(), ArchiveError> {
    let file = fs::File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let outpath = match entry.enclosed_name() {
            Some(path) => target_dir.join(path),
            None => return Err(ArchiveError::PathTraversal(entry.name().to_string())),
        };

        if entry.is_dir() {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut outfile = fs::File::create(&outpath)?;
            io::copy(&mut entry, &mut outfile)?;
        }
    }

    Ok(())
}
