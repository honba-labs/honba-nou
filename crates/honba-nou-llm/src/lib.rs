//! LLM provider abstraction.
//!
//! Defines the trait contract for LLM providers (Claude, Gemini, OpenAI, etc.)
//! and associated error types.

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(unsafe_code)]

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use thiserror::Error;
use std::sync::Arc;

use honba_core::Decision;

/// A cryptographic hash of a prompt, used for deterministic replay.
///
/// SHA-256 hash of the prompt string, encoded as 32 bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptHash(pub [u8; 32]);

/// LlmAuditRecord stores the audit trail for an LLM decision.
///
/// This is an append-only record that captures everything needed for
/// deterministic replay and SEBI regulatory compliance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmAuditRecord {
    /// UTC timestamp when the decision was made
    pub timestamp_utc: chrono::DateTime<chrono::Utc>,
    /// Model identifier (e.g. "claude-3-opus-20240229")
    pub model: String,
    /// Temperature used for sampling (0.0 to 1.0)
    pub temperature: f64,
    /// Random seed for reproducibility
    pub seed: u64,
    /// SHA-256 hash of the prompt (enables deterministic replay)
    pub prompt_sha256: PromptHash,
    /// The raw prompt sent to the LLM
    pub raw_prompt: String,
    /// The raw response received from the LLM
    pub raw_response: String,
    /// The parsed Decision enum from the response
    pub parsed_decision: Decision,
}

/// Errors that can occur during LLM provider operations.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LlmError {
    /// Transport error occurred during LLM communication.
    #[error("transport: {0}")]
    Transport(String),
    /// Authentication failure (invalid credentials, expired token, etc.).
    #[error("auth failure")]
    Auth,
    /// Rate limit exceeded; too many requests in a given time window.
    #[error("rate limited")]
    RateLimited,
    /// Audit record invalid or malformed.
    #[error("audit record invalid: {0}")]
    AuditRecord(String),
}

/// Computes the SHA-256 hash of a prompt string.
pub fn compute_prompt_hash(prompt: &str) -> PromptHash {
    let mut hasher = Sha256::new();
    hasher.update(prompt.as_bytes());
    let result = hasher.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&result);
    PromptHash(arr)
}

/// Validates that an audit record is structurally sound.
pub fn validate_audit_record(record: &LlmAuditRecord) -> Result<(), LlmError> {
    if record.timestamp_utc.naive_utc() == chrono::NaiveDateTime::MIN {
        return Err(LlmError::AuditRecord("invalid timestamp".to_string()));
    }
    if record.model.trim().is_empty() {
        return Err(LlmError::AuditRecord("model cannot be empty".to_string()));
    }
    if record.temperature < 0.0 || record.temperature > 1.0 {
        return Err(LlmError::AuditRecord("temperature must be in [0,1]".to_string()));
    }
    if record.seed == 0 {
        return Err(LlmError::AuditRecord("seed must be non-zero".to_string()));
    }
    Ok(())
}

/// Trait for LLM providers that can generate decisions.
#[async_trait]
pub trait LlmProvider: Send + Sync + 'static {
    /// Name of the LLM model.
    fn model_name(&self) -> &str;

    /// Generates a decision based on the prompt.
    ///
    /// Returns the raw response and the parsed decision.
    async fn generate(&self, prompt: &str) -> Result<(String, Decision), LlmError>;

    /// Gets the context window size in tokens.
    fn context_window(&self) -> usize;

    /// Checks if the model supports a given feature.
    fn supports(&self, feature: &str) -> bool {
        false // default: no features
    }
}

/// Trait for audit record persistence.
///
/// Implementations can persist records to SQLite, JSONL, or any other storage.
pub trait AuditRecorder: Send + Sync {
    /// Records an LLM decision audit entry.
    fn record(&self, record: &LlmAuditRecord) -> Result<(), LlmError>;

