use serde::Deserialize;
use sha1::Digest;
use sha1::Sha1;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::LaunchError;

#[derive(Debug, Deserialize)]
pub struct AssetIndexJson {
    objects: HashMap<String, AssetObject>,
    #[serde(rename = "virtual", alias = "virtual_map")]
    virtual_map: Option<bool>,
    map_to_resources: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct AssetObject {
    hash: String,
}

pub struct AssetManager {
    objects_dir: PathBuf,
    index_dir: PathBuf,
}

impl AssetManager {
    #[must_use]
    pub fn new(cache_dir: &Path) -> Self {
        Self {
            objects_dir: cache_dir.join("assets").join("objects"),
            index_dir: cache_dir.join("assets").join("indexes"),
        }
    }

    #[must_use]
    pub fn asset_index_path(&self, asset_id: &str) -> PathBuf {
        self.index_dir.join(format!("{asset_id}.json"))
    }

    #[must_use]
    pub fn asset_object_path(&self, hash: &str) -> PathBuf {
        let prefix = &hash[..2.min(hash.len())];
        self.objects_dir.join(prefix).join(hash)
    }

    /// # Errors
    /// Returns an error if the HTTP request fails or the SHA1 hash does not match.
    pub async fn download_asset_index(
        &self,
        http: &reqwest::Client,
        asset_index_id: &str,
        url: &str,
        sha1: Option<&str>,
    ) -> Result<PathBuf, LaunchError> {
        let path = self.asset_index_path(asset_index_id);
        if path.exists() {
            return Ok(path);
        }

        let resp = http.get(url).send().await?.bytes().await?;

        if let Some(expected_sha1) = sha1 {
            let mut hasher = Sha1::new();
            hasher.update(&resp);
            let computed = hex::encode(hasher.finalize());
            if computed != expected_sha1 {
                return Err(LaunchError::Launch(format!(
                    "Asset index SHA1 mismatch: expected {expected_sha1}, got {computed}"
                )));
            }
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, &resp)?;
        Ok(path)
    }

    /// # Errors
    /// Returns an error if the file cannot be read or parsed as JSON.
    pub fn parse_asset_index(&self, index_path: &Path) -> Result<AssetIndexJson, LaunchError> {
        let content = fs::read_to_string(index_path)?;
        let index: AssetIndexJson = serde_json::from_str(&content)?;
        Ok(index)
    }

    /// # Errors
    /// Returns an error if file system operations fail.
    pub fn reconstruct_virtual_assets(
        &self,
        instance_minecraft_dir: &Path,
        asset_index: &AssetIndexJson,
    ) -> Result<(), LaunchError> {
        if !asset_index.virtual_map.unwrap_or(false)
            && !asset_index.map_to_resources.unwrap_or(false)
        {
            return Ok(());
        }

        let target_dir = if asset_index.map_to_resources.unwrap_or(false) {
            instance_minecraft_dir.join("resources")
        } else {
            instance_minecraft_dir
                .parent()
                .unwrap_or(instance_minecraft_dir)
                .join("assets")
                .join("virtual")
                .join("legacy")
        };

        for (name, obj) in &asset_index.objects {
            let source = self.asset_object_path(&obj.hash);
            let target = target_dir.join(name);

            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }

            if !target.exists() && source.exists() {
                fs::copy(&source, &target)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_legacy_index_without_size_field() {
        // pre-1.6 indexes carry only `hash` per object (no `size`/`_size`).
        let dir = tempfile::tempdir().unwrap();
        let index = dir.path().join("pre-1.6.json");
        std::fs::write(
            &index,
            r#"{"map_to_resources": true, "objects": {"icon_16x16.png": {"hash": "bdf48ef6b5d0d23bbb02e17d04865216179f510a"}}}"#,
        )
        .unwrap();
        let mgr = AssetManager::new(dir.path());
        let parsed = mgr.parse_asset_index(&index).unwrap();
        assert!(parsed.map_to_resources.unwrap_or(false));
        assert_eq!(parsed.objects.len(), 1);
    }
}
