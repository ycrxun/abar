//! Durable Message lifecycle orchestration.
use super::{
    ApprovedWrite, ExternalToolCall, MessageExecutionResult, MessageExecutionState, MessageInput,
    MessageProviders, execute_message,
};
use crate::provider_bundle::{ProviderBundle, ProviderBundleError};
use crate::session::{SessionError, SessionStore};
use abar_api::context::RequestContext;
use abar_api::data::DataPolicy;
use abar_api::message::{Message, MessageState};
use abar_api::provider::{ApprovalEvidence, BoxFuture, ModelProvider, ProviderError, ToolRegistry};
use abar_api::revision::AgentRevision;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use thiserror::Error;

/// Providers and immutable evidence resolved for one Agent Revision.
#[derive(Clone)]
pub struct ResolvedMessageBundle {
    pub bundle: ProviderBundle,
    pub model: Arc<dyn ModelProvider>,
    pub tools: Arc<dyn ToolRegistry>,
    pub data_policy: Arc<dyn DataPolicy>,
}

impl std::fmt::Debug for ResolvedMessageBundle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ResolvedMessageBundle")
            .field("composition", &self.bundle.composition)
            .finish_non_exhaustive()
    }
}

/// Resolves all replaceable providers for one Agent Revision.
pub trait MessageBundleFactory: Send + Sync {
    fn resolve<'a>(
        &'a self,
        context: &'a RequestContext,
        revision: &'a AgentRevision,
        input: &'a str,
    ) -> BoxFuture<'a, Result<ResolvedMessageBundle, ProviderError>>;
}

