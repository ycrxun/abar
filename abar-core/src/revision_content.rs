//! File-backed behavior content for Agent Revisions.

use crate::digest::{content_digest, sha256_hex};
use crate::revision::RevisionStore;
use abar_api::content::{BehaviorContent, ContentProvider};
use abar_api::context::RequestContext;
use abar_api::error::ErrorCode;
use abar_api::provider::{BoxFuture, ProviderError, SkillContent, SkillMetadata};
use abar_api::revision::AgentRevision;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Loads behavior files materialized beside an immutable Revision.
#[derive(Clone, Debug)]
pub struct RevisionContentProvider {
    store: RevisionStore,
}

impl RevisionContentProvider {
    /// Creates a provider for a Revision catalog.
    #[must_use]
    pub fn new(store: RevisionStore) -> Self {
        Self { store }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BehaviorManifest {
    prompt: String,
    #[serde(default)]
    skills: Vec<ResourceRef>,
    #[serde(default)]
    rules: Vec<ResourceRef>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResourceRef {
    id: String,
    version: String,
    path: String,
}

#[derive(Deserialize)]
struct SkillFrontmatter {
    name: String,
    description: String,
    #[serde(rename = "disable-model-invocation", default)]
    disable_model_invocation: bool,
}

impl ContentProvider for RevisionContentProvider {
    fn load<'a>(
        &'a self,
        _context: &'a RequestContext,
        revision: &'a AgentRevision,
    ) -> BoxFuture<'a, Result<BehaviorContent, ProviderError>> {
        let result = load_behavior(&self.store, revision);
        Box::pin(async move { result })
    }
}

fn load_behavior(
    store: &RevisionStore,
    revision: &AgentRevision,
) -> Result<BehaviorContent, ProviderError> {
    let root = store.content_root(&revision.metadata.agent, revision.spec.revision);
    let manifest: BehaviorManifest = read_yaml(&root.join("behavior.yaml"))?;
    let prompt_path = safe_join(&root, &manifest.prompt)?;
    let prompt = fs::read(&prompt_path).map_err(io_error)?;
    if sha256_hex(&prompt) != revision.spec.behavior.prompt.digest {
        return Err(denied("Prompt digest does not match Agent Revision"));
    }
    let mut system_prompt =
        String::from_utf8(prompt).map_err(|_| denied("Prompt is not valid UTF-8"))?;
    if !manifest.rules.is_empty() {
        system_prompt.push_str("\n\n[business rules]\n");
        for rule in &manifest.rules {
            let path = safe_join(&root, &rule.path)?;
            let bytes = fs::read(path).map_err(io_error)?;
            let expected = revision
                .spec
                .behavior
                .rules
                .iter()
                .find(|value| value.id == rule.id)
                .ok_or_else(|| denied(format!("Rule is not declared: {}", rule.id)))?;
            if sha256_hex(&bytes) != expected.digest {
                return Err(denied(format!(
                    "Rule digest does not match Agent Revision: {}",
                    rule.id
                )));
            }
            system_prompt.push_str(
                &String::from_utf8(bytes)
                    .map_err(|_| denied(format!("Rule is not valid UTF-8: {}", rule.id)))?,
            );
            system_prompt.push('\n');
        }
    }
    let skills = manifest
        .skills
        .into_iter()
        .map(|resource| load_skill(&root, revision, resource))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(BehaviorContent {
        system_prompt,
        skills,
    })
}

fn load_skill(
    root: &Path,
    revision: &AgentRevision,
    resource: ResourceRef,
) -> Result<SkillContent, ProviderError> {
    let directory = safe_join(root, &resource.path)?;
    let files = collect_files(&directory)?;
    if files.len() != 1 || !files.contains_key(Path::new("SKILL.md")) {
        return Err(denied(
            "only SKILL.md is supported in the current Skill format",
        ));
    }
    let expected = revision
        .spec
        .behavior
        .skills
        .iter()
        .find(|skill| skill.id == resource.id)
        .ok_or_else(|| denied(format!("Skill is not declared: {}", resource.id)))?;
    if expected.version != resource.version {
        return Err(denied(format!(
            "Skill version does not match Agent Revision: {}",
            resource.id
        )));
    }
    if content_digest(&files) != expected.digest {
        return Err(denied(format!(
            "Skill digest does not match Agent Revision: {}",
            resource.id
        )));
    }
    let markdown = files
        .get(Path::new("SKILL.md"))
        .ok_or_else(|| denied(format!("SKILL.md is missing: {}", resource.id)))?;
    let markdown = std::str::from_utf8(markdown).map_err(|_| denied("Skill is not valid UTF-8"))?;
    let frontmatter = parse_frontmatter(markdown)?;
    if frontmatter.name != resource.id {
        return Err(denied("Skill name does not match behavior manifest"));
    }
    Ok(SkillContent {
        metadata: SkillMetadata {
            name: frontmatter.name,
            description: frontmatter.description,
            disable_model_invocation: frontmatter.disable_model_invocation,
            version: Some(resource.version),
        },
        markdown: markdown.to_owned(),
    })
}

fn collect_files(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>, ProviderError> {
    let mut files = BTreeMap::new();
    collect_files_inner(root, root, &mut files)?;
    Ok(files)
}

fn collect_files_inner(
    root: &Path,
    current: &Path,
    files: &mut BTreeMap<PathBuf, Vec<u8>>,
) -> Result<(), ProviderError> {
    for entry in fs::read_dir(current).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let file_type = entry.file_type().map_err(io_error)?;
        if file_type.is_symlink() {
            return Err(denied("Skill content contains a symlink"));
        }
        if file_type.is_dir() {
            collect_files_inner(root, &entry.path(), files)?;
        } else if file_type.is_file() {
            let relative = entry
                .path()
                .strip_prefix(root)
                .map_err(|_| denied("Skill path escaped root"))?
                .to_path_buf();
            files.insert(relative, fs::read(entry.path()).map_err(io_error)?);
        }
    }
    Ok(())
}

fn safe_join(root: &Path, relative: &str) -> Result<PathBuf, ProviderError> {
    let relative = Path::new(relative);
    if relative
        .components()
        .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(denied("unsafe behavior content path"));
    }
    let path = root.join(relative);
    if !path.starts_with(root) {
        return Err(denied("behavior content path escaped root"));
    }
    Ok(path)
}

