//! Comprehensive unit test suite for honba-nou-core:
//! Decision parser, grounding checks, SEBI algo ID validation, and indicator catalog.

use chrono::Utc;
use rust_decimal_macros::dec;

use honba_core::{Decision, OrderSide, Price, PriceBand, Quantity};
use honba_mon::{OtrConfig, RiskEngine, RiskLimits};
use honba_nou_core::{
    CatalogError, DecisionError, DecisionParser, IndicatorCatalog, IndicatorInvocation,
    PromptTemplate, StrategySpec, compute_prompt_hash, hash_prompt_hex, is_valid_sebi_algo_id,
    validate_spec_indicators,
};

#[test]
fn test_closed_decision_parser_success_and_markdown() {
    let parser = DecisionParser::new().with_universe(vec!["RELIANCE", "TCS", "INFY"]);

    let json_text = r#"{
        "action": "place_order",
        "symbol": "RELIANCE",
        "side": "BUY",
        "qty": "50",
        "price_limit": "2450.50",
        "algo_id": "SEBI_ALGO_007",
        "rationale": "Breakout above 20-day high"
    }"#;

    let dec = parser
        .parse_str(json_text)
        .expect("Should parse valid JSON");
    match &dec {
        Decision::PlaceOrder {
            symbol,
            side,
            qty,
            price_limit,
            algo_id,
            ..
        } => {
            assert_eq!(symbol, "RELIANCE");
            assert_eq!(*side, OrderSide::Buy);
            assert_eq!(qty.0, dec!(50));
            assert_eq!(price_limit.unwrap().0, dec!(2450.50));
            assert_eq!(algo_id, "SEBI_ALGO_007");
        }
        _ => panic!("Expected PlaceOrder decision"),
    }

    // Markdown block wrapping
    let markdown_text = format!(
        "Here is my trade recommendation:\n```json\n{}\n```\nExecution details above.",
        json_text
    );
    let dec_md = parser
        .parse_str(&markdown_text)
        .expect("Should parse markdown fenced JSON");
    assert_eq!(dec, dec_md);
}

#[test]
fn test_closed_decision_rejects_unknown_fields() {
    let parser = DecisionParser::new();

    // LLM hallucinating extra fields
    let hallucinated_json = r#"{
        "action": "place_order",
        "symbol": "RELIANCE",
        "side": "BUY",
        "qty": "10",
        "algo_id": "ALGO_01",
        "unauthorized_leverage": "5x",
        "hallucinated_flag": true
    }"#;

    let res = parser.parse_str(hallucinated_json);
    assert!(res.is_err());
    match res.unwrap_err() {
        DecisionError::UnknownField(msg) | DecisionError::Json(msg) => {
            assert!(msg.contains("unknown field `unauthorized_leverage`"));
        }
        other => panic!("Expected UnknownField error, got: {:?}", other),
    }
}

#[test]
fn test_sebi_algo_id_validation() {
    let parser = DecisionParser::new().with_strict_sebi_algo_format(true);

    // Valid SEBI IDs: 4 to 32 chars, uppercase letters, digits, underscores
    assert!(is_valid_sebi_algo_id("ALGO_01"));
    assert!(is_valid_sebi_algo_id("MOMENTUM_ALPHA_V1"));
    assert!(is_valid_sebi_algo_id("SEBI1234"));

    // Invalid SEBI IDs
    assert!(!is_valid_sebi_algo_id("ABC")); // < 4 chars
    assert!(!is_valid_sebi_algo_id("momentum_alpha")); // lowercase
    assert!(!is_valid_sebi_algo_id("ALGO-01")); // dash not allowed in strict SEBI regex ^[A-Z0-9_]{4,32}$
    assert!(!is_valid_sebi_algo_id("")); // empty

    let invalid_algo_decision = Decision::PlaceOrder {
        symbol: "RELIANCE".into(),
        side: OrderSide::Buy,
        qty: Quantity(dec!(10)),
        price_limit: None,
        algo_id: "algo_lowercase".into(),
        rationale: None,
    };

    let res = parser.validate_decision(&invalid_algo_decision, None, None);
    assert!(matches!(
        res,
        Err(DecisionError::InvalidAlgoIdFormat { .. })
    ));
}

