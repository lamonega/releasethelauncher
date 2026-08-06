//! HTTP layer shared by the backend: a default [`reqwest::Client`]
//! ([`default_client`]), streaming downloads with checksum validation
//! ([`download_to_file`]) and the metadata HTTP cache ([`cache`]).

pub mod cache;

use futures::StreamExt;
use sha2::digest::DynDigest;
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

impl HashKind {
    #[must_use]
    pub fn create_hasher(self) -> Box<dyn DynDigest + Send> {
        match self {
            Self::Sha1 => Box::new(sha1::Sha1::default()),
            Self::Sha256 => Box::new(sha2::Sha256::default()),
            Self::Sha512 => Box::new(sha2::Sha512::default()),
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
    progress: Option<&(dyn Fn(u64, u64) + Send + Sync)>,
) -> Result<(), NetError> {
    let res = client.get(url).send().await?.error_for_status()?;
    let total = res.content_length().unwrap_or(0);

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let tmp_dest = dest.with_file_name(format!(
        "{}.tmp",
        dest.file_name().unwrap_or_default().to_string_lossy()
    ));
    let mut file = tokio::fs::File::create(&tmp_dest).await?;

    let mut hasher = checksum.map(|(kind, _)| kind.create_hasher());
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
        if let Some(mut h) = hasher {
            let actual = hex::encode(h.finalize_reset());
            if !actual.eq_ignore_ascii_case(expected_hash) {
                if let Err(e) = tokio::fs::remove_file(&tmp_dest).await {
                    tracing::warn!("Failed to remove temp file {}: {e}", tmp_dest.display());
                }
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
