use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use thiserror::Error;

use crate::settings::{InstanceSettings, JavaSettings, SettingsError};
use release_the_launcher_constants::paths;

pub type InstanceId = String;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Instance '{0}' not found")]
    InstanceNotFound(String),
    #[error("Instance '{0}' already exists")]
    InstanceAlreadyExists(String),
    #[error("Settings error: {0}")]
    Settings(#[from] SettingsError),
}

pub struct Instance {
    pub id: InstanceId,
    pub root: PathBuf,
    pub settings: InstanceSettings,
}

impl Instance {
    #[must_use]
    pub fn minecraft_dir(&self) -> std::path::PathBuf {
        self.root
            .join(release_the_launcher_constants::paths::MINECRAFT_DIR)
    }

    #[must_use]
    pub fn mods_dir(&self) -> std::path::PathBuf {
        self.minecraft_dir()
            .join(release_the_launcher_constants::paths::MODS_DIR)
    }

    #[must_use]
    pub(crate) fn config_path(&self) -> std::path::PathBuf {
        self.root
            .join(release_the_launcher_constants::paths::INSTANCE_CONFIG_FILE_NAME)
    }
}

pub struct InstanceManager {
    instances_dir: PathBuf,
    instances: HashMap<InstanceId, Instance>,
}

impl InstanceManager {
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
    /// Returns [`CoreError::Io`] if reading the directory fails.
    pub fn discover(instances_dir: PathBuf) -> Result<Self, CoreError> {
        let mut instances = HashMap::new();
        if instances_dir.exists() {
            for entry in fs::read_dir(&instances_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    let config_path = path.join(paths::INSTANCE_CONFIG_FILE_NAME);
                    if config_path.exists() {
                        let settings = InstanceSettings::load(&config_path).unwrap_or_default();
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
        let minecraft_dir = instance_dir.join(paths::MINECRAFT_DIR);
        for sub in [
            paths::MODS_DIR,
            paths::CONFIG_DIR,
            paths::SAVES_DIR,
            paths::RESOURCE_PACKS_DIR,
            paths::SERVER_RESOURCE_PACKS_DIR,
        ] {
            fs::create_dir_all(minecraft_dir.join(sub))?;
        }
        fs::create_dir_all(instance_dir.join(paths::INDEX_DIR))?;

        let instance = Instance {
            id: id.clone(),
            root: instance_dir,
            settings,
        };
        let config_path = instance.config_path();
        instance.settings.save(&config_path)?;

        Ok(self.instances.entry(id).or_insert(instance))
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
        self.instances.get(id).map(Instance::mods_dir)
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
        java: &JavaSettings,
    ) -> Result<(), CoreError> {
        let instance = self
            .instances
            .get_mut(id)
            .ok_or_else(|| CoreError::InstanceNotFound(id.to_string()))?;
        instance.settings.java = java.clone();
        let config_path = instance.config_path();
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
