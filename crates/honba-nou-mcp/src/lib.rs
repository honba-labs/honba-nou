#![allow(clippy::collapsible_if)]

//! # honba-nou-mcp Model Context Protocol Quantitative Server
//!
//! Implements JSON-RPC 2.0 over Stdio and SSE transports conforming to the Model Context Protocol
//! specification (Task 3.1). Exposes quantitative tools for indicator catalog, strategy validation,
//! backtest execution, metric evaluation, strategy optimization, and decision explanation (Task 3.2).
//! Generates configuration manifests for Claude Desktop, Cursor, and Gemini CLI (Task 3.3).

pub mod error;
pub mod frontier;
pub mod server;
pub mod tools;
pub mod transport;

pub use error::McpError;
pub use frontier::{
    AgentManifestPaths, ClaudeDesktopConfig, CursorMcpConfig, GeminiMcpConfig, ManifestGenerator,
};
pub use server::{McpServer, RpcError, RpcMessage};
pub use tools::{
    BacktestDispatcherService, DecisionExplainorService, IndicatorCatalogService,
    MetricsCalculatorService, SpecValidatorService, StrategyOptimizerService,
    list_tool_definitions,
};
pub use transport::{
    AppState, SseSessionManager, create_router, run_stdio_server, run_stdio_transport,
};
