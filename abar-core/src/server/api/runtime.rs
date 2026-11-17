//! Read-only Runtime diagnostics.

use super::super::{ApiError, ServerState, auth::authenticate, observability};
use crate::session::{SessionError, SessionStore};
use abar_api::message::MessageState;
use abar_api::session::SessionStatus;
use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::get;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Arc;

pub(super) fn routes() -> Router<Arc<ServerState>> {
    Router::new().route("/v1/runtime/status", get(runtime_status))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeStatus {
    api_version: &'static str,
    server_version: &'static str,
    status: &'static str,
    checked_at: String,
    active_workers: usize,
    sessions: SessionCounts,
    messages: MessageCounts,
    providers: ProviderStatus,
    storage: StorageStatus,
}

#[derive(Default, Serialize)]
struct SessionCounts {
    total: usize,
    active: usize,
    idle: usize,
    closed: usize,
    archived: usize,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct MessageCounts {
    total: usize,
    queued: usize,
    running: usize,
    waiting_approval: usize,
    waiting_external: usize,
    completed: usize,
    failed: usize,
    cancelled: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderStatus {
    message_runtime_configured: bool,
    detail: Option<&'static str>,
}

#[derive(Serialize)]
struct StorageStatus {
    writable: bool,
}

async fn runtime_status(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
) -> Result<Json<RuntimeStatus>, ApiError> {
    let auth = authenticate(&state, &headers).await?;
    let store = SessionStore::new(state.root.join("sessions"));
    let sessions = store
        .list_for_user(&auth.tenant.tenant_id, &auth.subject_id, None)
        .map_err(session_error)?;
    let mut session_counts = SessionCounts::default();
    let mut message_counts = MessageCounts::default();
    let mut active_workers = 0;
    let active_message_ids = state
        .cancellations
        .lock()
        .expect("cancellation map poisoned")
        .keys()
        .cloned()
        .collect::<HashSet<_>>();
    for session in &sessions {
        session_counts.total += 1;
        match session.status {
            SessionStatus::Active => session_counts.active += 1,
            SessionStatus::Idle => session_counts.idle += 1,
            SessionStatus::Closed => session_counts.closed += 1,
            SessionStatus::Archived => session_counts.archived += 1,
        }
        for message in store.list_messages(&session.id).map_err(session_error)? {
            message_counts.total += 1;
            if active_message_ids.contains(&message.id) {
                active_workers += 1;
            }
            match message.state {
                MessageState::Queued => message_counts.queued += 1,
                MessageState::Running => message_counts.running += 1,
                MessageState::WaitingApproval => message_counts.waiting_approval += 1,
                MessageState::WaitingExternal => message_counts.waiting_external += 1,
                MessageState::Completed => message_counts.completed += 1,
                MessageState::Failed => message_counts.failed += 1,
                MessageState::Cancelled => message_counts.cancelled += 1,
            }
        }
    }
    let provider_configured = state.factory.validate_message_runtime().is_ok();
    let storage_writable = observability::storage_writable(&state.root);
    let status = if provider_configured && storage_writable {
        "ready"
    } else {
        "degraded"
    };
    Ok(Json(RuntimeStatus {
        api_version: "abar/v1",
        server_version: env!("CARGO_PKG_VERSION"),
        status,
        checked_at: time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .map_err(|error| ApiError::internal(error.to_string()))?,
        active_workers,
        sessions: session_counts,
        messages: message_counts,
        providers: ProviderStatus {
            message_runtime_configured: provider_configured,
            detail: (!provider_configured)
                .then_some("message Runtime provider configuration is invalid or unavailable"),
        },
        storage: StorageStatus {
            writable: storage_writable,
        },
    }))
}

fn session_error(error: SessionError) -> ApiError {
    match error {
        SessionError::SessionNotFound(_) | SessionError::MessageNotFound(_) => {
            ApiError::not_found(error.to_string())
        }
        _ => ApiError::internal(error.to_string()),
    }
}
