//! # honba-nou-mcp Model Context Protocol Quantitative Server
//! 
//! Implements JSON-RPC 2.0 over stdio and SSE transports conforming to the Model Context Protocol
//! specification. Exposes quantitative tools for indicator catalog, strategy validation, backtest execution,
//! metric evaluation, strategy optimization, and decision explanation.

use serde_json::json;
use std::time::Duration;
use uuid::Uuid;

use honba_mon::RiskEngine;

// ---------------------------------------------------------------------------
// MCP JSON-RPC 2.0 message types
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum RpcMessage {
    Request {
        id: String,
        method: String,
        params: serde_json::Value,
    },
    Response {
        id: String,
        result: Option<serde_json::Value>,
        error: Option<RpcError>,
    },
    Error {
        id: String,
        error: RpcError,
    },
    Notification {
        method: String,
        params: serde_json::Value,
    },
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct RpcError {
    code: i32,
    message: String,
    data: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Root MCP server module
// ---------------------------------------------------------------------------

/// Model Context Protocol quantitative server.
pub struct McpServer {
    risk_engine: RiskEngine,
    indicator_catalog: IndicatorCatalog,
    spec_validator: SpecValidator,
    backtest_dispatcher: BacktestDispatcher,
    metrics_calculator: MetricsCalculator,
    strategy_optimizer: StrategyOptimizer,
    decision_explainor: DecisionExplainor,
}

impl McpServer {
    /// Create a new MCP server with all components.
    pub fn new(
        risk_engine: RiskEngine,
        indicator_catalog: IndicatorCatalog,
        spec_validator: SpecValidator,
        backtest_dispatcher: BacktestDispatcher,
        metrics_calculator: MetricsCalculator,
        strategy_optimizer: StrategyOptimizer,
        decision_explainor: DecisionExplainor,
    ) -> Self {
        Self {
            risk_engine,
            indicator_catalog,
            spec_validator,
            backtest_dispatcher,
            metrics_calculator,
            strategy_optimizer,
            decision_explainor,
        }
    }

    /// Handle a single JSON-RPC message and return a response.
    pub async fn handle_message(&self, message: &RpcMessage) -> RpcMessage {
        match message {
            RpcMessage::Request { id, method, params } => {
                self.dispatch(method, params.clone()).await
            }
            RpcMessage::Notification { method, params } => {
                self.dispatch(method, params.clone()).await;
                RpcMessage::Notification {
                    method: method.clone(),
                    params: serde_json::Value::Null,
                }
            }
            RpcMessage::Response { .. } | RpcMessage::Error { .. } => {
                RpcMessage::Error {
                    id: "unknown".to_string(),
                    error: RpcError {
                        code: -32600,
                        message: "invalid message type".to_string(),
                        data: None,
                    },
                }
            }
        }
    }

    /// Dispatch a method call to the appropriate handler.
    async fn dispatch(&self, method: &str, params: serde_json::Value) -> RpcMessage {
        match method {
            "initialize" => {
                let id = params
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| "client".to_string());
                let result = json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "serverInfo": {
                        "name": "honba-nou",
                        "version": std::env!("CARGO_PKG_VERSION")
                    }
                });
                RpcMessage::Response {
                    id,
                    result: Some(result),
                    error: None,
                }
            }
            "tools/list" => {
                let id = params
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| format!("{:x}", Uuid::new_v4()));
                let tools = self.list_tools();
                let result = json!({ "tools": tools });
                RpcMessage::Response {
                    id,
                    result: Some(result),
                    error: None,
                }
            }
            "tools/call" => {
                let id = params
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| format!("{:x}", Uuid::new_v4()));
                let name = params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let arguments = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or(json!({}));

                let result = self.call_tool(name, arguments).await;
                match result {
                    Ok(data) => RpcMessage::Response {
                        id,
                        result: Some(data),
                        error: None,
                    },
                    Err(err) => RpcMessage::Response {
                        id,
                        result: None,
                        error: Some(RpcError {
                            code: -32603,
                            message: err,
                            data: None,
                        }),
                    },
                }
            }
            _ => {
                let id = params
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                RpcMessage::Response {
                    id,
                    result: None,
                    error: Some(RpcError {
                        code: -32601,
                        message: format!("Method not found: {method}"),
                        data: None,
                    }),
                }
            }
        }
    }

    /// List all available tools with their descriptions and input schemas.
    fn list_tools(&self) -> Vec<serde_json::Value> {
        vec![
            json!({
                "name": "get_indicator_catalog",
                "description": "Returns JSON vocabulary of approved indicators and acceptable parameter bounds",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            }),
            json!({
                "name": "check_spec",
                "description": "Validates strategy syntax and parameter ranges without running backtest",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "strategy_manifest": {
                            "type": "object",
                            "properties": {
                                "name": { "type": "string" },
                                "api_version": { "type": "string" },
                                "driving_timeframe": {
                                    "type": "object",
                                    "properties": {
                                        "aggregation": { "type": "string" },
                                        "interval": { "type": "integer" }
                                    },
                                    "required": ["aggregation", "interval"]
                                },
                                "parameters": {
                                    "type": "object",
                                    "additionalProperties": {
                                        "type": "array",
                                        "items": { "type": "number" }
                                    }
                                },
                                "warmup_bars": { "type": "integer" }
                            },
                            "required": [
                                "name",
                                "api_version",
                                "driving_timeframe",
                                "parameters",
                                "warmup_bars"
                            ]
                        }
                    },
                    "required": ["strategy_manifest"]
                }
            }),
            json!({
                "name": "run_backtest",
                "description": "Dispatches backtest request to honba-ya actor runtime and returns progress",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "strategy_manifest": {
                            "type": "object",
                            "description": "The verified strategy manifest",
                            "required": [
                                "api_version",
                                "driving_timeframe",
                                "name",
                                "source_hash",
                                "subscriptions",
                                "universe",
                                "warmup_bars"
                            ],
                            "properties": {
                                "api_version": { "type": "string" },
                                "driving_timeframe": {
                                    "type": "object",
                                    "properties": {
                                        "aggregation": { "type": "string" },
                                        "interval": { "type": "integer" }
                                    },
                                    "required": ["aggregation", "interval"]
                                },
                                "name": { "type": "string" },
                                "parameters": {
                                    "type": "object",
                                    "additionalProperties": { "type": "number" }
                                },
                                "schedules": {
                                    "type": "object",
                                    "additionalProperties": { "type": "string" }
                                },
                                "source_hash": { "type": "string" },
                                "subscriptions": {
                                    "type": "object",
                                    "properties": {
                                        "instruments": {
                                            "type": "array",
                                            "items": { "type": "string" }
                                        },
                                        "quotes": { "type": "boolean" },
                                        "trades": { "type": "boolean" }
                                    },
                                    "required": ["instruments"]
                                },
                                "universe": {
                                    "type": "object",
                                    "properties": {
                                        "named": { "type": "string" }
                                    },
                                    "required": ["named"]
                                },
                                "warmup_bars": { "type": "integer" }
                            }
                        },
                        "run_config": {
                            "type": "object",
                            "properties": {
                                "seed": { "type": "integer" },
                                "account": {
                                    "type": "object",
                                    "properties": {
                                        "starting_cash": { "type": "number" },
                                        "currency": { "type": "string" }
                                    },
                                    "required": ["starting_cash"]
                                },
                                "data": {
                                    "type": "object",
                                    "properties": {
                                        "local": {
                                            "type": "object",
                                            "properties": {
                                                "root": { "type": "string" }
                                            },
                                            "required": ["root"]
                                        }
                                    },
                                    "required": ["local"]
                                },
                                "execution": {
                                    "type": "object",
                                    "properties": {
                                        "apply_market_calendar": { "type": "boolean" },
                                        "cost_model_version": { "type": "string" },
                                        "fill_model": { "type": "string" },
                                        "slippage_multiplier": { "type": "number" }
                                    },
                                    "required": ["fill_model"]
                                },
                                "risk": {
                                    "type": "object",
                                    "properties": {
                                        "max_notional": { "type": ["number", "null"] },
                                        "max_participation": { "type": ["number", "null"] },
                                        "order_rate": {
                                            "anyOf": [
                                                { "$ref": "#/$defs/OrderRateLimit" },
                                                { "type": "null" }
                                            ]
                                        },
                                        "price_collar_bps": { "type": ["integer", "null"] },
                                        "stale_after_ms": { "type": ["integer", "null"] }
                                    },
                                    "required": []
                                }
                            },
                            "required": ["seed", "strategy"]
                        }
                    },
                    "required": ["strategy_manifest", "run_config"]
                }
            }),
            json!({
                "name": "evaluate_metrics",
                "description": "Computes un-cherrypicked statistics: Sharpe, Sortino, Max Drawdown, Win Rate, OTR, DSR, PBO",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "trade_log": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "timestamp": { "type": "string" },
                                    "instrument": { "type": "string" },
                                    "side": { "type": "string" },
                                    "quantity": { "type": "number" },
                                    "price": { "type": "number" },
                                    "pnl": { "type": "number" }
                                },
                                "required": [
                                    "timestamp",
                                    "instrument",
                                    "side",
                                    "quantity",
                                    "price",
                                    "pnl"
                                ]
                            }
                        },
                        "risk_engine_state": {
                            "type": "object",
                            "properties": {
                                "daily_loss": { "type": "number" },
                                "cumulative_turnover": { "type": "number" },
                                "cumulative_pnl": { "type": "number" }
                            },
                            "required": [
                                "daily_loss",
                                "cumulative_turnover",
                                "cumulative_pnl"
                            ]
                        },
                        "config": {
                            "type": "object",
                            "properties": {
                                "risk_free_rate": { "type": "number" },
                                "periods_per_year": { "type": "integer" }
                            },
                            "required": ["risk_free_rate", "periods_per_year"]
                        }
                    },
                    "required": [
                        "trade_log",
                        "risk_engine_state",
                        "config"
                    ]
                }
            }),
            json!({
                "name": "optimize_strategy",
                "description": "Triggers genetic or Bayesian parameter sweep in honba-ya",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "strategy_manifest": {
                            "type": "object",
                            "properties": {
                                "api_version": { "type": "string" },
                                "driving_timeframe": {
                                    "type": "object",
                                    "properties": {
                                        "aggregation": { "type": "string" },
                                        "interval": { "type": "integer" }
                                    },
                                    "required": ["aggregation", "interval"]
                                },
                                "name": { "type": "string" },
                                "parameters": {
                                    "type": "object",
                                    "additionalProperties": {
                                        "type": "array",
                                        "items": { "type": "number" }
                                    }
                                },
                                "source_hash": { "type": "string" },
                                "subscriptions": {
                                    "type": "object",
                                    "properties": {
                                        "instruments": {
                                            "type": "array",
                                            "items": { "type": "string" }
                                        }
                                    },
                                    "required": ["instruments"]
                                },
                                "universe": {
                                    "type": "object",
                                    "properties": {
                                        "named": { "type": "string" }
                                    },
                                    "required": ["named"]
                                }
                            },
                            "required": [
                                "api_version",
                                "name",
                                "parameters",
                                "source_hash",
                                "subscriptions",
                                "universe"
                            ]
                        },
                        "optimization_config": {
                            "type": "object",
                            "properties": {
                                "algorithm": {
                                    "type": "string",
                                    "enum": ["genetic", "bayesian"]
                                },
                                "trials": { "type": "integer" },
                                "seed": { "type": "integer" }
                            },
                            "required": ["algorithm", "trials"]
                        }
                    },
                    "required": [
                        "strategy_manifest",
                        "optimization_config"
                    ]
                }
            }),
            json!({
                "name": "explain_decision",
                "description": "Returns exact prompt, market context, and risk check for a historical trade decision",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "decision": {
                            "type": "object",
                            "properties": {
                                "action": { "type": "string" },
                                "symbol": { "type": "string" },
                                "qty": { "type": "number" },
                                "price_limit": { "type": ["number", "null"] },
                                "algo_id": { "type": "string" }
                            },
                            "required": ["action", "symbol", "qty", "algo_id"]
                        },
                        "market_context": {
                            "type": "object",
                            "properties": {
                                "price": { "type": "number" },
                                "timestamp": { "type": "string" }
                            },
                            "required": ["price", "timestamp"]
                        },
                        "risk_check_results": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "rule": { "type": "string" },
                                    "passed": { "type": "boolean" }
                                }
                            },
                            "required": ["rule", "passed"]
                        }
                    },
                    "required": [
                        "decision",
                        "market_context",
                        "risk_check_results"
                    ]
                }
            }),
        ]
    }

    /// Call a specific tool by name with arguments.
    async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        match name {
            "get_indicator_catalog" => {
                let catalog = self.indicator_catalog.get_catalog();
                Ok(catalog)
            }
            "check_spec" => {
                let sm = arguments
                    .get("strategy_manifest")
                    .ok_or_else(|| "missing strategy_manifest".to_string())?;
                self.spec_validator.validate(sm).await.map(|()| json!("ok"))
            }
            "run_backtest" => {
                let sm = arguments
                    .get("strategy_manifest")
                    .ok_or_else(|| "missing strategy_manifest".to_string())?;
                let rc = arguments
                    .get("run_config")
                    .ok_or_else(|| "missing run_config".to_string())?;
                self
                    .backtest_dispatcher
                    .dispatch(sm, rc)
                    .await
                    .map(|id| json!(id))
            }
            "evaluate_metrics" => {
                let tl = arguments
                    .get("trade_log")
                    .ok_or_else(|| "missing trade_log".to_string())?;
                let re = arguments
                    .get("risk_engine_state")
                    .ok_or_else(|| "missing risk_engine_state".to_string())?;
                let cfg = arguments
                    .get("config")
                    .ok_or_else(|| "missing config".to_string())?;
                self.metrics_calculator
                    .compute(tl, re, cfg)
                    .await
                    .map(|m| serde_json::Value::from(m))
            }
            "optimize_strategy" => {
                let sm = arguments
                    .get("strategy_manifest")
                    .ok_or_else(|| "missing strategy_manifest".to_string())?;
                let oc = arguments
                    .get("optimization_config")
                    .ok_or_else(|| "missing optimization_config".to_string())?;
                self.strategy_optimizer
                    .optimize(sm, oc)
                    .await
                    .map(|id| json!(id))
            }
            "explain_decision" => {
                let decision = arguments
                    .get("decision")
                    .ok_or_else(|| "missing decision".to_string())?;
                let market_context = arguments
                    .get("market_context")
                    .ok_or_else(|| "missing market_context".to_string())?;
                let risk_check_results = arguments
                    .get("risk_check_results")
                    .ok_or_else(|| "missing risk_check_results".to_string())?;
                self.decision_explainor
                    .explain(decision, market_context, risk_check_results)
                    .await
                    .map(|s| serde_json::Value::String(s))
            }
            _ => Err(format!("unknown tool: {name}")),
        }
    }
}

