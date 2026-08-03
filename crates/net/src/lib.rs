pub mod cache;

pub use cache::{CacheEntry, HttpMetaCache};

use futures::StreamExt;
use sha1::Digest as _;
use std::path::Path;
use thiserror::Error;

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
    #[error("Cache error: {0}")]
    Cache(String),
}

#[must_use]
pub fn default_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(release_the_launcher_constants::net::USER_AGENT)
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashKind {
    Sha1,
    Sha256,
    Sha512,
}

enum Hasher {
    Sha1(sha1::Sha1),
    Sha256(sha2::Sha256),
    Sha512(sha2::Sha512),
}

impl Hasher {
    fn new(kind: HashKind) -> Self {
        match kind {
            HashKind::Sha1 => Self::Sha1(sha1::Sha1::new()),
            HashKind::Sha256 => Self::Sha256(sha2::Sha256::new()),
            HashKind::Sha512 => Self::Sha512(sha2::Sha512::new()),
        }
    }

    fn update(&mut self, data: &[u8]) {
        match self {
            Self::Sha1(h) => h.update(data),
            Self::Sha256(h) => h.update(data),
            Self::Sha512(h) => h.update(data),
        }
    }

    fn finalize(self) -> String {
        match self {
            Self::Sha1(h) => hex::encode(h.finalize()),
            Self::Sha256(h) => hex::encode(h.finalize()),
            Self::Sha512(h) => hex::encode(h.finalize()),
        }
    }
}

/// Downloads a file from `url` and writes it to `dest`.
///
/// If `checksum` is provided, verifies the calculated hash against `expected_hash` (case-insensitive).
/// Progress callback `progress(downloaded, total)` is called on every chunk if provided.
///
/// # Errors
/// Returns [`NetError`] if request fails, IO fails, or checksum mismatches.
pub async fn download_to_file(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    checksum: Option<(HashKind, &str)>,
    progress: Option<impl Fn(u64, u64) + Send + Sync + 'static>,
) -> Result<(), NetError> {
    let res = client.get(url).send().await?.error_for_status()?;
    let total = res.content_length().unwrap_or(0);

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let tmp_dest = dest.with_extension("tmp");
    let mut file = tokio::fs::File::create(&tmp_dest).await?;

    let mut hasher = checksum.map(|(kind, _)| Hasher::new(kind));
    let mut downloaded: u64 = 0;
    let mut stream = res.bytes_stream();

    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res?;
        tokio::io::AsyncWriteExt::write_all(&mut file, &chunk).await?;

        if let Some(ref mut h) = hasher {
            h.update(&chunk);
        }

        downloaded += chunk.len() as u64;
        if let Some(ref cb) = progress {
            cb(downloaded, total);
        }
    }

    tokio::io::AsyncWriteExt::flush(&mut file).await?;
    drop(file);

    if let Some((_, expected_hash)) = checksum {
        if let Some(h) = hasher {
            let actual = h.finalize();
            if !actual.eq_ignore_ascii_case(expected_hash) {
                let _ = tokio::fs::remove_file(&tmp_dest).await;
                return Err(NetError::ChecksumMismatch {
                    expected: expected_hash.to_string(),
                    actual,
                });
            }
        }
    }

    tokio::fs::rename(&tmp_dest, dest).await?;
    Ok(())
}
