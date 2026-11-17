#![forbid(unsafe_code)]

//! Deterministic tool registry for offline tests.

use abar_api::context::RequestContext;
use abar_api::error::ErrorCode;
use abar_api::provider::{
    BoxFuture, ProviderError, ToolCallRequest, ToolCallResult, ToolDescriptor, ToolRegistry,
};
use std::collections::VecDeque;
use std::sync::Mutex;

/// Fake registry with a fixed descriptor set and scripted results.
#[derive(Debug)]
pub struct FakeToolRegistry {
    descriptors: Vec<ToolDescriptor>,
    results: Mutex<VecDeque<ToolCallResult>>,
}

impl FakeToolRegistry {
    /// Creates a registry.
    #[must_use]
    pub fn new(descriptors: Vec<ToolDescriptor>, results: Vec<ToolCallResult>) -> Self {
        Self {
            descriptors,
            results: Mutex::new(results.into()),
        }
    }
}

impl ToolRegistry for FakeToolRegistry {
    fn resolve_for_revision<'a>(
        &'a self,
        _context: &'a RequestContext,
        _revision: &'a abar_api::revision::AgentRevision,
    ) -> BoxFuture<'a, Result<Vec<ToolDescriptor>, ProviderError>> {
        let descriptors = self.descriptors.clone();
        Box::pin(async move { Ok(descriptors) })
    }

    fn call<'a>(
        &'a self,
        _context: &'a RequestContext,
        _request: &'a ToolCallRequest,
    ) -> BoxFuture<'a, Result<ToolCallResult, ProviderError>> {
        let result = self
            .results
            .lock()
            .expect("fake result queue poisoned")
            .pop_front()
            .ok_or_else(|| {
                ProviderError::new(
                    ErrorCode::ToolUnavailable,
                    "fake tool result queue is empty",
                    false,
                )
            });
        Box::pin(async move { result })
    }

    fn check<'a>(
        &'a self,
        _context: &'a RequestContext,
        _request: &'a ToolCallRequest,
    ) -> BoxFuture<'a, Result<Option<ToolCallResult>, ProviderError>> {
        Box::pin(async { Ok(None) })
    }
}

#[cfg(test)]
mod tests {
    use super::FakeToolRegistry;
    use abar_api::conformance::validate_tool_descriptors;
    use abar_api::context::{RequestContext, TenantContext};
    use abar_api::provider::{ToolDescriptor, ToolEffect, ToolRegistry};
    use abar_api::revision::AgentRevision;

    #[test]
    fn fake_registry_satisfies_descriptor_contract() {
        let descriptor = ToolDescriptor {
            id: "catalog.query-example".to_owned(),
            version: "1".to_owned(),
            description: "query".to_owned(),
            input_schema_json: "{}".to_owned(),
            output_schema_json: "{}".to_owned(),
            effect: ToolEffect::Read,
            schema_digest: format!("sha256:{}", "a".repeat(64)),
        };
        let registry = FakeToolRegistry::new(vec![descriptor], Vec::new());
        let context = RequestContext::new("req", TenantContext::new("tenant"));
        let revision: AgentRevision = serde_json::from_str(include_str!(
            "../../../abar-api/fixtures/core/agent-revision.json"
        ))
        .unwrap();
        let tools = block_on(registry.resolve_for_revision(&context, &revision)).unwrap();
        assert!(
            validate_tool_descriptors(&tools).is_empty(),
            "{:?}",
            validate_tool_descriptors(&tools)
        );
    }

    fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
        let mut future = std::pin::pin!(future);
        let waker = std::task::Waker::noop();
        let mut context = std::task::Context::from_waker(waker);
        loop {
            match future.as_mut().poll(&mut context) {
                std::task::Poll::Ready(value) => return value,
                std::task::Poll::Pending => std::thread::yield_now(),
            }
        }
    }
}
