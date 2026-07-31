use std::fs;
use std::io::{self, Read, Seek};
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
    #[error("Entry not found: {0}")]
    NotFound(String),
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

/// Reads the raw bytes of a named entry inside a reader containing ZIP archive data.
///
/// # Errors
///
/// Returns [`ArchiveError`] if ZIP reading fails or the entry is not found.
pub fn read_zip_entry_bytes_from_reader<R: Read + Seek>(
    reader: R,
    entry_name: &str,
) -> Result<Vec<u8>, ArchiveError> {
    let mut archive = ZipArchive::new(reader)?;
    let mut entry = archive
        .by_name(entry_name)
        .map_err(|_| ArchiveError::NotFound(entry_name.to_string()))?;
    let mut buffer = Vec::new();
    entry.read_to_end(&mut buffer)?;
    Ok(buffer)
}

/// Reads the raw bytes of a named entry inside a ZIP file.
///
/// # Errors
///
/// Returns [`ArchiveError`] if file I/O fails, ZIP reading fails, or the entry is not found.
pub fn read_zip_entry_bytes(zip_path: &Path, entry_name: &str) -> Result<Vec<u8>, ArchiveError> {
    let file = fs::File::open(zip_path)?;
    read_zip_entry_bytes_from_reader(file, entry_name)
}
