//! Error types for honba-nou-core.

use honba_core::{Price, PriceBand};
use honba_mon::RiskRefusal;
use rust_decimal::Decimal;
use thiserror::Error;

/// Errors arising during AI/LLM decision parsing and safety validation.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DecisionError {
    /// Failure to parse JSON or malformed format
    #[error("JSON deserialization error: {0}")]
    Json(String),

    /// Extra or unauthorized fields in the LLM response (deny_unknown_fields)
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

    /// Scrip-wise price band circuit violation
    #[error("Price band circuit violation: price {price} is outside circuit bounds [{lower}, {upper}]", lower = band.lower, upper = band.upper)]
    PriceBandViolation {
        /// Attempted price
        price: Price,
        /// Active price band
        band: PriceBand,
    },

    /// Missing mandatory SEBI algo ID
    #[error("algo_id cannot be empty")]
    EmptyAlgoId,

    /// SEBI algo ID does not conform to circular requirements
    #[error("algo_id '{algo_id}' invalid: must match pattern '{expected_format}'")]
    InvalidAlgoIdFormat {
        /// Provided algo ID
        algo_id: String,
        /// Expected regex / pattern format
        expected_format: &'static str,
    },

    /// Pre-trade RiskEngine refusal
    #[error("Pre-trade RiskEngine refused decision: {0}")]
    RiskRejected(RiskRefusal),
}

/// Errors arising during indicator catalog lookup and strategy spec validation.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CatalogError {
    /// Strategy requested an indicator not present in verified catalog
    #[error("Unapproved indicator '{0}' not in verified catalog")]
    UnapprovedIndicator(String),

    /// Required parameter missing for indicator
    #[error("Missing parameter '{parameter}' for indicator '{indicator}'")]
    MissingParameter {
        /// Indicator name
        indicator: String,
        /// Parameter name
        parameter: String,
    },

    /// Indicator parameter violates approved mathematical bounds
    #[error(
        "Parameter '{parameter}' value {value} for indicator '{indicator}' is outside approved bounds [{min}, {max}]"
    )]
    ParameterOutOfBounds {
        /// Indicator name
        indicator: String,
        /// Parameter name
        parameter: String,
        /// Provided value
        value: Decimal,
        /// Minimum permitted value
        min: Decimal,
        /// Maximum permitted value
        max: Decimal,
    },

    /// Inconsistent combination of parameters (e.g. fast >= slow in MACD)
    #[error("Invalid parameter combination for indicator '{indicator}': {reason}")]
    InvalidParameterCombination {
        /// Indicator name
        indicator: String,
        /// Description of why the parameter combination is invalid
        reason: String,
    },

    /// Overall strategy specification validation failure
    #[error("Strategy specification validation failed: {0}")]
    SpecValidationFailed(String),
}

/// Errors that can occur during LLM provider operations.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LlmError {
    /// Transport error occurred during LLM communication.
    #[error("transport: {0}")]
    Transport(String),
    /// Authentication failure (invalid credentials, expired token, etc.).
    #[error("auth failure")]
    Auth,
    /// Rate limit exceeded.
    #[error("rate limited")]
    RateLimited,
    /// Audit record invalid or malformed.
    #[error("audit record invalid: {0}")]
    AuditRecord(String),
    /// Decision parsing failure.
    #[error("parsing failed: {0}")]
    ParsingFailed(String),
}
