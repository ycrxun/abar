#![forbid(unsafe_code)]

//! OpenAI-compatible Chat Completions provider.

use abar_api::context::RequestContext;
use abar_api::error::ErrorCode;
use abar_api::message::TokenUsage;
use abar_api::provider::{
    BoxFuture, ModelProvider, ModelRequest, ModelResponse, ProviderError, ToolIntent,
};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Duration;

/// OpenAI-compatible model provider.
#[derive(Clone, Debug)]
pub struct OpenAiCompatibleProvider {
    client: reqwest::Client,
    endpoint: String,
    api_key: Option<String>,
    model_id: String,
    parameters: Value,
}

impl OpenAiCompatibleProvider {
    /// Creates a provider.
    #[must_use]
    pub fn new(
        endpoint: impl Into<String>,
        api_key: Option<String>,
        model_id: impl Into<String>,
    ) -> Self {
        Self::with_timeout(endpoint, api_key, model_id, 120)
    }

    /// Creates a provider with an HTTP timeout.
    #[must_use]
    pub fn with_timeout(
        endpoint: impl Into<String>,
        api_key: Option<String>,
        model_id: impl Into<String>,
        timeout_seconds: u64,
    ) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_seconds.max(1)))
            .build()
            .unwrap_or_default();
        Self {
            client,
            endpoint: endpoint.into().trim_end_matches('/').to_owned(),
            api_key,
            model_id: model_id.into(),
            parameters: Value::Null,
        }
    }

    /// Merges provider-specific request parameters.
    #[must_use]
    pub fn with_parameters(mut self, parameters: Option<Value>) -> Self {
        self.parameters = parameters.unwrap_or(Value::Null);
        self
    }
}

impl ModelProvider for OpenAiCompatibleProvider {
    fn complete<'a>(
        &'a self,
        _context: &'a RequestContext,
        request: &'a ModelRequest,
    ) -> BoxFuture<'a, Result<ModelResponse, ProviderError>> {
        Box::pin(async move {
            let mut names = BTreeMap::new();
            let tools: Vec<Value> = request
                .tools
                .iter()
                .enumerate()
                .map(|(index, tool)| {
                    let safe_name = format!("abar_tool_{index}");
                    names.insert(safe_name.clone(), tool.id.clone());
                    let parameters = serde_json::from_str(&tool.input_schema_json)
                        .unwrap_or_else(|_| serde_json::json!({"type": "object"}));
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": safe_name,
                            "description": tool.description,
                            "parameters": parameters
                        }
                    })
                })
                .collect();
            let mut messages = vec![serde_json::json!({
                "role": "user",
                "content": request.input
            })];
            if !request.tool_results.is_empty() {
                let mut calls = Vec::new();
                let mut results = Vec::new();
                for (index, result) in request.tool_results.iter().enumerate() {
                    let call_id = format!("abar_call_{index}");
                    let tool_name = request
                        .tools
                        .iter()
                        .position(|tool| tool.id == result.tool_id)
                        .map_or_else(
                            || "abar_tool_unknown".to_owned(),
                            |index| format!("abar_tool_{index}"),
                        );
                    calls.push(serde_json::json!({
                        "id": call_id,
                        "type": "function",
                        "function": {
                            "name": tool_name,
                            "arguments": "{}"
                        }
                    }));
                    results.push(serde_json::json!({
                        "role": "tool",
                        "tool_call_id": call_id,
                        "content": result.content
                    }));
                }
                messages.push(serde_json::json!({
                    "role": "assistant",
                    "content": "",
                    "tool_calls": calls
                }));
                messages.extend(results);
            }
            let mut body = serde_json::json!({
                "model": request.model_id,
                "messages": messages,
                "stream": false
            });
            if !tools.is_empty() {
                body["tools"] = Value::Array(tools);
                body["tool_choice"] = Value::String("auto".to_owned());
            }
            if let Value::Object(parameters) = &self.parameters {
                for (name, value) in parameters {
                    if !matches!(
                        name.as_str(),
                        "model" | "messages" | "tools" | "tool_choice" | "stream"
                    ) {
                        body[name] = value.clone();
                    }
                }
            }
            let mut http = self
                .client
                .post(format!("{}/chat/completions", self.endpoint))
                .header(CONTENT_TYPE, "application/json");
            if let Some(api_key) = &self.api_key {
                http = http.header(AUTHORIZATION, format!("Bearer {api_key}"));
            }
            let response = http.json(&body).send().await.map_err(http_error)?;
            if !response.status().is_success() {
                let status = response.status();
                let detail = response.text().await.unwrap_or_default();
                let detail = detail.chars().take(2_000).collect::<String>();
                return Err(provider_error(
                    ErrorCode::ModelUnavailable,
                    format!("model returned HTTP {status}: {detail}"),
                ));
            }
            let completion: Completion = response.json().await.map_err(http_error)?;
            let choice = completion.choices.into_iter().next().ok_or_else(|| {
                provider_error(ErrorCode::ModelUnavailable, "model returned no choices")
            })?;
            let output = choice.message.content.unwrap_or_default();
            let mut tool_intents = Vec::new();
            for call in choice.message.tool_calls.unwrap_or_default() {
                let Some(tool_id) = names.get(&call.function.name) else {
                    return Err(provider_error(
                        ErrorCode::ModelUnavailable,
                        format!("model requested unknown tool name {}", call.function.name),
                    ));
                };
                tool_intents.push(ToolIntent {
                    tool_id: tool_id.clone(),
                    arguments_json: call.function.arguments,
                });
            }
            Ok(ModelResponse {
                served_model_id: completion.model.unwrap_or_else(|| self.model_id.clone()),
                output,
                tool_intents,
                usage: completion
                    .usage
                    .map_or_else(TokenUsage::default, |usage| TokenUsage {
                        prompt_tokens: usage.prompt_tokens,
                        completion_tokens: usage.completion_tokens,
                        total_tokens: usage.total_tokens,
                    }),
            })
        })
    }
}

