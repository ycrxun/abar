//! Message approval and terminal-state persistence.

use super::{SessionError, SessionStatus, SessionStore, now_rfc3339, write_json_atomic};
use abar_api::message::{
    Message, MessageApproval, MessageCheckpoint, MessageState, PendingExternalCall,
    PendingToolCall, TokenUsage, ToolResultEvidence,
};

impl SessionStore {
    /// Persists an awaiting write approval and moves the Message to
    /// `waiting_approval`.
    pub fn request_approval(
        &self,
        session_id: &str,
        message_id: &str,
        pending: PendingToolCall,
        checkpoint: MessageCheckpoint,
    ) -> Result<Message, SessionError> {
        let _lock = self.lock()?;
        let mut message = self.get_message(session_id, message_id)?;
        if message.state != MessageState::Running
            || !message
                .state
                .can_transition_to(MessageState::WaitingApproval)
        {
            return Err(SessionError::InvalidMessageTransition {
                from: message.state,
                to: MessageState::WaitingApproval,
            });
        }
        message.model_usage = checkpoint.model_usage;
        message.pending_approval = Some(pending);
        message.pending_external = None;
        message.checkpoint = Some(checkpoint);
        message.state = MessageState::WaitingApproval;
        message.updated_at = now_rfc3339()?;
        write_json_atomic(&self.message_path(session_id, message_id), &message)?;
        self.upsert_approval_entry(&message)?;
        Ok(message)
    }

    /// Marks a Message complete with its final output.
    pub fn complete_message(
        &self,
        session_id: &str,
        message_id: &str,
        output: String,
        tool_results: Vec<ToolResultEvidence>,
        model_usage: TokenUsage,
    ) -> Result<Message, SessionError> {
        let _lock = self.lock()?;
        let mut message = self.get_message(session_id, message_id)?;
        let approval_id = message
            .pending_approval
            .as_ref()
            .map(|pending| pending.approval_id.clone());
        if !message.state.can_transition_to(MessageState::Completed) {
            return Err(SessionError::InvalidMessageTransition {
                from: message.state,
                to: MessageState::Completed,
            });
        }
        message.state = MessageState::Completed;
        message.output = Some(output);
        message.model_usage = model_usage;
        message.tool_results = tool_results;
        message.pending_approval = None;
        message.pending_external = None;
        message.checkpoint = None;
        message.updated_at = now_rfc3339()?;
        write_json_atomic(&self.message_path(session_id, message_id), &message)?;
        if let Some(approval_id) = approval_id {
            self.remove_approval_entry(&approval_id)?;
        }
        self.update_session_after_terminal(session_id, &message)?;
        Ok(message)
    }

    /// Marks a Message failed.
    pub fn fail_message(
        &self,
        session_id: &str,
        message_id: &str,
        output: String,
    ) -> Result<Message, SessionError> {
        let _lock = self.lock()?;
        let mut message = self.get_message(session_id, message_id)?;
        let approval_id = message
            .pending_approval
            .as_ref()
            .map(|pending| pending.approval_id.clone());
        if !message.state.can_transition_to(MessageState::Failed) {
            return Err(SessionError::InvalidMessageTransition {
                from: message.state,
                to: MessageState::Failed,
            });
        }
        message.state = MessageState::Failed;
        message.output = Some(output);
        message.pending_approval = None;
        message.pending_external = None;
        message.checkpoint = None;
        message.updated_at = now_rfc3339()?;
        write_json_atomic(&self.message_path(session_id, message_id), &message)?;
        if let Some(approval_id) = approval_id {
            self.remove_approval_entry(&approval_id)?;
        }
        self.update_session_after_terminal(session_id, &message)?;
        Ok(message)
    }

    /// Persists a Message whose external Tool result is still unknown.
    pub fn wait_for_external(
        &self,
        session_id: &str,
        message_id: &str,
        reason: String,
        pending: PendingExternalCall,
        checkpoint: MessageCheckpoint,
    ) -> Result<Message, SessionError> {
        let _lock = self.lock()?;
        let mut message = self.get_message(session_id, message_id)?;
        if !message
            .state
            .can_transition_to(MessageState::WaitingExternal)
        {
            return Err(SessionError::InvalidMessageTransition {
                from: message.state,
                to: MessageState::WaitingExternal,
            });
        }
        message.model_usage = checkpoint.model_usage;
        message.state = MessageState::WaitingExternal;
        message.output = Some(reason);
        message.pending_external = Some(pending);
        message.checkpoint = Some(checkpoint);
        message.updated_at = now_rfc3339()?;
        write_json_atomic(&self.message_path(session_id, message_id), &message)?;
        Ok(message)
    }

