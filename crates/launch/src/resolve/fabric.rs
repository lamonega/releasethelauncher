use super::parsers::parse_library;
use crate::{Component, LaunchError, Requirement, VersionFile};
use reqwest::Client;

use super::prism::PRISM_META_BASE;

#[must_use]
pub fn fabric_prism_meta_url() -> String {
    format!("{PRISM_META_BASE}/net.fabricmc.fabric-loader")
}

/// Fetches the `Fabric` component for a given Minecraft version and optional loader version.
///
/// # Errors
///
/// Returns [`LaunchError`] if the HTTP request or JSON parsing fails.
pub async fn fetch_fabric_component(
    client: &Client,
    mc_version: &str,
    loader_version: Option<&str>,
) -> Result<Component, LaunchError> {
    let base_url = fabric_prism_meta_url();
    let chosen_loader_version = if let Some(lv) = loader_version {
        lv.to_string()
    } else {
        let index_resp: serde_json::Value = client
            .get(format!("{base_url}/index.json"))
            .send()
            .await?
            .json()
            .await?;
        index_resp
            .get("versions")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|v| v.get("version"))
            .and_then(|v| v.as_str())
            .unwrap_or("0.16.9")
            .to_string()
    };

    let loader_url = format!("{base_url}/{chosen_loader_version}.json");

    let resp: serde_json::Value = client.get(&loader_url).send().await?.json().await?;
    let mut libraries = Vec::new();
    let mut main_class = None;

    if let Some(libs) = resp.get("libraries").and_then(|v| v.as_array()) {
        for lib in libs {
            libraries.extend(parse_library(lib));
        }
    }
    if let Some(mc) = resp.get("mainClass").and_then(|v| v.as_str()) {
        main_class = Some(mc.to_string());
    }

    let loader_ver = loader_version.unwrap_or("unknown");

    Ok(Component {
        uid: "net.fabricmc.fabric-loader".to_string(),
        version: loader_ver.to_string(),
        is_locked: true,
        dependencies: vec![
            Requirement {
                uid: "net.minecraft".to_string(),
                suggests: Some(mc_version.to_string()),
                equals: Some(mc_version.to_string()),
            },
            Requirement {
                uid: "net.fabricmc.intermediary".to_string(),
                suggests: Some(mc_version.to_string()),
                equals: None,
            },
        ],
        conflicts: vec![
            "net.neoforged".into(),
            "net.minecraftforge".into(),
            "org.quiltmc".into(),
        ],
        version_file: VersionFile {
            main_class,
            libraries,
            ..VersionFile::default()
        },
    })
}

/// Fetches available `Fabric` loader versions for a given Minecraft version.
///
/// # Errors
///
/// Returns [`LaunchError`] if the HTTP request fails.
pub async fn fetch_fabric_loader_versions(
    client: &Client,
    _mc_version: &str,
) -> Result<Vec<String>, LaunchError> {
    let url = format!("{}/index.json", fabric_prism_meta_url());
    let resp: serde_json::Value = client.get(&url).send().await?.json().await?;
    let mut versions = Vec::new();
    if let Some(arr) = resp.get("versions").and_then(|v| v.as_array()) {
        for v in arr {
            if let Some(ver) = v.get("version").and_then(|s| s.as_str()) {
                if !versions.contains(&ver.to_string()) {
                    versions.push(ver.to_string());
                }
            }
        }
    }
    Ok(versions)
}