#[derive(Deserialize)]
struct Completion {
    model: Option<String>,
    choices: Vec<CompletionChoice>,
    usage: Option<CompletionUsage>,
}

#[derive(Deserialize)]
struct CompletionUsage {
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
}

#[derive(Deserialize)]
struct CompletionChoice {
    message: CompletionMessage,
}

#[derive(Deserialize)]
struct CompletionMessage {
    content: Option<String>,
    tool_calls: Option<Vec<CompletionToolCall>>,
}

#[derive(Deserialize)]
struct CompletionToolCall {
    function: CompletionFunction,
}

#[derive(Deserialize)]
struct CompletionFunction {
    name: String,
    arguments: String,
}

fn provider_error(code: ErrorCode, message: impl Into<String>) -> ProviderError {
    ProviderError::new(code, message, false)
}

fn http_error(error: reqwest::Error) -> ProviderError {
    provider_error(ErrorCode::ModelUnavailable, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::OpenAiCompatibleProvider;
    use abar_api::context::{RequestContext, TenantContext};
    use abar_api::provider::{ModelProvider, ModelRequest, ModelTool};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn maps_tool_calls_back_to_artifact_ids() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = vec![0_u8; 8192];
            let read = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..read]).into_owned();
            let body = r#"{"model":"served-v1","choices":[{"message":{"content":null,"tool_calls":[{"function":{"name":"abar_tool_0","arguments":"{}"}}]}}],"usage":{"prompt_tokens":11,"completion_tokens":3,"total_tokens":14}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            request.contains(r#""role":"tool""#)
        });
        let provider =
            OpenAiCompatibleProvider::new(format!("http://{address}/v1"), None, "model-v1");
        let context = RequestContext::new("req", TenantContext::new("tenant"));
        let request = ModelRequest {
            model_id: "model-v1".to_owned(),
            input: "hello".to_owned(),
            tools: vec![ModelTool {
                id: "catalog.query-example".to_owned(),
                description: "query".to_owned(),
                input_schema_json: r#"{"type":"object"}"#.to_owned(),
            }],
            tool_results: vec![abar_api::provider::ModelToolResult {
                tool_id: "catalog.query-example".to_owned(),
                content: r#"{"available":7}"#.to_owned(),
            }],
        };

        let response = provider.complete(&context, &request).await.unwrap();

        assert_eq!(response.served_model_id, "served-v1");
        assert_eq!(response.tool_intents[0].tool_id, "catalog.query-example");
        assert_eq!(response.usage.total_tokens, 14);
        assert!(server.await.unwrap());
    }
}
