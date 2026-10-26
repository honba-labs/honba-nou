//! Unit tests for honba-nou-llm.

use chrono::Utc;
use honba_core::Decision;
use honba_nou_llm::{
    AuditRecorder, InMemoryAuditRecorder, LlmAuditRecord, hash_prompt, validate_record,
};
use rust_decimal_macros::dec;

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
        timestamp_utc: Utc::now(),
        model: "test-model".to_string(),
        temperature: dec!(0.7),
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
        timestamp_utc: Utc::now(),
        model: "test".to_string(),
        temperature: dec!(0.5),
        seed: 1,
        prompt_sha256: hash_prompt("test"),
        raw_prompt: "test".to_string(),
        raw_response: "test".to_string(),
        parsed_decision: Decision::Hold { reason: None },
    };

    recorder.record(&record).unwrap();
    let retrieved = recorder.retrieve(&hash_prompt("test")).unwrap();
    assert_eq!(retrieved.parsed_decision, record.parsed_decision);
}
