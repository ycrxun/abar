//! Immutable Agent Revision storage and current-pointer management.

mod build;
mod diff;
mod publish;
mod validation;

use abar_api::revision::{AgentRevision, ReviewDecision, ScopeReviewEvidence};
use fs2::FileExt;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub use build::{RevisionBuildError, build_agent_revision, project_agent_name};
pub use diff::{ScopeDiff, compare_resource_scopes, removed_tool_bindings};
pub use publish::{
    PublishOutcome, RevisionPublishError, publish_agent_revision, review_agent_revision,
};
use validation::validate_agent_name;
pub use validation::validate_revision;

/// Revision catalog failure.
#[derive(Debug, Error)]
pub enum RevisionError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid Agent name: {0}")]
    InvalidAgent(String),
    #[error("invalid Agent Revision: {0}")]
    InvalidRevision(String),
    #[error("Agent Revision not found: {agent}:{revision}")]
    NotFound { agent: String, revision: u64 },
    #[error("current Agent Revision is not set: {0}")]
    CurrentNotSet(String),
    #[error("scope review conflict: {0}")]
    ReviewConflict(String),
}

/// Filesystem-backed immutable Revision catalog.
#[derive(Clone, Debug)]
pub struct RevisionStore {
    root: PathBuf,
}

impl RevisionStore {
    /// Creates a Revision catalog rooted at `root`.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Stores one immutable Revision.
    pub fn put(&self, revision: &AgentRevision) -> Result<(), RevisionError> {
        validate_revision(revision)?;
        let _lock = self.lock()?;
        let directory = self.agent_dir(&revision.metadata.agent);
        fs::create_dir_all(&directory)?;
        let path = directory.join(format!("{}.json", revision.spec.revision));
        if path.exists() {
            let existing: AgentRevision = serde_json::from_slice(&fs::read(&path)?)?;
            if &existing == revision {
                return Ok(());
            }
            return Err(RevisionError::InvalidRevision(format!(
                "{}:{} already exists with different content",
                revision.metadata.agent, revision.spec.revision
            )));
        }
        write_json_atomic(&path, revision)
    }

    /// Returns one immutable Revision.
    pub fn get(&self, agent: &str, revision: u64) -> Result<AgentRevision, RevisionError> {
        validate_agent_name(agent)?;
        if revision == 0 {
            return Err(RevisionError::InvalidRevision("0".to_owned()));
        }
        let path = self.agent_dir(agent).join(format!("{revision}.json"));
        if !path.exists() {
            return Err(RevisionError::NotFound {
                agent: agent.to_owned(),
                revision,
            });
        }
        Ok(serde_json::from_slice(&fs::read(path)?)?)
    }

    /// Sets the current Revision pointer.
    pub fn set_current(&self, agent: &str, revision: u64) -> Result<(), RevisionError> {
        self.get(agent, revision)?;
        let _lock = self.lock()?;
        let path = self.agent_dir(agent).join("current");
        write_text_atomic(&path, &format!("{revision}\n"))
    }

    /// Returns the current Revision pointer.
    pub fn current_revision(&self, agent: &str) -> Result<u64, RevisionError> {
        validate_agent_name(agent)?;
        let path = self.agent_dir(agent).join("current");
        if !path.exists() {
            return Err(RevisionError::CurrentNotSet(agent.to_owned()));
        }
        let value = fs::read_to_string(path)?;
        let revision = value.trim().parse::<u64>().map_err(|_| {
            RevisionError::InvalidRevision(format!("invalid current pointer for {agent}"))
        })?;
        self.get(agent, revision)?;
        Ok(revision)
    }

    /// Returns the current immutable Revision.
    pub fn current(&self, agent: &str) -> Result<AgentRevision, RevisionError> {
        let revision = self.current_revision(agent)?;
        self.get(agent, revision)
    }

