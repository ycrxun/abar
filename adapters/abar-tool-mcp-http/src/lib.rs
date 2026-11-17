#![forbid(unsafe_code)]

//! MCP Streamable HTTP tool registry.

use abar_api::context::RequestContext;
use abar_api::error::ErrorCode;
use abar_api::provider::{
    BoxFuture, ProviderError, ToolCallRequest, ToolCallResult, ToolCallStatus, ToolDescriptor,
    ToolEffect, ToolRegistry,
};
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderName};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// MCP server connection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub name: String,
    pub endpoint: String,
    pub auth_ref: Option<String>,
    #[serde(default)]
    pub idempotency_check_tool: Option<String>,
    pub tools: BTreeMap<String, ToolEffect>,
}

/// MCP client implementing ToolRegistry.
#[derive(Clone, Debug)]
pub struct McpHttpToolRegistry {
    servers: BTreeMap<String, McpServerConfig>,
    client: reqwest::Client,
}

impl McpHttpToolRegistry {
    /// Creates a registry for named MCP servers.
    #[must_use]
    pub fn new(servers: Vec<McpServerConfig>) -> Self {
        Self {
            servers: servers
                .into_iter()
                .map(|server| (server.name.clone(), server))
                .collect(),
            client: reqwest::Client::new(),
        }
    }

    async fn discover(&self, server: &McpServerConfig) -> Result<Vec<McpTool>, ProviderError> {
        let request = self.request(server)?;
        let session_id = self
            .initialize(server, request.try_clone().expect("cloneable"))
            .await?;
        let mut list = request;
        if let Some(session_id) = &session_id {
            list = list.header("mcp-session-id", session_id);
        }
        let response = list
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/list",
                "params": {}
            }))
            .send()
            .await
            .map_err(http_error)?;
        let result = parse_rpc(response).await?;
        serde_json::from_value(result.get("tools").cloned().unwrap_or(Value::Array(vec![])))
            .map_err(|error| provider_error(ErrorCode::ToolUnavailable, error.to_string()))
    }

    fn request(&self, server: &McpServerConfig) -> Result<reqwest::RequestBuilder, ProviderError> {
        let mut request = self
            .client
            .post(&server.endpoint)
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json, text/event-stream");
        if let Some(auth_ref) = &server.auth_ref {
            let variable = auth_ref.strip_prefix("env://").ok_or_else(|| {
                provider_error(ErrorCode::ToolUnavailable, "authRef must use env://")
            })?;
            let token = std::env::var(variable).map_err(|_| {
                provider_error(
                    ErrorCode::ToolUnavailable,
                    format!("missing environment variable {variable}"),
                )
            })?;
            request = request.header(AUTHORIZATION, format!("Bearer {token}"));
        }
        Ok(request)
    }

    async fn initialize(
        &self,
        server: &McpServerConfig,
        request: reqwest::RequestBuilder,
    ) -> Result<Option<String>, ProviderError> {
        let _ = server;
        let response = request
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": {"name": "abar-run", "version": env!("CARGO_PKG_VERSION")}
                }
            }))
            .send()
            .await
            .map_err(http_error)?;
        let session_id = response
            .headers()
            .get("mcp-session-id")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let _ = parse_rpc(response).await?;
        Ok(session_id)
    }
}

