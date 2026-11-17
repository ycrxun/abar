use crate::app::{AppError, AppState};
use crate::server_client::server_json;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ReviewRequest {
    decision: String,
    comment: Option<String>,
}

pub(crate) async fn overview(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let token = token(&state, &headers, false).await?;
    let overview = server_json(
        &state,
        &token,
        reqwest::Method::GET,
        "/v1/control-plane/overview",
        None,
    )
    .await?;
    Ok(Json(overview))
}

pub(crate) async fn review(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((agent, revision)): Path<(String, u64)>,
    Json(request): Json<ReviewRequest>,
) -> Result<Json<Value>, AppError> {
    if !matches!(request.decision.as_str(), "approved" | "rejected") {
        return Err(AppError::bad_request(
            "decision must be approved or rejected",
        ));
    }
    let token = token(&state, &headers, true).await?;
    let evidence = server_json(
        &state,
        &token,
        reqwest::Method::POST,
        &format!("/v1/control-plane/agents/{agent}/revisions/{revision}/review"),
        Some(json!({
            "decision": request.decision,
            "comment": request.comment
        })),
    )
    .await?;
    Ok(Json(evidence))
}

pub(crate) async fn current_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let session = state.auth.authenticate(&headers, false).await?;
    Ok(Json(json!({
        "mode": if state.auth.is_oidc() { "oidc" } else { "local" },
        "issuer": state.auth.issuer(),
        "subjectId": session.subject_id,
        "displayName": session.display_name,
        "tenant": session.tenant,
        "roles": session.roles,
        "csrfToken": session.csrf_token
    })))
}

async fn token(state: &AppState, headers: &HeaderMap, write: bool) -> Result<String, AppError> {
    Ok(state.auth.authenticate(headers, write).await?.access_token)
}
