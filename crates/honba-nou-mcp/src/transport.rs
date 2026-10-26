//! Model Context Protocol transports: Stdio and SSE (Task 3.1).

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::stream::{self, Stream, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{Mutex, mpsc};
use uuid::Uuid;

use crate::error::McpError;
use crate::server::{McpServer, RpcError, RpcMessage};

// ---------------------------------------------------------------------------
// 1. Stdio Transport
// ---------------------------------------------------------------------------

/// Runs the MCP JSON-RPC 2.0 loop over any async reader and writer (e.g. stdin/stdout).
pub async fn run_stdio_transport<R, W>(
    server: Arc<McpServer>,
    mut reader: R,
    mut writer: W,
) -> Result<(), McpError>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).await?;
        if bytes_read == 0 {
            // EOF reached
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<RpcMessage>(trimmed) {
            Ok(msg) => server.handle_message(&msg).await,
            Err(e) => Some(RpcMessage::Response {
                id: json!(null),
                result: None,
                error: Some(RpcError::parse_error(format!("Malformed JSON: {e}"))),
            }),
        };

        if let Some(resp) = response {
            let serialized = serde_json::to_string(&resp)?;
            writer.write_all(serialized.as_bytes()).await?;
            writer.write_all(b"\n").await?;
            writer.flush().await?;
        }
    }

    Ok(())
}

/// Run the stdio server connecting directly to process stdin and stdout.
pub async fn run_stdio_server(server: Arc<McpServer>) -> Result<(), McpError> {
    let stdin = tokio::io::BufReader::new(tokio::io::stdin());
    let stdout = tokio::io::stdout();
    run_stdio_transport(server, stdin, stdout).await
}

// ---------------------------------------------------------------------------
// 2. Server-Sent Events (SSE) & HTTP Transport
// ---------------------------------------------------------------------------

/// Session state for SSE clients.
pub struct SseSessionManager {
    sessions: Mutex<HashMap<String, mpsc::UnboundedSender<String>>>,
}

impl Default for SseSessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SseSessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub async fn register(&self, session_id: String, tx: mpsc::UnboundedSender<String>) {
        self.sessions.lock().await.insert(session_id, tx);
    }

    pub async fn unregister(&self, session_id: &str) {
        self.sessions.lock().await.remove(session_id);
    }

    pub async fn send_message(&self, session_id: &str, message: String) -> Result<(), McpError> {
        let sender = {
            let map = self.sessions.lock().await;
            map.get(session_id).cloned()
        };

        if let Some(tx) = sender {
            tx.send(message)
                .map_err(|e| McpError::Transport(format!("Failed to send SSE event: {e}")))?;
            Ok(())
        } else {
            Err(McpError::SessionNotFound(session_id.to_string()))
        }
    }
}

/// Shared app state for HTTP/SSE routes.
#[derive(Clone)]
pub struct AppState {
    pub server: Arc<McpServer>,
    pub session_manager: Arc<SseSessionManager>,
}

#[derive(Debug, Deserialize)]
pub struct MessageQuery {
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
}

/// Construct the Axum Router conforming to the Model Context Protocol SSE specification.
pub fn create_router(server: Arc<McpServer>) -> Router {
    let session_manager = Arc::new(SseSessionManager::new());
    let state = AppState {
        server,
        session_manager,
    };

    Router::new()
        .route("/sse", get(sse_handler))
        .route("/message", post(message_handler))
        .route("/rpc", post(rpc_handler))
        .route("/health", get(health_handler))
        .with_state(state)
}

/// GET /sse handler conforming to MCP SSE transport specification.
async fn sse_handler(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let session_id = Uuid::new_v4().to_string();
    let (tx, rx) = mpsc::unbounded_channel::<String>();

    state
        .session_manager
        .register(session_id.clone(), tx.clone())
        .await;

    let endpoint_uri = format!("/message?sessionId={session_id}");

    let initial = stream::once(futures::future::ready(Ok(Event::default()
        .event("endpoint")
        .data(endpoint_uri))));

    let stream = stream::unfold(rx, |mut rx| async move {
        rx.recv()
            .await
            .map(|msg| (Ok(Event::default().event("message").data(msg)), rx))
    });

    Sse::new(initial.chain(stream)).keep_alive(KeepAlive::default())
}

/// POST /message handler receiving client JSON-RPC messages for an active SSE session.
async fn message_handler(
    State(state): State<AppState>,
    Query(query): Query<MessageQuery>,
    Json(payload): Json<RpcMessage>,
) -> Response {
    let resp = state.server.handle_message(&payload).await;

    if let (Some(session_id), Some(response_msg)) = (query.session_id, &resp) {
        if let Ok(serialized) = serde_json::to_string(response_msg) {
            let _ = state
                .session_manager
                .send_message(&session_id, serialized)
                .await;
        }
    }

    match resp {
        Some(msg) => Json(msg).into_response(),
        None => axum::http::StatusCode::ACCEPTED.into_response(),
    }
}

/// Direct JSON-RPC POST /rpc endpoint for non-streaming callers.
async fn rpc_handler(
    State(state): State<AppState>,
    Json(payload): Json<RpcMessage>,
) -> impl IntoResponse {
    let resp = state.server.handle_message(&payload).await;
    match resp {
        Some(msg) => Json(msg).into_response(),
        None => Json(json!({ "jsonrpc": "2.0", "result": null })).into_response(),
    }
}

/// GET /health endpoint.
async fn health_handler() -> &'static str {
    "honba-nou-mcp OK"
}
