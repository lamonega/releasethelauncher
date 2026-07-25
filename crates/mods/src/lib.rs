pub mod modrinth;
pub mod modrinth_types;
pub mod packwiz;
pub mod parser;

pub use modrinth::ModrinthProvider;

use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ModsError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("TOML error: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("Provider error: {0}")]
    Provider(String),
}

#[derive(Debug, Clone)]
pub enum Side {
    Client,
    Server,
    Universal,
}

impl Side {
    #[must_use]
    pub const fn as_str(&self) -> &str {
        match self {
            Self::Client => "client",
            Self::Server => "server",
            Self::Universal => "universal",
        }
    }
}

#[derive(Debug, Clone)]
pub enum ReleaseType {
    Release,
    Beta,
    Alpha,
}

impl ReleaseType {
    #[must_use]
    pub const fn as_str(&self) -> &str {
        match self {
            Self::Release => "release",
            Self::Beta => "beta",
            Self::Alpha => "alpha",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SortOrder {
    Relevance,
    Downloads,
    Follows,
    Newest,
    Updated,
}

impl SortOrder {
    #[must_use]
    pub const fn as_str(&self) -> &str {
        match self {
            Self::Relevance => "relevance",
            Self::Downloads => "downloads",
            Self::Follows => "follows",
            Self::Newest => "newest",
            Self::Updated => "updated",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SearchArgs {
    pub query: String,
    pub offset: usize,
    pub limit: usize,
    pub loaders: Vec<String>,
    pub mc_versions: Vec<String>,
    pub categories: Vec<String>,
    pub sort: SortOrder,
}

#[derive(Debug, Clone)]
pub struct SearchResults {
    pub hits: Vec<ProjectSummary>,
    pub total_hits: usize,
}

#[derive(Debug, Clone)]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub author: String,
    pub icon_url: Option<String>,
    pub downloads: u64,
    pub side: Side,
}

#[derive(Debug, Clone)]
pub struct ProjectInfo {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub authors: Vec<String>,
    pub icon_url: Option<String>,
    pub website_url: Option<String>,
    pub downloads: u64,
    pub side: Side,
}

#[derive(Debug, Clone)]
pub struct ModVersion {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub version_number: String,
    pub release_type: ReleaseType,
    pub mc_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub download_url: Option<String>,
    pub filename: String,
    pub hash: Option<String>,
    pub hash_type: Option<String>,
    pub file_size: u64,
}

#[derive(Debug, Clone)]
pub struct InstalledMod {
    pub path: PathBuf,
    pub hash: String,
    pub hash_type: String,
    pub project_id: Option<String>,
    pub version_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ModUpdate {
    pub installed: InstalledMod,
    pub latest: ModVersion,
}

#[derive(Debug, Clone)]
pub struct ModDetails {
    pub mod_id: String,
    pub name: String,
    pub version: String,
    pub mc_version: Option<String>,
    pub description: String,
    pub authors: Vec<String>,
    pub dependencies: Vec<String>,
    pub side: Option<String>,
}

#[async_trait::async_trait]
pub trait ModProvider: Send + Sync {
    fn name(&self) -> &str;

    async fn search(&self, args: SearchArgs) -> Result<SearchResults, ModsError>;

    async fn get_versions(
        &self,
        project_id: &str,
        mc_versions: &[String],
        loaders: &[String],
    ) -> Result<Vec<ModVersion>, ModsError>;

    async fn get_project(&self, project_id: &str) -> Result<ProjectInfo, ModsError>;

    async fn check_updates(
        &self,
        installed: &[InstalledMod],
        mc_versions: &[String],
        loaders: &[String],
    ) -> Result<Vec<ModUpdate>, ModsError>;

    async fn download_mod(
        &self,
        version: &ModVersion,
        target_dir: &Path,
    ) -> Result<PathBuf, ModsError>;
}
