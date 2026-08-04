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

pub struct LogBufferState {
    pub entries: VecDeque<LogEntry>,
    pub log_file: Option<File>,
}

#[derive(Clone)]
pub struct LogBuffer {
    inner: Arc<Mutex<LogBufferState>>,
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
            inner: Arc::new(Mutex::new(LogBufferState {
                entries: VecDeque::with_capacity(MAX_LOG_ENTRIES),
                log_file: None,
            })),
        }
    }

    pub fn set_log_file_path(&self, path: PathBuf) {
        let file = OpenOptions::new().create(true).append(true).open(path).ok();
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.log_file = file;
    }

    pub fn push(&self, entry: LogEntry) {
        println!(
            "[{}] [{}/{}] {}",
            entry.timestamp,
            entry.level.as_str(),
            entry.target,
            entry.message
        );

        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if let Some(ref mut file) = inner.log_file {
            let _ = writeln!(
                file,
                "[{}] [{}/{}] {}",
                entry.timestamp,
                entry.level.as_str(),
                entry.target,
                entry.message
            );
        }

        if inner.entries.len() >= MAX_LOG_ENTRIES {
            inner.entries.pop_front();
        }
        inner.entries.push_back(entry);
    }

    #[must_use]
    pub fn entries(&self) -> Vec<LogEntry> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entries
            .iter()
            .cloned()
            .collect()
    }
}
