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
    clippy::unnecessary_lazy_evaluations
)]

pub mod archive;
pub mod instance;
pub mod settings;

pub use archive::{extract_zip_to_dir, ArchiveError};
pub use instance::{CoreError, Instance, InstanceId, InstanceManager};
pub use settings::{GlobalSettings, InstanceSettings, JavaSettings, ModLoader};
