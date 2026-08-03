use crate::{Component, LaunchError};
use reqwest::Client;

use release_the_launcher_constants::urls::PRISM_META_BASE;
use super::loader::{fetch_meta_component, fetch_meta_loader_versions};

#[must_use]
pub fn quilt_prism_meta_url() -> String {
    format!("{PRISM_META_BASE}/org.quiltmc.quilt-loader")
}

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
    fetch_meta_component(
        client,
        &quilt_prism_meta_url(),
        "org.quiltmc.quilt-loader",
        mc_version,
        loader_version,
        &["net.neoforged", "net.minecraftforge", "net.fabricmc.fabric-loader"],
        "org.quiltmc.quilt-intermediary",
        "0.26.13",
    )
    .await
}

/// Fetches available `Quilt` loader versions for a given Minecraft version.
///
/// # Errors
///
/// Returns [`LaunchError`] if the HTTP request fails.
pub async fn fetch_quilt_loader_versions(
    client: &Client,
    _mc_version: &str,
) -> Result<Vec<String>, LaunchError> {
    fetch_meta_loader_versions(client, &quilt_prism_meta_url()).await
}
