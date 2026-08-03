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

pub struct LoaderParams<'a> {
    pub client: &'a Client,
    pub base_url: &'a str,
    pub uid: &'a str,
    pub mc_version: &'a str,
    pub loader_version: Option<&'a str>,
    pub conflict_uids: Vec<&'a str>,
    pub intermediary_uid: &'a str,
    pub default_fallback_version: &'a str,
}

/// Fetches a loader component (e.g. Fabric or Quilt) from Prism metadata format.
///
/// # Errors
///
/// Returns [`LaunchError`] if the HTTP request or JSON parsing fails.
pub async fn fetch_meta_component(params: LoaderParams<'_>) -> Result<Component, LaunchError> {
    let chosen_loader_version = if let Some(lv) = params.loader_version.filter(|lv| !lv.is_empty())
    {
        lv.to_string()
    } else {
        let index_resp: serde_json::Value = params
            .client
            .get(format!("{}/index.json", params.base_url))
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
            .unwrap_or(params.default_fallback_version)
            .to_string()
    };

    let loader_url = format!("{}/{chosen_loader_version}.json", params.base_url);
    let resp: serde_json::Value = params.client.get(&loader_url).send().await?.json().await?;
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

    let loader_ver = &chosen_loader_version;

    Ok(Component {
        uid: params.uid.to_string(),
        version: loader_ver.to_string(),
        is_locked: true,
        dependencies: vec![
            Requirement {
                uid: "net.minecraft".to_string(),
                suggests: Some(params.mc_version.to_string()),
                equals: Some(params.mc_version.to_string()),
            },
            Requirement {
                uid: params.intermediary_uid.to_string(),
                suggests: Some(params.mc_version.to_string()),
                equals: None,
            },
        ],
        conflicts: params
            .conflict_uids
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
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
