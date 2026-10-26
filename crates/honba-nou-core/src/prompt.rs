//! Prompt templates and deterministic SHA-256 prompt hashing.

use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A 32-byte cryptographic SHA-256 digest of a prompt payload.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct PromptHash(pub [u8; 32]);

impl PromptHash {
    /// Compute the SHA-256 hash of a prompt string.
    pub fn compute(prompt: &str) -> Self {
        Self(compute_prompt_hash(prompt))
    }

    /// Return lowercase hex string representation.
    pub fn to_hex(&self) -> String {
        sha256_hex(&self.0)
    }

    /// Access raw 32 bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Display for PromptHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// Compute the raw 32-byte SHA-256 digest of a prompt text string.
pub fn compute_prompt_hash(prompt: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(prompt.as_bytes());
    let result = hasher.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&result);
    arr
}

/// Compute hex-encoded SHA-256 string for arbitrary bytes.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(64);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(hex, "{:02x}", byte);
    }
    hex
}

/// Compute hex-encoded SHA-256 string for a prompt string.
pub fn hash_prompt_hex(prompt: &str) -> String {
    let hash = compute_prompt_hash(prompt);
    sha256_hex(&hash)
}

/// Dynamic prompt template supporting key-value substitution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PromptTemplate {
    /// Template identifier name
    pub name: String,
    /// Body template containing `{{var}}` placeholders
    pub body: String,
}

impl PromptTemplate {
    /// Create a new prompt template.
    pub fn new(name: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            body: body.into(),
        }
    }

    /// Replace single `{{context}}` variable.
    pub fn render(&self, ctx: &str) -> String {
        self.body.replace("{{context}}", ctx)
    }

    /// Render template with arbitrary key-value mappings.
    pub fn render_map(&self, vars: &HashMap<&str, &str>) -> String {
        let mut rendered = self.body.clone();
        for (k, v) in vars {
            let placeholder = format!("{{{{{}}}}}", k);
            rendered = rendered.replace(&placeholder, v);
        }
        rendered
    }
}
