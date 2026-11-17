#![forbid(unsafe_code)]

//! OIDC UserInfo identity and role-based approval authorization.

use abar_api::context::TenantContext;
use abar_api::policy::{AuthContext, AuthProvider};
use abar_api::provider::{BoxFuture, ProviderError};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::Mutex;

mod approval;

pub use approval::RoleApprovalAuthorizer;

/// OIDC UserInfo endpoint configuration.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OidcUserInfoConfig {
    pub user_info_endpoint: String,
    #[serde(default = "default_subject_claim")]
    pub subject_claim: String,
    #[serde(default = "default_tenant_claim")]
    pub tenant_claim: String,
    #[serde(default = "default_roles_claim")]
    pub roles_claim: String,
    #[serde(default)]
    pub allow_http: bool,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    #[serde(default)]
    pub issuer: Option<String>,
    #[serde(default)]
    pub audience: Option<String>,
    #[serde(default)]
    pub introspection_endpoint: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_secret: Option<String>,
    #[serde(default = "default_cache_seconds")]
    pub cache_seconds: u64,
}

impl OidcUserInfoConfig {
    /// Validates transport and claim configuration.
    pub fn validate(&self) -> Result<(), String> {
        if !self.user_info_endpoint.starts_with("https://") && !self.allow_http {
            return Err("OIDC UserInfo endpoint must use HTTPS".to_owned());
        }
        if !(self.user_info_endpoint.starts_with("https://")
            || self.user_info_endpoint.starts_with("http://"))
        {
            return Err("OIDC UserInfo endpoint must use HTTP or HTTPS".to_owned());
        }
        if self.subject_claim.trim().is_empty()
            || self.tenant_claim.trim().is_empty()
            || self.roles_claim.trim().is_empty()
        {
            return Err("OIDC claim names must not be empty".to_owned());
        }
        if self.timeout_seconds == 0 {
            return Err("OIDC timeoutSeconds must be positive".to_owned());
        }
        if self.cache_seconds > 300 {
            return Err("OIDC cacheSeconds must not exceed 300".to_owned());
        }
        self.introspection_config()?;
        Ok(())
    }

    fn introspection_config(&self) -> Result<Option<OidcIntrospectionConfig>, String> {
        let configured = self.issuer.is_some()
            || self.audience.is_some()
            || self.introspection_endpoint.is_some()
            || self.client_id.is_some()
            || self.client_secret.is_some();
        if !configured {
            return Ok(None);
        }
        let issuer = non_empty(self.issuer.as_deref(), "OIDC issuer")?;
        let audience = non_empty(self.audience.as_deref(), "OIDC audience")?;
        let endpoint = non_empty(
            self.introspection_endpoint.as_deref(),
            "OIDC introspection endpoint",
        )?;
        let client_id = non_empty(self.client_id.as_deref(), "OIDC clientId")?;
        let client_secret = non_empty(self.client_secret.as_deref(), "OIDC clientSecret")?;
        validate_http_endpoint(&issuer, self.allow_http, "OIDC issuer")?;
        validate_http_endpoint(&endpoint, self.allow_http, "OIDC introspection endpoint")?;
        Ok(Some(OidcIntrospectionConfig {
            issuer,
            audience,
            endpoint,
            client_id,
            client_secret,
        }))
    }
}

impl fmt::Debug for OidcUserInfoConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OidcUserInfoConfig")
            .field("user_info_endpoint", &self.user_info_endpoint)
            .field("subject_claim", &self.subject_claim)
            .field("tenant_claim", &self.tenant_claim)
            .field("roles_claim", &self.roles_claim)
            .field("allow_http", &self.allow_http)
            .field("timeout_seconds", &self.timeout_seconds)
            .field("issuer", &self.issuer)
            .field("audience", &self.audience)
            .field("introspection_endpoint", &self.introspection_endpoint)
            .field("client_id", &self.client_id)
            .field("cache_seconds", &self.cache_seconds)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

