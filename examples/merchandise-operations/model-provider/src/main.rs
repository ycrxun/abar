#![forbid(unsafe_code)]

use abar_api::context::{RequestContext, TenantContext};
use abar_api::provider::{ModelProvider, ModelRequest, ModelTool};
use abar_merchandise_demo_model::BusinessDemoModelProvider;
use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Clone)]
struct AppState {
    provider: Arc<BusinessDemoModelProvider>,
}

#[derive(Deserialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Value>,
    #[serde(default)]
    tools: Vec<OpenAiTool>,
}

#[derive(Deserialize)]
struct OpenAiTool {
    function: OpenAiFunction,
}

#[derive(Deserialize)]
struct OpenAiFunction {
    name: String,
    #[serde(default)]
    description: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let address: SocketAddr = std::env::var("ABAR_DEMO_MODEL_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:18081".to_owned())
        .parse()?;
    let provider = BusinessDemoModelProvider::new("example-model-v1", std::time::Duration::ZERO);
    let app = Router::new()
        .route("/healthz", get(health))
        .route("/v1/chat/completions", post(complete))
        .with_state(AppState {
            provider: Arc::new(provider),
        });
    let listener = tokio::net::TcpListener::bind(address).await?;
    println!("merchandise demo model listening on http://{address}/v1");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

async fn complete(
    State(state): State<AppState>,
    Json(request): Json<ChatRequest>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let input = request
        .messages
        .iter()
        .find(|message| message.get("role").and_then(Value::as_str) == Some("user"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            (
                axum::http::StatusCode::BAD_REQUEST,
                "messages must contain a user message".to_owned(),
            )
        })?;
    let tools = request
        .tools
        .iter()
        .map(|tool| {
            let id = business_tool_id(&tool.function.description);
            ModelTool {
                id,
                description: tool.function.description.clone(),
                input_schema_json: "{}".to_owned(),
            }
        })
        .collect::<Vec<_>>();
    let provider_request = ModelRequest {
        model_id: request.model.clone(),
        input: input.to_owned(),
        tools,
        tool_results: Vec::new(),
    };
    let context = RequestContext::new("demo-model", TenantContext::new("demo"));
    let response = state
        .provider
        .complete(&context, &provider_request)
        .await
        .map_err(|error| (axum::http::StatusCode::BAD_GATEWAY, error.to_string()))?;
    let tool_calls = response
        .tool_intents
        .into_iter()
        .map(|intent| {
            let name = request
                .tools
                .iter()
                .find(|tool| business_tool_id(&tool.function.description) == intent.tool_id)
                .map(|tool| tool.function.name.clone())
                .ok_or_else(|| {
                    (
                        axum::http::StatusCode::BAD_REQUEST,
                        format!("unknown tool id {}", intent.tool_id),
                    )
                })?;
            Ok(json!({
                "id": format!("call-{name}"),
                "type": "function",
                "function": {
                    "name": name,
                    "arguments": intent.arguments_json
                }
            }))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(json!({
        "model": request.model,
        "choices": [{
            "message": {
                "content": if tool_calls.is_empty() {
                    Value::String(response.output)
                } else {
                    Value::Null
                },
                "tool_calls": tool_calls
            }
        }],
        "usage": {
            "prompt_tokens": response.usage.prompt_tokens,
            "completion_tokens": response.usage.completion_tokens,
            "total_tokens": response.usage.total_tokens
        }
    })))
}

fn business_tool_id(description: &str) -> String {
    let operation = if description.contains("创建") && description.contains("调拨单") {
        "create_transfer_order"
    } else if description.contains("创建") && description.contains("配货单") {
        "create_allocation_order"
    } else if description.contains("创建") && description.contains("补货单") {
        "create_replenishment_order"
    } else if description.contains("配货计划") {
        "calculate_allocation_plan"
    } else if description.contains("补货计划") {
        "calculate_replenishment_plan"
    } else if description.contains("调拨建议") {
        "calculate_transfer_recommendations"
    } else {
        "unknown"
    };
    format!("business.{operation}")
}
