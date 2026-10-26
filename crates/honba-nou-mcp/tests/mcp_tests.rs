//! Integration and Golden Conformance tests for honba-nou-mcp.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use honba_nou_mcp::frontier::ManifestGenerator;
use honba_nou_mcp::server::{McpServer, RpcMessage};
use honba_nou_mcp::tools::list_tool_definitions;
use honba_nou_mcp::transport::{create_router, run_stdio_transport};
use serde_json::json;

#[tokio::test]
async fn test_mcp_initialize() {
    let server = McpServer::new();
    let msg = RpcMessage::Request {
        id: json!(1),
        method: "initialize".to_string(),
        params: json!({}),
    };

    let resp = server
        .handle_message(&msg)
        .await
        .expect("expected response");
    if let RpcMessage::Response { id, result, error } = resp {
        assert_eq!(id, json!(1));
        assert!(error.is_none());
        let res = result.expect("result should exist");
        assert_eq!(res["protocolVersion"], "2024-11-05");
        assert_eq!(res["serverInfo"]["name"], "honba-nou");
        assert!(res["capabilities"]["tools"].is_object());
    } else {
        panic!("unexpected response type");
    }
}

#[tokio::test]
async fn test_mcp_tools_list() {
    let server = McpServer::new();
    let msg = RpcMessage::Request {
        id: json!("req-2"),
        method: "tools/list".to_string(),
        params: json!({}),
    };

    let resp = server
        .handle_message(&msg)
        .await
        .expect("expected response");
    if let RpcMessage::Response { id, result, error } = resp {
        assert_eq!(id, json!("req-2"));
        assert!(error.is_none());
        let res = result.expect("result should exist");
        let tools = res["tools"].as_array().expect("tools array");
        assert!(tools.len() >= 6);

        let tool_names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();

        assert!(tool_names.contains(&"get_indicator_catalog"));
        assert!(tool_names.contains(&"check_spec"));
        assert!(tool_names.contains(&"run_backtest"));
        assert!(tool_names.contains(&"evaluate_metrics"));
        assert!(tool_names.contains(&"optimize_strategy"));
        assert!(tool_names.contains(&"explain_decision"));
    } else {
        panic!("unexpected response type");
    }
}

#[tokio::test]
async fn test_tool_get_indicator_catalog() {
    let server = McpServer::new();
    let msg = RpcMessage::Request {
        id: json!(3),
        method: "tools/call".to_string(),
        params: json!({
            "name": "get_indicator_catalog",
            "arguments": {}
        }),
    };

    let resp = server.handle_message(&msg).await.expect("response");
    if let RpcMessage::Response { result, error, .. } = resp {
        assert!(error.is_none());
        let res = result.unwrap();
        let structured = &res["structuredContent"];
        let indicators = structured["indicators"]
            .as_array()
            .expect("indicators array");
        assert!(indicators.len() >= 10);

        let names: Vec<&str> = indicators
            .iter()
            .filter_map(|i| i["name"].as_str())
            .collect();

        // Verify catalog contains core verified indicators
        assert!(names.contains(&"SMA"));
        assert!(names.contains(&"EMA"));
        assert!(names.contains(&"RSI"));
        assert!(names.contains(&"MACD"));
        assert!(names.contains(&"CPR"));
        assert!(names.contains(&"VWAP"));
        assert!(names.contains(&"ATR"));
        assert!(names.contains(&"SUPERTREND"));
        assert!(names.contains(&"BOLLINGER_BANDS"));
    } else {
        panic!("unexpected response type");
    }
}

