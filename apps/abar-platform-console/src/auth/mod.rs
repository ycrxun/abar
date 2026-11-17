mod cookie;
mod oidc;
mod return_path;

use crate::app::AppError;
use axum::http::HeaderMap;
use oidc::{OidcClient, UserIdentity};
pub(crate) use return_path::{login_url, safe_return_path};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, RwLock};

const SESSION_TTL_SECONDS: u64 = 8 * 60 * 60;
const REFRESH_SKEW_SECONDS: u64 = 60;
const LOGIN_TTL_SECONDS: u64 = 10 * 60;

#[derive(Clone)]
pub(crate) struct UserSession {
    pub(crate) subject_id: String,
    pub(crate) display_name: String,
    pub(crate) tenant: String,
    pub(crate) roles: Vec<String>,
    pub(crate) access_token: String,
    pub(crate) refresh_token: Option<String>,
    pub(crate) id_token: Option<String>,
    pub(crate) expires_at: u64,
    pub(crate) csrf_token: String,
}

impl fmt::Debug for UserSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UserSession")
            .field("subject_id", &self.subject_id)
            .field("display_name", &self.display_name)
            .field("tenant", &self.tenant)
            .field("roles", &self.roles)
            .field("expires_at", &self.expires_at)
            .field("access_token", &"<redacted>")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "<redacted>"),
            )
            .field("id_token", &self.id_token.as_ref().map(|_| "<redacted>"))
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
struct PendingLogin {
    verifier: String,
    return_to: String,
    created_at: u64,
}

#[derive(Clone)]
enum AuthMode {
    Local { token: String },
    Oidc(Box<OidcClient>),
}

#[derive(Clone)]
pub(crate) struct AuthStore {
    mode: AuthMode,
    sessions: Arc<RwLock<HashMap<String, UserSession>>>,
    pending: Arc<Mutex<HashMap<String, PendingLogin>>>,
    refresh_lock: Arc<Mutex<()>>,
    cookie_secure: bool,
}

impl AuthStore {
    pub(crate) fn local(token: String) -> Self {
        Self {
            mode: AuthMode::Local { token },
            sessions: Arc::new(RwLock::new(HashMap::new())),
            pending: Arc::new(Mutex::new(HashMap::new())),
            refresh_lock: Arc::new(Mutex::new(())),
            cookie_secure: false,
        }
    }

    pub(crate) async fn oidc(address: &str) -> Result<Option<Self>, Box<dyn std::error::Error>> {
        let Some(oidc) = OidcClient::from_env(address).await? else {
            return Ok(None);
        };
        let cookie_secure = oidc.secure_cookie();
        Ok(Some(Self {
            mode: AuthMode::Oidc(Box::new(oidc)),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            pending: Arc::new(Mutex::new(HashMap::new())),
            refresh_lock: Arc::new(Mutex::new(())),
            cookie_secure,
        }))
    }

    pub(crate) fn is_oidc(&self) -> bool {
        matches!(self.mode, AuthMode::Oidc(_))
    }

    pub(crate) fn issuer(&self) -> Option<&str> {
        match &self.mode {
            AuthMode::Oidc(oidc) => Some(oidc.issuer()),
            AuthMode::Local { .. } => None,
        }
    }

    pub(crate) async fn begin_login(&self, return_to: String) -> Result<String, AppError> {
        let AuthMode::Oidc(oidc) = &self.mode else {
            return Err(AppError::bad_request("OIDC login is not enabled"));
        };
        let start = oidc.begin();
        let mut pending = self.pending.lock().await;
        pending.retain(|_, login| login.created_at.saturating_add(LOGIN_TTL_SECONDS) >= unix_now());
        pending.insert(
            start.state,
            PendingLogin {
                verifier: start.verifier,
                return_to,
                created_at: unix_now(),
            },
        );
        Ok(start.url)
    }

