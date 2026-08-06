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
    pub data: Option<String>,
    pub max_age: u64,
    #[serde(default, alias = "current_age")]
    pub last_accessed: u64,
    pub is_eternal: bool,
}

fn cache_key(base: &str, path: &str) -> String {
    format!("{base}:{path}")
}

pub struct HttpMetaCache {
    entries: HashMap<String, CacheEntry>,
    file_path: PathBuf,
}

impl HttpMetaCache {
    #[must_use]
    pub fn load(path: &Path) -> Self {
        if path.exists() {
            if let Ok(content) = fs::read_to_string(path) {
                if let Ok(entries) = serde_json::from_str(&content) {
                    return Self {
                        entries,
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
        let json = serde_json::to_string_pretty(&self.entries)?;
        let tmp = self.file_path.with_extension("json.tmp");
        fs::write(&tmp, json)?;
        fs::rename(&tmp, &self.file_path)?;
        Ok(())
    }

    /// Resolves a cache entry for `(base, path)`. Returns `Some(CacheEntry)` if eternal or not expired.
    #[must_use]
    pub fn resolve(&self, base: &str, path: &str) -> Option<CacheEntry> {
        let entry = self.entries.get(&cache_key(base, path))?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if entry.is_eternal || (now.saturating_sub(entry.last_accessed) <= entry.max_age) {
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
        let key = cache_key(&entry.base_path, &entry.relative_path);
        self.entries.insert(key, entry);
    }

    pub fn remove(&mut self, base: &str, path: &str) {
        self.entries.remove(&cache_key(base, path));
    }

    #[must_use]
    pub fn entry(&self, base: &str, path: &str) -> Option<&CacheEntry> {
        self.entries.get(&cache_key(base, path))
    }
}
