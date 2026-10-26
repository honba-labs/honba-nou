//! Deterministic Prompt Replayer for offline backtesting and research-live parity.
//!
//! In backtest mode, intercepts LLM queries: if the prompt hash exists in the cache,
//! it replays the cached completion with zero latency and zero API cost. If a cache miss
//! occurs in backtest mode, it raises a deterministic [`ReplayError::CacheMiss`].
//! In record mode, it can route cache misses to a configured live provider and cache the result.

use std::collections::HashMap;
use std::path::Path;

use honba_core::{Decision, Exchange, InstrumentId, OrderRequest, OrderSide, OrderType};
use honba_nou_core::{LlmProvider, compute_prompt_hash, sha256_hex};

use crate::audit::{ChainedLlmAuditRecord, LlmAuditRecord};
use crate::cache::{JsonlPromptCache, PromptCache};
use crate::error::ReplayError;

/// Operating mode for the prompt replayer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayMode {
    /// Pure deterministic backtesting: cache misses strictly fail with [`ReplayError::CacheMiss`].
    BacktestDeterministic,
    /// Record and replay: cache misses are forwarded to a live LLM provider and persisted.
    RecordAndReplay,
}

/// Deterministic replayer of recorded LLM decision trails.
pub struct PromptReplayer {
    algo_id: String,
    mode: ReplayMode,
    records: Vec<ChainedLlmAuditRecord>,
    by_prompt_hash: HashMap<[u8; 32], usize>,
    current_idx: usize,
    cache: Option<Box<dyn PromptCache>>,
}

impl PromptReplayer {
    /// Create a new replayer from an explicit sequence of chained audit records.
    pub fn new(
        algo_id: impl Into<String>,
        records: Vec<ChainedLlmAuditRecord>,
    ) -> Result<Self, ReplayError> {
        let mut by_prompt_hash = HashMap::new();

        for (idx, chained) in records.iter().enumerate() {
            chained.record.verify_prompt_integrity()?;
            by_prompt_hash.insert(chained.record.prompt_sha256, idx);
        }

        Ok(Self {
            algo_id: algo_id.into(),
            mode: ReplayMode::BacktestDeterministic,
            records,
            by_prompt_hash,
            current_idx: 0,
            cache: None,
        })
    }

    /// Set operating mode (backtest deterministic vs record-and-replay).
    pub fn with_mode(mut self, mode: ReplayMode) -> Self {
        self.mode = mode;
        self
    }

    /// Attach an external prompt cache (e.g. JSONL or SQLite WAL).
    pub fn with_cache(mut self, cache: Box<dyn PromptCache>) -> Self {
        self.cache = Some(cache);
        self
    }

    /// Load and construct replayer from a JSONL file.
    pub fn from_jsonl_file(
        algo_id: impl Into<String>,
        path: impl AsRef<Path>,
    ) -> Result<Self, ReplayError> {
        let chain = JsonlPromptCache::read_chained_jsonl(path)?;
        Self::new(algo_id, chain)
    }

    /// Number of recorded turns loaded in memory.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// True if no records loaded in memory.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Current playback cursor index.
    pub fn current_index(&self) -> usize {
        self.current_idx
    }

    /// Reset playback cursor to the beginning.
    pub fn reset(&mut self) {
        self.current_idx = 0;
    }

    /// Look up a record in memory by exact prompt text.
    pub fn lookup_by_prompt(&self, prompt: &str) -> Option<&LlmAuditRecord> {
        let hash = compute_prompt_hash(prompt);
        self.lookup_by_prompt_hash(&hash)
    }

    /// Look up a record in memory by 32-byte SHA-256 prompt hash.
    pub fn lookup_by_prompt_hash(&self, hash: &[u8; 32]) -> Option<&LlmAuditRecord> {
        self.by_prompt_hash
            .get(hash)
            .map(|&idx| &self.records[idx].record)
    }

    /// Advance playback cursor and yield the next recorded turn.
    pub fn next_record(&mut self) -> Option<&ChainedLlmAuditRecord> {
        if self.current_idx < self.records.len() {
            let record = &self.records[self.current_idx];
            self.current_idx += 1;
            Some(record)
        } else {
            None
        }
    }

