//! Authentication contract.

use crate::context::TenantContext;
use crate::message::PendingToolCall;
use crate::provider::{BoxFuture, ProviderError};
use crate::session::Session;

/// Authentication result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthContext {
    pub subject_id: String,
    pub tenant: TenantContext,
    pub roles: Vec<String>,
}

impl AuthContext {
    /// Creates an authenticated identity without granted roles.
    #[must_use]
    pub fn new(subject_id: impl Into<String>, tenant: TenantContext) -> Self {
        Self {
            subject_id: subject_id.into(),
            tenant,
            roles: Vec::new(),
        }
    }
}

/// Authentication contract.
pub trait AuthProvider: Send + Sync {
    /// Authenticates a bearer token.
    fn authenticate<'a>(
        &'a self,
        token: &'a str,
    ) -> BoxFuture<'a, Result<AuthContext, ProviderError>>;
}

/// Verifies that an authenticated subject may decide one pending approval.
pub trait ApprovalAuthorizer: Send + Sync {
    /// Returns whether this authenticated subject may review one pending Tool call.
    fn can_review<'a>(
        &'a self,
        auth: &'a AuthContext,
        session: &'a Session,
        pending: &'a PendingToolCall,
    ) -> BoxFuture<'a, Result<bool, ProviderError>>;

    /// Authorizes one approval decision for an exact Session and Tool call.
    fn authorize<'a>(
        &'a self,
        auth: &'a AuthContext,
        session: &'a Session,
        pending: &'a PendingToolCall,
        decision: &'a str,
        scopes: &'a [String],
    ) -> BoxFuture<'a, Result<(), ProviderError>>;
}