#[tokio::test]
async fn test_tool_check_spec_valid_and_invalid() {
    let server = McpServer::new();

    // 1. Valid specification
    let valid_spec = json!({
        "name": "TEST_MOMENTUM_ALGO",
        "driving_timeframe": { "aggregation": "minute", "interval": 5 },
        "warmup_bars": 50,
        "indicators": [
            {
                "name": "RSI",
                "params": { "length": 14.0 }
            },
            {
                "name": "SMA",
                "params": { "length": 50.0 }
            }
        ]
    });

    let msg = RpcMessage::Request {
        id: json!(4),
        method: "tools/call".to_string(),
        params: json!({
            "name": "check_spec",
            "arguments": {
                "strategy_manifest": valid_spec
            }
        }),
    };

    let resp = server.handle_message(&msg).await.expect("response");
    if let RpcMessage::Response { result, error, .. } = resp {
        assert!(error.is_none());
        let res = result.unwrap();
        assert_eq!(res["structuredContent"]["status"], "valid");
    } else {
        panic!("unexpected response type");
    }

    // 2. Invalid specification with unapproved hallucinated indicator
    let invalid_spec = json!({
        "name": "HALLUCINATED_ALGO",
        "indicators": [
            {
                "name": "SECRET_QUANTUM_ORACLE",
                "params": { "alpha": 99.0 }
            }
        ]
    });

    let msg_invalid = RpcMessage::Request {
        id: json!(5),
        method: "tools/call".to_string(),
        params: json!({
            "name": "check_spec",
            "arguments": {
                "strategy_manifest": invalid_spec
            }
        }),
    };

    let resp_invalid = server.handle_message(&msg_invalid).await.expect("response");
    if let RpcMessage::Response { error, .. } = resp_invalid {
        assert!(error.is_some());
        let err = error.unwrap();
        assert!(
            err.message.contains("SECRET_QUANTUM_ORACLE") || err.message.contains("validation")
        );
    } else {
        panic!("expected validation error");
    }
}

#[tokio::test]
async fn test_tool_run_backtest() {
    let server = McpServer::new();
    let msg = RpcMessage::Request {
        id: json!(6),
        method: "tools/call".to_string(),
        params: json!({
            "name": "run_backtest",
            "arguments": {
                "strategy_manifest": {
                    "name": "NIFTY_TREND_V1"
                },
                "run_config": {
                    "seed": 1337,
                    "account": {
                        "starting_cash": 2500000.0,
                        "currency": "INR"
                    },
                    "execution": {
                        "apply_statutory_costs": true,
                        "t_plus_1_settlement": true
                    }
                }
            }
        }),
    };

    let resp = server.handle_message(&msg).await.expect("response");
    if let RpcMessage::Response { result, error, .. } = resp {
        assert!(error.is_none());
        let res = result.unwrap();
        let structured = &res["structuredContent"];
        assert_eq!(structured["status"], "queued");
        assert_eq!(structured["strategy_name"], "NIFTY_TREND_V1");
        assert_eq!(structured["statutory_costs_enforced"], true);
        assert_eq!(structured["settlement_timing"], "T+1");
        assert!(structured["backtest_id"].is_string());
    } else {
        panic!("unexpected response type");
    }
}

#[tokio::test]
async fn test_tool_evaluate_metrics() {
    let server = McpServer::new();

    let trade_log = json!([
        { "timestamp": "2026-01-01T09:15:00Z", "instrument": "RELIANCE.NSE", "side": "BUY", "quantity": 100, "price": 2500.0, "pnl": 1500.0 },
        { "timestamp": "2026-01-02T09:15:00Z", "instrument": "RELIANCE.NSE", "side": "SELL", "quantity": 100, "price": 2515.0, "pnl": -500.0 },
        { "timestamp": "2026-01-03T09:15:00Z", "instrument": "TCS.NSE", "side": "BUY", "quantity": 50, "price": 3800.0, "pnl": 2200.0 },
        { "timestamp": "2026-01-04T09:15:00Z", "instrument": "INFY.NSE", "side": "BUY", "quantity": 75, "price": 1800.0, "pnl": 800.0 }
    ]);

    let msg = RpcMessage::Request {
        id: json!(7),
        method: "tools/call".to_string(),
        params: json!({
            "name": "evaluate_metrics",
            "arguments": {
                "trade_log": trade_log,
                "config": {
                    "risk_free_rate": 0.06,
                    "periods_per_year": 252,
                    "num_trials": 5
                }
            }
        }),
    };

    let resp = server.handle_message(&msg).await.expect("response");
    if let RpcMessage::Response { result, error, .. } = resp {
        assert!(error.is_none());
        let res = result.unwrap();
        let structured = &res["structuredContent"];
        assert_eq!(structured["total_trades"], 4);
        assert_eq!(structured["win_rate"], 75.0);
        assert!(structured["sharpe"].as_f64().is_some());
        assert!(structured["sortino"].as_f64().is_some());
        assert!(structured["max_drawdown"].as_f64().is_some());
        assert!(structured["dsr"].as_f64().is_some());
        assert!(structured["pbo"].as_f64().is_some());
    } else {
        panic!("unexpected response type");
    }
}

