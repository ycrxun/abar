mod data_policy;
mod knowledge;
mod message_config;

use crate::support::default_revision_store;
use abar_api::context::RequestContext;
use abar_api::data::DataPolicy;
use abar_api::knowledge::KnowledgeProvider;
use abar_api::provider::{BoxFuture, ModelProvider, ProviderError, ToolRegistry};
use abar_api::revision::AgentRevision;
use abar_core::config::load_model_config;
use abar_core::message::{MessageBundleFactory, RevisionMessageBundleFactory};
use abar_core::revision::RevisionStore;
use abar_core::revision_content::RevisionContentProvider;
use abar_core::server::RuntimeProviderFactory;
use abar_model_openai::OpenAiCompatibleProvider;
use abar_tool_mcp_http::{McpHttpToolRegistry, McpServerConfig};
use std::sync::Arc;

pub(crate) struct DefaultProviderFactory;

impl RuntimeProviderFactory for DefaultProviderFactory {
    fn message_bundle<'a>(
        &'a self,
        context: &'a RequestContext,
        revision: &'a AgentRevision,
        input: &'a str,
    ) -> BoxFuture<'a, Result<abar_core::message::ResolvedMessageBundle, ProviderError>> {
        let result = self.build_message_factory();
        Box::pin(async move {
            let factory = result.map_err(|message| {
                ProviderError::new(abar_api::error::ErrorCode::InvalidRequest, message, false)
            })?;
            factory.resolve(context, revision, input).await
        })
    }

    fn validate_message_runtime(&self) -> Result<(), String> {
        self.build_message_factory().map(|_| ())
    }
}

impl DefaultProviderFactory {
    fn build_message_factory(&self) -> Result<RevisionMessageBundleFactory, String> {
        let config = message_config::load(None)?;
        let model_config =
            load_model_config(&config.model_ref).map_err(|error| error.to_string())?;
        let model_id = model_config.model_id.clone();
        let model = build_model(model_config)?;
        validate_mcp_configuration(&config.tools.mcp_servers)?;
        let tools: Arc<dyn ToolRegistry> =
            Arc::new(McpHttpToolRegistry::new(config.tools.mcp_servers.clone()));
        let knowledge: Arc<dyn KnowledgeProvider> = Arc::new(
            knowledge::FileKnowledgeProvider::new(config.knowledge.sources),
        );
        let data_policy: Arc<dyn DataPolicy> = Arc::new(data_policy::ConfiguredDataPolicy::new(
            config.data_policy.fields,
        ));
        let content = Arc::new(RevisionContentProvider::new(RevisionStore::new(
            default_revision_store(),
        )));
        Ok(RevisionMessageBundleFactory {
            content,
            tools,
            knowledge,
            data_policy,
            model,
            model_id,
            model_version: config.model_version,
        })
    }
}

fn validate_mcp_configuration(servers: &[McpServerConfig]) -> Result<(), String> {
    for server in servers {
        let has_write = server
            .tools
            .values()
            .any(|effect| *effect == abar_api::provider::ToolEffect::Write);
        if has_write && server.idempotency_check_tool.is_none() {
            return Err(format!(
                "MCP server {} with write Tools requires idempotencyCheckTool",
                server.name
            ));
        }
        if server
            .idempotency_check_tool
            .as_ref()
            .is_some_and(|tool| server.tools.contains_key(tool))
        {
            return Err(format!(
                "MCP server {} exposes its idempotency check Tool to the model",
                server.name
            ));
        }
    }
    Ok(())
}

fn build_model(
    config: abar_core::config::ModelRuntimeConfig,
) -> Result<Arc<dyn ModelProvider>, String> {
    match config.provider.as_str() {
        "openai-compatible" => {
            let api_key = config
                .auth_ref
                .as_deref()
                .and_then(|auth_ref| auth_ref.strip_prefix("env://"))
                .and_then(|variable| std::env::var(variable).ok());
            Ok(Arc::new(
                OpenAiCompatibleProvider::with_timeout(
                    config.endpoint,
                    api_key,
                    config.model_id,
                    config.timeout_seconds.unwrap_or(120),
                )
                .with_parameters(config.parameters),
            ))
        }
        provider => Err(format!("unsupported model provider: {provider}")),
    }
}

#[cfg(test)]
mod tests {
    use super::validate_mcp_configuration;
    use abar_tool_mcp_http::McpServerConfig;
    use std::collections::BTreeMap;

    #[test]
    fn write_server_requires_model_invisible_idempotency_lookup() {
        let mut server = McpServerConfig {
            name: "service".to_owned(),
            endpoint: "http://127.0.0.1:1/mcp".to_owned(),
            auth_ref: None,
            idempotency_check_tool: None,
            tools: BTreeMap::from([("write".to_owned(), abar_api::provider::ToolEffect::Write)]),
        };
        assert!(validate_mcp_configuration(std::slice::from_ref(&server)).is_err());
        server.idempotency_check_tool = Some("check".to_owned());
        assert!(validate_mcp_configuration(std::slice::from_ref(&server)).is_ok());
        server
            .tools
            .insert("check".to_owned(), abar_api::provider::ToolEffect::Read);
        assert!(validate_mcp_configuration(&[server]).is_err());
    }
}
