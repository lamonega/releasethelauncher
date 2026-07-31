pub mod cache;
pub mod validator;

pub use cache::{CacheEntry, HttpMetaCache};
pub use validator::{ChecksumValidator, Sha1Validator, Sha256Validator};

use futures::TryStreamExt;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use thiserror::Error;
use tokio::task::JoinHandle;
use tokio_util::io::StreamReader;
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
    #[error("Validator error: {0}")]
    Validator(#[from] validator::ValidatorError),
    #[error("Download failed after {0} retries")]
    MaxRetries(u32),
    #[error("Rate limited, retry after {0}s")]
    RateLimited(u64),
    #[error("Cache error: {0}")]
    Cache(String),
}

#[derive(Debug, Clone)]
pub struct HttpClientProvider {
    client: reqwest::Client,
}

impl HttpClientProvider {
    /// # Errors
    /// Returns [`NetError`] if building the HTTP client fails.
    pub fn new() -> Result<Self, NetError> {
        let client = reqwest::Client::builder()
            .user_agent(release_the_launcher_constants::net::USER_AGENT)
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        Ok(Self { client })
    }

    #[must_use]
    pub const fn with_client(client: reqwest::Client) -> Self {
        Self { client }
    }

    #[must_use]
    pub const fn client(&self) -> &reqwest::Client {
        &self.client
    }

    #[must_use]
    pub fn clone_client(&self) -> reqwest::Client {
        self.client.clone()
    }
}

impl Default for HttpClientProvider {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| {
            // Fallback: best-effort client preserving User-Agent and timeout.
            let client = reqwest::Client::builder()
                .user_agent(release_the_launcher_constants::net::USER_AGENT)
                .timeout(std::time::Duration::from_secs(
                    release_the_launcher_constants::net::NET_TIMEOUT_SECS,
                ))
                .build()
                .unwrap_or_default();
            Self::with_client(client)
        })
    }
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
    _retries: u32,
    /// Used by future retry implementation.
    _max_retries: u32,
    completed: usize,
    total: usize,
}

impl DownloadJob {
    #[must_use]
    pub const fn new(max_concurrent: usize) -> Self {
        Self {
            downloads: VecDeque::new(),
            doing: Vec::new(),
            max_concurrent,
            _retries: 0,
            _max_retries: release_the_launcher_constants::net::DEFAULT_MAX_RETRIES,
            completed: 0,
            total: 0,
        }
    }

    pub fn add(&mut self, task: DownloadTask) {
        self.total += 1;
        self.downloads.push_back(task);
    }

    #[must_use]
    pub const fn progress(&self) -> (usize, usize) {
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
                    Ok(Err(e)) => {
                        return Err(e);
                    }
                    Err(e) => {
                        return Err(NetError::Io(std::io::Error::other(e.to_string())));
                    }
                }
            }
        }
        Ok(())
    }
}

async fn execute_download(
    client: reqwest::Client,
    mut task: DownloadTask,
) -> Result<PathBuf, NetError> {
    let mut request = client.get(task.url.clone());

    for (key, value) in &task.headers {
        request = request.header(key.as_str(), value.as_str());
    }

    let response = request.send().await?;

    if !response.status().is_success() {
        return Err(NetError::Http(response.error_for_status().unwrap_err()));
    }

    let output_path = match &task.sink {
        Sink::File { path, .. } => path.clone(),
        Sink::Bytes(_) => PathBuf::new(),
    };

    let stream = response.bytes_stream();
    let mut reader = StreamReader::new(stream.map_err(std::io::Error::other));

    match &mut task.sink {
        Sink::File { path, atomic } => {
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }

            let dest = if *atomic {
                path.with_extension(release_the_launcher_constants::net::TEMP_FILE_EXT)
            } else {
                path.clone()
            };

            let mut file = tokio::fs::File::create(&dest).await?;
            let mut buf = vec![0u8; release_the_launcher_constants::net::DOWNLOAD_BUFFER_SIZE];

            loop {
                let n = tokio::io::AsyncReadExt::read(&mut reader, &mut buf).await?;
                if n == 0 {
                    break;
                }
                let chunk = &buf[..n];

                // Feed to validator if present
                if let Some(ref mut validator) = task.validator {
                    validator.update(chunk);
                }

                // Write to file
                tokio::io::AsyncWriteExt::write_all(&mut file, chunk).await?;
            }

            // Finalize validation
            drop(file);

            if let Some(validator) = task.validator.take() {
                let computed = validator.finalize()?;
                if let Some(ref expected) = task.expected_hash {
                    if !computed.eq_ignore_ascii_case(expected) {
                        // Remove the failed file
                        let _ = tokio::fs::remove_file(&dest).await;
                        return Err(NetError::ChecksumMismatch {
                            expected: expected.clone(),
                            actual: computed,
                        });
                    }
                }
            }

            if *atomic {
                tokio::fs::rename(&dest, path).await?;
            }

            Ok(output_path)
        }
        Sink::Bytes(data) => {
            let mut buf = vec![0u8; release_the_launcher_constants::net::DOWNLOAD_BUFFER_SIZE];

            loop {
                let n = tokio::io::AsyncReadExt::read(&mut reader, &mut buf).await?;
                if n == 0 {
                    break;
                }
                let chunk = &buf[..n];

                if let Some(ref mut validator) = task.validator {
                    validator.update(chunk);
                }

                data.lock().unwrap().extend_from_slice(chunk);
            }

            if let Some(validator) = task.validator.take() {
                let computed = validator.finalize()?;
                if let Some(ref expected) = task.expected_hash {
                    if !computed.eq_ignore_ascii_case(expected) {
                        return Err(NetError::ChecksumMismatch {
                            expected: expected.clone(),
                            actual: computed,
                        });
                    }
                }
            }

            Ok(PathBuf::new())
        }
    }
}
