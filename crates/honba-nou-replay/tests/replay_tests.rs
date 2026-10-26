//! Comprehensive unit, integration, and property test suite for honba-nou-replay:
//! Cryptographic prompt audit records, hash chaining, SQLite WAL / JSONL caching,
//! deterministic query interception, and parity testing.

use std::fs;

use chrono::Utc;
use honba_core::{Decision, OrderSide, Price, Quantity};
use honba_nou_core::{compute_prompt_hash, sha256_hex};
use honba_nou_replay::{
    AuditChainLedger, JsonlPromptCache, LlmAuditRecord, PromptCache, PromptReplayer, ReplayError,
    ReplayMode, SqlitePromptCache, verify_hash_chain,
};
use proptest::prelude::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

fn sample_decision_buy(symbol: &str, qty: u64, price: u64) -> Decision {
    Decision::PlaceOrder {
        symbol: symbol.to_string(),
        side: OrderSide::Buy,
        qty: Quantity::from_u64(qty),
        price_limit: Some(Price(Decimal::from(price))),
        algo_id: "ALGO_MOMENTUM_V1".to_string(),
        rationale: Some("Bullish breakout".to_string()),
    }
}

fn sample_decision_sell(symbol: &str, qty: u64, price: u64) -> Decision {
    Decision::PlaceOrder {
        symbol: symbol.to_string(),
        side: OrderSide::Sell,
        qty: Quantity::from_u64(qty),
        price_limit: Some(Price(Decimal::from(price))),
        algo_id: "ALGO_MOMENTUM_V1".to_string(),
        rationale: Some("Take profit target".to_string()),
    }
}

#[test]
fn test_llm_audit_record_creation_and_integrity() {
    let now = Utc::now();
    let prompt = "NIFTY 25000: advise execution strategy";
    let raw_resp = r#"{"action":"hold","reason":"Rangebound market"}"#;
    let decision = Decision::Hold {
        reason: Some("Rangebound market".into()),
    };

    let record = LlmAuditRecord::new(
        now,
        "claude-3-5-sonnet",
        dec!(0.2),
        1001,
        r#"{"price": 25000}"#,
        prompt,
        raw_resp,
        decision.clone(),
    );

    assert_eq!(record.prompt_sha256, compute_prompt_hash(prompt));
    assert_eq!(
        record.prompt_sha256_hex(),
        sha256_hex(&compute_prompt_hash(prompt))
    );
    assert!(record.verify_prompt_integrity().is_ok());

    // Tampering with prompt triggers integrity check error
    let mut tampered = record.clone();
    tampered.raw_prompt = "Tampered prompt text".to_string();
    assert!(matches!(
        tampered.verify_prompt_integrity(),
        Err(ReplayError::ChecksumMismatch { .. })
    ));
}

#[test]
fn test_cryptographic_hash_chaining_and_tamper_detection() {
    let mut ledger = AuditChainLedger::new();
    let now = Utc::now();

    let d1 = sample_decision_buy("RELIANCE", 20, 2500);
    let r1 = LlmAuditRecord::new(
        now,
        "model-a",
        dec!(0.0),
        42,
        "{}",
        "prompt-1",
        "resp-1",
        d1,
    );
    ledger.append(r1);

    let d2 = sample_decision_sell("TCS", 10, 3500);
    let r2 = LlmAuditRecord::new(
        now,
        "model-a",
        dec!(0.0),
        43,
        "{}",
        "prompt-2",
        "resp-2",
        d2,
    );
    ledger.append(r2);

    let d3 = Decision::Hold {
        reason: Some("No trade".into()),
    };
    let r3 = LlmAuditRecord::new(
        now,
        "model-a",
        dec!(0.0),
        44,
        "{}",
        "prompt-3",
        "resp-3",
        d3,
    );
    ledger.append(r3);

    assert_eq!(ledger.len(), 3);
    assert!(ledger.verify().is_ok());

    let records = ledger.records().to_vec();

    // Verify first link starts with zero hash
    assert_eq!(records[0].prev_record_hash, [0u8; 32]);
    // Verify link 1 prev matches link 0 hash
    assert_eq!(records[1].prev_record_hash, records[0].record_hash);
    // Verify link 2 prev matches link 1 hash
    assert_eq!(records[2].prev_record_hash, records[1].record_hash);

    // Tamper 1: Mutating prompt in middle record breaks chain
    let mut tampered = records.clone();
    tampered[1].record.raw_prompt = "TAMPERED_PROMPT".to_string();
    assert!(verify_hash_chain(&tampered).is_err());

    // Tamper 2: Mutating decision breaks hash chain
    let mut tampered_dec = records.clone();
    tampered_dec[0].record.parsed_decision = sample_decision_sell("RELIANCE", 999, 100);
    assert!(verify_hash_chain(&tampered_dec).is_err());

    // Tamper 3: Swapping order of records breaks sequence
    let swapped = vec![records[1].clone(), records[0].clone(), records[2].clone()];
    assert!(matches!(
        verify_hash_chain(&swapped),
        Err(ReplayError::ChainBroken { .. })
    ));
}

