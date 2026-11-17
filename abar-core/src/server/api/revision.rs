//! Agent Revision read APIs.

use super::super::{ApiError, ServerState, auth::authenticate};
use crate::revision::{RevisionError, RevisionStore};
use abar_api::revision::AgentRevision;
use axum::Json;
use axum::Router;
use axum::extract::{Path as AxumPath, State};
use axum::http::HeaderMap;
use axum::routing::get;
use serde::Serialize;
use std::sync::Arc;

pub(super) fn routes() -> Router<Arc<ServerState>> {
    Router::new()
        .route("/v1/agents/{agent}/revisions", get(list_revisions))
        .route(
            "/v1/agents/{agent}/revisions/current",
            get(get_current_revision),
        )
        .route("/v1/agents/{agent}/revisions/{revision}", get(get_revision))
}

#[derive(Serialize)]
struct RevisionListResponse {
    agent: String,
    revisions: Vec<u64>,
    current: Option<u64>,
}

async fn list_revisions(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    AxumPath(agent): AxumPath<String>,
) -> Result<Json<RevisionListResponse>, ApiError> {
    let _auth = authenticate(&state, &headers).await?;
    let store = store(&state);
    let revisions = store.revisions(&agent).map_err(revision_error)?;
    let current = match store.current_revision(&agent) {
        Ok(revision) => Some(revision),
        Err(RevisionError::CurrentNotSet(_)) => None,
        Err(error) => return Err(revision_error(error)),
    };
    Ok(Json(RevisionListResponse {
        agent,
        revisions,
        current,
    }))
}

async fn get_current_revision(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    AxumPath(agent): AxumPath<String>,
) -> Result<Json<AgentRevision>, ApiError> {
    let _auth = authenticate(&state, &headers).await?;
    store(&state)
        .current(&agent)
        .map(Json)
        .map_err(revision_error)
}

async fn get_revision(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    AxumPath((agent, revision)): AxumPath<(String, u64)>,
) -> Result<Json<AgentRevision>, ApiError> {
    let _auth = authenticate(&state, &headers).await?;
    store(&state)
        .get(&agent, revision)
        .map(Json)
        .map_err(revision_error)
}

fn store(state: &ServerState) -> RevisionStore {
    RevisionStore::new(state.root.join("revisions"))
}

fn revision_error(error: RevisionError) -> ApiError {
    match error {
        RevisionError::NotFound { .. } | RevisionError::CurrentNotSet(_) => {
            ApiError::not_found(error.to_string())
        }
        RevisionError::InvalidAgent(_)
        | RevisionError::InvalidRevision(_)
        | RevisionError::ReviewConflict(_) => ApiError::bad_request(error.to_string()),
        _ => ApiError::internal(error.to_string()),
    }
}
