/// Tenant scope propagated through all provider calls.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TenantContext {
    pub tenant_id: String,
}

impl TenantContext {
    /// Creates a tenant context.
    #[must_use]
    pub fn new(tenant_id: impl Into<String>) -> Self {
        Self {
            tenant_id: tenant_id.into(),
        }
    }
}

/// Per-request correlation and cancellation context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestContext {
    pub request_id: String,
    pub tenant: TenantContext,
}

impl RequestContext {
    /// Creates a request context.
    #[must_use]
    pub fn new(request_id: impl Into<String>, tenant: TenantContext) -> Self {
        Self {
            request_id: request_id.into(),
            tenant,
        }
    }
}
