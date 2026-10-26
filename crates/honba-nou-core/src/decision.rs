//! Closed LLM Decision Parser and Safety Validation Harness.
//!
//! Language models are untrusted for direct execution.
//! This module strictly enforces:
//! - Closed enum parsing using [`honba_core::Decision`] with `deny_unknown_fields`.
//! - Strict symbol universe verification (rejects unknown / undefined instruments).
//! - Strictly positive quantities and limit prices (zero float arithmetic).
//! - Mandatory SEBI `algo_id` format matching `^[A-Z0-9_]{4,32}$`.
//! - Grounding checks: asserts limit orders sit within instrument's active [`PriceBand`]
//!   and/or price collar bounds.
//! - Direct, mandatory pre-trade evaluation against [`honba_mon::RiskEngine`].

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use honba_core::{Decision, Price, PriceBand};
use honba_mon::RiskEngine;

use crate::error::DecisionError;

/// Format specification for SEBI algo identifiers.
pub const SEBI_ALGO_ID_REGEX: &str = "^[A-Z0-9_]{4,32}$";

/// Checks if an algo ID strictly matches SEBI Retail Algo circular rules (`^[A-Z0-9_]{4,32}$`).
pub fn is_valid_sebi_algo_id(algo_id: &str) -> bool {
    (4..=32).contains(&algo_id.len())
        && algo_id
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// Strict parser and safety validator for AI/LLM decisions.
#[derive(Debug, Clone)]
pub struct DecisionParser {
    /// Allowed trading universe symbols (if None, any non-empty symbol is allowed)
    pub allowed_universe: Option<HashSet<String>>,
    /// Maximum allowed deviation percentage for limit prices relative to market reference price.
    /// Default is 20.0 (i.e. ±20% price collar).
    pub max_price_deviation_pct: Decimal,
    /// Whether to strictly enforce the uppercase SEBI algo ID pattern (`^[A-Z0-9_]{4,32}$`).
    /// Defaults to true.
    pub strict_sebi_algo_format: bool,
}

impl Default for DecisionParser {
    fn default() -> Self {
        Self {
            allowed_universe: None,
            max_price_deviation_pct: dec!(20.00),
            strict_sebi_algo_format: true,
        }
    }
}

impl DecisionParser {
    /// Create a new decision parser with default settings.
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

    /// Set whether strict SEBI algo ID format (`^[A-Z0-9_]{4,32}$`) is enforced.
    pub fn with_strict_sebi_algo_format(mut self, strict: bool) -> Self {
        self.strict_sebi_algo_format = strict;
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

    /// Validate SEBI algo ID according to configured format rules.
    pub fn validate_algo_id(&self, algo_id: &str) -> Result<(), DecisionError> {
        let trimmed = algo_id.trim();
        if trimmed.is_empty() {
            return Err(DecisionError::EmptyAlgoId);
        }

        if self.strict_sebi_algo_format && !is_valid_sebi_algo_id(trimmed) {
            return Err(DecisionError::InvalidAlgoIdFormat {
                algo_id: trimmed.to_string(),
                expected_format: SEBI_ALGO_ID_REGEX,
            });
        }

        Ok(())
    }

    /// Perform strict safety validation and grounding check on a parsed [`Decision`].
    pub fn validate_decision(
        &self,
        decision: &Decision,
        reference_price: Option<Price>,
        price_band: Option<&PriceBand>,
    ) -> Result<(), DecisionError> {
        // Enforce core domain invariants
        if let Err(e) = decision.validate() {
            return Err(DecisionError::Json(e.to_string()));
        }

        match decision {
            Decision::PlaceOrder {
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

                // Quantity must be strictly positive
                if !qty.is_positive() {
                    return Err(DecisionError::NonPositiveQuantity(qty.0));
                }

                // Algo ID validation
                self.validate_algo_id(algo_id)?;

                // Limit price validation & grounding check
                if let Some(limit) = price_limit {
                    if !limit.is_positive() {
                        return Err(DecisionError::NonPositivePrice(*limit));
                    }

                    // Scrip-wise price band grounding check
                    if let Some(band) = price_band {
                        if !band.contains(*limit) {
                            return Err(DecisionError::PriceBandViolation {
                                price: *limit,
                                band: *band,
                            });
                        }
                    }

                    // Price collar grounding check against market reference price
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
            Decision::ModifyOrder {
                new_qty,
                new_price,
                algo_id,
                ..
            } => {
                self.validate_algo_id(algo_id)?;

                if let Some(qty) = new_qty {
                    if !qty.is_positive() {
                        return Err(DecisionError::NonPositiveQuantity(qty.0));
                    }
                }

                if let Some(price) = new_price {
                    if !price.is_positive() {
                        return Err(DecisionError::NonPositivePrice(*price));
                    }
                    if let Some(band) = price_band {
                        if !band.contains(*price) {
                            return Err(DecisionError::PriceBandViolation {
                                price: *price,
                                band: *band,
                            });
                        }
                    }
                }

                Ok(())
            }
            Decision::CancelOrder { algo_id, .. } => {
                if let Some(id) = algo_id {
                    self.validate_algo_id(id)?;
                }
                Ok(())
            }
            Decision::CancelAll { algo_id, .. } => {
                if let Some(id) = algo_id {
                    self.validate_algo_id(id)?;
                }
                Ok(())
            }
            Decision::Hold { .. } => Ok(()),
        }
    }

    /// Parse raw LLM output and perform full safety validation.
    pub fn parse_and_validate(
        &self,
        raw: &str,
        reference_price: Option<Price>,
        price_band: Option<&PriceBand>,
    ) -> Result<Decision, DecisionError> {
        let decision = self.parse_str(raw)?;
        self.validate_decision(&decision, reference_price, price_band)?;
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
pub fn extract_json_payload(raw: &str) -> &str {
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
