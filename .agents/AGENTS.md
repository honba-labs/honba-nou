# `honba-nou` Agent Guidelines & Architecture Invariants

**Subsystem**: Brain (AI Reasoning & Strategy Synthesis)
**Language**: Rust + Python
**Primary Responsibility**: AI trading reasoning loop, Model Context Protocol (MCP) server, prompt replayer, and StockSharp Odysseus genetic strategy synthesizer.

---

## 1. Role & Architectural Hints
* `honba-nou` bridges AI intelligence with quantitative execution:
  1. **Untrusted AI Principle**: The AI cannot emit arbitrary shell commands or bypass risk limits.
  2. **Closed Decision Grammar**: All AI responses must parse strictly into `honba_core::Decision` via `#[serde(deny_unknown_fields)]`.
  3. **Deterministic Replay Harness**: All prompts, system completions, seeds, and temperatures are SHA-256 hashed and archived in SQLite/Parquet for backtest auditability.
  4. **Model Context Protocol (MCP) Server**: Exposes market depth, technical indicators, account status, and order placement tools to external LLM agents.
  5. **StockSharp Odysseus Engine**: Genetic algorithm synthesis for discovering non-linear rule combinations.
* **Idea Inheritance**:
  - **NoFX**: Clamped typed decision grammar and untrusted LLM signal generator pattern.
  - **StockSharp**: Odysseus genetic rule builder and visual node graph bridge.
  - **QuantDinger**: AI research loop and journal tracking.

---

## 2. Coding Guidelines (DDD & TDD)
* **Zero Floats Policy**: Decimals for price targets and stop limits.
* **TDD & Parallel Test Directory Rule (MANDATORY)**:
  - All tests must live in `tests/` parallel to `src/`.
  - **STRICTLY FORBIDDEN**: In-file `#[test]` inside `src/`.
  - Include deterministic prompt-replay regression tests.

---

## 3. Development & CI Workflow
* **Linting & Formatting**:
  ```bash
  cargo fmt --check
  cargo clippy --all-targets -- -D warnings
  ```
* **Testing**:
  ```bash
  cargo test --all
  ```
