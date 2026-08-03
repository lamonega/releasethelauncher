use sysinfo::System;

/// Check if the system has at least `required_mb` megabytes of available memory.
#[must_use]
pub fn has_enough_memory(required_mb: u64) -> bool {
    let sys = System::new_all();
    let available_bytes = sys.available_memory();
    let available_mb = available_bytes / (1024 * 1024);
    available_mb >= required_mb
}
