use crate::{Component, LaunchError, Requirement, VersionFile};
use super::parsers::parse_library;
use reqwest::Client;

pub const FABRIC_META_URL: &str = "https://meta.fabricmc.net/v2";

pub async fn fetch_fabric_component(
    client: &Client,
    mc_version: &str,
    loader_version: Option<&str>,
) -> Result<Component, LaunchError> {
    let loader_url = if let Some(lv) = loader_version {
        format!("{FABRIC_META_URL}/versions/loader/{mc_version}/{lv}/profile/json")
    } else {
        let versions: Vec<serde_json::Value> = client
            .get(format!("{FABRIC_META_URL}/versions/loader/{mc_version}"))
            .send()
            .await?
            .json()
            .await?;
        let latest = versions.last().ok_or_else(|| {
            LaunchError::VersionNotFound("No Fabric loader version found".into())
        })?;
        let loader_ver = latest
            .get("loader")
            .and_then(|v| v.get("version"))
            .and_then(|v| v.as_str())
            .unwrap_or("0.16.9");
        format!("{FABRIC_META_URL}/versions/loader/{mc_version}/{loader_ver}/profile/json")
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

pub async fn fetch_fabric_loader_versions(
    client: &Client,
    mc_version: &str,
) -> Result<Vec<String>, LaunchError> {
    let url = format!("{FABRIC_META_URL}/versions/loader/{mc_version}");
    let resp: Vec<serde_json::Value> = client.get(&url).send().await?.json().await?;
    let mut versions = Vec::new();
    for v in resp {
        let is_stable = v
            .get("loader")
            .and_then(|l| l.get("stable"))
            .and_then(|s| s.as_bool())
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
