//! Verified Quantitative Indicator Catalog and Specification Validator.
//!
//! Language models are prone to hallucinating non-existent indicators or mathematically
//! invalid parameter combinations. This module enforces a closed, mathematically verified
//! vocabulary of technical and microstructure indicators with strict domain bounds.

use std::collections::HashMap;

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::CatalogError;

/// Categories of verified indicators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IndicatorCategory {
    /// Moving Averages (trend tracking)
    MovingAverage,
    /// Oscillators (momentum and mean reversion)
    Oscillator,
    /// Indian Microstructure specific indicators (CPR, VWAP)
    IndianMicrostructure,
    /// Volatility and Trend strength indicators
    VolatilityTrend,
}

/// Specification for an approved indicator parameter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ParameterSpec {
    /// Parameter identifier (e.g. "length", "multiplier")
    pub name: String,
    /// Minimum allowed value (inclusive)
    pub min: Decimal,
    /// Maximum allowed value (inclusive)
    pub max: Decimal,
    /// Granular step size
    pub step: Decimal,
    /// Default canonical value
    pub default: Decimal,
    /// Human-readable description
    pub description: String,
}

/// Approved mathematical indicator entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct IndicatorSpec {
    /// Canonical indicator symbol / name (e.g. "RSI", "MACD", "SUPERTREND")
    pub name: String,
    /// Indicator taxonomy classification
    pub category: IndicatorCategory,
    /// Human-readable explanation
    pub description: String,
    /// Required/allowed parameters with strict bounds
    pub parameters: Vec<ParameterSpec>,
}

/// Verified Indicator Catalog registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndicatorCatalog {
    indicators: HashMap<String, IndicatorSpec>,
}

impl Default for IndicatorCatalog {
    fn default() -> Self {
        Self::new()
    }
}

