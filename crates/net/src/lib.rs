pub mod cache;
pub mod validator;

pub use cache::{CacheEntry, HttpMetaCache};
pub use validator::{ChecksumValidator, Sha1Validator, Sha256Validator};

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use thiserror::Error;
use tokio::task::JoinHandle;
use url::Url;

#[derive(Error, Debug)]
pub enum NetError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },
    #[error("Download failed after {0} retries")]
    MaxRetries(u32),
    #[error("Rate limited, retry after {0}s")]
    RateLimited(u64),
    #[error("Cache error: {0}")]
    Cache(String),
}

pub enum Sink {
    File { path: PathBuf, atomic: bool },
    Bytes(Arc<Mutex<Vec<u8>>>),
}

pub struct DownloadTask {
    pub url: Url,
    pub sink: Sink,
    pub validator: Option<Box<dyn ChecksumValidator>>,
    pub cache_entry: Option<CacheEntry>,
    pub headers: HashMap<String, String>,
    pub expected_hash: Option<String>,
    pub hash_algorithm: Option<String>,
}

pub struct DownloadJob {
    downloads: VecDeque<DownloadTask>,
    doing: Vec<JoinHandle<Result<PathBuf, NetError>>>,
    max_concurrent: usize,
    /// Used by future retry implementation.
    #[allow(dead_code)]
    retries: u32,
    /// Used by future retry implementation.
    #[allow(dead_code)]
    max_retries: u32,
    completed: usize,
    total: usize,
}

impl DownloadJob {
    #[must_use]
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            downloads: VecDeque::new(),
            doing: Vec::new(),
            max_concurrent,
            retries: 0,
            max_retries: 3,
            completed: 0,
            total: 0,
        }
    }

    pub fn add(&mut self, task: DownloadTask) {
        self.total += 1;
        self.downloads.push_back(task);
    }

    #[must_use]
    pub fn progress(&self) -> (usize, usize) {
        (self.completed, self.total)
    }

    /// # Errors
    /// Returns `NetError` if any download or join fails.
    pub async fn execute(&mut self, client: &reqwest::Client) -> Result<(), NetError> {
        while !self.downloads.is_empty() || !self.doing.is_empty() {
            while self.doing.len() < self.max_concurrent {
                if let Some(task) = self.downloads.pop_front() {
                    let client = client.clone();
                    let handle = tokio::spawn(async move { execute_download(client, task).await });
                    self.doing.push(handle);
                } else {
                    break;
                }
            }

            if !self.doing.is_empty() {
                let doing = std::mem::take(&mut self.doing);
                let (result, _idx, remaining) = futures::future::select_all(doing).await;
                self.doing = remaining;
                match result {
                    Ok(Ok(_)) => self.completed += 1,
                    Ok(Err(e)) => eprintln!("Download error: {e}"),
                    Err(e) => eprintln!("Task join error: {e}"),
                }
            }
        }
        Ok(())
    }
}

async fn execute_download(
    client: reqwest::Client,
    task: DownloadTask,
) -> Result<PathBuf, NetError> {
    let mut request = client.get(task.url.clone());

    for (key, value) in &task.headers {
        request = request.header(key.as_str(), value.as_str());
    }

    let response = request.send().await?;

    if !response.status().is_success() {
        return Err(NetError::Http(response.error_for_status().unwrap_err()));
    }

    let _output_path = match &task.sink {
        Sink::File { path, .. } => path.clone(),
        Sink::Bytes(_) => PathBuf::new(),
    };

    let mut stream = response;
    let mut bytes = Vec::new();

    while let Some(chunk) = stream.chunk().await? {
        bytes.extend_from_slice(&chunk);
    }

    match task.sink {
        Sink::File { path, atomic } => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            if atomic {
                let tmp = path.with_extension("tmp");
                std::fs::write(&tmp, &bytes)?;
                std::fs::rename(&tmp, &path)?;
            } else {
                std::fs::write(&path, &bytes)?;
            }
            Ok(path)
        }
        Sink::Bytes(data) => {
            let mut guard = data.lock().unwrap();
            guard.extend_from_slice(&bytes);
            Ok(PathBuf::new())
        }
    }
}
