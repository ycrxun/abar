//! Structural and scope validation for Agent Revisions.

use super::RevisionError;
use abar_api::provider::ToolEffect;
use abar_api::revision::{
    AGENT_REVISION_API_VERSION, AGENT_REVISION_KIND, AgentRevision, ResourceScope, ToolBinding,
};
use std::collections::BTreeSet;

/// Validates one complete Revision before storage or execution.
pub fn validate_revision(revision: &AgentRevision) -> Result<(), RevisionError> {
    if revision.api_version != AGENT_REVISION_API_VERSION {
        return Err(invalid(format!(
            "apiVersion must be {AGENT_REVISION_API_VERSION}"
        )));
    }
    if revision.kind != AGENT_REVISION_KIND {
        return Err(invalid(format!("kind must be {AGENT_REVISION_KIND}")));
    }
    validate_agent_name(&revision.metadata.agent)?;
    if revision.spec.revision == 0 {
        return Err(invalid("revision must be greater than zero"));
    }
    validate_scope(&revision.spec.resource_scope)?;

    for skill in &revision.spec.behavior.skills {
        if !resource_allowed(&skill.id, &revision.spec.resource_scope.skills) {
            return Err(invalid(format!(
                "Skill {} is outside resourceScope.skills",
                skill.id
            )));
        }
    }
    for source in &revision.spec.knowledge_sources {
        if !resource_allowed(&source.id, &revision.spec.resource_scope.knowledge_sources) {
            return Err(invalid(format!(
                "Knowledge Source {} is outside resourceScope.knowledgeSources",
                source.id
            )));
        }
    }
    for binding in &revision.spec.tool_bindings {
        check_tool_binding(binding)?;
        let scope = match binding.effect {
            ToolEffect::Read => &revision.spec.resource_scope.tools.read,
            ToolEffect::Write => &revision.spec.resource_scope.tools.write,
        };
        if !resource_allowed(&binding.id, scope) {
            return Err(invalid(format!(
                "Tool {} is outside its resourceScope effect",
                binding.id
            )));
        }
    }
    Ok(())
}

pub(super) fn validate_agent_name(agent: &str) -> Result<(), RevisionError> {
    if agent.is_empty()
        || !agent.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
    {
        return Err(RevisionError::InvalidAgent(agent.to_owned()));
    }
    Ok(())
}

fn validate_scope(scope: &ResourceScope) -> Result<(), RevisionError> {
    validate_patterns("tool read", &scope.tools.read)?;
    validate_patterns("tool write", &scope.tools.write)?;
    validate_patterns("Skill", &scope.skills)?;
    validate_patterns("Knowledge Source", &scope.knowledge_sources)?;
    validate_patterns("data allow", &scope.data.allow)?;
    validate_patterns("data deny", &scope.data.deny)?;

    let overlap = scope
        .data
        .allow
        .iter()
        .filter(|value| scope.data.deny.contains(value))
        .cloned()
        .collect::<Vec<_>>();
    if !overlap.is_empty() {
        return Err(invalid(format!(
            "data classes cannot be both allowed and denied: {}",
            overlap.join(", ")
        )));
    }
    if scope.resources.model_class.is_empty() || scope.resources.max_steps == 0 {
        return Err(invalid("runtime resource limits must be positive"));
    }
    Ok(())
}

fn validate_patterns(label: &str, values: &[String]) -> Result<(), RevisionError> {
    let mut unique = BTreeSet::new();
    for value in values {
        if value.is_empty() {
            return Err(invalid(format!("{label} contains an empty value")));
        }
        if let Some(prefix) = value.strip_suffix('*') {
            if prefix.is_empty() || prefix.contains('*') {
                return Err(invalid(format!("{label} has an invalid wildcard: {value}")));
            }
        } else if value.contains('*') {
            return Err(invalid(format!("{label} has an invalid wildcard: {value}")));
        }
        if !unique.insert(value) {
            return Err(invalid(format!(
                "{label} contains duplicate value: {value}"
            )));
        }
    }
    Ok(())
}

pub(super) fn resource_allowed(id: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|pattern| pattern_matches(pattern, id))
}

pub(super) fn pattern_matches(pattern: &str, id: &str) -> bool {
    pattern
        .strip_suffix('*')
        .map_or(pattern == id, |prefix| id.starts_with(prefix))
}

pub(super) fn pattern_covers(broad: &str, narrow: &str) -> bool {
    match (broad.strip_suffix('*'), narrow.strip_suffix('*')) {
        (Some(broad_prefix), Some(narrow_prefix)) => narrow_prefix.starts_with(broad_prefix),
        (Some(broad_prefix), None) => narrow.starts_with(broad_prefix),
        (None, None) => broad == narrow,
        (None, Some(_)) => false,
    }
}

pub(super) fn check_tool_binding(binding: &ToolBinding) -> Result<(), RevisionError> {
    if binding.id.is_empty() || binding.version_range.is_empty() {
        return Err(invalid(
            "Tool binding id and versionRange must not be empty",
        ));
    }
    Ok(())
}

fn invalid(message: impl Into<String>) -> RevisionError {
    RevisionError::InvalidRevision(message.into())
}

#[cfg(test)]
mod tests {
    use super::validate_revision;
    use abar_api::revision::AgentRevision;

    fn fixture() -> AgentRevision {
        serde_json::from_str(include_str!(
            "../../../abar-api/fixtures/core/agent-revision.json"
        ))
        .unwrap()
    }

    #[test]
    fn accepts_core_revision_fixture() {
        validate_revision(&fixture()).unwrap();
    }

    #[test]
    fn rejects_skill_outside_resource_scope() {
        let mut revision = fixture();
        revision.spec.resource_scope.skills.clear();
        assert!(validate_revision(&revision).is_err());
    }

    #[test]
    fn rejects_tool_effect_outside_resource_scope() {
        let mut revision = fixture();
        revision.spec.resource_scope.tools.write.clear();
        assert!(validate_revision(&revision).is_err());
    }

    #[test]
    fn rejects_data_class_both_allowed_and_denied() {
        let mut revision = fixture();
        revision
            .spec
            .resource_scope
            .data
            .allow
            .push("restricted-a".to_owned());
        assert!(validate_revision(&revision).is_err());
    }
}
