use super::parsers::parse_library;
use crate::{Component, LaunchError, Requirement, VersionFile};
use reqwest::Client;

use super::prism::PRISM_META_BASE;

#[must_use]
pub fn neoforge_prism_meta_url() -> String {
    format!("{PRISM_META_BASE}/net.neoforged")
}

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
    let url = format!("{}/{neoforge_version}.json", neoforge_prism_meta_url());
    let resp: serde_json::Value = client.get(&url).send().await?.json().await?;
    let mut libraries = Vec::new();
    let main_class = resp
        .get("mainClass")
        .and_then(|v| v.as_str())
        .map(ToString::to_string);
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

use super::loader::MavenMetadata;

/// Fetches available `NeoForge` loader versions for a given Minecraft version.
///
/// # Errors
///
/// Returns [`LaunchError`] if the HTTP request fails.
pub async fn fetch_neoforge_loader_versions(
    client: &Client,
    mc_version: &str,
) -> Result<Vec<String>, LaunchError> {
    let url = format!(
        "{}/net/neoforged/neoforge/maven-metadata.xml",
        release_the_launcher_constants::urls::NEOFORGE_MAVEN
    );
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

    let mut versions = Vec::new();
    if let Ok(meta) = quick_xml::de::from_str::<MavenMetadata>(&resp) {
        for ver in meta.versioning.versions.version {
            if ver.starts_with(&neoforge_prefix) && !versions.contains(&ver) {
                versions.push(ver);
            }
        }
    }
    versions.reverse();
    Ok(versions)
}