/// Message lifecycle failure.
#[derive(Debug, Error)]
pub enum MessageServiceError {
    #[error("session store failed: {0}")]
    Session(#[from] SessionError),
    #[error("provider bundle failed: {0}")]
    Bundle(#[from] ProviderBundleError),
    #[error("provider resolution failed: {0}")]
    Provider(#[from] ProviderError),
    #[error("message execution failed: {0}")]
    Execution(#[from] super::MessageRunError),
    #[error("resolved provider composition changed")]
    CompositionMismatch,
    #[error("message is not resumable")]
    NotResumable,
}

/// Resolves, persists, and executes a fresh Message.
#[allow(clippy::too_many_arguments)]
pub async fn start_message(
    store: &SessionStore,
    context: &RequestContext,
    session_id: &str,
    revision: &AgentRevision,
    input: &str,
    requested_skills: &[String],
    factory: &dyn MessageBundleFactory,
    cancellation: Option<Arc<AtomicBool>>,
) -> Result<Message, MessageServiceError> {
    let message = prepare_message(
        store,
        context,
        session_id,
        revision,
        input,
        requested_skills,
        factory,
    )
    .await?;
    execute_prepared_message(
        store,
        context,
        session_id,
        &message.id,
        revision,
        factory,
        cancellation,
    )
    .await
}

/// Resolves and persists a queued Message without executing it.
pub async fn prepare_message(
    store: &SessionStore,
    context: &RequestContext,
    session_id: &str,
    revision: &AgentRevision,
    input: &str,
    requested_skills: &[String],
    factory: &dyn MessageBundleFactory,
) -> Result<Message, MessageServiceError> {
    let resolved = factory.resolve(context, revision, input).await?;
    let message = store.create_message(
        session_id,
        revision.spec.revision,
        input,
        requested_skills,
        resolved.bundle.composition.clone(),
    )?;
    store.append_event(
        session_id,
        &message.id,
        "message.created",
        serde_json::json!({
            "agentRevision": message.agent_revision,
            "inputLength": message.input.len()
        }),
    )?;
    Ok(message)
}

/// Executes a queued or recoverable Message in a worker task.
pub async fn execute_prepared_message(
    store: &SessionStore,
    context: &RequestContext,
    session_id: &str,
    message_id: &str,
    revision: &AgentRevision,
    factory: &dyn MessageBundleFactory,
    cancellation: Option<Arc<AtomicBool>>,
) -> Result<Message, MessageServiceError> {
    let mut message = store.get_message(session_id, message_id)?;
    if message.state == MessageState::Queued {
        message = store.transition_message(session_id, message_id, MessageState::Running)?;
        store.append_event(
            session_id,
            message_id,
            "message.started",
            serde_json::json!({}),
        )?;
    } else if message.state != MessageState::Running {
        return Err(MessageServiceError::NotResumable);
    }
    let resolved = factory.resolve(context, revision, &message.input).await?;
    if !same_composition(&message.composition, &resolved.bundle.composition) {
        return Err(MessageServiceError::CompositionMismatch);
    }
    let history = store
        .list_messages(session_id)?
        .into_iter()
        .filter(|value| value.id != message_id)
        .collect::<Vec<_>>();
    let state =
        message
            .checkpoint
            .clone()
            .map_or_else(MessageExecutionState::default, |checkpoint| {
                MessageExecutionState {
                    next_step: checkpoint.next_step,
                    model_usage: checkpoint.model_usage,
                    tool_results: checkpoint.tool_results,
                    approved_write: None,
                    external_tool: None,
                }
            });
    let scope = ExecutionScope {
        store,
        context,
        session_id,
        history: &history,
        revision,
        resolved: &resolved,
        cancellation: cancellation.as_deref(),
    };
    execute_with_state(&scope, &message, state).await
}

/// Resumes a Message after an approved write intent.
pub async fn resume_message(
    store: &SessionStore,
    context: &RequestContext,
    session_id: &str,
    message_id: &str,
    revision: &AgentRevision,
    factory: &dyn MessageBundleFactory,
    cancellation: Option<Arc<AtomicBool>>,
) -> Result<Message, MessageServiceError> {
    let mut message = store.get_message(session_id, message_id)?;
    let resolved = factory.resolve(context, revision, &message.input).await?;
    if message.state != MessageState::Running
        || !same_composition(&message.composition, &resolved.bundle.composition)
    {
        return Err(MessageServiceError::NotResumable);
    }
    let pending = message
        .pending_approval
        .clone()
        .ok_or(MessageServiceError::NotResumable)?;
    let approval = message
        .approval
        .as_ref()
        .ok_or(MessageServiceError::NotResumable)?;
    if approval.decision != "approved" {
        return Err(MessageServiceError::NotResumable);
    }
    let checkpoint = message
        .checkpoint
        .clone()
        .ok_or(MessageServiceError::NotResumable)?;
    scope_event(
        store,
        session_id,
        message_id,
        "message.resumed",
        serde_json::json!({"approvalId": approval.approval_id}),
    )?;
    let history = store
        .list_messages(session_id)?
        .into_iter()
        .filter(|value| value.id != message_id)
        .collect::<Vec<_>>();
    message.checkpoint = None;
    let state = MessageExecutionState {
        next_step: checkpoint.next_step,
        model_usage: checkpoint.model_usage,
        tool_results: checkpoint.tool_results.clone(),
        approved_write: Some(ApprovedWrite {
            pending,
            approval: ApprovalEvidence {
                approval_id: approval.approval_id.clone(),
                actor: approval.actor.clone(),
                scopes: approval.scopes.clone(),
            },
            retry_if_missing: true,
        }),
        external_tool: None,
    };
    let scope = ExecutionScope {
        store,
        context,
        session_id,
        history: &history,
        revision,
        resolved: &resolved,
        cancellation: cancellation.as_deref(),
    };
    execute_with_state(&scope, &message, state).await
}

/// Reconciles a waiting-external write without retrying a missing write.
pub async fn reconcile_message(
    store: &SessionStore,
    context: &RequestContext,
    session_id: &str,
    message_id: &str,
    revision: &AgentRevision,
    factory: &dyn MessageBundleFactory,
    cancellation: Option<Arc<AtomicBool>>,
) -> Result<Message, MessageServiceError> {
    let message = store.get_message(session_id, message_id)?;
    if message.state != MessageState::WaitingExternal {
        return Err(MessageServiceError::NotResumable);
    }
    let resolved = factory.resolve(context, revision, &message.input).await?;
    if !same_composition(&message.composition, &resolved.bundle.composition) {
        return Err(MessageServiceError::CompositionMismatch);
    }
    let pending = message
        .pending_external
        .clone()
        .ok_or(MessageServiceError::NotResumable)?;
    if message
        .approval
        .as_ref()
        .is_some_and(|approval| approval.decision != "approved")
    {
        return Err(MessageServiceError::NotResumable);
    }
    let checkpoint = message
        .checkpoint
        .clone()
        .ok_or(MessageServiceError::NotResumable)?;
    let message = store.transition_message(session_id, message_id, MessageState::Running)?;
    let mut reconciled = serde_json::json!({
        "toolId": pending.tool_id,
        "idempotencyKey": pending.idempotency_key
    });
    if let Some(approval) = &message.approval {
        reconciled["approvalId"] = serde_json::Value::String(approval.approval_id.clone());
    }
    scope_event(
        store,
        session_id,
        message_id,
        "message.reconciled",
        reconciled,
    )?;
    let history = store
        .list_messages(session_id)?
        .into_iter()
        .filter(|value| value.id != message_id)
        .collect::<Vec<_>>();
    let state = MessageExecutionState {
        next_step: checkpoint.next_step,
        model_usage: checkpoint.model_usage,
        tool_results: checkpoint.tool_results,
        approved_write: None,
        external_tool: Some(ExternalToolCall { pending }),
    };
    let scope = ExecutionScope {
        store,
        context,
        session_id,
        history: &history,
        revision,
        resolved: &resolved,
        cancellation: cancellation.as_deref(),
    };
    execute_with_state(&scope, &message, state).await
}

fn same_composition(
    left: &abar_api::message::ExecutionComposition,
    right: &abar_api::message::ExecutionComposition,
) -> bool {
    let mut left = left.clone();
    let mut right = right.clone();
    left.resolved_at.clone_from(&right.resolved_at);
    if left.knowledge.len() != right.knowledge.len() {
        return false;
    }
    for (left, right) in left.knowledge.iter_mut().zip(&mut right.knowledge) {
        right.retrieved_at.clone_from(&left.retrieved_at);
    }
    left == right
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "cancellation_tests.rs"]
mod cancellation_tests;

struct ExecutionScope<'a> {
    store: &'a SessionStore,
    context: &'a RequestContext,
    session_id: &'a str,
    history: &'a [Message],
    revision: &'a AgentRevision,
    resolved: &'a ResolvedMessageBundle,
    cancellation: Option<&'a AtomicBool>,
}

async fn execute_with_state(
    scope: &ExecutionScope<'_>,
    message: &Message,
    execution_state: MessageExecutionState,
) -> Result<Message, MessageServiceError> {
    let automatic_skills = scope
        .resolved
        .bundle
        .skills
        .iter()
        .filter(|skill| !skill.metadata.disable_model_invocation)
        .map(|skill| skill.metadata.clone())
        .collect::<Vec<_>>();
    let manual_skills = scope
        .resolved
        .bundle
        .skills
        .iter()
        .filter(|skill| message.requested_skills.contains(&skill.metadata.name))
        .cloned()
        .collect::<Vec<_>>();
    let providers = MessageProviders {
        model: scope.resolved.model.as_ref(),
        tools: scope.resolved.tools.as_ref(),
        data_policy: scope.resolved.data_policy.as_ref(),
    };
    let result = execute_message(
        scope.context,
        &MessageInput {
            message,
            revision: scope.revision,
            system_prompt: &scope.resolved.bundle.system_prompt,
            automatic_skills: &automatic_skills,
            manual_skills: &manual_skills,
            knowledge: &scope.resolved.bundle.knowledge,
            history: scope.history,
            tools: &scope.resolved.bundle.tools,
            model_id: &scope.resolved.bundle.model_id,
            execution_state,
        },
        &providers,
        scope.cancellation,
    )
    .await;
    match result {
        Ok(MessageExecutionResult::Completed(output)) => {
            scope_event(
                scope.store,
                scope.session_id,
                &message.id,
                "message.completed",
                serde_json::json!({"outputLength": output.output.len()}),
            )?;
            Ok(scope.store.complete_message(
                scope.session_id,
                &message.id,
                output.output,
                output.tool_results,
                output.model_usage,
            )?)
        }
        Ok(MessageExecutionResult::Cancelled {
            tool_results,
            model_usage,
        }) => Ok(scope.store.cancel_message(
            scope.session_id,
            &message.id,
            "cancelled".to_owned(),
            tool_results,
            model_usage,
        )?),
        Ok(MessageExecutionResult::WaitingApproval {
            pending,
            checkpoint,
        }) => {
            scope_event(
                scope.store,
                scope.session_id,
                &message.id,
                "approval.requested",
                serde_json::json!({
                    "approvalId": pending.approval_id,
                    "toolId": pending.tool_id,
                    "idempotencyKey": pending.idempotency_key
                }),
            )?;
            Ok(scope
                .store
                .request_approval(scope.session_id, &message.id, pending, checkpoint)?)
        }
        Ok(MessageExecutionResult::WaitingExternal {
            reason,
            pending,
            checkpoint,
        }) => {
            scope_event(
                scope.store,
                scope.session_id,
                &message.id,
                "message.waiting_external",
                serde_json::json!({"reason": reason}),
            )?;
            Ok(scope.store.wait_for_external(
                scope.session_id,
                &message.id,
                reason,
                pending,
                checkpoint,
            )?)
        }
        Err(error) => {
            scope_event(
                scope.store,
                scope.session_id,
                &message.id,
                "message.failed",
                serde_json::json!({"error": error.to_string()}),
            )?;
            let _ = scope
                .store
                .fail_message(scope.session_id, &message.id, error.to_string());
            Err(MessageServiceError::Execution(error))
        }
    }
}

fn scope_event(
    store: &SessionStore,
    session_id: &str,
    message_id: &str,
    event_type: &str,
    data: serde_json::Value,
) -> Result<(), MessageServiceError> {
    store.append_event(session_id, message_id, event_type, data)?;
    Ok(())
}
