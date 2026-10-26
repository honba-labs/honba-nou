//! Phase 5: Automatic Strategy Enhancer Loop.
//!
//! Analyzes trade journals to identify loss patterns and suggests approved
//! indicator filters and parameter adjustments for refinement.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::verification::StatisticalVerificationReport;

/// Trade journal diagnostic analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeJournalAnalysis {
    pub n_trades: u64,
    pub n_winners: u64,
    pub n_losers: u64,
    pub total_pnl: Decimal,
    pub avg_win: Decimal,
    pub avg_loss: Decimal,
    pub mae_per_trade: Vec<Decimal>,
    pub mfe_per_trade: Vec<Decimal>,
    pub efficiency_ratio: f64,
}

/// Analyze trade journal for loss clustering and regime diagnostics.
pub fn analyze_trade_journal(trade_log: &Value) -> Result<TradeJournalAnalysis, String> {
    let n_trades = trade_log
        .get("n_trades")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let n_winners = trade_log
        .get("n_winners")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let n_losers = trade_log
        .get("n_losers")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let total_pnl_i64 = trade_log
        .get("total_pnl")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let total_pnl = Decimal::new(total_pnl_i64, 0);

    let avg_win = if n_winners > 0 {
        total_pnl / Decimal::new(n_winners as i64, 0)
    } else {
        Decimal::ZERO
    };

    let avg_loss = if n_losers > 0 {
        total_pnl / Decimal::new(n_losers as i64, 0)
    } else {
        Decimal::ZERO
    };

    let mae_per_trade: Vec<Decimal> = trade_log
        .get("mae_per_trade")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_f64().map(|f| Decimal::from(f as i64)))
                .collect()
        })
        .unwrap_or_default();

    let mfe_per_trade: Vec<Decimal> = trade_log
        .get("mfe_per_trade")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_f64().map(|f| Decimal::from(f as i64)))
                .collect()
        })
        .unwrap_or_default();

    let sum_mae: Decimal = mae_per_trade.iter().sum();
    let sum_mfe: Decimal = mfe_per_trade.iter().sum();
    let efficiency_ratio = if sum_mae + sum_mfe != Decimal::ZERO {
        let ratio = total_pnl / (sum_mae + sum_mfe).abs();
        ratio.to_string().parse::<f64>().unwrap_or(0.0)
    } else {
        0.0
    };

    Ok(TradeJournalAnalysis {
        n_trades,
        n_winners,
        n_losers,
        total_pnl,
        avg_win,
        avg_loss,
        mae_per_trade,
        mfe_per_trade,
        efficiency_ratio,
    })
}

/// Refinement iteration journal entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HypothesisJournalEntry {
    pub hypothesis_id: String,
    pub strategy_spec: Value,
    pub verification: StatisticalVerificationReport,
    pub passed: bool,
    pub timestamp: DateTime<Utc>,
    pub failure_reason: Option<String>,
}

/// Suggest indicator filter based on failure regime.
pub fn suggest_indicator_filter(failure_regime: &str, current_spec: &Value) -> Value {
    let mut mutated = current_spec.clone();

    if failure_regime == "mean-reversion" || failure_regime == "sideways_chop" {
        let mut filter = serde_json::Map::new();
        filter.insert("indicator".to_string(), Value::String("ADX".to_string()));
        let mut params = serde_json::Map::new();
        params.insert("length".to_string(), Value::Number(14.into()));
        params.insert("threshold".to_string(), Value::Number(25.into()));
        filter.insert("params".to_string(), Value::Object(params));
        filter.insert("operator".to_string(), Value::String("gt".to_string()));

        let mut filters = mutated
            .get("filters")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        filters.push(Value::Object(filter));
        mutated["filters"] = Value::Array(filters);
    }

    mutated
}
