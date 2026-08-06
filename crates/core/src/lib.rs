//! Domain model and low-level utilities shared by the backend crates: the
//! instance model and filesystem layout ([`instance`]), persisted settings
//! ([`settings`]), archive extraction ([`archive`]), hashing ([`hash`]) and the
//! in-memory log buffer ([`log`]).

pub mod archive;
pub mod error;
pub mod hash;
pub mod instance;
pub mod log;
pub mod settings;

pub use archive::{extract_zip_with_filter, read_zip_entry_bytes, ArchiveError};
pub use error::CoreError;
pub use hash::{compute_sha1_bytes, compute_sha1_file};
pub use instance::{Instance, InstanceId, InstanceManager};
pub use log::{LogBuffer, LogEntry, LogLevel};
pub use settings::{GlobalSettings, InstanceSettings, JavaSettings, ModLoader};
