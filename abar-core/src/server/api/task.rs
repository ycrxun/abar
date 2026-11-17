use super::super::factory::RuntimeBundleFactory;
use super::super::{ApiError, ServerState, TaskRecord, auth::authenticate, message_worker};
use super::common::{request_context, revision_error, revision_store, session_error, store};
use super::message::message_service_error;
use crate::digest::sha256_hex;
use crate::message::prepare_message;
use abar_api::message::Message;
use abar_api::session::Session;
use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

pub(super) fn routes() -> Router<Arc<ServerState>> {
    Router::new().route("/v1/tasks", post(create_task))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateTaskRequest {
    request_id: String,
    agent: String,
    input: String,
    #[serde(default)]
    requested_skills: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskResponse {
    session: Session,
    message: Message,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PersistedTask {
    fingerprint: String,
    session_id: String,
    message_id: String,
}

async fn create_task(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Json(request): Json<CreateTaskRequest>,
) -> Result<(StatusCode, Json<TaskResponse>), ApiError> {
    validate_request_id(&request.request_id)?;
    if request.input.trim().is_empty()
        || request.input.len() > 8_000
        || request.agent.trim().is_empty()
    {
        return Err(ApiError::bad_request("invalid task request"));
    }
    let auth = authenticate(&state, &headers).await?;
    let key = format!(
        "{}\0{}\0{}",
        auth.tenant.tenant_id, auth.subject_id, request.request_id
    );
    let fingerprint_input = format!(
        "{}\0{}\0{}\0{}",
        request.agent,
        request.input,
        request.requested_skills.join(","),
        auth.subject_id
    );
    let fingerprint = sha256_hex(fingerprint_input.as_bytes());
    let task_key = sha256_hex(key.as_bytes());
    let revision = revision_store(&state)
        .current(&request.agent)
        .map_err(revision_error)?;
    if let Some(task) = existing_task(&state, &key, &task_key, &fingerprint)? {
        return Ok((StatusCode::OK, Json(task)));
    }

    let session_store = store(&state);
    let session =
        match session_store.create(&auth.tenant.tenant_id, &request.agent, &auth.subject_id) {
            Ok(session) => session,
            Err(error) => {
                remove_task(&state, &key);
                return Err(session_error(error));
            }
        };
    let adapter = RuntimeBundleFactory {
        factory: state.factory.clone(),
    };
    let message = prepare_message(
        &session_store,
        &request_context(&auth, &request.request_id),
        &session.id,
        &revision,
        &request.input,
        &request.requested_skills,
        &adapter,
    )
    .await;
    let message = match message {
        Ok(message) => message,
        Err(error) => {
            abandon_session(&session_store, &session.id);
            remove_task(&state, &key);
            return Err(message_service_error(error));
        }
    };
    let persisted = PersistedTask {
        fingerprint: fingerprint.clone(),
        session_id: session.id.clone(),
        message_id: message.id.clone(),
    };
    if let Err(error) = persist_task(&state, &task_key, &persisted) {
        eprintln!(
            "failed to persist Task idempotency record: {}",
            error.message
        );
    }
    message_worker::spawn(&state, &session.id, &message.id);
    state.tasks.lock().expect("task registry poisoned").insert(
        key,
        TaskRecord::Ready {
            fingerprint,
            session_id: session.id.clone(),
            message_id: message.id.clone(),
        },
    );
    Ok((
        StatusCode::ACCEPTED,
        Json(TaskResponse { session, message }),
    ))
}

fn existing_task(
    state: &ServerState,
    key: &str,
    task_key: &str,
    fingerprint: &str,
) -> Result<Option<TaskResponse>, ApiError> {
    let existing = state
        .tasks
        .lock()
        .expect("task registry poisoned")
        .get(key)
        .map(|record| match record {
            TaskRecord::Preparing { fingerprint: value } => (value.clone(), None),
            TaskRecord::Ready {
                fingerprint: value,
                session_id,
                message_id,
            } => (
                value.clone(),
                Some((session_id.clone(), message_id.clone())),
            ),
        });
    let existing = existing.or_else(|| {
        load_task(state, task_key)
            .map(|task| (task.fingerprint, Some((task.session_id, task.message_id))))
    });
    let Some((stored_fingerprint, ids)) = existing else {
        state.tasks.lock().expect("task registry poisoned").insert(
            key.to_owned(),
            TaskRecord::Preparing {
                fingerprint: fingerprint.to_owned(),
            },
        );
        return Ok(None);
    };
    if stored_fingerprint != fingerprint {
        return Err(ApiError::conflict(
            "requestId is already bound to another task request",
        ));
    }
    let Some((session_id, message_id)) = ids else {
        return Err(ApiError::conflict("task request is still being prepared"));
    };
    let session_store = store(state);
    match (
        session_store.get(&session_id),
        session_store.get_message(&session_id, &message_id),
    ) {
        (Ok(session), Ok(message)) => Ok(Some(TaskResponse { session, message })),
        _ => {
            remove_task(state, key);
            remove_persisted_task(state, task_key)?;
            Ok(None)
        }
    }
}

fn remove_task(state: &ServerState, key: &str) {
    state
        .tasks
        .lock()
        .expect("task registry poisoned")
        .remove(key);
}

fn task_path(state: &ServerState, task_key: &str) -> PathBuf {
    state.root.join("tasks").join(format!("{task_key}.json"))
}

fn load_task(state: &ServerState, task_key: &str) -> Option<PersistedTask> {
    let bytes = fs::read(task_path(state, task_key)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn persist_task(state: &ServerState, task_key: &str, task: &PersistedTask) -> Result<(), ApiError> {
    let path = task_path(state, task_key);
    fs::create_dir_all(path.parent().expect("task path has parent"))
        .map_err(|error| ApiError::internal(error.to_string()))?;
    let temporary = path.with_extension("tmp");
    let mut file =
        File::create(&temporary).map_err(|error| ApiError::internal(error.to_string()))?;
    let bytes = serde_json::to_vec(task).map_err(|error| ApiError::internal(error.to_string()))?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| ApiError::internal(error.to_string()))?;
    fs::rename(temporary, path).map_err(|error| ApiError::internal(error.to_string()))
}

fn remove_persisted_task(state: &ServerState, task_key: &str) -> Result<(), ApiError> {
    match fs::remove_file(task_path(state, task_key)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(ApiError::internal(error.to_string())),
    }
}

fn abandon_session(store: &crate::session::SessionStore, session_id: &str) {
    if store.close(session_id).is_ok() {
        let _ = store.archive(session_id);
    }
}

fn validate_request_id(value: &str) -> Result<(), ApiError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(ApiError::bad_request("invalid requestId"));
    }
    Ok(())
}
