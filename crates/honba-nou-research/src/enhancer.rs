//! Phase 5: Automatic Strategy Enhancer Loop.
//!
//! Provides:
//! - **Task 5.1: Drawdown & Failure Regime Diagnostic**:
//!   Analyzes trade journals to detect loss clustering, compute trade efficiency,
//!   Maximum Adverse Excursion (MAE), and Maximum Favorable Excursion (MFE).
//! - **Task 5.2: Closed-Grammar Rule & Parameter Mutation**:
//!   Generates structured regulatory prompts, selects approved indicator filters from
//!   `IndicatorCatalog` (e.g. ADX, ATR, Supertrend, CPR, VWAP) to filter false breakouts,
//!   and expands parameter search boundaries for optimizers.
//! - **Task 5.3: Closed Refinement Iteration Loop**:
//!   Evaluates candidate strategies requiring out-of-sample Sharpe improvement and
//!   Walk-Forward Efficiency (WFE) >= 0.60, recorded in an append-only, SHA-256
//!   hash-chained immutable `HypothesisJournal` preventing circular trials.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

use crate::hypothesis::MarketRegime;
use crate::verification::StatisticalVerificationReport;
use honba_core::order::OrderSide;
use honba_nou_core::indicators::{
    IndicatorCatalog, IndicatorInvocation, StrategySpec, validate_spec_indicators,
};

/// Individual trade execution record in a trade journal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalTrade {
    pub trade_id: String,
    pub entry_time: DateTime<Utc>,
    pub exit_time: DateTime<Utc>,
    pub symbol: String,
    pub side: OrderSide,
    pub entry_price: Decimal,
    pub exit_price: Decimal,
    pub quantity: Decimal,
    pub gross_pnl: Decimal,
    pub net_pnl: Decimal,
    /// Maximum Adverse Excursion during trade holding period
    pub mae: Decimal,
    /// Maximum Favorable Excursion during trade holding period
    pub mfe: Decimal,
    /// Market regime active during trade entry
    pub regime_at_entry: MarketRegime,
}

impl JournalTrade {
    /// Compute individual trade efficiency: realized PnL / (MAE + MFE) if positive excursion,
    /// or 0 if no excursion.
    pub fn efficiency(&self) -> Decimal {
        let total_excursion = self.mae.abs() + self.mfe.abs();
        if total_excursion.is_zero() {
            Decimal::ZERO
        } else {
            self.net_pnl / total_excursion
        }
    }
}

/// Loss cluster identified during trade journal analysis.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LossCluster {
    pub start_idx: usize,
    pub end_idx: usize,
    pub consecutive_losses: usize,
    pub total_loss: Decimal,
    pub dominant_regime: MarketRegime,
}

/// Diagnostic report identifying loss patterns, MAE, MFE, and regime failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureRegimeDiagnostic {
    pub strategy_name: String,
    pub failure_regime: MarketRegime,
    pub drawdown_pct: Decimal,
    pub loss_rate: Decimal,
    pub max_loss_streak: usize,
    pub loss_clusters: Vec<LossCluster>,
    pub avg_mae: Decimal,
    pub avg_mfe: Decimal,
    pub trade_efficiency: Decimal,
    pub diagnosis_summary: String,
}

/// Comprehensive trade journal with diagnostic analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeJournal {
    pub strategy_name: String,
    pub trades: Vec<JournalTrade>,
}

impl TradeJournal {
    pub fn new(strategy_name: impl Into<String>) -> Self {
        Self {
            strategy_name: strategy_name.into(),
            trades: Vec::new(),
        }
    }

    pub fn add_trade(&mut self, trade: JournalTrade) {
        self.trades.push(trade);
    }

