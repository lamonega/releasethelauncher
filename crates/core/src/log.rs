use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
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
    log_file: Arc<Mutex<Option<File>>>,
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
            log_file: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_log_file_path(&self, path: PathBuf) {
        let file = OpenOptions::new().create(true).append(true).open(path).ok();
        let mut guard = self
            .log_file
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard = file;
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

        // Write to log file if open
        {
            let mut file_guard = self
                .log_file
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(ref mut file) = *file_guard {
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
}
