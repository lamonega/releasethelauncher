pub mod resolve;
pub mod profile;
pub mod command;
pub mod natives;
pub mod assets;
pub mod download;

use thiserror::Error;

pub use resolve::DependencyResolver;
pub use profile::{LaunchProfile, AssetIndex, assemble_launch_profile};
pub use command::{build_command, launch_game};
pub use download::DownloadManager;

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

#[derive(Debug, Clone)]
pub struct VersionFile {
    pub main_class: Option<String>,
    pub minecraft_args: Option<String>,
    pub jvm_args: Vec<String>,
    pub libraries: Vec<Library>,
    pub traits: Vec<String>,
    pub compatible_java_majors: Vec<u32>,
    pub jar_mods: Vec<String>,
    pub tweakers: Vec<String>,
}

impl Default for VersionFile {
    fn default() -> Self {
        Self {
            main_class: None,
            minecraft_args: None,
            jvm_args: Vec::new(),
            libraries: Vec::new(),
            traits: Vec::new(),
            compatible_java_majors: vec![17, 21],
            jar_mods: Vec::new(),
            tweakers: Vec::new(),
        }
    }
}
