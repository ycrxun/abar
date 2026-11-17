use super::{SessionError, SessionStore};
use abar_api::message::{
    Message, MessageApproval, MessageCheckpoint, MessageState, PendingToolCall,
};
use abar_api::session::SessionStatus;

fn message(session_id: &str, id: &str) -> Message {
    let mut message: Message =
        serde_json::from_str(include_str!("../../../abar-api/fixtures/core/message.json")).unwrap();
    message.id = id.to_owned();
    message.session_id = session_id.to_owned();
    message.state = MessageState::Queued;
    message
}

#[test]
fn keeps_session_active_across_messages() {
    let root = tempfile::tempdir().unwrap();
    let store = SessionStore::new(root.path());
    let session = store.create("acme", "example-assistant", "alice").unwrap();
    let first = message(&session.id, "msg-1");
    store.put_message(&first).unwrap();
    store
        .transition_message(&session.id, "msg-1", MessageState::Running)
        .unwrap();
    store
        .transition_message(&session.id, "msg-1", MessageState::Completed)
        .unwrap();
    let second = message(&session.id, "msg-2");
    store.put_message(&second).unwrap();
    assert_eq!(store.list_messages(&session.id).unwrap().len(), 2);
    assert_eq!(
        store.get(&session.id).unwrap().status,
        SessionStatus::Active
    );
}

#[test]
fn enforces_single_active_message() {
    let root = tempfile::tempdir().unwrap();
    let store = SessionStore::new(root.path());
    let session = store.create("acme", "example-assistant", "alice").unwrap();
    store.put_message(&message(&session.id, "msg-1")).unwrap();
    assert!(matches!(
        store.put_message(&message(&session.id, "msg-2")),
        Err(SessionError::ActiveMessageExists)
    ));
}

#[test]
fn closes_and_archives_idle_session() {
    let root = tempfile::tempdir().unwrap();
    let store = SessionStore::new(root.path());
    let session = store.create("acme", "example-assistant", "alice").unwrap();
    let closed = store.close(&session.id).unwrap();
    assert_eq!(closed.status, SessionStatus::Closed);
    let archived = store.archive(&session.id).unwrap();
    assert_eq!(archived.status, SessionStatus::Archived);
}

#[test]
fn rejected_approval_returns_session_to_idle() {
    let root = tempfile::tempdir().unwrap();
    let store = SessionStore::new(root.path());
    let session = store.create("acme", "example-assistant", "alice").unwrap();
    let fixture: Message =
        serde_json::from_str(include_str!("../../../abar-api/fixtures/core/message.json")).unwrap();
    let message = store
        .create_message(
            &session.id,
            18,
            "perform operation",
            &[],
            fixture.composition,
        )
        .unwrap();
    store
        .transition_message(&session.id, &message.id, MessageState::Running)
        .unwrap();
    store
        .request_approval(
            &session.id,
            &message.id,
            PendingToolCall {
                approval_id: "approval-1".to_owned(),
                tool_id: "records.apply".to_owned(),
                arguments_json: "{}".to_owned(),
                idempotency_key: "key-1".to_owned(),
            },
            MessageCheckpoint {
                next_step: 1,
                tool_results: Vec::new(),
                model_usage: Default::default(),
            },
        )
        .unwrap();
    let indexed = store
        .list_approval_entries("acme", Some("example-assistant"))
        .unwrap();
    assert_eq!(indexed.len(), 1);
    assert_eq!(indexed[0].approval_id, "approval-1");
    assert!(
        store
            .list_approval_entries("other", Some("example-assistant"))
            .unwrap()
            .is_empty()
    );
    std::fs::remove_file(root.path().join("approval-index.json")).unwrap();
    assert_eq!(
        store
            .list_approval_entries("acme", Some("example-assistant"))
            .unwrap()
            .len(),
        1
    );
    let rejected = store
        .submit_approval(
            &session.id,
            &message.id,
            MessageApproval {
                approval_id: "approval-1".to_owned(),
                actor: "manager".to_owned(),
                scopes: vec!["east".to_owned()],
                decision: "rejected".to_owned(),
                comment: None,
                reviewed_at: "2026-11-17T09:01:00Z".to_owned(),
            },
        )
        .unwrap();
    assert_eq!(rejected.state, MessageState::Cancelled);
    assert!(rejected.pending_approval.is_none());
    assert!(
        store
            .list_approval_entries("acme", Some("example-assistant"))
            .unwrap()
            .is_empty()
    );
    assert_eq!(store.get(&session.id).unwrap().status, SessionStatus::Idle);
}