#[test]
fn test_jsonl_prompt_cache_roundtrip() {
    let tmp_path =
        std::env::temp_dir().join(format!("honba_jsonl_cache_{}.jsonl", uuid::Uuid::new_v4()));
    let now = Utc::now();

    let d = sample_decision_buy("INFY", 50, 1600);
    let record = LlmAuditRecord::new(
        now,
        "gpt-4o",
        dec!(0.1),
        999,
        "{}",
        "Evaluate INFY at 1600",
        "{\"action\":\"place_order\"}",
        d.clone(),
    );

    {
        let mut cache = JsonlPromptCache::open(&tmp_path).expect("Open JSONL cache");
        cache.put(&record).expect("Put record");
        let retrieved = cache.get(&record.prompt_sha256).expect("Get record");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().parsed_decision, d);
    }

    // Reopen and verify persistent in-memory indexing
    {
        let cache2 = JsonlPromptCache::open(&tmp_path).expect("Reopen JSONL cache");
        let retrieved = cache2.get(&record.prompt_sha256).expect("Get record");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().parsed_decision, d);
    }

    let _ = fs::remove_file(tmp_path);
}

#[tokio::test]
async fn test_sqlite_wal_prompt_cache() {
    let cache = SqlitePromptCache::in_memory()
        .await
        .expect("Initialize in-memory SQLite WAL cache");

    let now = Utc::now();
    let d1 = sample_decision_buy("TCS", 15, 3450);
    let r1 = LlmAuditRecord::new(
        now,
        "claude-3-5-sonnet",
        dec!(0.0),
        555,
        r#"{"symbol":"TCS"}"#,
        "Evaluate TCS breakout",
        r#"{"action":"place_order","symbol":"TCS"}"#,
        d1.clone(),
    );

    // Put and retrieve
    cache.put_record(&r1).await.expect("Put record");
    let fetched = cache
        .get_by_prompt_hash(&r1.prompt_sha256)
        .await
        .expect("Get record");

    assert!(fetched.is_some());
    let retrieved_record = fetched.unwrap();
    assert_eq!(retrieved_record.model, "claude-3-5-sonnet");
    assert_eq!(retrieved_record.parsed_decision, d1);

    // Chained record persistence
    let mut ledger = AuditChainLedger::new();
    ledger.append(r1.clone());
    let d2 = sample_decision_sell("TCS", 15, 3600);
    let r2 = LlmAuditRecord::new(
        now,
        "claude-3-5-sonnet",
        dec!(0.0),
        556,
        r#"{"symbol":"TCS"}"#,
        "Evaluate TCS exit",
        r#"{"action":"place_order","symbol":"TCS"}"#,
        d2,
    );
    ledger.append(r2);

    for chained in ledger.records() {
        cache.put_chained(chained).await.expect("Put chained");
    }

    let chain_retrieved = cache
        .list_chained_sequence()
        .await
        .expect("Fetch chained sequence");
    assert_eq!(chain_retrieved.len(), 2);
    assert!(verify_hash_chain(&chain_retrieved).is_ok());
}

#[tokio::test]
async fn test_prompt_replayer_intercept_query() {
    let mut ledger = AuditChainLedger::new();
    let prompt1 = "Prompt 1: buy RELIANCE";
    let raw1 = r#"{"action":"place_order","symbol":"RELIANCE","side":"BUY","qty":"10","algo_id":"ALGO_01"}"#;
    let d1 = sample_decision_buy("RELIANCE", 10, 2500);

    ledger.append(LlmAuditRecord::new(
        Utc::now(),
        "model",
        dec!(0.0),
        1,
        "{}",
        prompt1,
        raw1,
        d1.clone(),
    ));

    let mut replayer = PromptReplayer::new("ALGO_01", ledger.records().to_vec())
        .expect("Create replayer")
        .with_mode(ReplayMode::BacktestDeterministic);

    // Intercept with cache hit
    let (resp, dec) = replayer
        .intercept_query::<dyn honba_nou_core::LlmProvider>(prompt1, None)
        .await
        .expect("Cache hit should return without LLM provider");
    assert_eq!(resp, raw1);
    assert_eq!(dec, d1);

    // Intercept with cache miss in backtest mode: must raise deterministic CacheMiss error
    let unrecorded_prompt = "Prompt not in archive";
    let miss_err = replayer
        .intercept_query::<dyn honba_nou_core::LlmProvider>(unrecorded_prompt, None)
        .await;

    assert!(matches!(miss_err, Err(ReplayError::CacheMiss { .. })));
}

