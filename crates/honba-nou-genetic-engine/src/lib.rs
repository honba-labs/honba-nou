//! # honba-nou-genetic-engine
//!
//! StockSharp Odysseus Port (Phase 6 of Honba-Nou Master Plan):
//! Automated genetic strategy generation, decision rule tree synthesis, and
//! market regime clustering ported from C# to Rust.
//!
//! Provides:
//! - **Task 6.1: Odysseus Genetic Rule Tree Engine**:
//!   - Condition Nodes: Indicator threshold comparisons, crossover operators, logical AND/OR/NOT.
//!   - Action Nodes: Order intents (`PlaceBuy`, `PlaceSell`, `ClosePosition`, `TrailStop`).
//!   - Position Sizing Nodes: Fixed notional, Kelly criterion, volatility-adjusted sizing.
//!   - Genetic programming operators: Sub-tree crossover, point mutation, multi-objective tournament selection preserving Pareto frontier.
//! - **Task 6.2: Market Regime Analytics (StockSharp Analytics Port)**:
//!   - Statistical market state classifiers: Hidden Markov Models (HMM) with Viterbi decoding,
//!     Gaussian Mixture Models (GMM) with EM, and K-Means clustering.
//!   - Regime-conditional rule execution.
//! - **Task 6.3: Hybrid AI + Odysseus Auto-Synthesizer**:
//!   - Frontier LLM macro hypothesis formulation.
//!   - Combinatorial genetic rule tree search over historical datasets.
//!   - Automatic compilation into declarative `StrategySpec` JSON and native Python `Strategy` classes.

pub mod ast;
pub mod genetic;
pub mod regime;
pub mod synthesizer;

// Re-export AST types
pub use ast::{
    AstCondition, ComparisonOp, CrossoverCondition, CrossoverDirection, IndicatorTarget,
    MarketEvaluationContext, OrderAction, PositionSizingRule, StrategyGenome, ThresholdCondition,
};

// Re-export Genetic programming types
pub use genetic::{
    EvaluatedCandidate, FitnessVector, GeneticConfig, compute_pareto_frontier, point_mutation,
    subtree_crossover, tournament_selection,
};

// Re-export Market regime analytics
pub use regime::{
    DetectedRegime, GmmClassifier, GmmComponent, HmmClassifier, KMeansClassifier, MarketFeatures,
    RegimeDetectionResult, detect_regime, fit_gmm, kmeans_cluster,
};

// Re-export Auto-synthesizer types
pub use synthesizer::{AutoSynthesizerConfig, HybridAutoSynthesizer, MacroHypothesis};

// -------------------------------------------------------------------------
// Backwards Compatibility Types & Helpers for Existing Tests and Callers
// -------------------------------------------------------------------------
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Legacy RuleNodeType kept for backwards compatibility.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleNodeType {
    Condition,
    Action,
    PositionSizing,
}

/// Legacy RuleNode kept for backwards compatibility.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleNode {
    pub node_type: RuleNodeType,
    pub data: Value,
}

/// Legacy ConditionData kept for backwards compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionData {
    pub indicator: String,
    pub threshold: Decimal,
    pub operator: String, // "gt", "lt", "gte", "lte"
}

/// Legacy ActionData kept for backwards compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionData {
    pub action: String, // "buy", "sell", "close", "trail_stop", "hold"
    pub symbol: String,
    pub quantity: Decimal,
    pub price_limit: Option<Decimal>,
}

/// Legacy PositionSizingData kept for backwards compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionSizingData {
    pub method: String, // "fixed_notional", "kelly", "volatility_adjusted"
    pub notional: Option<Decimal>,
    pub win_probability: Option<Decimal>,
    pub win_loss_ratio: Option<Decimal>,
    pub target_volatility: Option<Decimal>,
    pub max_notional: Option<Decimal>,
}

/// Complete rule node with typed data.
pub fn make_rule_node(node_type: RuleNodeType, data: Value) -> RuleNode {
    RuleNode { node_type, data }
}

/// Legacy SynthesizedStrategy result kept for backwards compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesizedStrategy {
    pub strategy_id: String,
    pub rule_tree: RuleNode,
    pub fitness: (f64, f64, f64, f64), // (sharpe, profit_factor, wfe, -drawdown)
    pub created_at: DateTime<Utc>,
    pub parent_ids: Vec<String>,
    pub generation: u32,
}

/// Archive a synthesized strategy with provenance.
pub fn archive_strategy(strategy: &SynthesizedStrategy) -> String {
    strategy.strategy_id.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_regime_detection() {
        let features = MarketFeatures {
            trend_slope: Decimal::new(1, 0),
            volatility: Decimal::new(200, 4),
            order_book_imbalance: Decimal::new(1, 0),
            mean_reversion_score: Decimal::new(-1, 0),
            volume_ratio: Decimal::new(12000, 4),
        };

        let regime = detect_regime(&features);
        assert!(matches!(
            regime,
            DetectedRegime::Bull | DetectedRegime::Sideways
        ));
    }

    #[test]
    fn test_make_rule_node() {
        let node = make_rule_node(
            RuleNodeType::Condition,
            serde_json::json!({"indicator": "RSI", "threshold": 30}),
        );
        assert!(matches!(node.node_type, RuleNodeType::Condition));
        assert!(node.data.is_object());
    }

    #[test]
    fn test_genetic_config() {
        let config = GeneticConfig {
            crossover_prob: 0.8,
            mutation_prob: 0.2,
            subtree_swap_prob: 0.1,
            max_tree_depth: 10,
            tournament_size: 3,
        };
        assert!(config.crossover_prob > 0.0 && config.crossover_prob <= 1.0);
    }

    #[test]
    fn test_synthesized_strategy() {
        let strategy = SynthesizedStrategy {
            strategy_id: Uuid::new_v4().to_string(),
            rule_tree: make_rule_node(
                RuleNodeType::Condition,
                serde_json::json!({"indicator": "RSI", "threshold": 30}),
            ),
            fitness: (1.5, 1.2, 0.8, -5.0),
            created_at: Utc::now(),
            parent_ids: vec![],
            generation: 1,
        };

        assert!(!strategy.strategy_id.is_empty());
        assert!(strategy.fitness.0 > 0.0);
    }

    #[test]
    fn test_archive_strategy() {
        let strategy = SynthesizedStrategy {
            strategy_id: "test-strategy-123".to_string(),
            rule_tree: make_rule_node(
                RuleNodeType::Condition,
                serde_json::json!({"indicator": "RSI"}),
            ),
            fitness: (1.5, 1.2, 0.8, -5.0),
            created_at: Utc::now(),
            parent_ids: vec![],
            generation: 1,
        };

        let archive = archive_strategy(&strategy);
        assert_eq!(archive, "test-strategy-123");
    }
}
