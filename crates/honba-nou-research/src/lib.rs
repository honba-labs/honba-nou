//! # honba-nou-research
//!
//! Autonomous Quantitative Research Loop (Phase 4):
//! - **Task 4.1: Research Hypothesis Pipeline**:
//!   Generates falsifiable hypotheses per market regime (TrendUp, TrendDown, MeanReversion, VolatilityBreakout)
//!   and converts them into closed `StrategySpec` JSON adhering to the verified `IndicatorCatalog`.
//! - **Task 4.2: Isolated Parity Backtest Pipeline**:
//!   Ingests Lakehouse catalogs, partitions in-sample/out-of-sample data, and executes backtests
//!   enforcing exact Indian statutory costs (brokerage, STT, exchange charges, GST, stamp duty, SEBI fees)
//!   and T+1 Demat settlement.
//! - **Task 4.3: Out-of-Sample Statistical Verification**:
//!   Performs strict statistical verification rejecting candidates when $\text{DSR} < 0.95$,
//!   $\text{PBO} \ge 0.10$, or $\text{WFE} < 0.60$, and creates cryptographically signed
//!   `DatasetManifest` with SHA-256 provenance.

pub mod backtest;
pub mod enhancer;
pub mod hypothesis;
pub mod manifest;
pub mod verification;

pub use backtest::{
    EquityMarkRecord, IndianStatutoryCostCalculator, LakehouseCatalog, LakehouseSlice,
    ParityBacktestOutcome, ParityBacktestRunner, TradeFillRecord,
};
pub use enhancer::{
    HypothesisJournalEntry, TradeJournalAnalysis, analyze_trade_journal, suggest_indicator_filter,
};
pub use hypothesis::{Hypothesis, HypothesisGenerator, MarketRegime};
pub use manifest::DatasetManifest;
pub use verification::{
    StatisticalVerificationReport, StatisticalVerifier, VerificationThresholds,
};
