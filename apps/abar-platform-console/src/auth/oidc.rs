use super::cookie::{percent_encode, pkce_challenge, random_token};
use serde::Deserialize;
use serde_json::Value;
use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub(super) struct OidcClient {
    issuer: String,
    client_id: String,
    client_secret: Option<String>,
    redirect_uri: String,
    scopes: String,
    discovery: Discovery,
    client: reqwest::Client,
}

impl fmt::Debug for OidcClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OidcClient")
            .field("issuer", &self.issuer)
            .field("client_id", &self.client_id)
            .field("redirect_uri", &self.redirect_uri)
            .field("scopes", &self.scopes)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "<redacted>"),
            )
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Deserialize)]
struct Discovery {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    userinfo_endpoint: String,
    #[serde(default)]
    end_session_endpoint: Option<String>,
}

#[derive(Debug)]
pub(super) struct LoginStart {
    pub(super) url: String,
    pub(super) state: String,
    pub(super) verifier: String,
}

#[derive(Clone, Debug)]
pub(super) struct UserIdentity {
    pub(super) subject_id: String,
    pub(super) display_name: String,
    pub(super) tenant: String,
    pub(super) roles: Vec<String>,
}

#[derive(Debug)]
pub(super) struct TokenSet {
    pub(super) access_token: String,
    pub(super) refresh_token: Option<String>,
    pub(super) id_token: Option<String>,
    pub(super) expires_at: u64,
}

impl OidcClient {
    pub(super) async fn from_env(
        address: &str,
    ) -> Result<Option<Self>, Box<dyn std::error::Error>> {
        let Some(issuer) = env_optional("ABAR_PLATFORM_OIDC_ISSUER") else {
            return Ok(None);
        };
        let client_id = required_env("ABAR_PLATFORM_OIDC_CLIENT_ID")?;
        let allow_http = env_bool("ABAR_PLATFORM_OIDC_ALLOW_HTTP");
        validate_endpoint(&issuer, allow_http, "issuer")?;
        let redirect_uri = env_optional("ABAR_PLATFORM_OIDC_REDIRECT_URI")
            .unwrap_or_else(|| format!("http://{address}/auth/callback"));
        validate_endpoint(&redirect_uri, allow_http, "redirect URI")?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;
        let discovery_url = format!(
            "{}/.well-known/openid-configuration",
            issuer.trim_end_matches('/')
        );
        let discovery: Discovery = client
            .get(&discovery_url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        if discovery.issuer.trim_end_matches('/') != issuer.trim_end_matches('/') {
            return Err("OIDC discovery issuer does not match configured issuer".into());
        }
        validate_endpoint(
            &discovery.authorization_endpoint,
            allow_http,
            "authorization endpoint",
        )?;
        validate_endpoint(&discovery.token_endpoint, allow_http, "token endpoint")?;
        validate_endpoint(
            &discovery.userinfo_endpoint,
            allow_http,
            "UserInfo endpoint",
        )?;
        if let Some(endpoint) = &discovery.end_session_endpoint {
            validate_endpoint(endpoint, allow_http, "end-session endpoint")?;
        }
        Ok(Some(Self {
            issuer,
            client_id,
            client_secret: env_optional("ABAR_PLATFORM_OIDC_CLIENT_SECRET"),
            redirect_uri,
            scopes: std::env::var("ABAR_PLATFORM_OIDC_SCOPES")
                .unwrap_or_else(|_| "openid profile email".to_owned()),
            discovery,
            client,
        }))
    }

    pub(super) fn secure_cookie(&self) -> bool {
        !self.discovery.issuer.starts_with("http://")
    }

    pub(super) fn begin(&self) -> LoginStart {
        let state = random_token();
        let verifier = random_token();
        let challenge = pkce_challenge(&verifier);
        let separator = if self.discovery.authorization_endpoint.contains('?') {
            '&'
        } else {
            '?'
        };
        let url = format!(
            "{}{}response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method=S256",
            self.discovery.authorization_endpoint,
            separator,
            percent_encode(&self.client_id),
            percent_encode(&self.redirect_uri),
            percent_encode(&self.scopes),
            percent_encode(&state),
            percent_encode(&challenge),
        );
        LoginStart {
            url,
            state,
            verifier,
        }
    }

    pub(super) async fn exchange(&self, code: &str, verifier: &str) -> Result<TokenSet, String> {
        let mut form = vec![
            ("grant_type", "authorization_code".to_owned()),
            ("code", code.to_owned()),
            ("redirect_uri", self.redirect_uri.clone()),
            ("client_id", self.client_id.clone()),
            ("code_verifier", verifier.to_owned()),
        ];
        if let Some(secret) = &self.client_secret {
            form.push(("client_secret", secret.clone()));
        }
        self.token_request(form, None, None).await
    }

    pub(super) async fn refresh(
        &self,
        refresh_token: &str,
        id_token: Option<&str>,
    ) -> Result<TokenSet, String> {
        let mut form = vec![
            ("grant_type", "refresh_token".to_owned()),
            ("refresh_token", refresh_token.to_owned()),
            ("client_id", self.client_id.clone()),
        ];
        if let Some(secret) = &self.client_secret {
            form.push(("client_secret", secret.clone()));
        }
        self.token_request(
            form,
            Some(refresh_token.to_owned()),
            id_token.map(str::to_owned),
        )
        .await
    }

    async fn token_request(
        &self,
        form: Vec<(&str, String)>,
        prior_refresh_token: Option<String>,
        prior_id_token: Option<String>,
    ) -> Result<TokenSet, String> {
        let response = self
            .client
            .post(&self.discovery.token_endpoint)
            .form(&form)
            .send()
            .await
            .map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Err(format!("token endpoint returned {}", response.status()));
        }
        let value: Value = response.json().await.map_err(|error| error.to_string())?;
        let access_token = value
            .get("access_token")
            .and_then(Value::as_str)
            .ok_or("token endpoint returned no access_token")?
            .to_owned();
        let expires_in = value
            .get("expires_in")
            .and_then(Value::as_u64)
            .ok_or("token endpoint returned no expires_in")?;
        Ok(TokenSet {
            access_token,
            refresh_token: value
                .get("refresh_token")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or(prior_refresh_token),
            id_token: value
                .get("id_token")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or(prior_id_token),
            expires_at: unix_now().saturating_add(expires_in),
        })
    }

