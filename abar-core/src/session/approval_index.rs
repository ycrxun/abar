use super::{SessionError, SessionStore, now_rfc3339, write_json_atomic};
use abar_api::message::{Message, MessageState};
use serde::{Deserialize, Serialize};
use std::fs;

const INDEX_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ApprovalIndexEntry {
    pub(crate) tenant: String,
    pub(crate) approval_id: String,
    pub(crate) session_id: String,
    pub(crate) message_id: String,
    pub(crate) agent: String,
    pub(crate) agent_revision: u64,
    pub(crate) requester: String,
    pub(crate) tool_id: String,
    pub(crate) arguments_json: String,
    pub(crate) pending_since: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApprovalIndex {
    version: u32,
    approvals: Vec<ApprovalIndexEntry>,
}

impl SessionStore {
    /// Rebuilds the approval read model from Message source records.
    pub fn rebuild_approval_index(&self) -> Result<usize, SessionError> {
        let _lock = self.lock()?;
        let mut approvals = Vec::new();
        for session in self.list_all()? {
            for message in self.list_messages(&session.id)? {
                if message.state == MessageState::WaitingApproval {
                    approvals.push(entry(&session, &message)?);
                }
            }
        }
        approvals.sort_by(|left, right| right.pending_since.cmp(&left.pending_since));
        let count = approvals.len();
        self.write_approval_index(approvals)?;
        Ok(count)
    }

    /// Lists indexed approval tasks for one Tenant and optional Agent.
    pub(crate) fn list_approval_entries(
        &self,
        tenant: &str,
        agent: Option<&str>,
    ) -> Result<Vec<ApprovalIndexEntry>, SessionError> {
        let path = self.approval_index_path();
        if !path.exists() {
            self.rebuild_approval_index()?;
        }
        let bytes = fs::read(path)?;
        let index: ApprovalIndex = serde_json::from_slice(&bytes)?;
        let mut approvals = index
            .approvals
            .into_iter()
            .filter(|entry| {
                entry.tenant == tenant && agent.is_none_or(|agent| entry.agent == agent)
            })
            .collect::<Vec<_>>();
        approvals.sort_by(|left, right| right.pending_since.cmp(&left.pending_since));
        Ok(approvals)
    }

    pub(super) fn upsert_approval_entry(&self, message: &Message) -> Result<(), SessionError> {
        let session = self.get(&message.session_id)?;
        let mut approvals = match self.read_approval_index() {
            Ok(index) => index.approvals,
            Err(_) if !self.approval_index_path().exists() => Vec::new(),
            Err(error) => return Err(error),
        };
        let new_entry = entry(&session, message)?;
        approvals.retain(|entry| entry.approval_id != new_entry.approval_id);
        approvals.push(new_entry);
        self.write_approval_index(approvals)
    }

    pub(super) fn remove_approval_entry(&self, approval_id: &str) -> Result<(), SessionError> {
        let path = self.approval_index_path();
        if !path.exists() {
            return Ok(());
        }
        let mut index = self.read_approval_index()?;
        index
            .approvals
            .retain(|entry| entry.approval_id != approval_id);
        self.write_approval_index(index.approvals)
    }

    fn read_approval_index(&self) -> Result<ApprovalIndex, SessionError> {
        let bytes = fs::read(self.approval_index_path())?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    fn write_approval_index(&self, approvals: Vec<ApprovalIndexEntry>) -> Result<(), SessionError> {
        fs::create_dir_all(&self.root)?;
        write_json_atomic(
            &self.approval_index_path(),
            &ApprovalIndex {
                version: INDEX_VERSION,
                approvals,
            },
        )
    }

    fn approval_index_path(&self) -> std::path::PathBuf {
        self.root.join("approval-index.json")
    }
}

fn entry(
    session: &abar_api::session::Session,
    message: &Message,
) -> Result<ApprovalIndexEntry, SessionError> {
    let pending = message
        .pending_approval
        .as_ref()
        .ok_or_else(|| SessionError::InvalidIdentifier("pending approval is missing".to_owned()))?;
    let pending_since = if message.updated_at.is_empty() {
        now_rfc3339()?
    } else {
        message.updated_at.clone()
    };
    Ok(ApprovalIndexEntry {
        tenant: session.tenant.clone(),
        approval_id: pending.approval_id.clone(),
        session_id: session.id.clone(),
        message_id: message.id.clone(),
        agent: session.agent.clone(),
        agent_revision: message.agent_revision,
        requester: session.user.clone(),
        tool_id: pending.tool_id.clone(),
        arguments_json: pending.arguments_json.clone(),
        pending_since,
    })
}
