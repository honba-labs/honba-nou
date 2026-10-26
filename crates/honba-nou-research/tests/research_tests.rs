//! Tests for Phase 4: Autonomous Quantitative Research Loop.

use chrono::{Duration, TimeZone, Utc};
use rust_decimal_macros::dec;

use honba_core::order::OrderSide;
use honba_nou_core::indicators::{IndicatorInvocation, StrategySpec};
use honba_nou_research::backtest::{
    IndianStatutoryCostCalculator, LakehouseCatalog, ParityBacktestRunner,
};
use honba_nou_research::enhancer::{analyze_trade_journal, suggest_indicator_filter};
use honba_nou_research::hypothesis::{HypothesisGenerator, MarketRegime};
use honba_nou_research::manifest::DatasetManifest;
use honba_nou_research::verification::{StatisticalVerifier, VerificationThresholds};
use serde_json::json;

#[test]
fn test_hypothesis_generation_for_all_regimes() {
    let generator = HypothesisGenerator::new();
    let universe = vec!["RELIANCE.NSE".to_string(), "TCS.NSE".to_string()];

    let regimes = [
        MarketRegime::TrendUp,
        MarketRegime::TrendDown,
        MarketRegime::MeanReversion,
        MarketRegime::VolatilityBreakout,
    ];

    for regime in regimes {
        let hyp = generator
            .generate_for_regime(&format!("STRAT_{regime}"), regime, universe.clone())
            .expect("should generate valid hypothesis");

        assert_eq!(hyp.regime, regime);
        assert!(!hyp.id.is_empty());
        assert!(!hyp.description.is_empty());
        assert!(!hyp.entry_rule.is_empty());
        assert!(!hyp.exit_rule.is_empty());
        assert!(!hyp.prompt_hash.is_empty());

        // Validate strategy spec indicators adhere to catalog
        generator
            .validate_spec(&hyp.strategy_spec)
            .expect("strategy spec must adhere to catalog");
    }
}

#[test]
fn test_hypothesis_spec_validation_rejects_hallucinations() {
    let generator = HypothesisGenerator::new();

    let mut invalid_spec = StrategySpec::new("HALLUCINATED_STRAT");
    invalid_spec = invalid_spec.with_indicator(
        IndicatorInvocation::new("NON_EXISTENT_INDICATOR").with_param("param1", dec!(42)),
    );

    let res = generator.validate_spec(&invalid_spec);
    assert!(res.is_err());
    let err = res.err().unwrap();
    assert!(err.contains("NON_EXISTENT_INDICATOR") || err.contains("Unapproved"));
}

#[test]
fn test_lakehouse_catalog_partitioning() {
    let start = Utc.with_ymd_and_hms(2025, 1, 1, 9, 15, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2025, 12, 31, 15, 30, 0).unwrap();
    let catalog = LakehouseCatalog::new(
        "/data/lakehouse/parquet",
        vec!["INFY.NSE".to_string()],
        start,
        end,
    );

    let (is_slice, oos_slice) = catalog.partition(0.70);

    assert_eq!(is_slice.name, "IN_SAMPLE_TRAIN");
    assert_eq!(oos_slice.name, "OUT_OF_SAMPLE_VALIDATION");
    assert_eq!(is_slice.start_time, start);
    assert_eq!(oos_slice.end_time, end);
    assert_eq!(is_slice.end_time, oos_slice.start_time);
    assert!(!is_slice.sha256_checksum.is_empty());
    assert!(!oos_slice.sha256_checksum.is_empty());
}

