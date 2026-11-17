//! Minimal approval inbox projections.

use serde::{Deserialize, Serialize};

/// One pending write approval visible to an authorized reviewer.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApprovalTask {
    pub approval_id: String,
    pub session_id: String,
    pub message_id: String,
    pub agent: String,
    pub agent_revision: u64,
    pub requester: String,
    pub tool_id: String,
    pub arguments_json: String,
    pub pending_since: String,
}
