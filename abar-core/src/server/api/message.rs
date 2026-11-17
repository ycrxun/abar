//! Message execution, approval, recovery, and Event APIs.

use super::super::factory::RuntimeBundleFactory;
use super::super::{ApiError, ServerState, auth::authenticate, message_worker, observability};
use super::common::{
    owned_session, request_context, revision_error, revision_store, session_error, store,
    tenant_session,
};
use crate::message::{MessageServiceError, prepare_message};
use abar_api::message::{Message, MessageApproval, MessageState, ToolResultEvidence};
use abar_api::runtime_event::RuntimeEvent;
use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub(super) fn routes() -> Router<Arc<ServerState>> {
    Router::new()
        .route("/v1/sessions/{id}/messages", get(list_messages))
        .route("/v1/sessions/{id}/messages", post(create_message))
        .route("/v1/sessions/{id}/messages/{message_id}", get(get_message))
        .route(
            "/v1/sessions/{id}/messages/{message_id}/approval",
            post(submit_approval),
        )
        .route("/v1/messages/{message_id}", get(get_message_global))
        .route("/v1/messages/{message_id}/events", get(list_message_events))
        .route("/v1/messages/{message_id}/cancel", post(cancel_message))
        .route(
            "/v1/messages/{message_id}/reconcile",
            post(reconcile_message),
        )
}

