//! Replay and audit error types.

use thiserror::Error;

/// Errors arising during prompt logging, cryptographic chain verification, or replay.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ReplayError {
    /// File or I/O failure
    #[error("I/O error: {0}")]
    Io(String),

    /// JSON serialization or deserialization failure
    #[error("JSON error: {0}")]
    Json(String),

    /// Database failure (e.g. SQLite error)
    #[error("Database error: {0}")]
    Database(String),

    /// Prompt or output SHA256 checksum mismatch
    #[error(
        "Integrity checksum mismatch for record '{record_id}': expected SHA256 {expected}, calculated {actual}"
    )]
    ChecksumMismatch {
        /// Record identifier
        record_id: String,
        /// Expected SHA256 hex
        expected: String,
        /// Calculated SHA256 hex
        actual: String,
    },

    /// Cryptographic hash chain broken at sequence number
    #[error(
        "Cryptographic hash chain broken at sequence {sequence_number}: expected prev_hash {expected_prev}, actual {actual_prev}"
    )]
    ChainBroken {
        /// Sequence number where break was detected
        sequence_number: u64,
        /// Expected previous record hash (hex)
        expected_prev: String,
        /// Actual previous record hash (hex)
        actual_prev: String,
    },

    /// Deterministic cache miss in backtest mode
    #[error("Deterministic cache miss in backtest mode for prompt_sha256 {prompt_sha256}")]
    CacheMiss {
        /// Prompt hash in lowercase hex
        prompt_sha256: String,
    },

    /// No matching record found in replay log
    #[error("No matching replay record found for query: {0}")]
    NoRecordFound(String),

    /// Replay trail contains no records
    #[error("Replay log contains no records")]
    EmptyTrail,

    /// Decision parsing failed during replay
    #[error("Decision parsing failed: {0}")]
    DecisionParse(String),
}