#[test]
fn test_price_band_grounding_validation() {
    let parser = DecisionParser::new().with_universe(vec!["RELIANCE"]);

    let band = PriceBand::new(
        Price(dec!(2400.00)),
        Price(dec!(2600.00)),
        Price(dec!(2500.00)),
    );

    // Limit order within price band passes
    let in_band = Decision::PlaceOrder {
        symbol: "RELIANCE".into(),
        side: OrderSide::Buy,
        qty: Quantity(dec!(10)),
        price_limit: Some(Price(dec!(2450.00))),
        algo_id: "ALGO_01".into(),
        rationale: None,
    };
    assert!(
        parser
            .validate_decision(&in_band, None, Some(&band))
            .is_ok()
    );

    // Limit order breaching upper circuit
    let above_band = Decision::PlaceOrder {
        symbol: "RELIANCE".into(),
        side: OrderSide::Buy,
        qty: Quantity(dec!(10)),
        price_limit: Some(Price(dec!(2650.00))),
        algo_id: "ALGO_01".into(),
        rationale: None,
    };
    let err_upper = parser
        .validate_decision(&above_band, None, Some(&band))
        .unwrap_err();
    assert!(matches!(
        err_upper,
        DecisionError::PriceBandViolation { .. }
    ));

    // Limit order breaching lower circuit
    let below_band = Decision::PlaceOrder {
        symbol: "RELIANCE".into(),
        side: OrderSide::Buy,
        qty: Quantity(dec!(10)),
        price_limit: Some(Price(dec!(2350.00))),
        algo_id: "ALGO_01".into(),
        rationale: None,
    };
    let err_lower = parser
        .validate_decision(&below_band, None, Some(&band))
        .unwrap_err();
    assert!(matches!(
        err_lower,
        DecisionError::PriceBandViolation { .. }
    ));
}

#[test]
fn test_reference_price_collar_grounding() {
    let parser = DecisionParser::new().with_max_price_deviation(dec!(10.00)); // 10% max collar
    let ref_price = Price(dec!(2000.00));

    // Ungrounded order with 25% price deviation
    let ungrounded = Decision::PlaceOrder {
        symbol: "TCS".into(),
        side: OrderSide::Buy,
        qty: Quantity(dec!(5)),
        price_limit: Some(Price(dec!(2500.00))),
        algo_id: "ALGO_01".into(),
        rationale: None,
    };
    let err = parser
        .validate_decision(&ungrounded, Some(ref_price), None)
        .unwrap_err();
    match err {
        DecisionError::UngroundedPrice {
            limit,
            reference,
            deviation_pct,
            max_pct,
        } => {
            assert_eq!(limit.0, dec!(2500.00));
            assert_eq!(reference.0, dec!(2000.00));
            assert_eq!(deviation_pct, dec!(25.00));
            assert_eq!(max_pct, dec!(10.00));
        }
        other => panic!("Expected UngroundedPrice error, got: {:?}", other),
    }

    // Grounded order within 10%
    let grounded = Decision::PlaceOrder {
        symbol: "TCS".into(),
        side: OrderSide::Buy,
        qty: Quantity(dec!(5)),
        price_limit: Some(Price(dec!(2100.00))),
        algo_id: "ALGO_01".into(),
        rationale: None,
    };
    assert!(
        parser
            .validate_decision(&grounded, Some(ref_price), None)
            .is_ok()
    );
}

#[test]
fn test_risk_engine_pre_trade_integration() {
    let parser = DecisionParser::new();
    let mut limits = RiskLimits::default();
    limits.max_quantity_per_order = Some(Quantity(dec!(100)));

    let mut risk_engine = RiskEngine::new(limits, OtrConfig::default());
    risk_engine.register_algo("ALGO_01");
    let now = Utc::now();

    let valid_decision = Decision::PlaceOrder {
        symbol: "INFY".into(),
        side: OrderSide::Buy,
        qty: Quantity(dec!(50)),
        price_limit: Some(Price(dec!(1500.00))),
        algo_id: "ALGO_01".into(),
        rationale: None,
    };
    assert!(
        parser
            .evaluate_with_risk(&valid_decision, &mut risk_engine, now)
            .is_ok()
    );

    // Freeze limit breach
    let oversized_decision = Decision::PlaceOrder {
        symbol: "INFY".into(),
        side: OrderSide::Buy,
        qty: Quantity(dec!(500)),
        price_limit: Some(Price(dec!(1500.00))),
        algo_id: "ALGO_01".into(),
        rationale: None,
    };
    assert!(matches!(
        parser.evaluate_with_risk(&oversized_decision, &mut risk_engine, now),
        Err(DecisionError::RiskRejected(_))
    ));
}