impl ToolRegistry for McpHttpToolRegistry {
    fn resolve_for_revision<'a>(
        &'a self,
        _context: &'a RequestContext,
        _revision: &'a abar_api::revision::AgentRevision,
    ) -> BoxFuture<'a, Result<Vec<ToolDescriptor>, ProviderError>> {
        Box::pin(async move { self.list_all().await })
    }

    fn call<'a>(
        &'a self,
        context: &'a RequestContext,
        request: &'a ToolCallRequest,
    ) -> BoxFuture<'a, Result<ToolCallResult, ProviderError>> {
        Box::pin(async move {
            let (server_name, tool_name) = request.tool_id.split_once('.').ok_or_else(|| {
                provider_error(ErrorCode::InvalidRequest, "tool id must be server.tool")
            })?;
            let server = self.servers.get(server_name).ok_or_else(|| {
                provider_error(ErrorCode::ToolUnavailable, "MCP server is not configured")
            })?;
            let arguments: Value =
                serde_json::from_str(&request.arguments_json).map_err(|error| {
                    provider_error(
                        ErrorCode::InvalidRequest,
                        format!("invalid tool arguments: {error}"),
                    )
                })?;
            let request_builder = self.request(server)?;
            let session_id = self
                .initialize(
                    server,
                    request_builder.try_clone().expect("request cloneable"),
                )
                .await?;
            let mut params = serde_json::json!({
                "name": tool_name,
                "arguments": arguments
            });
            if let Some(approval) = &request.approval {
                params["_meta"] = serde_json::json!({
                    "abarApproval": {
                        "approvalId": approval.approval_id,
                        "actor": approval.actor,
                        "scopes": approval.scopes
                    }
                });
            }
            let mut call = request_builder
                .header("x-abar-session-id", &context.request_id)
                .header("x-abar-tenant-id", &context.tenant.tenant_id)
                .header(
                    HeaderName::from_static("idempotency-key"),
                    &request.idempotency_key,
                )
                .json(&serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 3,
                    "method": "tools/call",
                    "params": params
                }));
            if let Some(session_id) = &session_id {
                call = call.header("mcp-session-id", session_id);
            }
            let response = match call.send().await {
                Ok(response) => response,
                Err(error) if error.is_timeout() => {
                    return Ok(ToolCallResult {
                        status: ToolCallStatus::Unknown,
                        result_json: None,
                        message: Some("MCP call timed out".to_owned()),
                    });
                }
                Err(error) => return Err(http_error(error)),
            };
            match parse_rpc(response).await {
                Ok(result) => {
                    let business_result =
                        result.get("structuredContent").cloned().unwrap_or(result);
                    Ok(ToolCallResult {
                        status: ToolCallStatus::Confirmed,
                        result_json: Some(business_result.to_string()),
                        message: None,
                    })
                }
                Err(error) => Ok(ToolCallResult {
                    status: ToolCallStatus::Failed,
                    result_json: None,
                    message: Some(error.to_string()),
                }),
            }
        })
    }

    fn check<'a>(
        &'a self,
        context: &'a RequestContext,
        request: &'a ToolCallRequest,
    ) -> BoxFuture<'a, Result<Option<ToolCallResult>, ProviderError>> {
        Box::pin(async move { self.lookup_idempotency(context, request).await })
    }
}

impl McpHttpToolRegistry {
    async fn lookup_idempotency(
        &self,
        context: &RequestContext,
        request: &ToolCallRequest,
    ) -> Result<Option<ToolCallResult>, ProviderError> {
        let (server_name, _) = request.tool_id.split_once('.').ok_or_else(|| {
            provider_error(ErrorCode::InvalidRequest, "tool id must be server.tool")
        })?;
        let server = self.servers.get(server_name).ok_or_else(|| {
            provider_error(ErrorCode::ToolUnavailable, "MCP server is not configured")
        })?;
        let Some(check_tool) = &server.idempotency_check_tool else {
            return Ok(None);
        };
        if server.tools.contains_key(check_tool) {
            return Err(provider_error(
                ErrorCode::ToolUnavailable,
                "idempotency check Tool must not be model-callable",
            ));
        }
        let request_builder = self.request(server)?;
        let session_id = self
            .initialize(
                server,
                request_builder.try_clone().expect("request cloneable"),
            )
            .await?;
        let mut call = request_builder
            .header("x-abar-session-id", &context.request_id)
            .header("x-abar-tenant-id", &context.tenant.tenant_id)
            .header(
                HeaderName::from_static("idempotency-key"),
                &request.idempotency_key,
            )
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {
                    "name": check_tool,
                    "arguments": {
                        "idempotencyKey": request.idempotency_key
                    }
                }
            }));
        if let Some(session_id) = &session_id {
            call = call.header("mcp-session-id", session_id);
        }
        let response = call.send().await.map_err(http_error)?;
        let result = parse_rpc(response).await?;
        let business_result = result.get("structuredContent").cloned().unwrap_or(result);
        let found = business_result
            .get("found")
            .and_then(Value::as_bool)
            .ok_or_else(|| {
                provider_error(
                    ErrorCode::ToolUnavailable,
                    "idempotency check response has no boolean found field",
                )
            })?;
        if !found {
            return Ok(None);
        }
        let result = business_result.get("result").cloned().ok_or_else(|| {
            provider_error(
                ErrorCode::ToolUnavailable,
                "idempotency check response has no result",
            )
        })?;
        Ok(Some(ToolCallResult {
            status: ToolCallStatus::Confirmed,
            result_json: Some(result.to_string()),
            message: None,
        }))
    }

    async fn list_all(&self) -> Result<Vec<ToolDescriptor>, ProviderError> {
        let mut descriptors = Vec::new();
        for server in self.servers.values() {
            for tool in self.discover(server).await? {
                let Some(effect) = server.tools.get(&tool.name) else {
                    continue;
                };
                descriptors.push(ToolDescriptor {
                    id: format!("{}.{}", server.name, tool.name),
                    version: schema_digest(&tool),
                    description: tool.description.clone(),
                    input_schema_json: serde_json::to_string(&tool.input_schema)
                        .unwrap_or_else(|_| "{}".to_owned()),
                    output_schema_json: tool
                        .output_schema
                        .as_ref()
                        .and_then(|schema| serde_json::to_string(schema).ok())
                        .unwrap_or_else(|| "{}".to_owned()),
                    effect: *effect,
                    schema_digest: schema_digest(&tool),
                });
            }
        }
        descriptors.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(descriptors)
    }
}