#[derive(Serialize)]
struct MessageListResponse {
    messages: Vec<Message>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ApprovalRequest {
    approval_id: String,
    decision: String,
    #[serde(default)]
    scopes: Vec<String>,
    comment: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateMessageRequest {
    input: String,
    #[serde(default)]
    requested_skills: Vec<String>,
}

#[derive(Deserialize)]
struct EventQuery {
    after: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReconcileRequest {
    action: String,
}

#[derive(Serialize)]
struct EventsResponse {
    events: Vec<RuntimeEvent>,
    cursor: u64,
}

async fn list_messages(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<MessageListResponse>, ApiError> {
    let auth = authenticate(&state, &headers).await?;
    owned_session(&state, &auth, &id)?;
    let messages = store(&state).list_messages(&id).map_err(session_error)?;
    Ok(Json(MessageListResponse { messages }))
}

async fn get_message(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Path((id, message_id)): Path<(String, String)>,
) -> Result<Json<Message>, ApiError> {
    let auth = authenticate(&state, &headers).await?;
    owned_session(&state, &auth, &id)?;
    store(&state)
        .get_message(&id, &message_id)
        .map(Json)
        .map_err(session_error)
}

async fn create_message(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<CreateMessageRequest>,
) -> Result<(StatusCode, Json<Message>), ApiError> {
    let auth = authenticate(&state, &headers).await?;
    let session = owned_session(&state, &auth, &id)?;
    let revision = revision_store(&state)
        .current(&session.agent)
        .map_err(revision_error)?;
    let adapter = RuntimeBundleFactory {
        factory: state.factory.clone(),
    };
    let message = prepare_message(
        &store(&state),
        &request_context(&auth, &id),
        &id,
        &revision,
        &request.input,
        &request.requested_skills,
        &adapter,
    )
    .await
    .map_err(message_service_error)?;
    message_worker::spawn(&state, &id, &message.id);
    Ok((StatusCode::ACCEPTED, Json(message)))
}

async fn submit_approval(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Path((id, message_id)): Path<(String, String)>,
    Json(request): Json<ApprovalRequest>,
) -> Result<Json<Message>, ApiError> {
    let auth = authenticate(&state, &headers).await?;
    let session = tenant_session(&state, &auth, &id)?;
    if !matches!(request.decision.as_str(), "approved" | "rejected") {
        return Err(ApiError::bad_request(
            "decision must be approved or rejected",
        ));
    }
    let pending_message = store(&state)
        .get_message(&id, &message_id)
        .map_err(session_error)?;
    let pending = pending_message
        .pending_approval
        .as_ref()
        .ok_or_else(|| ApiError::conflict("Message has no pending approval"))?;
    state
        .approval_authorizer
        .authorize(&auth, &session, pending, &request.decision, &request.scopes)
        .await
        .map_err(|error| ApiError::forbidden(error.message))?;
    let message = store(&state)
        .submit_approval(
            &id,
            &message_id,
            MessageApproval {
                approval_id: request.approval_id,
                actor: auth.subject_id.clone(),
                scopes: request.scopes,
                decision: request.decision,
                comment: request.comment,
                reviewed_at: time::OffsetDateTime::now_utc()
                    .format(&time::format_description::well_known::Rfc3339)
                    .map_err(|error| ApiError::internal(error.to_string()))?,
            },
        )
        .map_err(session_error)?;
    let approved = message
        .approval
        .as_ref()
        .is_some_and(|approval| approval.decision == "approved");
    store(&state)
        .append_event(
            &id,
            &message_id,
            if approved {
                "approval.granted"
            } else {
                "approval.rejected"
            },
            serde_json::json!({
                "approvalId": message.approval.as_ref().map(|approval| &approval.approval_id),
                "actor": message.approval.as_ref().map(|approval| &approval.actor),
                "scopes": message.approval.as_ref().map(|approval| &approval.scopes),
                "roles": auth.roles
            }),
        )
        .map_err(session_error)?;
    observability::event(
        "info",
        if approved {
            "approval.granted"
        } else {
            "approval.rejected"
        },
        serde_json::json!({
            "approvalId": message.approval.as_ref().map(|approval| &approval.approval_id),
            "sessionId": id,
            "messageId": message_id,
            "actor": message.approval.as_ref().map(|approval| &approval.actor),
            "roles": auth.roles,
            "scopes": message.approval.as_ref().map(|approval| &approval.scopes)
        }),
    );
    state.metrics.record_approval(approved);
    if !approved {
        let _ = store(&state).append_event(
            &id,
            &message_id,
            "message.cancelled",
            serde_json::json!({"reason": "approval_rejected"}),
        );
        return Ok(Json(message));
    }
    message_worker::spawn(&state, &id, &message_id);
    Ok(Json(message))
}

async fn list_message_events(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Path(message_id): Path<String>,
    Query(query): Query<EventQuery>,
) -> Result<Json<EventsResponse>, ApiError> {
    let auth = authenticate(&state, &headers).await?;
    let store = store(&state);
    let (session, _) = store
        .find_message_for_user(&auth.tenant.tenant_id, &auth.subject_id, &message_id)
        .map_err(session_error)?;
    let after = query.after.unwrap_or(0);
    let events = store
        .list_events(&session.id, &message_id, after)
        .map_err(session_error)?;
    let cursor = events.last().map_or(after, |event| event.sequence);
    Ok(Json(EventsResponse { events, cursor }))
}

async fn get_message_global(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Path(message_id): Path<String>,
) -> Result<Json<Message>, ApiError> {
    let auth = authenticate(&state, &headers).await?;
    let (_, message) = store(&state)
        .find_message_for_user(&auth.tenant.tenant_id, &auth.subject_id, &message_id)
        .map_err(session_error)?;
    Ok(Json(message))
}

async fn cancel_message(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Path(message_id): Path<String>,
) -> Result<Json<Message>, ApiError> {
    let auth = authenticate(&state, &headers).await?;
    let (session, mut message) = store(&state)
        .find_message_for_user(&auth.tenant.tenant_id, &auth.subject_id, &message_id)
        .map_err(session_error)?;
    if !message_worker::cancel(&state, &message_id) {
        let tool_results = cancel_evidence(&message);
        message = store(&state)
            .cancel_message(
                &session.id,
                &message_id,
                "cancelled".to_owned(),
                tool_results,
                message.model_usage,
            )
            .map_err(session_error)?;
        let _ = store(&state).append_event(
            &session.id,
            &message_id,
            "message.cancelled",
            serde_json::json!({"reason": "user_cancelled"}),
        );
    }
    Ok(Json(message))
}

async fn reconcile_message(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Path(message_id): Path<String>,
    Json(request): Json<ReconcileRequest>,
) -> Result<Json<Message>, ApiError> {
    let auth = authenticate(&state, &headers).await?;
    let (session, message) = store(&state)
        .find_message_for_user(&auth.tenant.tenant_id, &auth.subject_id, &message_id)
        .map_err(session_error)?;
    match request.action.as_str() {
        "query" if message.state == MessageState::WaitingExternal => {
            message_worker::spawn(&state, &session.id, &message_id);
            Ok(Json(message))
        }
        "cancel" if message.state == MessageState::WaitingExternal => {
            let cancelled = store(&state)
                .cancel_message(
                    &session.id,
                    &message_id,
                    "reconciliation_cancelled".to_owned(),
                    cancel_evidence(&message),
                    message.model_usage,
                )
                .map_err(session_error)?;
            let _ = store(&state).append_event(
                &session.id,
                &message_id,
                "message.cancelled",
                serde_json::json!({"reason": "reconciliation_cancelled"}),
            );
            Ok(Json(cancelled))
        }
        "query" | "cancel" => Err(ApiError::conflict(
            "Message is not waiting for external reconciliation",
        )),
        _ => Err(ApiError::bad_request("action must be query or cancel")),
    }
}

pub(super) fn message_service_error(error: MessageServiceError) -> ApiError {
    match error {
        MessageServiceError::Session(error) => session_error(error),
        MessageServiceError::Provider(_) => ApiError::unavailable(error.to_string()),
        MessageServiceError::Execution(_)
        | MessageServiceError::Bundle(_)
        | MessageServiceError::CompositionMismatch
        | MessageServiceError::NotResumable => ApiError::conflict(error.to_string()),
    }
}

fn cancel_evidence(message: &Message) -> Vec<ToolResultEvidence> {
    if !message.tool_results.is_empty() {
        return message.tool_results.clone();
    }
    message
        .checkpoint
        .as_ref()
        .map_or_else(Vec::new, |checkpoint| checkpoint.tool_results.clone())
}
