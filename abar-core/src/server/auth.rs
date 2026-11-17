//! Local Server authorization.

use super::{ApiError, ServerState};
use abar_api::context::TenantContext;
use abar_api::message::PendingToolCall;
use abar_api::policy::{ApprovalAuthorizer, AuthContext, AuthProvider};
use abar_api::provider::{BoxFuture, ProviderError};
use abar_api::session::Session;
use axum::http::HeaderMap;

const LOCAL_APPROVER_ROLE: &str = "approver";
const LOCAL_PLATFORM_ROLE: &str = "platform-admin";

#[derive(Debug)]
pub(super) struct LocalBearerAuth {
    token: String,
}

impl LocalBearerAuth {
    pub(super) fn new(token: String) -> Self {
        Self { token }
    }
}

impl AuthProvider for LocalBearerAuth {
    fn authenticate<'a>(
        &'a self,
        token: &'a str,
    ) -> BoxFuture<'a, Result<AuthContext, ProviderError>> {
        Box::pin(async move {
            if token == self.token {
                Ok(AuthContext {
                    roles: vec![
                        LOCAL_APPROVER_ROLE.to_owned(),
                        LOCAL_PLATFORM_ROLE.to_owned(),
                    ],
                    ..AuthContext::new("local", TenantContext::new("local"))
                })
            } else {
                Err(ProviderError::new(
                    abar_api::error::ErrorCode::InvalidRequest,
                    "invalid bearer token",
                    false,
                ))
            }
        })
    }
}

#[derive(Debug)]
pub(super) struct LocalApprovalAuthorizer;

impl ApprovalAuthorizer for LocalApprovalAuthorizer {
    fn can_review<'a>(
        &'a self,
        auth: &'a AuthContext,
        session: &'a Session,
        pending: &'a PendingToolCall,
    ) -> BoxFuture<'a, Result<bool, ProviderError>> {
        Box::pin(async move {
            Ok(auth.roles.iter().any(|role| role == LOCAL_APPROVER_ROLE)
                && session.tenant == auth.tenant.tenant_id
                && session.user == auth.subject_id
                && !pending.approval_id.trim().is_empty()
                && !pending.tool_id.trim().is_empty())
        })
    }

    fn authorize<'a>(
        &'a self,
        auth: &'a AuthContext,
        session: &'a Session,
        pending: &'a PendingToolCall,
        decision: &'a str,
        scopes: &'a [String],
    ) -> BoxFuture<'a, Result<(), ProviderError>> {
        Box::pin(async move {
            if !matches!(decision, "approved" | "rejected") {
                return Err(provider_error("approval decision is invalid"));
            }
            if !auth.roles.iter().any(|role| role == LOCAL_APPROVER_ROLE) {
                return Err(provider_error("approver role is required"));
            }
            if session.tenant != auth.tenant.tenant_id || session.user != auth.subject_id {
                return Err(provider_error("approver cannot access this Session"));
            }
            if pending.approval_id.trim().is_empty() || pending.tool_id.trim().is_empty() {
                return Err(provider_error("pending approval evidence is incomplete"));
            }
            if decision == "approved" && scopes.is_empty() {
                return Err(provider_error("approval scope is required"));
            }
            Ok(())
        })
    }
}

fn provider_error(message: impl Into<String>) -> ProviderError {
    ProviderError::new(abar_api::error::ErrorCode::InvalidRequest, message, false)
}

pub(super) async fn authenticate(
    state: &ServerState,
    headers: &HeaderMap,
) -> Result<AuthContext, ApiError> {
    let token = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or_else(ApiError::unauthorized)?;
    state
        .auth
        .authenticate(token)
        .await
        .map_err(|_| ApiError::unauthorized())
}

#[cfg(test)]
mod tests {
    use super::LocalApprovalAuthorizer;
    use abar_api::context::TenantContext;
    use abar_api::message::PendingToolCall;
    use abar_api::policy::{ApprovalAuthorizer, AuthContext};
    use abar_api::session::{Session, SessionStatus};

    #[tokio::test]
    async fn local_approval_requires_role_and_scope() {
        let authorizer = LocalApprovalAuthorizer;
        let session = Session {
            id: "ses-1".to_owned(),
            tenant: "local".to_owned(),
            agent: "example-assistant".to_owned(),
            user: "local".to_owned(),
            status: SessionStatus::Active,
            created_at: "2026-11-17T00:00:00Z".to_owned(),
            last_active_at: "2026-11-17T00:00:00Z".to_owned(),
        };
        let pending = PendingToolCall {
            approval_id: "approval-1".to_owned(),
            tool_id: "records.write".to_owned(),
            arguments_json: "{}".to_owned(),
            idempotency_key: "key-1".to_owned(),
        };
        let auth = AuthContext {
            roles: vec!["approver".to_owned()],
            ..AuthContext::new("local", TenantContext::new("local"))
        };
        authorizer
            .authorize(
                &auth,
                &session,
                &pending,
                "approved",
                &["scope-a".to_owned()],
            )
            .await
            .unwrap();
        assert!(
            authorizer
                .can_review(&auth, &session, &pending)
                .await
                .unwrap()
        );
        assert!(
            authorizer
                .authorize(&auth, &session, &pending, "approved", &[])
                .await
                .is_err()
        );
        let unauthorized = AuthContext::new("local", TenantContext::new("local"));
        assert!(
            !authorizer
                .can_review(&unauthorized, &session, &pending)
                .await
                .unwrap()
        );
        assert!(
            authorizer
                .authorize(
                    &unauthorized,
                    &session,
                    &pending,
                    "approved",
                    &["scope-a".to_owned()]
                )
                .await
                .is_err()
        );
    }
}
