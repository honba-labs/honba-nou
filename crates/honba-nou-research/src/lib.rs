//! # honba-nou-research
//!
//! Autonomous Quantitative Research Loop (Phase 4 & Phase 5):
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
//! - **Task 5.1: Drawdown & Failure Regime Diagnostic**:
//!   Analyzes trade journals to detect loss clustering, compute trade efficiency, MAE, and MFE.
//! - **Task 5.2: Closed-Grammar Rule & Parameter Mutation**:
//!   Formulates regulatory prompts, mutates strategy specs with approved catalog indicators,
//!   and expands parameter search boundaries for optimizers.
//! - **Task 5.3: Closed Refinement Iteration Loop**:
//!   Iteratively validates candidates requiring out-of-sample Sharpe improvement and WFE >= 0.60,
//!   recording all iterations in an append-only, SHA-256 hash-chained `HypothesisJournal`.

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
    ClosedGrammarMutator, EnhancerPromptGenerator, FailureRegimeDiagnostic, HypothesisJournal,
    HypothesisJournalEntry, JournalTrade, LossCluster, ParameterBoundary, ParameterSearchSpace,
    RefinementIterationLoop, RefinementOutcome, TradeJournal, TradeJournalAnalysis,
    analyze_trade_journal, suggest_indicator_filter,
};
pub use hypothesis::{Hypothesis, HypothesisGenerator, MarketRegime};
pub use manifest::DatasetManifest;
pub use verification::{
    StatisticalVerificationReport, StatisticalVerifier, VerificationThresholds,
};
