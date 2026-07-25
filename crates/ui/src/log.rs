use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

const MAX_LOG_ENTRIES: usize = 1000;

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: LogLevel,
    pub message: String,
    pub target: String,
}

/// Censors sensitive tokens from a log message.
#[must_use]
pub fn censor_tokens(message: &str) -> String {
    let mut result = message.to_string();

    // Patterns to censor (case-insensitive matching on keys, redact values)
    let patterns = &[
        "\"accessToken\"",
        "\"token\"",
        "\"access_token\"",
        "\"refresh_token\"",
        "\"Authorization\"",
        "\"XBL-STS\"",
        "\"XblToken\"",
    ];

    for pattern in patterns {
        if let Some(idx) = result.find(pattern) {
            let after_pattern = idx + pattern.len();
            if after_pattern < result.len() {
                // Find the value after the colon/separator
                let rest = &result[after_pattern..];
                if let Some(colon_pos) = rest.find(':') {
                    let value_start = after_pattern + colon_pos + 1;
                    let rest_after_colon = &result[value_start..];
                    // Find the value (skip whitespace, find end of quoted string or word)
                    let trimmed_start = rest_after_colon
                        .find(|c: char| c != ' ' && c != '\t' && c != ':')
                        .map_or(value_start, |p| value_start + p);
                    let rest_trimmed = &result[trimmed_start..];
                    let value_end = rest_trimmed.strip_prefix('"').map_or_else(
                        || {
                            rest_trimmed
                                .find([',', '}', ' ', '\n'])
                                .map_or(result.len(), |p| trimmed_start + p)
                        },
                        |stripped| {
                            stripped
                                .find('"')
                                .map_or(rest_trimmed.len(), |p| trimmed_start + 1 + p + 1)
                        },
                    );
                    let censor_len = value_end.saturating_sub(trimmed_start);
                    if censor_len > 0 {
                        result.replace_range(trimmed_start..value_end, "[REDACTED]");
                    }
                }
            }
        }
    }

    // Also censor long hex/base64-looking strings that could be tokens
    let words: Vec<&str> = result.split(' ').collect();
    let mut censored_words: Vec<String> = Vec::new();
    for word in &words {
        let w = word.trim_matches(|c: char| c == '"' || c == '\'' || c == ',');
        if w.len() > 32 && w.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.') {
            censored_words.push("[REDACTED]".to_string());
        } else {
            censored_words.push(word.to_string());
        }
    }
    censored_words.join(" ")
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
        }
    }

    pub fn push(&self, entry: LogEntry) {
        let censored = LogEntry {
            timestamp: entry.timestamp,
            level: entry.level,
            message: censor_tokens(&entry.message),
            target: entry.target,
        };
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
