//! Deterministic prompt and output audit logging with SHA256 checksums,
//! model version tags, and seed tracking.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use honba_core::Decision;

use crate::error::ReplayError;

/// Calculate hex-encoded SHA256 checksum of an input string.
pub fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Immutable, auditable record of an LLM query and response turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmRecord {
    /// Unique identifier for this turn
    pub record_id: String,
    /// UTC timestamp of invocation
    pub timestamp: DateTime<Utc>,
    /// Model version identifier (e.g. "gpt-4o-2024-08-06", "claude-3-5-sonnet-20241022")
    pub model_version: String,
    /// Random seed for deterministic inference (if configured)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// Sampling temperature
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// Exact prompt text sent to LLM
    pub prompt: String,
    /// Hex-encoded SHA256 checksum of the prompt
    pub prompt_sha256: String,
    /// Exact raw output string returned by the LLM
    pub raw_output: String,
    /// Hex-encoded SHA256 checksum of the output
    pub output_sha256: String,
    /// Parsed and validated decision (if parsing succeeded)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<Decision>,
}

impl LlmRecord {
    /// Create a new audit record, automatically computing SHA256 hashes.
    pub fn new(
        prompt: impl Into<String>,
        raw_output: impl Into<String>,
        model_version: impl Into<String>,
        seed: Option<u64>,
        temperature: Option<f64>,
        decision: Option<Decision>,
    ) -> Self {
        let prompt_str = prompt.into();
        let raw_str = raw_output.into();
        let p_sha = sha256_hex(&prompt_str);
        let o_sha = sha256_hex(&raw_str);

        Self {
            record_id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            model_version: model_version.into(),
            seed,
            temperature,
            prompt: prompt_str,
            prompt_sha256: p_sha,
            raw_output: raw_str,
            output_sha256: o_sha,
            decision,
        }
    }

    /// Verify that the stored SHA256 checksums match the actual content.
    pub fn verify_integrity(&self) -> Result<(), ReplayError> {
        let calc_p = sha256_hex(&self.prompt);
        if calc_p != self.prompt_sha256 {
            return Err(ReplayError::ChecksumMismatch {
                record_id: self.record_id.clone(),
                expected: self.prompt_sha256.clone(),
                actual: calc_p,
            });
        }

        let calc_o = sha256_hex(&self.raw_output);
        if calc_o != self.output_sha256 {
            return Err(ReplayError::ChecksumMismatch {
                record_id: self.record_id.clone(),
                expected: self.output_sha256.clone(),
                actual: calc_o,
            });
        }

        Ok(())
    }
}

/// Append-only audit logger for LLM interaction trails.
#[derive(Debug, Default, Clone)]
pub struct LlmTrailLogger;

impl LlmTrailLogger {
    /// Append an audit record to a JSONL file.
    pub fn append_record(path: impl AsRef<Path>, record: &LlmRecord) -> Result<(), ReplayError> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path.as_ref())
            .map_err(|e| ReplayError::Io(e.to_string()))?;

        let json = serde_json::to_string(record).map_err(|e| ReplayError::Json(e.to_string()))?;
        writeln!(file, "{}", json).map_err(|e| ReplayError::Io(e.to_string()))?;
        Ok(())
    }

    /// Write an entire sequence of audit records to a JSONL file.
    pub fn write_jsonl(path: impl AsRef<Path>, records: &[LlmRecord]) -> Result<(), ReplayError> {
        let mut file = File::create(path.as_ref()).map_err(|e| ReplayError::Io(e.to_string()))?;

        for record in records {
            let json =
                serde_json::to_string(record).map_err(|e| ReplayError::Json(e.to_string()))?;
            writeln!(file, "{}", json).map_err(|e| ReplayError::Io(e.to_string()))?;
        }
        Ok(())
    }

    /// Read and verify all audit records from a JSONL file.
    pub fn read_jsonl(path: impl AsRef<Path>) -> Result<Vec<LlmRecord>, ReplayError> {
        let file = File::open(path.as_ref()).map_err(|e| ReplayError::Io(e.to_string()))?;
        let reader = BufReader::new(file);
        let mut records = Vec::new();

        for (idx, line_res) in reader.lines().enumerate() {
            let line = line_res.map_err(|e| ReplayError::Io(e.to_string()))?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let record: LlmRecord = serde_json::from_str(trimmed)
                .map_err(|e| ReplayError::Json(format!("Line {idx}: {e}")))?;
            record.verify_integrity()?;
            records.push(record);
        }

        Ok(records)
    }
}
