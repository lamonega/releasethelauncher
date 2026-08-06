//! Domain model and low-level utilities shared by the backend crates: the
//! instance model and filesystem layout ([`instance`]), persisted settings
//! ([`settings`]), archive extraction ([`archive`]), hashing ([`hash`]) and the
//! in-memory log buffer ([`log`]).
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::module_name_repetitions,
    clippy::struct_excessive_bools,
    clippy::too_many_lines,
    clippy::doc_markdown,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::similar_names
)]
pub mod archive;
pub mod hash;
pub mod instance;
pub mod log;
pub mod settings;

pub use archive::{extract_zip_with_filter, read_zip_entry_bytes, ArchiveError};
pub use hash::{compute_sha1_bytes, compute_sha1_file};
pub use instance::{CoreError, Instance, InstanceId, InstanceManager};
pub use log::{LogBuffer, LogEntry, LogLevel};
pub use settings::{GlobalSettings, InstanceSettings, JavaSettings, ModLoader};