impl IndicatorCatalog {
    /// Construct the official verified indicator catalog with all approved mathematical indicators.
    pub fn new() -> Self {
        let mut catalog = HashMap::new();

        // 1. Moving Averages: SMA, EMA (lengths 2–500)
        catalog.insert(
            "SMA".to_string(),
            IndicatorSpec {
                name: "SMA".to_string(),
                category: IndicatorCategory::MovingAverage,
                description: "Simple Moving Average over a lookback window".to_string(),
                parameters: vec![ParameterSpec {
                    name: "length".to_string(),
                    min: dec!(2),
                    max: dec!(500),
                    step: dec!(1),
                    default: dec!(20),
                    description: "Number of price bars in rolling window".to_string(),
                }],
            },
        );

        catalog.insert(
            "EMA".to_string(),
            IndicatorSpec {
                name: "EMA".to_string(),
                category: IndicatorCategory::MovingAverage,
                description: "Exponential Moving Average with exponential weighting".to_string(),
                parameters: vec![ParameterSpec {
                    name: "length".to_string(),
                    min: dec!(2),
                    max: dec!(500),
                    step: dec!(1),
                    default: dec!(20),
                    description: "Lookback smoothing length".to_string(),
                }],
            },
        );

        // 2. Oscillators: RSI (2–100), MACD (fast < slow), Stochastic, ADX (5–50)
        catalog.insert(
            "RSI".to_string(),
            IndicatorSpec {
                name: "RSI".to_string(),
                category: IndicatorCategory::Oscillator,
                description: "Relative Strength Index momentum oscillator".to_string(),
                parameters: vec![ParameterSpec {
                    name: "length".to_string(),
                    min: dec!(2),
                    max: dec!(100),
                    step: dec!(1),
                    default: dec!(14),
                    description: "RSI calculation period".to_string(),
                }],
            },
        );

        catalog.insert(
            "MACD".to_string(),
            IndicatorSpec {
                name: "MACD".to_string(),
                category: IndicatorCategory::Oscillator,
                description: "Moving Average Convergence Divergence with signal line".to_string(),
                parameters: vec![
                    ParameterSpec {
                        name: "fast_length".to_string(),
                        min: dec!(2),
                        max: dec!(50),
                        step: dec!(1),
                        default: dec!(12),
                        description: "Fast exponential moving average length".to_string(),
                    },
                    ParameterSpec {
                        name: "slow_length".to_string(),
                        min: dec!(2),
                        max: dec!(200),
                        step: dec!(1),
                        default: dec!(26),
                        description:
                            "Slow exponential moving average length (must be > fast_length)"
                                .to_string(),
                    },
                    ParameterSpec {
                        name: "signal_length".to_string(),
                        min: dec!(2),
                        max: dec!(50),
                        step: dec!(1),
                        default: dec!(9),
                        description: "Signal line smoothing length".to_string(),
                    },
                ],
            },
        );

        catalog.insert(
            "STOCHASTIC".to_string(),
            IndicatorSpec {
                name: "STOCHASTIC".to_string(),
                category: IndicatorCategory::Oscillator,
                description: "Stochastic Oscillator (%K and %D lines)".to_string(),
                parameters: vec![
                    ParameterSpec {
                        name: "k_period".to_string(),
                        min: dec!(1),
                        max: dec!(100),
                        step: dec!(1),
                        default: dec!(14),
                        description: "%K lookback period".to_string(),
                    },
                    ParameterSpec {
                        name: "d_period".to_string(),
                        min: dec!(1),
                        max: dec!(50),
                        step: dec!(1),
                        default: dec!(3),
                        description: "%D moving average smoothing period".to_string(),
                    },
                    ParameterSpec {
                        name: "slowing".to_string(),
                        min: dec!(1),
                        max: dec!(50),
                        step: dec!(1),
                        default: dec!(3),
                        description: "Internal slowing factor".to_string(),
                    },
                ],
            },
        );

        catalog.insert(
            "ADX".to_string(),
            IndicatorSpec {
                name: "ADX".to_string(),
                category: IndicatorCategory::Oscillator,
                description: "Average Directional Index measuring trend strength (5-50)"
                    .to_string(),
                parameters: vec![ParameterSpec {
                    name: "length".to_string(),
                    min: dec!(5),
                    max: dec!(50),
                    step: dec!(1),
                    default: dec!(14),
                    description: "ADX directional smoothing period".to_string(),
                }],
            },
        );

        // 3. Indian Microstructure: CPR (Pivot, TC, BC), VWAP + Standard Deviation Bands
        catalog.insert(
            "CPR".to_string(),
            IndicatorSpec {
                name: "CPR".to_string(),
                category: IndicatorCategory::IndianMicrostructure,
                description: "Central Pivot Range (Pivot, Top Central, Bottom Central) for intraday Indian equities".to_string(),
                parameters: vec![],
            },
        );

        catalog.insert(
            "VWAP".to_string(),
            IndicatorSpec {
                name: "VWAP".to_string(),
                category: IndicatorCategory::IndianMicrostructure,
                description: "Volume Weighted Average Price with intraday standard deviation bands"
                    .to_string(),
                parameters: vec![ParameterSpec {
                    name: "band_multiplier".to_string(),
                    min: dec!(0.5),
                    max: dec!(5.0),
                    step: dec!(0.5),
                    default: dec!(1.0),
                    description: "Standard deviation band distance multiplier".to_string(),
                }],
            },
        );

        // 4. Volatility & Trend: ATR (1–200), Supertrend (period 5–50, multiplier 1.0–5.0), Bollinger Bands
        catalog.insert(
            "ATR".to_string(),
            IndicatorSpec {
                name: "ATR".to_string(),
                category: IndicatorCategory::VolatilityTrend,
                description: "Average True Range measuring market volatility".to_string(),
                parameters: vec![ParameterSpec {
                    name: "period".to_string(),
                    min: dec!(1),
                    max: dec!(200),
                    step: dec!(1),
                    default: dec!(14),
                    description: "True range smoothing period".to_string(),
                }],
            },
        );

        catalog.insert(
            "SUPERTREND".to_string(),
            IndicatorSpec {
                name: "SUPERTREND".to_string(),
                category: IndicatorCategory::VolatilityTrend,
                description: "Supertrend indicator based on ATR and median price".to_string(),
                parameters: vec![
                    ParameterSpec {
                        name: "period".to_string(),
                        min: dec!(5),
                        max: dec!(50),
                        step: dec!(1),
                        default: dec!(10),
                        description: "ATR period".to_string(),
                    },
                    ParameterSpec {
                        name: "multiplier".to_string(),
                        min: dec!(1.0),
                        max: dec!(5.0),
                        step: dec!(0.1),
                        default: dec!(3.0),
                        description: "ATR band multiplier".to_string(),
                    },
                ],
            },
        );

        catalog.insert(
            "BOLLINGER_BANDS".to_string(),
            IndicatorSpec {
                name: "BOLLINGER_BANDS".to_string(),
                category: IndicatorCategory::VolatilityTrend,
                description: "Bollinger Bands volatility envelope (middle, upper, lower bands)"
                    .to_string(),
                parameters: vec![
                    ParameterSpec {
                        name: "length".to_string(),
                        min: dec!(2),
                        max: dec!(200),
                        step: dec!(1),
                        default: dec!(20),
                        description: "Moving average window length".to_string(),
                    },
                    ParameterSpec {
                        name: "std_dev".to_string(),
                        min: dec!(1.0),
                        max: dec!(5.0),
                        step: dec!(0.1),
                        default: dec!(2.0),
                        description: "Standard deviation multiplier".to_string(),
                    },
                ],
            },
        );

        Self {
            indicators: catalog,
        }
    }

