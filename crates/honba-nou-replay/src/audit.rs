//! Append-only audit record schema and cryptographic hash chaining.
//!
//! Provides SEBI regulatory compliance by maintaining an immutable, tamper-evident
//! SHA-256 hash chain linking sequential LLM inference turns.

use chrono::{DateTime, Utc};
use honba_core::Decision;
use honba_nou_core::{compute_prompt_hash, sha256_hex};
use rust_decimal::Decimal;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::ReplayError;

/// Append-only audit record schema for an LLM query and response turn.
///
/// Implements exact field requirements from Master Plan §2.1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LlmAuditRecord {
    /// UTC timestamp of inference
    pub timestamp_utc: DateTime<Utc>,
    /// Model identifier name (e.g. "claude-3-5-sonnet", "gemini-1.5-pro")
    pub model: String,
    /// Sampling temperature (exact Decimal, zero floats)
    pub temperature: Decimal,
    /// Random seed for deterministic inference
    pub seed: u64,
    /// SHA-256 hash of the prompt (32 bytes)
    pub prompt_sha256: [u8; 32],
    /// Serialized market context JSON
    pub market_context_json: String,
    /// Exact raw prompt sent to the model
    pub raw_prompt: String,
    /// Raw string completion received from the model
    pub raw_response: String,
    /// Closed, validated [`Decision`] enum
    pub parsed_decision: Decision,
}

impl LlmAuditRecord {
    /// Construct a new audit record, automatically computing the SHA-256 hash of raw_prompt.
    pub fn new(
        timestamp_utc: DateTime<Utc>,
        model: impl Into<String>,
        temperature: Decimal,
        seed: u64,
        market_context_json: impl Into<String>,
        raw_prompt: impl Into<String>,
        raw_response: impl Into<String>,
        parsed_decision: Decision,
    ) -> Self {
        let p_str = raw_prompt.into();
        let prompt_sha256 = compute_prompt_hash(&p_str);

        Self {
            timestamp_utc,
            model: model.into(),
            temperature,
            seed,
            prompt_sha256,
            market_context_json: market_context_json.into(),
            raw_prompt: p_str,
            raw_response: raw_response.into(),
            parsed_decision,
        }
    }

    /// Return lowercase hex string of prompt_sha256.
    pub fn prompt_sha256_hex(&self) -> String {
        sha256_hex(&self.prompt_sha256)
    }

    /// Verify that the stored prompt_sha256 matches the hash of raw_prompt.
    pub fn verify_prompt_integrity(&self) -> Result<(), ReplayError> {
        let calculated = compute_prompt_hash(&self.raw_prompt);
        if calculated != self.prompt_sha256 {
            return Err(ReplayError::ChecksumMismatch {
                record_id: format!("{}:{}", self.model, self.seed),
                expected: self.prompt_sha256_hex(),
                actual: sha256_hex(&calculated),
            });
        }
        Ok(())
    }
}

/// Cryptographically chained audit record linking sequential records via SHA-256.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ChainedLlmAuditRecord {
    /// 0-indexed sequential monotonically increasing sequence number
    pub sequence_number: u64,
    /// Hash of the immediately preceding record (32 zeros for genesis)
    pub prev_record_hash: [u8; 32],
    /// Hash of this record chaining prev_record_hash and inner fields
    pub record_hash: [u8; 32],
    /// Inner audit record payload
    pub record: LlmAuditRecord,
}

impl ChainedLlmAuditRecord {
    /// Return lowercase hex string of record_hash.
    pub fn record_hash_hex(&self) -> String {
        sha256_hex(&self.record_hash)
    }

    /// Return lowercase hex string of prev_record_hash.
    pub fn prev_record_hash_hex(&self) -> String {
        sha256_hex(&self.prev_record_hash)
    }
}