#[test]
fn test_replayer_sequential_order_generation() {
    let mut ledger = AuditChainLedger::new();
    let d1 = sample_decision_buy("RELIANCE", 25, 2480);
    let d2 = sample_decision_sell("TCS", 10, 3500);

    ledger.append(LlmAuditRecord::new(
        Utc::now(),
        "m",
        dec!(0),
        1,
        "{}",
        "p1",
        "r1",
        d1,
    ));
    ledger.append(LlmAuditRecord::new(
        Utc::now(),
        "m",
        dec!(0),
        2,
        "{}",
        "p2",
        "r2",
        d2,
    ));

    let mut replayer = PromptReplayer::new("TEST_ALGO", ledger.records().to_vec()).unwrap();
    assert_eq!(replayer.len(), 2);

    let orders1 = replayer.next_orders();
    assert_eq!(orders1.len(), 1);
    assert_eq!(orders1[0].instrument.symbol, "RELIANCE");
    assert_eq!(orders1[0].quantity.0, dec!(25));
    assert_eq!(orders1[0].side, OrderSide::Buy);

    let orders2 = replayer.next_orders();
    assert_eq!(orders2.len(), 1);
    assert_eq!(orders2[0].instrument.symbol, "TCS");
    assert_eq!(orders2[0].quantity.0, dec!(10));
    assert_eq!(orders2[0].side, OrderSide::Sell);

    assert!(replayer.next_orders().is_empty());

    // Reset and replay again deterministically
    replayer.reset();
    let re_orders1 = replayer.next_orders();
    assert_eq!(re_orders1.len(), 1);
    assert_eq!(re_orders1[0].instrument.symbol, "RELIANCE");
}

// ---------------------------------------------------------------------------
// Property-based testing: Replay Parity (Task 2.3)
// ---------------------------------------------------------------------------

proptest! {
    /// Property test asserting that an arbitrary generated sequence of decisions,
    /// when archived and replayed through PromptReplayer, reproduces the exact
    /// identical sequence of decisions byte-for-byte and enum-for-enum.
    #[test]
    fn test_proptest_replay_parity(
        symbols in prop::collection::vec("[A-Z]{3,8}", 1..20),
        quantities in prop::collection::vec(1u64..100u64, 1..20),
        prices in prop::collection::vec(100u64..5000u64, 1..20),
        sides in prop::collection::vec(prop::bool::ANY, 1..20),
    ) {
        let n = symbols.len().min(quantities.len()).min(prices.len()).min(sides.len());
        prop_assume!(n > 0);

        let mut expected_decisions = Vec::new();
        let mut ledger = AuditChainLedger::new();
        let now = Utc::now();

        for i in 0..n {
            let side = if sides[i] { OrderSide::Buy } else { OrderSide::Sell };
            let dec = Decision::PlaceOrder {
                symbol: symbols[i].clone(),
                side,
                qty: Quantity::from_u64(quantities[i]),
                price_limit: Some(Price(Decimal::from(prices[i]))),
                algo_id: "ALGO_PROPTEST".to_string(),
                rationale: Some(format!("Step {i}")),
            };

            expected_decisions.push(dec.clone());

            let prompt = format!("Step {i}: Evaluate {}", symbols[i]);
            let response = serde_json::to_string(&dec).unwrap();

            ledger.append(LlmAuditRecord::new(
                now,
                "prop-model",
                Decimal::ZERO,
                i as u64,
                "{}",
                prompt,
                response,
                dec,
            ));
        }

        // Verify cryptographic hash chain before playback
        prop_assert!(ledger.verify().is_ok());

        let mut replayer = PromptReplayer::new("ALGO_PROPTEST", ledger.records().to_vec()).unwrap();
        prop_assert_eq!(replayer.len(), n);

        // Replay and assert 100% parity
        let mut replayed_decisions = Vec::new();
        while let Some(dec) = replayer.next_decision() {
            replayed_decisions.push(dec.clone());
        }

        prop_assert_eq!(expected_decisions.len(), replayed_decisions.len());
        for (idx, (exp, act)) in expected_decisions.iter().zip(replayed_decisions.iter()).enumerate() {
            prop_assert_eq!(exp, act, "Parity mismatch at index {}", idx);
        }
    }
}