#[test]
fn test_indian_statutory_costs_exactness() {
    // 100 shares of Reliance at ₹2500 = ₹250,000 notional
    let qty = dec!(100.0);
    let price = dec!(2500.0);

    // BUY side delivery
    let buy_fees =
        IndianStatutoryCostCalculator::compute_cash_equity_fees(OrderSide::Buy, qty, price, true);

    // Brokerage is min(0.0003 * 250,000 = ₹75, ₹20) -> ₹20
    assert_eq!(buy_fees.brokerage.amount, dec!(20.0));
    // STT delivery buy 0.1% of 250,000 -> ₹250
    assert_eq!(buy_fees.stt.amount, dec!(250.0));
    // Exchange charges 0.00297% of 250,000 = 7.425 -> rounded half to even is ₹7.42
    assert_eq!(buy_fees.exchange_charges.amount, dec!(7.42));
    // Stamp duty 0.015% of 250,000 -> ₹37.50
    assert_eq!(buy_fees.stamp_duty.amount, dec!(37.50));
    // SEBI charges ₹10/crore -> ₹0.25
    assert_eq!(buy_fees.sebi_charges.amount, dec!(0.25));

    let total = buy_fees.total().amount;
    assert!(total > dec!(300.0));
    assert_eq!(
        buy_fees.brokerage.currency,
        honba_core::types::Currency::Inr
    );

    // SELL side delivery
    let sell_fees =
        IndianStatutoryCostCalculator::compute_cash_equity_fees(OrderSide::Sell, qty, price, true);
    // Stamp duty is 0 on sell
    assert_eq!(sell_fees.stamp_duty.amount, dec!(0.0));
    // STT is 0.1% on delivery sell
    assert_eq!(sell_fees.stt.amount, dec!(250.0));
}

#[test]
fn test_isolated_parity_backtest_execution() {
    let t1 = Utc.with_ymd_and_hms(2026, 1, 5, 9, 30, 0).unwrap();
    let t2 = Utc.with_ymd_and_hms(2026, 1, 6, 15, 0, 0).unwrap();

    let fills = vec![
        (
            t1,
            "TCS.NSE",
            OrderSide::Buy,
            dec!(50.0),
            dec!(3500.0),
            dec!(0.50),
        ),
        (
            t2,
            "TCS.NSE",
            OrderSide::Sell,
            dec!(50.0),
            dec!(3550.0),
            dec!(0.50),
        ),
    ];

    let outcome = ParityBacktestRunner::run_simulation(dec!(1000000.0), &fills);

    assert_eq!(outcome.total_trades, 2);
    assert!(outcome.total_statutory_fees > dec!(0.0));
    assert_eq!(outcome.trade_log.len(), 2);
    assert_eq!(outcome.equity_history.len(), 2);

    // Check T+1 settlement date
    assert_eq!(
        outcome.trade_log[0].settlement_date,
        t1.date_naive() + Duration::days(1)
    );
    assert_eq!(
        outcome.trade_log[1].settlement_date,
        t2.date_naive() + Duration::days(1)
    );
}

#[test]
fn test_statistical_verification_dsr_rejection() {
    let verifier = StatisticalVerifier::default();

    // Small or flat returns yielding low DSR
    let is_returns = vec![0.001, 0.002, -0.001, 0.000, 0.001, 0.002];
    let oos_returns = vec![0.000, -0.001, 0.001, -0.002, 0.000];

    // High number of trials (e.g. 500) severely deflates the observed Sharpe
    let report = verifier.verify_candidate(&is_returns, &oos_returns, 500);

    // Strict rejection test: DSR < 0.95 must reject candidate!
    assert!(!report.passed);
    assert!(report.deflated_sharpe_ratio < 0.95);
    assert!(
        report
            .rejection_reasons
            .iter()
            .any(|r| r.contains("Deflated Sharpe Ratio"))
    );
}

#[test]
fn test_statistical_verification_pbo_rejection() {
    let thresholds = VerificationThresholds {
        min_dsr: 0.50, // low min_dsr to isolate PBO check
        max_pbo: 0.10, // strict 10% maximum PBO
        min_wfe: 0.60,
        risk_free_rate: 0.06,
        periods_per_year: 252.0,
    };
    let verifier = StatisticalVerifier::new(thresholds);

    // Severely degraded out-of-sample returns
    let is_returns = vec![0.02, 0.03, 0.015, 0.025, 0.02];
    let oos_returns = vec![-0.01, 0.00, -0.02, 0.005, -0.01];

    let report = verifier.verify_candidate(&is_returns, &oos_returns, 100);

    assert!(!report.passed);
    assert!(report.probability_of_overfitting >= 0.10);
    assert!(
        report
            .rejection_reasons
            .iter()
            .any(|r| r.contains("Probability of Backtest Overfitting"))
    );
}

