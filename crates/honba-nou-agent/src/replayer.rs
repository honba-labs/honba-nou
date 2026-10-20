//! Deterministic Prompt Replayer for backtesting recorded LLM decision trails.
//!
//! Enables recorded LLM interaction logs to be replayed through the event-driven
//! backtester deterministically with full research-live parity and risk gate enforcement.

use std::collections::HashMap;
use std::path::Path;

use honba_core::{Decision, InstrumentId, OrderRequest, OrderSide, OrderType};

use crate::audit::{LlmRecord, LlmTrailLogger, sha256_hex};
use crate::error::ReplayError;

/// Deterministic replayer of recorded LLM decision trails.
#[derive(Debug, Clone)]
pub struct PromptReplayer {
    algo_id: String,
    records: Vec<LlmRecord>,
    by_prompt_hash: HashMap<String, usize>,
    by_output_hash: HashMap<String, usize>,
    current_idx: usize,
}

impl PromptReplayer {
    /// Create a new replayer from a sequence of audit records.
    /// Verifies SHA256 integrity for all loaded records.
    pub fn new(algo_id: impl Into<String>, records: Vec<LlmRecord>) -> Result<Self, ReplayError> {
        if records.is_empty() {
            return Err(ReplayError::EmptyTrail);
        }

        let mut by_prompt_hash = HashMap::new();
        let mut by_output_hash = HashMap::new();

        for (idx, record) in records.iter().enumerate() {
            record.verify_integrity()?;
            by_prompt_hash.insert(record.prompt_sha256.clone(), idx);
            by_output_hash.insert(record.output_sha256.clone(), idx);
        }

        Ok(Self {
            algo_id: algo_id.into(),
            records,
            by_prompt_hash,
            by_output_hash,
            current_idx: 0,
        })
    }

    /// Load and construct replayer from a JSONL string.
    pub fn from_jsonl_str(algo_id: impl Into<String>, content: &str) -> Result<Self, ReplayError> {
        let mut records = Vec::new();
        for (idx, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let record: LlmRecord = serde_json::from_str(trimmed)
                .map_err(|e| ReplayError::Json(format!("Line {idx}: {e}")))?;
            record.verify_integrity()?;
            records.push(record);
        }

        Self::new(algo_id, records)
    }

    /// Load and construct replayer from a JSONL file.
    pub fn from_jsonl_file(
        algo_id: impl Into<String>,
        path: impl AsRef<Path>,
    ) -> Result<Self, ReplayError> {
        let records = LlmTrailLogger::read_jsonl(path)?;
        Self::new(algo_id, records)
    }

    /// Number of recorded turns.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// True if no records loaded.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Current playback cursor.
    pub fn current_index(&self) -> usize {
        self.current_idx
    }

    /// Reset playback cursor to beginning.
    pub fn reset(&mut self) {
        self.current_idx = 0;
    }

    /// Look up a record by exact prompt text.
    pub fn lookup_by_prompt(&self, prompt: &str) -> Option<&LlmRecord> {
        let hash = sha256_hex(prompt);
        self.lookup_by_prompt_sha256(&hash)
    }

    /// Look up a record by hex SHA256 of prompt.
    pub fn lookup_by_prompt_sha256(&self, sha: &str) -> Option<&LlmRecord> {
        self.by_prompt_hash.get(sha).map(|&idx| &self.records[idx])
    }

    /// Look up a record by hex SHA256 of raw output.
    pub fn lookup_by_output_sha256(&self, sha: &str) -> Option<&LlmRecord> {
        self.by_output_hash.get(sha).map(|&idx| &self.records[idx])
    }

    /// Advance playback cursor and yield the next recorded turn.
    pub fn next_record(&mut self) -> Option<&LlmRecord> {
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
        while self.current_idx < self.records.len() {
            let idx = self.current_idx;
            self.current_idx += 1;
            if self.records[idx].decision.is_some() {
                return self.records[idx].decision.as_ref();
            }
        }
        None
    }

    /// Convert the next recorded decision into candidate [`OrderRequest`]s.
    pub fn next_orders(&mut self) -> Vec<OrderRequest> {
        while self.current_idx < self.records.len() {
            let idx = self.current_idx;
            self.current_idx += 1;
            if let Some(dec) = &self.records[idx].decision {
                return Self::decision_to_orders(dec, &self.algo_id);
            }
        }
        Vec::new()
    }

    /// Map a domain [`Decision`] into canonical [`OrderRequest`]s.
    pub fn decision_to_orders(decision: &Decision, fallback_algo_id: &str) -> Vec<OrderRequest> {
        match decision {
            Decision::Buy {
                symbol,
                exchange,
                qty,
                price_limit,
                algo_id,
                ..
            } => {
                let inst = InstrumentId::new(*exchange, symbol.clone());
                let aid = if algo_id.trim().is_empty() {
                    fallback_algo_id.to_string()
                } else {
                    algo_id.clone()
                };

                let order_type = match price_limit {
                    Some(limit) => OrderType::Limit(*limit),
                    None => OrderType::Market,
                };
                let mut order = OrderRequest::new(inst, OrderSide::Buy, order_type, *qty, aid);
                order.tag = Some("replay_buy".into());
                vec![order]
            }
            Decision::Sell {
                symbol,
                exchange,
                qty,
                price_limit,
                algo_id,
                ..
            } => {
                let inst = InstrumentId::new(*exchange, symbol.clone());
                let aid = if algo_id.trim().is_empty() {
                    fallback_algo_id.to_string()
                } else {
                    algo_id.clone()
                };

                let order_type = match price_limit {
                    Some(limit) => OrderType::Limit(*limit),
                    None => OrderType::Market,
                };
                let mut order = OrderRequest::new(inst, OrderSide::Sell, order_type, *qty, aid);
                order.tag = Some("replay_sell".into());
                vec![order]
            }
            Decision::Hold { .. } | Decision::CancelAll { .. } => Vec::new(),
        }
    }
}
