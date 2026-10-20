//! Closed LLM Decision Parser and Safety Validation Harness.
//!
//! Language models are untrusted for direct execution.
//! This module strictly enforces:
//! - Closed enum parsing using [`honba_core::Decision`] with `deny_unknown_fields`.
//! - Strict symbol universe verification (rejects unknown / undefined instruments).
//! - Strictly positive quantities and limit prices.
//! - Grounding check: rejects ungrounded limit prices outside safety collars.
//! - Direct, mandatory pre-trade evaluation against [`honba_mon::RiskEngine`].

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use honba_core::{Decision, Price};
use honba_mon::RiskEngine;

use crate::error::DecisionError;

/// Strict parser and safety validator for AI/LLM decisions.
#[derive(Debug, Clone)]
pub struct DecisionParser {
    /// Allowed trading universe symbols (if None, any non-empty symbol is allowed)
    pub allowed_universe: Option<HashSet<String>>,
    /// Maximum allowed deviation percentage for limit prices relative to market reference price
    /// Default is 20.0 (i.e. \u{b1}20% price collar)
    pub max_price_deviation_pct: Decimal,
}

impl Default for DecisionParser {
    fn default() -> Self {
        Self {
            allowed_universe: None,
            max_price_deviation_pct: dec!(20.00),
        }
    }
}

impl DecisionParser {
    /// Create a new decision parser with default 20% limit price collar.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set an explicit allowed trading universe (e.g. NIFTY 100 constituent symbols).
    pub fn with_universe(mut self, symbols: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let set: HashSet<String> = symbols.into_iter().map(Into::into).collect();
        self.allowed_universe = Some(set);
        self
    }

    /// Set maximum allowed limit price deviation from reference price (in percent).
    pub fn with_max_price_deviation(mut self, pct: Decimal) -> Self {
        self.max_price_deviation_pct = pct;
        self
    }

    /// Parse raw LLM output text into [`honba_core::Decision`], stripping markdown fences.
    ///
    /// Enforces `deny_unknown_fields`: any unauthorized field injected by the model
    /// will result in an immediate [`DecisionError::UnknownField`] or [`DecisionError::Json`].
    pub fn parse_str(&self, raw: &str) -> Result<Decision, DecisionError> {
        let cleaned = extract_json_payload(raw);
        let decision: Decision = serde_json::from_str(cleaned).map_err(|e| {
            let msg = e.to_string();
            if msg.contains("unknown field") {
                DecisionError::UnknownField(msg)
            } else {
                DecisionError::Json(msg)
            }
        })?;

        Ok(decision)
    }

    /// Perform strict safety validation on a parsed [`Decision`].
    pub fn validate_decision(
        &self,
        decision: &Decision,
        reference_price: Option<Price>,
    ) -> Result<(), DecisionError> {
        // Enforce core domain invariants
        if let Err(e) = decision.validate() {
            return Err(DecisionError::Json(e.to_string()));
        }

        match decision {
            Decision::Buy {
                symbol,
                qty,
                price_limit,
                algo_id,
                ..
            }
            | Decision::Sell {
                symbol,
                qty,
                price_limit,
                algo_id,
                ..
            } => {
                let trimmed_sym = symbol.trim();
                if trimmed_sym.is_empty() {
                    return Err(DecisionError::EmptySymbol);
                }

                // Universe validation
                if let Some(universe) = &self.allowed_universe {
                    if !universe.contains(trimmed_sym) {
                        return Err(DecisionError::SymbolNotInUniverse(trimmed_sym.to_string()));
                    }
                }

                // Quantity must be positive
                if !qty.is_positive() {
                    return Err(DecisionError::NonPositiveQuantity(qty.0));
                }

                // Algo ID must be non-empty
                if algo_id.trim().is_empty() {
                    return Err(DecisionError::EmptyAlgoId);
                }

                // Limit price validation & grounding check
                if let Some(limit) = price_limit {
                    if !limit.is_positive() {
                        return Err(DecisionError::NonPositivePrice(*limit));
                    }

                    // Check grounding against market reference price
                    if let Some(ref_price) = reference_price {
                        if ref_price.is_positive() {
                            let diff = (limit.0 - ref_price.0).abs();
                            let deviation_pct = (diff / ref_price.0) * dec!(100.00);
                            if deviation_pct > self.max_price_deviation_pct {
                                return Err(DecisionError::UngroundedPrice {
                                    limit: *limit,
                                    reference: ref_price,
                                    deviation_pct: deviation_pct.round_dp(2),
                                    max_pct: self.max_price_deviation_pct,
                                });
                            }
                        }
                    }
                }

                Ok(())
            }
            Decision::Hold { .. } => Ok(()),
            Decision::CancelAll { algo_id, .. } => {
                if let Some(id) = algo_id {
                    if id.trim().is_empty() {
                        return Err(DecisionError::EmptyAlgoId);
                    }
                }
                Ok(())
            }
        }
    }

    /// Parse raw LLM output and perform full safety validation.
    pub fn parse_and_validate(
        &self,
        raw: &str,
        reference_price: Option<Price>,
    ) -> Result<Decision, DecisionError> {
        let decision = self.parse_str(raw)?;
        self.validate_decision(&decision, reference_price)?;
        Ok(decision)
    }

    /// Submit a validated decision to [`honba_mon::RiskEngine`] pre-trade gate.
    pub fn evaluate_with_risk(
        &self,
        decision: &Decision,
        risk_engine: &mut RiskEngine,
        now: DateTime<Utc>,
    ) -> Result<(), DecisionError> {
        risk_engine
            .check_decision(decision, now)
            .map_err(DecisionError::RiskRejected)
    }
}

/// Helper to strip markdown JSON fences: ```json ... ``` or extract outer `{ ... }`.
fn extract_json_payload(raw: &str) -> &str {
    let s = raw.trim();
    if let Some(stripped) = s.strip_prefix("```json") {
        if let Some(end) = stripped.rfind("```") {
            return stripped[..end].trim();
        }
    } else if let Some(stripped) = s.strip_prefix("```") {
        if let Some(end) = stripped.rfind("```") {
            return stripped[..end].trim();
        }
    }

    // If there is surrounding text outside the JSON object, extract between first '{' and last '}'
    if let (Some(start), Some(end)) = (s.find('{'), s.rfind('}')) {
        if start <= end {
            return &s[start..=end];
        }
    }

    s
}
