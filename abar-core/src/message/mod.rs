//! Message execution loop over resolved providers and context.

mod factory;
mod service;

use crate::context::{ContextAssembler, ContextInput, ContextToolResult};
use crate::digest::sha256_hex;
use abar_api::context::RequestContext;
use abar_api::data::DataPolicy;
use abar_api::knowledge::KnowledgeDocument;
use abar_api::message::{
    Message, MessageCheckpoint, PendingExternalCall, PendingToolCall, TokenUsage,
    ToolResultEvidence,
};
use abar_api::provider::{
    ApprovalEvidence, ModelProvider, ModelRequest, SkillContent, SkillMetadata, ToolCallRequest,
    ToolCallStatus, ToolDescriptor, ToolEffect, ToolRegistry,
};
use abar_api::revision::AgentRevision;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;

pub use factory::RevisionMessageBundleFactory;
pub use service::{
    MessageBundleFactory, MessageServiceError, ResolvedMessageBundle, execute_prepared_message,
    prepare_message, reconcile_message, resume_message, start_message,
};

/// Message execution failure.
#[derive(Debug, Error)]
pub enum MessageRunError {
    #[error("provider failed: {0}")]
    Provider(#[from] abar_api::provider::ProviderError),
    #[error("tool is not declared for this Message: {0}")]
    ToolNotAllowed(String),
    #[error("tool effect is not permitted: {0}")]
    EffectDenied(String),
    #[error("tool failed: {0}")]
    ToolFailed(String),
    #[error("multiple tool intents containing a write are not resumable")]
    MultipleToolIntentsWithWrite,
    #[error("model identity mismatch")]
    ModelMismatch,
    #[error("max steps exhausted")]
    MaxSteps,
}

/// Resolved dependencies for one Message.
pub struct MessageProviders<'a> {
    pub model: &'a dyn ModelProvider,
    pub tools: &'a dyn ToolRegistry,
    pub data_policy: &'a dyn DataPolicy,
}

/// Pre-resolved attributes for one Message execution.
pub struct MessageInput<'a> {
    pub message: &'a Message,
    pub revision: &'a AgentRevision,
    pub system_prompt: &'a str,
    pub automatic_skills: &'a [SkillMetadata],
    pub manual_skills: &'a [SkillContent],
    pub knowledge: &'a [KnowledgeDocument],
    pub history: &'a [Message],
    pub tools: &'a [ToolDescriptor],
    pub model_id: &'a str,
    pub execution_state: MessageExecutionState,
}

/// Successful Message output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessageOutput {
    pub output: String,
    pub tool_results: Vec<ToolResultEvidence>,
    pub model_usage: TokenUsage,
}

/// Result of one Message executor turn.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MessageExecutionResult {
    Completed(MessageOutput),
    Cancelled {
        tool_results: Vec<ToolResultEvidence>,
        model_usage: TokenUsage,
    },
    WaitingApproval {
        pending: PendingToolCall,
        checkpoint: MessageCheckpoint,
    },
    WaitingExternal {
        reason: String,
        pending: PendingExternalCall,
        checkpoint: MessageCheckpoint,
    },
}

/// Persisted execution state supplied when starting or resuming a Message.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MessageExecutionState {
    pub next_step: u32,
    pub tool_results: Vec<ToolResultEvidence>,
    pub approved_write: Option<ApprovedWrite>,
    pub external_tool: Option<ExternalToolCall>,
    pub model_usage: TokenUsage,
}

struct ToolCallInput<'a> {
    revision: &'a AgentRevision,
    tool: &'a ToolDescriptor,
    arguments_json: &'a str,
    idempotency_key: &'a str,
    approval: Option<ApprovalEvidence>,
    retry_if_missing: bool,
}

/// Approved write call to execute before continuing the model loop.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApprovedWrite {
    pub pending: PendingToolCall,
    pub approval: ApprovalEvidence,
    pub retry_if_missing: bool,
}

/// Previously issued Tool call whose external result must be queried.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalToolCall {
    pub pending: PendingExternalCall,
}

impl fmt::Debug for MessageProviders<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("MessageProviders { .. }")
    }
}

impl fmt::Debug for MessageInput<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MessageInput")
            .field("message", self.message)
            .field("revision", &self.revision.metadata.agent)
            .finish_non_exhaustive()
    }
}

