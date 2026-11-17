//! Tenant-scoped approval inbox.

use super::super::{ApiError, ServerState, auth::authenticate};
use super::common::{session_error, store};
use abar_api::approval::ApprovalTask;
use abar_api::message::PendingToolCall;
use axum::Json;
use axum::Router;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::routing::get;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub(super) fn routes() -> Router<Arc<ServerState>> {
    Router::new().route("/v1/approvals", get(list_approvals))
}

#[derive(Deserialize)]
struct ApprovalQuery {
    agent: Option<String>,
}

#[derive(Serialize)]
struct ApprovalListResponse {
    approvals: Vec<ApprovalTask>,
}

async fn list_approvals(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Query(query): Query<ApprovalQuery>,
) -> Result<Json<ApprovalListResponse>, ApiError> {
    let auth = authenticate(&state, &headers).await?;
    let mut approvals = Vec::new();
    let entries = store(&state)
        .list_approval_entries(&auth.tenant.tenant_id, query.agent.as_deref())
        .map_err(session_error)?;
    for entry in entries {
        let session = store(&state)
            .get(&entry.session_id)
            .map_err(session_error)?;
        let pending = PendingToolCall {
            approval_id: entry.approval_id.clone(),
            tool_id: entry.tool_id.clone(),
            arguments_json: entry.arguments_json.clone(),
            idempotency_key: String::new(),
        };
        let visible = state
            .approval_authorizer
            .can_review(&auth, &session, &pending)
            .await
            .map_err(|error| ApiError::forbidden(error.message))?;
        if visible {
            approvals.push(ApprovalTask {
                approval_id: entry.approval_id,
                session_id: entry.session_id,
                message_id: entry.message_id,
                agent: entry.agent,
                agent_revision: entry.agent_revision,
                requester: entry.requester,
                tool_id: entry.tool_id,
                arguments_json: entry.arguments_json,
                pending_since: entry.pending_since,
            });
        }
    }
    Ok(Json(ApprovalListResponse { approvals }))
}
