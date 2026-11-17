use super::{MessageExecutionResult, MessageInput, MessageProviders, execute_message};
use abar_api::context::{RequestContext, TenantContext};
use abar_api::data::{DataClassification, DataPolicy};
use abar_api::message::Message;
use abar_api::provider::{
    BoxFuture, ModelProvider, ModelRequest, ModelResponse, ProviderError, ToolCallRequest,
    ToolCallResult, ToolCallStatus, ToolDescriptor, ToolEffect, ToolIntent, ToolRegistry,
};
use abar_api::revision::AgentRevision;
use std::sync::{Arc, Mutex};

struct Model {
    calls: Arc<Mutex<usize>>,
}
impl ModelProvider for Model {
    fn complete<'a>(
        &'a self,
        _context: &'a RequestContext,
        request: &'a ModelRequest,
    ) -> BoxFuture<'a, Result<ModelResponse, ProviderError>> {
        let calls = self.calls.clone();
        Box::pin(async move {
            let mut calls = calls.lock().unwrap();
            *calls += 1;
            Ok(if *calls == 1 {
                ModelResponse {
                    served_model_id: request.model_id.clone(),
                    output: String::new(),
                    tool_intents: vec![ToolIntent {
                        tool_id: "catalog.query-example".to_owned(),
                        arguments_json: "{}".to_owned(),
                    }],
                    usage: abar_api::message::TokenUsage {
                        prompt_tokens: 10,
                        completion_tokens: 2,
                        total_tokens: 12,
                    },
                }
            } else {
                ModelResponse {
                    served_model_id: request.model_id.clone(),
                    output: "done".to_owned(),
                    tool_intents: Vec::new(),
                    usage: abar_api::message::TokenUsage {
                        prompt_tokens: 5,
                        completion_tokens: 3,
                        total_tokens: 8,
                    },
                }
            })
        })
    }
}

struct Tools;
impl ToolRegistry for Tools {
    fn call<'a>(
        &'a self,
        _context: &'a RequestContext,
        _request: &'a ToolCallRequest,
    ) -> BoxFuture<'a, Result<ToolCallResult, ProviderError>> {
        Box::pin(async {
            Ok(ToolCallResult {
                status: ToolCallStatus::Confirmed,
                result_json: Some("{\"available\":10}".to_owned()),
                message: None,
            })
        })
    }
    fn check<'a>(
        &'a self,
        _context: &'a RequestContext,
        _request: &'a ToolCallRequest,
    ) -> BoxFuture<'a, Result<Option<ToolCallResult>, ProviderError>> {
        Box::pin(async { Ok(None) })
    }
}

struct Policy;
impl DataPolicy for Policy {
    fn classify<'a>(
        &'a self,
        _context: &'a RequestContext,
        _payload: &'a str,
    ) -> BoxFuture<'a, Result<DataClassification, ProviderError>> {
        Box::pin(async { Ok(DataClassification { fields: Vec::new() }) })
    }
    fn project_for_model<'a>(
        &'a self,
        _context: &'a RequestContext,
        payload: &'a str,
        _classification: &'a DataClassification,
        _allow: &'a [String],
        _deny: &'a [String],
    ) -> BoxFuture<'a, Result<String, ProviderError>> {
        Box::pin(async move { Ok(payload.to_owned()) })
    }
    fn guard_output<'a>(
        &'a self,
        _context: &'a RequestContext,
        output: &'a str,
        _deny: &'a [String],
    ) -> BoxFuture<'a, Result<String, ProviderError>> {
        Box::pin(async move { Ok(output.to_owned()) })
    }
}

#[tokio::test]
async fn executes_read_tool_and_returns_output() {
    let revision: AgentRevision = serde_json::from_str(include_str!(
        "../../../abar-api/fixtures/core/agent-revision.json"
    ))
    .unwrap();
    let message: Message =
        serde_json::from_str(include_str!("../../../abar-api/fixtures/core/message.json")).unwrap();
    let context = RequestContext::new("req", TenantContext::new("acme"));
    let model = Model {
        calls: Arc::new(Mutex::new(0)),
    };
    let tools = Tools;
    let policy = Policy;
    let descriptors = [ToolDescriptor {
        id: "catalog.query-example".to_owned(),
        version: "1.0.0".to_owned(),
        description: "query".to_owned(),
        input_schema_json: "{}".to_owned(),
        output_schema_json: "{}".to_owned(),
        effect: ToolEffect::Read,
        schema_digest: format!("sha256:{}", "1".repeat(64)),
    }];
    let execution = execute_message(
        &context,
        &MessageInput {
            message: &message,
            revision: &revision,
            system_prompt: "system",
            automatic_skills: &[],
            manual_skills: &[],
            knowledge: &[],
            history: &[],
            tools: &descriptors,
            model_id: "fake",
            execution_state: super::MessageExecutionState::default(),
        },
        &MessageProviders {
            model: &model,
            tools: &tools,
            data_policy: &policy,
        },
        None,
    )
    .await
    .unwrap();
    let MessageExecutionResult::Completed(output) = execution else {
        panic!("expected completed Message");
    };
    assert_eq!(output.output, "done");
    assert_eq!(output.tool_results.len(), 1);
    assert_eq!(output.tool_results[0].tool_id, "catalog.query-example");
    assert_eq!(output.tool_results[0].content, "{\"available\":10}");
    assert_eq!(output.model_usage.total_tokens, 20);
}