    pub(super) async fn user_info(&self, token: &str) -> Result<UserIdentity, String> {
        let response = self
            .client
            .get(&self.discovery.userinfo_endpoint)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Err(format!("UserInfo returned {}", response.status()));
        }
        let claims: Value = response.json().await.map_err(|error| error.to_string())?;
        let subject_id = required_claim(&claims, "sub")?;
        let display_name = claims
            .get("name")
            .or_else(|| claims.get("preferred_username"))
            .and_then(Value::as_str)
            .unwrap_or(&subject_id)
            .to_owned();
        let tenant = required_claim(&claims, "tenant")?;
        let roles = string_list(claims.get("roles"));
        Ok(UserIdentity {
            subject_id,
            display_name,
            tenant,
            roles,
        })
    }

    pub(super) fn logout_url(&self, id_token: Option<&str>) -> Option<String> {
        let endpoint = self.discovery.end_session_endpoint.as_ref()?;
        let separator = if endpoint.contains('?') { '&' } else { '?' };
        let post_logout_redirect =
            format!("{}/", self.redirect_uri.trim_end_matches("/auth/callback"));
        let mut url = format!(
            "{endpoint}{separator}client_id={}&post_logout_redirect_uri={}",
            percent_encode(&self.client_id),
            percent_encode(&post_logout_redirect),
        );
        if let Some(id_token) = id_token {
            url.push_str("&id_token_hint=");
            url.push_str(&percent_encode(id_token));
        }
        Some(url)
    }

    pub(super) fn issuer(&self) -> &str {
        &self.issuer
    }
}

fn required_claim(claims: &Value, name: &str) -> Result<String, String> {
    claims
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("UserInfo claim {name} is missing"))
}

fn string_list(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::String(value)) => vec![value.clone()],
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

fn required_env(name: &str) -> Result<String, Box<dyn std::error::Error>> {
    env_optional(name).ok_or_else(|| format!("{name} is required").into())
}

fn env_optional(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn env_bool(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes"))
}

fn validate_endpoint(value: &str, allow_http: bool, name: &str) -> Result<(), String> {
    if !value.starts_with("https://") && !(allow_http && value.starts_with("http://")) {
        return Err(format!("OIDC {name} must use HTTPS"));
    }
    Ok(())
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}