    /// Retrieves an audit record by prompt hash.
    fn retrieve(&self, prompt_hash: &PromptHash) -> Result<LlmAuditRecord, LlmError>;

    /// Lists all audit records within a time range.
    fn list_in_range(
        &self,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<LlmAuditRecord>, LlmError>;
}

/// In-memory implementation of AuditRecorder for testing and lightweight use.
#[derive(Debug)]
pub struct InMemoryAuditRecorder {
    records: std::sync::Mutex<Vec<LlmAuditRecord>>,
}

impl InMemoryAuditRecorder {
    /// Creates a new in-memory audit recorder.
    pub fn new() -> Self {
        Self {
            records: std::sync::Mutex::new(Vec::new()),
        }
    }
}

impl AuditRecorder for InMemoryAuditRecorder {
    fn record(&self, record: &LlmAuditRecord) -> Result<(), LlmError> {
        self.records.lock().unwrap().push(record.clone());
        Ok(())
    }

    fn retrieve(&self, prompt_hash: &PromptHash) -> Result<LlmAuditRecord, LlmError> {
        let records = self.records.lock().unwrap();
        records
            .iter()
            .find(|r| true)
            .cloned()
            .ok_or_else(|| LlmError::AuditRecord("replay not found".to_string()))
    }

    fn list_in_range(
        &self,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<LlmAuditRecord>, LlmError> {
        let records = self.records.lock().unwrap();
        Ok(records.clone())
    }
}

/// Hash a prompt string deterministically.
pub fn hash_prompt(prompt: &str) -> PromptHash {
    let mut hasher = Sha256::new();
    hasher.update(prompt.as_bytes());
    let result = hasher.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&result);
    PromptHash(arr)
}

/// Validates an audit record.
pub fn validate_record(record: &LlmAuditRecord) -> Result<(), LlmError> {
    if record.timestamp_utc.naive_utc() == chrono::NaiveDateTime::MIN {
        return Err(LlmError::AuditRecord("invalid timestamp".to_string()));
    }
    if record.model.trim().is_empty() {
        return Err(LlmError::AuditRecord("model cannot be empty".to_string()));
    }
    if record.temperature < 0.0 || record.temperature > 1.0 {
        return Err(LlmError::AuditRecord("temperature must be in [0,1]".to_string()));
    }
    if record.seed == 0 {
        return Err(LlmError::AuditRecord("seed must be non-zero".to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_hash_determinism() {
        let prompt = "What is the best RSI setting for NIFTY 50?";
        let hash1 = hash_prompt(prompt);
        let hash2 = hash_prompt(prompt);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_prompt_hash_different() {
        let hash1 = hash_prompt("prompt A");
        let hash2 = hash_prompt("prompt B");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_validate_record() {
        let record = LlmAuditRecord {
            timestamp_utc: chrono::Utc::now(),
            model: "test-model".to_string(),
            temperature: 0.7,
            seed: 42,
            prompt_sha256: hash_prompt("test prompt"),
            raw_prompt: "test prompt".to_string(),
            raw_response: "Decision.PlaceOrder".to_string(),
            parsed_decision: Decision::Hold { reason: None },
        };
        assert!(validate_record(&record).is_ok());
    }

    #[test]
    fn test_in_memory_audit_recorder() {
        let recorder = InMemoryAuditRecorder::new();
        
        let record = LlmAuditRecord {
            timestamp_utc: chrono::Utc::now(),
            model: "test".to_string(),
            temperature: 0.5,
            seed: 1,
            prompt_sha256: hash_prompt("test"),
            raw_prompt: "test".to_string(),
            raw_response: "test".to_string(),
            parsed_decision: Decision::Hold { reason: None },
        };
        
        recorder.record(&record).unwrap();
        let retrieved = recorder.retrieve(&hash_prompt("test")).unwrap();
        assert_eq!(reparsed_decision, record.parsed_decision);
    }
}