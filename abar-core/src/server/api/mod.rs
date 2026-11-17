//! Session, Message, and Agent Revision APIs.

mod approval;
mod common;
mod control_plane;
mod message;
mod message_stream;
mod metrics;
mod revision;
mod runtime;
mod session;
mod task;

use super::ServerState;
use axum::Router;
use std::sync::Arc;

pub(super) fn routes() -> Router<Arc<ServerState>> {
    approval::routes()
        .merge(control_plane::routes())
        .merge(metrics::routes())
        .merge(message::routes())
        .merge(message_stream::routes())
        .merge(revision::routes())
        .merge(session::routes())
        .merge(task::routes())
        .merge(runtime::routes())
}