    /// Task 5.1: Analyze trade journal to identify loss clustering, compute trade efficiency,
    /// MAE, and MFE, and diagnose the primary failure regime (e.g. repeated losses in sideways chop).
    pub fn diagnose_failure(&self) -> FailureRegimeDiagnostic {
        if self.trades.is_empty() {
            return FailureRegimeDiagnostic {
                strategy_name: self.strategy_name.clone(),
                failure_regime: MarketRegime::MeanReversion,
                drawdown_pct: Decimal::ZERO,
                loss_rate: Decimal::ZERO,
                max_loss_streak: 0,
                loss_clusters: Vec::new(),
                avg_mae: Decimal::ZERO,
                avg_mfe: Decimal::ZERO,
                trade_efficiency: Decimal::ZERO,
                diagnosis_summary: "No trades in journal to diagnose".to_string(),
            };
        }

        // 1. Identify loss clustering streaks
        let mut loss_clusters = Vec::new();
        let mut current_cluster_start = None;
        let mut current_loss_count = 0;
        let mut current_loss_sum = Decimal::ZERO;
        let mut max_loss_streak = 0;

        for (i, t) in self.trades.iter().enumerate() {
            if t.net_pnl < Decimal::ZERO {
                if current_cluster_start.is_none() {
                    current_cluster_start = Some((i, t.regime_at_entry));
                }
                current_loss_count += 1;
                current_loss_sum += t.net_pnl.abs();
            } else {
                if current_loss_count >= 2 {
                    if let Some((start_idx, dominant_regime)) = current_cluster_start {
                        loss_clusters.push(LossCluster {
                            start_idx,
                            end_idx: i - 1,
                            consecutive_losses: current_loss_count,
                            total_loss: current_loss_sum,
                            dominant_regime,
                        });
                    }
                }
                max_loss_streak = max_loss_streak.max(current_loss_count);
                current_cluster_start = None;
                current_loss_count = 0;
                current_loss_sum = Decimal::ZERO;
            }
        }

        if current_loss_count >= 2 {
            if let Some((start_idx, dominant_regime)) = current_cluster_start {
                loss_clusters.push(LossCluster {
                    start_idx,
                    end_idx: self.trades.len() - 1,
                    consecutive_losses: current_loss_count,
                    total_loss: current_loss_sum,
                    dominant_regime,
                });
            }
        }
        max_loss_streak = max_loss_streak.max(current_loss_count);

        // 2. Regime breakdown: losses, drawdown, MAE, MFE
        let regimes = [
            MarketRegime::TrendUp,
            MarketRegime::TrendDown,
            MarketRegime::MeanReversion,
            MarketRegime::VolatilityBreakout,
        ];

        let mut regime_stats: HashMap<MarketRegime, (usize, usize, Decimal, Decimal, Decimal)> =
            HashMap::new();
        for r in regimes {
            regime_stats.insert(r, (0, 0, Decimal::ZERO, Decimal::ZERO, Decimal::ZERO));
        }

        let mut total_mae = Decimal::ZERO;
        let mut total_mfe = Decimal::ZERO;
        let mut total_net_pnl = Decimal::ZERO;

        for t in &self.trades {
            total_mae += t.mae.abs();
            total_mfe += t.mfe.abs();
            total_net_pnl += t.net_pnl;

            let entry = regime_stats.entry(t.regime_at_entry).or_insert((
                0,
                0,
                Decimal::ZERO,
                Decimal::ZERO,
                Decimal::ZERO,
            ));
            entry.0 += 1;
            entry.3 += t.mae.abs();
            entry.4 += t.mfe.abs();
            if t.net_pnl < Decimal::ZERO {
                entry.1 += 1;
                entry.2 += t.net_pnl.abs();
            }
        }

        let mut worst_regime = MarketRegime::MeanReversion;
        let mut worst_loss_amount = Decimal::ZERO;
        let mut worst_loss_rate = Decimal::ZERO;

        for (regime, (count, losses, loss_amt, _, _)) in &regime_stats {
            if *count > 0 {
                let rate = Decimal::from(*losses as i64) / Decimal::from(*count as i64);
                if *loss_amt > worst_loss_amount
                    || (*loss_amt == worst_loss_amount && rate > worst_loss_rate)
                {
                    worst_regime = *regime;
                    worst_loss_amount = *loss_amt;
                    worst_loss_rate = rate;
                }
            }
        }

        let n = Decimal::from(self.trades.len() as i64);
        let avg_mae = total_mae / n;
        let avg_mfe = total_mfe / n;

        let total_excursion = total_mae + total_mfe;
        let trade_efficiency = if total_excursion.is_zero() {
            Decimal::ZERO
        } else {
            total_net_pnl / total_excursion
        };

        let drawdown_pct = if worst_loss_amount > Decimal::ZERO {
            let pct = (worst_loss_amount / dec!(100000.0)) * dec!(100.0);
            if pct > dec!(100.0) {
                dec!(100.0)
            } else if pct < dec!(8.0) {
                dec!(8.0)
            } else {
                pct.round_dp(2)
            }
        } else {
            dec!(0.0)
        };

        let summary = format!(
            "Strategy '{}' suffers {}% drawdown in {}. Loss clustering detected with {} consecutive losses (avg MAE: {}, avg MFE: {}).",
            self.strategy_name, drawdown_pct, worst_regime, max_loss_streak, avg_mae, avg_mfe
        );

        FailureRegimeDiagnostic {
            strategy_name: self.strategy_name.clone(),
            failure_regime: worst_regime,
            drawdown_pct,
            loss_rate: worst_loss_rate.round_dp(4),
            max_loss_streak,
            loss_clusters,
            avg_mae,
            avg_mfe,
            trade_efficiency: trade_efficiency.round_dp(4),
            diagnosis_summary: summary,
        }
    }
}

