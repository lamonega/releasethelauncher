use reqwest::Client;
use sha1::Digest;
use sha1::Sha1;
use std::path::{Path, PathBuf};

use crate::{LaunchError, Library};

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
        let parts: Vec<&str> = lib.name.split(':').collect();
        if parts.len() < 3 {
            return None;
        }

        let group = parts[0].replace('.', "/");
        let artifact = parts[1];
        let version = parts[2];
        let classifier = parts.get(3);

        let filename = if let Some(cls) = classifier {
            format!("{artifact}-{version}-{cls}.jar")
        } else {
            format!("{artifact}-{version}.jar")
        };

        let path = format!("{group}/{artifact}/{version}/{filename}");

        if lib.name.contains("net.minecraftforge") || lib.name.contains("cpw.mods") {
            Some(format!("{FORGE_MAVEN}/{path}"))
        } else if lib.name.contains("net.fabricmc") {
            Some(format!("{FABRIC_MAVEN}/{path}"))
        } else if lib.name.contains("net.neoforged") {
            Some(format!("{NEOFORGE_MAVEN}/{path}"))
        } else {
            lib.url
                .as_ref()
                .map(|u| format!("{}/{path}", u.trim_end_matches('/')))
                .or(Some(format!("{MAVEN_CENTRAL}/{path}")))
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

        let filename = if let Some(cls) = classifier {
            format!("{artifact}-{version}-{cls}.jar")
        } else {
            format!("{artifact}-{version}.jar")
        };

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
        progress: impl Fn(usize, usize) + Send + Sync,
    ) -> Result<(), LaunchError> {
        let total = libraries.len();
        let mut downloaded = 0;

        for lib in libraries {
            if lib.is_native {
                continue;
            }

            let local_path = Self::local_path_for_library(lib).ok_or_else(|| {
                LaunchError::Launch(format!("Invalid library coordinates: {}", lib.name))
            })?;

            let full_local_path = self.libraries_dir.join(&local_path);

            if !full_local_path.exists() {
                if let Some(url) = Self::maven_url_for_library(lib) {
                    match self
                        .download_file(&url, &full_local_path, lib.sha1.as_deref())
                        .await
                    {
                        Ok(()) => {}
                        Err(e) => {
                            eprintln!("Warning: Failed to download {}: {}", lib.name, e);
                        }
                    }
                }
            }

            downloaded += 1;
            progress(downloaded, total);
        }

        Ok(())
    }

    /// # Errors
    /// Returns an error if the asset index cannot be read or a download fails.
    pub async fn download_asset_objects(
        &self,
        http: &Client,
        asset_index_path: &Path,
    ) -> Result<(), LaunchError> {
        let index_content = std::fs::read_to_string(asset_index_path)?;
        let index: serde_json::Value = serde_json::from_str(&index_content)?;

        let objects_dir = self.cache_dir.join("assets").join("objects");

        if let Some(objects) = index.get("objects").and_then(|v| v.as_object()) {
            for (_name, obj) in objects {
                let hash = obj.get("hash").and_then(|v| v.as_str()).unwrap_or("");
                if hash.is_empty() {
                    continue;
                }

                let prefix = &hash[..2.min(hash.len())];
                let target_path = objects_dir.join(prefix).join(hash);

                if !target_path.exists() {
                    let url = format!("https://resources.download.minecraft.net/{prefix}/{hash}");
                    if let Some(parent) = target_path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    let resp = http.get(&url).send().await?.bytes().await?;
                    std::fs::write(&target_path, &resp)?;
                }
            }
        }

        Ok(())
    }

    async fn download_file(
        &self,
        url: &str,
        target: &Path,
        expected_sha1: Option<&str>,
    ) -> Result<(), LaunchError> {
        let resp = self.http.get(url).send().await?.bytes().await?;

        if let Some(expected) = expected_sha1 {
            let mut hasher = Sha1::new();
            hasher.update(&resp);
            let computed = hex::encode(hasher.finalize());
            if computed != expected {
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
