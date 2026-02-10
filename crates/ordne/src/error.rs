use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum OrdneError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Drive not found: {0}")]
    DriveNotFound(String),

    #[error("File not found: {0}")]
    FileNotFound(PathBuf),

    #[error("Hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },

    #[error("Insufficient space: {available} bytes available, {required} bytes required")]
    InsufficientSpace { available: u64, required: u64 },

    #[error("Invalid status transition: {from} -> {to}")]
    InvalidStatusTransition { from: String, to: String },

    #[error("Migration error: {0}")]
    Migration(String),

    #[error("Plan not found: {0}")]
    PlanNotFound(i64),

    #[error("Plan not approved: {0}")]
    PlanNotApproved(i64),

    #[error("External tool error: {tool} failed: {message}")]
    ExternalTool { tool: String, message: String },

    #[error("Source file changed: expected hash {expected}, current hash {actual}")]
    SourceChanged { expected: String, actual: String },

    #[error("Destination verification failed for {path}")]
    DestinationVerification { path: PathBuf },

    #[error("Drive offline: {0}")]
    DriveOffline(String),

    #[error("Invalid backend: {0}")]
    InvalidBackend(String),

    #[error("User input error: {0}")]
    UserInput(String),
}

impl From<dialoguer::Error> for OrdneError {
    fn from(err: dialoguer::Error) -> Self {
        OrdneError::UserInput(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, OrdneError>;