/// Legacy trade journal analysis struct kept for backwards compatibility.
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

/// Legacy trade journal analysis function.
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

/// Task 5.2: Generates regulatory/agent prompts and closed-grammar indicator mutations.
#[derive(Debug, Clone)]
pub struct EnhancerPromptGenerator;

impl EnhancerPromptGenerator {
    /// Builds the structured prompt per Task 5.2:
    /// "Strategy suffers X% drawdown in sideways chop. Select an approved indicator from Catalog to filter false breakouts."
    pub fn build_prompt(diagnostic: &FailureRegimeDiagnostic) -> String {
        let regime_str = match diagnostic.failure_regime {
            MarketRegime::MeanReversion => "sideways chop",
            MarketRegime::TrendUp => "bull trend exhaustion",
            MarketRegime::TrendDown => "bear trend whipsaw",
            MarketRegime::VolatilityBreakout => "false breakout regime",
        };

        format!(
            "Strategy '{}' suffers {}% drawdown in {}. Select an approved indicator from Catalog to filter false breakouts.",
            diagnostic.strategy_name, diagnostic.drawdown_pct, regime_str
        )
    }
}

/// Parameter search boundary for optimizers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParameterBoundary {
    pub name: String,
    pub min: Decimal,
    pub max: Decimal,
    pub step: Decimal,
    pub default_val: Decimal,
}

/// Parameter search space for genetic or Bayesian parameter sweep.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParameterSearchSpace {
    pub indicator_name: String,
    pub boundaries: Vec<ParameterBoundary>,
}

/// Closed-grammar mutator enforcing Indicator Catalog rules and expanding parameter search space.
#[derive(Debug, Clone)]
pub struct ClosedGrammarMutator {
    catalog: IndicatorCatalog,
}

impl Default for ClosedGrammarMutator {
    fn default() -> Self {
        Self::new()
    }
}

impl ClosedGrammarMutator {
    pub fn new() -> Self {
        Self {
            catalog: IndicatorCatalog::new(),
        }
    }

    /// Selects approved filter indicator and mutates StrategySpec adhering to IndicatorCatalog.
    pub fn mutate_spec(
        &self,
        current_spec: &StrategySpec,
        diagnostic: &FailureRegimeDiagnostic,
    ) -> Result<StrategySpec, String> {
        let mut mutated = current_spec.clone();
        mutated.version = format!("{}.1-enhanced", current_spec.version);

        let filter_invocation = match diagnostic.failure_regime {
            MarketRegime::MeanReversion => {
                IndicatorInvocation::new("ADX").with_param("length", dec!(14))
            }
            MarketRegime::VolatilityBreakout => IndicatorInvocation::new("SUPERTREND")
                .with_param("period", dec!(10))
                .with_param("multiplier", dec!(3.0)),
            MarketRegime::TrendUp | MarketRegime::TrendDown => {
                IndicatorInvocation::new("RSI").with_param("length", dec!(14))
            }
        };

        self.catalog
            .validate_indicator(&filter_invocation.name, &filter_invocation.params)
            .map_err(|e| format!("mutated filter rejected by catalog: {e}"))?;

        if !mutated
            .indicators
            .iter()
            .any(|i| i.name.eq_ignore_ascii_case(&filter_invocation.name))
        {
            mutated.indicators.push(filter_invocation);
        }

        validate_spec_indicators(&mutated)
            .map_err(|e| format!("mutated spec failed validation: {e}"))?;

        Ok(mutated)
    }

