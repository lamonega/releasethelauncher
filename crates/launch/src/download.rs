use reqwest::Client;
use sha1::Digest;
use sha1::Sha1;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};

use crate::platform;
use crate::{LaunchError, Library};

const MOJANG_LIBRARIES: &str = "https://libraries.minecraft.net";
#[allow(dead_code)]
const MAVEN_CENTRAL: &str = "https://repo1.maven.org/maven2";
const FORGE_MAVEN: &str = "https://files.minecraftforge.net/maven";
const FABRIC_MAVEN: &str = "https://maven.fabricmc.net";
const NEOFORGE_MAVEN: &str = "https://maven.neoforged.net/releases";

pub struct DownloadManager {
    http: Client,
    libraries_dir: PathBuf,
    cache_dir: PathBuf,
}

#[must_use]
pub fn library_filename(lib: &Library) -> String {
    let parts: Vec<&str> = lib.name.split(':').collect();
    if parts.len() < 3 {
        return format!("{}.jar", lib.name);
    }
    let artifact = parts[1];
    let mut version = parts[2];
    let classifier_raw = parts.get(3).copied();

    let (classifier, mut ext) = if let Some(cls) = classifier_raw {
        if let Some((c, e)) = cls.split_once('@') {
            (Some(c), e)
        } else {
            (Some(cls), "jar")
        }
    } else if let Some((v, e)) = version.split_once('@') {
        version = v;
        (None, e)
    } else {
        (None, "jar")
    };

    if ext == "jar" {
        if let Some(ref url) = lib.url {
            if url.ends_with(".zip") {
                ext = "zip";
            }
        }
    }

    classifier.map_or_else(
        || format!("{artifact}-{version}.{ext}"),
        |cls| format!("{artifact}-{version}-{cls}.{ext}"),
    )
}

impl DownloadManager {
    #[must_use]
    pub fn new(cache_dir: PathBuf) -> Self {
        let libraries_dir = cache_dir.join("libraries");
        Self {
            http: Client::new(),
            libraries_dir,
            cache_dir,
        }
    }

    #[must_use]
    pub fn libraries_dir(&self) -> &Path {
        &self.libraries_dir
    }

    fn maven_url_for_library(lib: &Library) -> Option<String> {
        if let Some(ref url) = lib.url {
            if url.starts_with("http://") || url.starts_with("https://") {
                if url.ends_with(".jar") || url.ends_with(".zip") {
                    return Some(url.clone());
                }
                let parts: Vec<&str> = lib.name.split(':').collect();
                if parts.len() >= 3 {
                    let group = parts[0].replace('.', "/");
                    let artifact = parts[1];
                    let version = parts[2];
                    let filename = library_filename(lib);
                    let path = format!("{group}/{artifact}/{version}/{filename}");
                    return Some(format!("{}/{path}", url.trim_end_matches('/')));
                }
                return Some(url.clone());
            }
        }

        let parts: Vec<&str> = lib.name.split(':').collect();
        if parts.len() < 3 {
            return None;
        }

        let group = parts[0].replace('.', "/");
        let artifact = parts[1];
        let version = parts[2];
        let filename = library_filename(lib);

        let path = format!("{group}/{artifact}/{version}/{filename}");

        if lib.name.contains("net.minecraftforge") || lib.name.contains("cpw.mods") {
            Some(format!("{FORGE_MAVEN}/{path}"))
        } else if lib.name.contains("net.fabricmc") {
            Some(format!("{FABRIC_MAVEN}/{path}"))
        } else if lib.name.contains("net.neoforged") {
            Some(format!("{NEOFORGE_MAVEN}/{path}"))
        } else {
            Some(format!("{MOJANG_LIBRARIES}/{path}"))
        }
    }

    #[must_use]
    pub fn local_path_for_library(lib: &Library) -> Option<PathBuf> {
        let parts: Vec<&str> = lib.name.split(':').collect();
        if parts.len() < 3 {
            return None;
        }

        let group = parts[0].replace('.', "/");
        let artifact = parts[1];
        let version = parts[2];
        let filename = library_filename(lib);

        Some(
            PathBuf::from(&group)
                .join(artifact)
                .join(version)
                .join(filename),
        )
    }

