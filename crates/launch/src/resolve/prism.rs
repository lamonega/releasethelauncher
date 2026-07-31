use super::parsers::parse_version_json;
use crate::{Component, LaunchError, VersionFile};
use reqwest::Client;

pub const PRISM_META_BASE: &str = "https://meta.prismlauncher.org/v1";

#[derive(Debug, Clone)]
pub struct VersionManifestEntry {
    pub id: String,
    pub url: String,
    pub version_type: String,
}

#[derive(Debug)]
pub struct VersionManifest {
    pub versions: Vec<VersionManifestEntry>,
}

/// Fetches the Minecraft version index from Prism Meta.
///
/// # Errors
///
/// Returns [`LaunchError`] if the HTTP request or JSON parsing fails.
pub async fn fetch_manifest(client: &Client) -> Result<VersionManifest, LaunchError> {
    let index_url = format!("{PRISM_META_BASE}/net.minecraft/index.json");
    let resp: serde_json::Value = client.get(&index_url).send().await?.json().await?;

    let versions: Vec<VersionManifestEntry> = resp["versions"].as_array().map_or(vec![], |arr| {
        arr.iter()
            .filter_map(|v| {
                let id = v["version"].as_str()?.to_string();
                let version_type = v["type"].as_str().unwrap_or("release").to_string();
                let url = format!("{PRISM_META_BASE}/net.minecraft/{id}.json");
                Some(VersionManifestEntry {
                    id,
                    url,
                    version_type,
                })
            })
            .collect()
    });

    Ok(VersionManifest { versions })
}

/// Fetches version metadata for a given URL or version ID from Prism Meta.
///
/// # Errors
///
/// Returns [`LaunchError`] if the HTTP request fails.
pub async fn fetch_version_metadata(
    client: &Client,
    url_or_version: &str,
) -> Result<VersionFile, LaunchError> {
    let url = if url_or_version.starts_with("http://") || url_or_version.starts_with("https://") {
        url_or_version.to_string()
    } else {
        format!("{PRISM_META_BASE}/net.minecraft/{url_or_version}.json")
    };

    let resp: serde_json::Value = client.get(&url).send().await?.json().await?;
    Ok(parse_version_json(&resp))
}

/// Fetches the vanilla component for a given Minecraft version exclusively from Prism Meta.
///
/// # Errors
///
/// Returns [`LaunchError`] if the HTTP request or JSON parsing fails.
pub async fn fetch_vanilla_component(
    client: &Client,
    manifest: Option<&VersionManifest>,
    version_id: &str,
) -> Result<Component, LaunchError> {
    let url = manifest
        .and_then(|m| {
            m.versions
                .iter()
                .find(|v| v.id == version_id)
                .map(|v| v.url.clone())
        })
        .unwrap_or_else(|| format!("{PRISM_META_BASE}/net.minecraft/{version_id}.json"));

    let resp: serde_json::Value = client.get(&url).send().await?.json().await?;
    let version_file = parse_version_json(&resp);
    let dependencies = super::parsers::parse_requires(&resp);

    Ok(Component {
        uid: "net.minecraft".to_string(),
        version: version_id.to_string(),
        is_locked: true,
        dependencies,
        conflicts: Vec::new(),
        version_file,
    })
}
