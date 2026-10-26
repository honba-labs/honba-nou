# 本場・脳 (Honba-Nou) Master Plan: Frontier AI Execution Engine

## 1. Executive Summary & Non-Negotiable Directives

Honba-Nou represents the quantitative research, execution intelligence, and frontier AI reasoning brain of the Honba ecosystem. Operating under strict SEBI algorithmic trading guidelines, strict Indian market microstructure constraints (ICCL settlement, circular price bands, STT, turnover charges), and zero-loss risk gates, Honba-Nou bridges frontier LLMs (Claude 3.7 Sonnet, Gemini 2.5 Flash, DeepSeek-R1) with deterministic low-latency execution engines (`honba-ya`, `honba-mon`, `honba-me`).

### Immutable Architectural Tenets

1. **Deterministic Replay Guarantee**: Every strategy, prompt, decision, and market observation must be cryptographically pinned (SHA-256) and replayable offline with bit-for-bit parity against live execution.
2. **Zero Floats in Decision Contracts**: LLM JSON completions and execution boundaries must never communicate raw IEEE-754 floats. All prices, quantities, and cash are strictly parsed into `rust_decimal::Decimal`, `Price`, `Quantity`, or `Money`.
3. **Mandatory SEBI Audit Envelope**: No order intent leaves Honba-Nou without an append-only audit record containing the full market context, temperature, seed, model ID, raw LLM completion, registered SEBI `algo_id`, and pre-trade risk check timestamps.
4. **Closed Decision Grammar**: The model operates within a bounded domain grammar. Free-form text completions are rejected; only strongly typed `Decision` schemas passing JSON-Schema validation and circuit-breaker grounding checks can be dispatched to `honba-mon` / `honba-ya`.
5. **Fail-Closed Circuit Grounding**: All LLM suggested prices are validated against the real-time NSE/BSE dynamic price bands (`PriceBand`). Limit prices outside the circuit band or exceeding maximum allowable slippage immediately fail closed.

---

## 2. Fine-Grained Implementation Tasks

### Phase 1: Foundation & Closed Decision Contracts (P0)

- [x] **Task 1.1: Dependency & Domain Type Unification**
  - Wire `honba-core` into `honba-nou/Cargo.toml`.
  - Migrate `StrategyRunner` and `trader-loop` from excised `honba-kokoro-risk` to `honba-mon` and evaluate `Decision` against `RiskEngine` per [Risk Migration Plan](../honba-docs/docs/architecture/migration_plan_kokoro_risk_to_mon.md).
  - Remove all raw float usage in AI completions, enforcing `Price`, `Quantity`, `Money`, and `rust_decimal::Decimal`.
  - Define `honba-nou-core` crate providing shared traits and prompt types.

- [x] **Task 1.2: Closed Decision Parser (`honba_core::Decision`)**
  - Implement zero-allocation parser for `Decision` with `#[serde(deny_unknown_fields)]`.
  - Implement grounding validator: assert limit orders sit within instrument's active `PriceBand`.
  - Enforce presence of registered SEBI `algo_id` string matching `^[A-Z0-9_]{4,32}$`.
  - Unit tests verifying instant rejection of unknown fields, ungrounded prices, and missing algo IDs.

- [x] **Task 1.3: Verified Indicator Catalog**
  - Implement `IndicatorCatalog` registry containing approved mathematical indicators:
    - Moving Averages: SMA, EMA (lengths 2–500).
    - Oscillators: RSI (2–100), MACD (fast < slow), Stochastic, ADX (5–50).
    - Indian Microstructure: CPR (Pivot, TC, BC), VWAP + Standard Deviation Bands.
    - Volatility & Trend: ATR (1–200), Supertrend (period 5–50, multiplier 1.0–5.0), Bollinger Bands.
  - Implement `validate_spec_indicators(spec: &StrategySpec)` rejecting any non-catalog indicator.

---

### Phase 2: Cryptographic Prompt & Decision Replay Harness (P0)

- [x] **Task 2.1: Audit Record Schema (`LlmAuditRecord`)**
  - Define append-only schema:
    ```rust
    pub struct LlmAuditRecord {
        pub timestamp_utc: DateTime<Utc>,
        pub model: String,
        pub temperature: Decimal,
        pub seed: u64,
        pub prompt_sha256: [u8; 32],
        pub market_context_json: String,
        pub raw_prompt: String,
        pub raw_response: String,
        pub parsed_decision: Decision,
    }
    ```
  - Implement cryptographic hash chaining linking sequential records for SEBI regulatory compliance.

- [x] **Task 2.2: Replay Engine (`PromptReplayer`)**
  - Implement local SQLite WAL / JSONL prompt cache keyed by `prompt_sha256`.
  - In backtest mode, intercept LLM queries: if prompt hash exists in cache, replay cached completion with zero latency and zero API cost.
  - If cache miss in backtest: raise deterministic error or route to configured live provider.

- [x] **Task 2.3: Replay Parity Testing**
  - Write property tests asserting that replaying an archived prompt sequence reproduces the exact sequence of `Decision` enums.

---

### Phase 3: Model Context Protocol (MCP) Quantitative Server (P0)

- [x] **Task 3.1: MCP Server Transport (`honba-nou-mcp`)**
  - Implement JSON-RPC 2.0 over Stdio and SSE transports conforming to Model Context Protocol specification.
  - Integrate with `honba-ya` actor runtime for backtest execution.

- [x] **Task 3.2: Quantitative Tool Handlers**
  - `get_indicator_catalog`: Returns JSON vocabulary of approved indicators and acceptable parameter bounds.
  - `check_spec`: Validates strategy syntax and parameter ranges without running backtest.
  - `run_backtest`: Dispatches backtest request to `honba-ya-backtest` and returns progress.
  - `evaluate_metrics`: Computes un-cherrypicked statistics: Sharpe, Sortino, Max Drawdown, Win Rate, OTR, DSR, PBO.
  - `optimize_strategy`: Triggers genetic or Bayesian parameter sweep in `honba-ya`.
  - `explain_decision`: Returns exact prompt, market context, and risk check for a historical trade decision.

