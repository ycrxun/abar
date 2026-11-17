//! Tenant-scoped platform catalog and Scope Review APIs.

use super::super::{ApiError, ServerState, auth::authenticate};
use crate::revision::{RevisionError, RevisionStore, review_agent_revision, validate_revision};
use crate::session::{SessionError, SessionStore};
use abar_api::message::MessageState;
use abar_api::policy::AuthContext;
use abar_api::revision::{ReviewDecision, ScopeReviewEvidence};
use abar_api::session::SessionStatus;
use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

pub(super) fn routes() -> Router<Arc<ServerState>> {
    Router::new()
        .route("/v1/control-plane/overview", get(overview))
        .route(
            "/v1/control-plane/agents/{agent}/revisions/{revision}/review",
            post(review_revision),
        )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ControlPlaneOverview {
    api_version: &'static str,
    server_version: &'static str,
    tenant: String,
    generated_at: String,
    runtime: RuntimeSummary,
    agents: Vec<AgentSummary>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeSummary {
    status: &'static str,
    active_workers: usize,
    sessions: BTreeMap<String, usize>,
    messages: BTreeMap<String, usize>,
    tokens: TokenSummary,
    provider_configured: bool,
    storage_writable: bool,
}

#[derive(Default, Serialize)]
struct TokenSummary {
    prompt: u64,
    completion: u64,
    total: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentSummary {
    agent: String,
    current_revision: Option<u64>,
    revision_count: usize,
    sessions: usize,
    messages: usize,
    tokens: TokenSummary,
    revisions: Vec<RevisionSummary>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RevisionSummary {
    revision: u64,
    status: &'static str,
    model_class: String,
    max_steps: u32,
    read_tools: Vec<String>,
    write_tools: Vec<String>,
    skills: Vec<String>,
    knowledge_sources: Vec<String>,
    review: Option<ScopeReviewEvidence>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReviewRequest {
    decision: String,
    comment: Option<String>,
}

async fn overview(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
) -> Result<Json<ControlPlaneOverview>, ApiError> {
    let auth = authenticate(&state, &headers).await?;
    require_platform_role(&state, &auth)?;
    let revision_store = RevisionStore::new(state.root.join("revisions"));
    let session_store = SessionStore::new(state.root.join("sessions"));
    let sessions = session_store
        .list_for_tenant(&auth.tenant.tenant_id)
        .map_err(session_error)?;
    let mut global_sessions = BTreeMap::new();
    let mut global_messages = BTreeMap::new();
    let mut global_tokens = TokenSummary::default();
    let mut agents: BTreeMap<String, AgentSummary> = BTreeMap::new();

    for agent in revision_store.agents().map_err(revision_error)? {
        let current_revision = revision_store.current_revision(&agent).ok();
        let revisions = revision_summaries(&revision_store, &agent, current_revision)?;
        agents.insert(
            agent.clone(),
            AgentSummary {
                agent,
                current_revision,
                revision_count: revisions.len(),
                sessions: 0,
                messages: 0,
                tokens: TokenSummary::default(),
                revisions,
            },
        );
    }

    for session in sessions {
        increment(&mut global_sessions, session_status(&session.status));
        if let Some(agent) = agents.get_mut(&session.agent) {
            agent.sessions += 1;
        }
        for message in session_store
            .list_messages(&session.id)
            .map_err(session_error)?
        {
            increment(&mut global_messages, message_state(&message.state));
            add_tokens(&mut global_tokens, &message.model_usage);
            if let Some(agent) = agents.get_mut(&session.agent) {
                agent.messages += 1;
                add_tokens(&mut agent.tokens, &message.model_usage);
            }
        }
    }

    let provider_configured = state.factory.validate_message_runtime().is_ok();
    let storage_writable = super::super::observability::storage_writable(&state.root);
    let active_workers = state
        .cancellations
        .lock()
        .expect("cancellation map poisoned")
        .len();
    Ok(Json(ControlPlaneOverview {
        api_version: "abar/v1",
        server_version: env!("CARGO_PKG_VERSION"),
        tenant: auth.tenant.tenant_id,
        generated_at: now()?,
        runtime: RuntimeSummary {
            status: if provider_configured && storage_writable {
                "ready"
            } else {
                "degraded"
            },
            active_workers,
            sessions: global_sessions,
            messages: global_messages,
            tokens: global_tokens,
            provider_configured,
            storage_writable,
        },
        agents: agents.into_values().collect(),
    }))
}

async fn review_revision(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Path((agent, revision)): Path<(String, u64)>,
    Json(request): Json<ReviewRequest>,
) -> Result<Json<ScopeReviewEvidence>, ApiError> {
    let auth = authenticate(&state, &headers).await?;
    require_platform_role(&state, &auth)?;
    let decision = match request.decision.as_str() {
        "approved" => ReviewDecision::Approved,
        "rejected" => ReviewDecision::Rejected,
        _ => {
            return Err(ApiError::bad_request(
                "decision must be approved or rejected",
            ));
        }
    };
    let store = RevisionStore::new(state.root.join("revisions"));
    let evidence = review_agent_revision(
        &store,
        &agent,
        revision,
        &auth.subject_id,
        decision,
        request.comment,
    )
    .map_err(|error| ApiError::conflict(error.to_string()))?;
    Ok(Json(evidence))
}

fn revision_summaries(
    store: &RevisionStore,
    agent: &str,
    current: Option<u64>,
) -> Result<Vec<RevisionSummary>, ApiError> {
    store
        .revisions(agent)
        .map_err(revision_error)?
        .into_iter()
        .map(|number| {
            let revision = store.get(agent, number).map_err(revision_error)?;
            validate_revision(&revision).map_err(revision_error)?;
            let review = store
                .review_evidence(agent, number)
                .map_err(revision_error)?;
            let status = if current == Some(number) {
                "current"
            } else {
                match review.as_ref().map(|value| value.decision) {
                    Some(ReviewDecision::Approved) => "approved",
                    Some(ReviewDecision::Rejected) => "rejected",
                    None => "waiting_review",
                }
            };
            Ok(RevisionSummary {
                revision: number,
                status,
                model_class: revision.spec.resource_scope.resources.model_class,
                max_steps: revision.spec.resource_scope.resources.max_steps,
                read_tools: revision.spec.resource_scope.tools.read,
                write_tools: revision.spec.resource_scope.tools.write,
                skills: revision.spec.resource_scope.skills,
                knowledge_sources: revision.spec.resource_scope.knowledge_sources,
                review,
            })
        })
        .collect()
}

fn require_platform_role(state: &ServerState, auth: &AuthContext) -> Result<(), ApiError> {
    if auth
        .roles
        .iter()
        .any(|role| role == &state.control_plane_role)
    {
        Ok(())
    } else {
        Err(ApiError::forbidden("platform role is required"))
    }
}

fn increment(counts: &mut BTreeMap<String, usize>, state: &str) {
    *counts.entry(state.to_owned()).or_default() += 1;
}

fn add_tokens(totals: &mut TokenSummary, usage: &abar_api::message::TokenUsage) {
    totals.prompt = totals.prompt.saturating_add(usage.prompt_tokens);
    totals.completion = totals.completion.saturating_add(usage.completion_tokens);
    totals.total = totals.total.saturating_add(usage.total_tokens);
}

fn session_status(status: &SessionStatus) -> &'static str {
    match status {
        SessionStatus::Active => "active",
        SessionStatus::Idle => "idle",
        SessionStatus::Closed => "closed",
        SessionStatus::Archived => "archived",
    }
}

fn message_state(state: &MessageState) -> &'static str {
    match state {
        MessageState::Queued => "queued",
        MessageState::Running => "running",
        MessageState::WaitingApproval => "waiting_approval",
        MessageState::WaitingExternal => "waiting_external",
        MessageState::Completed => "completed",
        MessageState::Failed => "failed",
        MessageState::Cancelled => "cancelled",
    }
}

fn now() -> Result<String, ApiError> {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|error| ApiError::internal(error.to_string()))
}

fn revision_error(error: RevisionError) -> ApiError {
    match error {
        RevisionError::NotFound { .. } | RevisionError::CurrentNotSet(_) => {
            ApiError::not_found(error.to_string())
        }
        _ => ApiError::internal(error.to_string()),
    }
}

fn session_error(error: SessionError) -> ApiError {
    ApiError::internal(error.to_string())
}
