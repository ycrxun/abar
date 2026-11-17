//! Embedded HTTP Server for Agent Revision, Session, and Message Runtime.

mod api;
mod auth;
mod factory;
mod message_worker;
mod observability;

use crate::digest::sha256_hex;
use abar_api::policy::{ApprovalAuthorizer, AuthProvider};
use axum::Json;
use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Router, serve};
use fs2::FileExt;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

pub use factory::RuntimeProviderFactory;

/// Server startup or handler failure.
#[derive(Debug, Error)]
pub enum ServerError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Session error: {0}")]
    Session(#[from] crate::session::SessionError),
    #[error("another server is already using this state directory")]
    AlreadyRunning,
}

/// A running embedded Server.
pub struct EmbeddedServer {
    endpoint: String,
    token: String,
    task: JoinHandle<()>,
}

impl fmt::Debug for EmbeddedServer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EmbeddedServer")
            .field("endpoint", &self.endpoint)
            .finish_non_exhaustive()
    }
}

impl EmbeddedServer {
    /// Starts an embedded Server on loopback with a random port.
    pub async fn start(
        state_root: impl Into<PathBuf>,
        factory: Arc<dyn RuntimeProviderFactory>,
    ) -> Result<Self, ServerError> {
        let token = session_token();
        let auth: Arc<dyn AuthProvider> = Arc::new(auth::LocalBearerAuth::new(token.clone()));
        let approval_authorizer: Arc<dyn ApprovalAuthorizer> =
            Arc::new(auth::LocalApprovalAuthorizer);
        Self::start_with_services(
            state_root,
            factory,
            auth,
            approval_authorizer,
            Some(token),
            "platform-admin",
        )
        .await
    }

    /// Starts an embedded Server with externally supplied identity and approval authority.
    pub async fn start_with_services(
        state_root: impl Into<PathBuf>,
        factory: Arc<dyn RuntimeProviderFactory>,
        auth: Arc<dyn AuthProvider>,
        approval_authorizer: Arc<dyn ApprovalAuthorizer>,
        connection_token: Option<String>,
        control_plane_role: impl Into<String>,
    ) -> Result<Self, ServerError> {
        let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))).await?;
        let address = listener.local_addr()?;
        let token = connection_token.unwrap_or_default();
        let root = state_root.into();
        fs::create_dir_all(&root)?;
        let lock = acquire_server_lock(&root)?;
        crate::session::SessionStore::new(root.join("sessions"))
            .rebuild_approval_index()
            .map_err(ServerError::Session)?;
        let state = Arc::new(ServerState {
            root,
            auth,
            approval_authorizer,
            control_plane_role: control_plane_role.into(),
            factory,
            cancellations: Mutex::new(HashMap::new()),
            tasks: Mutex::new(HashMap::new()),
            metrics: observability::RuntimeMetrics::new(),
            _lock: lock,
        });
        message_worker::recover(state.clone()).await?;
        let app = Router::new()
            .merge(api::routes())
            .route("/healthz", get(health))
            .route("/v1/capabilities", get(capabilities))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                observability::track_http,
            ))
            .layer(DefaultBodyLimit::max(8 * 1024 * 1024))
            .with_state(state);
        let task = tokio::spawn(async move {
            if let Err(error) = serve(listener, app).await {
                eprintln!("embedded server failed: {error}");
            }
        });
        observability::event(
            "info",
            "server.started",
            serde_json::json!({"endpoint": format!("http://{address}")}),
        );
        Ok(Self {
            endpoint: format!("http://{address}"),
            token,
            task,
        })
    }

    /// Returns the loopback endpoint.
    #[must_use]
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Returns the local bearer token, or an empty string for external identity.
    #[must_use]
    pub fn token(&self) -> &str {
        &self.token
    }
}

impl Drop for EmbeddedServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

struct ServerState {
    root: PathBuf,
    auth: Arc<dyn AuthProvider>,
    approval_authorizer: Arc<dyn ApprovalAuthorizer>,
    control_plane_role: String,
    factory: Arc<dyn RuntimeProviderFactory>,
    cancellations: Mutex<HashMap<String, Arc<AtomicBool>>>,
    tasks: Mutex<HashMap<String, TaskRecord>>,
    metrics: observability::RuntimeMetrics,
    _lock: File,
}

enum TaskRecord {
    Preparing {
        fingerprint: String,
    },
    Ready {
        fingerprint: String,
        session_id: String,
        message_id: String,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Capabilities {
    server_version: &'static str,
    api_version: &'static str,
    formats: Formats,
    transports: Vec<&'static str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Formats {
    agent_revision: &'static str,
    session: &'static str,
    message: &'static str,
    runtime_event: &'static str,
}

struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    fn unauthorized() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "unauthorized",
            message: "missing or invalid bearer token".to_owned(),
        }
    }

    fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            code: "forbidden",
            message: message.into(),
        }
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_request",
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "not_found",
            message: message.into(),
        }
    }

    fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            code: "message_conflict",
            message: message.into(),
        }
    }

    fn unavailable(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "provider_unavailable",
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal_error",
            message: message.into(),
        }
    }
}

#[derive(Serialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorBody {
    code: &'static str,
    message: String,
    retryable: bool,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorEnvelope {
                error: ErrorBody {
                    code: self.code,
                    message: self.message,
                    retryable: false,
                },
            }),
        )
            .into_response()
    }
}

async fn health() -> &'static str {
    "ok"
}

async fn capabilities() -> Json<Capabilities> {
    Json(Capabilities {
        server_version: env!("CARGO_PKG_VERSION"),
        api_version: "abar/v1",
        formats: Formats {
            agent_revision: "agent-revision/v1",
            session: "session/v1",
            message: "message/v1",
            runtime_event: "runtime-event/v1",
        },
        transports: vec!["mcp-streamable-http"],
    })
}

fn acquire_server_lock(root: &Path) -> Result<File, ServerError> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("server.lock"))?;
    match file.try_lock_exclusive() {
        Ok(()) => Ok(file),
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
            Err(ServerError::AlreadyRunning)
        }
        Err(error) => Err(ServerError::Io(error)),
    }
}

fn session_token() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    sha256_hex(format!("{now}:{}", std::process::id()).as_bytes())
        .trim_start_matches("sha256:")
        .to_owned()
}
