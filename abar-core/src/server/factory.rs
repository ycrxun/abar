//! Runtime provider construction boundary.

use abar_api::context::RequestContext;
use abar_api::provider::{BoxFuture, ProviderError};
use abar_api::revision::AgentRevision;
use std::sync::Arc;

/// Creates and validates Message runtime providers.
pub trait RuntimeProviderFactory: Send + Sync {
    /// Validates the local Message provider configuration.
    fn validate_message_runtime(&self) -> Result<(), String> {
        Err("Message providers are not configured".to_owned())
    }

    /// Resolves the Message Runtime bundle.
    fn message_bundle<'a>(
        &'a self,
        _context: &'a RequestContext,
        _revision: &'a AgentRevision,
        _input: &'a str,
    ) -> BoxFuture<'a, Result<crate::message::ResolvedMessageBundle, ProviderError>> {
        Box::pin(async {
            Err(ProviderError::new(
                abar_api::error::ErrorCode::InvalidRequest,
                "Message providers are not configured",
                false,
            ))
        })
    }
}

/// Owned adapter from RuntimeProviderFactory to MessageBundleFactory.
pub(super) struct RuntimeBundleFactory {
    pub(super) factory: Arc<dyn RuntimeProviderFactory>,
}

impl crate::message::MessageBundleFactory for RuntimeBundleFactory {
    fn resolve<'a>(
        &'a self,
        context: &'a RequestContext,
        revision: &'a AgentRevision,
        input: &'a str,
    ) -> BoxFuture<'a, Result<crate::message::ResolvedMessageBundle, ProviderError>> {
        self.factory.message_bundle(context, revision, input)
    }
}
