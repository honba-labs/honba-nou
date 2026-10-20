//! Error types for closed LLM decision parsing, validation, and replay harness.

use rust_decimal::Decimal;
use thiserror::Error;

use honba_core::Price;
use honba_mon::RiskRefusal;

/// Errors arising during LLM decision parsing and safety validation.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DecisionError {
    /// Failure to parse JSON or malformed format
    #[error("JSON deserialization error: {0}")]
    Json(String),

    /// Extra or unauthorized fields in the LLM response
    #[error("Unknown or unauthorized field detected (deny_unknown_fields): {0}")]
    UnknownField(String),

    /// Symbol cannot be empty
    #[error("Symbol cannot be empty")]
    EmptySymbol,

    /// Symbol is not in the allowed trading universe
    #[error("Symbol '{0}' is not in the allowed trading universe")]
    SymbolNotInUniverse(String),

    /// Quantity must be strictly positive
    #[error("Quantity must be strictly positive, got {0}")]
    NonPositiveQuantity(Decimal),

    /// Limit price must be strictly positive
    #[error("Price limit must be strictly positive, got {0}")]
    NonPositivePrice(Price),

    /// Ungrounded limit price violating reference price collar or bounds
    #[error(
        "Ungrounded limit price: limit {limit} deviates by {deviation_pct}% from reference {reference} (max allowed {max_pct}%)"
    )]
    UngroundedPrice {
        /// Attempted limit price
        limit: Price,
        /// Market reference price
        reference: Price,
        /// Calculated deviation percentage
        deviation_pct: Decimal,
        /// Maximum allowed percentage deviation
        max_pct: Decimal,
    },

    /// Missing mandatory SEBI algo ID
    #[error("algo_id cannot be empty")]
    EmptyAlgoId,

    /// Pre-trade RiskEngine refusal
    #[error("Pre-trade RiskEngine refused decision: {0}")]
    RiskRejected(RiskRefusal),
}

/// Errors arising during prompt logging, integrity verification, or replay.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ReplayError {
    /// File or I/O failure
    #[error("I/O error: {0}")]
    Io(String),

    /// JSON serialization or deserialization failure
    #[error("JSON error: {0}")]
    Json(String),

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

    /// No matching record found in replay log
    #[error("No matching replay record found for query: {0}")]
    NoRecordFound(String),

    /// Replay trail is empty
    #[error("Replay log contains no records")]
    EmptyTrail,
}
