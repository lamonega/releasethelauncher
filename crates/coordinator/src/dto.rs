use std::path::PathBuf;

use release_the_launcher_auth::AccountType;
use release_the_launcher_core::JavaSettings;
use release_the_launcher_mods::ModDetails;

/// Lightweight snapshot of an instance, safe to render without holding a
/// reference into the instance manager.
#[derive(Debug, Clone)]
pub struct InstanceSummary {
    pub id: String,
    pub name: String,
    pub mc_version: String,
    pub loader_name: String,
    pub root: PathBuf,
    pub java: JavaSettings,
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
    pub skin_url: Option<String>,
    pub is_active: bool,
}