    /// Retrieve an approved indicator specification by name (case-insensitive).
    pub fn get(&self, name: &str) -> Option<&IndicatorSpec> {
        self.indicators.get(&name.to_uppercase())
    }

    /// Check if an indicator name is approved.
    pub fn is_approved(&self, name: &str) -> bool {
        self.indicators.contains_key(&name.to_uppercase())
    }

    /// List all approved indicators.
    pub fn list(&self) -> Vec<&IndicatorSpec> {
        self.indicators.values().collect()
    }

    /// Validate an indicator invocation name and parameters against catalog mathematical rules.
    pub fn validate_indicator(
        &self,
        name: &str,
        params: &HashMap<String, Decimal>,
    ) -> Result<(), CatalogError> {
        let upper = name.to_uppercase();
        let spec = self
            .indicators
            .get(&upper)
            .ok_or_else(|| CatalogError::UnapprovedIndicator(name.to_string()))?;

        // Check parameter bounds
        for param_spec in &spec.parameters {
            if let Some(val) = params.get(&param_spec.name) {
                if *val < param_spec.min || *val > param_spec.max {
                    return Err(CatalogError::ParameterOutOfBounds {
                        indicator: upper.clone(),
                        parameter: param_spec.name.clone(),
                        value: *val,
                        min: param_spec.min,
                        max: param_spec.max,
                    });
                }
            }
        }

        // Specific cross-parameter constraints
        if upper == "MACD" {
            let fast = params.get("fast_length").copied().unwrap_or(dec!(12));
            let slow = params.get("slow_length").copied().unwrap_or(dec!(26));
            if fast >= slow {
                return Err(CatalogError::InvalidParameterCombination {
                    indicator: "MACD".to_string(),
                    reason: format!(
                        "fast_length ({fast}) must be strictly less than slow_length ({slow})"
                    ),
                });
            }
        }

        Ok(())
    }

    /// Validate all indicators in a strategy specification.
    pub fn validate_spec(&self, spec: &StrategySpec) -> Result<(), CatalogError> {
        if spec.name.trim().is_empty() {
            return Err(CatalogError::SpecValidationFailed(
                "Strategy name cannot be empty".to_string(),
            ));
        }

        for ind in &spec.indicators {
            self.validate_indicator(&ind.name, &ind.params)?;
        }

        Ok(())
    }
}

/// Invocation of a specific indicator within a strategy specification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct IndicatorInvocation {
    /// Indicator name (e.g. "SMA", "RSI", "MACD")
    pub name: String,
    /// Parameter values (e.g. "length" -> 14)
    #[serde(default)]
    pub params: HashMap<String, Decimal>,
}

impl IndicatorInvocation {
    /// Helper to create a new indicator invocation.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            params: HashMap::new(),
        }
    }

    /// Add a parameter to the invocation.
    pub fn with_param(mut self, key: impl Into<String>, value: Decimal) -> Self {
        self.params.insert(key.into(), value);
        self
    }
}

/// Strategy specification holding declarative indicators and parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StrategySpec {
    /// Strategy identifier name
    pub name: String,
    /// Strategy version string
    #[serde(default = "default_version")]
    pub version: String,
    /// Declared indicator dependencies
    #[serde(default)]
    pub indicators: Vec<IndicatorInvocation>,
    /// Additional JSON metadata
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

impl StrategySpec {
    /// Create a new strategy specification.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: default_version(),
            indicators: Vec::new(),
            metadata: None,
        }
    }

    /// Add an indicator invocation.
    pub fn with_indicator(mut self, indicator: IndicatorInvocation) -> Self {
        self.indicators.push(indicator);
        self
    }
}

/// Standalone function to validate strategy specification indicators against the global catalog.
pub fn validate_spec_indicators(spec: &StrategySpec) -> Result<(), CatalogError> {
    let catalog = IndicatorCatalog::new();
    catalog.validate_spec(spec)
}
