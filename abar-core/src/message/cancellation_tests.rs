use super::{MessageBundleFactory, ResolvedMessageBundle, start_message};
use crate::provider_bundle::{ProviderBundleInput, build_provider_bundle};
use crate::session::SessionStore;
use abar_api::content::BehaviorContent;
use abar_api::context::{RequestContext, TenantContext};
use abar_api::data::{DataClassification, DataPolicy};
use abar_api::knowledge::KnowledgeDocument;
use abar_api::message::MessageState;
use abar_api::provider::{
    BoxFuture, ModelProvider, ModelRequest, ModelResponse, ProviderError, SkillContent,
    SkillMetadata, ToolCallRequest, ToolCallResult, ToolCallStatus, ToolDescriptor, ToolEffect,
    ToolIntent, ToolRegistry,
};
use abar_api::revision::AgentRevision;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

struct Model;
impl ModelProvider for Model {
    fn complete<'a>(
        &'a self,
        _context: &'a RequestContext,
        request: &'a ModelRequest,
    ) -> BoxFuture<'a, Result<ModelResponse, ProviderError>> {
        Box::pin(async move {
            Ok(ModelResponse {
                served_model_id: request.model_id.clone(),
                output: String::new(),
                tool_intents: vec![ToolIntent {
                    tool_id: "catalog.query-example".to_owned(),
                    arguments_json: "{}".to_owned(),
                }],
                usage: Default::default(),
            })
        })
    }
}

struct CancellingTools {
    cancellation: Arc<AtomicBool>,
}

impl ToolRegistry for CancellingTools {
    fn call<'a>(
        &'a self,
        _context: &'a RequestContext,
        _request: &'a ToolCallRequest,
    ) -> BoxFuture<'a, Result<ToolCallResult, ProviderError>> {
        let cancellation = self.cancellation.clone();
        Box::pin(async move {
            cancellation.store(true, Ordering::Release);
            Ok(ToolCallResult {
                status: ToolCallStatus::Confirmed,
                result_json: Some("{\"ok\":true}".to_owned()),
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

struct Factory(ResolvedMessageBundle);
impl MessageBundleFactory for Factory {
    fn resolve<'a>(
        &'a self,
        _context: &'a RequestContext,
        _revision: &'a AgentRevision,
        _input: &'a str,
    ) -> BoxFuture<'a, Result<ResolvedMessageBundle, ProviderError>> {
        Box::pin(async move { Ok(self.0.clone()) })
    }
}

fn bundle(cancellation: Arc<AtomicBool>) -> ResolvedMessageBundle {
    let revision: AgentRevision = serde_json::from_str(include_str!(
        "../../../abar-api/fixtures/core/agent-revision.json"
    ))
    .unwrap();
    let behavior = BehaviorContent {
        system_prompt: "system".to_owned(),
        skills: [("decision-guidance", "11"), ("approval-guidance", "7")]
            .into_iter()
            .map(|(name, version)| SkillContent {
                metadata: SkillMetadata {
                    name: name.to_owned(),
                    description: name.to_owned(),
                    disable_model_invocation: false,
                    version: Some(version.to_owned()),
                },
                markdown: name.to_owned(),
            })
            .collect(),
    };
    let tools = [
        ("catalog.query-example", ToolEffect::Read),
        ("records.apply", ToolEffect::Write),
    ]
    .into_iter()
    .map(|(id, effect)| ToolDescriptor {
        id: id.to_owned(),
        version: "1".to_owned(),
        description: id.to_owned(),
        input_schema_json: "{}".to_owned(),
        output_schema_json: "{}".to_owned(),
        effect,
        schema_digest: format!("sha256:{}", "1".repeat(64)),
    })
    .collect::<Vec<_>>();
    let knowledge = vec![KnowledgeDocument {
        source: "policy".to_owned(),
        document_id: "doc".to_owned(),
        version: "1".to_owned(),
        digest: format!("sha256:{}", "2".repeat(64)),
        content: "policy".to_owned(),
        retrieved_at: "2026-11-17T09:00:00Z".to_owned(),
    }];
    let bundle = build_provider_bundle(ProviderBundleInput {
        revision: &revision,
        behavior: &behavior,
        tools: &tools,
        knowledge: &knowledge,
        model_id: "fake",
        model_version: "1",
        resolved_at: "2026-11-17T09:00:01Z",
    })
    .unwrap();
    ResolvedMessageBundle {
        bundle,
        model: Arc::new(Model),
        tools: Arc::new(CancellingTools { cancellation }),
        data_policy: Arc::new(Policy),
    }
}

#[tokio::test]
async fn cancellation_keeps_confirmed_tool_evidence() {
    let directory = tempfile::tempdir().unwrap();
    let store = SessionStore::new(directory.path());
    let session = store.create("acme", "example-assistant", "alice").unwrap();
    let revision: AgentRevision = serde_json::from_str(include_str!(
        "../../../abar-api/fixtures/core/agent-revision.json"
    ))
    .unwrap();
    let cancellation = Arc::new(AtomicBool::new(false));
    let factory = Factory(bundle(cancellation.clone()));
    let context = RequestContext::new("req", TenantContext::new("acme"));
    let message = start_message(
        &store,
        &context,
        &session.id,
        &revision,
        "query",
        &[],
        &factory,
        Some(cancellation),
    )
    .await
    .unwrap();
    assert_eq!(message.state, MessageState::Cancelled);
    assert_eq!(message.tool_results.len(), 1);
    assert_eq!(message.tool_results[0].content, "{\"ok\":true}");
}