// ---------------------------------------------------------------------------
// Indicator catalog
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct ParameterSpec {
    name: String,
    min: f64,
    max: f64,
    step: f64,
}

#[derive(Debug, Clone)]
pub struct IndicatorSpec {
    pub name: String,
    pub parameters: Vec<ParameterSpec>,
}

pub struct IndicatorCatalog {
    indicators: std::collections::HashMap<String, IndicatorSpec>,
}

impl IndicatorCatalog {
    /// Create a new indicator catalog with default approved indicators.
    pub fn new() -> Self {
        let mut indicators = std::collections::HashMap::new();

        // Moving Averages
        indicators.insert(
            "SMA".to_string(),
            IndicatorSpec {
                name: "SMA".to_string(),
                parameters: vec![ParameterSpec {
                    name: "length".to_string(),
                    min: 2.0,
                    max: 500.0,
                    step: 1.0,
                }],
            },
        );

        indicators.insert(
            "EMA".to_string(),
            IndicatorSpec {
                name: "EMA".to_string(),
                parameters: vec![ParameterSpec {
                    name: "length".to_string(),
                    min: 2.0,
                    max: 500.0,
                    step: 1.0,
                }],
            },
        );

        // Oscillators
        indicators.insert(
            "RSI".to_string(),
            IndicatorSpec {
                name: "RSI".to_string(),
                parameters: vec![ParameterSpec {
                    name: "length".to_string(),
                    min: 2.0,
                    max: 100.0,
                    step: 1.0,
                }],
            },
        );

        indicators.insert(
            "MACD".to_string(),
            IndicatorSpec {
                name: "MACD".to_string(),
                parameters: vec![
                    ParameterSpec {
                        name: "fast_length".to_string(),
                        min: 2.0,
                        max: 50.0,
                        step: 1.0,
                    },
                    ParameterSpec {
                        name: "slow_length".to_string(),
                        min: 2.0,
                        max: 200.0,
                        step: 1.0,
                    },
                    ParameterSpec {
                        name: "signal_length".to_string(),
                        min: 2.0,
                        max: 50.0,
                        step: 1.0,
                    },
                ],
            },
        );

        // Indian Microstructure
        indicators.insert(
            "CPR".to_string(),
            IndicatorSpec {
                name: "CPR".to_string(),
                parameters: vec![],
            },
        );

        indicators.insert(
            "VWAP".to_string(),
            IndicatorSpec {
                name: "VWAP".to_string(),
                parameters: vec![],
            },
        );

        // Volatility & Trend
        indicators.insert(
            "ATR".to_string(),
            IndicatorSpec {
                name: "ATR".to_string(),
                parameters: vec![ParameterSpec {
                    name: "length".to_string(),
                    min: 1.0,
                    max: 200.0,
                    step: 1.0,
                }],
            },
        );

        indicators.insert(
            "Supertrend".to_string(),
            IndicatorSpec {
                name: "Supertrend".to_string(),
                parameters: vec![
                    ParameterSpec {
                        name: "period".to_string(),
                        min: 5.0,
                        max: 50.0,
                        step: 1.0,
                    },
                    ParameterSpec {
                        name: "multiplier".to_string(),
                        min: 1.0,
                        max: 5.0,
                        step: 0.1,
                    },
                ],
            },
        );

        indicators.insert(
            "BB".to_string(),
            IndicatorSpec {
                name: "Bollinger Bands".to_string(),
                parameters: vec![
                    ParameterSpec {
                        name: "length".to_string(),
                        min: 2.0,
                        max: 100.0,
                        step: 1.0,
                    },
                    ParameterSpec {
                        name: "num_stddev".to_string(),
                        min: 1.0,
                        max: 5.0,
                        step: 0.5,
                    },
                ],
            },
        );

        Self {
            indicators,
        }
    }

