use crate::app::{AppError, AppState};
use crate::presentation::visible_message_input;
use crate::server_client::server_json;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CreateTaskRequest {
    input: String,
    request_id: String,
}

#[derive(Deserialize)]
pub(crate) struct AppendMessageRequest {
    input: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ApprovalRequest {
    approval_id: String,
    decision: String,
    #[serde(default)]
    scopes: Vec<String>,
    comment: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct ReconcileRequest {
    action: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DetailQuery {
    message_id: Option<String>,
    event_after: Option<u64>,
}

pub(crate) async fn list_sessions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let token = token(&state, &headers, false).await?;
    let response = server_json(&state, &token, reqwest::Method::GET, "/v1/sessions", None).await?;
    let mut sessions = response
        .get("sessions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    sessions.retain(|session| {
        session
            .get("agent")
            .and_then(Value::as_str)
            .is_some_and(|agent| agent == state.agent.as_str())
    });
    for session in &mut sessions {
        if let Some(input) = session
            .get("latestMessageInput")
            .and_then(Value::as_str)
            .map(visible_message_input)
        {
            session["latestMessageInput"] = Value::String(input);
        }
        if let Some(state_value) = session
            .get("latestMessageState")
            .filter(|value| !value.is_null())
            .cloned()
        {
            session["state"] = state_value;
        }
    }
    Ok(Json(json!({"sessions": sessions})))
}

pub(crate) async fn create_task(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<CreateTaskRequest>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let input = request.input.trim();
    if input.is_empty() || input.len() > 8_000 {
        return Err(AppError::bad_request(
            "task input must contain 1 to 8000 bytes",
        ));
    }
    let token = token(&state, &headers, true).await?;
    let task = server_json(
        &state,
        &token,
        reqwest::Method::POST,
        "/v1/tasks",
        Some(json!({
            "requestId": request.request_id,
            "agent": state.agent,
            "input": input,
            "requestedSkills": []
        })),
    )
    .await?;
    let session_id = task
        .pointer("/session/id")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::internal("created Task has no Session id"))?;
    let message = task
        .get("message")
        .cloned()
        .ok_or_else(|| AppError::internal("created Task has no Message"))?;
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "sessionId": session_id,
            "messageId": message.get("id"),
            "message": message
        })),
    ))
}

pub(crate) async fn append_message(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(request): Json<AppendMessageRequest>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let input = request.input.trim();
    if input.is_empty() || input.len() > 8_000 {
        return Err(AppError::bad_request(
            "task input must contain 1 to 8000 bytes",
        ));
    }
    let token = token(&state, &headers, true).await?;
    ensure_session_agent(&state, &token, &session_id).await?;
    let message = server_json(
        &state,
        &token,
        reqwest::Method::POST,
        &format!("/v1/sessions/{session_id}/messages"),
        Some(json!({"input": input, "requestedSkills": []})),
    )
    .await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "sessionId": session_id,
            "messageId": message.get("id"),
            "message": message
        })),
    ))
}