    /// Cancels a non-terminal Message.
    pub fn cancel_message(
        &self,
        session_id: &str,
        message_id: &str,
        reason: String,
        tool_results: Vec<ToolResultEvidence>,
        model_usage: TokenUsage,
    ) -> Result<Message, SessionError> {
        let _lock = self.lock()?;
        let mut message = self.get_message(session_id, message_id)?;
        let approval_id = message
            .pending_approval
            .as_ref()
            .map(|pending| pending.approval_id.clone());
        if !message.state.can_transition_to(MessageState::Cancelled) {
            return Err(SessionError::InvalidMessageTransition {
                from: message.state,
                to: MessageState::Cancelled,
            });
        }
        message.state = MessageState::Cancelled;
        message.output = Some(reason);
        message.model_usage = model_usage;
        message.tool_results = tool_results;
        message.pending_approval = None;
        message.pending_external = None;
        message.checkpoint = None;
        message.updated_at = now_rfc3339()?;
        write_json_atomic(&self.message_path(session_id, message_id), &message)?;
        if let Some(approval_id) = approval_id {
            self.remove_approval_entry(&approval_id)?;
        }
        self.update_session_after_terminal(session_id, &message)?;
        Ok(message)
    }

    /// Records approval evidence and resumes or rejects the pending Message.
    pub fn submit_approval(
        &self,
        session_id: &str,
        message_id: &str,
        approval: MessageApproval,
    ) -> Result<Message, SessionError> {
        let _lock = self.lock()?;
        let mut message = self.get_message(session_id, message_id)?;
        if message.state != MessageState::WaitingApproval || message.pending_approval.is_none() {
            return Err(SessionError::InvalidMessageTransition {
                from: message.state,
                to: if approval.decision == "approved" {
                    MessageState::Running
                } else {
                    MessageState::Cancelled
                },
            });
        }
        let pending =
            message
                .pending_approval
                .as_ref()
                .ok_or(SessionError::InvalidMessageTransition {
                    from: message.state,
                    to: message.state,
                })?;
        if pending.approval_id != approval.approval_id {
            return Err(SessionError::InvalidIdentifier(
                "approvalId does not match pending approval".to_owned(),
            ));
        }
        let pending_approval_id = pending.approval_id.clone();
        let next = if approval.decision == "approved" {
            MessageState::Running
        } else if approval.decision == "rejected" {
            MessageState::Cancelled
        } else {
            return Err(SessionError::InvalidMessageTransition {
                from: message.state,
                to: message.state,
            });
        };
        if approval.actor.trim().is_empty() || approval.approval_id.trim().is_empty() {
            return Err(SessionError::InvalidIdentifier(
                "approval evidence is incomplete".to_owned(),
            ));
        }
        message.approval = Some(approval);
        message.state = next;
        if next == MessageState::Cancelled {
            message.pending_approval = None;
        }
        message.updated_at = now_rfc3339()?;
        write_json_atomic(&self.message_path(session_id, message_id), &message)?;
        self.remove_approval_entry(&pending_approval_id)?;
        let mut session = self.get(session_id)?;
        session.status = if next == MessageState::Cancelled {
            SessionStatus::Idle
        } else {
            SessionStatus::Active
        };
        session.last_active_at = message.updated_at.clone();
        write_json_atomic(&self.session_dir(session_id).join("session.json"), &session)?;
        Ok(message)
    }

    fn update_session_after_terminal(
        &self,
        session_id: &str,
        message: &Message,
    ) -> Result<(), SessionError> {
        let mut session = self.get(session_id)?;
        session.status = SessionStatus::Idle;
        session.last_active_at = message.updated_at.clone();
        write_json_atomic(&self.session_dir(session_id).join("session.json"), &session)
    }
}