- [x] **Task 3.3: Frontier Agent Integration**
  - Generate MCP configuration manifests for Claude Desktop, Cursor, and Gemini CLI.

---

### Phase 4: Autonomous Quantitative Research Loop (P1)

- [x] **Task 4.1: Research Hypothesis Pipeline**
  - Implement `HypothesisGenerator` prompting LLMs to formulate falsifiable quantitative trading ideas based on market regimes (trend, mean-reversion, volatility breakout).
  - Convert hypothesis into closed `StrategySpec` JSON adhering to Indicator Catalog.

- [x] **Task 4.2: Isolated Parity Backtest Pipeline**
  - Ingest Lakehouse Parquet catalog (`honba-data`) for in-sample training window.
  - Execute backtest via `honba-ya-backtest` enforcing exact Indian statutory costs and T+1 settlement.
  - Collect trade log, execution slippage, and equity mark history.

- [x] **Task 4.3: Out-of-Sample Statistical Verification**
  - Partition historical data into In-Sample (Train) and Out-of-Sample (Validation) slices.
  - Compute Deflated Sharpe Ratio (DSR) adjusting for number of trials. Reject any candidate with $\text{DSR} < 0.95$.
  - Compute Probability of Backtest Overfitting (PBO). Reject any candidate with $\text{PBO} \ge 0.10$.
  - Generate cryptographically signed `DatasetManifest` with SHA-256 provenance.

---

### Phase 5: Automatic Strategy Enhancer Loop (P1)

- [ ] **Task 5.1: Drawdown & Failure Regime Diagnostic**
  - Analyze trade journal to identify loss clustering (e.g. repeated losses during rangebound sideways chop).
  - Compute trade efficiency, MAE (Maximum Adverse Excursion), and MFE (Maximum Favorable Excursion).

- [ ] **Task 5.2: Closed-Grammar Rule & Parameter Mutation**
  - Prompt AI agent with diagnosed regime failure and prompt:
    *"Strategy suffers 8% drawdown in sideways chop. Select an approved indicator from Catalog to filter false breakouts."*
  - AI selects approved filter (e.g. `ADX > 25` or `ATR expansion`) and mutates strategy spec.
  - Automatically expands parameter search boundaries for GA / Bayesian optimizer in `honba-ya`.

- [ ] **Task 5.3: Closed Refinement Iteration Loop**
  - Resubmit mutated candidate to `honba-ya-backtest`.
  - Require that out-of-sample Sharpe improves while Walk-Forward Efficiency (WFE) remains $\ge 0.60$.
  - Archive every iteration in an immutable `HypothesisJournal` (retaining failed hypotheses to prevent circular trials).

---

### Phase 6: StockSharp Odysseus Port & Genetic Strategy Synthesizer (P1)
> **Porting StockSharp Odysseus (StockSharp.Algo.Analytics / Odysseus)**:
> Automated genetic strategy generation, decision rule tree synthesis, and market regime clustering ported from C# to Rust/Python.

- [ ] **Task 6.1: Odysseus Genetic Rule Tree Engine**
  - [ ] Port StockSharp Odysseus Strategy Genome / Abstract Syntax Tree (AST):
    - Condition Nodes: Indicator threshold comparisons (e.g. `RSI < 30`, `Close > EMA_50`), crossover operators, logical `AND` / `OR` / `NOT` branching.
    - Action Nodes: Order intents (`PlaceBuy`, `PlaceSell`, `ClosePosition`, `TrailStop`).
    - Position Sizing Nodes: Fixed notional, Kelly criterion, volatility-adjusted sizing.
  - [ ] Implement genetic programming operators:
    - Sub-tree crossover (swapping branch logic between parent rule trees).
    - Point mutation (perturbing indicator parameters, swapping operators `>` to `<`, or substituting indicators from the 100+ StockSharp catalog).
    - Multi-objective tournament selection preserving Pareto frontier (Sharpe, Profit Factor, Drawdown).

- [ ] **Task 6.2: Market Regime Analytics (StockSharp Analytics Port)**
  - [ ] Implement statistical market state classifiers:
    - Hidden Markov Models (HMM) for bull/bear/sideways regime detection.
    - Gaussian Mixture Models (GMM) and KMeans clustering on volatility, trend slope, and order book imbalance.
  - [ ] Support regime-conditional rule execution (e.g., enable trend-following sub-tree only during high-ADX trending regimes).

- [ ] **Task 6.3: Hybrid AI + Odysseus Auto-Synthesizer**
  - [ ] Frontier LLM (Claude/Gemini) formulates macro hypotheses and selects candidate indicator families.
  - [ ] Odysseus genetic engine searches the combinatorial space of exact threshold rules and branch conditions over historical tick datasets.
  - [ ] Validated rule trees automatically compile into declarative `StrategySpec` JSON or native Python `Strategy` classes.

---

## 3. Verification & CI Test Suite

- [x] `cargo test -p honba-nou-core`: Unit tests for Decision parser, grounding checks, and indicator catalog.
- [x] `cargo test -p honba-nou-mcp`: Integration tests for all MCP tool endpoints.
- [x] `cargo test -p honba-nou-replay`: Deterministic replay tests verifying identical offline backtest output.
- [x] `cargo test -p honba-nou-research`: Unit and integration tests for Phase 4 autonomous quantitative research loop.
- [x] Golden conformance test verifying JSON schema synchronization with `schema/mcp/mcp_tools.json`.
