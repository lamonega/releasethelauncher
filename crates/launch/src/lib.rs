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
    clippy::uninlined_format_args,
    clippy::must_use_candidate,
    clippy::manual_strip,
    clippy::unnecessary_map_or,
    clippy::match_same_arms,
    clippy::collapsible_if,
    clippy::assigning_clones,
    clippy::items_after_statements,
    clippy::unnecessary_sort_by,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::manual_let_else,
    clippy::if_not_else,
    clippy::unnecessary_lazy_evaluations
)]

pub mod assets;
pub mod command;
pub mod download;
pub mod java;
pub mod memory;
pub mod natives;
pub mod platform;
pub mod profile;
pub mod resolve;

use thiserror::Error;

pub use command::{
    build_command, launch_game, run_post_launch_command, run_pre_launch_command, PlayerAuth,
};
pub use download::{library_filename, DownloadManager};
pub use natives::{extract_natives, is_native_binary, verify_natives_dir};
pub use profile::{assemble_launch_profile, AssetIndex, LaunchProfile};
pub use resolve::DependencyResolver;

#[derive(Debug, Clone)]
pub struct Component {
    pub uid: String,
    pub version: String,
    pub is_locked: bool,
    pub dependencies: Vec<Requirement>,
    pub conflicts: Vec<String>,
    pub version_file: VersionFile,
}

#[derive(Debug, Clone)]
pub struct Requirement {
    pub uid: String,
    pub suggests: Option<String>,
    pub equals: Option<String>,
}

#[derive(Error, Debug)]
pub enum LaunchError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("Version not found: {0}")]
    VersionNotFound(String),
    #[error("Dependency conflict: {0}")]
    DependencyConflict(String),
    #[error("Launch error: {0}")]
    Launch(String),
    #[error("Java not found: {0}")]
    JavaNotFound(String),
    #[error("Net error: {0}")]
    Net(#[from] release_the_launcher_net::NetError),
}

#[derive(Debug, Clone)]
pub struct Library {
    pub name: String,
    pub url: Option<String>,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    pub is_native: bool,
    pub rules: Vec<Rule>,
    pub extract: Option<Extract>,
}

#[derive(Debug, Clone, Default)]
pub struct Rule {
    pub action: String,
    pub os: Option<RuleOs>,
    pub features: std::collections::HashMap<String, bool>,
}

#[derive(Debug, Clone, Default)]
pub struct RuleOs {
    pub name: Option<String>,
    pub arch: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Extract {
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ClientDownload {
    pub url: String,
    pub sha1: Option<String>,
    pub size: u64,
}

#[derive(Debug, Clone, Default)]
pub struct VersionFile {
    pub main_class: Option<String>,
    pub minecraft_args: Option<String>,
    pub jvm_args: Vec<String>,
    pub libraries: Vec<Library>,
    pub traits: Vec<String>,
    pub compatible_java_majors: Vec<u32>,
    pub jar_mods: Vec<String>,
    pub tweakers: Vec<String>,
    pub asset_index: Option<AssetIndex>,
    pub client_download: Option<ClientDownload>,
    pub version_type: Option<String>,
}

