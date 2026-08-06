use std::path::PathBuf;

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
    pub hits: Vec<ProjectInfo>,
    pub total_hits: usize,
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

/// A mod entry in a mods directory, with its enabled/disabled state.
#[derive(Debug, Clone)]
pub struct ModEntry {
    pub path: PathBuf,
    pub name: String,
    pub enabled: bool,
}
