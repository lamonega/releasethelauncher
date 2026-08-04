//! Mod management: the [`ModProvider`] trait, the [`ModrinthProvider`]
//! implementation ([`modrinth`]) and mod-metadata parsing ([`parser`]).
pub mod modrinth;
pub(crate) mod modrinth_types;
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

pub trait ModProvider: Send + Sync {
    fn name(&self) -> &str;

    fn search(
        &self,
        args: SearchArgs,
    ) -> impl std::future::Future<Output = Result<SearchResults, ModsError>> + Send;

    fn get_versions(
        &self,
        project_id: &str,
        mc_versions: &[String],
        loaders: &[String],
    ) -> impl std::future::Future<Output = Result<Vec<ModVersion>, ModsError>> + Send;

    fn get_project(
        &self,
        project_id: &str,
    ) -> impl std::future::Future<Output = Result<ProjectInfo, ModsError>> + Send;

    fn check_updates(
        &self,
        installed: &[InstalledMod],
        mc_versions: &[String],
        loaders: &[String],
    ) -> impl std::future::Future<Output = Result<Vec<ModUpdate>, ModsError>> + Send;

    fn download_mod(
        &self,
        version: &ModVersion,
        target_dir: &Path,
    ) -> impl std::future::Future<Output = Result<PathBuf, ModsError>> + Send;
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

fn toggle_mod_extension(
    path: &Path,
    expected_suffix: &str,
    new_path: impl FnOnce(&str) -> PathBuf,
    err_msg: &str,
) -> Result<(), ModsError> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .ok_or_else(|| ModsError::Provider(err_msg.into()))?;
    if name.ends_with(expected_suffix) {
        let target = new_path(&name);
        let final_path = if target.exists() {
            get_unique_resource_name(&target)
        } else {
            target
        };
        std::fs::rename(path, &final_path)?;
        Ok(())
    } else {
        Err(ModsError::Provider(err_msg.into()))
    }
}

/// Enables a mod by renaming its file from `*.jar.disabled` to `*.jar`.
///
/// # Errors
///
/// Returns an error if the rename fails.
pub fn enable_mod(path: &Path) -> Result<(), ModsError> {
    toggle_mod_extension(
        path,
        ".disabled",
        |n| path.with_file_name(n.trim_end_matches(".disabled")),
        "File is not a disabled mod",
    )
}

/// Disables a mod by renaming its file from `*.jar` to `*.jar.disabled`.
///
/// # Errors
///
/// Returns an error if the rename fails.
pub fn disable_mod(path: &Path) -> Result<(), ModsError> {
    toggle_mod_extension(
        path,
        ".jar",
        |n| path.with_file_name(format!("{n}.disabled")),
        "File is not an enabled mod",
    )
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
        let candidate = parent.join(format!("{stem}.duplicate{counter}{ext}"));
        if !candidate.exists() {
            return candidate;
        }
        counter += 1;
    }
}

/// Joins `rel` onto `base`, guaranteeing the result stays inside `base`.
///
/// Absolute paths and any `..` component are rejected outright. When the parent
/// directory of the destination already exists, it is canonicalized and must
/// still resolve under a canonicalized `base`; if the directory does not exist
/// yet that check is skipped, but the component checks always apply.
///
/// # Errors
///
/// Returns an error if `rel` is absolute, contains a `..` component, or the
/// canonicalized destination escapes `base`.
pub(crate) fn safe_join_under(base: &Path, rel: &Path) -> Result<PathBuf, ModsError> {
    if rel.as_os_str().is_empty() {
        return Ok(base.to_path_buf());
    }
    if rel.is_absolute()
        || rel.components().any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(ModsError::Provider("Unsafe path".into()));
    }

    let joined = base.join(rel);

    if let Some(parent) = joined.parent() {
        if parent.exists() {
            if let (Ok(canon_base), Ok(canon_parent)) = (base.canonicalize(), parent.canonicalize())
            {
                if !canon_parent.starts_with(&canon_base) {
                    return Err(ModsError::Provider("Unsafe path".into()));
                }
            }
        }
    }

    Ok(joined)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_join_under_rejects_unsafe_components() {
        let base = Path::new("/tmp/rtl_mods_base");
        assert!(safe_join_under(base, Path::new("mods/legit.jar")).is_ok());
        assert!(safe_join_under(base, Path::new("legit")).is_ok());
        assert!(safe_join_under(base, Path::new("")).is_ok());

        assert!(safe_join_under(base, Path::new("/etc/passwd")).is_err());
        assert!(safe_join_under(base, Path::new("../../evil")).is_err());
        assert!(safe_join_under(base, Path::new("mods/../../evil")).is_err());
        assert!(safe_join_under(base, Path::new("resources/../..")).is_err());
        assert!(safe_join_under(base, Path::new("overrides/../../evil.sh")).is_err());
    }

    #[test]
    fn safe_join_under_keeps_result_under_base() {
        let temp_dir = std::env::temp_dir().join(format!(
            "rtl_mods_path_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::remove_dir_all(&temp_dir).ok();
        std::fs::create_dir_all(&temp_dir).unwrap();

        let base = temp_dir.join("instance").join(".minecraft");
        let joined = safe_join_under(&base, Path::new("mods/legit.jar")).unwrap();
        assert_eq!(joined, base.join("mods/legit.jar"));
        assert!(joined.starts_with(&base));

        std::fs::remove_dir_all(&temp_dir).ok();
    }
}
