//! Prompt cache implementations: local JSONL and SQLite WAL mode.
//!
//! Provides zero-latency, zero-cost prompt cache lookups keyed by SHA-256 prompt hash.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use honba_core::Decision;
use honba_nou_core::sha256_hex;
use rust_decimal::Decimal;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Row, SqlitePool};

use crate::audit::{ChainedLlmAuditRecord, LlmAuditRecord, verify_hash_chain};
use crate::error::ReplayError;

/// Trait for prompt caches keyed by `prompt_sha256`.
pub trait PromptCache: Send + Sync {
    /// Retrieve an audit record by 32-byte prompt SHA-256 digest.
    fn get(&self, prompt_hash: &[u8; 32]) -> Result<Option<LlmAuditRecord>, ReplayError>;

    /// Insert or update an audit record.
    fn put(&mut self, record: &LlmAuditRecord) -> Result<(), ReplayError>;

    /// Retrieve all records stored in this cache.
    fn list_all(&self) -> Result<Vec<LlmAuditRecord>, ReplayError>;
}

/// In-memory prompt cache.
#[derive(Debug, Default, Clone)]
pub struct InMemoryPromptCache {
    records: HashMap<[u8; 32], LlmAuditRecord>,
}

impl InMemoryPromptCache {
    /// Create new empty cache.
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
        }
    }
}

impl PromptCache for InMemoryPromptCache {
    fn get(&self, prompt_hash: &[u8; 32]) -> Result<Option<LlmAuditRecord>, ReplayError> {
        Ok(self.records.get(prompt_hash).cloned())
    }

    fn put(&mut self, record: &LlmAuditRecord) -> Result<(), ReplayError> {
        self.records.insert(record.prompt_sha256, record.clone());
        Ok(())
    }

    fn list_all(&self) -> Result<Vec<LlmAuditRecord>, ReplayError> {
        Ok(self.records.values().cloned().collect())
    }
}

/// JSONL file-backed prompt cache with fast in-memory indexing.
#[derive(Debug, Clone)]
pub struct JsonlPromptCache {
    path: std::path::PathBuf,
    records: HashMap<[u8; 32], LlmAuditRecord>,
}

impl JsonlPromptCache {
    /// Open or create a JSONL cache file at the specified path.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ReplayError> {
        let p = path.as_ref().to_path_buf();
        let mut records = HashMap::new();

        if p.exists() {
            let file = File::open(&p).map_err(|e| ReplayError::Io(e.to_string()))?;
            let reader = BufReader::new(file);

            for (idx, line_res) in reader.lines().enumerate() {
                let line = line_res.map_err(|e| ReplayError::Io(e.to_string()))?;
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let record: LlmAuditRecord = serde_json::from_str(trimmed)
                    .map_err(|e| ReplayError::Json(format!("Line {idx}: {e}")))?;
                record.verify_prompt_integrity()?;
                records.insert(record.prompt_sha256, record);
            }
        }

        Ok(Self { path: p, records })
    }

    /// Write a list of chained records into a JSONL file.
    pub fn write_chained_jsonl(
        path: impl AsRef<Path>,
        chain: &[ChainedLlmAuditRecord],
    ) -> Result<(), ReplayError> {
        verify_hash_chain(chain)?;
        let mut file = File::create(path.as_ref()).map_err(|e| ReplayError::Io(e.to_string()))?;

        for item in chain {
            let json = serde_json::to_string(item).map_err(|e| ReplayError::Json(e.to_string()))?;
            writeln!(file, "{}", json).map_err(|e| ReplayError::Io(e.to_string()))?;
        }
        Ok(())
    }

    /// Read and verify chained records from a JSONL file.
    pub fn read_chained_jsonl(
        path: impl AsRef<Path>,
    ) -> Result<Vec<ChainedLlmAuditRecord>, ReplayError> {
        let file = File::open(path.as_ref()).map_err(|e| ReplayError::Io(e.to_string()))?;
        let reader = BufReader::new(file);
        let mut chain = Vec::new();

        for (idx, line_res) in reader.lines().enumerate() {
            let line = line_res.map_err(|e| ReplayError::Io(e.to_string()))?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let item: ChainedLlmAuditRecord = serde_json::from_str(trimmed)
                .map_err(|e| ReplayError::Json(format!("Line {idx}: {e}")))?;
            chain.push(item);
        }

        verify_hash_chain(&chain)?;
        Ok(chain)
    }
}

impl PromptCache for JsonlPromptCache {
    fn get(&self, prompt_hash: &[u8; 32]) -> Result<Option<LlmAuditRecord>, ReplayError> {
        Ok(self.records.get(prompt_hash).cloned())
    }

