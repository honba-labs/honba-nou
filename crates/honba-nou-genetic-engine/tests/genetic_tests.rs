//! Integration Tests for Phase 6: StockSharp Odysseus Port & Genetic Strategy Synthesizer.

use rand::SeedableRng;
use rand::rngs::StdRng;
use rust_decimal_macros::dec;

use honba_nou_core::indicators::{IndicatorCatalog, validate_spec_indicators};
use honba_nou_genetic_engine::ast::{
    AstCondition, ComparisonOp, CrossoverCondition, CrossoverDirection, IndicatorTarget,
    MarketEvaluationContext, OrderAction, PositionSizingRule, StrategyGenome, ThresholdCondition,
};
use honba_nou_genetic_engine::genetic::{
    EvaluatedCandidate, FitnessVector, compute_pareto_frontier, point_mutation, subtree_crossover,
    tournament_selection,
};
use honba_nou_genetic_engine::regime::{
    DetectedRegime, GmmClassifier, HmmClassifier, KMeansClassifier,
};
use honba_nou_genetic_engine::synthesizer::{
    AutoSynthesizerConfig, HybridAutoSynthesizer, MacroHypothesis,
};

#[test]
fn test_task_6_1_odysseus_ast_and_evaluation() {
    // 1. Condition Nodes: RSI < 30 AND Close > EMA(50)
    let condition = AstCondition::And(vec![
        AstCondition::Threshold(ThresholdCondition {
            indicator: "RSI".to_string(),
            param: dec!(14),
            op: ComparisonOp::Lt,
            target: IndicatorTarget::Constant(dec!(30)),
        }),
        AstCondition::Threshold(ThresholdCondition {
            indicator: "EMA".to_string(),
            param: dec!(50),
            op: ComparisonOp::Lt,
            target: IndicatorTarget::PriceField("close".to_string()),
        }),
    ]);

    // 2. Action Node & Position Sizing Node
    let genome = StrategyGenome {
        id: "GENOME_TEST_1".to_string(),
        name: "ODYSSEUS_RSI_EMA_TREND".to_string(),
        entry_condition: condition,
        exit_condition: AstCondition::Threshold(ThresholdCondition {
            indicator: "RSI".to_string(),
            param: dec!(14),
            op: ComparisonOp::Gt,
            target: IndicatorTarget::Constant(dec!(70)),
        }),
        entry_action: OrderAction::PlaceBuy {
            symbol: "INFY.NSE".to_string(),
            limit_offset: Some(dec!(0.50)),
        },
        exit_action: OrderAction::ClosePosition {
            symbol: "INFY.NSE".to_string(),
        },
        position_sizing: PositionSizingRule::KellyCriterion {
            win_prob: dec!(0.60),
            win_loss_ratio: dec!(2.0),
            fractional: dec!(0.50), // Half-Kelly
            max_notional: dec!(200000.0),
        },
        regime_filter: Some(DetectedRegime::Bull),
        generation: 0,
        parent_ids: Vec::new(),
    };

    // 3. Test Evaluation against MarketContext:
    let mut ctx = MarketEvaluationContext::new(DetectedRegime::Bull);
    ctx.set_price("close", dec!(1520.0));
    ctx.set_indicator("RSI", dec!(14), dec!(25.0)); // RSI is 25 (< 30) -> matches
    ctx.set_indicator("EMA", dec!(50), dec!(1500.0)); // Close (1520) > EMA(50) (1500) -> matches

    let eval_res = genome.evaluate(&ctx, dec!(500000.0));
    assert!(eval_res.is_some());
    let (action, notional) = eval_res.unwrap();
    assert!(matches!(action, OrderAction::PlaceBuy { .. }));
    // Kelly sizing calculation check: p=0.6, q=0.4, b=2.0 -> kelly = 0.6 - (0.4/2.0) = 0.40
    // Half-Kelly: 0.40 * 0.50 = 0.20 -> 0.20 * 500,000 = 100,000
    assert_eq!(notional, dec!(100000.0));

    // 4. Test Crossover condition operator
    let crossover_cond = AstCondition::Crossover(CrossoverCondition {
        fast_indicator: "EMA".to_string(),
        fast_param: dec!(12),
        slow_indicator: "EMA".to_string(),
        slow_param: dec!(26),
        direction: CrossoverDirection::CrossesAbove,
    });
    ctx.set_indicator_series("EMA", dec!(12), dec!(99.0), dec!(102.0));
    ctx.set_indicator_series("EMA", dec!(26), dec!(100.0), dec!(101.0));
    assert!(crossover_cond.evaluate(&ctx));
}

