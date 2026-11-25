use std::fmt;

/// Recoverable failure while reading or writing a project.
#[derive(Debug)]
pub struct MotError {
    message: String,
}

impl MotError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for MotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for MotError {}

impl From<std::io::Error> for MotError {
    fn from(value: std::io::Error) -> Self {
        Self::new(value.to_string())
    }
}

impl From<serde_json::Error> for MotError {
    fn from(value: serde_json::Error) -> Self {
        Self::new(value.to_string())
    }
}

impl From<image::ImageError> for MotError {
    fn from(value: image::ImageError) -> Self {
        Self::new(value.to_string())
    }
}

impl From<serde_yml::Error> for MotError {
    fn from(value: serde_yml::Error) -> Self {
        Self::new(value.to_string())
    }
}

pub(crate) type Result<T> = std::result::Result<T, MotError>;
