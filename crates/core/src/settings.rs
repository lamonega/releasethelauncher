use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModLoader {
    Vanilla,
    Fabric { loader_version: String },
    Quilt { loader_version: String },
    Forge { loader_version: String },
    NeoForge { loader_version: String },
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
    #[serde(default = "default_close_after_launch")]
    pub close_after_launch: bool,
}

fn default_close_after_launch() -> bool {
    false
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JavaSettings {
    pub path: Option<String>,
    pub memory_min: Option<String>,
    pub memory_max: Option<String>,
}

impl Default for JavaSettings {
    fn default() -> Self {
        Self {
            path: None,
            memory_min: Some("1G".to_string()),
            memory_max: Some("2G".to_string()),
        }
    }
}

impl InstanceSettings {
    #[must_use = "Creates a new InstanceSettings with default Java settings"]
    pub fn new(name: String, minecraft_version: String, loader: ModLoader) -> Self {
        Self {
            format_version: 1,
            name,
            minecraft_version,
            last_launch_time: None,
            loader,
            java: JavaSettings::default(),
            pre_launch_command: String::new(),
            post_launch_command: String::new(),
            close_after_launch: false,
        }
    }

    /// Loads instance settings from a TOML file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or the content cannot be parsed as TOML.
    pub fn load(path: &Path) -> Result<Self, toml::de::Error> {
        let content = fs::read_to_string(path).unwrap_or_default();
        toml::from_str(&content)
    }

    /// Saves instance settings to a TOML file.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization to TOML fails or the file cannot be written.
    pub fn save(&self, path: &Path) -> Result<(), std::io::Error> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        fs::write(path, content)
    }

    #[must_use = "Returns the name of the mod loader as a string"]
    pub const fn loader_name(&self) -> &str {
        match &self.loader {
            ModLoader::Vanilla => "Vanilla",
            ModLoader::Fabric { .. } => "Fabric",
            ModLoader::Quilt { .. } => "Quilt",
            ModLoader::Forge { .. } => "Forge",
            ModLoader::NeoForge { .. } => "NeoForge",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSettings {
    pub format_version: u32,
    pub java_path: Option<String>,
    pub memory_min: Option<String>,
    pub memory_max: Option<String>,
    pub close_after_launch: bool,
    pub pre_launch_command: String,
    pub post_launch_command: String,
}

impl Default for GlobalSettings {
    fn default() -> Self {
        Self {
            format_version: 1,
            java_path: None,
            memory_min: Some("1G".to_string()),
            memory_max: Some("2G".to_string()),
            close_after_launch: false,
            pre_launch_command: String::new(),
            post_launch_command: String::new(),
        }
    }
}

impl GlobalSettings {
    pub fn load(path: &Path) -> Self {
        let content = fs::read_to_string(path).unwrap_or_default();
        toml::from_str(&content).unwrap_or_default()
    }

    /// # Errors
    pub fn save(&self, path: &Path) -> Result<(), std::io::Error> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)
    }

    /// Merge instance settings with global defaults.
    /// Instance-level values override global when `Some`.
    #[must_use]
    pub fn java_path_for(&self, instance_java_path: Option<&str>) -> Option<String> {
        instance_java_path
            .filter(|s| !s.is_empty())
            .map(String::from)
            .or_else(|| self.java_path.clone())
    }

    #[must_use]
    pub fn memory_min_for(&self, instance_memory_min: &Option<String>) -> String {
        instance_memory_min
            .clone()
            .or_else(|| self.memory_min.clone())
            .unwrap_or_else(|| "1G".to_string())
    }

    #[must_use]
    pub fn memory_max_for(&self, instance_memory_max: &Option<String>) -> String {
        instance_memory_max
            .clone()
            .or_else(|| self.memory_max.clone())
            .unwrap_or_else(|| "2G".to_string())
    }
}