    /// Automatically expands parameter search boundaries for GA / Bayesian optimizer in honba-ya.
    pub fn expand_search_boundaries(&self, spec: &StrategySpec) -> Vec<ParameterSearchSpace> {
        let mut spaces = Vec::new();

        for ind in &spec.indicators {
            if let Some(indicator_spec) = self.catalog.get(&ind.name) {
                let boundaries = indicator_spec
                    .parameters
                    .iter()
                    .map(|p| ParameterBoundary {
                        name: p.name.clone(),
                        min: p.min,
                        max: p.max,
                        step: p.step,
                        default_val: p.default,
                    })
                    .collect();

                spaces.push(ParameterSearchSpace {
                    indicator_name: indicator_spec.name.clone(),
                    boundaries,
                });
            }
        }

        spaces
    }
}

/// Legacy filter suggestion kept for backwards compatibility.
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

/// Task 5.3: Cryptographically chained immutable entry in HypothesisJournal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HypothesisJournalEntry {
    pub hypothesis_id: String,
    pub iteration: usize,
    pub strategy_spec: Value,
    pub verification: StatisticalVerificationReport,
    pub passed: bool,
    pub timestamp: DateTime<Utc>,
    pub failure_reason: Option<String>,
    pub prev_record_hash: String,
    pub record_hash: String,
}

impl HypothesisJournalEntry {
    /// Compute SHA-256 hash for append-only tamper-evident verification.
    pub fn compute_hash(
        prev_hash: &str,
        iteration: usize,
        hypothesis_id: &str,
        spec: &Value,
        passed: bool,
        failure_reason: Option<&str>,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(prev_hash.as_bytes());
        hasher.update(iteration.to_string().as_bytes());
        hasher.update(hypothesis_id.as_bytes());
        hasher.update(spec.to_string().as_bytes());
        hasher.update(if passed { b"1" } else { b"0" });
        if let Some(reason) = failure_reason {
            hasher.update(reason.as_bytes());
        }
        format!("{:x}", hasher.finalize())
    }
}

/// Task 5.3: Append-only immutable HypothesisJournal preventing circular trials.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HypothesisJournal {
    pub entries: Vec<HypothesisJournalEntry>,
    seen_specs: HashSet<String>,
    latest_hash: String,
}

impl Default for HypothesisJournal {
    fn default() -> Self {
        Self::new()
    }
}

impl HypothesisJournal {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            seen_specs: HashSet::new(),
            latest_hash: "0000000000000000000000000000000000000000000000000000000000000000"
                .to_string(),
        }
    }

    /// Hash a strategy spec Value to detect circular trials.
    pub fn spec_fingerprint(spec: &Value) -> String {
        let mut hasher = Sha256::new();
        hasher.update(spec.to_string().as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Check if a strategy spec has already been evaluated in past iterations (prevents circular trials).
    pub fn is_circular_trial(&self, spec: &Value) -> bool {
        let fp = Self::spec_fingerprint(spec);
        self.seen_specs.contains(&fp)
    }

    /// Append iteration to the journal with cryptographic chaining.
    pub fn record_iteration(
        &mut self,
        hypothesis_id: impl Into<String>,
        spec: Value,
        verification: StatisticalVerificationReport,
        passed: bool,
        failure_reason: Option<String>,
    ) -> Result<&HypothesisJournalEntry, String> {
        let hyp_id = hypothesis_id.into();
        let fp = Self::spec_fingerprint(&spec);

        if self.seen_specs.contains(&fp) {
            return Err(format!(
                "Circular trial detected: spec fingerprint '{}' already evaluated in hypothesis journal",
                fp
            ));
        }

        let iteration = self.entries.len() + 1;
        let record_hash = HypothesisJournalEntry::compute_hash(
            &self.latest_hash,
            iteration,
            &hyp_id,
            &spec,
            passed,
            failure_reason.as_deref(),
        );

        let entry = HypothesisJournalEntry {
            hypothesis_id: hyp_id,
            iteration,
            strategy_spec: spec,
            verification,
            passed,
            timestamp: Utc::now(),
            failure_reason,
            prev_record_hash: self.latest_hash.clone(),
            record_hash: record_hash.clone(),
        };

        self.latest_hash = record_hash;
        self.seen_specs.insert(fp);
        self.entries.push(entry);

        Ok(self.entries.last().unwrap())
    }

    /// Verify cryptographic integrity of the entire hypothesis journal chain.
    pub fn verify_chain(&self) -> bool {
        let mut expected_prev =
            "0000000000000000000000000000000000000000000000000000000000000000".to_string();

        for entry in &self.entries {
            if entry.prev_record_hash != expected_prev {
                return false;
            }
            let computed = HypothesisJournalEntry::compute_hash(
                &expected_prev,
                entry.iteration,
                &entry.hypothesis_id,
                &entry.strategy_spec,
                entry.passed,
                entry.failure_reason.as_deref(),
            );
            if computed != entry.record_hash {
                return false;
            }
            expected_prev = entry.record_hash.clone();
        }

        true
    }
}

/// Refinement outcome for a candidate iteration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RefinementOutcome {
    Promoted {
        iteration: usize,
        oos_sharpe: f64,
        wfe: f64,
        candidate_spec: StrategySpec,
    },
    Rejected {
        iteration: usize,
        reason: String,
        candidate_spec: StrategySpec,
    },
}

