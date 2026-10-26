//! Tests for honba-nou-trader-loop.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use honba_core::{Decision, OrderSide, Price, Quantity};
use honba_mon::{OtrConfig, RiskEngine, RiskLimits};
use honba_nou_llm::{LlmError, LlmProvider};
use honba_nou_trader_loop::{TraderConfig, TraderLoop};
use rust_decimal_macros::dec;

struct MockLlm;

#[async_trait]
impl LlmProvider for MockLlm {
    fn model_name(&self) -> &str {
        "mock"
    }

    async fn generate(&self, _prompt: &str) -> Result<(String, Decision), LlmError> {
        Ok((
            "{}".to_string(),
            Decision::Hold {
                reason: Some("Mock hold".into()),
            },
        ))
    }

    fn context_window(&self) -> usize {
        4096
    }
}

#[test]
fn test_trader_loop_evaluates_decision_against_risk_engine() {
    let config = TraderConfig {
        name: "test_trader".to_string(),
        universe: vec!["TCS".to_string()],
        llm: Arc::new(MockLlm),
        risk_limits: RiskLimits::default(),
    };

    let trader = TraderLoop::new(config);
    let mut limits = RiskLimits::default();
    limits.max_quantity_per_order = Some(Quantity(dec!(50)));

    let mut risk_engine = RiskEngine::new(limits, OtrConfig::default());
    risk_engine.register_algo("ALGO_01");

    let valid_decision = Decision::PlaceOrder {
        symbol: "TCS".to_string(),
        side: OrderSide::Buy,
        qty: Quantity(dec!(20)),
        price_limit: Some(Price(dec!(3500))),
        algo_id: "ALGO_01".to_string(),
        rationale: None,
    };

    let now = Utc::now();
    assert!(
        trader
            .evaluate_decision(&valid_decision, &mut risk_engine, now)
            .is_ok()
    );

    let oversized_decision = Decision::PlaceOrder {
        symbol: "TCS".to_string(),
        side: OrderSide::Buy,
        qty: Quantity(dec!(100)),
        price_limit: Some(Price(dec!(3500))),
        algo_id: "ALGO_01".to_string(),
        rationale: None,
    };

    assert!(
        trader
            .evaluate_decision(&oversized_decision, &mut risk_engine, now)
            .is_err()
    );
}
