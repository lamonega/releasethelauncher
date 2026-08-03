use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub base_path: String,
    pub relative_path: String,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub md5: Option<String>,
    pub max_age: u64,
    #[serde(default, alias = "current_age")]
    pub last_accessed: u64,
    pub is_eternal: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheFile {
    entries: HashMap<String, HashMap<String, CacheEntry>>,
}

pub struct HttpMetaCache {
    entries: HashMap<String, HashMap<String, CacheEntry>>,
    file_path: PathBuf,
}

impl HttpMetaCache {
    #[must_use]
    pub fn load(path: &Path) -> Self {
        if path.exists() {
            if let Ok(content) = fs::read_to_string(path) {
                if let Ok(data) = serde_json::from_str::<CacheFile>(&content) {
                    return Self {
                        entries: data.entries,
                        file_path: path.to_path_buf(),
                    };
                }
            }
        }
        Self {
            entries: HashMap::new(),
            file_path: path.to_path_buf(),
        }
    }

    /// # Errors
    /// Returns an error if writing to the cache file fails.
    pub fn save(&self) -> std::io::Result<()> {
        let data = CacheFile {
            entries: self.entries.clone(),
        };
        let json = serde_json::to_string_pretty(&data)?;
        let tmp = self.file_path.with_extension("json.tmp");
        fs::write(&tmp, &json)?;
        fs::rename(&tmp, &self.file_path)?;
        Ok(())
    }

    /// Resolves a cache entry for `(base, path)`. Returns `Some(CacheEntry)` if eternal or not expired.
    ///
    /// # Panics
    /// Panics if the system time is before the Unix epoch.
    #[must_use]
    pub fn resolve(&mut self, base: &str, path: &str) -> Option<CacheEntry> {
        let entry = self.entries.get_mut(base)?.get_mut(path)?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if entry.is_eternal || (now.saturating_sub(entry.last_accessed) <= entry.max_age) {
            entry.last_accessed = now;
            Some(entry.clone())
        } else {
            None
        }
    }

    pub fn update(&mut self, mut entry: CacheEntry) {
        if entry.last_accessed == 0 {
            entry.last_accessed = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
        }
        self.entries
            .entry(entry.base_path.clone())
            .or_default()
            .insert(entry.relative_path.clone(), entry);
    }

    pub fn remove(&mut self, base: &str, path: &str) {
        if let Some(base_map) = self.entries.get_mut(base) {
            base_map.remove(path);
            if base_map.is_empty() {
                self.entries.remove(base);
            }
        }
    }

    #[must_use]
    pub fn entry(&self, base: &str, path: &str) -> Option<&CacheEntry> {
        self.entries.get(base)?.get(path)
    }
}
