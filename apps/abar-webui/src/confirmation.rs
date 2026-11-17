use crate::app::{AppError, AppState};
use crate::handlers::ensure_session_agent;
use crate::server_client::server_json;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ConfirmRequest {
    confirmation: Value,
    agent_revision: u64,
}

pub(crate) async fn confirm_plan(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((session_id, message_id)): Path<(String, String)>,
    Json(request): Json<ConfirmRequest>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    if !request
        .confirmation
        .as_object()
        .is_some_and(|value| !value.is_empty())
    {
        return Err(AppError::bad_request(
            "confirmation must be a non-empty object",
        ));
    }
    let auth = state.auth.authenticate(&headers, true).await?;
    let token = auth.access_token;
    ensure_session_agent(&state, &token, &session_id).await?;
    let message = server_json(
        &state,
        &token,
        reqwest::Method::GET,
        &format!("/v1/messages/{message_id}"),
        None,
    )
    .await?;
    if message.get("sessionId").and_then(Value::as_str) != Some(session_id.as_str())
        || message.get("state").and_then(Value::as_str) != Some("completed")
        || message.get("agentRevision").and_then(Value::as_u64) != Some(request.agent_revision)
    {
        return Err(AppError::bad_request(
            "selected Message is not a completed plan Message",
        ));
    }
    let evidence = message
        .get("toolResults")
        .and_then(Value::as_array)
        .filter(|values| !values.is_empty())
        .or_else(|| {
            message
                .get("checkpoint")
                .and_then(|value| value.get("toolResults"))
                .and_then(Value::as_array)
        })
        .ok_or_else(|| AppError::bad_request("selected Message has no plan evidence"))?;
    let matches = matches_confirmation_evidence(evidence, &request.confirmation);
    if !matches {
        return Err(AppError::bad_request(
            "confirmation does not match the selected Message evidence",
        ));
    }
    let requested_skills = state
        .view
        .pointer("/confirmation/requestedSkills")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let confirmation = json!({
        "confirmation": request.confirmation,
        "agentRevision": request.agent_revision
    });
    let template = state
        .view
        .pointer("/confirmation/inputTemplate")
        .and_then(Value::as_str)
        .unwrap_or("确认执行当前会话中已展示的业务方案。");
    let input = format!("[structured confirmation]\n{confirmation}\n{template}");
    let created = server_json(
        &state,
        &token,
        reqwest::Method::POST,
        &format!("/v1/sessions/{session_id}/messages"),
        Some(json!({
            "input": input,
            "requestedSkills": requested_skills
        })),
    )
    .await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "sessionId": session_id,
            "messageId": created.get("id"),
            "message": created
        })),
    ))
}

fn matches_confirmation_evidence(evidence: &[Value], expected: &Value) -> bool {
    evidence.iter().any(|result| {
        result
            .get("content")
            .and_then(Value::as_str)
            .and_then(|content| serde_json::from_str::<Value>(content).ok())
            .and_then(|result| result.get("confirmation").cloned())
            .is_some_and(|confirmation| confirmation == *expected)
    })
}

#[cfg(test)]
mod tests {
    use super::matches_confirmation_evidence;
    use serde_json::json;

    #[test]
    fn matches_the_exact_confirmation_from_message_evidence() {
        let confirmation = json!({
            "kind": "example",
            "digest": "sha256:abc",
            "snapshot": "snapshot-1"
        });
        let evidence = [json!({
            "content": json!({"confirmation": confirmation.clone()}).to_string()
        })];
        assert!(matches_confirmation_evidence(&evidence, &confirmation));
    }

    #[test]
    fn rejects_confirmation_that_differs_from_evidence() {
        let evidence = [json!({
            "content": json!({
                "confirmation": {
                    "kind": "example",
                    "digest": "sha256:abc"
                }
            })
            .to_string()
        })];
        assert!(!matches_confirmation_evidence(
            &evidence,
            &json!({"kind": "other", "digest": "sha256:abc"})
        ));
    }
}
