//! StockSharp Odysseus Port: Abstract Syntax Tree (AST) & Strategy Genome
//!
//! Provides condition nodes, action nodes, position sizing nodes, and rule tree
//! evaluation for automated strategy synthesis.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::regime::DetectedRegime;

/// Comparison operators for condition threshold evaluations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonOp {
    Gt,
    Lt,
    Gte,
    Lte,
    Eq,
}

impl ComparisonOp {
    pub fn evaluate(&self, left: Decimal, right: Decimal) -> bool {
        match self {
            Self::Gt => left > right,
            Self::Lt => left < right,
            Self::Gte => left >= right,
            Self::Lte => left <= right,
            Self::Eq => left == right,
        }
    }

    pub fn invert(&self) -> Self {
        match self {
            Self::Gt => Self::Lt,
            Self::Lt => Self::Gt,
            Self::Gte => Self::Lte,
            Self::Lte => Self::Gte,
            Self::Eq => Self::Eq,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Gt => ">",
            Self::Lt => "<",
            Self::Gte => ">=",
            Self::Lte => "<=",
            Self::Eq => "==",
        }
    }
}

/// Direction for crossover operators (e.g. Fast EMA crosses above Slow EMA).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossoverDirection {
    CrossesAbove,
    CrossesBelow,
}

/// Comparison target in threshold conditions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndicatorTarget {
    Constant(Decimal),
    PriceField(String), // "close", "open", "high", "low", "vwap"
    Indicator { name: String, param: Decimal },
}

/// Threshold comparison condition (e.g. `RSI(14) < 30`, `Close > EMA(50)`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThresholdCondition {
    pub indicator: String,
    pub param: Decimal,
    pub op: ComparisonOp,
    pub target: IndicatorTarget,
}

/// Crossover condition between two indicators (e.g. `EMA(12)` crosses above `EMA(26)`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrossoverCondition {
    pub fast_indicator: String,
    pub fast_param: Decimal,
    pub slow_indicator: String,
    pub slow_param: Decimal,
    pub direction: CrossoverDirection,
}

/// Abstract Syntax Tree (AST) condition node supporting logical branching.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AstCondition {
    Threshold(ThresholdCondition),
    Crossover(CrossoverCondition),
    And(Vec<AstCondition>),
    Or(Vec<AstCondition>),
    Not(Box<AstCondition>),
    Regime(DetectedRegime),
    AlwaysTrue,
}

impl AstCondition {
    /// Evaluate condition against current market context.
    pub fn evaluate(&self, ctx: &MarketEvaluationContext) -> bool {
        match self {
            Self::Threshold(t) => {
                let left_val = match ctx.get_indicator(&t.indicator, t.param) {
                    Some(v) => v,
                    None => return false,
                };
                let right_val = match &t.target {
                    IndicatorTarget::Constant(c) => *c,
                    IndicatorTarget::PriceField(field) => match ctx.get_price(field) {
                        Some(p) => p,
                        None => return false,
                    },
                    IndicatorTarget::Indicator { name, param } => {
                        match ctx.get_indicator(name, *param) {
                            Some(v) => v,
                            None => return false,
                        }
                    }
                };
                t.op.evaluate(left_val, right_val)
            }
            Self::Crossover(c) => {
                let (fast_prev, fast_curr) =
                    match ctx.get_indicator_series(&c.fast_indicator, c.fast_param) {
                        Some((p, curr)) => (p, curr),
                        None => return false,
                    };
                let (slow_prev, slow_curr) =
                    match ctx.get_indicator_series(&c.slow_indicator, c.slow_param) {
                        Some((p, curr)) => (p, curr),
                        None => return false,
                    };
                match c.direction {
                    CrossoverDirection::CrossesAbove => {
                        fast_prev <= slow_prev && fast_curr > slow_curr
                    }
                    CrossoverDirection::CrossesBelow => {
                        fast_prev >= slow_prev && fast_curr < slow_curr
                    }
                }
            }
            Self::And(children) => {
                if children.is_empty() {
                    true
                } else {
                    children.iter().all(|c| c.evaluate(ctx))
                }
            }
            Self::Or(children) => {
                if children.is_empty() {
                    false
                } else {
                    children.iter().any(|c| c.evaluate(ctx))
                }
            }
            Self::Not(inner) => !inner.evaluate(ctx),
            Self::Regime(regime) => ctx.current_regime == *regime,
            Self::AlwaysTrue => true,
        }
    }

    /// Count total nodes in condition AST.
    pub fn node_count(&self) -> usize {
        match self {
            Self::Threshold(_) | Self::Crossover(_) | Self::Regime(_) | Self::AlwaysTrue => 1,
            Self::Not(inner) => 1 + inner.node_count(),
            Self::And(children) | Self::Or(children) => {
                1 + children.iter().map(|c| c.node_count()).sum::<usize>()
            }
        }
    }
}