#[test]
fn test_statistical_verification_passing_candidate() {
    let thresholds = VerificationThresholds {
        min_dsr: 0.90,
        max_pbo: 0.15,
        min_wfe: 0.60,
        risk_free_rate: 0.06,
        periods_per_year: 252.0,
    };
    let verifier = StatisticalVerifier::new(thresholds);

    // Consistently strong positive returns with low variance and few trials
    let is_returns = vec![0.015, 0.018, 0.016, 0.020, 0.017, 0.019, 0.016, 0.018];
    let oos_returns = vec![0.014, 0.017, 0.015, 0.019, 0.016, 0.018, 0.015, 0.017];

    let report = verifier.verify_candidate(&is_returns, &oos_returns, 1);

    assert!(report.deflated_sharpe_ratio >= 0.90);
    assert!(report.probability_of_overfitting < 0.15);
    assert!(report.walk_forward_efficiency >= 0.60);
    assert!(report.passed);
    assert!(report.rejection_reasons.is_empty());
}

#[test]
fn test_dataset_manifest_cryptographic_provenance_and_tamper_detection() {
    let verifier = StatisticalVerifier::default();
    let is_returns = vec![0.01, 0.02, 0.015];
    let oos_returns = vec![0.01, 0.015, 0.012];
    let report = verifier.verify_candidate(&is_returns, &oos_returns, 1);

    let start_is = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    let end_is = Utc.with_ymd_and_hms(2025, 6, 30, 23, 59, 59).unwrap();
    let start_oos = Utc.with_ymd_and_hms(2025, 7, 1, 0, 0, 0).unwrap();
    let end_oos = Utc.with_ymd_and_hms(2025, 12, 31, 23, 59, 59).unwrap();

    let manifest = DatasetManifest::create(
        "parquet_catalog_manifest_sha256_mock_content",
        "{\"strategy\": \"TREND_V1\"}",
        42,
        start_is,
        end_is,
        start_oos,
        end_oos,
        report,
    );

    // 1. Initial signature must be valid
    assert!(manifest.verify_provenance());

    // 2. Tampering test: altering seed breaks signature
    let mut tampered_manifest = manifest.clone();
    tampered_manifest.seed = 999;
    assert!(!tampered_manifest.verify_provenance());

    // 3. Tampering test: altering spec hash breaks signature
    let mut tampered_manifest2 = manifest.clone();
    tampered_manifest2.strategy_spec_hash =
        "0000000000000000000000000000000000000000000000000000000000000000".to_string();
    assert!(!tampered_manifest2.verify_provenance());
}

#[test]
fn test_trade_journal_analysis_and_filter_suggestion() {
    let trade_log = json!({
        "n_trades": 10,
        "n_winners": 4,
        "n_losers": 6,
        "total_pnl": -15000,
        "mae_per_trade": [500.0, 800.0, 1200.0],
        "mfe_per_trade": [200.0, 400.0, 300.0]
    });

    let analysis = analyze_trade_journal(&trade_log).expect("analysis");
    assert_eq!(analysis.n_trades, 10);
    assert_eq!(analysis.n_winners, 4);
    assert_eq!(analysis.n_losers, 6);

    let current_spec = json!({
        "name": "CHOP_LOSING_STRAT",
        "indicators": [{ "name": "SMA" }]
    });

    let mutated = suggest_indicator_filter("sideways_chop", &current_spec);
    let filters = mutated["filters"].as_array().expect("filters array");
    assert_eq!(filters[0]["indicator"], "ADX");
    assert_eq!(filters[0]["operator"], "gt");
}