    /// Lists stored Revision numbers in ascending order.
    pub fn revisions(&self, agent: &str) -> Result<Vec<u64>, RevisionError> {
        validate_agent_name(agent)?;
        let directory = self.agent_dir(agent);
        if !directory.exists() {
            return Ok(Vec::new());
        }
        let mut revisions = Vec::new();
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if entry.file_type()?.is_file()
                && entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "json")
                && let Some(stem) = entry.path().file_stem().and_then(|stem| stem.to_str())
                && let Ok(revision) = stem.parse::<u64>()
            {
                revisions.push(revision);
            }
        }
        revisions.sort_unstable();
        Ok(revisions)
    }

    /// Lists Agent names that have at least one stored Revision.
    pub fn agents(&self) -> Result<Vec<String>, RevisionError> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let mut agents = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let Some(agent) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if validate_agent_name(&agent).is_ok() && !self.revisions(&agent)?.is_empty() {
                agents.push(agent);
            }
        }
        agents.sort();
        Ok(agents)
    }

    /// Returns Scope Review evidence for one candidate Revision, when present.
    pub fn review_evidence(
        &self,
        agent: &str,
        revision: u64,
    ) -> Result<Option<ScopeReviewEvidence>, RevisionError> {
        validate_agent_name(agent)?;
        if revision == 0 {
            return Err(RevisionError::InvalidRevision("0".to_owned()));
        }
        let path = self
            .agent_dir(agent)
            .join("reviews")
            .join(format!("{revision}.json"));
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_slice(&fs::read(path)?)?))
    }

    /// Returns the next Revision number for an Agent.
    pub fn next_revision(&self, agent: &str) -> Result<u64, RevisionError> {
        Ok(self
            .revisions(agent)?
            .last()
            .map_or(1, |revision| revision + 1))
    }

    /// Returns the materialized behavior-content root for a Revision.
    #[must_use]
    pub fn content_root(&self, agent: &str, revision: u64) -> PathBuf {
        self.agent_dir(agent)
            .join("content")
            .join(revision.to_string())
    }

    /// Stores Scope Review evidence and activates an approved candidate.
    pub fn review(&self, evidence: &ScopeReviewEvidence) -> Result<(), RevisionError> {
        validate_review(evidence)?;
        let _lock = self.lock()?;
        let candidate = self.get(&evidence.agent, evidence.candidate_revision)?;
        let current = match self.current_revision(&evidence.agent) {
            Ok(revision) => Some(revision),
            Err(RevisionError::CurrentNotSet(_)) => None,
            Err(error) => return Err(error),
        };
        if evidence.from_revision != current {
            return Err(RevisionError::ReviewConflict(format!(
                "review base {} does not match current {:?}",
                evidence
                    .from_revision
                    .map_or_else(|| "none".to_owned(), |revision| revision.to_string()),
                current
            )));
        }
        let directory = self.agent_dir(&evidence.agent).join("reviews");
        fs::create_dir_all(&directory)?;
        let path = directory.join(format!("{}.json", evidence.candidate_revision));
        if path.exists() {
            let existing: ScopeReviewEvidence = serde_json::from_slice(&fs::read(&path)?)?;
            if &existing != evidence {
                return Err(RevisionError::ReviewConflict(format!(
                    "review already exists for {}:{}",
                    evidence.agent, evidence.candidate_revision
                )));
            }
        } else {
            write_json_atomic(&path, evidence)?;
        }
        if evidence.decision == ReviewDecision::Approved {
            if candidate.metadata.agent != evidence.agent {
                return Err(RevisionError::ReviewConflict(
                    "candidate Agent does not match review".to_owned(),
                ));
            }
            drop(_lock);
            self.set_current(&evidence.agent, evidence.candidate_revision)?;
        }
        Ok(())
    }

    fn agent_dir(&self, agent: &str) -> PathBuf {
        self.root.join(agent)
    }

    fn lock(&self) -> Result<File, RevisionError> {
        fs::create_dir_all(&self.root)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("revisions.lock"))?;
        file.lock_exclusive()?;
        Ok(file)
    }
}

fn write_json_atomic(path: &Path, value: &impl serde::Serialize) -> Result<(), RevisionError> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    write_bytes_atomic(path, &bytes)
}

fn validate_review(evidence: &ScopeReviewEvidence) -> Result<(), RevisionError> {
    validate_agent_name(&evidence.agent)?;
    if evidence.candidate_revision == 0 {
        return Err(RevisionError::InvalidRevision("0".to_owned()));
    }
    if evidence.reviewer.trim().is_empty() || evidence.reviewed_at.trim().is_empty() {
        return Err(RevisionError::ReviewConflict(
            "reviewer and reviewedAt are required".to_owned(),
        ));
    }
    Ok(())
}

