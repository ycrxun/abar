//! Durable long-lived Sessions and Message state transitions.

mod approval;
mod approval_index;
mod events;

use abar_api::message::{ExecutionComposition, Message, MessageState, TokenUsage};
use abar_api::session::{Session, SessionStatus};
use fs2::FileExt;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

static SESSION_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Session or Message persistence failure.
#[derive(Debug, Error)]
pub enum SessionError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid identifier: {0}")]
    InvalidIdentifier(String),
    #[error("Session not found: {0}")]
    SessionNotFound(String),
    #[error("Message not found: {0}")]
    MessageNotFound(String),
    #[error("Session already has an active Message")]
    ActiveMessageExists,
    #[error("Message already exists: {0}")]
    MessageExists(String),
    #[error("invalid Session transition")]
    InvalidSessionTransition,
    #[error("invalid Message transition: {from:?} -> {to:?}")]
    InvalidMessageTransition {
        from: MessageState,
        to: MessageState,
    },
    #[error("system clock is outside supported range")]
    Clock,
}

/// Filesystem-backed Session and Message store.
#[derive(Clone, Debug)]
pub struct SessionStore {
    root: PathBuf,
}

impl SessionStore {
    /// Creates a store rooted at `root`.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Creates an active Session.
    pub fn create(&self, tenant: &str, agent: &str, user: &str) -> Result<Session, SessionError> {
        validate_value("tenant", tenant)?;
        validate_value("agent", agent)?;
        validate_value("user", user)?;
        let _lock = self.lock()?;
        let now = now_rfc3339()?;
        let session = Session {
            id: generate_session_id()?,
            tenant: tenant.to_owned(),
            agent: agent.to_owned(),
            user: user.to_owned(),
            status: SessionStatus::Active,
            created_at: now.clone(),
            last_active_at: now,
        };
        let directory = self.session_dir(&session.id);
        fs::create_dir_all(directory.join("messages"))?;
        write_json_atomic(&directory.join("session.json"), &session)?;
        Ok(session)
    }

    /// Returns one Session.
    pub fn get(&self, session_id: &str) -> Result<Session, SessionError> {
        validate_identifier("session", session_id)?;
        let path = self.session_dir(session_id).join("session.json");
        if !path.exists() {
            return Err(SessionError::SessionNotFound(session_id.to_owned()));
        }
        Ok(serde_json::from_slice(&fs::read(path)?)?)
    }

