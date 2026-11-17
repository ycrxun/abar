//! Stable provider contracts.

use crate::context::RequestContext;
use crate::error::ErrorCode;
use crate::message::TokenUsage;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::future::Future;
use std::pin::Pin;

/// Boxed, object-safe asynchronous provider result.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Provider failure with a stable wire code.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderError {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
}

impl ProviderError {
    /// Creates a provider error.
    #[must_use]
    pub fn new(code: ErrorCode, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code,
            message: message.into(),
            retryable,
        }
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ProviderError {}

/// One model completion request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelRequest {
    pub model_id: String,
    pub input: String,
    pub tools: Vec<ModelTool>,
    pub tool_results: Vec<ModelToolResult>,
}

/// Tool description exposed to a model provider.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelTool {
    pub id: String,
    pub description: String,
    pub input_schema_json: String,
}

/// One projected Tool result returned to the model provider.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelToolResult {
    pub tool_id: String,
    pub content: String,
}

/// A tool call proposed by the model before authorization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolIntent {
    pub tool_id: String,
    pub arguments_json: String,
}

/// Model completion result.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ModelResponse {
    pub served_model_id: String,
    pub output: String,
    pub tool_intents: Vec<ToolIntent>,
    pub usage: TokenUsage,
}

/// Model provider contract.
pub trait ModelProvider: Send + Sync {
    /// Performs a model completion.
    fn complete<'a>(
        &'a self,
        context: &'a RequestContext,
        request: &'a ModelRequest,
    ) -> BoxFuture<'a, Result<ModelResponse, ProviderError>>;
}

/// Safety implication declared by the business for a tool.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolEffect {
    Read,
    Write,
}

/// Snapshot of a tool discovered at build time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolDescriptor {
    pub id: String,
    pub version: String,
    pub description: String,
    pub input_schema_json: String,
    pub output_schema_json: String,
    pub effect: ToolEffect,
    pub schema_digest: String,
}

/// One authorized tool invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolCallRequest {
    pub tool_id: String,
    pub arguments_json: String,
    pub idempotency_key: String,
    pub effect: ToolEffect,
    pub approval: Option<ApprovalEvidence>,
}

/// Human approval bound to one exact tool call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApprovalEvidence {
    pub approval_id: String,
    pub actor: String,
    pub scopes: Vec<String>,
}

/// Tool result status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolCallStatus {
    Confirmed,
    Failed,
    Unknown,
}

/// Tool invocation result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolCallResult {
    pub status: ToolCallStatus,
    pub result_json: Option<String>,
    pub message: Option<String>,
}

/// Tool discovery and invocation contract.
pub trait ToolRegistry: Send + Sync {
    /// Resolves actual tool versions for one Agent Revision.
    fn resolve_for_revision<'a>(
        &'a self,
        _context: &'a RequestContext,
        _revision: &'a crate::revision::AgentRevision,
    ) -> BoxFuture<'a, Result<Vec<ToolDescriptor>, ProviderError>> {
        Box::pin(async {
            Err(ProviderError::new(
                ErrorCode::ToolUnavailable,
                "ToolRegistry does not support Revision resolution",
                false,
            ))
        })
    }

    /// Calls an authorized tool.
    fn call<'a>(
        &'a self,
        context: &'a RequestContext,
        request: &'a ToolCallRequest,
    ) -> BoxFuture<'a, Result<ToolCallResult, ProviderError>>;

    /// Looks up the existing result for one exact Tool call.
    fn check<'a>(
        &'a self,
        context: &'a RequestContext,
        request: &'a ToolCallRequest,
    ) -> BoxFuture<'a, Result<Option<ToolCallResult>, ProviderError>>;
}

/// L1 Skill metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
    pub disable_model_invocation: bool,
    pub version: Option<String>,
}

/// L2 Skill content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SkillContent {
    pub metadata: SkillMetadata,
    pub markdown: String,
}

impl fmt::Display for ToolEffect {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read => formatter.write_str("read"),
            Self::Write => formatter.write_str("write"),
        }
    }
}
