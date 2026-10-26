//! Core shared traits for LLM providers, audit storage, and decision generation.

use async_trait::async_trait;
use honba_core::Decision;

use crate::error::LlmError;

/// Core interface for Language Model providers.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Identifier name of the model (e.g. "claude-3-5-sonnet", "gemini-1.5-pro").
    fn model_name(&self) -> &str;

    /// Generate an execution decision from a structured prompt.
    ///
    /// Returns the raw response string and the parsed, validated [`Decision`].
    async fn generate(&self, prompt: &str) -> Result<(String, Decision), LlmError>;

    /// Maximum context window size in tokens.
    fn context_window(&self) -> usize;

    /// Check if the model or provider supports an optional capability.
    fn supports(&self, _feature: &str) -> bool {
        false
    }
}