    fn put(&mut self, record: &LlmAuditRecord) -> Result<(), ReplayError> {
        record.verify_prompt_integrity()?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| ReplayError::Io(e.to_string()))?;

        let json = serde_json::to_string(record).map_err(|e| ReplayError::Json(e.to_string()))?;
        writeln!(file, "{}", json).map_err(|e| ReplayError::Io(e.to_string()))?;

        self.records.insert(record.prompt_sha256, record.clone());
        Ok(())
    }

    fn list_all(&self) -> Result<Vec<LlmAuditRecord>, ReplayError> {
        Ok(self.records.values().cloned().collect())
    }
}

/// SQLite WAL mode prompt cache.
#[derive(Debug, Clone)]
pub struct SqlitePromptCache {
    pool: SqlitePool,
}

impl SqlitePromptCache {
    /// Connect to SQLite and initialize schema with Write-Ahead Logging (WAL) enabled.
    pub async fn connect(url: &str) -> Result<Self, ReplayError> {
        let options = SqliteConnectOptions::from_str(url)
            .map_err(|e| ReplayError::Database(e.to_string()))?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .map_err(|e| ReplayError::Database(e.to_string()))?;

        let cache = Self { pool };
        cache.init_schema().await?;
        Ok(cache)
    }

    /// Connect to an in-memory SQLite database (ideal for zero-I/O tests).
    pub async fn in_memory() -> Result<Self, ReplayError> {
        Self::connect("sqlite::memory:").await
    }

