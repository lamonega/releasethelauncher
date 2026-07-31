use crate::{Component, LaunchError, Requirement, VersionFile};
use super::parsers::parse_library;
use reqwest::Client;

pub const QUILT_META_URL: &str = "https://meta.quiltmc.org/v3";

/// Fetches the `Quilt` component for a given Minecraft version and optional loader version.
///
/// # Errors
///
/// Returns [`LaunchError`] if the HTTP request or JSON parsing fails.
pub async fn fetch_quilt_component(
    client: &Client,
    mc_version: &str,
    loader_version: Option<&str>,
) -> Result<Component, LaunchError> {
    let loader_url = if let Some(lv) = loader_version {
        format!("{QUILT_META_URL}/versions/loader/{mc_version}/{lv}/profile/json")
    } else {
        let versions: Vec<serde_json::Value> = client
            .get(format!("{QUILT_META_URL}/versions/loader/{mc_version}"))
            .send()
            .await?
            .json()
            .await?;
        let latest = versions.first().ok_or_else(|| {
            LaunchError::VersionNotFound("No Quilt loader version found".into())
        })?;
        let loader_ver = latest
            .get("loader")
            .and_then(|v| v.get("version"))
            .and_then(|v| v.as_str())
            .unwrap_or("0.26.13");
        format!("{QUILT_META_URL}/versions/loader/{mc_version}/{loader_ver}/profile/json")
    };

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
        uid: "org.quiltmc.quilt-loader".to_string(),
        version: loader_ver.to_string(),
        is_locked: true,
        dependencies: vec![
            Requirement {
                uid: "net.minecraft".to_string(),
                suggests: Some(mc_version.to_string()),
                equals: Some(mc_version.to_string()),
            },
            Requirement {
                uid: "org.quiltmc.quilt-intermediary".to_string(),
                suggests: Some(mc_version.to_string()),
                equals: None,
            },
        ],
        conflicts: vec![
            "net.neoforged".into(),
            "net.minecraftforge".into(),
            "net.fabricmc.fabric-loader".into(),
        ],
        version_file: VersionFile {
            main_class,
            libraries,
            ..VersionFile::default()
        },
    })
}

/// Fetches available `Quilt` loader versions for a given Minecraft version.
///
/// # Errors
///
/// Returns [`LaunchError`] if the HTTP request fails.
pub async fn fetch_quilt_loader_versions(
    client: &Client,
    mc_version: &str,
) -> Result<Vec<String>, LaunchError> {
    let url = format!("{QUILT_META_URL}/versions/loader/{mc_version}");
    let resp: Vec<serde_json::Value> = client.get(&url).send().await?.json().await?;
    let mut versions = Vec::new();
    for v in resp {
        let is_stable = v
            .get("loader")
            .and_then(|l| l.get("stable"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        if !is_stable {
            continue;
        }
        if let Some(ver) = v
            .get("loader")
            .and_then(|l| l.get("version"))
            .and_then(|s| s.as_str())
        {
            if !versions.contains(&ver.to_string()) {
                versions.push(ver.to_string());
            }
        }
    }
    Ok(versions)
}
