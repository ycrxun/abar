//! Builds the frozen Agent Revision contract from a declarative project.

use super::{RevisionError, validate_revision};
use crate::digest::{content_digest, sha256_hex, validate_relative_path};
use abar_api::provider::ToolEffect;
use abar_api::revision::{
    AGENT_REVISION_API_VERSION, AGENT_REVISION_KIND, AgentRevision, BehaviorSpec, ContentReference,
    DataScope, KnowledgeSourceBinding, ResourceLimits, ResourceScope, RevisionMetadata,
    RevisionSpec, ToolBinding, ToolScope, VersionedContentReference,
};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Revision project build failure.
#[derive(Debug, Error)]
pub enum RevisionBuildError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("Revision error: {0}")]
    Revision(#[from] RevisionError),
    #[error("invalid Revision project: {0}")]
    Invalid(String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentDocument {
    #[serde(rename = "apiVersion")]
    api_version: String,
    kind: String,
    metadata: AgentMetadata,
    spec: AgentSpec,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentMetadata {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentSpec {
    model_class: String,
    max_steps: u32,
    scope: String,
    behavior: String,
    knowledge: String,
    tools: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScopeDocument {
    tools: ToolScope,
    skills: Vec<String>,
    #[serde(rename = "knowledgeSources")]
    knowledge_sources: Vec<String>,
    data: DataScope,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BehaviorDocument {
    prompt: String,
    #[serde(default)]
    skills: Vec<BehaviorResource>,
    #[serde(default)]
    rules: Vec<BehaviorResource>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BehaviorResource {
    id: String,
    version: String,
    path: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KnowledgeDocument {
    #[serde(default)]
    sources: Vec<KnowledgeSourceBinding>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolDocument {
    tools: Vec<ToolBinding>,
}

/// Builds one Revision from a declarative source directory.
pub fn build_agent_revision(
    source: &Path,
    revision: u64,
) -> Result<AgentRevision, RevisionBuildError> {
    let source = source.canonicalize()?;
    let agent = read_agent(&source)?;
    let scope: ScopeDocument = read_yaml(&resolved(&source, &agent.spec.scope)?)?;
    let behavior: BehaviorDocument = read_yaml(&resolved(&source, &agent.spec.behavior)?)?;
    let knowledge: KnowledgeDocument = read_yaml(&resolved(&source, &agent.spec.knowledge)?)?;
    let tools: ToolDocument = read_yaml(&resolved(&source, &agent.spec.tools)?)?;

    let resource_scope = ResourceScope {
        tools: scope.tools,
        skills: scope.skills,
        knowledge_sources: scope.knowledge_sources,
        data: scope.data,
        resources: ResourceLimits {
            model_class: agent.spec.model_class,
            max_steps: agent.spec.max_steps,
        },
    };

    let behavior = BehaviorSpec {
        prompt: build_content_reference(&source, &behavior.prompt)?,
        skills: build_versioned_references(&source, behavior.skills, ResourceKind::Directory)?,
        rules: build_versioned_references(&source, behavior.rules, ResourceKind::File)?,
    };
    validate_unique_sources(&knowledge.sources)?;
    validate_unique_tools(&tools.tools)?;

    let revision = AgentRevision {
        api_version: AGENT_REVISION_API_VERSION.to_owned(),
        kind: AGENT_REVISION_KIND.to_owned(),
        metadata: RevisionMetadata {
            agent: agent.metadata.name,
        },
        spec: RevisionSpec {
            revision,
            resource_scope,
            behavior,
            knowledge_sources: knowledge.sources,
            tool_bindings: tools.tools,
        },
    };
    validate_revision(&revision)?;
    Ok(revision)
}

/// Reads the stable Agent name from a Revision project.
pub fn project_agent_name(source: &Path) -> Result<String, RevisionBuildError> {
    Ok(read_agent(&source.canonicalize()?)?.metadata.name)
}

fn read_agent(source: &Path) -> Result<AgentDocument, RevisionBuildError> {
    let agent: AgentDocument = read_yaml(&source.join("agent.yaml"))?;
    if agent.api_version != AGENT_REVISION_API_VERSION || agent.kind != "Agent" {
        return Err(RevisionBuildError::Invalid(
            "agent.yaml must use apiVersion abar/v1 and kind Agent".to_owned(),
        ));
    }
    Ok(agent)
}

fn build_content_reference(
    source: &Path,
    reference: &str,
) -> Result<ContentReference, RevisionBuildError> {
    let path = resolved(source, reference)?;
    let bytes = fs::read(path)?;
    Ok(ContentReference {
        reference: reference.to_owned(),
        digest: sha256_hex(&bytes),
    })
}

enum ResourceKind {
    Directory,
    File,
}

fn build_versioned_references(
    source: &Path,
    resources: Vec<BehaviorResource>,
    kind: ResourceKind,
) -> Result<Vec<VersionedContentReference>, RevisionBuildError> {
    let mut seen = BTreeSet::new();
    let mut built = Vec::new();
    for resource in resources {
        if !seen.insert(resource.id.clone()) {
            return Err(RevisionBuildError::Invalid(format!(
                "duplicate behavior resource: {}",
                resource.id
            )));
        }
        let path = resolved(source, &resource.path)?;
        let digest = match kind {
            ResourceKind::Directory => digest_directory(&path)?,
            ResourceKind::File => sha256_hex(&fs::read(path)?),
        };
        built.push(VersionedContentReference {
            id: resource.id,
            version: resource.version,
            digest,
        });
    }
    built.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(built)
}

fn digest_directory(root: &Path) -> Result<String, RevisionBuildError> {
    if !root.is_dir() {
        return Err(RevisionBuildError::Invalid(format!(
            "behavior resource is not a directory: {}",
            root.display()
        )));
    }
    let mut files = BTreeMap::new();
    collect_files(root, root, &mut files)?;
    if files.is_empty() {
        return Err(RevisionBuildError::Invalid(format!(
            "behavior directory is empty: {}",
            root.display()
        )));
    }
    Ok(content_digest(&files))
}

fn collect_files(
    root: &Path,
    current: &Path,
    files: &mut BTreeMap<PathBuf, Vec<u8>>,
) -> Result<(), RevisionBuildError> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            return Err(RevisionBuildError::Invalid(format!(
                "symlink is not allowed: {}",
                entry.path().display()
            )));
        }
        if file_type.is_dir() {
            collect_files(root, &entry.path(), files)?;
        } else if file_type.is_file() {
            let relative = entry
                .path()
                .strip_prefix(root)
                .map_err(|_| RevisionBuildError::Invalid("resource escaped root".to_owned()))?
                .to_path_buf();
            validate_relative_path(&relative).map_err(RevisionBuildError::Invalid)?;
            files.insert(relative, fs::read(entry.path())?);
        }
    }
    Ok(())
}

fn validate_unique_sources(sources: &[KnowledgeSourceBinding]) -> Result<(), RevisionBuildError> {
    let mut seen = BTreeSet::new();
    for source in sources {
        if source.retrieval.top_k == 0 || !seen.insert(&source.id) {
            return Err(RevisionBuildError::Invalid(format!(
                "invalid or duplicate Knowledge Source: {}",
                source.id
            )));
        }
    }
    Ok(())
}

fn validate_unique_tools(tools: &[ToolBinding]) -> Result<(), RevisionBuildError> {
    let mut seen = BTreeSet::new();
    for tool in tools {
        if tool.id.is_empty()
            || tool.version_range.is_empty()
            || !seen.insert((&tool.id, effect_name(tool.effect)))
        {
            return Err(RevisionBuildError::Invalid(format!(
                "invalid or duplicate Tool binding: {}",
                tool.id
            )));
        }
    }
    Ok(())
}

fn effect_name(effect: ToolEffect) -> &'static str {
    match effect {
        ToolEffect::Read => "read",
        ToolEffect::Write => "write",
    }
}

fn read_yaml<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, RevisionBuildError> {
    Ok(serde_yaml::from_reader(fs::File::open(path)?)?)
}

fn resolved(root: &Path, relative: &str) -> Result<PathBuf, RevisionBuildError> {
    let relative = Path::new(relative);
    validate_relative_path(relative).map_err(RevisionBuildError::Invalid)?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        if fs::symlink_metadata(&current)?.file_type().is_symlink() {
            return Err(RevisionBuildError::Invalid(format!(
                "symlink is not allowed: {}",
                relative.display()
            )));
        }
    }
    let path = root.join(relative);
    let canonical = path.canonicalize()?;
    if !canonical.starts_with(root) {
        return Err(RevisionBuildError::Invalid(format!(
            "path escapes project root: {}",
            relative.display()
        )));
    }
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::{build_agent_revision, digest_directory};

    fn fixture() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../abar-api/fixtures/revision-project")
    }

    #[test]
    fn builds_and_validates_frozen_revision() {
        let revision = build_agent_revision(&fixture(), 18).unwrap();
        assert_eq!(revision.metadata.agent, "example-assistant");
        assert_eq!(revision.spec.revision, 18);
        assert_eq!(revision.spec.behavior.skills.len(), 1);
        assert_eq!(revision.spec.knowledge_sources.len(), 1);
        assert!(revision.spec.behavior.prompt.digest.starts_with("sha256:"));
    }

    #[test]
    fn build_is_deterministic() {
        let first = build_agent_revision(&fixture(), 18).unwrap();
        let second = build_agent_revision(&fixture(), 18).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn directory_digest_changes_with_content() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("SKILL.md"), "first").unwrap();
        let first = digest_directory(root.path()).unwrap();
        std::fs::write(root.path().join("SKILL.md"), "second").unwrap();
        assert_ne!(first, digest_directory(root.path()).unwrap());
    }
}
