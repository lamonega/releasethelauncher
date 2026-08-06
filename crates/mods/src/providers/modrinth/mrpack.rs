use std::fs;
use std::path::{Path, PathBuf};

use crate::error::ModsError;
use crate::fs::safe_join_under;
use crate::providers::modrinth::types::{MrpackFile, MrpackIndex};

use release_the_launcher_net::HashKind;

pub(crate) fn count_modpack_file_sizes(files: &[MrpackFile], mc_dir: &Path) -> (u64, u64) {
    let mut total_bytes: u64 = 0;
    let mut initial_downloaded: u64 = 0;
    for file_obj in files {
        let size = file_obj.file_size;
        if let Ok(dest) = safe_join_under(mc_dir, Path::new(&file_obj.path)) {
            total_bytes += size;
            if dest.exists() && dest.metadata().is_ok_and(|m| m.len() > 0) {
                initial_downloaded += size;
            }
        } else {
            log::warn!(
                "Skipping modpack file outside target dir: {}",
                file_obj.path
            );
        }
    }
    (total_bytes, initial_downloaded)
}

fn unpack_jar_entry(
    zip_entry: &mut zip::read::ZipFile,
    name: &str,
    new_jar_path: &Path,
    zip_writer: &mut Option<zip::ZipWriter<fs::File>>,
) {
    let Some(prefix) = name
        .strip_prefix("Jar/")
        .or_else(|| name.strip_prefix("jar/"))
        .or_else(|| name.strip_prefix("minecraft/"))
    else {
        return;
    };
    let inner_name = &name[prefix.len()..];
    if inner_name.is_empty() {
        return;
    }
    if zip_writer.is_none() {
        if let Ok(jar_file) = fs::File::create(new_jar_path) {
            *zip_writer = Some(zip::ZipWriter::new(jar_file));
        }
    }
    if let Some(zw) = zip_writer {
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        if zip_entry.is_dir() {
            let _ = zw.add_directory(inner_name, options);
        } else if zw.start_file(inner_name, options).is_ok() {
            let mut buffer = Vec::new();
            if std::io::Read::read_to_end(zip_entry, &mut buffer).is_ok() {
                let _ = std::io::Write::write_all(zw, &buffer);
            }
        }
    }
}

fn unpack_resource_entry(zip_entry: &mut zip::read::ZipFile, name: &str, resources_dest: &Path) {
    let prefix = if name.starts_with("Resources/") {
        "Resources/"
    } else {
        "resources/"
    };
    let inner_name = &name[prefix.len()..];
    if inner_name.is_empty() {
        return;
    }
    let Ok(out_path) = safe_join_under(resources_dest, Path::new(inner_name)) else {
        return;
    };
    if zip_entry.is_dir() {
        let _ = fs::create_dir_all(&out_path);
    } else {
        if let Some(parent) = out_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(mut out_file) = fs::File::create(&out_path) {
            let _ = std::io::copy(zip_entry, &mut out_file);
        }
    }
}

fn unpack_mods_entry(zip_entry: &mut zip::read::ZipFile, inner_name: &str, target_dir: &Path) {
    if zip_entry.is_dir() || inner_name.is_empty() {
        return;
    }
    let Ok(out_path) = safe_join_under(target_dir, Path::new(inner_name)) else {
        return;
    };
    if let Some(parent) = out_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut out_file) = fs::File::create(&out_path) {
        let _ = std::io::copy(zip_entry, &mut out_file);
    }
}

pub(crate) fn unpack_structured_mod_archive_if_needed(path: &Path, target_dir: &Path) -> PathBuf {
    if path
        .extension()
        .and_then(|s| s.to_str())
        .map(str::to_lowercase)
        .as_deref()
        != Some("zip")
    {
        return path.to_path_buf();
    }

    let Ok(file) = fs::File::open(path) else {
        return path.to_path_buf();
    };
    let Ok(mut archive) = zip::ZipArchive::new(file) else {
        return path.to_path_buf();
    };

    let parent_mc_dir = target_dir.parent().unwrap_or(target_dir);
    let resources_dest = parent_mc_dir.join("resources");
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("mod");
    let new_jar_path = target_dir.join(format!("{stem}.jar"));

    let mut has_jar_dir = false;
    let mut has_resources_dir = false;
    let mut has_mods_dir = false;
    let mut zip_writer: Option<zip::ZipWriter<fs::File>> = None;

    for i in 0..archive.len() {
        let Ok(mut entry) = archive.by_index(i) else {
            continue;
        };
        let name = entry.name().to_string();

        if name.starts_with("Jar/") || name.starts_with("jar/") || name.starts_with("minecraft/") {
            has_jar_dir = true;
            unpack_jar_entry(&mut entry, &name, &new_jar_path, &mut zip_writer);
        } else if name.starts_with("Resources/") || name.starts_with("resources/") {
            has_resources_dir = true;
            unpack_resource_entry(&mut entry, &name, &resources_dest);
        } else if let Some(inner_name) = name.strip_prefix("mods/") {
            has_mods_dir = true;
            unpack_mods_entry(&mut entry, inner_name, target_dir);
        }
    }

    if let Some(zw) = zip_writer {
        let _ = zw.finish();
    }

    if !has_jar_dir && !has_resources_dir && !has_mods_dir {
        return path.to_path_buf();
    }

    fs::remove_file(path).ok();
    if has_jar_dir {
        new_jar_path
    } else {
        target_dir.to_path_buf()
    }
}

