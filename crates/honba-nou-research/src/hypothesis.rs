//! Task 4.1: Research Hypothesis Pipeline.
//!
//! Formulates falsifiable quantitative trading hypotheses based on market regimes
//! (trend, mean-reversion, volatility breakout) and converts them into closed
//! `StrategySpec` instances adhering strictly to the verified `IndicatorCatalog`.

use chrono::{DateTime, Utc};
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use honba_nou_core::indicators::{
    IndicatorCatalog, IndicatorInvocation, StrategySpec as CoreStrategySpec,
};

/// Market regimes for hypothesis formulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketRegime {
    /// Upward trending market (momentum, breakout)
    TrendUp,
    /// Downward trending market (short momentum, breakdown)
    TrendDown,
    /// Rangebound / sideways market (mean reversion, oscillators)
    MeanReversion,
    /// Volatility expansion / breakout regime (squeeze release, volatility surge)
    VolatilityBreakout,
}

impl std::fmt::Display for MarketRegime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MarketRegime::TrendUp => write!(f, "trend-up"),
            MarketRegime::TrendDown => write!(f, "trend-down"),
            MarketRegime::MeanReversion => write!(f, "mean-reversion"),
            MarketRegime::VolatilityBreakout => write!(f, "volatility-breakout"),
        }
    }
}

/// Falsifiable quantitative trading hypothesis.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Hypothesis {
    /// Unique identifier for this hypothesis
    pub id: String,
    /// Target market regime
    pub regime: MarketRegime,
    /// Falsifiable proposition describing market mechanism and edge
    pub description: String,
    /// Entry condition rule
    pub entry_rule: String,
    /// Exit / risk management condition rule
    pub exit_rule: String,
    /// Closed declarative strategy specification
    pub strategy_spec: CoreStrategySpec,
    /// Prompt used to generate hypothesis (if LLM was queried)
    pub prompt_used: Option<String>,
    /// SHA-256 hash of prompt and specification
    pub prompt_hash: String,
    /// Timestamp when hypothesis was generated
    pub created_at: DateTime<Utc>,
}

/// Generator for quantitative research hypotheses.
pub struct HypothesisGenerator {
    catalog: IndicatorCatalog,
}

impl Default for HypothesisGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl HypothesisGenerator {
    /// Create a new hypothesis generator backed by the verified indicator catalog.
    pub fn new() -> Self {
        Self {
            catalog: IndicatorCatalog::new(),
        }
    }

    /// Access the underlying indicator catalog.
    pub fn catalog(&self) -> &IndicatorCatalog {
        &self.catalog
    }

    /// Formulate a falsifiable quantitative trading hypothesis for a given market regime.
    ///
    /// The generated strategy specification uses strictly verified indicators from the catalog:
    /// - TrendUp: EMA + Supertrend
    /// - TrendDown: VWAP + MACD
    /// - MeanReversion: RSI + CPR
    /// - VolatilityBreakout: Bollinger Bands + ATR + ADX
    pub fn generate_for_regime(
        &self,
        name: &str,
        regime: MarketRegime,
        universe: Vec<String>,
    ) -> Result<Hypothesis, String> {
        let (description, entry_rule, exit_rule, spec) = match regime {
            MarketRegime::TrendUp => {
                let mut spec = CoreStrategySpec::new(name);
                spec = spec
                    .with_indicator(IndicatorInvocation::new("EMA").with_param("length", dec!(20)));
                spec = spec.with_indicator(
                    IndicatorInvocation::new("SUPERTREND")
                        .with_param("period", dec!(10))
                        .with_param("multiplier", dec!(3.0)),
                );
                (
                    format!(
                        "TrendUp hypothesis: Price above EMA(20) with positive Supertrend(10, 3) indicates persistent upward momentum across {:?}.",
                        universe
                    ),
                    "Enter LONG when Close > EMA(20) and Supertrend flips positive".to_string(),
                    "Exit when Close crosses below Supertrend line or trailing stop of 2.5% is reached".to_string(),
                    spec,
                )
            }
            MarketRegime::TrendDown => {
                let mut spec = CoreStrategySpec::new(name);
                spec = spec.with_indicator(
                    IndicatorInvocation::new("VWAP").with_param("band_multiplier", dec!(1.5)),
                );
                spec = spec.with_indicator(
                    IndicatorInvocation::new("MACD")
                        .with_param("fast_length", dec!(12))
                        .with_param("slow_length", dec!(26))
                        .with_param("signal_length", dec!(9)),
                );
                (
                    format!(
                        "TrendDown hypothesis: Price trading below lower VWAP band with negative MACD histogram signals institutional selling across {:?}.",
                        universe
                    ),
                    "Enter SHORT when Close < lower VWAP band and MACD histogram < 0".to_string(),
                    "Exit when MACD line crosses above signal line or 2% profit target is hit"
                        .to_string(),
                    spec,
                )
            }
            MarketRegime::MeanReversion => {
                let mut spec = CoreStrategySpec::new(name);
                spec = spec
                    .with_indicator(IndicatorInvocation::new("RSI").with_param("length", dec!(14)));
                spec = spec.with_indicator(IndicatorInvocation::new("CPR"));
                (
                    format!(
                        "MeanReversion hypothesis: RSI < 30 at CPR bottom central pivot offers favorable risk-reward bounce in sideways markets across {:?}.",
                        universe
                    ),
                    "Enter LONG when RSI(14) < 30 and Price touches Bottom Central Pivot (BC)"
                        .to_string(),
                    "Exit when RSI crosses above 50 or reaches Central Pivot (P)".to_string(),
                    spec,
                )
            }
            MarketRegime::VolatilityBreakout => {
                let mut spec = CoreStrategySpec::new(name);
                spec = spec.with_indicator(
                    IndicatorInvocation::new("BOLLINGER_BANDS")
                        .with_param("length", dec!(20))
                        .with_param("std_dev", dec!(2.0)),
                );
                spec = spec
                    .with_indicator(IndicatorInvocation::new("ATR").with_param("period", dec!(14)));
                spec = spec
                    .with_indicator(IndicatorInvocation::new("ADX").with_param("length", dec!(14)));
                (
                    format!(
                        "VolatilityBreakout hypothesis: Compression of Bollinger Bands followed by ATR expansion and ADX > 25 generates high-probability directional breakouts across {:?}.",
                        universe
                    ),
                    "Enter on bar Close outside Bollinger Bands when ATR > 14-period SMA and ADX > 25".to_string(),
                    "Exit on ATR trailing stop of 2.0x ATR or reversal inside bands".to_string(),
                    spec,
                )
            }
        };

        // Strict grounding validation against official IndicatorCatalog
        self.catalog
            .validate_spec(&spec)
            .map_err(|e| format!("StrategySpec failed catalog validation: {e}"))?;

        let prompt_text = format!("{}:{}:{}", regime, name, description);
        let mut hasher = Sha256::new();
        hasher.update(prompt_text.as_bytes());
        let prompt_hash = format!("{:x}", hasher.finalize());

        Ok(Hypothesis {
            id: Uuid::new_v4().to_string(),
            regime,
            description,
            entry_rule,
            exit_rule,
            strategy_spec: spec,
            prompt_used: Some(prompt_text),
            prompt_hash,
            created_at: Utc::now(),
        })
    }

    /// Validate any candidate strategy spec against the verified catalog.
    pub fn validate_spec(&self, spec: &CoreStrategySpec) -> Result<(), String> {
        self.catalog
            .validate_spec(spec)
            .map_err(|e| format!("Indicator catalog validation error: {e}"))
    }
}
