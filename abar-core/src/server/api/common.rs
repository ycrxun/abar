use super::super::{ApiError, ServerState};
use crate::revision::{RevisionError, RevisionStore};
use crate::session::{SessionError, SessionStore};
use abar_api::context::{RequestContext, TenantContext};
use abar_api::policy::AuthContext;
use abar_api::session::Session;

pub(super) fn store(state: &ServerState) -> SessionStore {
    SessionStore::new(state.root.join("sessions"))
}

pub(super) fn owned_session(
    state: &ServerState,
    auth: &AuthContext,
    session_id: &str,
) -> Result<Session, ApiError> {
    let session = store(state).get(session_id).map_err(session_error)?;
    if session.tenant != auth.tenant.tenant_id || session.user != auth.subject_id {
        return Err(ApiError::not_found("Session not found"));
    }
    Ok(session)
}

pub(super) fn tenant_session(
    state: &ServerState,
    auth: &AuthContext,
    session_id: &str,
) -> Result<Session, ApiError> {
    let session = store(state).get(session_id).map_err(session_error)?;
    if session.tenant != auth.tenant.tenant_id {
        return Err(ApiError::not_found("Session not found"));
    }
    Ok(session)
}

pub(super) fn request_context(auth: &AuthContext, request_id: &str) -> RequestContext {
    RequestContext::new(
        request_id,
        TenantContext::new(auth.tenant.tenant_id.clone()),
    )
}

pub(super) fn revision_store(state: &ServerState) -> RevisionStore {
    RevisionStore::new(state.root.join("revisions"))
}

pub(super) fn revision_error(error: RevisionError) -> ApiError {
    match error {
        RevisionError::NotFound { .. } | RevisionError::CurrentNotSet(_) => {
            ApiError::not_found(error.to_string())
        }
        _ => ApiError::bad_request(error.to_string()),
    }
}

pub(super) fn session_error(error: SessionError) -> ApiError {
    match error {
        SessionError::SessionNotFound(_) | SessionError::MessageNotFound(_) => {
            ApiError::not_found(error.to_string())
        }
        SessionError::ActiveMessageExists
        | SessionError::MessageExists(_)
        | SessionError::InvalidSessionTransition
        | SessionError::InvalidMessageTransition { .. } => ApiError::conflict(error.to_string()),
        SessionError::InvalidIdentifier(_) => ApiError::bad_request(error.to_string()),
        _ => ApiError::internal(error.to_string()),
    }
}
