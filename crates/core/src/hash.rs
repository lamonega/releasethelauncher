use sha1::{Digest, Sha1};
use std::fs::File;
use std::io;
use std::path::Path;

pub fn compute_sha1_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha1::digest(bytes))
}

/// # Errors
///
/// Returns [`io::Error`] if opening or reading the file fails.
pub fn compute_sha1_file(path: &Path) -> Result<String, io::Error> {
    let mut file = File::open(path)?;
    let mut hasher = Sha1::new();
    io::copy(&mut file, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
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
}