fn write_text_atomic(path: &Path, value: &str) -> Result<(), RevisionError> {
    write_bytes_atomic(path, value.as_bytes())
}

fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<(), RevisionError> {
    let temporary = path.with_extension("tmp");
    let mut file = File::create(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{RevisionError, RevisionStore};
    use abar_api::revision::{AgentRevision, ReviewDecision, ScopeReviewEvidence};

    fn fixture() -> AgentRevision {
        serde_json::from_str(include_str!(
            "../../../abar-api/fixtures/core/agent-revision.json"
        ))
        .unwrap()
    }

    #[test]
    fn stores_and_resolves_current_revision() {
        let root = tempfile::tempdir().unwrap();
        let store = RevisionStore::new(root.path());
        let revision = fixture();
        store.put(&revision).unwrap();
        store.put(&revision).unwrap();
        store.set_current("example-assistant", 18).unwrap();

        assert_eq!(store.revisions("example-assistant").unwrap(), vec![18]);
        assert_eq!(store.current_revision("example-assistant").unwrap(), 18);
        assert_eq!(store.current("example-assistant").unwrap(), revision);
    }

    #[test]
    fn rejects_revision_rewrite() {
        let root = tempfile::tempdir().unwrap();
        let store = RevisionStore::new(root.path());
        let revision = fixture();
        store.put(&revision).unwrap();
        let mut changed = revision;
        changed.spec.behavior.prompt.digest = format!("sha256:{}", "9".repeat(64));
        assert!(matches!(
            store.put(&changed),
            Err(RevisionError::InvalidRevision(_))
        ));
    }

    #[test]
    fn rejects_unknown_current_revision() {
        let root = tempfile::tempdir().unwrap();
        let store = RevisionStore::new(root.path());
        assert!(matches!(
            store.current("missing-agent"),
            Err(RevisionError::CurrentNotSet(_))
        ));
    }

    #[test]
    fn approves_candidate_bound_to_current_revision() {
        let root = tempfile::tempdir().unwrap();
        let store = RevisionStore::new(root.path());
        let first = fixture();
        store.put(&first).unwrap();
        store.set_current("example-assistant", 18).unwrap();
        let mut candidate = fixture();
        candidate.spec.revision = 19;
        candidate
            .spec
            .resource_scope
            .tools
            .write
            .push("records.cancel".to_owned());
        store.put(&candidate).unwrap();
        store
            .review(&ScopeReviewEvidence {
                agent: "example-assistant".to_owned(),
                from_revision: Some(18),
                candidate_revision: 19,
                reviewer: "security@example.com".to_owned(),
                decision: ReviewDecision::Approved,
                comment: None,
                reviewed_at: "2026-11-17T10:00:00Z".to_owned(),
            })
            .unwrap();
        assert_eq!(store.current_revision("example-assistant").unwrap(), 19);
        assert_eq!(
            store.agents().unwrap(),
            vec!["example-assistant".to_owned()]
        );
        assert_eq!(
            store
                .review_evidence("example-assistant", 19)
                .unwrap()
                .unwrap()
                .decision,
            ReviewDecision::Approved
        );
    }

    #[test]
    fn rejects_stale_scope_review() {
        let root = tempfile::tempdir().unwrap();
        let store = RevisionStore::new(root.path());
        let first = fixture();
        store.put(&first).unwrap();
        store.set_current("example-assistant", 18).unwrap();
        let mut candidate = first.clone();
        candidate.spec.revision = 19;
        store.put(&candidate).unwrap();
        let result = store.review(&ScopeReviewEvidence {
            agent: "example-assistant".to_owned(),
            from_revision: Some(17),
            candidate_revision: 19,
            reviewer: "security@example.com".to_owned(),
            decision: ReviewDecision::Approved,
            comment: None,
            reviewed_at: "2026-11-17T10:00:00Z".to_owned(),
        });
        assert!(matches!(result, Err(RevisionError::ReviewConflict(_))));
        assert_eq!(store.current_revision("example-assistant").unwrap(), 18);
    }
}