pub(crate) async fn download_mrpack_files(
    http: &reqwest::Client,
    target_dir: &Path,
    index: &MrpackIndex,
    progress: impl Fn(u64, u64, &str) + Send + Sync + 'static,
) -> Result<(), ModsError> {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    let mc_dir = target_dir.join(".minecraft");
    let (total_bytes, initial_downloaded) = count_modpack_file_sizes(&index.files, &mc_dir);

    let downloaded_b = Arc::new(AtomicU64::new(initial_downloaded));
    let progress_cb = Arc::new(progress);
    let client = http.clone();
    let concurrency = release_the_launcher_constants::net::DEFAULT_MAX_CONCURRENT_DOWNLOADS;

    let mut join_set = tokio::task::JoinSet::new();

    for file_obj in &index.files {
        let Some(url) = file_obj.downloads.first().cloned() else {
            continue;
        };
        let path_str = file_obj.path.clone();
        let Ok(dest) = safe_join_under(&mc_dir, Path::new(&path_str)) else {
            log::warn!("Skipping modpack file outside target dir: {path_str}");
            continue;
        };
        let display_name = Path::new(&path_str)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or(path_str);

        let client = client.clone();
        let downloaded_cnt = downloaded_b.clone();
        let progress_ref = progress_cb.clone();
        let size = file_obj.file_size;

        let checksum = file_obj
            .hashes
            .get("sha512")
            .map(|h| (HashKind::Sha512, h.clone()))
            .or_else(|| {
                file_obj
                    .hashes
                    .get("sha1")
                    .map(|h| (HashKind::Sha1, h.clone()))
            });

        if join_set.len() >= concurrency {
            join_set.join_next().await;
        }

        join_set.spawn(async move {
            if !dest.exists() || dest.metadata().map_or(true, |m| m.len() == 0) {
                let checksum_ref = checksum.as_ref().map(|(k, h)| (*k, h.as_str()));
                if release_the_launcher_net::download_to_file(
                    &client,
                    &url,
                    &dest,
                    checksum_ref,
                    None,
                )
                .await
                .is_ok()
                {
                    downloaded_cnt.fetch_add(size, Ordering::SeqCst);
                } else {
                    log::warn!("Failed to download mod file {display_name}");
                }
            } else {
                downloaded_cnt.fetch_add(size, Ordering::SeqCst);
            }
            let cur = downloaded_cnt.load(Ordering::SeqCst);
            progress_ref(cur, total_bytes.max(cur), &display_name);
        });
    }

    while join_set.join_next().await.is_some() {}

    Ok(())
}

#[cfg(test)]
fn extract_mod_entries<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    target_dir: &Path,
) {
    for i in 0..archive.len() {
        if let Ok(mut zip_entry) = archive.by_index(i) {
            let name = zip_entry.name().to_string();
            if let Some(inner_name) = name.strip_prefix("mods/") {
                if !zip_entry.is_dir() && !inner_name.is_empty() {
                    if let Ok(out_path) = safe_join_under(target_dir, Path::new(inner_name)) {
                        if let Some(parent) = out_path.parent() {
                            let _ = fs::create_dir_all(parent);
                        }
                        if let Ok(mut out_file) = fs::File::create(&out_path) {
                            let _ = std::io::copy(&mut zip_entry, &mut out_file);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
fn extract_resource_entries<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    resources_dest: &Path,
) {
    for i in 0..archive.len() {
        if let Ok(mut zip_entry) = archive.by_index(i) {
            let name = zip_entry.name().to_string();
            if name.starts_with("Resources/") || name.starts_with("resources/") {
                let prefix = if name.starts_with("Resources/") {
                    "Resources/"
                } else {
                    "resources/"
                };
                let inner_name = &name[prefix.len()..];
                if !inner_name.is_empty() {
                    if let Ok(out_path) = safe_join_under(resources_dest, Path::new(inner_name)) {
                        if zip_entry.is_dir() {
                            let _ = fs::create_dir_all(&out_path);
                        } else {
                            if let Some(parent) = out_path.parent() {
                                let _ = fs::create_dir_all(parent);
                            }
                            if let Ok(mut out_file) = fs::File::create(&out_path) {
                                let _ = std::io::copy(&mut zip_entry, &mut out_file);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn temp_root() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rtl_mods_extract_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        fs::remove_dir_all(&dir).ok();
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn build_malicious_zip() -> Vec<u8> {
        let cursor = std::io::Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(cursor);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        writer.start_file("mods/legit.jar", options).unwrap();
        writer.write_all(b"jar").unwrap();
        writer.start_file("mods/../../evil", options).unwrap();
        writer.write_all(b"evil").unwrap();
        writer.start_file("resources/legit.txt", options).unwrap();
        writer.write_all(b"text").unwrap();
        writer
            .start_file("resources/../../evil.sh", options)
            .unwrap();
        writer.write_all(b"evil").unwrap();
        writer
            .start_file("overrides/../../evil.sh", options)
            .unwrap();
        writer.write_all(b"evil").unwrap();

        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn extract_entries_reject_traversal() {
        let root = temp_root();
        let target_dir = root.join("instance");
        let resources_dest = root.join("resources");
        fs::create_dir_all(&target_dir).unwrap();
        fs::create_dir_all(&resources_dest).unwrap();

        let bytes = build_malicious_zip();
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();

        extract_mod_entries(&mut archive, &target_dir);
        extract_resource_entries(&mut archive, &resources_dest);

        assert!(target_dir.join("legit.jar").exists());
        assert!(resources_dest.join("legit.txt").exists());
        assert!(!root.join("evil").exists());
        assert!(!root.join("evil.sh").exists());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn overrides_traversal_is_rejected_by_enclosed_name() {
        let bytes = build_malicious_zip();
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        for i in 0..archive.len() {
            let entry = archive.by_index(i).unwrap();
            if entry.name() == "overrides/../../evil.sh" {
                assert!(entry.enclosed_name().is_none());
            }
        }
    }
}
