use super::parsers::parse_library;
use crate::{Component, LaunchError, Requirement, VersionFile};
use reqwest::Client;

pub const NEOFORGE_PRISM_META_URL: &str = "https://meta.prismlauncher.org/v1/net.neoforged";

/// Fetches the `NeoForge` component for a given Minecraft and `NeoForge` version.
///
/// # Errors
///
/// Returns [`LaunchError`] if the HTTP request or JSON parsing fails.
pub async fn fetch_neoforge_component(
    client: &Client,
    mc_version: &str,
    neoforge_version: &str,
) -> Result<Component, LaunchError> {
    let url = format!("{NEOFORGE_PRISM_META_URL}/{neoforge_version}.json");
    let resp: serde_json::Value = client.get(&url).send().await?.json().await?;
    let mut libraries = Vec::new();
    let mut main_class = None;

    if let Some(mc_main) = resp.get("mainClass").and_then(|v| v.as_str()) {
        main_class = Some(mc_main.to_string());
    }
    if let Some(libs) = resp.get("libraries").and_then(|v| v.as_array()) {
        for lib in libs {
            libraries.extend(parse_library(lib));
        }
    }

    Ok(Component {
        uid: "net.neoforged".to_string(),
        version: neoforge_version.to_string(),
        is_locked: true,
        dependencies: vec![Requirement {
            uid: "net.minecraft".to_string(),
            suggests: Some(mc_version.to_string()),
            equals: Some(mc_version.to_string()),
        }],
        conflicts: vec![
            "net.minecraftforge".into(),
            "net.fabricmc.fabric-loader".into(),
            "org.quiltmc".into(),
        ],
        version_file: VersionFile {
            main_class,
            libraries,
            ..VersionFile::default()
        },
    })
}

/// Fetches available `NeoForge` loader versions for a given Minecraft version.
///
/// # Errors
///
/// Returns [`LaunchError`] if the HTTP request fails.
pub async fn fetch_neoforge_loader_versions(
    client: &Client,
    mc_version: &str,
) -> Result<Vec<String>, LaunchError> {
    let url = "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml";
    let resp = client.get(url).send().await?.text().await?;
    let neoforge_prefix = mc_version.strip_prefix("1.").map_or_else(
        || mc_version.to_string(),
        |stripped| {
            let parts: Vec<&str> = stripped.split('.').collect();
            if parts.len() >= 2 {
                format!("{}.{}.", parts[0], parts[1])
            } else if parts.len() == 1 {
                format!("{}.0.", parts[0])
            } else {
                mc_version.to_string()
            }
        },
    );

    let tag_prefix = format!("<version>{neoforge_prefix}");
    let mut versions = Vec::new();
    for line in resp.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(&tag_prefix) && trimmed.ends_with("</version>") {
            let ver = trimmed
                .strip_prefix("<version>")
                .and_then(|s| s.strip_suffix("</version>"))
                .unwrap_or("");
            if !ver.is_empty() && !versions.contains(&ver.to_string()) {
                versions.push(ver.to_string());
            }
        }
    }
    versions.reverse();
    Ok(versions)
}
