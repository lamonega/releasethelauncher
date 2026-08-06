use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML parse error: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("Instance '{0}' not found")]
    InstanceNotFound(String),
    #[error("Instance '{0}' already exists")]
    InstanceAlreadyExists(String),
}
