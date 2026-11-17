//! Conservative resource-scope comparison used by publication review.

use super::validation::{pattern_covers, resource_allowed};
use abar_api::revision::{ResourceScope, ToolBinding};
use serde::{Deserialize, Serialize};

/// Difference between two resource scopes.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeDiff {
    pub review_required: bool,
    pub expanded: Vec<String>,
    pub narrowed: Vec<String>,
}

/// Compares two scopes and requires review for every new or expanded capability.
#[must_use]
pub fn compare_resource_scopes(previous: &ResourceScope, candidate: &ResourceScope) -> ScopeDiff {
    let mut diff = ScopeDiff::default();
    compare_patterns(
        "tool.read",
        &previous.tools.read,
        &candidate.tools.read,
        &mut diff,
    );
    compare_patterns(
        "tool.write",
        &previous.tools.write,
        &candidate.tools.write,
        &mut diff,
    );
    compare_patterns("skills", &previous.skills, &candidate.skills, &mut diff);
    compare_patterns(
        "knowledgeSources",
        &previous.knowledge_sources,
        &candidate.knowledge_sources,
        &mut diff,
    );
    compare_patterns(
        "data.allow",
        &previous.data.allow,
        &candidate.data.allow,
        &mut diff,
    );
    compare_deny(&previous.data.deny, &candidate.data.deny, &mut diff);
    compare_resources(previous, candidate, &mut diff);
    diff.review_required = !diff.expanded.is_empty();
    diff.expanded.sort();
    diff.expanded.dedup();
    diff.narrowed.sort();
    diff.narrowed.dedup();
    diff
}

fn compare_patterns(label: &str, previous: &[String], candidate: &[String], diff: &mut ScopeDiff) {
    for old in previous {
        if !candidate.iter().any(|new| pattern_covers(new, old)) {
            diff.narrowed.push(format!("{label}:{old}"));
        }
    }
    for new in candidate {
        if !previous.iter().any(|old| pattern_covers(old, new)) {
            diff.expanded.push(format!("{label}:{new}"));
        }
    }
}

fn compare_deny(previous: &[String], candidate: &[String], diff: &mut ScopeDiff) {
    for class in candidate {
        if !previous.contains(class) {
            diff.narrowed.push(format!("data.deny:{class}"));
        }
    }
    for class in previous {
        if !candidate.contains(class) {
            diff.expanded.push(format!("data.deny:{class}"));
        }
    }
}

fn compare_resources(previous: &ResourceScope, candidate: &ResourceScope, diff: &mut ScopeDiff) {
    if previous.resources.model_class != candidate.resources.model_class {
        diff.expanded.push("resources.modelClass".to_owned());
    }
    compare_limit(
        "resources.maxSteps",
        previous.resources.max_steps,
        candidate.resources.max_steps,
        diff,
    );
}

fn compare_limit(label: &str, previous: u32, candidate: u32, diff: &mut ScopeDiff) {
    if candidate > previous {
        diff.expanded.push(label.to_owned());
    } else if candidate < previous {
        diff.narrowed.push(label.to_owned());
    }
}

/// Returns bindings whose effects are no longer present in the candidate scope.
#[must_use]
pub fn removed_tool_bindings(
    previous: &[ToolBinding],
    candidate_scope: &ResourceScope,
) -> Vec<ToolBinding> {
    previous
        .iter()
        .filter(|tool| {
            let scope = match tool.effect {
                abar_api::provider::ToolEffect::Read => &candidate_scope.tools.read,
                abar_api::provider::ToolEffect::Write => &candidate_scope.tools.write,
            };
            !resource_allowed(&tool.id, scope)
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{ScopeDiff, compare_resource_scopes, removed_tool_bindings};
    use abar_api::provider::ToolEffect;
    use abar_api::revision::{AgentRevision, ResourceScope, ToolBinding};

    fn fixture() -> AgentRevision {
        serde_json::from_str(include_str!(
            "../../../abar-api/fixtures/core/agent-revision.json"
        ))
        .unwrap()
    }

    fn assert_review(diff: &ScopeDiff) {
        assert!(diff.review_required, "{diff:?}");
    }

    #[test]
    fn identical_scopes_need_no_review() {
        let revision = fixture();
        let diff =
            compare_resource_scopes(&revision.spec.resource_scope, &revision.spec.resource_scope);
        assert_eq!(diff, ScopeDiff::default());
    }

    #[test]
    fn adding_write_tool_requires_review() {
        let revision = fixture();
        let mut candidate = revision.spec.resource_scope.clone();
        candidate.tools.write.push("records.cancel".to_owned());
        let diff = compare_resource_scopes(&revision.spec.resource_scope, &candidate);
        assert_review(&diff);
        assert!(
            diff.expanded
                .contains(&"tool.write:records.cancel".to_owned())
        );
    }

    #[test]
    fn broadening_tool_namespace_requires_review() {
        let revision = fixture();
        let mut candidate = revision.spec.resource_scope.clone();
        candidate.tools.read = vec!["catalog.*".to_owned()];
        let diff = compare_resource_scopes(&revision.spec.resource_scope, &candidate);
        assert_review(&diff);
    }

    #[test]
    fn narrowing_scope_does_not_require_review() {
        let revision = fixture();
        let mut candidate = revision.spec.resource_scope.clone();
        candidate
            .tools
            .read
            .retain(|tool| tool == "catalog.query-*");
        candidate
            .skills
            .retain(|skill| skill == "decision-guidance");
        let diff = compare_resource_scopes(&revision.spec.resource_scope, &candidate);
        assert!(!diff.review_required);
        assert!(!diff.narrowed.is_empty());
    }

    #[test]
    fn data_allow_expansion_and_deny_removal_require_review() {
        let revision = fixture();
        let mut candidate = revision.spec.resource_scope.clone();
        candidate.data.allow.push("customer-profile".to_owned());
        candidate.data.deny.retain(|class| class != "restricted-c");
        let diff = compare_resource_scopes(&revision.spec.resource_scope, &candidate);
        assert_review(&diff);
    }

    #[test]
    fn reports_bindings_removed_from_candidate_scope() {
        let revision = fixture();
        let mut scope: ResourceScope = revision.spec.resource_scope.clone();
        scope.tools.write.clear();
        let removed = removed_tool_bindings(
            &[ToolBinding {
                id: "records.apply".to_owned(),
                version_range: ">=6.2 <7.0".to_owned(),
                effect: ToolEffect::Write,
            }],
            &scope,
        );
        assert_eq!(removed.len(), 1);
    }
}
