//! Error types for the MCP quantitative server.

/// MCP-specific errors.
#[derive(Debug, thiserror::Error)]
pub enum McpError {
    /// JSON parse error
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    /// Method not found
    #[error("Method not found: {0}")]
    MethodNotFound(String),

    /// Invalid arguments
    #[error("Invalid arguments: {0}")]
    InvalidArguments(String),

    /// Backtest execution failed
    #[error("Backtest execution failed: {0}")]
    BacktestFailed(String),

    /// Strategy validation failed
    #[error("Strategy validation failed: {0}")]
    ValidationFailed(String),

    /// Indicator catalog error
    #[error("Indicator catalog error: {0}")]
    IndicatorError(String),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),

    /// Transport error
    #[error("Transport error: {0}")]
    Transport(String),

    /// Session not found
    #[error("Session not found: {0}")]
    SessionNotFound(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