#[test]
fn test_task_6_1_genetic_operators_and_pareto_frontier() {
    let mut rng = StdRng::seed_from_u64(42);
    let catalog = IndicatorCatalog::new();

    let parent_a = StrategyGenome {
        id: "PARENT_A".to_string(),
        name: "STRAT_A".to_string(),
        entry_condition: AstCondition::Threshold(ThresholdCondition {
            indicator: "RSI".to_string(),
            param: dec!(14),
            op: ComparisonOp::Lt,
            target: IndicatorTarget::Constant(dec!(30)),
        }),
        exit_condition: AstCondition::Threshold(ThresholdCondition {
            indicator: "RSI".to_string(),
            param: dec!(14),
            op: ComparisonOp::Gt,
            target: IndicatorTarget::Constant(dec!(70)),
        }),
        entry_action: OrderAction::PlaceBuy {
            symbol: "TCS.NSE".to_string(),
            limit_offset: None,
        },
        exit_action: OrderAction::ClosePosition {
            symbol: "TCS.NSE".to_string(),
        },
        position_sizing: PositionSizingRule::FixedNotional {
            amount: dec!(100000.0),
        },
        regime_filter: None,
        generation: 0,
        parent_ids: Vec::new(),
    };

    let parent_b = StrategyGenome {
        id: "PARENT_B".to_string(),
        name: "STRAT_B".to_string(),
        entry_condition: AstCondition::Threshold(ThresholdCondition {
            indicator: "ADX".to_string(),
            param: dec!(14),
            op: ComparisonOp::Gt,
            target: IndicatorTarget::Constant(dec!(25)),
        }),
        exit_condition: AstCondition::Threshold(ThresholdCondition {
            indicator: "ADX".to_string(),
            param: dec!(14),
            op: ComparisonOp::Lt,
            target: IndicatorTarget::Constant(dec!(20)),
        }),
        entry_action: OrderAction::PlaceBuy {
            symbol: "TCS.NSE".to_string(),
            limit_offset: None,
        },
        exit_action: OrderAction::ClosePosition {
            symbol: "TCS.NSE".to_string(),
        },
        position_sizing: PositionSizingRule::VolatilityAdjusted {
            target_vol: dec!(0.02),
            current_vol: dec!(0.015),
            max_notional: dec!(150000.0),
        },
        regime_filter: Some(DetectedRegime::Bull),
        generation: 0,
        parent_ids: Vec::new(),
    };

    // 1. Subtree crossover test
    let (mut child_a, child_b) = subtree_crossover(&parent_a, &parent_b, &mut rng);
    assert_ne!(child_a.id, parent_a.id);
    assert_eq!(child_a.generation, 1);
    assert_eq!(child_b.generation, 1);
    assert_eq!(child_a.parent_ids.len(), 2);

    // 2. Point mutation test
    point_mutation(&mut child_a, &catalog, &mut rng);
    assert_eq!(child_a.generation, 1);

    // 3. Multi-objective Pareto frontier test
    let c1 = EvaluatedCandidate {
        genome: parent_a.clone(),
        fitness: FitnessVector::new(2.1, 1.8, 5.0, 0.75), // Strong candidate
        rank: 0,
    };
    let c2 = EvaluatedCandidate {
        genome: parent_b.clone(),
        fitness: FitnessVector::new(1.5, 1.2, 8.0, 0.62), // Dominated by c1 in all dimensions
        rank: 0,
    };
    let c3 = EvaluatedCandidate {
        genome: child_a.clone(),
        fitness: FitnessVector::new(1.9, 2.0, 4.0, 0.80), // Non-dominated (higher profit factor & lower drawdown)
        rank: 0,
    };

    let candidates = vec![c1.clone(), c2.clone(), c3.clone()];
    let frontier = compute_pareto_frontier(&candidates);

    // c1 and c3 must be in the Pareto frontier, while c2 must be excluded (dominated by c1 and c3)
    assert_eq!(frontier.len(), 2);
    assert!(frontier.iter().any(|c| c.genome.id == "PARENT_A"));
    assert!(frontier.iter().any(|c| c.genome.id == child_a.id));
    assert!(!frontier.iter().any(|c| c.genome.id == "PARENT_B"));

    // 4. Tournament selection test
    let selected = tournament_selection(&candidates, 2, &mut rng);
    assert!(selected.fitness.sharpe >= 1.5);
}