    /// Lists Sessions visible to one user, most recently active first.
    pub fn list_for_user(
        &self,
        tenant: &str,
        user: &str,
        agent: Option<&str>,
    ) -> Result<Vec<Session>, SessionError> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let mut sessions = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let Some(session_id) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let session = self.get(&session_id)?;
            if session.tenant == tenant
                && session.user == user
                && agent.is_none_or(|agent| session.agent == agent)
            {
                sessions.push(session);
            }
        }
        sessions.sort_by(|left, right| right.last_active_at.cmp(&left.last_active_at));
        Ok(sessions)
    }

    /// Lists all Sessions in one Tenant, most recently active first.
    pub fn list_for_tenant(&self, tenant: &str) -> Result<Vec<Session>, SessionError> {
        let mut sessions = self.list_all()?;
        sessions.retain(|session| session.tenant == tenant);
        sessions.sort_by(|left, right| right.last_active_at.cmp(&left.last_active_at));
        Ok(sessions)
    }

    /// Lists every Session in the local store.
    pub fn list_all(&self) -> Result<Vec<Session>, SessionError> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let mut sessions = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            if entry.file_type()?.is_dir()
                && let Some(session_id) = entry.file_name().to_str()
            {
                sessions.push(self.get(session_id)?);
            }
        }
        Ok(sessions)
    }

    /// Stores a newly-created active Message.
    pub fn put_message(&self, message: &Message) -> Result<(), SessionError> {
        validate_identifier("session", &message.session_id)?;
        validate_identifier("message", &message.id)?;
        let _lock = self.lock()?;
        let mut session = self.get(&message.session_id)?;
        if session.status == SessionStatus::Closed || session.status == SessionStatus::Archived {
            return Err(SessionError::InvalidSessionTransition);
        }
        let path = self.message_path(&message.session_id, &message.id);
        if path.exists() {
            return Err(SessionError::MessageExists(message.id.clone()));
        }
        if !message.state.is_active() {
            return Err(SessionError::InvalidMessageTransition {
                from: MessageState::Queued,
                to: message.state,
            });
        }
        if self
            .list_messages(&message.session_id)?
            .iter()
            .any(|existing| existing.state.is_active())
        {
            return Err(SessionError::ActiveMessageExists);
        }
        write_json_atomic(&path, message)?;
        if session.status != SessionStatus::Active
            && !session.status.can_transition_to(SessionStatus::Active)
        {
            return Err(SessionError::InvalidSessionTransition);
        }
        session.status = SessionStatus::Active;
        session.last_active_at = now_rfc3339()?;
        write_json_atomic(
            &self.session_dir(&message.session_id).join("session.json"),
            &session,
        )
    }

    /// Creates a queued Message for a long-lived Session.
    pub fn create_message(
        &self,
        session_id: &str,
        agent_revision: u64,
        input: &str,
        requested_skills: &[String],
        composition: ExecutionComposition,
    ) -> Result<Message, SessionError> {
        validate_value("input", input)?;
        let now = now_rfc3339()?;
        let message = Message {
            id: generate_message_id()?,
            session_id: session_id.to_owned(),
            agent_revision,
            state: MessageState::Queued,
            input: input.to_owned(),
            requested_skills: requested_skills.to_vec(),
            output: None,
            model_usage: TokenUsage::default(),
            tool_results: Vec::new(),
            pending_approval: None,
            pending_external: None,
            approval: None,
            checkpoint: None,
            created_at: now.clone(),
            updated_at: now,
            composition,
        };
        self.put_message(&message)?;
        Ok(message)
    }

    /// Returns one Message.
    pub fn get_message(&self, session_id: &str, message_id: &str) -> Result<Message, SessionError> {
        let path = self.message_path(session_id, message_id);
        if !path.exists() {
            return Err(SessionError::MessageNotFound(message_id.to_owned()));
        }
        Ok(serde_json::from_slice(&fs::read(path)?)?)
    }

    /// Lists Messages in creation order.
    pub fn list_messages(&self, session_id: &str) -> Result<Vec<Message>, SessionError> {
        validate_identifier("session", session_id)?;
        let directory = self.session_dir(session_id).join("messages");
        if !directory.exists() {
            return Err(SessionError::SessionNotFound(session_id.to_owned()));
        }
        let mut messages: Vec<Message> = Vec::new();
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if entry.file_type()?.is_file()
                && entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "json")
            {
                messages.push(serde_json::from_slice(&fs::read(entry.path())?)?);
            }
        }
        messages.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(messages)
    }

    /// Applies one legal Message state transition.
    pub fn transition_message(
        &self,
        session_id: &str,
        message_id: &str,
        next: MessageState,
    ) -> Result<Message, SessionError> {
        let _lock = self.lock()?;
        let mut session = self.get(session_id)?;
        let mut message = self.get_message(session_id, message_id)?;
        if !message.state.can_transition_to(next) {
            return Err(SessionError::InvalidMessageTransition {
                from: message.state,
                to: next,
            });
        }
        let now = now_rfc3339()?;
        message.state = next;
        message.updated_at = now.clone();
        write_json_atomic(&self.message_path(session_id, message_id), &message)?;
        let next_session = if next.is_active() {
            SessionStatus::Active
        } else {
            SessionStatus::Idle
        };
        if session.status != next_session && session.status.can_transition_to(next_session) {
            session.status = next_session;
        }
        session.last_active_at = now;
        write_json_atomic(&self.session_dir(session_id).join("session.json"), &session)?;
        Ok(message)
    }

    /// Closes a Session without active Messages.
    pub fn close(&self, session_id: &str) -> Result<Session, SessionError> {
        let _lock = self.lock()?;
        let mut session = self.get(session_id)?;
        if self
            .list_messages(session_id)?
            .iter()
            .any(|message| message.state.is_active())
        {
            return Err(SessionError::ActiveMessageExists);
        }
        if !session.status.can_transition_to(SessionStatus::Closed) {
            return Err(SessionError::InvalidSessionTransition);
        }
        session.status = SessionStatus::Closed;
        write_json_atomic(&self.session_dir(session_id).join("session.json"), &session)?;
        Ok(session)
    }

    /// Archives a closed Session.
    pub fn archive(&self, session_id: &str) -> Result<Session, SessionError> {
        let _lock = self.lock()?;
        let mut session = self.get(session_id)?;
        if !session.status.can_transition_to(SessionStatus::Archived) {
            return Err(SessionError::InvalidSessionTransition);
        }
        session.status = SessionStatus::Archived;
        write_json_atomic(&self.session_dir(session_id).join("session.json"), &session)?;
        Ok(session)
    }

    fn message_path(&self, session_id: &str, message_id: &str) -> PathBuf {
        self.session_dir(session_id)
            .join("messages")
            .join(format!("{message_id}.json"))
    }

    fn session_dir(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    fn lock(&self) -> Result<File, SessionError> {
        fs::create_dir_all(&self.root)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("sessions.lock"))?;
        file.lock_exclusive()?;
        Ok(file)
    }
}

fn validate_value(label: &str, value: &str) -> Result<(), SessionError> {
    if value.trim().is_empty() {
        return Err(SessionError::InvalidIdentifier(format!("{label} is empty")));
    }
    Ok(())
}

fn validate_identifier(label: &str, value: &str) -> Result<(), SessionError> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(SessionError::InvalidIdentifier(format!("{label}: {value}")));
    }
    Ok(())
}

fn generate_session_id() -> Result<String, SessionError> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| SessionError::Clock)?
        .as_nanos();
    let sequence = SESSION_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    Ok(format!("ses_{nanos:x}_{sequence:x}"))
}

fn generate_message_id() -> Result<String, SessionError> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| SessionError::Clock)?
        .as_nanos();
    let sequence = SESSION_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    Ok(format!("msg_{nanos:x}_{sequence:x}"))
}

fn now_rfc3339() -> Result<String, SessionError> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|_| SessionError::Clock)
}

fn write_json_atomic(path: &Path, value: &impl serde::Serialize) -> Result<(), SessionError> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    let temporary = path.with_extension("tmp");
    let mut file = File::create(temporary)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    fs::rename(path.with_extension("tmp"), path)?;
    Ok(())
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
