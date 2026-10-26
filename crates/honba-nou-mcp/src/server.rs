//! MCP Server and JSON-RPC 2.0 message handling.

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::McpError;
use crate::tools::{
    BacktestDispatcherService, DecisionExplainorService, IndicatorCatalogService,
    MetricsCalculatorService, SpecValidatorService, StrategyOptimizerService,
    list_tool_definitions,
};

/// JSON-RPC 2.0 message envelope.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(untagged)]
pub enum RpcMessage {
    Request {
        id: serde_json::Value,
        method: String,
        #[serde(default)]
        params: serde_json::Value,
    },
    Response {
        id: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<RpcError>,
    },
    Notification {
        method: String,
        #[serde(default)]
        params: serde_json::Value,
    },
}

/// JSON-RPC 2.0 error object.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl RpcError {
    pub fn parse_error(msg: impl Into<String>) -> Self {
        Self {
            code: -32700,
            message: msg.into(),
            data: None,
        }
    }

    pub fn invalid_request(msg: impl Into<String>) -> Self {
        Self {
            code: -32600,
            message: msg.into(),
            data: None,
        }
    }

    pub fn method_not_found(method: &str) -> Self {
        Self {
            code: -32601,
            message: format!("Method not found: {method}"),
            data: None,
        }
    }

    pub fn invalid_params(msg: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: msg.into(),
            data: None,
        }
    }

    pub fn internal_error(msg: impl Into<String>) -> Self {
        Self {
            code: -32603,
            message: msg.into(),
            data: None,
        }
    }
}

/// Model Context Protocol quantitative server.
pub struct McpServer {
    pub indicator_catalog: IndicatorCatalogService,
    pub spec_validator: SpecValidatorService,
    pub backtest_dispatcher: BacktestDispatcherService,
    pub metrics_calculator: MetricsCalculatorService,
    pub strategy_optimizer: StrategyOptimizerService,
    pub decision_explainor: DecisionExplainorService,
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

impl McpServer {
    /// Create a new default MCP server instance.
    pub fn new() -> Self {
        Self {
            indicator_catalog: IndicatorCatalogService::new(),
            spec_validator: SpecValidatorService::new(None),
            backtest_dispatcher: BacktestDispatcherService::new(),
            metrics_calculator: MetricsCalculatorService::new(),
            strategy_optimizer: StrategyOptimizerService::new(),
            decision_explainor: DecisionExplainorService::new(),
        }
    }

    /// Handle an incoming JSON-RPC 2.0 message and generate a response.
    pub async fn handle_message(&self, message: &RpcMessage) -> Option<RpcMessage> {
        match message {
            RpcMessage::Request { id, method, params } => {
                let resp = self.dispatch(method, params.clone(), id.clone()).await;
                Some(resp)
            }
            RpcMessage::Notification { method, params } => {
                // Notifications do not return responses per JSON-RPC 2.0
                let _ = self.dispatch(method, params.clone(), json!(null)).await;
                None
            }
            RpcMessage::Response { .. } => None,
        }
    }

    /// Dispatch a JSON-RPC method call to the appropriate tool handler.
    async fn dispatch(
        &self,
        method: &str,
        params: serde_json::Value,
        id: serde_json::Value,
    ) -> RpcMessage {
        match method {
            "initialize" => {
                let result = json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": { "listChanged": false },
                        "logging": {}
                    },
                    "serverInfo": {
                        "name": "honba-nou",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                });
                RpcMessage::Response {
                    id,
                    result: Some(result),
                    error: None,
                }
            }
            "tools/list" => {
                let tools = list_tool_definitions();
                let result = json!({ "tools": tools });
                RpcMessage::Response {
                    id,
                    result: Some(result),
                    error: None,
                }
            }
            "tools/call" => {
                let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

                match self.call_tool(name, arguments).await {
                    Ok(tool_result) => {
                        let result = json!({
                            "content": [
                                {
                                    "type": "text",
                                    "text": serde_json::to_string_pretty(&tool_result).unwrap_or_default()
                                }
                            ],
                            "structuredContent": tool_result
                        });
                        RpcMessage::Response {
                            id,
                            result: Some(result),
                            error: None,
                        }
                    }
                    Err(err) => RpcMessage::Response {
                        id,
                        result: None,
                        error: Some(RpcError::internal_error(err.to_string())),
                    },
                }
            }
            "ping" => RpcMessage::Response {
                id,
                result: Some(json!({})),
                error: None,
            },
            _ => RpcMessage::Response {
                id,
                result: None,
                error: Some(RpcError::method_not_found(method)),
            },
        }
    }

    /// Call an MCP quantitative tool by name.
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        match name {
            "get_indicator_catalog" => Ok(self.indicator_catalog.get_catalog_json()),
            "check_spec" => {
                let sm = arguments.get("strategy_manifest").ok_or_else(|| {
                    McpError::InvalidArguments("missing strategy_manifest".to_string())
                })?;
                self.spec_validator.validate(sm)
            }
            "run_backtest" => {
                let sm = arguments.get("strategy_manifest").ok_or_else(|| {
                    McpError::InvalidArguments("missing strategy_manifest".to_string())
                })?;
                let rc = arguments
                    .get("run_config")
                    .ok_or_else(|| McpError::InvalidArguments("missing run_config".to_string()))?;
                self.backtest_dispatcher.dispatch(sm, rc)
            }
            "evaluate_metrics" => {
                let tl = arguments
                    .get("trade_log")
                    .ok_or_else(|| McpError::InvalidArguments("missing trade_log".to_string()))?;
                let re = arguments.get("risk_engine_state");
                let cfg = arguments.get("config");
                self.metrics_calculator.compute(tl, re, cfg)
            }
            "optimize_strategy" => {
                let sm = arguments.get("strategy_manifest").ok_or_else(|| {
                    McpError::InvalidArguments("missing strategy_manifest".to_string())
                })?;
                let oc = arguments.get("optimization_config").ok_or_else(|| {
                    McpError::InvalidArguments("missing optimization_config".to_string())
                })?;
                self.strategy_optimizer.optimize(sm, oc)
            }
            "explain_decision" => {
                let decision = arguments
                    .get("decision")
                    .ok_or_else(|| McpError::InvalidArguments("missing decision".to_string()))?;
                let market_context = arguments.get("market_context").ok_or_else(|| {
                    McpError::InvalidArguments("missing market_context".to_string())
                })?;
                let risk_check_results = arguments.get("risk_check_results").ok_or_else(|| {
                    McpError::InvalidArguments("missing risk_check_results".to_string())
                })?;
                self.decision_explainor
                    .explain(decision, market_context, risk_check_results)
            }
            _ => Err(McpError::MethodNotFound(format!("unknown tool: {name}"))),
        }
    }
}