    /// Create tables and indices for prompt cache.
    async fn init_schema(&self) -> Result<(), ReplayError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS prompt_cache (
                prompt_sha256 BLOB PRIMARY KEY,
                prompt_sha256_hex TEXT NOT NULL,
                timestamp_utc TEXT NOT NULL,
                model TEXT NOT NULL,
                temperature TEXT NOT NULL,
                seed INTEGER NOT NULL,
                market_context_json TEXT NOT NULL,
                raw_prompt TEXT NOT NULL,
                raw_response TEXT NOT NULL,
                parsed_decision_json TEXT NOT NULL,
                sequence_number INTEGER,
                prev_record_hash BLOB,
                record_hash BLOB
            );
            CREATE INDEX IF NOT EXISTS idx_prompt_hex ON prompt_cache(prompt_sha256_hex);
            CREATE INDEX IF NOT EXISTS idx_seq_num ON prompt_cache(sequence_number);
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| ReplayError::Database(e.to_string()))?;

        Ok(())
    }

    /// Put an audit record into SQLite.
    pub async fn put_record(&self, record: &LlmAuditRecord) -> Result<(), ReplayError> {
        record.verify_prompt_integrity()?;
        let decision_json = serde_json::to_string(&record.parsed_decision)
            .map_err(|e| ReplayError::Json(e.to_string()))?;
        let ts_str = record.timestamp_utc.to_rfc3339();
        let temp_str = record.temperature.to_string();
        let prompt_hex = sha256_hex(&record.prompt_sha256);

        sqlx::query(
            r#"
            INSERT OR REPLACE INTO prompt_cache (
                prompt_sha256, prompt_sha256_hex, timestamp_utc, model, temperature,
                seed, market_context_json, raw_prompt, raw_response, parsed_decision_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&record.prompt_sha256[..])
        .bind(prompt_hex)
        .bind(ts_str)
        .bind(&record.model)
        .bind(temp_str)
        .bind(record.seed as i64)
        .bind(&record.market_context_json)
        .bind(&record.raw_prompt)
        .bind(&record.raw_response)
        .bind(decision_json)
        .execute(&self.pool)
        .await
        .map_err(|e| ReplayError::Database(e.to_string()))?;

        Ok(())
    }

    /// Put a chained audit record into SQLite.
    pub async fn put_chained(&self, chained: &ChainedLlmAuditRecord) -> Result<(), ReplayError> {
        chained.record.verify_prompt_integrity()?;
        let decision_json = serde_json::to_string(&chained.record.parsed_decision)
            .map_err(|e| ReplayError::Json(e.to_string()))?;
        let ts_str = chained.record.timestamp_utc.to_rfc3339();
        let temp_str = chained.record.temperature.to_string();
        let prompt_hex = sha256_hex(&chained.record.prompt_sha256);

        sqlx::query(
            r#"
            INSERT OR REPLACE INTO prompt_cache (
                prompt_sha256, prompt_sha256_hex, timestamp_utc, model, temperature,
                seed, market_context_json, raw_prompt, raw_response, parsed_decision_json,
                sequence_number, prev_record_hash, record_hash
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&chained.record.prompt_sha256[..])
        .bind(prompt_hex)
        .bind(ts_str)
        .bind(&chained.record.model)
        .bind(temp_str)
        .bind(chained.record.seed as i64)
        .bind(&chained.record.market_context_json)
        .bind(&chained.record.raw_prompt)
        .bind(&chained.record.raw_response)
        .bind(decision_json)
        .bind(chained.sequence_number as i64)
        .bind(&chained.prev_record_hash[..])
        .bind(&chained.record_hash[..])
        .execute(&self.pool)
        .await
        .map_err(|e| ReplayError::Database(e.to_string()))?;

        Ok(())
    }

    /// Retrieve an audit record by prompt hash.
    pub async fn get_by_prompt_hash(
        &self,
        prompt_hash: &[u8; 32],
    ) -> Result<Option<LlmAuditRecord>, ReplayError> {
        let row_opt = sqlx::query(
            r#"
            SELECT timestamp_utc, model, temperature, seed, market_context_json,
                   raw_prompt, raw_response, parsed_decision_json
            FROM prompt_cache
            WHERE prompt_sha256 = ?
            "#,
        )
        .bind(&prompt_hash[..])
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ReplayError::Database(e.to_string()))?;

        match row_opt {
            Some(row) => {
                let ts_str: String = row.get(0);
                let timestamp_utc = DateTime::parse_from_rfc3339(&ts_str)
                    .map_err(|e| ReplayError::Database(e.to_string()))?
                    .with_timezone(&Utc);
                let model: String = row.get(1);
                let temp_str: String = row.get(2);
                let temperature = Decimal::from_str(&temp_str)
                    .map_err(|e| ReplayError::Database(e.to_string()))?;
                let seed_i64: i64 = row.get(3);
                let market_context_json: String = row.get(4);
                let raw_prompt: String = row.get(5);
                let raw_response: String = row.get(6);
                let decision_json: String = row.get(7);

                let parsed_decision: Decision = serde_json::from_str(&decision_json)
                    .map_err(|e| ReplayError::Json(e.to_string()))?;

                let record = LlmAuditRecord {
                    timestamp_utc,
                    model,
                    temperature,
                    seed: seed_i64 as u64,
                    prompt_sha256: *prompt_hash,
                    market_context_json,
                    raw_prompt,
                    raw_response,
                    parsed_decision,
                };

                Ok(Some(record))
            }
            None => Ok(None),
        }
    }

    /// Retrieve all chained records ordered by sequence_number.
    pub async fn list_chained_sequence(&self) -> Result<Vec<ChainedLlmAuditRecord>, ReplayError> {
        let rows = sqlx::query(
            r#"
            SELECT sequence_number, prev_record_hash, record_hash, prompt_sha256,
                   timestamp_utc, model, temperature, seed, market_context_json,
                   raw_prompt, raw_response, parsed_decision_json
            FROM prompt_cache
            WHERE sequence_number IS NOT NULL
            ORDER BY sequence_number ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ReplayError::Database(e.to_string()))?;

        let mut result = Vec::new();

        for row in rows {
            let seq: i64 = row.get(0);
            let prev_bytes: Vec<u8> = row.get(1);
            let rec_bytes: Vec<u8> = row.get(2);
            let prompt_bytes: Vec<u8> = row.get(3);
            let ts_str: String = row.get(4);
            let timestamp_utc = DateTime::parse_from_rfc3339(&ts_str)
                .map_err(|e| ReplayError::Database(e.to_string()))?
                .with_timezone(&Utc);
            let model: String = row.get(5);
            let temp_str: String = row.get(6);
            let temperature =
                Decimal::from_str(&temp_str).map_err(|e| ReplayError::Database(e.to_string()))?;
            let seed: i64 = row.get(7);
            let market_context_json: String = row.get(8);
            let raw_prompt: String = row.get(9);
            let raw_response: String = row.get(10);
            let decision_json: String = row.get(11);

            let parsed_decision: Decision = serde_json::from_str(&decision_json)
                .map_err(|e| ReplayError::Json(e.to_string()))?;

            let mut prev_record_hash = [0u8; 32];
            prev_record_hash.copy_from_slice(&prev_bytes);

            let mut record_hash = [0u8; 32];
            record_hash.copy_from_slice(&rec_bytes);

            let mut prompt_sha256 = [0u8; 32];
            prompt_sha256.copy_from_slice(&prompt_bytes);

            let record = LlmAuditRecord {
                timestamp_utc,
                model,
                temperature,
                seed: seed as u64,
                prompt_sha256,
                market_context_json,
                raw_prompt,
                raw_response,
                parsed_decision,
            };

            result.push(ChainedLlmAuditRecord {
                sequence_number: seq as u64,
                prev_record_hash,
                record_hash,
                record,
            });
        }

        Ok(result)
    }
}
