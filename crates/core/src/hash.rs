use sha1::{Digest, Sha1};
use sha2::Sha256;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

fn hash_file<D: Digest>(path: &Path, mut hasher: D) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut buffer = [0u8; 8192];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[must_use]
pub fn compute_sha1_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha1::digest(bytes))
}

/// # Errors
///
/// Returns [`io::Error`] if opening or reading the file fails.
pub fn compute_sha1_file(path: &Path) -> Result<String, io::Error> {
    hash_file(path, Sha1::new())
}

#[must_use]
pub(crate) fn compute_sha256_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// # Errors
///
/// Returns [`io::Error`] if opening or reading the file fails.
pub(crate) fn compute_sha256_file(path: &Path) -> Result<String, io::Error> {
    hash_file(path, Sha256::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha1_known_vector() {
        assert_eq!(
            compute_sha1_bytes(b"hello world"),
            "2aae6c35c94fcfb415dbe95f408b9ce91ee846ed"
        );
    }

    #[test]
    fn test_sha256_known_vector() {
        assert_eq!(
            compute_sha256_bytes(b"hello world"),
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }
}