    /// # Errors
    /// Returns an error if a library has invalid coordinates or a download fails.
    pub async fn download_libraries(
        &self,
        libraries: &[Library],
        progress: impl Fn(u64, u64, &str) + Send + Sync + 'static,
    ) -> Result<(), LaunchError> {
        debug!(
            total_libraries = libraries.len(),
            "Starting download_libraries processing"
        );

        let applicable: Vec<&Library> = libraries
            .iter()
            .filter(|lib| {
                let include = platform::should_include_library(lib);
                if !include {
                    debug!(
                        name = %lib.name,
                        is_native = lib.is_native,
                        "Skipping library: excluded by platform rules"
                    );
                }
                include
            })
            .collect();

        let native_count = applicable.iter().filter(|lib| lib.is_native).count();
        info!(
            total = applicable.len(),
            native = native_count,
            regular = applicable.len() - native_count,
            "Applicable libraries for current platform"
        );

        if applicable.is_empty() {
            return Ok(());
        }

        // Pre-scan: sum sizes of existing files for accurate byte tracking
        let mut initial_downloaded: u64 = 0;

        for lib in &applicable {
            if let Some(ref p) = Self::local_path_for_library(lib) {
                let full_path = self.libraries_dir.join(p);
                if full_path.exists() && full_path.metadata().is_ok_and(|m| m.len() >= 1000) {
                    let size = full_path.metadata().map(|m| m.len()).unwrap_or(0);
                    initial_downloaded += size;
                }
            }
        }

        let total_bytes = Arc::new(AtomicU64::new(initial_downloaded));
        let downloaded_bytes = Arc::new(AtomicU64::new(initial_downloaded));
        let semaphore = Arc::new(Semaphore::new(16));
        let progress_cb = Arc::new(progress);

        let mut tasks = Vec::new();

        for lib in applicable {
            let local_path = match Self::local_path_for_library(lib) {
                Some(p) => p,
                None => {
                    warn!(
                        name = %lib.name,
                        is_native = lib.is_native,
                        "Skipping library: invalid Maven coordinates"
                    );
                    continue;
                }
            };

            let full_local_path = self.libraries_dir.join(&local_path);
            let parts: Vec<&str> = lib.name.split(':').collect();
            let display_name = if parts.len() >= 2 {
                parts[1].to_string()
            } else {
                lib.name.clone()
            };

            let exists_ok = full_local_path.exists()
                && full_local_path.metadata().is_ok_and(|m| m.len() >= 1000);

            let url = match Self::maven_url_for_library(lib) {
                Some(u) => u,
                None => {
                    warn!(
                        name = %lib.name,
                        is_native = lib.is_native,
                        "Skipping library: could not resolve Maven download URL"
                    );
                    continue;
                }
            };

            if exists_ok {
                debug!(
                    name = %lib.name,
                    is_native = lib.is_native,
                    path = %full_local_path.display(),
                    "Library already downloaded and cached, skipping"
                );
            } else {
                info!(
                    name = %lib.name,
                    is_native = lib.is_native,
                    url = %url,
                    "Queuing library download"
                );
            }

            let sem = semaphore.clone();
            let http = self.http.clone();
            let sha1 = lib.sha1.clone();
            let total_b = total_bytes.clone();
            let downloaded_b = downloaded_bytes.clone();
            let progress_ref = progress_cb.clone();
            let lib_name = lib.name.clone();
            let is_native = lib.is_native;

            tasks.push(tokio::spawn(async move {
                if !exists_ok {
                    let _permit = sem.acquire().await.map_err(|e| {
                        let err_msg = format!("Semaphore error acquiring permit for library '{lib_name}': {e}");
                        error!(name = %lib_name, is_native = is_native, error = %e, "Semaphore permit failed");
                        LaunchError::Launch(err_msg)
                    })?;

                    info!(
                        name = %lib_name,
                        is_native = is_native,
                        url = %url,
                        "Downloading library file"
                    );

                    let response = http.get(&url).send().await.map_err(|e| {
                        let err_msg = format!("HTTP error downloading library '{lib_name}' from '{url}': {e}");
                        error!(name = %lib_name, is_native = is_native, url = %url, error = %e, "HTTP request failed");
                        LaunchError::Launch(err_msg)
                    })?;

                    if !response.status().is_success() {
                        let status = response.status();
                        let err_msg = format!("HTTP status {status} downloading library '{lib_name}' from '{url}'");
                        error!(name = %lib_name, is_native = is_native, url = %url, status = %status, "HTTP response status failure");
                        return Err(LaunchError::Launch(err_msg));
                    }

                    let file_size = response.content_length().unwrap_or(0) as u64;
                    total_b.fetch_add(file_size, Ordering::SeqCst);

                    let resp = response.bytes().await.map_err(|e| {
                        let err_msg = format!("Failed to read body bytes for library '{lib_name}' from '{url}': {e}");
                        error!(name = %lib_name, is_native = is_native, url = %url, error = %e, "Failed reading response bytes");
                        LaunchError::Launch(err_msg)
                    })?;

                    if let Some(ref expected) = sha1 {
                        let mut hasher = Sha1::new();
                        hasher.update(&resp);
                        let computed = hex::encode(hasher.finalize());
                        if !computed.eq_ignore_ascii_case(expected) {
                            let err_msg = format!("SHA1 mismatch for library '{lib_name}' from '{url}': expected {expected}, got {computed}");
                            error!(name = %lib_name, is_native = is_native, url = %url, expected = %expected, computed = %computed, "Checksum mismatch");
                            return Err(LaunchError::Launch(err_msg));
                        }
                    }

                    if let Some(parent) = full_local_path.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| {
                            let err_msg = format!("Failed to create parent directory '{}' for library '{lib_name}': {e}", parent.display());
                            error!(name = %lib_name, is_native = is_native, path = %parent.display(), error = %e, "Dir creation failed");
                            LaunchError::Launch(err_msg)
                        })?;
                    }

                    let tmp = full_local_path.with_extension("tmp");
                    std::fs::write(&tmp, &resp).map_err(|e| {
                        let err_msg = format!("Failed to write temporary file '{}' for library '{lib_name}': {e}", tmp.display());
                        error!(name = %lib_name, is_native = is_native, tmp_path = %tmp.display(), error = %e, "File write failed");
                        LaunchError::Launch(err_msg)
                    })?;

                    std::fs::rename(&tmp, &full_local_path).map_err(|e| {
                        let err_msg = format!("Failed to rename temporary file '{}' to '{}' for library '{lib_name}': {e}", tmp.display(), full_local_path.display());
                        error!(name = %lib_name, is_native = is_native, tmp_path = %tmp.display(), target_path = %full_local_path.display(), error = %e, "File rename failed");
                        LaunchError::Launch(err_msg)
                    })?;

                    let file_size = resp.len() as u64;
                    downloaded_b.fetch_add(file_size, Ordering::SeqCst);
                    info!(
                        name = %lib_name,
                        is_native = is_native,
                        size = file_size,
                        path = %full_local_path.display(),
                        "Library downloaded successfully"
                    );
                }

                let cur = downloaded_b.load(Ordering::SeqCst);
                let tot = total_b.load(Ordering::SeqCst);
                progress_ref(cur, tot.max(cur), &display_name);
                Ok::<(), LaunchError>(())
            }));
        }

        for task in tasks {
            task.await
                .map_err(|e| LaunchError::Launch(format!("Library download task failed to join: {e}")))?
                ?;
        }

        info!("All applicable libraries processed successfully");
        Ok(())
    }

    /// # Errors
    /// Returns an error if the asset index cannot be read or a download fails.
    pub async fn download_asset_objects(
        &self,
        http: &Client,
        asset_index_path: &Path,
        progress: impl Fn(u64, u64, &str) + Send + Sync + 'static,
    ) -> Result<(), LaunchError> {
        let index_content = std::fs::read_to_string(asset_index_path)?;
        let index: serde_json::Value = serde_json::from_str(&index_content)?;

        let objects_dir = self.cache_dir.join("assets").join("objects");

        if let Some(objects) = index.get("objects").and_then(|v| v.as_object()) {
            if objects.is_empty() {
                return Ok(());
            }

            // Pre-scan: sum sizes of existing and missing assets
            let mut initial_downloaded: u64 = 0;
            let mut total_bytes: u64 = 0;

            for (_, obj) in objects {
                let hash = obj
                    .get("hash")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let size = obj.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
                total_bytes += size;

                if !hash.is_empty() {
                    let prefix = &hash[..2.min(hash.len())];
                    let target_path = objects_dir.join(prefix).join(hash);
                    if target_path.exists() {
                        initial_downloaded += size;
                    }
                }
            }

            let total_b = Arc::new(AtomicU64::new(total_bytes));
            let downloaded_b = Arc::new(AtomicU64::new(initial_downloaded));
            let semaphore = Arc::new(Semaphore::new(16));
            let progress_cb = Arc::new(progress);

            let mut tasks = Vec::new();

            for (name, obj) in objects {
                let hash = obj
                    .get("hash")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let size = obj.get("size").and_then(|v| v.as_u64()).unwrap_or(0);

                if hash.is_empty() {
                    let cur = downloaded_b.fetch_add(size, Ordering::SeqCst) + size;
                    let tot = total_b.load(Ordering::SeqCst);
                    progress_cb(cur, tot.max(cur), name);
                    continue;
                }

                let prefix = hash[..2.min(hash.len())].to_string();
                let target_path = objects_dir.join(&prefix).join(&hash);
                let name_clone = name.clone();

                let sem = semaphore.clone();
                let client = http.clone();
                let downloaded_cnt = downloaded_b.clone();
                let total_cnt = total_b.clone();
                let progress_ref = progress_cb.clone();

                tasks.push(tokio::spawn(async move {
                    if !target_path.exists() {
                        let _permit = sem.acquire().await.unwrap();
                        let url =
                            format!("https://resources.download.minecraft.net/{prefix}/{hash}");
                        if let Some(parent) = target_path.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        if let Ok(resp) = client.get(&url).send().await {
                            if resp.status().is_success() {
                                if let Ok(bytes) = resp.bytes().await {
                                    let tmp = target_path.with_extension("tmp");
                                    let _ = std::fs::write(&tmp, &bytes);
                                    let _ = std::fs::rename(&tmp, &target_path);
                                    downloaded_cnt.fetch_add(size, Ordering::SeqCst);
                                    debug!(asset = %name_clone, hash = %hash, "Downloaded asset object");
                                } else {
                                    warn!(asset = %name_clone, hash = %hash, "Failed reading asset bytes");
                                }
                            } else {
                                warn!(asset = %name_clone, hash = %hash, status = %resp.status(), "Asset download HTTP failure");
                            }
                        } else {
                            warn!(asset = %name_clone, hash = %hash, "Asset HTTP request failed");
                        }
                    } else {
                        downloaded_cnt.fetch_add(size, Ordering::SeqCst);
                        debug!(asset = %name_clone, hash = %hash, "Asset object already cached");
                    }

                    let cur = downloaded_cnt.load(Ordering::SeqCst);
                    let tot = total_cnt.load(Ordering::SeqCst);
                    progress_ref(cur, tot.max(cur), &name_clone);
                }));
            }

            for task in tasks {
                let _ = task.await;
            }
        }

        Ok(())
    }

    /// # Errors
    /// Returns an error if the download fails or SHA1 does not match.
    pub async fn download_client_jar(
        &self,
        target_path: &Path,
        url: &str,
        expected_sha1: Option<&str>,
    ) -> Result<(), LaunchError> {
        if target_path.exists() && target_path.metadata().map_or(false, |m| m.len() > 1000) {
            debug!(path = %target_path.display(), "Client JAR already cached, skipping download");
            return Ok(());
        }
        info!(path = %target_path.display(), url = %url, "Downloading client JAR");
        self.download_file(url, target_path, expected_sha1).await
    }

    async fn download_file(
        &self,
        url: &str,
        target: &Path,
        expected_sha1: Option<&str>,
    ) -> Result<(), LaunchError> {
        info!(url = %url, target = %target.display(), "Downloading file");
        let response = self.http.get(url).send().await.map_err(|e| {
            error!(url = %url, error = %e, "HTTP request failed");
            LaunchError::Launch(format!("HTTP error downloading {url}: {e}"))
        })?;
        if !response.status().is_success() {
            let status = response.status();
            error!(url = %url, status = %status, "HTTP response status failure");
            return Err(LaunchError::Launch(format!(
                "HTTP status {status} downloading {url}"
            )));
        }
        let resp = response.bytes().await.map_err(|e| {
            error!(url = %url, error = %e, "Failed reading response body");
            LaunchError::Launch(format!("Failed reading response bytes from {url}: {e}"))
        })?;

        if let Some(expected) = expected_sha1 {
            let mut hasher = Sha1::new();
            hasher.update(&resp);
            let computed = hex::encode(hasher.finalize());
            if !computed.eq_ignore_ascii_case(expected) {
                error!(url = %url, expected = %expected, computed = %computed, "SHA1 mismatch");
                return Err(LaunchError::Launch(format!(
                    "SHA1 mismatch for {url}: expected {expected}, got {computed}"
                )));
            }
        }

        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                error!(target = %target.display(), error = %e, "Dir creation failed");
                LaunchError::Launch(format!("Failed to create parent directory for {}: {e}", target.display()))
            })?;
        }

        let tmp = target.with_extension("tmp");
        std::fs::write(&tmp, &resp).map_err(|e| {
            error!(tmp = %tmp.display(), error = %e, "File write failed");
            LaunchError::Launch(format!("Failed to write temporary file {}: {e}", tmp.display()))
        })?;
        std::fs::rename(&tmp, target).map_err(|e| {
            error!(tmp = %tmp.display(), target = %target.display(), error = %e, "File rename failed");
            LaunchError::Launch(format!("Failed to rename temporary file {} to {}: {e}", tmp.display(), target.display()))
        })?;

        debug!(target = %target.display(), "File downloaded successfully");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDir(PathBuf);
    impl TestDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("rtl_test_{}_{}", name, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
            let _ = std::fs::create_dir_all(&path);
            Self(path)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn test_local_path_and_maven_url_for_standard_and_native_libraries() {
        let std_lib = Library {
            name: "org.lwjgl:lwjgl:3.3.1".to_string(),
            url: None,
            sha1: None,
            size: None,
            is_native: false,
            rules: vec![],
            extract: None,
        };

        let native_lib = Library {
            name: "org.lwjgl:lwjgl:3.3.1:natives-windows".to_string(),
            url: None,
            sha1: None,
            size: None,
            is_native: true,
            rules: vec![],
            extract: None,
        };

        let std_path = DownloadManager::local_path_for_library(&std_lib).unwrap();
        assert_eq!(
            std_path,
            PathBuf::from("org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1.jar")
        );

        let native_path = DownloadManager::local_path_for_library(&native_lib).unwrap();
        assert_eq!(
            native_path,
            PathBuf::from("org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-windows.jar")
        );

        let std_url = DownloadManager::maven_url_for_library(&std_lib).unwrap();
        assert_eq!(
            std_url,
            "https://libraries.minecraft.net/org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1.jar"
        );

        let native_url = DownloadManager::maven_url_for_library(&native_lib).unwrap();
        assert_eq!(
            native_url,
            "https://libraries.minecraft.net/org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-windows.jar"
        );
    }

    #[tokio::test]
    async fn test_download_libraries_skips_cached() {
        let dir = TestDir::new("skip_cached");
        let dm = DownloadManager::new(dir.path().to_path_buf());

        let native_lib = Library {
            name: "org.lwjgl:lwjgl:3.3.1:natives-windows".to_string(),
            url: None,
            sha1: None,
            size: None,
            is_native: true,
            rules: vec![],
            extract: None,
        };

        // Create pre-cached library file >= 1000 bytes
        let local_rel = DownloadManager::local_path_for_library(&native_lib).unwrap();
        let cached_path = dm.libraries_dir().join(local_rel);
        std::fs::create_dir_all(cached_path.parent().unwrap()).unwrap();
        std::fs::write(&cached_path, vec![0u8; 1200]).unwrap();

        let progress_called = Arc::new(AtomicU64::new(0));
        let progress_called_clone = progress_called.clone();

        let res = dm
            .download_libraries(&[native_lib], move |_, _, _| {
                progress_called_clone.fetch_add(1, Ordering::SeqCst);
            })
            .await;

        assert!(res.is_ok());
        assert_eq!(progress_called.load(Ordering::SeqCst), 1);
    }
}