/// Compute the cryptographic record hash chaining prev_hash with record contents.
pub fn compute_record_hash(prev_hash: &[u8; 32], seq: u64, record: &LlmAuditRecord) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(prev_hash);
    hasher.update(&seq.to_be_bytes());
    if let Some(nanos) = record.timestamp_utc.timestamp_nanos_opt() {
        hasher.update(&nanos.to_be_bytes());
    } else {
        hasher.update(&record.timestamp_utc.timestamp().to_be_bytes());
    }
    hasher.update(record.model.as_bytes());
    hasher.update(record.temperature.to_string().as_bytes());
    hasher.update(&record.seed.to_be_bytes());
    hasher.update(&record.prompt_sha256);
    hasher.update(record.market_context_json.as_bytes());
    hasher.update(record.raw_prompt.as_bytes());
    hasher.update(record.raw_response.as_bytes());

    if let Ok(decision_bytes) = serde_json::to_vec(&record.parsed_decision) {
        hasher.update(&decision_bytes);
    }

    let result = hasher.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&result);
    arr
}

/// Verify that a sequence of chained audit records forms an unbroken, untampered hash chain.
pub fn verify_hash_chain(chain: &[ChainedLlmAuditRecord]) -> Result<(), ReplayError> {
    if chain.is_empty() {
        return Ok(());
    }

    let mut expected_prev = [0u8; 32];

    for (idx, item) in chain.iter().enumerate() {
        let expected_seq = idx as u64;
        if item.sequence_number != expected_seq {
            return Err(ReplayError::ChainBroken {
                sequence_number: item.sequence_number,
                expected_prev: format!("seq {expected_seq}"),
                actual_prev: format!("seq {}", item.sequence_number),
            });
        }

        if item.prev_record_hash != expected_prev {
            return Err(ReplayError::ChainBroken {
                sequence_number: item.sequence_number,
                expected_prev: sha256_hex(&expected_prev),
                actual_prev: sha256_hex(&item.prev_record_hash),
            });
        }

        // Verify inner prompt hash
        item.record.verify_prompt_integrity()?;

        // Verify record hash calculation
        let calculated =
            compute_record_hash(&item.prev_record_hash, item.sequence_number, &item.record);
        if calculated != item.record_hash {
            return Err(ReplayError::ChecksumMismatch {
                record_id: format!("chain_seq_{}", item.sequence_number),
                expected: sha256_hex(&item.record_hash),
                actual: sha256_hex(&calculated),
            });
        }

        expected_prev = item.record_hash;
    }

    Ok(())
}

/// Builder and state tracker for appending records into an unbroken hash chain.
#[derive(Debug, Default, Clone)]
pub struct AuditChainLedger {
    chain: Vec<ChainedLlmAuditRecord>,
    last_hash: [u8; 32],
}

impl AuditChainLedger {
    /// Create a new empty audit ledger.
    pub fn new() -> Self {
        Self {
            chain: Vec::new(),
            last_hash: [0u8; 32],
        }
    }

    /// Number of records in chain.
    pub fn len(&self) -> usize {
        self.chain.len()
    }

    /// True if chain is empty.
    pub fn is_empty(&self) -> bool {
        self.chain.is_empty()
    }

    /// Append an audit record to the chain, computing its hash and linking it to the previous record.
    pub fn append(&mut self, record: LlmAuditRecord) -> &ChainedLlmAuditRecord {
        let seq = self.chain.len() as u64;
        let prev_record_hash = self.last_hash;
        let record_hash = compute_record_hash(&prev_record_hash, seq, &record);

        let chained = ChainedLlmAuditRecord {
            sequence_number: seq,
            prev_record_hash,
            record_hash,
            record,
        };

        self.last_hash = record_hash;
        self.chain.push(chained);
        self.chain.last().unwrap()
    }

    /// Get reference to all chained records.
    pub fn records(&self) -> &[ChainedLlmAuditRecord] {
        &self.chain
    }

    /// Verify integrity of entire internal chain.
    pub fn verify(&self) -> Result<(), ReplayError> {
        verify_hash_chain(&self.chain)
    }
}
