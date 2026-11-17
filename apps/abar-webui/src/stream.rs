use crate::app::{AppError, AppState};
use crate::handlers::ensure_session_agent;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::Response;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
pub(crate) struct StreamQuery {
    after: Option<u64>,
}

pub(crate) async fn stream_message(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((session_id, message_id)): Path<(String, String)>,
    Query(query): Query<StreamQuery>,
) -> Result<Response, AppError> {
    let auth = state.auth.authenticate(&headers, false).await?;
    ensure_session_agent(&state, &auth.access_token, &session_id).await?;
    let after = query.after.unwrap_or(0);
    let mut request = state
        .client
        .get(format!(
            "{}/v1/sessions/{session_id}/messages/{message_id}/stream",
            state.endpoint
        ))
        .bearer_auth(auth.access_token)
        .header(reqwest::header::ACCEPT, "text/event-stream");
    if let Some(last_event_id) = headers.get("last-event-id") {
        request = request.header("last-event-id", last_event_id);
    } else {
        request = request.query(&[("after", after)]);
    }
    let upstream = request
        .send()
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    if upstream.status() != StatusCode::OK {
        return Err(AppError::internal(format!(
            "Server stream returned {}",
            upstream.status()
        )));
    }
    let mut response = Response::new(Body::from_stream(upstream.bytes_stream()));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream; charset=utf-8"),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
        .headers_mut()
        .insert(header::CONNECTION, HeaderValue::from_static("keep-alive"));
    response.headers_mut().insert(
        HeaderName::from_static("x-accel-buffering"),
        HeaderValue::from_static("no"),
    );
    Ok(response)
}