/// Executes one Message with an immutable resolved context.
pub async fn execute_message(
    context: &RequestContext,
    input: &MessageInput<'_>,
    providers: &MessageProviders<'_>,
    cancellation: Option<&AtomicBool>,
) -> Result<MessageExecutionResult, MessageRunError> {
    let mut tool_results = input.execution_state.tool_results.clone();
    let mut model_usage = input.execution_state.model_usage;
    let mut step = input.execution_state.next_step.max(1);
    if cancellation.is_some_and(|value| value.load(Ordering::Acquire)) {
        return Ok(MessageExecutionResult::Cancelled {
            tool_results,
            model_usage,
        });
    }
    if let Some(external) = &input.execution_state.external_tool {
        let tool = input
            .tools
            .iter()
            .find(|tool| tool.id == external.pending.tool_id)
            .ok_or_else(|| MessageRunError::ToolNotAllowed(external.pending.tool_id.clone()))?;
        let approval = if tool.effect == ToolEffect::Write {
            let approval = input
                .message
                .approval
                .as_ref()
                .ok_or(MessageRunError::EffectDenied(tool.id.clone()))?;
            Some(ApprovalEvidence {
                approval_id: approval.approval_id.clone(),
                actor: approval.actor.clone(),
                scopes: approval.scopes.clone(),
            })
        } else {
            None
        };
        let projected = call_tool(
            context,
            providers,
            ToolCallInput {
                revision: input.revision,
                tool,
                arguments_json: &external.pending.arguments_json,
                idempotency_key: &external.pending.idempotency_key,
                approval,
                retry_if_missing: tool.effect == ToolEffect::Read,
            },
        )
        .await?;
        let Some(projected) = projected else {
            return Ok(MessageExecutionResult::WaitingExternal {
                reason: format!("Tool result is unknown: {}", tool.id),
                pending: external.pending.clone(),
                checkpoint: MessageCheckpoint {
                    next_step: step,
                    tool_results,
                    model_usage,
                },
            });
        };
        tool_results.push(ToolResultEvidence {
            tool_id: tool.id.clone(),
            content: projected,
        });
        step += 1;
    }
    if let Some(approved) = &input.execution_state.approved_write {
        if cancellation.is_some_and(|value| value.load(Ordering::Acquire)) {
            return Ok(MessageExecutionResult::Cancelled {
                tool_results,
                model_usage,
            });
        }
        let tool = input
            .tools
            .iter()
            .find(|tool| tool.id == approved.pending.tool_id)
            .ok_or_else(|| MessageRunError::ToolNotAllowed(approved.pending.tool_id.clone()))?;
        let projected = call_tool(
            context,
            providers,
            ToolCallInput {
                revision: input.revision,
                tool,
                arguments_json: &approved.pending.arguments_json,
                idempotency_key: &approved.pending.idempotency_key,
                approval: Some(approved.approval.clone()),
                retry_if_missing: approved.retry_if_missing,
            },
        )
        .await?;
        let Some(projected) = projected else {
            return Ok(MessageExecutionResult::WaitingExternal {
                reason: format!("Tool result is unknown: {}", tool.id),
                pending: PendingExternalCall {
                    tool_id: approved.pending.tool_id.clone(),
                    arguments_json: approved.pending.arguments_json.clone(),
                    idempotency_key: approved.pending.idempotency_key.clone(),
                },
                checkpoint: MessageCheckpoint {
                    next_step: step,
                    tool_results,
                    model_usage,
                },
            });
        };
        tool_results.push(ToolResultEvidence {
            tool_id: tool.id.clone(),
            content: projected,
        });
        step += 1;
    }
    while step <= input.revision.spec.resource_scope.resources.max_steps {
        if cancellation.is_some_and(|value| value.load(Ordering::Acquire)) {
            return Ok(MessageExecutionResult::Cancelled {
                tool_results,
                model_usage,
            });
        }
        let context_results = tool_results
            .iter()
            .map(|result| ContextToolResult {
                tool_id: &result.tool_id,
                content: &result.content,
            })
            .collect::<Vec<_>>();
        let model_input = ContextAssembler::new(20).assemble(&ContextInput {
            system_prompt: input.system_prompt,
            automatic_skills: input.automatic_skills,
            manual_skills: input.manual_skills,
            knowledge: input.knowledge,
            history: input.history,
            tool_results: &context_results,
            current_input: &input.message.input,
        });
        let response = providers
            .model
            .complete(
                context,
                &ModelRequest {
                    model_id: input.model_id.to_owned(),
                    input: model_input,
                    tools: input
                        .tools
                        .iter()
                        .map(|tool| abar_api::provider::ModelTool {
                            id: tool.id.clone(),
                            description: tool.description.clone(),
                            input_schema_json: tool.input_schema_json.clone(),
                        })
                        .collect(),
                    tool_results: tool_results
                        .iter()
                        .map(|result| abar_api::provider::ModelToolResult {
                            tool_id: result.tool_id.clone(),
                            content: result.content.clone(),
                        })
                        .collect(),
                },
            )
            .await?;
        model_usage += response.usage;
        if response.served_model_id != input.model_id {
            return Err(MessageRunError::ModelMismatch);
        }
        if cancellation.is_some_and(|value| value.load(Ordering::Acquire)) {
            return Ok(MessageExecutionResult::Cancelled {
                tool_results,
                model_usage,
            });
        }
        if response.tool_intents.is_empty() {
            let output = providers
                .data_policy
                .guard_output(
                    context,
                    &response.output,
                    &input.revision.spec.resource_scope.data.deny,
                )
                .await?;
            return Ok(MessageExecutionResult::Completed(MessageOutput {
                output,
                tool_results,
                model_usage,
            }));
        }
        if response.tool_intents.len() > 1
            && response.tool_intents.iter().any(|intent| {
                input
                    .tools
                    .iter()
                    .find(|tool| tool.id == intent.tool_id)
                    .is_some_and(|tool| tool.effect == ToolEffect::Write)
            })
        {
            return Err(MessageRunError::MultipleToolIntentsWithWrite);
        }
        for intent in response.tool_intents {
            if cancellation.is_some_and(|value| value.load(Ordering::Acquire)) {
                return Ok(MessageExecutionResult::Cancelled {
                    tool_results,
                    model_usage,
                });
            }
            let tool = input
                .tools
                .iter()
                .find(|tool| tool.id == intent.tool_id)
                .ok_or_else(|| MessageRunError::ToolNotAllowed(intent.tool_id.clone()))?;
            if tool.effect == ToolEffect::Write {
                let idempotency_key = sha256_hex(
                    format!(
                        "{}:{}:{}:{}:{}",
                        input.message.session_id,
                        input.message.id,
                        tool.id,
                        intent.arguments_json,
                        step
                    )
                    .as_bytes(),
                );
                return Ok(MessageExecutionResult::WaitingApproval {
                    pending: PendingToolCall {
                        approval_id: sha256_hex(
                            format!(
                                "approval:{}:{}:{}:{}:{}",
                                input.message.session_id,
                                input.message.id,
                                tool.id,
                                intent.arguments_json,
                                idempotency_key
                            )
                            .as_bytes(),
                        ),
                        tool_id: tool.id.clone(),
                        arguments_json: intent.arguments_json,
                        idempotency_key,
                    },
                    checkpoint: MessageCheckpoint {
                        next_step: step,
                        tool_results: tool_results.clone(),
                        model_usage,
                    },
                });
            }
            let idempotency_key = sha256_hex(
                format!(
                    "{}:{}:{}:{}:{}",
                    input.message.session_id,
                    input.message.id,
                    tool.id,
                    intent.arguments_json,
                    step
                )
                .as_bytes(),
            );
            let projected = call_tool(
                context,
                providers,
                ToolCallInput {
                    revision: input.revision,
                    tool,
                    arguments_json: &intent.arguments_json,
                    idempotency_key: &idempotency_key,
                    approval: None,
                    retry_if_missing: true,
                },
            )
            .await?;
            let Some(projected) = projected else {
                return Ok(MessageExecutionResult::WaitingExternal {
                    reason: format!("Tool result is unknown: {}", tool.id),
                    pending: PendingExternalCall {
                        tool_id: tool.id.clone(),
                        arguments_json: intent.arguments_json,
                        idempotency_key,
                    },
                    checkpoint: MessageCheckpoint {
                        next_step: step,
                        tool_results,
                        model_usage,
                    },
                });
            };
            tool_results.push(ToolResultEvidence {
                tool_id: tool.id.clone(),
                content: projected,
            });
        }
        step += 1;
    }
    Err(MessageRunError::MaxSteps)
}

