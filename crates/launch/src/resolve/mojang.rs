use super::parsers::parse_version_json;
use crate::{Component, LaunchError, VersionFile};
use reqwest::Client;

pub const VERSION_MANIFEST_URL: &str =
    "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";

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

/// Fetches the Mojang version manifest.
///
/// # Errors
///
/// Returns [`LaunchError`] if the HTTP request or JSON parsing fails.
pub async fn fetch_manifest(client: &Client) -> Result<VersionManifest, LaunchError> {
    let resp: serde_json::Value = client
        .get(VERSION_MANIFEST_URL)
        .send()
        .await?
        .json()
        .await?;

    let versions: Vec<VersionManifestEntry> = resp["versions"].as_array().map_or(vec![], |arr| {
        arr.iter()
            .filter_map(|v| {
                Some(VersionManifestEntry {
                    id: v["id"].as_str()?.to_string(),
                    url: v["url"].as_str()?.to_string(),
                    version_type: v["type"].as_str().unwrap_or("release").to_string(),
                })
            })
            .collect()
    });

    Ok(VersionManifest { versions })
}

/// Fetches version metadata for a given URL.
///
/// # Errors
///
/// Returns [`LaunchError`] if the HTTP request fails.
pub async fn fetch_version_metadata(
    client: &Client,
    url: &str,
) -> Result<VersionFile, LaunchError> {
    let resp: serde_json::Value = client.get(url).send().await?.json().await?;
    Ok(parse_version_json(&resp))
}

/// Fetches the vanilla component for a given Minecraft version.
///
/// # Errors
///
/// Returns [`LaunchError`] if the version is not found in the manifest or network fails.
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
        .ok_or_else(|| LaunchError::VersionNotFound(version_id.to_string()))?;

    let version_file = fetch_version_metadata(client, &url).await?;
    Ok(Component {
        uid: "net.minecraft".to_string(),
        version: version_id.to_string(),
        is_locked: true,
        dependencies: Vec::new(),
        conflicts: Vec::new(),
        version_file,
    })
}
