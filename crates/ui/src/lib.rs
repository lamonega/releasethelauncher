pub mod views;

use release_the_launcher_auth::AccountList;
use release_the_launcher_core::InstanceManager;

pub struct App {
    pub instance_manager: InstanceManager,
    pub account_list: AccountList,
    pub current_view: View,
    pub download_progress: Option<(usize, usize)>,
    pub status_message: String,
}

#[derive(Debug, Clone)]
pub enum View {
    InstanceList,
    InstanceDetail { id: String },
    AccountList,
    AccountLogin,
    NewInstance,
    ModBrowser { instance_id: String },
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    /// # Panics
    ///
    /// Panics if the `/dev/null` fallback path cannot be discovered.
    #[must_use]
    pub fn new() -> Self {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".config"))
            .join("release-the-launcher");

        let instances_dir = config_dir.join("instances");
        let accounts_path = config_dir.join("accounts.json");

        std::fs::create_dir_all(&config_dir).ok();
        std::fs::create_dir_all(&instances_dir).ok();

        let instance_manager = InstanceManager::discover(instances_dir).unwrap_or_else(|e| {
            eprintln!("Failed to discover instances: {e}");
            InstanceManager::discover(std::path::PathBuf::from("/dev/null")).unwrap()
        });

        let account_list = AccountList::load(&accounts_path);

        Self {
            instance_manager,
            account_list,
            current_view: View::InstanceList,
            download_progress: None,
            status_message: String::new(),
        }
    }
}