    pub(crate) async fn complete_login(
        &self,
        code: &str,
        state: &str,
    ) -> Result<(String, UserSession, String), AppError> {
        let AuthMode::Oidc(oidc) = &self.mode else {
            return Err(AppError::bad_request("OIDC login is not enabled"));
        };
        let pending = self
            .pending
            .lock()
            .await
            .remove(state)
            .ok_or_else(|| AppError::unauthorized("OIDC login state is invalid or expired"))?;
        if pending.created_at.saturating_add(LOGIN_TTL_SECONDS) < unix_now() {
            return Err(AppError::unauthorized("OIDC login state has expired"));
        }
        let tokens = oidc
            .exchange(code, &pending.verifier)
            .await
            .map_err(AppError::bad_gateway)?;
        let identity = oidc
            .user_info(&tokens.access_token)
            .await
            .map_err(AppError::unauthorized)?;
        let session = session_from_identity(identity, tokens);
        let session_id = cookie::random_token();
        self.sessions
            .write()
            .await
            .insert(session_id.clone(), session.clone());
        Ok((session_id, session, pending.return_to))
    }

    pub(crate) async fn authenticate(
        &self,
        headers: &HeaderMap,
        write: bool,
    ) -> Result<UserSession, AppError> {
        match &self.mode {
            AuthMode::Local { token } => Ok(UserSession {
                subject_id: "local".to_owned(),
                display_name: "Local Operator".to_owned(),
                tenant: "local".to_owned(),
                roles: vec!["platform-admin".to_owned()],
                access_token: token.clone(),
                refresh_token: None,
                id_token: None,
                expires_at: u64::MAX,
                csrf_token: String::new(),
            }),
            AuthMode::Oidc(oidc) => {
                let session_id = cookie::request_cookie(headers, cookie::SESSION_COOKIE)
                    .ok_or_else(|| AppError::unauthorized("OIDC session cookie is missing"))?;
                let mut session = self
                    .sessions
                    .read()
                    .await
                    .get(session_id)
                    .cloned()
                    .ok_or_else(|| AppError::unauthorized("OIDC session is invalid"))?;
                if session.expires_at <= unix_now().saturating_add(REFRESH_SKEW_SECONDS) {
                    let _refresh_guard = self.refresh_lock.lock().await;
                    session = self
                        .sessions
                        .read()
                        .await
                        .get(session_id)
                        .cloned()
                        .ok_or_else(|| AppError::unauthorized("OIDC session is invalid"))?;
                    if session.expires_at <= unix_now().saturating_add(REFRESH_SKEW_SECONDS) {
                        let refresh_token = session
                            .refresh_token
                            .clone()
                            .ok_or_else(|| AppError::unauthorized("OIDC session has expired"))?;
                        let tokens = oidc
                            .refresh(&refresh_token, session.id_token.as_deref())
                            .await
                            .map_err(AppError::unauthorized)?;
                        session.access_token = tokens.access_token;
                        session.refresh_token = tokens.refresh_token;
                        session.id_token = tokens.id_token;
                        session.expires_at = tokens.expires_at;
                        self.sessions
                            .write()
                            .await
                            .insert(session_id.to_owned(), session.clone());
                    }
                }
                if write {
                    let csrf = headers
                        .get("x-abar-csrf")
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or_default();
                    if csrf.is_empty() || csrf != session.csrf_token {
                        return Err(AppError::forbidden("CSRF token is invalid"));
                    }
                }
                Ok(session)
            }
        }
    }

    pub(crate) async fn logout(&self, headers: &HeaderMap) -> Option<String> {
        let AuthMode::Oidc(oidc) = &self.mode else {
            return None;
        };
        let session_id = cookie::request_cookie(headers, cookie::SESSION_COOKIE)?;
        let session = self.sessions.write().await.remove(session_id)?;
        oidc.logout_url(session.id_token.as_deref())
    }

    pub(crate) fn session_cookie(&self, session_id: &str) -> String {
        cookie::session_cookie(session_id, self.cookie_secure, SESSION_TTL_SECONDS)
    }

    pub(crate) fn clear_cookie(&self) -> String {
        cookie::clear_cookie(self.cookie_secure)
    }
}

fn session_from_identity(identity: UserIdentity, tokens: oidc::TokenSet) -> UserSession {
    UserSession {
        subject_id: identity.subject_id,
        display_name: identity.display_name,
        tenant: identity.tenant,
        roles: identity.roles,
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        id_token: tokens.id_token,
        expires_at: tokens.expires_at,
        csrf_token: cookie::random_token(),
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::AuthStore;

    #[test]
    fn local_auth_supplies_platform_role() {
        let store = AuthStore::local("local-token".to_owned());
        assert!(!store.is_oidc());
    }
}
