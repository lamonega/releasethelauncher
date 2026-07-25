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
