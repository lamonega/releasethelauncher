use sha1::Sha1;
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ValidatorError {
    #[error("Checksum mismatch: expected {expected}, got {actual}")]
    Mismatch { expected: String, actual: String },
}

pub trait ChecksumValidator: Send {
    fn update(&mut self, data: &[u8]);
    /// # Errors
    /// Returns `ValidatorError::Mismatch` if the checksum does not match the expected value.
    fn finalize(self: Box<Self>) -> Result<String, ValidatorError>;
}

pub struct Sha256Validator {
    expected: Option<String>,
    state: Sha256,
}

impl Sha256Validator {
    #[must_use]
    pub fn new(expected: Option<String>) -> Self {
        Self {
            expected,
            state: Sha256::new(),
        }
    }
}

impl ChecksumValidator for Sha256Validator {
    fn update(&mut self, data: &[u8]) {
        self.state.update(data);
    }

    fn finalize(self: Box<Self>) -> Result<String, ValidatorError> {
        let result = self.state.finalize();
        let hex = hex::encode(result);

        if let Some(ref expected) = self.expected {
            if hex != *expected {
                return Err(ValidatorError::Mismatch {
                    expected: expected.clone(),
                    actual: hex,
                });
            }
        }

        Ok(hex)
    }
}

pub struct Sha1Validator {
    expected: Option<String>,
    state: Sha1,
}

impl Sha1Validator {
    #[must_use]
    pub fn new(expected: Option<String>) -> Self {
        Self {
            expected,
            state: Sha1::new(),
        }
    }
}

impl ChecksumValidator for Sha1Validator {
    fn update(&mut self, data: &[u8]) {
        self.state.update(data);
    }

    fn finalize(self: Box<Self>) -> Result<String, ValidatorError> {
        let result = self.state.finalize();
        let hex = hex::encode(result);

        if let Some(ref expected) = self.expected {
            if hex != *expected {
                return Err(ValidatorError::Mismatch {
                    expected: expected.clone(),
                    actual: hex,
                });
            }
        }

        Ok(hex)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha1_validator_success() {
        let mut validator = Box::new(Sha1Validator::new(Some(
            "2fd4e1c67a2d28fced849ee1bb76e7391b93eb12".to_string(),
        )));
        validator.update(b"The quick brown fox jumps over the lazy dog");
        assert!(validator.finalize().is_ok());
    }

    #[test]
    fn test_sha256_validator_success() {
        let mut validator = Box::new(Sha256Validator::new(Some(
            "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592".to_string(),
        )));
        validator.update(b"The quick brown fox jumps over the lazy dog");
        assert!(validator.finalize().is_ok());
    }
}