async fn call_tool(
    context: &RequestContext,
    providers: &MessageProviders<'_>,
    input: ToolCallInput<'_>,
) -> Result<Option<String>, MessageRunError> {
    let request = ToolCallRequest {
        tool_id: input.tool.id.clone(),
        arguments_json: input.arguments_json.to_owned(),
        idempotency_key: input.idempotency_key.to_owned(),
        effect: input.tool.effect,
        approval: input.approval.clone(),
    };
    if let Some(result) = providers.tools.check(context, &request).await? {
        return project_result(context, providers, input.revision, result).await;
    }
    if !input.retry_if_missing {
        return Ok(None);
    }
    let result = providers.tools.call(context, &request).await?;
    project_result(context, providers, input.revision, result).await
}

async fn project_result(
    context: &RequestContext,
    providers: &MessageProviders<'_>,
    revision: &AgentRevision,
    result: abar_api::provider::ToolCallResult,
) -> Result<Option<String>, MessageRunError> {
    let raw = match result.status {
        ToolCallStatus::Confirmed => result
            .result_json
            .unwrap_or_else(|| result.message.unwrap_or_default()),
        ToolCallStatus::Unknown => return Ok(None),
        ToolCallStatus::Failed => {
            return Err(MessageRunError::ToolFailed("tool".to_owned()));
        }
    };
    let classification = providers.data_policy.classify(context, &raw).await?;
    Ok(Some(
        providers
            .data_policy
            .project_for_model(
                context,
                &raw,
                &classification,
                &revision.spec.resource_scope.data.allow,
                &revision.spec.resource_scope.data.deny,
            )
            .await?,
    ))
}

#[cfg(test)]
mod tests;
