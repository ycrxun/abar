use super::{
    MessageBundleFactory, ResolvedMessageBundle, execute_prepared_message, prepare_message,
    reconcile_message, resume_message, start_message,
};
use crate::provider_bundle::{ProviderBundleInput, build_provider_bundle};
use crate::session::SessionStore;
use abar_api::content::BehaviorContent;
use abar_api::context::{RequestContext, TenantContext};
use abar_api::data::{DataClassification, DataPolicy};
use abar_api::knowledge::KnowledgeDocument;
use abar_api::message::{MessageApproval, MessageState};
use abar_api::provider::{
    BoxFuture, ModelProvider, ModelRequest, ModelResponse, ProviderError, SkillContent,
    SkillMetadata, ToolCallRequest, ToolCallResult, ToolCallStatus, ToolDescriptor, ToolEffect,
    ToolIntent, ToolRegistry,
};
use abar_api::revision::AgentRevision;
use std::sync::atomic::AtomicBool;
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
                        tool_id: "records.apply".to_owned(),
                        arguments_json: "{\"plan\":\"approved\"}".to_owned(),
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
                    output: "operation completed".to_owned(),
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
        request: &'a ToolCallRequest,
    ) -> BoxFuture<'a, Result<ToolCallResult, ProviderError>> {
        assert_eq!(request.effect, ToolEffect::Write);
        assert!(request.approval.is_some());
        Box::pin(async {
            Ok(ToolCallResult {
                status: ToolCallStatus::Confirmed,
                result_json: Some("{\"orderId\":\"TO-1\"}".to_owned()),
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

struct ExternalReadModel {
    calls: Arc<Mutex<usize>>,
}

impl ModelProvider for ExternalReadModel {
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
                        arguments_json: "{\"region\":\"east\"}".to_owned(),
                    }],
                    usage: Default::default(),
                }
            } else {
                ModelResponse {
                    served_model_id: request.model_id.clone(),
                    output: "catalog checked".to_owned(),
                    tool_intents: Vec::new(),
                    usage: Default::default(),
                }
            })
        })
    }
}

struct ExternalReadTools {
    checks: Arc<Mutex<usize>>,
    calls: Arc<Mutex<usize>>,
}

impl ToolRegistry for ExternalReadTools {
    fn call<'a>(
        &'a self,
        _context: &'a RequestContext,
        _request: &'a ToolCallRequest,
    ) -> BoxFuture<'a, Result<ToolCallResult, ProviderError>> {
        let calls = self.calls.clone();
        Box::pin(async move {
            *calls.lock().unwrap() += 1;
            Ok(ToolCallResult {
                status: ToolCallStatus::Unknown,
                result_json: None,
                message: Some("external query pending".to_owned()),
            })
        })
    }

    fn check<'a>(
        &'a self,
        _context: &'a RequestContext,
        _request: &'a ToolCallRequest,
    ) -> BoxFuture<'a, Result<Option<ToolCallResult>, ProviderError>> {
        let checks = self.checks.clone();
        Box::pin(async move {
            let mut checks = checks.lock().unwrap();
            *checks += 1;
            Ok((*checks > 1).then(|| ToolCallResult {
                status: ToolCallStatus::Confirmed,
                result_json: Some("{\"available\":8}".to_owned()),
                message: None,
            }))
        })
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

struct Factory {
    bundle: ResolvedMessageBundle,
}
impl MessageBundleFactory for Factory {
    fn resolve<'a>(
        &'a self,
        _context: &'a RequestContext,
        _revision: &'a AgentRevision,
        _input: &'a str,
    ) -> BoxFuture<'a, Result<ResolvedMessageBundle, ProviderError>> {
        Box::pin(async move { Ok(self.bundle.clone()) })
    }
}

