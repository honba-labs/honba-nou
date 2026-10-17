//! Typed decision parsing.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Decision {
    Buy { symbol: String, qty: u64, limit: Option<String> },
    Sell { symbol: String, qty: u64, limit: Option<String> },
    Hold { reason: String },
}