#[test]
fn test_indicator_catalog_approved_indicators() {
    let catalog = IndicatorCatalog::new();

    // Check canonical catalog entries exist
    assert!(catalog.is_approved("SMA"));
    assert!(catalog.is_approved("EMA"));
    assert!(catalog.is_approved("RSI"));
    assert!(catalog.is_approved("MACD"));
    assert!(catalog.is_approved("STOCHASTIC"));
    assert!(catalog.is_approved("ADX"));
    assert!(catalog.is_approved("CPR"));
    assert!(catalog.is_approved("VWAP"));
    assert!(catalog.is_approved("ATR"));
    assert!(catalog.is_approved("SUPERTREND"));
    assert!(catalog.is_approved("BOLLINGER_BANDS"));

    // Case-insensitive lookup
    assert!(catalog.is_approved("rsi"));
    assert!(catalog.is_approved("Supertrend"));

    // Unapproved indicator
    assert!(!catalog.is_approved("NEURAL_BLACK_BOX"));
    assert!(!catalog.is_approved("MOON_PHASE_OSCILLATOR"));
}

#[test]
fn test_indicator_catalog_parameter_bounds() {
    let catalog = IndicatorCatalog::new();

    // Valid RSI
    let mut valid_rsi = std::collections::HashMap::new();
    valid_rsi.insert("length".to_string(), dec!(14));
    assert!(catalog.validate_indicator("RSI", &valid_rsi).is_ok());

    // RSI out of bounds: length = 1 (< 2)
    let mut invalid_rsi_low = std::collections::HashMap::new();
    invalid_rsi_low.insert("length".to_string(), dec!(1));
    assert!(matches!(
        catalog.validate_indicator("RSI", &invalid_rsi_low),
        Err(CatalogError::ParameterOutOfBounds { .. })
    ));

    // RSI out of bounds: length = 150 (> 100)
    let mut invalid_rsi_high = std::collections::HashMap::new();
    invalid_rsi_high.insert("length".to_string(), dec!(150));
    assert!(matches!(
        catalog.validate_indicator("RSI", &invalid_rsi_high),
        Err(CatalogError::ParameterOutOfBounds { .. })
    ));

    // MACD fast < slow rule
    let mut valid_macd = std::collections::HashMap::new();
    valid_macd.insert("fast_length".to_string(), dec!(12));
    valid_macd.insert("slow_length".to_string(), dec!(26));
    valid_macd.insert("signal_length".to_string(), dec!(9));
    assert!(catalog.validate_indicator("MACD", &valid_macd).is_ok());

    let mut invalid_macd = std::collections::HashMap::new();
    invalid_macd.insert("fast_length".to_string(), dec!(30));
    invalid_macd.insert("slow_length".to_string(), dec!(20)); // fast >= slow!
    assert!(matches!(
        catalog.validate_indicator("MACD", &invalid_macd),
        Err(CatalogError::InvalidParameterCombination { .. })
    ));

    // Supertrend bounds
    let mut valid_st = std::collections::HashMap::new();
    valid_st.insert("period".to_string(), dec!(10));
    valid_st.insert("multiplier".to_string(), dec!(3.0));
    assert!(catalog.validate_indicator("SUPERTREND", &valid_st).is_ok());

    let mut invalid_st_mult = std::collections::HashMap::new();
    invalid_st_mult.insert("period".to_string(), dec!(10));
    invalid_st_mult.insert("multiplier".to_string(), dec!(6.0)); // > 5.0
    assert!(matches!(
        catalog.validate_indicator("SUPERTREND", &invalid_st_mult),
        Err(CatalogError::ParameterOutOfBounds { .. })
    ));
}

#[test]
fn test_validate_spec_indicators() {
    let spec = StrategySpec::new("NIFTY_TREND_ALPHA")
        .with_indicator(IndicatorInvocation::new("EMA").with_param("length", dec!(50)))
        .with_indicator(IndicatorInvocation::new("RSI").with_param("length", dec!(14)))
        .with_indicator(
            IndicatorInvocation::new("SUPERTREND")
                .with_param("period", dec!(10))
                .with_param("multiplier", dec!(3.0)),
        );

    assert!(validate_spec_indicators(&spec).is_ok());

    // Spec with hallucinated indicator
    let bad_spec = StrategySpec::new("HALLUCINATED_SPEC")
        .with_indicator(IndicatorInvocation::new("QUANTUM_MOMENTUM"));
    assert!(matches!(
        validate_spec_indicators(&bad_spec),
        Err(CatalogError::UnapprovedIndicator(_))
    ));
}

#[test]
fn test_prompt_template_and_hashing() {
    let template = PromptTemplate::new(
        "nifty_rebalance",
        "Market State: {{context}}. Please provide trade decision.",
    );
    let rendered = template.render("NIFTY 25000 RSI 65");
    assert_eq!(
        rendered,
        "Market State: NIFTY 25000 RSI 65. Please provide trade decision."
    );

    let hash = compute_prompt_hash(&rendered);
    let hex = hash_prompt_hex(&rendered);
    assert_eq!(hex.len(), 64);
    assert_eq!(hash.len(), 32);

    // Determinism test
    let hash2 = compute_prompt_hash(&rendered);
    assert_eq!(hash, hash2);
}
