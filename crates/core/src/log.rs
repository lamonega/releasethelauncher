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
            log_file_path: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_log_file_path(&self, path: PathBuf) {
        if let Ok(mut p) = self.log_file_path.lock() {
            *p = Some(path);
        }
    }

    pub fn push(&self, entry: LogEntry) {
        // Print to standard output so running in terminal shows all logs live
        println!(
            "[{}] [{}/{}] {}",
            entry.timestamp,
            entry.level.as_str(),
            entry.target,
            entry.message
        );

        // Write to log file if path is set
        if let Ok(path_opt) = self.log_file_path.lock() {
            if let Some(ref path) = *path_opt {
                if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
                    let _ = writeln!(
                        file,
                        "[{}] [{}/{}] {}",
                        entry.timestamp,
                        entry.level.as_str(),
                        entry.target,
                        entry.message
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
        buffer.push_back(entry);
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
