//! Message Runtime event projection.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One committed Message event.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeEvent {
    pub event_id: String,
    pub session_id: String,
    pub message_id: String,
    pub sequence: u64,
    pub event_type: String,
    pub at: String,
    pub schema_version: String,
    pub data: Value,
}
