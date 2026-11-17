//! Message execution evidence.

use crate::knowledge::KnowledgeCitation;
use crate::provider::ToolEffect;
use serde::{Deserialize, Serialize};

/// Durable Message state.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageState {
    Queued,
    Running,
    WaitingApproval,
    WaitingExternal,
    Completed,
    Failed,
    Cancelled,
}

impl MessageState {
    /// Returns whether the Message has reached a terminal state.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    /// Returns whether the Message currently occupies the Session.
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(
            self,
            Self::Queued | Self::Running | Self::WaitingApproval | Self::WaitingExternal
        )
    }

    /// Validates one Message state transition.
    #[must_use]
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Queued, Self::Running | Self::Failed | Self::Cancelled)
                | (
                    Self::Running,
                    Self::WaitingApproval
                        | Self::WaitingExternal
                        | Self::Completed
                        | Self::Failed
                        | Self::Cancelled
                )
                | (
                    Self::WaitingApproval,
                    Self::Running | Self::Failed | Self::Cancelled
                )
                | (
                    Self::WaitingExternal,
                    Self::Running | Self::Failed | Self::Cancelled
                )
        )
    }
}

/// One execution of an Agent Revision.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Message {
    pub id: String,
    pub session_id: String,
    pub agent_revision: u64,
    pub state: MessageState,
    pub input: String,
    pub requested_skills: Vec<String>,
    pub output: Option<String>,
    #[serde(default)]
    pub model_usage: TokenUsage,
    pub tool_results: Vec<ToolResultEvidence>,
    pub pending_approval: Option<PendingToolCall>,
    #[serde(default)]
    pub pending_external: Option<PendingExternalCall>,
    pub approval: Option<MessageApproval>,
    pub checkpoint: Option<MessageCheckpoint>,
    pub created_at: String,
    pub updated_at: String,
    pub composition: ExecutionComposition,
}

/// Recoverable Message execution state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MessageCheckpoint {
    pub next_step: u32,
    pub tool_results: Vec<ToolResultEvidence>,
    #[serde(default)]
    pub model_usage: TokenUsage,
}

/// Accumulated model Token usage for one Message.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TokenUsage {
    #[serde(default)]
    pub prompt_tokens: u64,
    #[serde(default)]
    pub completion_tokens: u64,
    #[serde(default)]
    pub total_tokens: u64,
}

impl std::ops::AddAssign for TokenUsage {
    fn add_assign(&mut self, rhs: Self) {
        self.prompt_tokens = self.prompt_tokens.saturating_add(rhs.prompt_tokens);
        self.completion_tokens = self.completion_tokens.saturating_add(rhs.completion_tokens);
        self.total_tokens = self.total_tokens.saturating_add(rhs.total_tokens);
    }
}

/// One model-visible Tool result retained for recovery.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolResultEvidence {
    pub tool_id: String,
    pub content: String,
}

/// Exact write intent awaiting human approval.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PendingToolCall {
    pub approval_id: String,
    pub tool_id: String,
    pub arguments_json: String,
    pub idempotency_key: String,
}

/// Exact Tool call whose external result must be queried.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PendingExternalCall {
    pub tool_id: String,
    pub arguments_json: String,
    pub idempotency_key: String,
}

/// Final approval evidence for one Message.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MessageApproval {
    pub approval_id: String,
    pub actor: String,
    pub scopes: Vec<String>,
    pub decision: String,
    pub comment: Option<String>,
    pub reviewed_at: String,
}

/// Actual resolved versions used by one Message.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionComposition {
    pub scope_digest: String,
    pub behavior_digest: String,
    pub model: ResolvedModel,
    pub tools: Vec<ResolvedTool>,
    pub skills: Vec<ResolvedSkill>,
    pub knowledge: Vec<KnowledgeCitation>,
    pub resolved_at: String,
}

/// Model selected at Message start.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedModel {
    pub route: String,
    pub model_id: String,
    pub version: String,
}

/// Tool selected at Message start.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedTool {
    pub id: String,
    pub version: String,
    pub schema_digest: String,
    pub effect: ToolEffect,
}

/// Skill selected at Message start.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedSkill {
    pub id: String,
    pub version: String,
    pub digest: String,
}

#[cfg(test)]
mod tests {
    use super::{Message, MessageState};

    #[test]
    fn parses_core_message_fixture() {
        let message: Message =
            serde_json::from_str(include_str!("../fixtures/core/message.json")).unwrap();
        assert_eq!(message.id, "msg-example-18");
        assert_eq!(message.agent_revision, 18);
        assert_eq!(message.state, MessageState::Running);
        assert_eq!(message.composition.tools.len(), 1);
        assert_eq!(message.composition.skills.len(), 1);
        assert_eq!(message.composition.knowledge.len(), 1);
    }

    #[test]
    fn validates_message_transitions() {
        assert!(MessageState::Queued.can_transition_to(MessageState::Running));
        assert!(MessageState::Running.can_transition_to(MessageState::WaitingApproval));
        assert!(MessageState::WaitingApproval.can_transition_to(MessageState::Running));
        assert!(!MessageState::Completed.can_transition_to(MessageState::Running));
        assert!(!MessageState::Running.can_transition_to(MessageState::Queued));
    }
}