#[tokio::test]
async fn test_tool_optimize_strategy() {
    let server = McpServer::new();
    let msg = RpcMessage::Request {
        id: json!(8),
        method: "tools/call".to_string(),
        params: json!({
            "name": "optimize_strategy",
            "arguments": {
                "strategy_manifest": { "name": "CPR_BREAKOUT" },
                "optimization_config": {
                    "algorithm": "genetic",
                    "trials": 50,
                    "seed": 99
                }
            }
        }),
    };

    let resp = server.handle_message(&msg).await.expect("response");
    if let RpcMessage::Response { result, error, .. } = resp {
        assert!(error.is_none());
        let res = result.unwrap();
        let structured = &res["structuredContent"];
        assert_eq!(structured["status"], "started");
        assert_eq!(structured["algorithm"], "genetic");
        assert_eq!(structured["total_trials"], 50);
        assert!(
            structured["job_id"]
                .as_str()
                .unwrap()
                .contains("sweep_genetic_50_99")
        );
    } else {
        panic!("unexpected response type");
    }
}

#[tokio::test]
async fn test_tool_explain_decision() {
    let server = McpServer::new();
    let msg = RpcMessage::Request {
        id: json!(9),
        method: "tools/call".to_string(),
        params: json!({
            "name": "explain_decision",
            "arguments": {
                "decision": {
                    "action": "BUY",
                    "symbol": "HDFCBANK.NSE",
                    "qty": 50.0,
                    "price_limit": 1650.0,
                    "algo_id": "SEBI_ALGO_HDFC_001"
                },
                "market_context": {
                    "price": 1648.5,
                    "timestamp": "2026-10-26T10:00:00Z"
                },
                "risk_check_results": [
                    { "rule": "PriceBandCheck", "passed": true },
                    { "rule": "MaxParticipationLimit", "passed": true },
                    { "rule": "DailyDrawdownLimit", "passed": true }
                ]
            }
        }),
    };

    let resp = server.handle_message(&msg).await.expect("response");
    if let RpcMessage::Response { result, error, .. } = resp {
        assert!(error.is_none());
        let res = result.unwrap();
        let structured = &res["structuredContent"];
        assert_eq!(structured["sebi_algo_id"], "SEBI_ALGO_HDFC_001");
        assert_eq!(structured["passed_pre_trade_risk"], true);
        assert!(
            structured["explanation"]
                .as_str()
                .unwrap()
                .contains("HDFCBANK.NSE")
        );
    } else {
        panic!("unexpected response type");
    }
}

#[tokio::test]
async fn test_stdio_transport() {
    let server = Arc::new(McpServer::new());

    let input_lines = format!(
        "{}\n{}\n",
        serde_json::to_string(&json!({
            "id": 101,
            "method": "initialize",
            "params": {}
        }))
        .unwrap(),
        serde_json::to_string(&json!({
            "id": 102,
            "method": "ping",
            "params": {}
        }))
        .unwrap(),
    );

    let reader = std::io::Cursor::new(input_lines.into_bytes());
    let mut writer = Vec::new();

    run_stdio_transport(server, reader, &mut writer)
        .await
        .expect("stdio transport execution");

    let output_str = String::from_utf8(writer).expect("valid utf8");
    let lines: Vec<&str> = output_str.trim().split('\n').collect();
    assert_eq!(lines.len(), 2);

    let resp1: serde_json::Value = serde_json::from_str(lines[0]).expect("json 1");
    assert_eq!(resp1["id"], 101);
    assert_eq!(resp1["result"]["serverInfo"]["name"], "honba-nou");

    let resp2: serde_json::Value = serde_json::from_str(lines[1]).expect("json 2");
    assert_eq!(resp2["id"], 102);
}

#[tokio::test]
async fn test_sse_and_http_transport() {
    let server = Arc::new(McpServer::new());
    let router = create_router(server);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind listener");
    let addr = listener.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    let client = reqwest::Client::new();

    // 1. Test /health
    let health_resp = client
        .get(format!("http://{addr}/health"))
        .send()
        .await
        .expect("health request");
    assert_eq!(health_resp.status(), reqwest::StatusCode::OK);

    // 2. Test /rpc JSON-RPC call
    let rpc_resp = client
        .post(format!("http://{addr}/rpc"))
        .json(&json!({
            "id": "http-1",
            "method": "tools/list",
            "params": {}
        }))
        .send()
        .await
        .expect("rpc request");

    assert_eq!(rpc_resp.status(), reqwest::StatusCode::OK);
    let rpc_body: serde_json::Value = rpc_resp.json().await.expect("json body");
    assert_eq!(rpc_body["id"], "http-1");
    let tools = rpc_body["result"]["tools"].as_array().expect("tools array");
    assert!(tools.len() >= 6);

    // 3. Test /sse endpoint returns endpoint event
    let sse_resp = client
        .get(format!("http://{addr}/sse"))
        .send()
        .await
        .expect("sse request");
    assert_eq!(sse_resp.status(), reqwest::StatusCode::OK);
    let content_type = sse_resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(content_type.contains("text/event-stream"));

    server_handle.abort();
}

