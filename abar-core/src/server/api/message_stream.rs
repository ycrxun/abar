use super::super::{ApiError, ServerState, auth::authenticate};
use super::common::{session_error, store};
use abar_api::runtime_event::RuntimeEvent;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use futures_util::stream;
use serde::Deserialize;
use serde_json::json;
use std::collections::VecDeque;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

pub(super) fn routes() -> Router<Arc<ServerState>> {
    Router::new().route(
        "/v1/sessions/{id}/messages/{message_id}/stream",
        get(stream_message),
    )
}

#[derive(Deserialize)]
struct StreamQuery {
    after: Option<u64>,
}

struct StreamState {
    store: crate::session::SessionStore,
    session_id: String,
    message_id: String,
    after: u64,
    snapshot: String,
    pending: VecDeque<RuntimeEvent>,
}

async fn stream_message(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Path((session_id, message_id)): Path<(String, String)>,
    Query(query): Query<StreamQuery>,
) -> Result<Response, ApiError> {
    let auth = authenticate(&state, &headers).await?;
    let session_store = store(&state);
    let (session, _message) = session_store
        .find_message_for_user(&auth.tenant.tenant_id, &auth.subject_id, &message_id)
        .map_err(session_error)?;
    if session.id != session_id {
        return Err(ApiError::not_found("Message not found"));
    }
    let after = headers
        .get("last-event-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
        .or(query.after)
        .unwrap_or(0);
    let state = StreamState {
        store: session_store,
        session_id,
        message_id,
        after,
        snapshot: String::new(),
        pending: VecDeque::new(),
    };
    let events = stream::unfold(state, |mut state| async move {
        loop {
            if let Some(event) = state.pending.pop_front() {
                state.after = event.sequence;
                let payload = serde_json::to_string(&event).ok()?;
                return Some((
                    Ok::<_, Infallible>(
                        Event::default()
                            .id(event.sequence.to_string())
                            .event("runtime")
                            .data(payload),
                    ),
                    state,
                ));
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
            let events = state
                .store
                .list_events(&state.session_id, &state.message_id, state.after)
                .ok()?;
            if !events.is_empty() {
                state.pending.extend(events);
                continue;
            }
            let message = state
                .store
                .get_message(&state.session_id, &state.message_id)
                .ok()?;
            let snapshot = serde_json::to_string(&json!({
                "state": message.state,
                "pendingApproval": message.pending_approval,
                "approval": message.approval,
                "pendingExternal": message.pending_external,
                "output": message.output,
                "modelUsage": message.model_usage,
                "toolResults": message.tool_results,
                "updatedAt": message.updated_at
            }))
            .ok()?;
            if snapshot != state.snapshot {
                state.snapshot = snapshot.clone();
                return Some((
                    Ok::<_, Infallible>(Event::default().event("message").data(snapshot)),
                    state,
                ));
            }
        }
    });
    Ok(Sse::new(events)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("heartbeat"),
        )
        .into_response())
}
