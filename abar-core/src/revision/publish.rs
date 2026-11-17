//! Revision publication orchestration.

use super::{
    RevisionBuildError, RevisionError, RevisionStore, build_agent_revision,
    compare_resource_scopes, project_agent_name,
};
use abar_api::revision::{AgentRevision, ReviewDecision, ScopeReviewEvidence};
use std::fs;
use std::path::Path;
use thiserror::Error;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// Revision publication failure.
#[derive(Debug, Error)]
pub enum RevisionPublishError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("build failed: {0}")]
    Build(#[from] RevisionBuildError),
    #[error("store failed: {0}")]
    Store(#[from] RevisionError),
    #[error("cannot produce review timestamp: {0}")]
    ReviewTime(String),
}

/// Result of publishing a Revision candidate.
#[derive(Clone, Debug)]
pub struct PublishOutcome {
    pub revision: AgentRevision,
    pub created: bool,
    pub active: bool,
    pub review_required: bool,
    pub scope_diff: Option<super::ScopeDiff>,
}

/// Records a Scope Review and returns its immutable evidence.
pub fn review_agent_revision(
    store: &RevisionStore,
    agent: &str,
    candidate_revision: u64,
    reviewer: &str,
    decision: ReviewDecision,
    comment: Option<String>,
) -> Result<ScopeReviewEvidence, RevisionPublishError> {
    let from_revision = match store.current_revision(agent) {
        Ok(revision) => Some(revision),
        Err(RevisionError::CurrentNotSet(_)) => None,
        Err(error) => return Err(error.into()),
    };
    let reviewed_at = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| RevisionPublishError::ReviewTime(error.to_string()))?;
    let evidence = ScopeReviewEvidence {
        agent: agent.to_owned(),
        from_revision,
        candidate_revision,
        reviewer: reviewer.to_owned(),
        decision,
        comment,
        reviewed_at,
    };
    store.review(&evidence)?;
    Ok(evidence)
}

/// Builds and publishes a Revision project.
pub fn publish_agent_revision(
    store: &RevisionStore,
    source: &Path,
) -> Result<PublishOutcome, RevisionPublishError> {
    let agent = project_agent_name(source)?;
    let current = match store.current(&agent) {
        Ok(revision) => Some(revision),
        Err(RevisionError::CurrentNotSet(_)) => None,
        Err(error) => return Err(error.into()),
    };
    if let Some(current) = current.as_ref() {
        let same = build_agent_revision(source, current.spec.revision)?;
        if &same == current {
            materialize_behavior(store, source, &agent, current.spec.revision)?;
            return Ok(PublishOutcome {
                revision: current.clone(),
                created: false,
                active: true,
                review_required: false,
                scope_diff: None,
            });
        }
    }

    let revision_number = store.next_revision(&agent)?;
    let candidate = build_agent_revision(source, revision_number)?;
    let scope_diff = current.as_ref().map(|current| {
        compare_resource_scopes(&current.spec.resource_scope, &candidate.spec.resource_scope)
    });
    let review_required = scope_diff
        .as_ref()
        .is_some_and(|difference| difference.review_required);
    materialize_behavior(store, source, &agent, revision_number)?;
    store.put(&candidate)?;
    if !review_required {
        store.set_current(&agent, revision_number)?;
    }
    Ok(PublishOutcome {
        revision: candidate,
        created: true,
        active: !review_required,
        review_required,
        scope_diff,
    })
}

fn materialize_behavior(
    store: &RevisionStore,
    source: &Path,
    agent: &str,
    revision: u64,
) -> Result<(), RevisionPublishError> {
    let destination = store.content_root(agent, revision);
    if destination.exists() {
        return Ok(());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = destination.with_extension("tmp");
    if temporary.exists() {
        fs::remove_dir_all(&temporary)?;
    }
    fs::create_dir_all(&temporary)?;
    for file in ["agent.yaml", "scope.yaml", "behavior.yaml", "tools.yaml"] {
        copy_file(&source.join(file), &temporary.join(file))?;
    }
    for directory in ["prompt", "skills", "rules"] {
        copy_directory(&source.join(directory), &temporary.join(directory))?;
    }
    fs::rename(temporary, destination)?;
    Ok(())
}

fn copy_file(source: &Path, destination: &Path) -> Result<(), RevisionPublishError> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, destination)?;
    Ok(())
}

fn copy_directory(source: &Path, destination: &Path) -> Result<(), RevisionPublishError> {
    if !source.exists() {
        return Ok(());
    }
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            return Err(RevisionPublishError::Build(RevisionBuildError::Invalid(
                format!("symlink is not allowed: {}", entry.path().display()),
            )));
        }
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_directory(&entry.path(), &target)?;
        } else if file_type.is_file() {
            copy_file(&entry.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::publish_agent_revision;
    use crate::revision::RevisionStore;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn fixture() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../abar-api/fixtures/revision-project")
    }

    fn copy_fixture() -> tempfile::TempDir {
        let temporary = tempfile::tempdir().unwrap();
        copy_tree(&fixture(), temporary.path());
        temporary
    }

    fn copy_tree(source: &Path, target: &Path) {
        fs::create_dir_all(target).unwrap();
        for entry in fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            let destination = target.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &destination);
            } else {
                fs::copy(entry.path(), destination).unwrap();
            }
        }
    }

    #[test]
    fn first_publication_becomes_current() {
        let root = tempfile::tempdir().unwrap();
        let store = RevisionStore::new(root.path());
        let outcome = publish_agent_revision(&store, &fixture()).unwrap();
        assert!(outcome.created);
        assert!(outcome.active);
        assert!(!outcome.review_required);
        assert_eq!(store.current("example-assistant").unwrap().spec.revision, 1);
    }

    #[test]
    fn unchanged_project_is_idempotent() {
        let root = tempfile::tempdir().unwrap();
        let store = RevisionStore::new(root.path());
        assert!(publish_agent_revision(&store, &fixture()).unwrap().created);
        let second = publish_agent_revision(&store, &fixture()).unwrap();
        assert!(!second.created);
        assert_eq!(store.revisions("example-assistant").unwrap(), vec![1]);
    }

    #[test]
    fn behavior_change_becomes_current_without_review() {
        let project = copy_fixture();
        let root = tempfile::tempdir().unwrap();
        let store = RevisionStore::new(root.path());
        publish_agent_revision(&store, project.path()).unwrap();
        fs::write(
            project.path().join("rules/decision-rules.yaml"),
            "minimumQuantity: 3\n",
        )
        .unwrap();
        let outcome = publish_agent_revision(&store, project.path()).unwrap();
        assert!(outcome.created);
        assert!(outcome.active);
        assert!(!outcome.review_required);
        assert_eq!(store.current("example-assistant").unwrap().spec.revision, 2);
    }

    #[test]
    fn expanded_scope_waits_for_review() {
        let project = copy_fixture();
        let root = tempfile::tempdir().unwrap();
        let store = RevisionStore::new(root.path());
        publish_agent_revision(&store, project.path()).unwrap();
        let scope_path = project.path().join("scope.yaml");
        let mut scope = fs::read_to_string(&scope_path).unwrap();
        scope = scope.replace(
            "    - records.apply\n",
            "    - records.apply\n    - records.cancel\n",
        );
        fs::write(scope_path, scope).unwrap();
        let outcome = publish_agent_revision(&store, project.path()).unwrap();
        assert!(outcome.created);
        assert!(!outcome.active);
        assert!(outcome.review_required);
        assert_eq!(store.current("example-assistant").unwrap().spec.revision, 1);
        assert_eq!(store.revisions("example-assistant").unwrap(), vec![1, 2]);
    }
}