#[tokio::test]
async fn test_frontier_manifest_generation() {
    let tmp_dir = std::env::temp_dir().join(format!("mcp_manifests_{}", uuid::Uuid::new_v4()));
    let paths = ManifestGenerator::write_all(
        &tmp_dir,
        "/usr/local/bin/honba-nou-mcp",
        &["--transport", "stdio"],
        Some("http://127.0.0.1:8000/sse"),
    )
    .expect("write manifests");

    assert!(paths.claude_desktop_path.exists());
    assert!(paths.cursor_path.exists());
    assert!(paths.gemini_path.exists());

    // Verify Claude Desktop format
    let claude_str = fs::read_to_string(&paths.claude_desktop_path).unwrap();
    let claude_val: serde_json::Value = serde_json::from_str(&claude_str).unwrap();
    assert_eq!(
        claude_val["mcpServers"]["honba-nou"]["command"],
        "/usr/local/bin/honba-nou-mcp"
    );

    // Verify Cursor format
    let cursor_str = fs::read_to_string(&paths.cursor_path).unwrap();
    let cursor_val: serde_json::Value = serde_json::from_str(&cursor_str).unwrap();
    assert_eq!(
        cursor_val["mcpServers"]["honba-nou"]["url"],
        "http://127.0.0.1:8000/sse"
    );

    // Verify Gemini CLI format
    let gemini_str = fs::read_to_string(&paths.gemini_path).unwrap();
    let gemini_val: serde_json::Value = serde_json::from_str(&gemini_str).unwrap();
    assert_eq!(
        gemini_val["mcp_servers"]["honba-nou"]["command"],
        "/usr/local/bin/honba-nou-mcp"
    );

    let _ = fs::remove_dir_all(tmp_dir);
}

#[test]
fn test_golden_mcp_tools_schema_conformance() {
    // 1. Verify schema/mcp/mcp_tools.json exists and conforms to MCP tool schema specifications
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut schema_path = manifest_dir.join("schema/mcp/mcp_tools.json");
    if !schema_path.exists() {
        schema_path = manifest_dir.join("../../schema/mcp/mcp_tools.json");
    }
    assert!(
        schema_path.exists(),
        "schema/mcp/mcp_tools.json must exist at {}",
        schema_path.display()
    );

    let content = fs::read_to_string(&schema_path).expect("read mcp_tools.json");
    let schema_val: serde_json::Value = serde_json::from_str(&content).expect("valid json");

    let tools = schema_val["tools"]
        .as_array()
        .expect("tools array in schema");
    assert!(!tools.is_empty(), "tools array must not be empty");

    for tool in tools {
        let name = tool["name"].as_str().expect("tool must have a string name");
        assert!(!name.trim().is_empty(), "tool name cannot be blank");

        let desc = tool["description"]
            .as_str()
            .expect("tool must have description");
        assert!(!desc.trim().is_empty(), "tool description cannot be blank");

        assert!(
            tool["inputSchema"].is_object(),
            "tool {name} must have inputSchema object"
        );
        let schema_type = tool["inputSchema"]["type"].as_str().unwrap_or("");
        assert_eq!(
            schema_type, "object",
            "tool {name} inputSchema must be type object"
        );
    }

    // 2. Verify all quantitative tool definitions in honba-nou-mcp conform to MCP tool schema
    let quantitative_tools = list_tool_definitions();
    assert!(quantitative_tools.len() >= 6);

    for tool in &quantitative_tools {
        let name = tool["name"].as_str().expect("name");
        let desc = tool["description"].as_str().expect("description");
        assert!(!name.is_empty());
        assert!(!desc.is_empty());
        assert_eq!(tool["inputSchema"]["type"], "object");
    }
}
