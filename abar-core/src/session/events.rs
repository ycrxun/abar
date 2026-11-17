//! Append-only Runtime Event projection for Messages.

use super::{Message, Session, SessionError, SessionStore, now_rfc3339};
use abar_api::runtime_event::RuntimeEvent;
use serde_json::Value;
use std::fs::{self, OpenOptions};
use std::io::Write;

impl SessionStore {
    /// Appends one Message event and fsyncs it before returning.
    pub fn append_event(
        &self,
        session_id: &str,
        message_id: &str,
        event_type: &str,
        data: Value,
    ) -> Result<RuntimeEvent, SessionError> {
        self.get_message(session_id, message_id)?;
        let _lock = self.lock()?;
        let path = self.event_path(session_id, message_id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let sequence = self
            .list_events(session_id, message_id, 0)?
            .last()
            .map_or(1, |event| event.sequence + 1);
        let event = RuntimeEvent {
            event_id: format!("{message_id}:{sequence}"),
            session_id: session_id.to_owned(),
            message_id: message_id.to_owned(),
            sequence,
            event_type: event_type.to_owned(),
            at: now_rfc3339()?,
            schema_version: "runtime-event/v1".to_owned(),
            data,
        };
        let mut line = serde_json::to_vec(&event)?;
        line.push(b'\n');
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        file.write_all(&line)?;
        file.sync_all()?;
        Ok(event)
    }

    /// Reads committed Message events after one cursor.
    pub fn list_events(
        &self,
        session_id: &str,
        message_id: &str,
        after: u64,
    ) -> Result<Vec<RuntimeEvent>, SessionError> {
        let path = self.event_path(session_id, message_id);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(path)?;
        let mut events = Vec::new();
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let event = match serde_json::from_str::<RuntimeEvent>(line) {
                Ok(event) => event,
                Err(_) if !content.ends_with('\n') => break,
                Err(error) => return Err(error.into()),
            };
            if event.sequence > after {
                events.push(event);
            }
        }
        Ok(events)
    }

    /// Finds a Message visible to one user.
    pub fn find_message_for_user(
        &self,
        tenant: &str,
        user: &str,
        message_id: &str,
    ) -> Result<(Session, Message), SessionError> {
        if !self.root.exists() {
            return Err(SessionError::MessageNotFound(message_id.to_owned()));
        }
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let Some(session_id) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let session = self.get(&session_id)?;
            if session.tenant != tenant || session.user != user {
                continue;
            }
            if let Ok(message) = self.get_message(&session_id, message_id) {
                return Ok((session, message));
            }
        }
        Err(SessionError::MessageNotFound(message_id.to_owned()))
    }

    fn event_path(&self, session_id: &str, message_id: &str) -> std::path::PathBuf {
        self.session_dir(session_id)
            .join("messages")
            .join(format!("{message_id}.events.jsonl"))
    }
}