/// Action nodes for order intents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderAction {
    PlaceBuy {
        symbol: String,
        limit_offset: Option<Decimal>,
    },
    PlaceSell {
        symbol: String,
        limit_offset: Option<Decimal>,
    },
    ClosePosition {
        symbol: String,
    },
    TrailStop {
        symbol: String,
        trailing_distance: Decimal,
        step: Decimal,
    },
    Hold,
}

/// Position sizing rules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PositionSizingRule {
    FixedNotional {
        amount: Decimal,
    },
    KellyCriterion {
        win_prob: Decimal,
        win_loss_ratio: Decimal,
        fractional: Decimal,
        max_notional: Decimal,
    },
    VolatilityAdjusted {
        target_vol: Decimal,
        current_vol: Decimal,
        max_notional: Decimal,
    },
}

impl PositionSizingRule {
    /// Compute recommended order quantity or notional based on rule.
    pub fn compute_notional(&self, base_equity: Decimal) -> Decimal {
        match self {
            Self::FixedNotional { amount } => *amount,
            Self::KellyCriterion {
                win_prob,
                win_loss_ratio,
                fractional,
                max_notional,
            } => {
                // Kelly % = p - (1-p)/b
                if *win_loss_ratio <= Decimal::ZERO {
                    return Decimal::ZERO;
                }
                let loss_prob = Decimal::ONE - *win_prob;
                let kelly = *win_prob - (loss_prob / *win_loss_ratio);
                let sizing = (kelly * *fractional * base_equity).max(Decimal::ZERO);
                sizing.min(*max_notional)
            }
            Self::VolatilityAdjusted {
                target_vol,
                current_vol,
                max_notional,
            } => {
                if *current_vol <= Decimal::ZERO {
                    *max_notional
                } else {
                    let scale = *target_vol / *current_vol;
                    (base_equity * scale).min(*max_notional)
                }
            }
        }
    }
}

/// Market evaluation context feeding bars and indicators into the AST evaluator.
#[derive(Debug, Clone, Default)]
pub struct MarketEvaluationContext {
    pub prices: HashMap<String, Decimal>,
    pub indicators: HashMap<(String, String), Decimal>, // (name, param_str) -> current_val
    pub indicator_history: HashMap<(String, String), (Decimal, Decimal)>, // (name, param_str) -> (prev, curr)
    pub current_regime: DetectedRegime,
}

impl MarketEvaluationContext {
    pub fn new(current_regime: DetectedRegime) -> Self {
        Self {
            prices: HashMap::new(),
            indicators: HashMap::new(),
            indicator_history: HashMap::new(),
            current_regime,
        }
    }

    pub fn set_price(&mut self, field: impl Into<String>, price: Decimal) {
        self.prices.insert(field.into().to_lowercase(), price);
    }

    pub fn get_price(&self, field: &str) -> Option<Decimal> {
        self.prices.get(&field.to_lowercase()).copied()
    }

    pub fn set_indicator(&mut self, name: &str, param: Decimal, val: Decimal) {
        let key = (name.to_uppercase(), param.to_string());
        self.indicators.insert(key, val);
    }

    pub fn get_indicator(&self, name: &str, param: Decimal) -> Option<Decimal> {
        let key = (name.to_uppercase(), param.to_string());
        self.indicators.get(&key).copied()
    }

    pub fn set_indicator_series(
        &mut self,
        name: &str,
        param: Decimal,
        prev: Decimal,
        curr: Decimal,
    ) {
        let key = (name.to_uppercase(), param.to_string());
        self.indicator_history.insert(key.clone(), (prev, curr));
        self.indicators.insert(key, curr);
    }

    pub fn get_indicator_series(&self, name: &str, param: Decimal) -> Option<(Decimal, Decimal)> {
        let key = (name.to_uppercase(), param.to_string());
        self.indicator_history.get(&key).copied()
    }
}

/// Strategy Genome representing a candidate rule tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StrategyGenome {
    pub id: String,
    pub name: String,
    pub entry_condition: AstCondition,
    pub exit_condition: AstCondition,
    pub entry_action: OrderAction,
    pub exit_action: OrderAction,
    pub position_sizing: PositionSizingRule,
    pub regime_filter: Option<DetectedRegime>,
    pub generation: u32,
    pub parent_ids: Vec<String>,
}

impl StrategyGenome {
    /// Evaluate genome against market context and return triggered action and notional.
    pub fn evaluate(
        &self,
        ctx: &MarketEvaluationContext,
        equity: Decimal,
    ) -> Option<(OrderAction, Decimal)> {
        // If regime filter is active, check whether current regime matches
        if let Some(target_regime) = &self.regime_filter {
            if ctx.current_regime != *target_regime {
                return None;
            }
        }

        // Check entry condition
        if self.entry_condition.evaluate(ctx) {
            let notional = self.position_sizing.compute_notional(equity);
            return Some((self.entry_action.clone(), notional));
        }

        // Check exit condition
        if self.exit_condition.evaluate(ctx) {
            return Some((self.exit_action.clone(), Decimal::ZERO));
        }

        None
    }
}
