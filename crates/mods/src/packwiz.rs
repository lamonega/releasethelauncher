use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PackwizMod {
    pub name: String,
    pub filename: String,
    pub side: String,

    #[serde(rename = "x-mc-launcher")]
    pub launcher: Option<PackwizLauncherData>,

    #[serde(rename = "download")]
    pub download: PackwizDownload,

    #[serde(rename = "update")]
    pub update: Option<PackwizUpdate>,

    #[serde(rename = "dependencies")]
    pub dependencies: Option<HashMap<String, PackwizDependency>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PackwizLauncherData {
    pub loaders: Vec<String>,
    #[serde(rename = "mc-versions")]
    pub mc_versions: Vec<String>,
    #[serde(rename = "release-type")]
    pub release_type: String,
    #[serde(rename = "version-number")]
    pub version_number: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PackwizDownload {
    pub mode: String,
    pub url: Option<String>,
    #[serde(rename = "hash-format")]
    pub hash_format: String,
    pub hash: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PackwizUpdate {
    #[serde(rename = "modrinth")]
    pub modrinth: Option<PackwizModrinthUpdate>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PackwizModrinthUpdate {
    #[serde(rename = "mod-id")]
    pub mod_id: String,
    #[serde(rename = "version-id")]
    pub version_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PackwizDependency {
    #[serde(rename = "type")]
    pub dep_type: String,
}

/// Save packwiz mod metadata to a TOML file.
///
/// # Errors
///
/// Returns an error if serialization or file writing fails.
pub fn save_packwiz_metadata(
    index_dir: &Path,
    mod_name: &str,
    metadata: &PackwizMod,
) -> Result<(), std::io::Error> {
    let filename = format!("{}.pw.toml", mod_name.to_lowercase().replace(' ', "-"));
    let path = index_dir.join(&filename);
    let content = toml::to_string_pretty(metadata)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(path, content)
}

#[must_use]
pub fn load_packwiz_metadata(index_dir: &Path) -> Vec<(String, PackwizMod)> {
    let mut mods = Vec::new();

    if let Ok(entries) = fs::read_dir(index_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(mod_data) = toml::from_str::<PackwizMod>(&content) {
                        let name = path
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        mods.push((name, mod_data));
                    }
                }
            }
        }
    }

    mods
}

/// Remove packwiz mod metadata TOML file.
///
/// # Errors
///
/// Returns an error if the file exists but cannot be removed.
pub fn remove_packwiz_metadata(index_dir: &Path, mod_name: &str) -> Result<(), std::io::Error> {
    let filename = format!("{}.pw.toml", mod_name.to_lowercase().replace(' ', "-"));
    let path = index_dir.join(&filename);
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}
