//! Durable Message worker scheduling.

use super::factory::RuntimeBundleFactory;
use super::{ServerState, observability};
use crate::message::{execute_prepared_message, reconcile_message, resume_message};
use crate::revision::RevisionStore;
use crate::session::SessionStore;
use abar_api::context::{RequestContext, TenantContext};
use abar_api::message::MessageState;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) async fn recover(state: Arc<ServerState>) -> Result<(), super::ServerError> {
    let sessions = SessionStore::new(state.root.join("sessions"));
    for session in sessions.list_all()? {
        for message in sessions.list_messages(&session.id)? {
            match message.state {
                MessageState::Queued | MessageState::Running | MessageState::WaitingExternal => {
                    spawn(&state, &session.id, &message.id);
                }
                _ => {}
            }
        }
    }
    Ok(())
}

pub(super) fn spawn(state: &Arc<ServerState>, session_id: &str, message_id: &str) {
    let cancellation = Arc::new(AtomicBool::new(false));
    {
        let mut cancellations = state
            .cancellations
            .lock()
            .expect("cancellation map poisoned");
        if cancellations.contains_key(message_id) {
            return;
        }
        cancellations.insert(message_id.to_owned(), cancellation.clone());
    }
    let task_state = state.clone();
    let session_id = session_id.to_owned();
    let message_id = message_id.to_owned();
    observability::event(
        "info",
        "message.worker_started",
        serde_json::json!({"sessionId": session_id, "messageId": message_id}),
    );
    tokio::spawn(async move {
        let result = run(&task_state, &session_id, &message_id, cancellation.clone()).await;
        if let Err(error) = &result {
            task_state.metrics.record_worker_failure();
            eprintln!("Message {message_id} stopped: {error}");
            let sessions = SessionStore::new(task_state.root.join("sessions"));
            if let Ok(message) = sessions.get_message(&session_id, &message_id)
                && message.state.is_active()
            {
                let _ = sessions.fail_message(&session_id, &message_id, error.clone());
                let _ = sessions.append_event(
                    &session_id,
                    &message_id,
                    "message.failed",
                    serde_json::json!({"error": error}),
                );
            }
        }
        observability::event(
            if result.is_ok() { "info" } else { "error" },
            "message.worker_finished",
            serde_json::json!({
                "sessionId": session_id,
                "messageId": message_id,
                "outcome": if result.is_ok() { "finished" } else { "failed" },
                "error": result.err()
            }),
        );
        let mut cancellations = task_state
            .cancellations
            .lock()
            .expect("cancellation map poisoned");
        if cancellations
            .get(&message_id)
            .is_some_and(|active| Arc::ptr_eq(active, &cancellation))
        {
            cancellations.remove(&message_id);
        }
    });
}

pub(super) fn cancel(state: &ServerState, message_id: &str) -> bool {
    state
        .cancellations
        .lock()
        .expect("cancellation map poisoned")
        .get(message_id)
        .is_some_and(|token| {
            token.store(true, Ordering::Release);
            true
        })
}

async fn run(
    state: &Arc<ServerState>,
    session_id: &str,
    message_id: &str,
    cancellation: Arc<AtomicBool>,
) -> Result<(), String> {
    let sessions = SessionStore::new(state.root.join("sessions"));
    let session = sessions
        .get(session_id)
        .map_err(|error| error.to_string())?;
    let message = sessions
        .get_message(session_id, message_id)
        .map_err(|error| error.to_string())?;
    let revisions = RevisionStore::new(state.root.join("revisions"));
    let revision = revisions
        .get(&session.agent, message.agent_revision)
        .map_err(|error| error.to_string())?;
    let context = RequestContext::new(session_id, TenantContext::new(session.tenant));
    let factory = RuntimeBundleFactory {
        factory: state.factory.clone(),
    };
    if message.state == MessageState::WaitingExternal {
        reconcile_message(
            &sessions,
            &context,
            session_id,
            message_id,
            &revision,
            &factory,
            Some(cancellation),
        )
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
    } else if message.pending_approval.is_some() {
        resume_message(
            &sessions,
            &context,
            session_id,
            message_id,
            &revision,
            &factory,
            Some(cancellation),
        )
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
    } else {
        execute_prepared_message(
            &sessions,
            &context,
            session_id,
            message_id,
            &revision,
            &factory,
            Some(cancellation),
        )
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
    }
}
