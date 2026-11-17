//! Immutable resolved provider inputs for one Message.

use crate::digest::sha256_hex;
use abar_api::content::BehaviorContent;
use abar_api::knowledge::{KnowledgeCitation, KnowledgeDocument};
use abar_api::message::{ExecutionComposition, ResolvedModel, ResolvedSkill, ResolvedTool};
use abar_api::provider::{ToolDescriptor, ToolEffect};
use abar_api::revision::AgentRevision;
use thiserror::Error;

/// Provider Bundle resolution failure.
#[derive(Debug, Error)]
pub enum ProviderBundleError {
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Skill is missing from resolved behavior: {0}")]
    SkillMismatch(String),
    #[error("Knowledge Source is outside Revision scope: {0}")]
    KnowledgeDenied(String),
    #[error("Tool is outside Revision binding: {0}")]
    ToolDenied(String),
    #[error("Tool effect mismatch: {0}")]
    ToolEffectMismatch(String),
}

/// Inputs resolved from providers immediately before Message execution.
#[derive(Debug)]
pub struct ProviderBundleInput<'a> {
    pub revision: &'a AgentRevision,
    pub behavior: &'a BehaviorContent,
    pub tools: &'a [ToolDescriptor],
    pub knowledge: &'a [KnowledgeDocument],
    pub model_id: &'a str,
    pub model_version: &'a str,
    pub resolved_at: &'a str,
}

/// Immutable bundle passed to Context Assembly and Message Executor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderBundle {
    pub composition: ExecutionComposition,
    pub system_prompt: String,
    pub skills: Vec<abar_api::provider::SkillContent>,
    pub knowledge: Vec<KnowledgeDocument>,
    pub tools: Vec<ToolDescriptor>,
    pub model_id: String,
}

/// Validates provider results and constructs immutable Message evidence.
pub fn build_provider_bundle(
    input: ProviderBundleInput<'_>,
) -> Result<ProviderBundle, ProviderBundleError> {
    let model_route = &input.revision.spec.resource_scope.resources.model_class;
    let mut tools = Vec::new();
    let mut resolved_tool_descriptors = Vec::new();
    for descriptor in input.tools {
        let Some(binding) = input.revision.spec.tool_bindings.iter().find(|binding| {
            allowed(&binding.id, &descriptor.id) && binding.effect == descriptor.effect
        }) else {
            continue;
        };
        if binding.effect != descriptor.effect {
            return Err(ProviderBundleError::ToolEffectMismatch(
                descriptor.id.clone(),
            ));
        }
        tools.push(ResolvedTool {
            id: descriptor.id.clone(),
            version: descriptor.version.clone(),
            schema_digest: descriptor.schema_digest.clone(),
            effect: match descriptor.effect {
                ToolEffect::Read => ToolEffect::Read,
                ToolEffect::Write => ToolEffect::Write,
            },
        });
        resolved_tool_descriptors.push(descriptor.clone());
    }
    for binding in &input.revision.spec.tool_bindings {
        if !input
            .tools
            .iter()
            .any(|tool| allowed(&binding.id, &tool.id) && tool.effect == binding.effect)
        {
            return Err(ProviderBundleError::ToolDenied(binding.id.clone()));
        }
    }
    let mut skills = Vec::new();
    for required in &input.revision.spec.behavior.skills {
        input
            .behavior
            .skills
            .iter()
            .find(|skill| {
                skill.metadata.name == required.id
                    && skill.metadata.version.as_deref() == Some(required.version.as_str())
            })
            .ok_or_else(|| ProviderBundleError::SkillMismatch(required.id.clone()))?;
        skills.push(ResolvedSkill {
            id: required.id.clone(),
            version: required.version.clone(),
            digest: required.digest.clone(),
        });
    }
    for document in input.knowledge {
        if !input
            .revision
            .spec
            .resource_scope
            .knowledge_sources
            .iter()
            .any(|source| source == &document.source)
        {
            return Err(ProviderBundleError::KnowledgeDenied(
                document.source.clone(),
            ));
        }
    }
    for source in &input.revision.spec.knowledge_sources {
        if source.required && !input.knowledge.iter().any(|doc| doc.source == source.id) {
            return Err(ProviderBundleError::KnowledgeDenied(source.id.clone()));
        }
    }
    let composition = ExecutionComposition {
        scope_digest: digest(&input.revision.spec.resource_scope)?,
        behavior_digest: digest(&input.revision.spec.behavior)?,
        model: ResolvedModel {
            route: model_route.clone(),
            model_id: input.model_id.to_owned(),
            version: input.model_version.to_owned(),
        },
        tools,
        skills,
        knowledge: input
            .knowledge
            .iter()
            .map(KnowledgeCitation::from)
            .collect(),
        resolved_at: input.resolved_at.to_owned(),
    };
    Ok(ProviderBundle {
        composition,
        system_prompt: input.behavior.system_prompt.clone(),
        skills: input.behavior.skills.clone(),
        knowledge: input.knowledge.to_vec(),
        tools: resolved_tool_descriptors,
        model_id: input.model_id.to_owned(),
    })
}