pub(crate) async fn session_detail(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<DetailQuery>,
) -> Result<Json<Value>, AppError> {
    let token = token(&state, &headers, false).await?;
    let session = server_json(
        &state,
        &token,
        reqwest::Method::GET,
        &format!("/v1/sessions/{id}"),
        None,
    )
    .await?;
    assert_session_agent(&state, &session)?;
    let messages = server_json(
        &state,
        &token,
        reqwest::Method::GET,
        &format!("/v1/sessions/{id}/messages"),
        None,
    )
    .await?;
    let message_values = messages
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let message = query
        .message_id
        .as_deref()
        .and_then(|message_id| {
            message_values
                .iter()
                .find(|message| message.get("id").and_then(Value::as_str) == Some(message_id))
        })
        .or_else(|| message_values.last())
        .cloned();
    let message_history = message_values
        .iter()
        .map(|message| {
            let input = message
                .get("input")
                .and_then(Value::as_str)
                .map(visible_message_input);
            json!({
                "id": message.get("id"),
                "state": message.get("state"),
                "input": input,
                "output": message.get("output"),
                "modelUsage": message.get("modelUsage"),
                "createdAt": message.get("createdAt")
            })
        })
        .collect::<Vec<_>>();
    let (message_id, events) = if let Some(message) = &message {
        let message_id = message
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let events = server_json(
            &state,
            &token,
            reqwest::Method::GET,
            &format!(
                "/v1/messages/{message_id}/events?after={}",
                query.event_after.unwrap_or(0)
            ),
            None,
        )
        .await?
        .get("events")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
        (Some(message_id.to_owned()), events)
    } else {
        (None, Vec::new())
    };
    let mut audit = Vec::new();
    for event in &events {
        let event_type = event
            .get("eventType")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let data = event.get("data").cloned().unwrap_or_else(|| json!({}));
        audit.push(json!({
            "sequence": event.get("sequence"),
            "type": event_type,
            "at": event.get("at"),
            "data": data,
            "summary": event_summary(event_type, &data)
        }));
    }
    let evidence_results = message
        .as_ref()
        .and_then(|message| message.get("toolResults"))
        .and_then(Value::as_array)
        .filter(|results| !results.is_empty())
        .or_else(|| {
            message
                .as_ref()
                .and_then(|message| message.get("checkpoint"))
                .and_then(|checkpoint| checkpoint.get("toolResults"))
                .and_then(Value::as_array)
        });
    let tool_results = evidence_results
        .map(|results| {
            results
                .iter()
                .map(|result| {
                    let content = result
                        .get("content")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    json!({
                        "toolId": result.get("toolId"),
                        "content": content,
                        "result": serde_json::from_str::<Value>(content)
                            .unwrap_or_else(|_| Value::String(content.to_owned()))
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let message_state = message
        .as_ref()
        .and_then(|message| message.get("state"))
        .cloned()
        .unwrap_or_else(|| session.get("status").cloned().unwrap_or(Value::Null));
    let event_cursor = events
        .last()
        .and_then(|event| event.get("sequence"))
        .and_then(Value::as_u64)
        .unwrap_or(query.event_after.unwrap_or(0));
    Ok(Json(json!({
        "id": id,
        "agent": session.get("agent"),
        "user": session.get("user"),
        "state": message_state,
        "sessionStatus": session.get("status"),
        "messageId": message_id,
        "messages": message_history,
        "agentRevision": message.as_ref().and_then(|value| value.get("agentRevision")),
        "createdAt": message.as_ref().and_then(|value| value.get("createdAt")).or_else(|| session.get("createdAt")),
        "updatedAt": message.as_ref().and_then(|value| value.get("updatedAt")).or_else(|| session.get("lastActiveAt")),
        "pendingApproval": message.as_ref().and_then(|value| value.get("pendingApproval")).cloned().unwrap_or(Value::Null),
        "approval": message.as_ref().and_then(|value| value.get("approval")).cloned().unwrap_or(Value::Null),
        "composition": message.as_ref().and_then(|value| value.get("composition")).cloned().unwrap_or(Value::Null),
        "output": message.as_ref().and_then(|value| value.get("output")).cloned().unwrap_or(Value::Null),
        "modelUsage": message.as_ref().and_then(|value| value.get("modelUsage")).cloned().unwrap_or_else(|| json!({
            "promptTokens": 0,
            "completionTokens": 0,
            "totalTokens": 0
        })),
        "toolResults": tool_results,
        "events": audit,
        "eventCursor": event_cursor
    })))
}

pub(crate) async fn submit_approval(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((session_id, message_id)): Path<(String, String)>,
    Json(request): Json<ApprovalRequest>,
) -> Result<Json<Value>, AppError> {
    let token = token(&state, &headers, true).await?;
    ensure_session_agent(&state, &token, &session_id).await?;
    if !matches!(request.decision.as_str(), "approved" | "rejected") {
        return Err(AppError::bad_request(
            "decision must be approved or rejected",
        ));
    }
    let body = json!({
        "approvalId": request.approval_id,
        "decision": request.decision,
        "scopes": request.scopes,
        "comment": request.comment
    });
    let value = server_json(
        &state,
        &token,
        reqwest::Method::POST,
        &format!("/v1/sessions/{session_id}/messages/{message_id}/approval"),
        Some(body),
    )
    .await?;
    Ok(Json(value))
}

pub(crate) async fn cancel_message(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(message_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let token = token(&state, &headers, true).await?;
    ensure_message_agent(&state, &token, &message_id).await?;
    let value = server_json(
        &state,
        &token,
        reqwest::Method::POST,
        &format!("/v1/messages/{message_id}/cancel"),
        None,
    )
    .await?;
    Ok(Json(value))
}

pub(crate) async fn reconcile_message(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(message_id): Path<String>,
    Json(request): Json<ReconcileRequest>,
) -> Result<Json<Value>, AppError> {
    let token = token(&state, &headers, true).await?;
    ensure_message_agent(&state, &token, &message_id).await?;
    if !matches!(request.action.as_str(), "query" | "cancel") {
        return Err(AppError::bad_request("action must be query or cancel"));
    }
    let value = server_json(
        &state,
        &token,
        reqwest::Method::POST,
        &format!("/v1/messages/{message_id}/reconcile"),
        Some(json!({"action": request.action})),
    )
    .await?;
    Ok(Json(value))
}

pub(crate) async fn list_approvals(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let token = token(&state, &headers, false).await?;
    let response = server_json(&state, &token, reqwest::Method::GET, "/v1/approvals", None).await?;
    let approvals = response
        .get("approvals")
        .and_then(Value::as_array)
        .map(|approvals| {
            approvals
                .iter()
                .filter(|approval| {
                    approval
                        .get("agent")
                        .and_then(Value::as_str)
                        .is_some_and(|agent| agent == state.agent.as_str())
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(Json(json!({"approvals": approvals})))
}

async fn token(state: &AppState, headers: &HeaderMap, write: bool) -> Result<String, AppError> {
    Ok(state.auth.authenticate(headers, write).await?.access_token)
}

pub(crate) async fn ensure_session_agent(
    state: &AppState,
    token: &str,
    session_id: &str,
) -> Result<(), AppError> {
    let session = server_json(
        state,
        token,
        reqwest::Method::GET,
        &format!("/v1/sessions/{session_id}"),
        None,
    )
    .await?;
    assert_session_agent(state, &session)
}

async fn ensure_message_agent(
    state: &AppState,
    token: &str,
    message_id: &str,
) -> Result<(), AppError> {
    let message = server_json(
        state,
        token,
        reqwest::Method::GET,
        &format!("/v1/messages/{message_id}"),
        None,
    )
    .await?;
    let session_id = message
        .get("sessionId")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::internal("Message has no Session id"))?;
    ensure_session_agent(state, token, session_id).await
}

fn assert_session_agent(state: &AppState, session: &Value) -> Result<(), AppError> {
    if session.get("agent").and_then(Value::as_str) == Some(state.agent.as_str()) {
        Ok(())
    } else {
        Err(AppError::forbidden(
            "Session is not bound to this WebUI Agent",
        ))
    }
}

fn event_summary(event_type: &str, data: &Value) -> String {
    match event_type {
        "approval.requested" => format!(
            "工具 {} 等待审批",
            data.get("toolId")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        ),
        "approval.granted" | "approval.rejected" => format!(
            "{} · 范围 {}",
            data.get("actor")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            data.get("scopes").cloned().unwrap_or_else(|| json!([]))
        ),
        "message.waiting_external" => data
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("等待外部结果")
            .to_owned(),
        "message.failed" => data
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("执行失败")
            .to_owned(),
        "migration.completed" => "旧 Session 已导入，自动执行已关闭".to_owned(),
        _ => String::new(),
    }
}