fn resolved_bundle() -> ResolvedMessageBundle {
    let revision: AgentRevision = serde_json::from_str(include_str!(
        "../../../abar-api/fixtures/core/agent-revision.json"
    ))
    .unwrap();
    let behavior = BehaviorContent {
        system_prompt: "system".to_owned(),
        skills: ["decision-guidance", "approval-guidance"]
            .into_iter()
            .enumerate()
            .map(|(index, name)| SkillContent {
                metadata: SkillMetadata {
                    name: name.to_owned(),
                    description: name.to_owned(),
                    disable_model_invocation: false,
                    version: Some(if index == 0 { "11" } else { "7" }.to_owned()),
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
        version: "1.0.0".to_owned(),
        description: id.to_owned(),
        input_schema_json: "{}".to_owned(),
        output_schema_json: "{}".to_owned(),
        effect,
        schema_digest: format!("sha256:{}", "1".repeat(64)),
    })
    .collect::<Vec<_>>();
    let knowledge = [KnowledgeDocument {
        source: "policy".to_owned(),
        document_id: "doc-12".to_owned(),
        version: "2026-11-17".to_owned(),
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
        model: Arc::new(Model {
            calls: Arc::new(Mutex::new(0)),
        }),
        tools: Arc::new(Tools),
        data_policy: Arc::new(Policy),
    }
}

#[tokio::test]
async fn pauses_and_resumes_after_approval() {
    let store = SessionStore::new(tempfile::tempdir().unwrap().path());
    let session = store.create("acme", "example-assistant", "alice").unwrap();
    let revision: AgentRevision = serde_json::from_str(include_str!(
        "../../../abar-api/fixtures/core/agent-revision.json"
    ))
    .unwrap();
    let factory = Factory {
        bundle: resolved_bundle(),
    };
    let context = RequestContext::new("req", TenantContext::new("acme"));
    let paused = start_message(
        &store,
        &context,
        &session.id,
        &revision,
        "perform operation",
        &[],
        &factory,
        None,
    )
    .await
    .unwrap();
    assert_eq!(paused.state, MessageState::WaitingApproval);
    assert!(paused.pending_approval.is_some());
    assert_eq!(paused.model_usage.total_tokens, 12);
    let approved = store
        .submit_approval(
            &session.id,
            &paused.id,
            MessageApproval {
                approval_id: paused
                    .pending_approval
                    .as_ref()
                    .unwrap()
                    .approval_id
                    .clone(),
                actor: "manager".to_owned(),
                scopes: vec!["east".to_owned()],
                decision: "approved".to_owned(),
                comment: None,
                reviewed_at: "2026-11-17T09:01:00Z".to_owned(),
            },
        )
        .unwrap();
    assert_eq!(approved.state, MessageState::Running);
    let completed = resume_message(
        &store,
        &context,
        &session.id,
        &paused.id,
        &revision,
        &factory,
        None,
    )
    .await
    .unwrap();
    assert_eq!(completed.state, MessageState::Completed);
    assert_eq!(completed.output.as_deref(), Some("operation completed"));
    assert_eq!(completed.model_usage.total_tokens, 20);
    let events = store.list_events(&session.id, &paused.id, 0).unwrap();
    let event_types = events
        .iter()
        .map(|event| event.event_type.as_str())
        .collect::<Vec<_>>();
    assert!(event_types.contains(&"approval.requested"));
    assert!(event_types.contains(&"message.resumed"));
    assert!(event_types.contains(&"message.completed"));
}

#[tokio::test]
async fn queries_the_exact_external_read_before_continuing() {
    let store = SessionStore::new(tempfile::tempdir().unwrap().path());
    let session = store.create("acme", "example-assistant", "alice").unwrap();
    let revision: AgentRevision = serde_json::from_str(include_str!(
        "../../../abar-api/fixtures/core/agent-revision.json"
    ))
    .unwrap();
    let checks = Arc::new(Mutex::new(0));
    let calls = Arc::new(Mutex::new(0));
    let mut bundle = resolved_bundle();
    bundle.model = Arc::new(ExternalReadModel {
        calls: Arc::new(Mutex::new(0)),
    });
    bundle.tools = Arc::new(ExternalReadTools {
        checks: checks.clone(),
        calls: calls.clone(),
    });
    let factory = Factory { bundle };
    let context = RequestContext::new("req", TenantContext::new("acme"));
    let waiting = start_message(
        &store,
        &context,
        &session.id,
        &revision,
        "query catalog",
        &[],
        &factory,
        None,
    )
    .await
    .unwrap();
    assert_eq!(waiting.state, MessageState::WaitingExternal);
    assert_eq!(
        waiting.pending_external.as_ref().unwrap().tool_id,
        "catalog.query-example"
    );
    assert_eq!(*calls.lock().unwrap(), 1);

    let completed = reconcile_message(
        &store,
        &context,
        &session.id,
        &waiting.id,
        &revision,
        &factory,
        None,
    )
    .await
    .unwrap();
    assert_eq!(completed.state, MessageState::Completed);
    assert_eq!(*checks.lock().unwrap(), 2);
    assert_eq!(*calls.lock().unwrap(), 1);
    assert_eq!(completed.tool_results[0].tool_id, "catalog.query-example");
}

#[tokio::test]
async fn cancellation_and_restart_resume_are_durable() {
    let store = SessionStore::new(tempfile::tempdir().unwrap().path());
    let session = store.create("acme", "example-assistant", "alice").unwrap();
    let revision: AgentRevision = serde_json::from_str(include_str!(
        "../../../abar-api/fixtures/core/agent-revision.json"
    ))
    .unwrap();
    let factory = Factory {
        bundle: resolved_bundle(),
    };
    let context = RequestContext::new("req", TenantContext::new("acme"));
    let cancelled = start_message(
        &store,
        &context,
        &session.id,
        &revision,
        "perform operation",
        &[],
        &factory,
        Some(Arc::new(AtomicBool::new(true))),
    )
    .await
    .unwrap();
    assert_eq!(cancelled.state, MessageState::Cancelled);

    let second = store.create("acme", "example-assistant", "alice").unwrap();
    let prepared = prepare_message(
        &store,
        &context,
        &second.id,
        &revision,
        "perform operation",
        &[],
        &factory,
    )
    .await
    .unwrap();
    store
        .transition_message(&second.id, &prepared.id, MessageState::Running)
        .unwrap();
    let completed = execute_prepared_message(
        &store,
        &context,
        &second.id,
        &prepared.id,
        &revision,
        &factory,
        None,
    )
    .await
    .unwrap();
    assert!(matches!(
        completed.state,
        MessageState::WaitingApproval | MessageState::Completed
    ));
}