fn digest(value: &impl serde::Serialize) -> Result<String, ProviderBundleError> {
    Ok(sha256_hex(&serde_json::to_vec(value)?))
}

fn allowed(id: &str, pattern: &str) -> bool {
    pattern
        .strip_suffix('*')
        .map_or(pattern == id, |prefix| id.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::{ProviderBundleError, ProviderBundleInput, build_provider_bundle};
    use abar_api::content::BehaviorContent;
    use abar_api::knowledge::KnowledgeDocument;
    use abar_api::provider::{SkillContent, SkillMetadata, ToolDescriptor, ToolEffect};
    use abar_api::revision::AgentRevision;

    fn revision() -> AgentRevision {
        serde_json::from_str(include_str!(
            "../../abar-api/fixtures/core/agent-revision.json"
        ))
        .unwrap()
    }

    fn behavior() -> BehaviorContent {
        BehaviorContent {
            system_prompt: "system".to_owned(),
            skills: ["decision-guidance", "approval-guidance"]
                .into_iter()
                .enumerate()
                .map(|(index, name)| SkillContent {
                    metadata: SkillMetadata {
                        name: name.to_owned(),
                        description: name.to_owned(),
                        disable_model_invocation: false,
                        version: Some(if index == 0 { "11" } else { "7" }.to_owned()),
                    },
                    markdown: name.to_owned(),
                })
                .collect(),
        }
    }

    fn tools() -> Vec<ToolDescriptor> {
        [
            ("catalog.query-example", ToolEffect::Read),
            ("records.apply", ToolEffect::Write),
            ("other.business-tool", ToolEffect::Read),
        ]
        .into_iter()
        .map(|(id, effect)| ToolDescriptor {
            id: id.to_owned(),
            version: "1.0.0".to_owned(),
            description: id.to_owned(),
            input_schema_json: "{}".to_owned(),
            output_schema_json: "{}".to_owned(),
            effect,
            schema_digest: format!("sha256:{}", "1".repeat(64)),
        })
        .collect()
    }

    #[test]
    fn builds_composition_from_resolved_providers() {
        let revision = revision();
        let behavior = behavior();
        let tools = tools();
        let knowledge = [KnowledgeDocument {
            source: "policy".to_owned(),
            document_id: "doc-12".to_owned(),
            version: "2026-11-17".to_owned(),
            digest: format!("sha256:{}", "2".repeat(64)),
            content: "policy".to_owned(),
            retrieved_at: "2026-11-17T09:00:00Z".to_owned(),
        }];
        let bundle = build_provider_bundle(ProviderBundleInput {
            revision: &revision,
            behavior: &behavior,
            tools: &tools,
            knowledge: &knowledge,
            model_id: "local/qwen",
            model_version: "2026-11-01",
            resolved_at: "2026-11-17T09:00:01Z",
        })
        .unwrap();
        assert_eq!(bundle.composition.tools.len(), 2);
        assert_eq!(bundle.tools.len(), 2);
        assert_eq!(bundle.composition.skills.len(), 2);
        assert_eq!(bundle.composition.knowledge.len(), 1);
        assert_eq!(bundle.composition.model.route, "general-tool-use");
    }

    #[test]
    fn fails_closed_when_required_knowledge_is_missing() {
        let revision = revision();
        let behavior = behavior();
        let tools = tools();
        let result = build_provider_bundle(ProviderBundleInput {
            revision: &revision,
            behavior: &behavior,
            tools: &tools,
            knowledge: &[],
            model_id: "local/qwen",
            model_version: "2026-11-01",
            resolved_at: "2026-11-17T09:00:01Z",
        });
        assert!(matches!(
            result,
            Err(ProviderBundleError::KnowledgeDenied(_))
        ));
    }
}
