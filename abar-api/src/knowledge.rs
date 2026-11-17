//! Knowledge source and retrieval contracts.

use crate::context::RequestContext;
use crate::provider::{BoxFuture, ProviderError};
use serde::{Deserialize, Serialize};

/// One retrieval request against an allowed Knowledge Source.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeQuery {
    pub source: String,
    pub query: String,
    pub top_k: u32,
}

/// Knowledge document returned to Runtime.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeDocument {
    pub source: String,
    pub document_id: String,
    pub version: String,
    pub digest: String,
    pub content: String,
    pub retrieved_at: String,
}

/// Citation evidence recorded by a Message.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeCitation {
    pub source: String,
    pub document_id: String,
    pub version: String,
    pub digest: String,
    pub retrieved_at: String,
}

impl From<&KnowledgeDocument> for KnowledgeCitation {
    fn from(document: &KnowledgeDocument) -> Self {
        Self {
            source: document.source.clone(),
            document_id: document.document_id.clone(),
            version: document.version.clone(),
            digest: document.digest.clone(),
            retrieved_at: document.retrieved_at.clone(),
        }
    }
}

/// Replaceable knowledge retrieval boundary.
pub trait KnowledgeProvider: Send + Sync {
    /// Retrieves documents from one already-authorized Knowledge Source.
    fn retrieve<'a>(
        &'a self,
        context: &'a RequestContext,
        query: &'a KnowledgeQuery,
    ) -> BoxFuture<'a, Result<Vec<KnowledgeDocument>, ProviderError>>;
}