/// OIDC UserInfo-backed bearer authentication.
#[derive(Clone)]
pub struct OidcUserInfoAuthProvider {
    endpoint: String,
    subject_claim: String,
    tenant_claim: String,
    roles_claim: String,
    client: reqwest::Client,
    introspection: Option<OidcIntrospectionConfig>,
    cache_seconds: u64,
    cache: Arc<RwLock<HashMap<String, CachedAuth>>>,
    refresh_lock: Arc<Mutex<()>>,
}

impl OidcUserInfoAuthProvider {
    /// Creates a provider after validating configuration.
    pub fn new(config: OidcUserInfoConfig) -> Result<Self, String> {
        config.validate()?;
        let introspection = config.introspection_config()?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()
            .map_err(|error| error.to_string())?;
        Ok(Self {
            endpoint: config.user_info_endpoint,
            subject_claim: config.subject_claim,
            tenant_claim: config.tenant_claim,
            roles_claim: config.roles_claim,
            client,
            introspection,
            cache_seconds: config.cache_seconds,
            cache: Arc::new(RwLock::new(HashMap::new())),
            refresh_lock: Arc::new(Mutex::new(())),
        })
    }
}

impl fmt::Debug for OidcUserInfoAuthProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OidcUserInfoAuthProvider")
            .field("endpoint", &self.endpoint)
            .field("subject_claim", &self.subject_claim)
            .field("tenant_claim", &self.tenant_claim)
            .field("roles_claim", &self.roles_claim)
            .field("introspection", &self.introspection)
            .field("cache_seconds", &self.cache_seconds)
            .finish_non_exhaustive()
    }
}

impl AuthProvider for OidcUserInfoAuthProvider {
    fn authenticate<'a>(
        &'a self,
        token: &'a str,
    ) -> BoxFuture<'a, Result<AuthContext, ProviderError>> {
        Box::pin(async move {
            let key = token_hash(token);
            if let Some(auth) = self.cached_auth(&key) {
                return Ok(auth);
            }
            let refresh_guard = if self.cache_seconds > 0 {
                Some(self.refresh_lock.lock().await)
            } else {
                None
            };
            if let Some(auth) = self.cached_auth(&key) {
                return Ok(auth);
            }
            let token_expires_at = if let Some(config) = &self.introspection {
                self.validate_introspection(token, config).await?
            } else {
                unix_now().saturating_add(self.cache_seconds)
            };
            let response = self
                .client
                .get(&self.endpoint)
                .bearer_auth(token)
                .header(reqwest::header::ACCEPT, "application/json")
                .send()
                .await
                .map_err(|error| provider_error(error.to_string()))?;
            if !response.status().is_success() {
                return Err(provider_error(format!(
                    "OIDC UserInfo returned {}",
                    response.status()
                )));
            }
            let claims: Value = response
                .json()
                .await
                .map_err(|error| provider_error(error.to_string()))?;
            let subject = string_claim(&claims, &self.subject_claim)?;
            let tenant = string_claim(&claims, &self.tenant_claim)?;
            let roles = roles_claim(&claims, &self.roles_claim)?;
            let auth = AuthContext {
                roles,
                ..AuthContext::new(subject, TenantContext::new(tenant))
            };
            if self.cache_seconds > 0 {
                let expires_at = unix_now()
                    .saturating_add(self.cache_seconds)
                    .min(token_expires_at);
                if let Ok(mut cache) = self.cache.write() {
                    cache.retain(|_, value| value.expires_at > unix_now());
                    cache.insert(
                        key,
                        CachedAuth {
                            auth: auth.clone(),
                            expires_at,
                        },
                    );
                }
            }
            drop(refresh_guard);
            Ok(auth)
        })
    }
}

impl OidcUserInfoAuthProvider {
    fn cached_auth(&self, key: &str) -> Option<AuthContext> {
        if self.cache_seconds == 0 {
            return None;
        }
        let cache = self.cache.read().ok()?;
        let value = cache.get(key)?;
        (value.expires_at > unix_now()).then(|| value.auth.clone())
    }

