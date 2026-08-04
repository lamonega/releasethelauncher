use release_the_launcher_constants::defaults;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::io;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("IO error reading settings: {0}")]
    Io(#[from] io::Error),
    #[error("TOML parse error: {0}")]
    Parse(#[from] toml::de::Error),
}

fn load_toml<T: DeserializeOwned + Default>(path: &Path) -> Result<T, SettingsError> {
    if !path.exists() {
        return Ok(T::default());
    }
    let content = fs::read_to_string(path)?;
    Ok(toml::from_str(&content)?)
}

fn save_toml<T: Serialize>(value: &T, path: &Path) -> std::io::Result<()> {
    let content = toml::to_string_pretty(value)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum ModLoader {
    #[default]
    Vanilla,
    Fabric {
        loader_version: String,
    },
    Quilt {
        loader_version: String,
    },
    Forge {
        loader_version: String,
    },
    NeoForge {
        loader_version: String,
    },
}

impl ModLoader {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Vanilla => "Vanilla",
            Self::Fabric { .. } => "Fabric",
            Self::Quilt { .. } => "Quilt",
            Self::Forge { .. } => "Forge",
            Self::NeoForge { .. } => "NeoForge",
        }
    }
}

impl fmt::Display for ModLoader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vanilla => write!(f, "Vanilla"),
            Self::Fabric { loader_version }
            | Self::Quilt { loader_version }
            | Self::Forge { loader_version }
            | Self::NeoForge { loader_version } => write!(f, "{} {loader_version}", self.name()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceSettings {
    pub format_version: u32,
    pub name: String,
    pub minecraft_version: String,
    pub last_launch_time: Option<u64>,
    pub loader: ModLoader,
    pub java: JavaSettings,
    #[serde(default)]
    pub pre_launch_command: String,
    #[serde(default)]
    pub post_launch_command: String,
    #[serde(default)]
    pub close_after_launch: bool,
    #[serde(default)]
    pub modpack_project_id: Option<String>,
    #[serde(default)]
    pub modpack_version_id: Option<String>,
}

impl Default for InstanceSettings {
    fn default() -> Self {
        Self {
            format_version: defaults::SETTINGS_FORMAT_VERSION,
            name: String::new(),
            minecraft_version: String::new(),
            last_launch_time: None,
            loader: ModLoader::Vanilla,
            java: JavaSettings::default(),
            pre_launch_command: String::new(),
            post_launch_command: String::new(),
            close_after_launch: false,
            modpack_project_id: None,
            modpack_version_id: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JavaSettings {
    pub path: Option<String>,
    pub memory_min: Option<String>,
    pub memory_max: Option<String>,
}

impl InstanceSettings {
    #[must_use = "Creates a new InstanceSettings with default Java settings"]
    pub fn new(name: String, minecraft_version: String, loader: ModLoader) -> Self {
        Self {
            name,
            minecraft_version,
            loader,
            ..Default::default()
        }
    }

    pub fn load(path: &Path) -> Result<Self, SettingsError> {
        load_toml(path)
    }

    /// Saves instance settings to a TOML file.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization to TOML fails or the file cannot be written.
    pub fn save(&self, path: &Path) -> Result<(), std::io::Error> {
        save_toml(self, path)
    }

    #[must_use = "Returns the name of the mod loader as a string"]
    pub const fn loader_name(&self) -> &str {
        self.loader.name()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSettings {
    pub format_version: u32,
    #[serde(default)]
    pub java: JavaSettings,
    pub close_after_launch: bool,
    #[serde(default)]
    pub pre_launch_command: String,
    #[serde(default)]
    pub post_launch_command: String,
}

impl Default for GlobalSettings {
    fn default() -> Self {
        Self {
            format_version: defaults::SETTINGS_FORMAT_VERSION,
            java: JavaSettings {
                path: None,
                memory_min: Some(defaults::DEFAULT_MEMORY_MIN.to_string()),
                memory_max: Some(defaults::DEFAULT_MEMORY_MAX.to_string()),
            },
            close_after_launch: false,
            pre_launch_command: String::new(),
            post_launch_command: String::new(),
        }
    }
}

impl GlobalSettings {
    pub fn load(path: &Path) -> Result<Self, SettingsError> {
        load_toml(path)
    }

    /// # Errors
    pub fn save(&self, path: &Path) -> Result<(), std::io::Error> {
        save_toml(self, path)
    }

    /// Merge instance settings with global defaults.
    /// Instance-level values override global when `Some`.
    #[must_use]
    pub fn java_path_for(&self, instance_java_path: Option<&str>) -> Option<String> {
        instance_java_path
            .filter(|s| !s.is_empty())
            .or(self.java.path.as_deref())
            .map(String::from)
    }

    #[must_use]
    pub fn memory_min_for(&self, instance_memory_min: Option<&str>) -> String {
        instance_memory_min
            .or(self.java.memory_min.as_deref())
            .unwrap_or(release_the_launcher_constants::defaults::DEFAULT_MEMORY_MIN)
            .to_string()
    }

    #[must_use]
    pub fn memory_max_for(&self, instance_memory_max: Option<&str>) -> String {
        instance_memory_max
            .or(self.java.memory_max.as_deref())
            .unwrap_or(release_the_launcher_constants::defaults::DEFAULT_MEMORY_MAX)
            .to_string()
    }
}
