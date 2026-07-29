use reqwest::Client;
use sha1::Digest;
use sha1::Sha1;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Semaphore;

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
                if url.ends_with(".jar") {
                    return Some(url.clone());
                }
                let parts: Vec<&str> = lib.name.split(':').collect();
                if parts.len() >= 3 {
                    let group = parts[0].replace('.', "/");
                    let artifact = parts[1];
                    let version = parts[2];
                    let classifier = parts.get(3);
                    let filename = classifier.map_or_else(
                        || format!("{artifact}-{version}.jar"),
                        |cls| format!("{artifact}-{version}-{cls}.jar"),
                    );
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
        let classifier = parts.get(3);

        let filename = classifier.map_or_else(
            || format!("{artifact}-{version}.jar"),
            |cls| format!("{artifact}-{version}-{cls}.jar"),
        );

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

    fn local_path_for_library(lib: &Library) -> Option<PathBuf> {
        let parts: Vec<&str> = lib.name.split(':').collect();
        if parts.len() < 3 {
            return None;
        }

        let group = parts[0].replace('.', "/");
        let artifact = parts[1];
        let version = parts[2];
        let classifier = parts.get(3);

        let filename = classifier.map_or_else(
            || format!("{artifact}-{version}.jar"),
            |cls| format!("{artifact}-{version}-{cls}.jar"),
        );

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
        let applicable: Vec<&Library> = libraries
            .iter()
            .filter(|lib| !lib.is_native && platform::should_include(&lib.rules))
            .collect();
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
                None => continue,
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
                None => continue,
            };

            let sem = semaphore.clone();
            let http = self.http.clone();
            let sha1 = lib.sha1.clone();
            let total_b = total_bytes.clone();
            let downloaded_b = downloaded_bytes.clone();
            let progress_ref = progress_cb.clone();

            tasks.push(tokio::spawn(async move {
                if !exists_ok {
                    let _permit = sem.acquire().await.unwrap();
                    let response = http.get(&url).send().await.map_err(|e| {
                        LaunchError::Launch(format!("HTTP error downloading {url}: {e}"))
                    })?;
                    if !response.status().is_success() {
                        return Err(LaunchError::Launch(format!(
                            "HTTP {} downloading {url}",
                            response.status()
                        )));
                    }
                    let file_size = response.content_length().unwrap_or(0) as u64;
                    total_b.fetch_add(file_size, Ordering::SeqCst);

                    let resp = response.bytes().await.map_err(|e| {
                        LaunchError::Launch(format!("Bytes error downloading {url}: {e}"))
                    })?;

                    if let Some(ref expected) = sha1 {
                        let mut hasher = Sha1::new();
                        hasher.update(&resp);
                        let computed = hex::encode(hasher.finalize());
                        if !computed.eq_ignore_ascii_case(expected) {
                            return Err(LaunchError::Launch(format!(
                                "SHA1 mismatch for {url}: expected {expected}, got {computed}"
                            )));
                        }
                    }

                    if let Some(parent) = full_local_path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let tmp = full_local_path.with_extension("tmp");
                    let _ = std::fs::write(&tmp, &resp);
                    let _ = std::fs::rename(&tmp, &full_local_path);

                    downloaded_b.fetch_add(resp.len() as u64, Ordering::SeqCst);
                }

                let cur = downloaded_b.load(Ordering::SeqCst);
                let tot = total_b.load(Ordering::SeqCst);
                progress_ref(cur, tot.max(cur), &display_name);
                Ok::<(), LaunchError>(())
            }));
        }

        for task in tasks {
            task.await
                .map_err(|e| LaunchError::Launch(e.to_string()))??;
        }

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
                                }
                            }
                        }
                    } else {
                        downloaded_cnt.fetch_add(size, Ordering::SeqCst);
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
            return Ok(());
        }
        self.download_file(url, target_path, expected_sha1).await
    }

    async fn download_file(
        &self,
        url: &str,
        target: &Path,
        expected_sha1: Option<&str>,
    ) -> Result<(), LaunchError> {
        let response = self.http.get(url).send().await?;
        if !response.status().is_success() {
            return Err(LaunchError::Launch(format!(
                "HTTP status {} downloading {url}",
                response.status()
            )));
        }
        let resp = response.bytes().await?;

        if let Some(expected) = expected_sha1 {
            let mut hasher = Sha1::new();
            hasher.update(&resp);
            let computed = hex::encode(hasher.finalize());
            if !computed.eq_ignore_ascii_case(expected) {
                return Err(LaunchError::Launch(format!(
                    "SHA1 mismatch for {url}: expected {expected}, got {computed}"
                )));
            }
        }

        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let tmp = target.with_extension("tmp");
        std::fs::write(&tmp, &resp)?;
        std::fs::rename(&tmp, target)?;

        Ok(())
    }
}
