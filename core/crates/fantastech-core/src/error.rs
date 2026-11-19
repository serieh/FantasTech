//! Core error types and conversions.

use fantastech_protocol::ProtocolError;
use thiserror::Error;

/// High-level core errors.
#[derive(Error, Debug)]
pub enum CoreError {
    /// TOML serialization error.
    #[error("TOML serialization error: {0}")]
    Toml(#[from] toml::ser::Error),

    /// TOML deserialization error.
    #[error("TOML deserialization error: {0}")]
    TomlDe(#[from] toml::de::Error),

    /// JSON serialization or parsing error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Underlying protocol error.
    #[error("Protocol error: {0}")]
    Protocol(#[from] ProtocolError),

    /// Macro exceeds hardware limits.
    #[error("Macro compilation failed: {0}")]
    MacroCompilationFailed(&'static str),

    /// Unknown key name.
    #[error("Unknown key identifier: {0}")]
    UnknownKeyName(String),

    /// Unknown action descriptor.
    #[error("Unknown action descriptor: {0}")]
    UnknownAction(String),
}
