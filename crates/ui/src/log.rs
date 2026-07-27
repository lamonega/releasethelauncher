use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const MAX_LOG_ENTRIES: usize = 1000;

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: LogLevel,
    pub message: String,
    pub target: String,
}

/// Censor sensitive values from a log message using a find/replace map.
///
/// This follows `PrismLauncher`'s approach: build a `Vec<(&str, &str)>` of
/// `(secret, replacement)` pairs and apply them all.
#[must_use]
pub fn censor_tokens(message: &str, filters: &[(&str, &str)]) -> String {
    let mut result = message.to_string();
    for &(pattern, replacement) in filters {
        result = result.replace(pattern, replacement);
    }
    // Also redact long hex/base64-looking strings that could be tokens
    let words: Vec<String> = result
        .split(' ')
        .map(|w| {
            let trimmed = w.trim_matches(|c: char| c == '"' || c == '\'' || c == ',');
            if trimmed.len() > 32
                && trimmed
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
            {
                "[REDACTED]".to_string()
            } else {
                w.to_string()
            }
        })
        .collect();
    words.join(" ")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    #[must_use]
    pub const fn as_str(&self) -> &str {
        match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }
}

#[derive(Clone)]
pub struct LogBuffer {
    entries: Arc<Mutex<VecDeque<LogEntry>>>,
    censor_filters: Arc<Mutex<Vec<(String, String)>>>,
    log_file_path: Arc<Mutex<Option<PathBuf>>>,
}

impl Default for LogBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl LogBuffer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Arc::new(Mutex::new(VecDeque::with_capacity(MAX_LOG_ENTRIES))),
            censor_filters: Arc::new(Mutex::new(Vec::new())),
            log_file_path: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_log_file_path(&self, path: PathBuf) {
        if let Ok(mut p) = self.log_file_path.lock() {
            *p = Some(path);
        }
    }

    /// Adds a censor filter: any occurrence of `secret` in log output is replaced with `replacement`.
    pub fn add_censor_filter(&self, secret: String, replacement: String) {
        if let Ok(mut filters) = self.censor_filters.lock() {
            filters.push((secret, replacement));
        }
    }

    /// Clears all censor filters (e.g., when account changes).
    pub fn clear_censor_filters(&self) {
        if let Ok(mut filters) = self.censor_filters.lock() {
            filters.clear();
        }
    }

    pub fn push(&self, entry: LogEntry) {
        let censored_msg = if let Ok(filters) = self.censor_filters.lock() {
            let refs: Vec<(&str, &str)> = filters
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            censor_tokens(&entry.message, &refs)
        } else {
            entry.message.clone()
        };
        let censored = LogEntry {
            timestamp: entry.timestamp,
            level: entry.level,
            message: censored_msg,
            target: entry.target,
        };

        // Print to standard output so running in terminal shows all logs live
        println!(
            "[{}] [{}/{}] {}",
            censored.timestamp,
            censored.level.as_str(),
            censored.target,
            censored.message
        );

        // Write to log file if path is set
        if let Ok(path_opt) = self.log_file_path.lock() {
            if let Some(ref path) = *path_opt {
                if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
                    let _ = writeln!(
                        file,
                        "[{}] [{}/{}] {}",
                        censored.timestamp,
                        censored.level.as_str(),
                        censored.target,
                        censored.message
                    );
                }
            }
        }

        let mut buffer = self
            .entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if buffer.len() >= MAX_LOG_ENTRIES {
            buffer.pop_front();
        }
        buffer.push_back(censored);
    }

    #[must_use]
    pub fn entries(&self) -> Vec<LogEntry> {
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .cloned()
            .collect()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
