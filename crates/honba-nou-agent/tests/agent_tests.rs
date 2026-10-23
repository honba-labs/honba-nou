//! Comprehensive tests for honba-nou-agent:
//! Closed decision parser, safety validations, prompt audit logging, and replay engine.

use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::fs;

use honba_core::{Decision, Exchange, InstrumentId, OrderSide, Price, PriceBand, Quantity};
use honba_mon::{OtrConfig, RiskEngine, RiskLimits};
use honba_nou_agent::{
    DecisionError, DecisionParser, LlmRecord, LlmTrailLogger, PromptReplayer, ReplayError,
    sha256_hex,
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
        "Here is my trade recommendation:\n```json\n{}\n```\nExplanation follows.",
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
fn test_strict_validation_rules() {
    let parser = DecisionParser::new()
        .with_universe(vec!["RELIANCE", "TCS"])
        .with_max_price_deviation(dec!(10.00)); // 10% collar

    // 1. Symbol not in allowed universe
    let unknown_sym = Decision::PlaceOrder {
        symbol: "UNKNOWN_PUMP".into(),
        side: OrderSide::Buy,
        qty: Quantity(dec!(10)),
        price_limit: None,
        algo_id: "ALGO_01".into(),
        rationale: None,
    };
    assert_eq!(
        parser.validate_decision(&unknown_sym, None),
        Err(DecisionError::SymbolNotInUniverse("UNKNOWN_PUMP".into()))
    );

    // 2. Non-positive quantity
    let zero_qty = Decision::PlaceOrder {
        symbol: "RELIANCE".into(),
        side: OrderSide::Buy,
        qty: Quantity(Decimal::ZERO),
        price_limit: None,
        algo_id: "ALGO_01".into(),
        rationale: None,
    };
    assert!(matches!(
        parser.validate_decision(&zero_qty, None),
        Err(DecisionError::NonPositiveQuantity(_)) | Err(DecisionError::Json(_))
    ));

    // 3. Empty algo ID
    let empty_algo = Decision::PlaceOrder {
        symbol: "RELIANCE".into(),
        side: OrderSide::Buy,
        qty: Quantity(dec!(10)),
        price_limit: None,
        algo_id: "  ".into(),
        rationale: None,
    };
    assert!(matches!(
        parser.validate_decision(&empty_algo, None),
        Err(DecisionError::EmptyAlgoId) | Err(DecisionError::Json(_))
    ));

    // 4. Ungrounded limit price (deviates > 10%)
    let ref_price = Price(dec!(2000.00));
    let ungrounded_buy = Decision::PlaceOrder {
        symbol: "RELIANCE".into(),
        side: OrderSide::Buy,
        qty: Quantity(dec!(10)),
        price_limit: Some(Price(dec!(2500.00))), // 25% above reference
        algo_id: "ALGO_01".into(),
        rationale: None,
    };
    let err = parser
        .validate_decision(&ungrounded_buy, Some(ref_price))
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

    // Grounded limit price (within 10%) passes
    let grounded_buy = Decision::PlaceOrder {
        symbol: "RELIANCE".into(),
        side: OrderSide::Buy,
        qty: Quantity(dec!(10)),
        price_limit: Some(Price(dec!(2050.00))), // 2.5% deviation
        algo_id: "ALGO_01".into(),
        rationale: None,
    };
    assert!(
        parser
            .validate_decision(&grounded_buy, Some(ref_price))
            .is_ok()
    );
}

#[test]
fn test_pre_trade_risk_engine_integration() {
    let parser = DecisionParser::new();
    let mut limits = RiskLimits::default();
    limits.max_quantity_per_order = Some(Quantity(dec!(100)));

    let mut risk_engine = RiskEngine::new(limits, OtrConfig::default());
    risk_engine.register_algo("ALGO_01");
    let now = Utc::now();

    // 1. Order within limits succeeds
    let valid_decision = Decision::PlaceOrder {
        symbol: "TCS".into(),
        side: OrderSide::Buy,
        qty: Quantity(dec!(50)),
        price_limit: Some(Price(dec!(3500.00))),
        algo_id: "ALGO_01".into(),
        rationale: None,
    };
    assert!(
        parser
            .evaluate_with_risk(&valid_decision, &mut risk_engine, now)
            .is_ok()
    );

    // 2. Order exceeding max quantity is refused by RiskEngine
    let oversized_decision = Decision::PlaceOrder {
        symbol: "TCS".into(),
        side: OrderSide::Buy,
        qty: Quantity(dec!(500)), // Limit is 100
        price_limit: Some(Price(dec!(3500.00))),
        algo_id: "ALGO_01".into(),
        rationale: None,
    };
    let refusal = parser.evaluate_with_risk(&oversized_decision, &mut risk_engine, now);
    assert!(matches!(refusal, Err(DecisionError::RiskRejected(_))));

    // 3. Price band violation
    let inst = InstrumentId::new(Exchange::Nse, "INFY");
    let band = PriceBand::new(
        Price(dec!(1350.00)),
        Price(dec!(1650.00)),
        Price(dec!(1500.00)),
    );
    risk_engine.set_price_band(inst, band);

    let out_of_band = Decision::PlaceOrder {
        symbol: "INFY".into(),
        side: OrderSide::Buy,
        qty: Quantity(dec!(10)),
        price_limit: Some(Price(dec!(1800.00))), // Outside upper band 1650
        algo_id: "ALGO_01".into(),
        rationale: None,
    };
    let band_err = parser.evaluate_with_risk(&out_of_band, &mut risk_engine, now);
    assert!(matches!(band_err, Err(DecisionError::RiskRejected(_))));

    // 4. Global kill-switch halts all decisions
    risk_engine.halt_global("SEBI emergency market closure", now);
    let halted_err = parser.evaluate_with_risk(&valid_decision, &mut risk_engine, now);
    assert!(matches!(halted_err, Err(DecisionError::RiskRejected(_))));
}

#[test]
fn test_prompt_audit_logging_and_sha256_checksums() {
    let prompt = "Market update: NIFTY at 25,000. Recommend position.";
    let raw_output = r#"{"action": "hold", "reason": "Market at resistance"}"#;

    let record = LlmRecord::new(
        prompt,
        raw_output,
        "gpt-4o-2024-08-06",
        Some(42),
        Some(0.0),
        Some(Decision::Hold {
            reason: Some("Market at resistance".into()),
        }),
    );

    assert_eq!(record.prompt_sha256, sha256_hex(prompt));
    assert_eq!(record.output_sha256, sha256_hex(raw_output));
    assert!(record.verify_integrity().is_ok());

    // Tampering with content detects mismatch
    let mut tampered = record.clone();
    tampered.prompt = "Tampered prompt text".to_string();
    assert!(matches!(
        tampered.verify_integrity(),
        Err(ReplayError::ChecksumMismatch { .. })
    ));

    // File roundtrip
    let tmp_file =
        std::env::temp_dir().join(format!("honba_test_trail_{}.jsonl", uuid::Uuid::new_v4()));
    LlmTrailLogger::write_jsonl(&tmp_file, &[record.clone()]).expect("Should write JSONL");

    let loaded = LlmTrailLogger::read_jsonl(&tmp_file).expect("Should read JSONL");
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].prompt_sha256, record.prompt_sha256);
    let _ = fs::remove_file(tmp_file);
}