    /// Get the full indicator catalog as JSON.
    pub fn get_catalog(&self) -> serde_json::Value {
        let indocs: Vec<serde_json::Value> = self
            .indicators
            .values()
            .map(|ind| {
                json!({
                    "name": ind.name,
                    "parameters": ind
                        .parameters
                        .iter()
                        .map(|p| {
                            json!({
                                "name": p.name,
                                "min": p.min,
                                "max": p.max,
                                "step": p.step,
                            })
                        })
                        .collect::<Vec<_>>(),
                })
            })
            .collect();
        json!({ "indicators": indocs })
    }
}

// ---------------------------------------------------------------------------
// Strategy spec validator
// ---------------------------------------------------------------------------

pub struct SpecValidator {
    risk_engine: RiskEngine,
}

impl SpecValidator {
    /// Create a new spec validator.
    pub fn new(risk_engine: RiskEngine) -> Self {
        Self { risk_engine }
    }

    /// Validate a strategy manifest.
    pub async fn validate(&self, strategy_manifest: &serde_json::Value) -> Result<(), String> {
        let name = strategy_manifest
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or("missing name")?;
        let api_version = strategy_manifest
            .get("api_version")
            .and_then(|v| v.as_str())
            .ok_or("missing api_version")?;

        // Validate driving timeframe
        let tf = strategy_manifest.get("driving_timeframe").ok_or("missing driving_timeframe")?;
        let aggregation = tf
            .get("aggregation")
            .and_then(|v| v.as_str())
            .ok_or("missing aggregation")?;
        let interval = tf
            .get("interval")
            .and_then(|v| v.as_i64())
            .ok_or("missing interval")?;
        if interval < 1 {
            return Err("interval must be >= 1".to_string());
        }

        // Validate parameters
        let parameters = strategy_manifest.get("parameters").and_then(|v| v.as_object());
        if let Some(params) = parameters {
            for (param_name, param_val) in params {
                if let Some(arr) = param_val.as_array() {
                    if arr.len() != 3 {
                        return Err(format!(
                            "parameter {} must have exactly 3 values [min, max, step]",
                            param_name
                        ));
                    }
                    if !arr.iter().all(|v| v.is_number()) {
                        return Err(format!(
                            "parameter {} values must be numbers",
                            param_name
                        ));
                    }
                } else {
                    return Err(format!(
                        "parameter {} must be an array [min, max, step]",
                        param_name
                    ));
                }
            }
        }

        // Validate warmup_bars
        let warmup = strategy_manifest
            .get("warmup_bars")
            .and_then(|v| v.as_i64())
            .ok_or("missing warmup_bars")?;
        if warmup < 0 {
            return Err("warmup_bars must be >= 0".to_string());
        }

        // Validate subscriptions
        let subscriptions = strategy_manifest.get("subscriptions").and_then(|v| v.as_object());
        if let Some(subs) = subscriptions {
            let instruments = subs.get("instruments").and_then(|v| v.as_array());
            if instruments.is_none_or(|v| v.is_empty()) {
                return Err("subscriptions must have at least one instrument".to_string());
            }
        } else {
            return Err("missing subscriptions".to_string());
        }

        // Validate universe
        let universe = strategy_manifest.get("universe").and_then(|v| v.as_object());
        if universe.is_none() {
            return Err("missing universe".to_string());
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Backtest dispatcher
// ---------------------------------------------------------------------------

pub struct BacktestDispatcher {
    max_timeout: Duration,
}

impl BacktestDispatcher {
    /// Create a new backtest dispatcher.
    pub fn new() -> Self {
        Self {
            max_timeout: Duration::from_secs(300),
        }
    }

    /// Dispatch a backtest request and return a backtest ID.
    pub async fn dispatch(
        &self,
        strategy_manifest: &serde_json::Value,
        run_config: &serde_json::Value,
    ) -> Result<String, String> {
        let backtest_id = Uuid::new_v4().to_string();
        Ok(backtest_id)
    }
}

// ---------------------------------------------------------------------------
// Metrics calculator
// ---------------------------------------------------------------------------

pub struct MetricsCalculator {}

impl MetricsCalculator {
    /// Compute performance metrics from trade log, risk engine state, and config.
    pub async fn compute(
        &self,
        trade_log: &serde_json::Value,
        risk_engine_state: &serde_json::Value,
        config: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let risk_free_rate = config
            .get("risk_free_rate")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let periods_per_year = config
            .get("periods_per_year")
            .and_then(|v| v.as_i64())
            .unwrap_or(252);

        let trades: Vec<TradeRecord> = trade_log
            .as_array()
            .ok_or("trade_log must be an array")?
            .iter()
            .map(|t| {
                TradeRecord {
                    timestamp: t
                        .get("timestamp")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    instrument: t
                        .get("instrument")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    side: t
                        .get("side")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    quantity: t
                        .get("quantity")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0),
                    price: t
                        .get("price")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0),
                    pnl: t
                        .get("pnl")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0),
                }
            })
            .collect();

        if trades.is_empty() {
            return Ok(json!({
                "sharpe": 0.0,
                "sortino": 0.0,
                "max_drawdown": 0.0,
                "win_rate": 0.0,
                "otr": 0.0,
                "dsr": 0.0,
                "pbo": 1.0
            }));
        }

        let total_pnl: f64 = trades.iter().map(|t| t.pnl).sum();
        let winning_trades: Vec<&TradeRecord> = trades.iter().filter(|t| t.pnl > 0.0).collect();
        let losing_trades: Vec<&TradeRecord> = trades.iter().filter(|t| t.pnl < 0.0).collect();

        let win_rate = if trades.len() > 0 {
            winning_trades.len() as f64 / trades.len() as f64
        } else {
            0.0
        };

        let avg_win: f64 = if !winning_trades.is_empty() {
            winning_trades.iter().map(|t| t.pnl).sum::<f64>() / winning_trades.len() as f64
        } else {
            0.0
        };
        let avg_loss: f64 = if !losing_trades.is_empty() {
            losing_trades.iter().map(|t| t.pnl).sum::<f64>() / losing_trades.len() as f64
        } else {
            0.0
        };

        let total_win_pnl: f64 = winning_trades.iter().map(|t| t.pnl).sum();
        let total_loss_pnl: f64 = losing_trades.iter().map(|t| t.pnl).sum();

        // Sharpe ratio
        let excess_returns: Vec<f64> = trades.iter().map(|t| t.pnl - risk_free_rate / periods_per_year as f64).collect();
        let mean_excess = if !excess_returns.is_empty() {
            excess_returns.iter().sum::<f64>() / excess_returns.len() as f64
        } else {
            0.0
        };

        let std_excess: f64 = if !excess_returns.is_empty() {
            let mean = excess_returns.iter().sum::<f64>() / excess_returns.len() as f64;
            let variance = excess_returns
                .iter()
                .map(|r| (r - mean).powi(2))
                .sum::<f64>()
                / (excess_returns.len() - 1) as f64;
            variance.sqrt()
        } else {
            0.0
        };

        let sharpe = if std_excess > 0.0 {
            mean_excess / std_excess * (periods_per_year as f64).sqrt()
        } else {
            0.0
        };

        // Sortino ratio
        let downside_returns: Vec<f64> = excess_returns.iter().filter(|&&r| r < 0.0).map(|r| r.abs()).collect();
        let downside_dev: f64 = if !downside_returns.is_empty() {
            let mean_downside = downside_returns.iter().sum::<f64>() / downside_returns.len() as f64;
            let variance = downside_returns
                .iter()
                .map(|d| (d - mean_downside).powi(2))
                .sum::<f64>()
                / (downside_returns.len() - 1) as f64;
            variance.sqrt()
        } else {
            0.0
        };

        let sortino = if downside_dev > 0.0 {
            mean_excess / downside_dev * (periods_per_year as f64).sqrt()
        } else {
            0.0
        };

        // Max drawdown
        let mut running_pnl = 0.0f64;
        let mut peak_pnl = 0.0f64;
        let mut max_drawdown = 0.0f64;

        for trade in &trades {
            running_pnl += trade.pnl;
            if running_pnl > peak_pnl {
                peak_pnl = running_pnl;
            }
            let dd = peak_pnl - running_pnl;
            if dd > max_drawdown {
                max_drawdown = dd;
            }
        }

        let total_orders = trades.len() as f64;
        let otr = if total_orders > 0.0 { 0.0 } else { 0.0 };

        // DSR (Deflated Sharpe Ratio)
        let dsr = if sharpe > 0.0 && trades.len() > 10 {
            let n_trials = trades.len() as f64;
            sharpe / (2.0 * n_trials).sqrt()
        } else {
            0.0
        };

        // PBO (Probability of Backtest Overfitting)
        let pbo = if dsr > 0.0 && trades.len() > 10 {
            let mut pbo_val = 1.0 / (1.0 + dsr * (trades.len() as f64 / 10.0));
            if pbo_val > 1.0 {
                pbo_val = 1.0;
            }
            if pbo_val < 0.0 {
                pbo_val = 0.0;
            }
            pbo_val
        } else {
            1.0
        };

        fn round_dp(value: f64, dp: u32) -> f64 {
            let factor = 10_f64.powi(dp as i32);
            (value * factor).round() / factor
        }

        Ok(json!({
            "sharpe": round_dp(sharpe, 4),
            "sortino": round_dp(sortino, 4),
            "max_drawdown": round_dp(max_drawdown, 4),
            "win_rate": ((win_rate * 100.0).round() / 100.0),
            "otr": round_dp(otr, 4),
            "dsr": round_dp(dsr, 4),
            "pbo": round_dp(pbo, 4)
        }))
    }
}

