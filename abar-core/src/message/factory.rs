//! Default layered Message Bundle Factory.

use super::{MessageBundleFactory, ResolvedMessageBundle};
use crate::provider_bundle::{ProviderBundleInput, build_provider_bundle};
use abar_api::content::ContentProvider;
use abar_api::context::RequestContext;
use abar_api::data::DataPolicy;
use abar_api::knowledge::{KnowledgeDocument, KnowledgeProvider, KnowledgeQuery};
use abar_api::provider::{BoxFuture, ModelProvider, ProviderError, ToolRegistry};
use abar_api::revision::AgentRevision;
use std::sync::Arc;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// Composes Revision content, Tool, Knowledge, Model, and DataPolicy providers.
#[derive(Clone)]
pub struct RevisionMessageBundleFactory {
    pub content: Arc<dyn ContentProvider>,
    pub tools: Arc<dyn ToolRegistry>,
    pub knowledge: Arc<dyn KnowledgeProvider>,
    pub data_policy: Arc<dyn DataPolicy>,
    pub model: Arc<dyn ModelProvider>,
    pub model_id: String,
    pub model_version: String,
}

impl std::fmt::Debug for RevisionMessageBundleFactory {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RevisionMessageBundleFactory")
            .field("model_id", &self.model_id)
            .field("model_version", &self.model_version)
            .finish_non_exhaustive()
    }
}

impl MessageBundleFactory for RevisionMessageBundleFactory {
    fn resolve<'a>(
        &'a self,
        context: &'a RequestContext,
        revision: &'a AgentRevision,
        input: &'a str,
    ) -> BoxFuture<'a, Result<ResolvedMessageBundle, ProviderError>> {
        Box::pin(async move {
            let behavior = self.content.load(context, revision).await?;
            let tools = self.tools.resolve_for_revision(context, revision).await?;
            let mut knowledge = Vec::<KnowledgeDocument>::new();
            for source in &revision.spec.knowledge_sources {
                match self
                    .knowledge
                    .retrieve(
                        context,
                        &KnowledgeQuery {
                            source: source.id.clone(),
                            query: input.to_owned(),
                            top_k: source.retrieval.top_k,
                        },
                    )
                    .await
                {
                    Ok(mut documents) => knowledge.append(&mut documents),
                    Err(error) if !source.required => {
                        let _ = error;
                    }
                    Err(error) => return Err(error),
                }
            }
            let resolved_at = OffsetDateTime::now_utc()
                .format(&Rfc3339)
                .map_err(|error| {
                    ProviderError::new(
                        abar_api::error::ErrorCode::InvalidRequest,
                        error.to_string(),
                        false,
                    )
                })?;
            let bundle = build_provider_bundle(ProviderBundleInput {
                revision,
                behavior: &behavior,
                tools: &tools,
                knowledge: &knowledge,
                model_id: &self.model_id,
                model_version: &self.model_version,
                resolved_at: &resolved_at,
            })
            .map_err(|error| {
                ProviderError::new(
                    abar_api::error::ErrorCode::InvalidRequest,
                    error.to_string(),
                    false,
                )
            })?;
            Ok(ResolvedMessageBundle {
                bundle,
                model: self.model.clone(),
                tools: self.tools.clone(),
                data_policy: self.data_policy.clone(),
            })
        })
    }
}
