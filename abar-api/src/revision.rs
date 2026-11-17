//! Agent Revision contracts.

use crate::provider::ToolEffect;
use serde::{Deserialize, Serialize};

/// Frozen Agent Revision wire version.
pub const AGENT_REVISION_API_VERSION: &str = "abar/v1";
/// Frozen Agent Revision kind.
pub const AGENT_REVISION_KIND: &str = "AgentRevision";

/// Immutable capability contract produced by Build.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentRevision {
    pub api_version: String,
    pub kind: String,
    pub metadata: RevisionMetadata,
    pub spec: RevisionSpec,
}

/// Stable Agent identity targeted by a Revision.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RevisionMetadata {
    pub agent: String,
}

/// Immutable Revision payload.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevisionSpec {
    pub revision: u64,
    pub resource_scope: ResourceScope,
    pub behavior: BehaviorSpec,
    pub knowledge_sources: Vec<KnowledgeSourceBinding>,
    pub tool_bindings: Vec<ToolBinding>,
}

/// Resources available to an Agent Revision.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceScope {
    pub tools: ToolScope,
    pub skills: Vec<String>,
    pub knowledge_sources: Vec<String>,
    pub data: DataScope,
    pub resources: ResourceLimits,
}

/// Maximum Tool access split by effect.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolScope {
    pub read: Vec<String>,
    pub write: Vec<String>,
}

/// Allowed and denied data classes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DataScope {
    pub allow: Vec<String>,
    pub deny: Vec<String>,
}

/// Runtime resource limits.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceLimits {
    pub model_class: String,
    pub max_steps: u32,
}

/// Behavior content pinned by a Revision.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BehaviorSpec {
    pub prompt: ContentReference,
    pub skills: Vec<VersionedContentReference>,
    pub rules: Vec<VersionedContentReference>,
}

/// Content reference with digest.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContentReference {
    #[serde(rename = "ref")]
    pub reference: String,
    pub digest: String,
}

/// Versioned behavior resource.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VersionedContentReference {
    pub id: String,
    pub version: String,
    pub digest: String,
}

/// Knowledge Source selected by a Revision.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeSourceBinding {
    pub id: String,
    pub required: bool,
    pub retrieval: KnowledgeRetrieval,
}

/// Knowledge retrieval policy.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeRetrieval {
    pub top_k: u32,
}

/// Tool range and maximum effect.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolBinding {
    pub id: String,
    pub version_range: String,
    pub effect: ToolEffect,
}

/// Scope review decision.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReviewDecision {
    Approved,
    Rejected,
}

/// Auditable scope review bound to one candidate Revision.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScopeReviewEvidence {
    pub agent: String,
    pub from_revision: Option<u64>,
    pub candidate_revision: u64,
    pub reviewer: String,
    pub decision: ReviewDecision,
    pub comment: Option<String>,
    pub reviewed_at: String,
}

#[cfg(test)]
mod tests {
    use super::{AGENT_REVISION_API_VERSION, AGENT_REVISION_KIND, AgentRevision};

    #[test]
    fn parses_core_agent_revision_fixture() {
        let revision: AgentRevision =
            serde_json::from_str(include_str!("../fixtures/core/agent-revision.json")).unwrap();
        assert_eq!(revision.api_version, AGENT_REVISION_API_VERSION);
        assert_eq!(revision.kind, AGENT_REVISION_KIND);
        assert_eq!(revision.metadata.agent, "example-assistant");
        assert_eq!(revision.spec.revision, 18);
        assert_eq!(revision.spec.behavior.skills.len(), 2);
        assert_eq!(revision.spec.knowledge_sources.len(), 2);
        assert_eq!(revision.spec.tool_bindings.len(), 2);
    }

    #[test]
    fn revision_round_trips_without_losing_fields() {
        let revision: AgentRevision =
            serde_json::from_str(include_str!("../fixtures/core/agent-revision.json")).unwrap();
        let encoded = serde_json::to_string(&revision).unwrap();
        let decoded: AgentRevision = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, revision);
    }
}
