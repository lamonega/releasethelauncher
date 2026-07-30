#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines,
    clippy::option_if_let_else,
    clippy::module_name_repetitions,
    clippy::struct_field_names,
    clippy::similar_names,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::redundant_closure_for_method_calls,
    clippy::map_unwrap_or,
    clippy::inconsistent_struct_constructor,
    clippy::doc_markdown,
    clippy::single_match_else,
    clippy::use_self,
    clippy::uninlined_format_args
)]

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

/// A mod entry in a mods directory, with its enabled/disabled state.
#[derive(Debug, Clone)]
pub struct ModEntry {
    pub path: PathBuf,
    pub name: String,
    pub enabled: bool,
}

/// Lists all `.jar` and `.jar.disabled` files in a mods directory.
#[must_use]
pub fn list_mods(mods_dir: &Path) -> Vec<ModEntry> {
    let mut entries = Vec::new();
    if !mods_dir.exists() {
        return entries;
    }
    if let Ok(read_dir) = std::fs::read_dir(mods_dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".jar.disabled") {
                let mod_name = name.trim_end_matches(".disabled").to_string();
                entries.push(ModEntry {
                    path,
                    name: mod_name,
                    enabled: false,
                });
            } else if std::path::Path::new(&name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("jar"))
            {
                entries.push(ModEntry {
                    path,
                    name,
                    enabled: true,
                });
            }
        }
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

/// Enables a mod by renaming its file from `*.jar.disabled` to `*.jar`.
///
/// # Errors
///
/// Returns an error if the rename fails.
pub fn enable_mod(path: &Path) -> Result<(), ModsError> {
    if let Some(name) = path.file_name() {
        let name_str = name.to_string_lossy();
        if name_str.ends_with(".disabled") {
            let new_name = name_str.trim_end_matches(".disabled");
            let new_path = path.with_file_name(new_name);
            if new_path.exists() {
                let unique = get_unique_resource_name(&new_path);
                std::fs::rename(path, &unique)?;
            } else {
                std::fs::rename(path, &new_path)?;
            }
            return Ok(());
        }
    }
    Err(ModsError::Provider("File is not a disabled mod".into()))
}

/// Disables a mod by renaming its file from `*.jar` to `*.jar.disabled`.
///
/// # Errors
///
/// Returns an error if the rename fails.
pub fn disable_mod(path: &Path) -> Result<(), ModsError> {
    if let Some(name) = path.file_name() {
        let name_str = name.to_string_lossy();
        if name_str.ends_with(".jar") {
            let new_name = format!("{name_str}.disabled");
            let new_path = path.with_file_name(&new_name);
            if new_path.exists() {
                let unique = get_unique_resource_name(&new_path);
                std::fs::rename(path, &unique)?;
            } else {
                std::fs::rename(path, &new_path)?;
            }
            return Ok(());
        }
    }
    Err(ModsError::Provider("File is not an enabled mod".into()))
}

/// Returns a unique file path by appending `.duplicate` suffixes.
fn get_unique_resource_name(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .map_or_else(|| "mod".to_string(), |s| s.to_string_lossy().to_string());
    let ext = path
        .extension()
        .map_or_else(String::new, |e| format!(".{}", e.to_string_lossy()));
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut counter = 1;
    loop {
        let candidate = parent.join(format!("{stem}.duplicate{ext}{counter}"));
        if !candidate.exists() {
            return candidate;
        }
        counter += 1;
    }
}
