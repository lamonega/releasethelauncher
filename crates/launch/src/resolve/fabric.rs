use crate::{Component, LaunchError};
use reqwest::Client;

use release_the_launcher_constants::urls::PRISM_META_BASE;
use super::loader::{fetch_meta_component, fetch_meta_loader_versions};

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
    fetch_meta_component(
        client,
        &fabric_prism_meta_url(),
        "net.fabricmc.fabric-loader",
        mc_version,
        loader_version,
        &["net.neoforged", "net.minecraftforge", "org.quiltmc"],
        "net.fabricmc.intermediary",
        "0.16.9",
    )
    .await
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
    fetch_meta_loader_versions(client, &fabric_prism_meta_url()).await
}