    async fn validate_introspection(
        &self,
        token: &str,
        config: &OidcIntrospectionConfig,
    ) -> Result<u64, ProviderError> {
        let response = self
            .client
            .post(&config.endpoint)
            .basic_auth(&config.client_id, Some(&config.client_secret))
            .form(&[("token", token), ("token_type_hint", "access_token")])
            .send()
            .await
            .map_err(|error| provider_error(error.to_string()))?;
        if !response.status().is_success() {
            return Err(provider_error(format!(
                "OIDC introspection returned {}",
                response.status()
            )));
        }
        let claims: Value = response
            .json()
            .await
            .map_err(|error| provider_error(error.to_string()))?;
        if claims.get("active").and_then(Value::as_bool) != Some(true) {
            return Err(provider_error("OIDC access token is not active"));
        }
        let issuer = claims
            .get("iss")
            .and_then(Value::as_str)
            .ok_or_else(|| provider_error("OIDC introspection claim iss is missing"))?;
        if issuer != config.issuer {
            return Err(provider_error("OIDC access token issuer does not match"));
        }
        let audience_matches = match claims.get("aud") {
            Some(Value::String(audience)) => audience == &config.audience,
            Some(Value::Array(audiences)) => audiences
                .iter()
                .any(|audience| audience.as_str() == Some(&config.audience)),
            _ => false,
        };
        if !audience_matches {
            return Err(provider_error("OIDC access token audience does not match"));
        }
        let expires_at = claims
            .get("exp")
            .and_then(Value::as_u64)
            .ok_or_else(|| provider_error("OIDC introspection claim exp is missing"))?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| provider_error(error.to_string()))?
            .as_secs();
        if expires_at <= now {
            return Err(provider_error("OIDC access token has expired"));
        }
        Ok(expires_at)
    }
}

#[derive(Clone)]
struct CachedAuth {
    auth: AuthContext,
    expires_at: u64,
}

#[derive(Clone)]
struct OidcIntrospectionConfig {
    issuer: String,
    audience: String,
    endpoint: String,
    client_id: String,
    client_secret: String,
}

impl fmt::Debug for OidcIntrospectionConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OidcIntrospectionConfig")
            .field("issuer", &self.issuer)
            .field("audience", &self.audience)
            .field("endpoint", &self.endpoint)
            .field("client_id", &self.client_id)
            .field("client_secret", &"<redacted>")
            .finish()
    }
}

fn string_claim(claims: &Value, name: &str) -> Result<String, ProviderError> {
    claims
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| provider_error(format!("OIDC claim {name} is missing")))
}

fn roles_claim(claims: &Value, name: &str) -> Result<Vec<String>, ProviderError> {
    let value = claims
        .get(name)
        .ok_or_else(|| provider_error(format!("OIDC claim {name} is missing")))?;
    match value {
        Value::String(role) => Ok(vec![role.clone()]),
        Value::Array(values) => values
            .iter()
            .map(|value| {
                value.as_str().map(str::to_owned).ok_or_else(|| {
                    provider_error(format!("OIDC claim {name} must contain strings"))
                })
            })
            .collect(),
        _ => Err(provider_error(format!(
            "OIDC claim {name} must be a string or string array"
        ))),
    }
}

pub(crate) fn provider_error(message: impl Into<String>) -> ProviderError {
    ProviderError::new(abar_api::error::ErrorCode::InvalidRequest, message, false)
}

fn token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn non_empty(value: Option<&str>, name: &str) -> Result<String, String> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("{name} must not be empty"))
}

fn validate_http_endpoint(value: &str, allow_http: bool, name: &str) -> Result<(), String> {
    if !value.starts_with("https://") && !(allow_http && value.starts_with("http://")) {
        return Err(format!("{name} must use HTTPS"));
    }
    Ok(())
}

fn default_subject_claim() -> String {
    "sub".to_owned()
}

fn default_tenant_claim() -> String {
    "tenant".to_owned()
}

fn default_roles_claim() -> String {
    "roles".to_owned()
}

const fn default_timeout() -> u64 {
    5
}

const fn default_cache_seconds() -> u64 {
    30
}

#[cfg(test)]
mod tests;
