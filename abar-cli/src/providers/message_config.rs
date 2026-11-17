use abar_tool_mcp_http::McpServerConfig;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Local provider configuration for Message Runtime.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct MessageRuntimeConfig {
    pub model_ref: String,
    pub model_version: String,
    #[serde(default)]
    pub tools: ToolsConfig,
    #[serde(default)]
    pub knowledge: KnowledgeConfig,
    #[serde(default)]
    pub data_policy: DataPolicyConfig,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ToolsConfig {
    #[serde(default)]
    pub mcp_servers: Vec<McpServerConfig>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct KnowledgeConfig {
    #[serde(default)]
    pub sources: BTreeMap<String, FileKnowledgeConfig>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct FileKnowledgeConfig {
    pub path: PathBuf,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DataPolicyConfig {
    #[serde(default)]
    pub fields: Vec<DataFieldRule>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DataFieldRule {
    pub path: String,
    pub data_class: String,
}

pub(super) fn load(path: Option<&Path>) -> Result<MessageRuntimeConfig, String> {
    let path = path.map_or_else(default_path, Path::to_path_buf);
    let mut config: MessageRuntimeConfig = serde_yaml::from_reader(
        fs::File::open(&path).map_err(|error| format!("{}: {error}", path.display()))?,
    )
    .map_err(|error| format!("{}: {error}", path.display()))?;
    if config.model_ref.trim().is_empty() || config.model_version.trim().is_empty() {
        return Err("modelRef and modelVersion are required".to_owned());
    }
    if let Some(parent) = path.parent() {
        for source in config.knowledge.sources.values_mut() {
            if source.path.is_relative() {
                source.path = parent.join(&source.path);
            }
        }
    }
    Ok(config)
}

fn default_path() -> PathBuf {
    if let Ok(path) = std::env::var("ABAR_MESSAGE_CONFIG") {
        return PathBuf::from(path);
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".abar")
        .join("message.yaml")
}
