#![forbid(unsafe_code)]

//! Deterministic model provider used by tests.

use abar_api::context::RequestContext;
use abar_api::provider::{BoxFuture, ModelProvider, ModelRequest, ModelResponse, ProviderError};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// A provider that returns a fixed response.
#[derive(Debug)]
pub struct FakeModelProvider {
    served_model_id: String,
    output: String,
    responses: Mutex<VecDeque<ModelResponse>>,
    delay: Duration,
    calls: AtomicUsize,
}

impl FakeModelProvider {
    /// Creates a deterministic provider.
    #[must_use]
    pub fn new(served_model_id: impl Into<String>, output: impl Into<String>) -> Self {
        Self {
            served_model_id: served_model_id.into(),
            output: output.into(),
            responses: Mutex::new(VecDeque::new()),
            delay: Duration::ZERO,
            calls: AtomicUsize::new(0),
        }
    }

    /// Creates a provider with scripted responses followed by a final fixed response.
    #[must_use]
    pub fn with_responses(
        served_model_id: impl Into<String>,
        output: impl Into<String>,
        responses: Vec<ModelResponse>,
    ) -> Self {
        Self {
            served_model_id: served_model_id.into(),
            output: output.into(),
            responses: Mutex::new(responses.into()),
            delay: Duration::ZERO,
            calls: AtomicUsize::new(0),
        }
    }

    /// Adds an asynchronous response delay.
    #[must_use]
    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    /// Returns the number of completed model calls.
    #[must_use]
    pub fn call_count(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }
}

impl ModelProvider for FakeModelProvider {
    fn complete<'a>(
        &'a self,
        _context: &'a RequestContext,
        _request: &'a ModelRequest,
    ) -> BoxFuture<'a, Result<ModelResponse, ProviderError>> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let delay = self.delay;
        let response = self
            .responses
            .lock()
            .expect("fake response queue poisoned")
            .pop_front()
            .unwrap_or_else(|| ModelResponse {
                served_model_id: self.served_model_id.clone(),
                output: self.output.clone(),
                tool_intents: Vec::new(),
                usage: Default::default(),
            });
        Box::pin(async move {
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            Ok(response)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::FakeModelProvider;
    use abar_api::conformance::validate_model_response;
    use abar_api::context::{RequestContext, TenantContext};
    use abar_api::provider::{ModelProvider, ModelRequest};

    #[test]
    fn returns_fixed_response_and_counts_calls() {
        let provider = FakeModelProvider::new("model-v1", "ok");
        let context = RequestContext::new("req-1", TenantContext::new("tenant"));
        let request = ModelRequest {
            model_id: "model-v1".to_owned(),
            input: "hello".to_owned(),
            tools: Vec::new(),
            tool_results: Vec::new(),
        };

        let response = futures_lite_block_on(provider.complete(&context, &request));

        let response = response.unwrap();
        assert!(
            validate_model_response(&request, &response).is_empty(),
            "{:?}",
            validate_model_response(&request, &response)
        );
        assert_eq!(response.output, "ok");
        assert_eq!(provider.call_count(), 1);
    }

    fn futures_lite_block_on<T>(future: impl std::future::Future<Output = T>) -> T {
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