#[derive(Clone, Debug, Deserialize)]
struct McpTool {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(rename = "inputSchema")]
    input_schema: Value,
    #[serde(rename = "outputSchema")]
    output_schema: Option<Value>,
}

async fn parse_rpc(response: reqwest::Response) -> Result<Value, ProviderError> {
    if !response.status().is_success() {
        return Err(provider_error(
            ErrorCode::ToolUnavailable,
            format!("MCP returned HTTP {}", response.status()),
        ));
    }
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let body = response.text().await.map_err(http_error)?;
    let json = if content_type.contains("text/event-stream") {
        body.lines()
            .find_map(|line| line.strip_prefix("data: "))
            .unwrap_or(&body)
            .to_owned()
    } else {
        body
    };
    let envelope: RpcEnvelope = serde_json::from_str(&json).map_err(|error| {
        provider_error(
            ErrorCode::ToolUnavailable,
            format!("invalid MCP response: {error}"),
        )
    })?;
    if let Some(error) = envelope.error {
        return Err(provider_error(
            ErrorCode::ToolUnavailable,
            format!("MCP error {}: {}", error.code, error.message),
        ));
    }
    envelope
        .result
        .ok_or_else(|| provider_error(ErrorCode::ToolUnavailable, "MCP response has no result"))
}

#[derive(Deserialize)]
struct RpcEnvelope {
    result: Option<Value>,
    error: Option<RpcError>,
}

#[derive(Deserialize)]
struct RpcError {
    code: i64,
    message: String,
}

fn schema_digest(tool: &McpTool) -> String {
    let value = serde_json::json!({
        "name": tool.name,
        "description": tool.description,
        "inputSchema": tool.input_schema,
        "outputSchema": tool.output_schema,
    });
    let mut hasher = Sha256::new();
    hasher.update(serde_json::to_vec(&value).unwrap_or_default());
    format!("sha256:{}", hex(hasher.finalize().as_slice()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn provider_error(code: ErrorCode, message: impl Into<String>) -> ProviderError {
    ProviderError::new(code, message, false)
}

fn http_error(error: reqwest::Error) -> ProviderError {
    provider_error(ErrorCode::ToolUnavailable, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{McpHttpToolRegistry, McpServerConfig};
    use abar_api::context::{RequestContext, TenantContext};
    use abar_api::provider::{ToolCallRequest, ToolEffect, ToolRegistry};
    use std::collections::BTreeMap;

    #[test]
    fn unconfigured_idempotency_lookup_fails_closed_to_unknown() {
        let registry = McpHttpToolRegistry::new(vec![McpServerConfig {
            name: "service".to_owned(),
            endpoint: "http://127.0.0.1:1/mcp".to_owned(),
            auth_ref: None,
            idempotency_check_tool: None,
            tools: BTreeMap::from([("write".to_owned(), ToolEffect::Write)]),
        }]);
        let context = RequestContext::new("req", TenantContext::new("tenant"));
        let request = ToolCallRequest {
            tool_id: "service.write".to_owned(),
            arguments_json: "{}".to_owned(),
            idempotency_key: "key".to_owned(),
            effect: ToolEffect::Write,
            approval: None,
        };
        assert!(
            block_on(registry.check(&context, &request))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn model_callable_check_tool_is_rejected() {
        let registry = McpHttpToolRegistry::new(vec![McpServerConfig {
            name: "service".to_owned(),
            endpoint: "http://127.0.0.1:1/mcp".to_owned(),
            auth_ref: None,
            idempotency_check_tool: Some("check_idempotency".to_owned()),
            tools: BTreeMap::from([
                ("write".to_owned(), ToolEffect::Write),
                ("check_idempotency".to_owned(), ToolEffect::Read),
            ]),
        }]);
        let context = RequestContext::new("req", TenantContext::new("tenant"));
        let request = ToolCallRequest {
            tool_id: "service.write".to_owned(),
            arguments_json: "{}".to_owned(),
            idempotency_key: "key".to_owned(),
            effect: ToolEffect::Write,
            approval: None,
        };
        assert!(block_on(registry.check(&context, &request)).is_err());
    }

    fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
        let mut future = std::pin::pin!(future);
        let waker = std::task::Waker::noop();
        let mut context = std::task::Context::from_waker(waker);
        loop {
            match future.as_mut().poll(&mut context) {
                std::task::Poll::Ready(value) => return value,
                std::task::Poll::Pending => std::thread::yield_now(),
            }
        }
    }
}
