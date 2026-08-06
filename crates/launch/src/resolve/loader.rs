use super::parsers::{parse_library, VersionJson};
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

#[derive(Debug, Deserialize)]
pub struct PrismIndex {
    #[serde(default)]
    pub versions: Vec<PrismVersionEntry>,
}

#[derive(Debug, Deserialize)]
pub struct PrismVersionEntry {
    pub version: String,
}

pub struct LoaderParams<'a> {
    pub client: &'a Client,
    pub base_url: &'a str,
    pub uid: &'a str,
    pub mc_version: &'a str,
    pub loader_version: Option<&'a str>,
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
        let index: PrismIndex = params
            .client
            .get(format!("{}/index.json", params.base_url))
            .send()
            .await?
            .json()
            .await?;
        index
            .versions
            .first()
            .map(|v| v.version.clone())
            .unwrap_or_else(|| params.default_fallback_version.to_string())
    };

    let loader_url = format!("{}/{chosen_loader_version}.json", params.base_url);
    let vj: VersionJson = params.client.get(&loader_url).send().await?.json().await?;
    let mut libraries = Vec::new();

    if let Some(libs) = &vj.libraries {
        for lib in libs {
            libraries.extend(parse_library(lib));
        }
    }
    let main_class = vj.main_class.clone();

    Ok(Component {
        uid: params.uid.to_string(),
        version: chosen_loader_version,
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
    let index: PrismIndex = client.get(&url).send().await?.json().await?;
    let mut versions = Vec::new();
    for v in index.versions {
        if !versions.contains(&v.version) {
            versions.push(v.version);
        }
    }
    Ok(versions)
}
