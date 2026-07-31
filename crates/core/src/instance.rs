use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::settings::InstanceSettings;

pub type InstanceId = String;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("TOML serialize error: {0}")]
    TomlSer(#[from] toml::ser::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Instance '{0}' not found")]
    InstanceNotFound(String),
    #[error("Instance '{0}' already exists")]
    InstanceAlreadyExists(String),
}

pub struct Instance {
    pub id: InstanceId,
    pub root: PathBuf,
    pub settings: InstanceSettings,
}

pub struct InstanceManager {
    instances_dir: PathBuf,
    instances: HashMap<InstanceId, Instance>,
}

impl InstanceManager {
    #[must_use]
    pub fn instances_dir(&self) -> &Path {
        &self.instances_dir
    }

    #[must_use]
    pub fn new(instances_dir: PathBuf) -> Self {
        Self {
            instances_dir,
            instances: HashMap::new(),
        }
    }

    /// Discovers all valid instances in the given directory.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Io`] if reading the directory or instance config fails,
    /// or [`CoreError::Toml`] if parsing an instance config fails.
    pub fn discover(instances_dir: PathBuf) -> Result<Self, CoreError> {
        let mut instances = HashMap::new();
        if instances_dir.exists() {
            for entry in fs::read_dir(&instances_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    let config_path = path.join("instance.toml");
                    if config_path.exists() {
                        let settings = InstanceSettings::load(&config_path)?;
                        let id = path.file_name().map_or_else(
                            || "unknown".to_string(),
                            |n| n.to_string_lossy().to_string(),
                        );
                        instances.insert(
                            id.clone(),
                            Instance {
                                id,
                                root: path,
                                settings,
                            },
                        );
                    }
                }
            }
        }
        Ok(Self {
            instances_dir,
            instances,
        })
    }

    /// Creates a new instance with the given name and settings.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InstanceAlreadyExists`] if an instance with the same name already exists,
    /// or [`CoreError::Io`] if creating directories or writing the config fails.
    pub fn create(
        &mut self,
        name: &str,
        settings: InstanceSettings,
    ) -> Result<&Instance, CoreError> {
        let id = name.to_string();
        if self.instances.contains_key(&id) {
            return Err(CoreError::InstanceAlreadyExists(id));
        }

        let instance_dir = self.instances_dir.join(&id);
        let minecraft_dir = instance_dir.join(".minecraft");
        let mods_dir = minecraft_dir.join("mods");
        let config_dir = minecraft_dir.join("config");
        let saves_dir = minecraft_dir.join("saves");
        let resourcepacks_dir = minecraft_dir.join("resourcepacks");
        let index_dir = instance_dir.join(".index");
        let server_resource_packs_dir = minecraft_dir.join("server-resource-packs");

        fs::create_dir_all(&mods_dir)?;
        fs::create_dir_all(&config_dir)?;
        fs::create_dir_all(&saves_dir)?;
        fs::create_dir_all(&resourcepacks_dir)?;
        fs::create_dir_all(&index_dir)?;
        fs::create_dir_all(&server_resource_packs_dir)?;

        let config_path = instance_dir.join("instance.toml");
        settings.save(&config_path)?;

        let instance = Instance {
            id: id.clone(),
            root: instance_dir,
            settings,
        };
        self.instances.insert(id.clone(), instance);
        self.instances
            .get(&id)
            .ok_or(CoreError::InstanceNotFound(id))
    }

    /// Deletes the instance with the given ID, removing it from disk.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InstanceNotFound`] if no instance with the given ID exists,
    /// or [`CoreError::Io`] if removing the instance directory fails.
    pub fn delete(&mut self, id: &InstanceId) -> Result<(), CoreError> {
        let instance = self
            .instances
            .get(id)
            .ok_or_else(|| CoreError::InstanceNotFound(id.clone()))?;
        fs::remove_dir_all(&instance.root)?;
        self.instances.remove(id);
        Ok(())
    }

    #[must_use = "Returns the list of all discovered instances"]
    pub fn list(&self) -> Vec<&Instance> {
        self.instances.values().collect()
    }

    #[must_use = "Returns the instance with the given ID, or None if not found"]
    pub fn get(&self, id: &InstanceId) -> Option<&Instance> {
        self.instances.get(id)
    }

    #[must_use = "Returns the mods directory path for the given instance, or None if not found"]
    pub fn get_mods_dir(&self, id: &InstanceId) -> Option<PathBuf> {
        self.instances
            .get(id)
            .map(|i| i.root.join(".minecraft").join("mods"))
    }

    #[must_use = "Returns the index directory path for the given instance, or None if not found"]
    pub fn get_index_dir(&self, id: &InstanceId) -> Option<PathBuf> {
        self.instances.get(id).map(|i| i.root.join(".index"))
    }

    /// Updates and saves Java settings for a specific instance.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InstanceNotFound`] if the instance is not found,
    /// or [`CoreError::Io`] if saving `instance.toml` fails.
    pub fn update_instance_java_settings(
        &mut self,
        id: &str,
        path: Option<String>,
        memory_min: Option<String>,
        memory_max: Option<String>,
    ) -> Result<(), CoreError> {
        let instance = self
            .instances
            .get_mut(id)
            .ok_or_else(|| CoreError::InstanceNotFound(id.to_string()))?;
        instance.settings.java.path = path;
        instance.settings.java.memory_min = memory_min;
        instance.settings.java.memory_max = memory_max;
        let config_path = instance.root.join("instance.toml");
        instance.settings.save(&config_path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ModLoader;

    #[test]
    fn test_instance_manager_create_and_delete() {
        let temp_dir = std::env::temp_dir().join(format!("rtl_test_inst_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        let mut manager = InstanceManager::new(temp_dir.clone());

        let settings = InstanceSettings::new(
            "test-instance".to_string(),
            "1.20.1".to_string(),
            ModLoader::Vanilla,
        );
        let inst = manager.create("test-instance", settings).unwrap();
        assert_eq!(inst.id, "test-instance");
        assert!(inst.root.exists());

        assert!(manager.get(&"test-instance".to_string()).is_some());
        assert_eq!(manager.list().len(), 1);

        manager.delete(&"test-instance".to_string()).unwrap();
        assert!(manager.get(&"test-instance".to_string()).is_none());
        assert_eq!(manager.list().len(), 0);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
