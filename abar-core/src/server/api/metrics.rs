//! Internal readiness and Prometheus metrics.

use super::super::{ApiError, ServerState, observability};
use super::common::{session_error, store};
use abar_api::message::MessageState;
use abar_api::session::SessionStatus;
use axum::Router;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::json;
use std::fmt::Write;
use std::sync::Arc;

pub(super) fn routes() -> Router<Arc<ServerState>> {
    Router::new()
        .route("/readyz", get(ready))
        .route("/metrics", get(metrics))
}

async fn ready(State(state): State<Arc<ServerState>>) -> Response {
    let provider = state.factory.validate_message_runtime().is_ok();
    let storage = observability::storage_writable(&state.root);
    let status = if provider && storage {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        status,
        axum::Json(json!({
            "status": if provider && storage { "ready" } else { "degraded" },
            "providerConfigured": provider,
            "storageWritable": storage
        })),
    )
        .into_response()
}

async fn metrics(State(state): State<Arc<ServerState>>) -> Result<Response, ApiError> {
    let sessions = store(&state).list_all().map_err(session_error)?;
    let mut session_counts = [0_u64; 4];
    let mut message_counts = [0_u64; 7];
    let mut prompt_tokens = 0_u64;
    let mut completion_tokens = 0_u64;
    let mut total_tokens = 0_u64;
    for session in sessions {
        session_counts[match session.status {
            SessionStatus::Active => 0,
            SessionStatus::Idle => 1,
            SessionStatus::Closed => 2,
            SessionStatus::Archived => 3,
        }] += 1;
        for message in store(&state)
            .list_messages(&session.id)
            .map_err(session_error)?
        {
            prompt_tokens = prompt_tokens.saturating_add(message.model_usage.prompt_tokens);
            completion_tokens =
                completion_tokens.saturating_add(message.model_usage.completion_tokens);
            total_tokens = total_tokens.saturating_add(message.model_usage.total_tokens);
            message_counts[match message.state {
                MessageState::Queued => 0,
                MessageState::Running => 1,
                MessageState::WaitingApproval => 2,
                MessageState::WaitingExternal => 3,
                MessageState::Completed => 4,
                MessageState::Failed => 5,
                MessageState::Cancelled => 6,
            }] += 1;
        }
    }
    let snapshot = state.metrics.snapshot();
    let active_workers = state
        .cancellations
        .lock()
        .expect("cancellation map poisoned")
        .len();
    let provider = state.factory.validate_message_runtime().is_ok();
    let storage = observability::storage_writable(&state.root);
    let mut output = String::new();
    writeln!(output, "# HELP abar_up ABAR Server process is running.").ok();
    writeln!(output, "# TYPE abar_up gauge\nabar_up 1").ok();
    writeln!(
        output,
        "# TYPE abar_uptime_seconds gauge\nabar_uptime_seconds {}",
        state.metrics.uptime_seconds()
    )
    .ok();
    writeln!(
        output,
        "# TYPE abar_ready gauge\nabar_ready {}",
        u8::from(provider && storage)
    )
    .ok();
    writeln!(
        output,
        "# TYPE abar_active_workers gauge\nabar_active_workers {active_workers}"
    )
    .ok();
    writeln!(
        output,
        "# TYPE abar_http_requests_total counter\nabar_http_requests_total {}",
        snapshot.requests_total
    )
    .ok();
    writeln!(
        output,
        "# TYPE abar_http_errors_total counter\nabar_http_errors_total {}",
        snapshot.request_errors_total
    )
    .ok();
    writeln!(
        output,
        "# TYPE abar_http_request_duration_seconds_sum counter\nabar_http_request_duration_seconds_sum {}",
        snapshot.request_duration_micros_total as f64 / 1_000_000.0
    )
    .ok();
    writeln!(
        output,
        "# TYPE abar_http_request_duration_seconds_count counter\nabar_http_request_duration_seconds_count {}",
        snapshot.requests_total
    )
    .ok();
    writeln!(
        output,
        "# TYPE abar_message_worker_failures_total counter\nabar_message_worker_failures_total {}",
        snapshot.worker_failures_total
    )
    .ok();
    writeln!(
        output,
        "# TYPE abar_approval_decisions_total counter\nabar_approval_decisions_total{{decision=\"approved\"}} {}",
        snapshot.approvals_granted_total
    )
    .ok();
    writeln!(
        output,
        "abar_approval_decisions_total{{decision=\"rejected\"}} {}",
        snapshot.approvals_rejected_total
    )
    .ok();
    write_session_metrics(&mut output, &session_counts);
    write_message_metrics(&mut output, &message_counts);
    writeln!(output, "# TYPE abar_model_tokens_total counter").ok();
    writeln!(
        output,
        "abar_model_tokens_total{{type=\"prompt\"}} {prompt_tokens}"
    )
    .ok();
    writeln!(
        output,
        "abar_model_tokens_total{{type=\"completion\"}} {completion_tokens}"
    )
    .ok();
    writeln!(
        output,
        "abar_model_tokens_total{{type=\"total\"}} {total_tokens}"
    )
    .ok();
    writeln!(
        output,
        "# TYPE abar_storage_writable gauge\nabar_storage_writable {}",
        u8::from(storage)
    )
    .ok();
    writeln!(
        output,
        "# TYPE abar_provider_configured gauge\nabar_provider_configured {}",
        u8::from(provider)
    )
    .ok();
    Ok((
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        output,
    )
        .into_response())
}

fn write_session_metrics(output: &mut String, counts: &[u64; 4]) {
    writeln!(output, "# TYPE abar_sessions gauge").ok();
    for (status, count) in ["active", "idle", "closed", "archived"]
        .into_iter()
        .zip(counts)
    {
        writeln!(output, "abar_sessions{{status=\"{status}\"}} {count}").ok();
    }
}

fn write_message_metrics(output: &mut String, counts: &[u64; 7]) {
    writeln!(output, "# TYPE abar_messages gauge").ok();
    for (state, count) in [
        "queued",
        "running",
        "waiting_approval",
        "waiting_external",
        "completed",
        "failed",
        "cancelled",
    ]
    .into_iter()
    .zip(counts)
    {
        writeln!(output, "abar_messages{{state=\"{state}\"}} {count}").ok();
    }
}
