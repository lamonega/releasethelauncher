# Coordinator Interface (T2.0 contract)

This document is the **exact** contract for the T2.x migration tasks: the UI
crate (`crates/ui`) may only perform stateful/IO work through the `Coordinator`
facade listed below. The legacy getters (`instance_manager(_mut)`,
`account_list(_mut)`, `global_settings(_mut)`, `http_provider()`, `queue()`,
`log_buffer()`) are still present in `crates/coordinator/src/lib.rs` but are
scheduled for removal (getters in T2.10; the rest later). Do **not** add new
call sites to them.

Source of truth: `crates/coordinator/src/lib.rs` and `crates/coordinator/src/dto.rs`.

## DTOs

Defined in `crates/coordinator/src/dto.rs`, re-exported from the crate root
(`release_the_launcher_coordinator::{AccountSummary, InstanceSummary, InstalledModEntry}`).

```rust
/// Lightweight snapshot of an instance, safe to render without holding a
/// reference into the instance manager.
#[derive(Debug, Clone)]
pub struct InstanceSummary {
    pub id: String,
    pub name: String,
    pub mc_version: String,
    pub loader_name: String,
    pub root: PathBuf,
}

/// A single installed mod entry, including parsed metadata when available.
#[derive(Debug, Clone)]
pub struct InstalledModEntry {
    pub name: String,
    pub path: PathBuf,
    pub enabled: bool,
    pub details: Option<ModDetails>,
}

/// Lightweight snapshot of an account, safe to render without holding a
/// reference into the account list.
#[derive(Debug, Clone)]
pub struct AccountSummary {
    pub name: String,
    pub account_type: AccountType,
    pub auth_state: AuthState,
    pub skin_url: Option<String>,
    pub is_active: bool,
}
```

Type mappings (all fully qualified as they appear in the coordinator crate):

- `PathBuf` = `std::path::PathBuf`, `Path` = `std::path::Path`
- `GlobalSettings` = `release_the_launcher_core::settings::GlobalSettings`
- `ModLoader` = `release_the_launcher_core::settings::ModLoader`
- `AccountData` = `release_the_launcher_auth::AccountData`
- `AccountType` = `release_the_launcher_auth::AccountType`
- `AuthState` = `release_the_launcher_auth::AuthState`
- `ModDetails` = `release_the_launcher_mods::ModDetails`

## Reads

All reads return owned values (no borrows into `Coordinator` state).

```rust
/// Lightweight snapshot of the instance, or `None` if it does not exist.
#[must_use]
pub fn instance_summary(&self, id: &str) -> Option<InstanceSummary>;

/// Ids of all discovered instances.
#[must_use]
pub fn instance_ids(&self) -> Vec<String>;

/// Installed mods of an instance, with parsed metadata when available.
#[must_use]
pub fn list_instance_mods(&self, id: &str) -> Vec<InstalledModEntry>;

/// Mods directory for an instance, or `None` if it does not exist.
#[must_use]
pub fn instance_mods_dir(&self, id: &str) -> Option<PathBuf>;

/// Resolved `latest.log` path for an instance (`.minecraft/logs/latest.log`
/// preferred, falling back to `logs/latest.log` next to the root), or `None`
/// if neither exists.
#[must_use]
pub fn instance_log_path(&self, id: &str) -> Option<PathBuf>;

/// Snapshot of every account for rendering.
#[must_use]
pub fn accounts(&self) -> Vec<AccountSummary>;

/// Clone of the current global settings.
#[must_use]
pub fn settings(&self) -> GlobalSettings;
```

Semantics worth noting:

- `list_instance_mods` includes disabled mods and embeds the exact
  metadata-matching heuristic the mods view used to perform (match against
  enabled mods' parsed metadata by id/name, falling back to a direct parse of
  each file). It reuses the private `parse_enabled_mod_metadata` helper that
  `mods_metadata` and `request_mods_metadata` now share.
- Missing instance on any read returns `Vec::new()` / `None` (no error).

## Mutations

Each mutation persists to disk internally (calls the underlying `save()`).
UI-facing errors are `String` (the existing convention).

```rust
/// Deletes the instance with the given id, removing it from disk.
pub fn delete_instance(&mut self, id: &str) -> Result<(), String>;

/// Creates a new instance and returns its id (which equals `name`).
pub fn create_instance(
    &mut self,
    name: String,
    mc_version: String,
    loader: ModLoader,
    modpack_project_id: Option<String>,
    modpack_version_id: Option<String>,
) -> Result<String, String>;

/// Updates and persists an instance's Java path and memory settings.
pub fn update_instance_java_settings(
    &mut self,
    id: &str,
    java_path: Option<String>,
    memory_min: Option<String>,
    memory_max: Option<String>,
) -> Result<(), String>;

/// Enables or disables the mod at `mod_path` depending on its current state
/// (a `.jar.disabled` file is enabled, anything else is disabled).
pub fn toggle_mod(&mut self, id: &str, mod_path: &Path) -> Result<(), String>;

/// Adds an offline account and persists the account list.
pub fn add_offline_account(&mut self, username: &str) -> Result<(), String>;

/// Adds an account and persists the account list.
/// `account` is the unboxed `release_the_launcher_auth::AccountData`
/// (dereference the `Box` from `UiMessage::MsLoginSuccess` before calling).
pub fn add_account(&mut self, account: AccountData) -> Result<(), String>;

/// Marks the account at `index` as active and persists the account list.
pub fn set_active_account(&mut self, index: usize) -> Result<(), String>;

/// Removes the account at `index` and persists the account list.
pub fn remove_account(&mut self, index: usize) -> Result<(), String>;

/// Replaces the global settings in memory and persists them to disk.
pub fn update_settings(&mut self, settings: GlobalSettings) -> Result<(), String>;
```

Notes:

- `create_instance` builds `InstanceSettings` internally (name, MC version,
  loader, optional modpack ids) and delegates to `InstanceManager::create`,
  returning the new instance id. It serves both `new_instance/manual.rs`
  (`create(&state.name, settings)`) and `new_instance/mod.rs::handle_install_result`
  (`create(instance_id, settings)`, whose `instance_id` is the modpack name).
- `set_active_account` / `remove_account` return `Err` when `index` is out of
  bounds (the previous code silently no-op'd; no save happens on error).
- `toggle_mod` keeps `id` in the signature for contract stability; the
  operation is keyed off `mod_path` alone.

## Delegation (internal refactor, behavior unchanged)

- `mods_metadata` → delegates to the shared private helper
  `parse_enabled_mod_metadata(mods_dir: &Path)` (identical output to before).
- `request_mods_metadata` → runs the same helper inside its `spawn_blocking`.
- `check_mod_updates` → sources `mc_version`/`loader_name` via `instance_summary`
  and the mods dir via `instance_mods_dir`, so path logic lives in one place.

## Recorded decisions

### #2 — `GlobalSettings` mutation style

**Decision: whole-struct `update_settings(&mut self, settings: GlobalSettings) -> Result<(), String>`.**

`settings_view.rs` edits a local clone obtained from `settings()` and commits
once via `update_settings` on save. The coordinator replaces its in-memory copy
and persists via `global_settings.save(&self.settings_path)`.

### #3 — Log file IO

**Decision: keep the disk read + mtime/len caching in the view, behind a
coordinator-provided path.**

`Coordinator` only resolves the log path via `instance_log_path(id)`
(preferring `.minecraft/logs/latest.log`, falling back to `logs/latest.log`
next to the instance root). The view keeps its mtime/size caching and the
`fs::read_to_string` call; it must not build `.minecraft/logs` paths itself.
