use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::AccountData;

#[derive(Debug, Serialize, Deserialize)]
pub struct AccountListFile {
    pub format_version: u32,
    pub active_index: Option<usize>,
    pub accounts: Vec<AccountData>,
}

pub struct AccountList {
    pub accounts: Vec<AccountData>,
    pub active_index: Option<usize>,
    file_path: PathBuf,
}

impl AccountList {
    #[must_use]
    pub fn load(path: &Path) -> Self {
        if path.exists() {
            if let Ok(content) = fs::read_to_string(path) {
                if let Ok(data) = serde_json::from_str::<AccountListFile>(&content) {
                    return Self {
                        accounts: data.accounts,
                        active_index: data.active_index,
                        file_path: path.to_path_buf(),
                    };
                }
            }
        }
        Self {
            accounts: Vec::new(),
            active_index: None,
            file_path: path.to_path_buf(),
        }
    }

    /// # Errors
    ///
    /// Returns an error if writing the account list file to disk fails.
    pub fn save(&self) -> std::io::Result<()> {
        let data = AccountListFile {
            format_version: 1,
            active_index: self.active_index,
            accounts: self.accounts.clone(),
        };
        let json = serde_json::to_string_pretty(&data)?;
        let tmp = self.file_path.with_extension("json.tmp");
        fs::write(&tmp, &json)?;
        fs::rename(&tmp, &self.file_path)?;
        Ok(())
    }

    pub fn add(&mut self, account: AccountData) {
        self.accounts.push(account);
        if self.accounts.len() == 1 {
            self.active_index = Some(0);
        }
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.accounts.len() {
            self.accounts.remove(index);
            match self.active_index {
                Some(ai) if ai == index => {
                    self.active_index = if self.accounts.is_empty() {
                        None
                    } else {
                        Some(0)
                    };
                }
                Some(ai) if ai > index => {
                    self.active_index = Some(ai - 1);
                }
                _ => {}
            }
        }
    }

    #[must_use]
    pub fn active(&self) -> Option<&AccountData> {
        self.active_index.and_then(|i| self.accounts.get(i))
    }

    pub fn set_active(&mut self, index: usize) -> bool {
        if index < self.accounts.len() {
            self.active_index = Some(index);
            true
        } else {
            false
        }
    }
}
