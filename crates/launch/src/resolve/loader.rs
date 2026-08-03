use super::parsers::parse_library;
use crate::{Component, LaunchError, Requirement, VersionFile};
use reqwest::Client;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct MavenMetadata {
    pub versioning: Versioning,
}

#[derive(Debug, Deserialize)]
pub struct Versioning {
    pub versions: Versions,
}

#[derive(Debug, Deserialize)]
pub struct Versions {
    #[serde(default)]
    pub version: Vec<String>,
}

/// Fetches a loader component (e.g. Fabric or Quilt) from Prism metadata format.
///
/// # Errors
///
/// Returns [`LaunchError`] if the HTTP request or JSON parsing fails.
#[allow(clippy::too_many_arguments)]
pub async fn fetch_meta_component(
    client: &Client,
    base_url: &str,
    uid: &str,
    mc_version: &str,
    loader_version: Option<&str>,
    conflict_uids: &[&str],
    intermediary_uid: &str,
    default_fallback_version: &str,
) -> Result<Component, LaunchError> {
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
            .unwrap_or(default_fallback_version)
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
        uid: uid.to_string(),
        version: loader_ver.to_string(),
        is_locked: true,
        dependencies: vec![
            Requirement {
                uid: "net.minecraft".to_string(),
                suggests: Some(mc_version.to_string()),
                equals: Some(mc_version.to_string()),
            },
            Requirement {
                uid: intermediary_uid.to_string(),
                suggests: Some(mc_version.to_string()),
                equals: None,
            },
        ],
        conflicts: conflict_uids.iter().map(|s| (*s).to_string()).collect(),
        version_file: VersionFile {
            main_class,
            libraries,
            ..VersionFile::default()
        },
    })
}

/// Fetches available loader versions from Prism metadata format.
///
/// # Errors
///
/// Returns [`LaunchError`] if the HTTP request fails.
pub async fn fetch_meta_loader_versions(
    client: &Client,
    base_url: &str,
) -> Result<Vec<String>, LaunchError> {
    let url = format!("{base_url}/index.json");
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
