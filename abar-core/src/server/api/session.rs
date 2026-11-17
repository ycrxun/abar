//! Long-lived Session APIs.

use super::super::{ApiError, ServerState, auth::authenticate};
use super::common::{owned_session, session_error, store};
use abar_api::message::MessageState;
use abar_api::session::Session;
use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub(super) fn routes() -> Router<Arc<ServerState>> {
    Router::new()
        .route("/v1/sessions", get(list_sessions).post(create_session))
        .route("/v1/sessions/{id}", get(get_session))
        .route("/v1/sessions/{id}/close", post(close_session))
        .route("/v1/sessions/{id}/archive", post(archive_session))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateSessionRequest {
    agent: String,
}

#[derive(Deserialize)]
struct SessionQuery {
    agent: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionListResponse {
    sessions: Vec<SessionSummary>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionSummary {
    #[serde(flatten)]
    session: Session,
    latest_message_id: Option<String>,
    latest_message_state: Option<MessageState>,
    latest_message_input: Option<String>,
    message_count: usize,
}

async fn create_session(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Json(request): Json<CreateSessionRequest>,
) -> Result<(StatusCode, Json<Session>), ApiError> {
    let auth = authenticate(&state, &headers).await?;
    let session = store(&state)
        .create(&auth.tenant.tenant_id, &request.agent, &auth.subject_id)
        .map_err(session_error)?;
    Ok((StatusCode::CREATED, Json(session)))
}

async fn list_sessions(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Query(query): Query<SessionQuery>,
) -> Result<Json<SessionListResponse>, ApiError> {
    let auth = authenticate(&state, &headers).await?;
    let sessions = store(&state)
        .list_for_user(
            &auth.tenant.tenant_id,
            &auth.subject_id,
            query.agent.as_deref(),
        )
        .map_err(session_error)?;
    let sessions = sessions
        .into_iter()
        .map(|session| {
            let messages = store(&state)
                .list_messages(&session.id)
                .map_err(session_error)?;
            let latest = messages.last();
            Ok(SessionSummary {
                latest_message_id: latest.map(|message| message.id.clone()),
                latest_message_state: latest.map(|message| message.state),
                latest_message_input: latest.map(|message| message.input.clone()),
                message_count: messages.len(),
                session,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    Ok(Json(SessionListResponse { sessions }))
}

async fn get_session(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Session>, ApiError> {
    let auth = authenticate(&state, &headers).await?;
    Ok(Json(owned_session(&state, &auth, &id)?))
}

async fn close_session(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Session>, ApiError> {
    let auth = authenticate(&state, &headers).await?;
    owned_session(&state, &auth, &id)?;
    store(&state).close(&id).map(Json).map_err(session_error)
}

async fn archive_session(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Session>, ApiError> {
    let auth = authenticate(&state, &headers).await?;
    owned_session(&state, &auth, &id)?;
    store(&state).archive(&id).map(Json).map_err(session_error)
}
