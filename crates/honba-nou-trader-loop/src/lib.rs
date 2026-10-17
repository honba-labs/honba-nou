//! Async trader loop per strategy instance.
use honba_kokoro_risk::RiskLimits;
use honba_nou_llm::{LlmError, LlmProvider};
use std::sync::Arc;

pub struct TraderConfig {
    pub name: String,
    pub universe: Vec<String>,
    pub llm: Arc<dyn LlmProvider>,
    pub risk_limits: RiskLimits,
}

pub struct TraderLoop {
    pub config: TraderConfig,
}

impl TraderLoop {
    pub fn new(config: TraderConfig) -> Self {
        Self { config }
    }

    pub async fn run(&self) -> Result<(), LlmError> {
        tracing::info!(name = %self.config.name, "starting trader loop");
        Ok(())
    }
}