#[test]
fn test_task_6_2_market_regime_analytics_hmm_gmm_kmeans() {
    // 1. Hidden Markov Model (HMM) Viterbi Decoding:
    let hmm = HmmClassifier::standard_three_state();

    // Sequence of observations:
    // First 4 bars positive returns -> Bull
    // Next 4 bars negative returns -> Bear
    // Next 4 bars near zero -> Sideways
    let obs = vec![
        0.015, 0.020, 0.018, 0.016, // Bull
        -0.018, -0.022, -0.020, -0.015, // Bear
        0.0001, -0.0002, 0.0001, 0.0000, // Sideways
    ];

    let decoded_path = hmm.viterbi(&obs);
    assert_eq!(decoded_path.len(), 12);
    assert_eq!(decoded_path[0], DetectedRegime::Bull);
    assert_eq!(decoded_path[1], DetectedRegime::Bull);
    assert_eq!(decoded_path[4], DetectedRegime::Bear);
    assert_eq!(decoded_path[5], DetectedRegime::Bear);
    assert_eq!(decoded_path[10], DetectedRegime::Sideways);

    // Predict current regime
    let current_regime = hmm.predict_current_regime(&obs);
    assert_eq!(current_regime, DetectedRegime::Sideways);

    // 2. Gaussian Mixture Model (GMM) with EM clustering:
    let returns_sample = vec![
        -0.03, -0.025, -0.02, -0.015, // Bearish cluster
        0.000, 0.001, -0.001, 0.002, // Neutral cluster
        0.02, 0.025, 0.03, 0.035, // Bullish cluster
    ];
    let gmm = GmmClassifier::fit(&returns_sample, 3, 20);
    assert_eq!(gmm.components.len(), 3);
    let pred_bull = gmm.predict(0.028);
    assert!(pred_bull.confidence > 0.33);

    // 3. K-Means clustering on multi-dimensional feature space
    let feature_data = vec![
        vec![1.5, 0.01, 0.6], // Bull trend, low vol, buy pressure
        vec![1.8, 0.012, 0.7],
        vec![-1.5, 0.03, -0.6], // Bear trend, high vol, sell pressure
        vec![-1.8, 0.035, -0.7],
        vec![0.0, 0.005, 0.0], // Sideways flat
        vec![0.1, 0.006, 0.05],
    ];
    let kmeans = KMeansClassifier::fit(&feature_data, 3, 15);
    assert_eq!(kmeans.n_clusters, 3);
    let cluster_regime = kmeans.predict(&[1.6, 0.011, 0.65]);
    assert_ne!(cluster_regime, DetectedRegime::Unknown);

    // 4. Regime-conditional rule execution
    let mut ctx_bull = MarketEvaluationContext::new(DetectedRegime::Bull);
    ctx_bull.set_price("close", dec!(100.0));
    ctx_bull.set_indicator("ADX", dec!(14), dec!(30.0));

    let regime_filtered_genome = StrategyGenome {
        id: "REGIME_STRAT".to_string(),
        name: "TREND_BULL_ONLY".to_string(),
        entry_condition: AstCondition::AlwaysTrue,
        exit_condition: AstCondition::AlwaysTrue,
        entry_action: OrderAction::PlaceBuy {
            symbol: "SBIN.NSE".to_string(),
            limit_offset: None,
        },
        exit_action: OrderAction::ClosePosition {
            symbol: "SBIN.NSE".to_string(),
        },
        position_sizing: PositionSizingRule::FixedNotional {
            amount: dec!(50000.0),
        },
        regime_filter: Some(DetectedRegime::Bull),
        generation: 0,
        parent_ids: Vec::new(),
    };

    // In Bull regime: executes
    assert!(
        regime_filtered_genome
            .evaluate(&ctx_bull, dec!(100000.0))
            .is_some()
    );

    // In Bear regime: does NOT execute!
    let ctx_bear = MarketEvaluationContext::new(DetectedRegime::Bear);
    assert!(
        regime_filtered_genome
            .evaluate(&ctx_bear, dec!(100000.0))
            .is_none()
    );
}

#[test]
fn test_task_6_3_hybrid_ai_odysseus_auto_synthesizer() {
    // 1. Frontier LLM formulates macro hypothesis
    let macro_hyp = MacroHypothesis {
        hypothesis_id: "HYP_MACRO_TREND_01".to_string(),
        target_regime: DetectedRegime::Bull,
        thesis: "Bullish macro expansion in IT sector; momentum breakout with trend filter"
            .to_string(),
        candidate_indicator_families: vec![
            "EMA".to_string(),
            "ADX".to_string(),
            "SUPERTREND".to_string(),
        ],
        target_universe: vec!["TCS.NSE".to_string(), "INFY.NSE".to_string()],
    };

    let config = AutoSynthesizerConfig {
        regime_aware: true,
        population_size: 10,
        generations: 3,
        crossover_prob: 0.85,
        mutation_prob: 0.25,
    };

    let synthesizer = HybridAutoSynthesizer::new(config);

    // 2. Synthesize Pareto candidate
    let best_candidate = synthesizer.synthesize(&macro_hyp, 12345);
    assert!(best_candidate.fitness.sharpe >= 1.20);
    assert!(best_candidate.fitness.walk_forward_efficiency >= 0.60);
    assert!(best_candidate.fitness.max_drawdown <= 8.0);

    // 3. Compile rule tree into declarative StrategySpec JSON
    let strategy_spec = synthesizer.compile_to_strategy_spec(&best_candidate.genome);
    assert!(!strategy_spec.name.is_empty());
    assert!(!strategy_spec.indicators.is_empty());
    assert_eq!(strategy_spec.version, "1.0.0-odysseus");

    // Must strictly pass global Indicator Catalog validation!
    validate_spec_indicators(&strategy_spec).expect("synthesized spec must adhere to catalog");

    // 4. Compile rule tree into native Python Strategy class
    let python_code = synthesizer.compile_to_python(&best_candidate.genome);
    assert!(python_code.contains("class SYNTH_"));
    assert!(python_code.contains("def on_bar(self, ctx):"));
    assert!(python_code.contains("ctx.get_price(\"close\")"));
    assert!(python_code.contains("ctx.get_regime()"));
    assert!(python_code.contains("self.position = 1"));
    assert!(python_code.contains("ctx.buy(quantity=100)"));
}