fn parse_frontmatter(content: &str) -> Result<SkillFrontmatter, ProviderError> {
    let rest = content
        .strip_prefix("---\n")
        .ok_or_else(|| denied("Skill frontmatter is missing"))?;
    let end = rest
        .find("\n---\n")
        .ok_or_else(|| denied("Skill frontmatter is not closed"))?;
    serde_yaml::from_str(&rest[..end])
        .map_err(|error| denied(format!("invalid Skill frontmatter: {error}")))
}

fn read_yaml<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, ProviderError> {
    serde_yaml::from_reader(fs::File::open(path).map_err(io_error)?)
        .map_err(|error| denied(format!("invalid behavior manifest: {error}")))
}

fn io_error(error: std::io::Error) -> ProviderError {
    ProviderError::new(ErrorCode::RevisionInvalid, error.to_string(), false)
}

fn denied(message: impl Into<String>) -> ProviderError {
    ProviderError::new(ErrorCode::SkillDenied, message, false)
}

#[cfg(test)]
mod tests {
    use super::RevisionContentProvider;
    use crate::revision::{RevisionStore, publish_agent_revision};
    use abar_api::content::ContentProvider;
    use abar_api::context::{RequestContext, TenantContext};
    use std::path::Path;

    #[tokio::test]
    async fn loads_materialized_behavior_and_verifies_digests() {
        let root = tempfile::tempdir().unwrap();
        let store = RevisionStore::new(root.path());
        let project =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../abar-api/fixtures/revision-project");
        publish_agent_revision(&store, &project).unwrap();
        let revision = store.current("example-assistant").unwrap();
        let provider = RevisionContentProvider::new(store);
        let context = RequestContext::new("req", TenantContext::new("acme"));
        let behavior = provider.load(&context, &revision).await.unwrap();
        assert!(behavior.system_prompt.contains("business rules"));
        assert_eq!(behavior.skills.len(), 1);
        assert_eq!(behavior.skills[0].metadata.name, "decision-guidance");
    }
}