    /// Advance playback cursor and yield the next recorded [`Decision`].
    pub fn next_decision(&mut self) -> Option<&Decision> {
        if self.current_idx < self.records.len() {
            let idx = self.current_idx;
            self.current_idx += 1;
            Some(&self.records[idx].record.parsed_decision)
        } else {
            None
        }
    }

    /// Advance playback cursor and convert the next recorded decision into candidate [`OrderRequest`]s.
    pub fn next_orders(&mut self) -> Vec<OrderRequest> {
        let fallback_aid = self.algo_id.clone();
        if let Some(decision) = self.next_decision() {
            Self::decision_to_orders(decision, &fallback_aid)
        } else {
            Vec::new()
        }
    }

    /// Intercept an LLM query turn during strategy execution.
    ///
    /// - If `prompt_sha256` is found in memory or cache: returns `(raw_response, parsed_decision)`
    ///   with zero latency and zero API cost.
    /// - If cache miss occurs in backtest mode (or `live_provider` is None): returns
    ///   deterministic error `Err(ReplayError::CacheMiss)`.
    /// - If cache miss occurs with `live_provider` provided: queries live LLM provider,
    ///   persists the turn to cache, and returns the result.
    pub async fn intercept_query<P: ?Sized + LlmProvider>(
        &mut self,
        prompt: &str,
        live_provider: Option<&P>,
    ) -> Result<(String, Decision), ReplayError> {
        let hash = compute_prompt_hash(prompt);

        // 1. Check in-memory indexed records
        if let Some(record) = self.lookup_by_prompt_hash(&hash) {
            return Ok((record.raw_response.clone(), record.parsed_decision.clone()));
        }

        // 2. Check attached cache if available
        if let Some(ref cache) = self.cache {
            if let Some(record) = cache.get(&hash)? {
                return Ok((record.raw_response.clone(), record.parsed_decision.clone()));
            }
        }

        // 3. Cache miss handling
        match (self.mode, live_provider) {
            (ReplayMode::BacktestDeterministic, _) | (_, None) => Err(ReplayError::CacheMiss {
                prompt_sha256: sha256_hex(&hash),
            }),
            (ReplayMode::RecordAndReplay, Some(provider)) => {
                let (raw_resp, decision) = provider
                    .generate(prompt)
                    .await
                    .map_err(|e| ReplayError::DecisionParse(e.to_string()))?;

                let record = LlmAuditRecord::new(
                    chrono::Utc::now(),
                    provider.model_name(),
                    rust_decimal::Decimal::ZERO,
                    42,
                    "{}",
                    prompt,
                    &raw_resp,
                    decision.clone(),
                );

                if let Some(ref mut cache) = self.cache {
                    cache.put(&record)?;
                }

                Ok((raw_resp, decision))
            }
        }
    }

    /// Map a domain [`Decision`] into canonical [`OrderRequest`]s.
    pub fn decision_to_orders(decision: &Decision, fallback_algo_id: &str) -> Vec<OrderRequest> {
        match decision {
            Decision::PlaceOrder {
                symbol,
                side,
                qty,
                price_limit,
                algo_id,
                ..
            } => {
                let aid = if algo_id.trim().is_empty() {
                    fallback_algo_id.to_string()
                } else {
                    algo_id.clone()
                };

                let inst = InstrumentId::new(Exchange::Nse, symbol.clone());
                let order_type = match price_limit {
                    Some(limit) => OrderType::Limit(*limit),
                    None => OrderType::Market,
                };

                let mut order = OrderRequest::new(inst, *side, order_type, *qty, aid);
                order.tag = Some(if *side == OrderSide::Buy {
                    "replay_buy".into()
                } else {
                    "replay_sell".into()
                });
                vec![order]
            }
            Decision::ModifyOrder { .. }
            | Decision::CancelOrder { .. }
            | Decision::Hold { .. }
            | Decision::CancelAll { .. } => Vec::new(),
        }
    }
}
