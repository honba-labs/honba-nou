//! LLM provider abstraction.
use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub text: String,
    pub tokens_in: u32,
    pub tokens_out: u32,
}

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("transport: {0}")]
    Transport(String),
    #[error("auth failure")]
    Auth,
    #[error("rate limited")]
    RateLimited,
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, prompt: &str) -> Result<LlmResponse, LlmError>;
}

pub struct OpenAiProvider {
    pub api_key: String,
}

impl OpenAiProvider {
    pub fn new(k: impl Into<String>) -> Self {
        Self { api_key: k.into() }
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    async fn complete(&self, _p: &str) -> Result<LlmResponse, LlmError> {
        Err(LlmError::Transport("not implemented".into()))
    }
}
