use crate::provider_error;
use abar_api::message::PendingToolCall;
use abar_api::policy::{ApprovalAuthorizer, AuthContext};
use abar_api::provider::{BoxFuture, ProviderError};
use abar_api::session::Session;
use serde_json::Value;

/// Requires one configured role and non-empty scopes for approvals.
#[derive(Clone, Debug)]
pub struct RoleApprovalAuthorizer {
    required_role: String,
    scope_field: Option<String>,
}

impl RoleApprovalAuthorizer {
    /// Creates an authorizer for the required role.
    #[must_use]
    pub fn new(required_role: impl Into<String>) -> Self {
        Self {
            required_role: required_role.into(),
            scope_field: None,
        }
    }

    /// Creates an authorizer that can also enforce regional role suffixes.
    #[must_use]
    pub fn with_scope_field(
        required_role: impl Into<String>,
        scope_field: impl Into<String>,
    ) -> Self {
        Self {
            required_role: required_role.into(),
            scope_field: Some(scope_field.into()),
        }
    }

    fn scoped_roles(&self, auth: &AuthContext) -> (bool, Vec<String>) {
        if auth.roles.iter().any(|role| role == &self.required_role) {
            return (true, Vec::new());
        }
        let prefix = format!("{}:", self.required_role);
        let scopes = auth
            .roles
            .iter()
            .filter_map(|role| role.strip_prefix(&prefix))
            .flat_map(|value| value.split(','))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        (!scopes.is_empty(), scopes)
    }

    fn required_scopes(&self, pending: &PendingToolCall) -> Result<Vec<String>, ProviderError> {
        let Some(field) = &self.scope_field else {
            return Ok(Vec::new());
        };
        let arguments: Value = serde_json::from_str(&pending.arguments_json)
            .map_err(|error| provider_error(error.to_string()))?;
        let scopes = arguments
            .get(field)
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if scopes.is_empty() {
            return Err(provider_error("approval required scopes are missing"));
        }
        Ok(scopes)
    }
}

impl ApprovalAuthorizer for RoleApprovalAuthorizer {
    fn can_review<'a>(
        &'a self,
        auth: &'a AuthContext,
        session: &'a Session,
        pending: &'a PendingToolCall,
    ) -> BoxFuture<'a, Result<bool, ProviderError>> {
        Box::pin(async move {
            let (has_role, allowed_scopes) = self.scoped_roles(auth);
            if !has_role
                || session.tenant != auth.tenant.tenant_id
                || pending.approval_id.trim().is_empty()
                || pending.tool_id.trim().is_empty()
            {
                return Ok(false);
            }
            if self.scope_field.is_none() || auth.roles.contains(&self.required_role) {
                return Ok(true);
            }
            let required = self.required_scopes(pending)?;
            Ok(required.iter().all(|scope| allowed_scopes.contains(scope)))
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
            let (has_role, allowed_scopes) = self.scoped_roles(auth);
            if !has_role {
                return Err(provider_error("required approval role is missing"));
            }
            if session.tenant != auth.tenant.tenant_id {
                return Err(provider_error("approver tenant does not match Session"));
            }
            if pending.approval_id.trim().is_empty() || pending.tool_id.trim().is_empty() {
                return Err(provider_error("pending approval evidence is incomplete"));
            }
            if decision == "approved" && scopes.is_empty() {
                return Err(provider_error("approval scope is required"));
            }
            if self.scope_field.is_some() && !auth.roles.contains(&self.required_role) {
                let required = self.required_scopes(pending)?;
                if !required.iter().all(|scope| scopes.contains(scope)) {
                    return Err(provider_error(
                        "approval does not cover all required scopes",
                    ));
                }
                if !scopes.iter().all(|scope| allowed_scopes.contains(scope)) {
                    return Err(provider_error(
                        "approver role is outside the requested scope",
                    ));
                }
            }
            Ok(())
        })
    }
}