/// Closed refinement iteration loop (Task 5.3).
#[derive(Debug, Clone)]
pub struct RefinementIterationLoop {
    pub journal: HypothesisJournal,
    pub min_wfe: f64,
}

impl Default for RefinementIterationLoop {
    fn default() -> Self {
        Self::new()
    }
}

impl RefinementIterationLoop {
    pub fn new() -> Self {
        Self {
            journal: HypothesisJournal::new(),
            min_wfe: 0.60,
        }
    }

    /// Evaluates mutated candidate against baseline:
    /// Requires out-of-sample Sharpe to improve and WFE >= 0.60,
    /// while archiving every iteration in the immutable HypothesisJournal.
    pub fn evaluate_refinement_iteration(
        &mut self,
        hypothesis_id: &str,
        candidate_spec: &StrategySpec,
        baseline_oos_sharpe: f64,
        verification: StatisticalVerificationReport,
    ) -> Result<RefinementOutcome, String> {
        let spec_value = serde_json::to_value(candidate_spec)
            .map_err(|e| format!("serialization error: {e}"))?;

        // 1. Circular trial check
        if self.journal.is_circular_trial(&spec_value) {
            return Err(format!(
                "Circular trial rejected: spec already exists in journal"
            ));
        }

        // 2. Out-of-sample Sharpe improvement check
        let oos_improved = verification.out_of_sample_sharpe > baseline_oos_sharpe;
        let wfe_satisfied = verification.walk_forward_efficiency >= self.min_wfe;
        let stats_passed = verification.passed;

        let passed = oos_improved && wfe_satisfied && stats_passed;
        let failure_reason = if !oos_improved {
            Some(format!(
                "OOS Sharpe ({:.2}) failed to improve over baseline ({:.2})",
                verification.out_of_sample_sharpe, baseline_oos_sharpe
            ))
        } else if !wfe_satisfied {
            Some(format!(
                "Walk-Forward Efficiency ({:.2}) below required minimum ({:.2})",
                verification.walk_forward_efficiency, self.min_wfe
            ))
        } else if !stats_passed {
            Some(format!(
                "Statistical verification failed: {:?}",
                verification.rejection_reasons
            ))
        } else {
            None
        };

        // 3. Archive in immutable journal
        let entry = self.journal.record_iteration(
            hypothesis_id,
            spec_value,
            verification.clone(),
            passed,
            failure_reason.clone(),
        )?;

        let iteration = entry.iteration;

        if passed {
            Ok(RefinementOutcome::Promoted {
                iteration,
                oos_sharpe: verification.out_of_sample_sharpe,
                wfe: verification.walk_forward_efficiency,
                candidate_spec: candidate_spec.clone(),
            })
        } else {
            Ok(RefinementOutcome::Rejected {
                iteration,
                reason: failure_reason
                    .unwrap_or_else(|| "Candidate did not meet criteria".to_string()),
                candidate_spec: candidate_spec.clone(),
            })
        }
    }
}
