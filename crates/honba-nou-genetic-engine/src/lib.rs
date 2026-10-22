//! # honba-nou-genetic-engine
//!
//! StockSharp Odysseus Port
//! Types and interfaces for genetic strategy synthesis, rule trees, and
//! market regime analytics (Phase 6 of honba-nou master plan).

use serde::{Serialize, Deserialize};
use serde_json::Value;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

// Market regimes detectable by statistical classifiers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectedRegime {
    Bull,
    Bear,
    Sideways,
    HighVolatility,
    LowVolatility,
    Unknown,
}

/// Market features for regime classification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketFeatures {
    pub trend_slope: Decimal,
    pub volatility: Decimal,
    pub order_book_imbalance: Decimal,
    pub mean_reversion_score: Decimal,
    pub volume_ratio: Decimal,
}

/// Detected regime result with confidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegimeDetectionResult {
    pub regime: DetectedRegime,
    pub confidence: f64,
}

/// Rule node types for the Odysseus genetic rule tree engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleNodeType {
    Condition,
    Action,
    PositionSizing,
}

/// Rule node in the genetic programming tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleNode {
    pub node_type: RuleNodeType,
    pub data: Value,
}

/// Condition data for indicator threshold comparisons.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionData {
    pub indicator: String,
    pub threshold: Decimal,
    pub operator: String, // "gt", "lt", "gte", "lte"
}

/// Action data for order execution intents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionData {
    pub action: String,   // "buy", "sell", "close", "trail_stop", "hold"
    pub symbol: String,
    pub quantity: Decimal,
    pub price_limit: Option<Decimal>,
}

/// Position sizing data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionSizingData {
    pub method: String,        // "fixed_notional", "kelly", "volatility_adjusted"
    pub notional: Option<Decimal>,
    pub win_probability: Option<Decimal>,
    pub win_loss_ratio: Option<Decimal>,
    pub target_volatility: Option<Decimal>,
    pub max_notional: Option<Decimal>,
}

/// Complete rule node with typed data.
pub fn make_rule_node(node_type: RuleNodeType, data: Value) -> RuleNode {
    RuleNode {
        node_type,
        data,
    }
}

/// Genetic operator configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneticConfig {
    pub crossover_prob: f64,
    pub mutation_prob: f64,
    pub subtree_swap_prob: f64,
    pub max_tree_depth: u32,
    pub tournament_size: u32,
}

/// Market regime detection (simplified).
pub fn detect_regime(_features: &MarketFeatures) -> DetectedRegime {
    let trending = _features.trend_slope > Decimal::new(0, 0);
    let high_vol = _features.volatility > Decimal::from(3) / Decimal::from(100);

    if trending && !high_vol {
        DetectedRegime::Bull
    } else if !trending && high_vol {
        DetectedRegime::HighVolatility
    } else if !trending && !high_vol {
        DetectedRegime::Sideways
    } else {
        DetectedRegime::Unknown
    }
}

/// Fit Gaussian Mixture Model (simplified interface).
pub fn fit_gmm(_features: &[MarketFeatures], _n_components: usize) -> Vec<Value> {
    (0.._n_components)
        .map(|_| {
            serde_json::json!({
                "weight": 1.0 / _n_components as f64,
                "mean": 0.0f64,
                "covariance": 0.1f64
            })
        })
        .collect()
}

/// Run KMeans clustering (simplified interface).
pub fn kmeans_cluster(
    _features: &[MarketFeatures],
    _n_clusters: usize,
    _n_iterations: u32,
) -> Value {
    let assignments: Vec<usize> = (0.._features.len()).map(|i| i % _n_clusters).collect();
    serde_json::json!({
        "cluster_assignments": assignments,
        "centroids": 0.5f64,
        "wcss": 0.25f64
    })
}

/// Auto-synthesizer configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoSynthesizerConfig {
    pub regime_aware: bool,
    pub population_size: usize,
    pub generations: u32,
    pub crossover_prob: f64,
    pub mutation_prob: f64,
}

/// Synthesized strategy result.
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
pub fn archive_strategy(_strategy: &SynthesizedStrategy) -> String {
    _strategy.strategy_id.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regime_detection() {
        let features = MarketFeatures {
            trend_slope: Decimal::new(1, 0),
            volatility: Decimal::new(0.02, 4),
            order_book_imbalance: Decimal::new(1, 0),
            mean_reversion_score: Decimal::new(-1, 0),
            volume_ratio: Decimal::new(1.2, 4),
        };

        let regime = detect_regime(&features);
        assert!(matches!(regime, DetectedRegime::Bull | DetectedRegime::Sideways));
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
            rule_node: make_rule_node(
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
            rule_node: make_rule_node(
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