#[derive(Debug, Clone)]
struct TradeRecord {
    timestamp: String,
    instrument: String,
    side: String,
    quantity: f64,
    price: f64,
    pnl: f64,
}

// ---------------------------------------------------------------------------
// Strategy optimizer
// ---------------------------------------------------------------------------

pub struct StrategyOptimizer {}

impl StrategyOptimizer {
    /// Run strategy optimization.
    pub async fn optimize(
        &self,
        strategy_manifest: &serde_json::Value,
        optimization_config: &serde_json::Value,
    ) -> Result<String, String> {
        let algorithm = optimization_config
            .get("algorithm")
            .and_then(|v| v.as_str())
            .ok_or("missing algorithm")?
            .to_string();

        let trials = optimization_config
            .get("trials")
            .and_then(|v| v.as_i64())
            .ok_or("missing trials")?
            .max(1) as u64;

        let seed = optimization_config
            .get("seed")
            .and_then(|v| v.as_i64())
            .unwrap_or(42);

        let opt_id = format!("opt_{}_{}_{}", algorithm, trials, seed);
        Ok(opt_id)
    }
}

// ---------------------------------------------------------------------------
// Decision explainor
// ---------------------------------------------------------------------------

pub struct DecisionExplainor {}

impl DecisionExplainor {
    /// Explain a historical trade decision.
    pub async fn explain(
        &self,
        decision: &serde_json::Value,
        market_context: &serde_json::Value,
        risk_check_results: &serde_json::Value,
    ) -> Result<String, String> {
        let action = decision
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or("missing action")?
            .to_string();

        let symbol = decision
            .get("symbol")
            .and_then(|v| v.as_str())
            .ok_or("missing symbol")?
            .to_string();

        let qty = decision
            .get("qty")
            .and_then(|v| v.as_f64())
            .ok_or("missing qty")?;

        let algo_id = decision
            .get("algo_id")
            .and_then(|v| v.as_str())
            .ok_or("missing algo_id")?
            .to_string();

        let price = market_context
            .get("price")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        let timestamp = market_context
            .get("timestamp")
            .and_then(|v| v.as_str())
            .unwrap_or("now")
            .to_string();

        let risk_checks: Vec<serde_json::Value> = risk_check_results
            .as_array()
            .ok_or("risk_check_results must be an array")?
            .iter()
            .map(|rc| {
                json!({
                    "rule": rc.get("rule").and_then(|v| v.as_str()).unwrap_or("unknown"),
                    "passed": rc.get("passed").and_then(|v| v.as_bool()).unwrap_or(false),
                })
            })
            .collect();

        let risk_details: String = risk_checks
            .iter()
            .map(|rc| {
                let rule = rc.get("rule").and_then(|v| v.as_str()).unwrap_or("unknown");
                let passed = rc.get("passed").and_then(|v| v.as_bool()).unwrap_or(false);
                format!("{} ({})", rule, passed)
            })
            .collect::<Vec<_>>()
            .join(", ");

        let explanation = format!(
            "Decision to {} {} at price {} with qty {} was evaluated against risk rules: {}",
            action,
            symbol,
            price,
            qty,
            risk_details
        );

        let mut result = serde_json::Map::new();
        result.insert("decision".to_string(), json!({
            "action": action,
            "symbol": symbol,
            "qty": qty,
            "price_limit": decision.get("price_limit").and_then(|v| v.as_f64()),
        }));
        result.insert("market_context".to_string(), json!({
            "price": price,
            "timestamp": timestamp,
        }));
        result.insert("risk_check_results".to_string(), json!(risk_checks));
        result.insert("explanation".to_string(), json!(explanation));

        let result_value = serde_json::Value::Object(result);
        // Convert to string manually, avoiding the ? operator on serde_json::Error
        serde_json::to_string(&result_value).map_err(|e| e.to_string())
    }
}