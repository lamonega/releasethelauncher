//! Game launch pipeline: version resolution and dependency graphs
//! ([`resolve`]), asset/native/library downloads ([`download`], [`assets`],
//! [`natives`]), Java discovery ([`java`]) and command assembly ([`command`]).
pub mod assets;
pub mod command;
pub mod download;
pub mod fml;
pub mod java;
pub mod memory;
pub mod natives;
pub(crate) mod platform;
pub mod profile;
pub mod resolve;

use thiserror::Error;

pub use command::{build_command, run_post_launch_command, run_pre_launch_command, PlayerAuth};
pub use download::DownloadManager;
pub use fml::ensure_fml_deobfuscation_data;
pub use natives::{extract_natives, verify_natives_dir};
pub use profile::{assemble_launch_profile, AssetIndex, LaunchProfile};
pub use resolve::DependencyResolver;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MavenCoord {
    pub group: String,
    pub artifact: String,
    pub version: String,
    pub classifier: Option<String>,
    pub ext: String,
}

impl MavenCoord {
    #[must_use]
    pub(crate) fn parse(name: &str) -> Option<Self> {
        let (base, ext) = if let Some((b, e)) = name.split_once('@') {
            (b, e.to_string())
        } else {
            (name, "jar".to_string())
        };

        let parts: Vec<&str> = base.split(':').collect();
        if parts.len() < 3 {
            return None;
        }

        let group = parts[0].to_string();
        let artifact = parts[1].to_string();
        let version = parts[2].to_string();
        let classifier = if parts.len() > 3 {
            Some(parts[3..].join(":"))
        } else {
            None
        };

        Some(Self {
            group,
            artifact,
            version,
            classifier,
            ext,
        })
    }

    #[must_use]
    pub(crate) fn filename(&self) -> String {
        match &self.classifier {
            Some(cls) => format!("{}-{}-{}.{}", self.artifact, self.version, cls, self.ext),
            None => format!("{}-{}.{}", self.artifact, self.version, self.ext),
        }
    }

    #[must_use]
    pub(crate) fn is_zip_artifact(&self) -> bool {
        self.ext.eq_ignore_ascii_case("zip")
    }
}

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
    #[error("Dependency conflict: component {component} conflicts with {conflict}")]
    DependencyConflict { component: String, conflict: String },
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rule {
    pub action: String,
    pub os: Option<RuleOs>,
    pub features: std::collections::HashMap<String, bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
