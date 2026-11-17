#![forbid(unsafe_code)]

use abar_mock_mcp_server::{MockCatalog, MockProfile};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Clone)]
struct AppState {
    catalog: Arc<MockCatalog>,
    profile: MockProfile,
}

#[derive(Deserialize)]
struct RpcRequest {
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let address: SocketAddr = std::env::var("ABAR_MOCK_MCP_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:18080".to_owned())
        .parse()?;
    let profile = MockProfile::parse(
        &std::env::var("ABAR_MOCK_PROFILE").unwrap_or_else(|_| "normal".to_owned()),
    )?;
    let state = AppState {
        catalog: Arc::new(MockCatalog::new()),
        profile,
    };
    let app = Router::new()
        .route("/healthz", get(health))
        .route("/reports/merchandise-anomalies", get(merchandise_anomalies))
        .route("/mcp", post(mcp))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(address).await?;
    println!("mock MCP server listening on http://{address}/mcp");
    axum::serve(listener, app).await?;
    Ok(())
}

#[derive(Deserialize)]
struct ReportQuery {
    limit: Option<usize>,
}

async fn merchandise_anomalies(
    State(state): State<AppState>,
    Query(query): Query<ReportQuery>,
) -> Json<Value> {
    Json(
        state
            .catalog
            .merchandise_anomalies(query.limit.unwrap_or(5)),
    )
}

async fn health(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "status": "ok",
        "dataset": "retail-transfer-v1",
        "stores": 300,
        "orders": state.catalog.order_count(),
        "defaultProfile": state.profile.name(),
        "profiles": MockProfile::list()
    }))
}

async fn mcp(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<RpcRequest>,
) -> Response {
    let Some(id) = request.id else {
        return StatusCode::ACCEPTED.into_response();
    };
    match request.method.as_str() {
        "initialize" => success(
            id,
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {"tools": {}},
                "serverInfo": {
                    "name": "abar-transfer-mock",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        ),
        "ping" => success(id, json!({})),
        "tools/list" => success(id, json!({"tools": state.catalog.tool_definitions()})),
        "tools/call" => {
            let params = request.params.unwrap_or_else(|| json!({}));
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return rpc_error(id, -32602, "tools/call requires params.name");
            };
            let mut arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let refresh_snapshot = arguments
                .get("refreshSnapshot")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if let Some(arguments) = arguments.as_object_mut() {
                arguments.remove("refreshSnapshot");
            }
            let idempotency_key = headers
                .get("idempotency-key")
                .and_then(|value| value.to_str().ok());
            let metadata = params.get("_meta");
            let profile = match headers
                .get("x-abar-mock-profile")
                .and_then(|value| value.to_str().ok())
                .map(MockProfile::parse)
                .transpose()
            {
                Ok(profile) => profile.unwrap_or(state.profile),
                Err(error) => return rpc_error(id, -32602, &error),
            };
            if profile == MockProfile::Unavailable {
                return rpc_error(id, -32000, "simulated ERP/WMS provider unavailable");
            }
            let delay = profile.delay();
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            if std::env::var("ABAR_MOCK_LOG_CALLS").is_ok_and(|value| value == "true") {
                eprintln!(
                    "{}",
                    json!({
                        "event": "mock.tool_call",
                        "tool": name,
                        "profile": profile.name(),
                        "arguments": arguments
                    })
                );
            }
            match state
                .catalog
                .call_tool(name, &arguments, idempotency_key, metadata)
            {
                Ok(mut result) => {
                    profile.transform(&mut result);
                    if result.get("dataAsOf").is_some()
                        && let Some(session_id) = headers
                            .get("x-abar-session-id")
                            .and_then(|value| value.to_str().ok())
                    {
                        let (snapshot_id, snapshot_version) =
                            state.catalog.snapshot(session_id, refresh_snapshot);
                        result["snapshotId"] = Value::String(snapshot_id.clone());
                        result["snapshotVersion"] = json!(snapshot_version);
                        if let Some(confirmation) = result
                            .get_mut("confirmation")
                            .and_then(Value::as_object_mut)
                        {
                            confirmation
                                .insert("snapshotId".to_owned(), Value::String(snapshot_id));
                        }
                    }
                    let text = serde_json::to_string(&result)
                        .unwrap_or_else(|_| "{\"error\":\"serialization failed\"}".to_owned());
                    success(
                        id,
                        json!({
                            "content": [{"type": "text", "text": text}],
                            "structuredContent": result,
                            "isError": false
                        }),
                    )
                }
                Err(error) => rpc_error(id, -32602, &error),
            }
        }
        method => rpc_error(id, -32601, &format!("unknown method: {method}")),
    }
}

fn success(id: Value, result: Value) -> Response {
    Json(json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    }))
    .into_response()
}

fn rpc_error(id: Value, code: i64, message: &str) -> Response {
    Json(json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    }))
    .into_response()
}