#[test]
fn test_deterministic_prompt_replayer() {
    let p1 = "Tick RELIANCE 2500";
    let o1 = r#"{"action": "place_order", "symbol": "RELIANCE", "side": "BUY", "qty": "20", "price_limit": "2505.00", "algo_id": "REPLAY_ALGO"}"#;
    let d1: Decision = serde_json::from_str(o1).unwrap();

    let p2 = "Tick TCS 3500";
    let o2 = r#"{"action": "place_order", "symbol": "TCS", "side": "SELL", "qty": "15", "algo_id": "REPLAY_ALGO"}"#;
    let d2: Decision = serde_json::from_str(o2).unwrap();

    let r1 = LlmRecord::new(p1, o1, "gpt-4o", Some(100), Some(0.0), Some(d1));
    let r2 = LlmRecord::new(p2, o2, "gpt-4o", Some(100), Some(0.0), Some(d2));

    let mut replayer = PromptReplayer::new("REPLAY_ALGO", vec![r1.clone(), r2.clone()]).unwrap();
    assert_eq!(replayer.len(), 2);

    // Lookup by prompt
    let looked_up = replayer.lookup_by_prompt(p1).expect("Record 1 found");
    assert_eq!(looked_up.output_sha256, r1.output_sha256);

    // Sequential replay
    let orders1 = replayer.next_orders();
    assert_eq!(orders1.len(), 1);
    assert_eq!(orders1[0].instrument.symbol, "RELIANCE");
    assert_eq!(orders1[0].quantity.0, dec!(20));
    assert_eq!(orders1[0].algo_id, "REPLAY_ALGO");

    let orders2 = replayer.next_orders();
    assert_eq!(orders2.len(), 1);
    assert_eq!(orders2[0].instrument.symbol, "TCS");
    assert_eq!(orders2[0].quantity.0, dec!(15));

    // End of stream
    assert!(replayer.next_orders().is_empty());

    // Reset and replay again
    replayer.reset();
    let re_orders1 = replayer.next_orders();
    assert_eq!(re_orders1.len(), 1);
    assert_eq!(re_orders1[0].instrument.symbol, "RELIANCE");
}
