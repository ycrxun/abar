//! Behavior content loading for Agent Revision execution.

use crate::context::RequestContext;
use crate::provider::{BoxFuture, ProviderError, SkillContent};
use crate::revision::AgentRevision;

/// Fully resolved behavior content for one revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BehaviorContent {
    pub system_prompt: String,
    pub skills: Vec<SkillContent>,
}

/// Loads Prompt, Skills, and rules referenced by an Agent Revision.
pub trait ContentProvider: Send + Sync {
    fn load<'a>(
        &'a self,
        context: &'a RequestContext,
        revision: &'a AgentRevision,
    ) -> BoxFuture<'a, Result<BehaviorContent, ProviderError>>;
}
