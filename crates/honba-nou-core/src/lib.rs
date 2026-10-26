//! # 本場・脳 honba-nou-core
//!
//! Core domain traits, closed decision parser, grounding checks, and verified indicator catalog.
//!
//! # Core Capabilities
//! - Closed decision grammar parser with `serde(deny_unknown_fields)` enforcement.
//! - Pre-trade price grounding checks against instrument [`honba_core::PriceBand`] and price collars.
//! - Mandatory SEBI `algo_id` validation (`^[A-Z0-9_]{4,32}$`).
//! - Integration with [`honba_mon::RiskEngine`].
//! - Verified mathematical [`IndicatorCatalog`] preventing hallucinations.
//! - Deterministic prompt hashing and template rendering.

pub mod decision;
pub mod error;
pub mod indicators;
pub mod prompt;
pub mod traits;

pub use decision::{
    extract_json_payload, is_valid_sebi_algo_id, DecisionParser, SEBI_ALGO_ID_REGEX,
};
pub use error::{CatalogError, DecisionError, LlmError};
pub use indicators::{
    validate_spec_indicators, IndicatorCatalog, IndicatorCategory, IndicatorInvocation,
    IndicatorSpec, ParameterSpec, StrategySpec,
};
pub use prompt::{compute_prompt_hash, hash_prompt_hex, sha256_hex, PromptHash, PromptTemplate};
pub use traits::LlmProvider;
