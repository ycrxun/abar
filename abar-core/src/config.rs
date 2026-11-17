//! Local model configuration.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use thiserror::Error;

/// Configuration loading failure.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("model reference not found: {0}")]
    ModelNotFound(String),
    #[error("invalid model configuration: {0}")]
    Invalid(String),
}

/// Resolved model configuration.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRuntimeConfig {
    pub provider: String,
    pub model_id: String,
    pub endpoint: String,
    pub auth_ref: Option<String>,
    pub capabilities: Option<Vec<String>>,
    pub provider_contract_version: Option<String>,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
    #[serde(default)]
    pub parameters: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize)]
struct ModelConfigDocument {
    models: BTreeMap<String, ModelRuntimeConfig>,
}

/// Loads a model by reference from `ABAR_CONFIG` or `~/.abar/config.yaml`.
pub fn load_model_config(reference: &str) -> Result<ModelRuntimeConfig, ConfigError> {
    let path = config_path();
    let document: ModelConfigDocument = serde_yaml::from_reader(fs::File::open(&path)?)?;
    let config = document
        .models
        .get(reference)
        .cloned()
        .ok_or_else(|| ConfigError::ModelNotFound(reference.to_owned()))?;
    if !config.endpoint.starts_with("https://") && !config.endpoint.starts_with("http://") {
        return Err(ConfigError::Invalid(format!(
            "model endpoint for {reference} must use http:// or https://"
        )));
    }
    if let Some(auth_ref) = &config.auth_ref
        && !auth_ref.starts_with("env://")
    {
        return Err(ConfigError::Invalid(format!(
            "model authRef for {reference} must use env://"
        )));
    }
    Ok(config)
}

/// Resolves the active local configuration path.
#[must_use]
pub fn config_path() -> PathBuf {
    if let Ok(path) = std::env::var("ABAR_CONFIG") {
        return PathBuf::from(path);
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".abar")
        .join("config.yaml")
}
