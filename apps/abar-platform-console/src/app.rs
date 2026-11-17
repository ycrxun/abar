use crate::auth::AuthStore;
use crate::connection::{load_connection, load_endpoint};
use crate::handlers;
use crate::ui;
use axum::Json;
use axum::Router;
use axum::extract::{Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use serde::Deserialize;
use serde::Serialize;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) client: reqwest::Client,
    pub(crate) endpoint: String,
    pub(crate) auth: AuthStore,
}

#[derive(Debug)]
pub(crate) struct AppError {
    pub(crate) status: StatusCode,
    pub(crate) message: String,
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

impl AppError {
    pub(crate) fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    pub(crate) fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
        }
    }

    pub(crate) fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
        }
    }

    pub(crate) fn bad_gateway(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            message: message.into(),
        }
    }

    pub(crate) fn internal(message: impl Into<String>) -> Self {
        Self::bad_gateway(message)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorBody {
                error: self.message,
            }),
        )
            .into_response()
    }
}

pub(crate) async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let address = parse_address()?;
    let address_text = address.to_string();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    let (endpoint, auth) = if let Some(auth) = AuthStore::oidc(&address_text).await? {
        (load_endpoint()?, auth)
    } else {
        let connection = load_connection()?;
        (connection.endpoint, AuthStore::local(connection.token))
    };
    let state = Arc::new(AppState {
        client,
        endpoint: endpoint.trim_end_matches('/').to_owned(),
        auth,
    });
    let app = Router::new()
        .route("/", get(index))
        .route("/assets/platform.css", get(platform_css))
        .route("/assets/platform.js", get(platform_js))
        .route("/healthz", get(health))
        .route("/auth/login", get(login))
        .route("/auth/callback", get(callback))
        .route("/auth/logout", post(logout))
        .route("/api/me", get(handlers::current_user))
        .route("/api/overview", get(handlers::overview))
        .route(
            "/api/agents/{agent}/revisions/{revision}/review",
            post(handlers::review),
        )
        .layer(middleware::from_fn(security_headers))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind(address).await?;
    println!("platform console: http://{address}");
    println!(
        "auth: {}",
        if state.auth.is_oidc() {
            "oidc"
        } else {
            "local"
        }
    );
    axum::serve(listener, app).await?;
    Ok(())
}

fn parse_address() -> Result<SocketAddr, Box<dyn std::error::Error>> {
    let mut address =
        std::env::var("ABAR_PLATFORM_ADDR").unwrap_or_else(|_| "127.0.0.1:19092".to_owned());
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--addr" => {
                address = arguments
                    .next()
                    .ok_or("--addr requires a value")?
                    .to_owned();
            }
            "--help" | "-h" => {
                println!("abar-platform-console [--addr 127.0.0.1:19092]");
                println!("OIDC: set ABAR_PLATFORM_OIDC_ISSUER and ABAR_PLATFORM_OIDC_CLIENT_ID");
                std::process::exit(0);
            }
            value => return Err(format!("unknown option: {value}").into()),
        }
    }
    Ok(address.parse()?)
}

async fn security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'",
        ),
    );
    response
}

async fn index(State(state): State<Arc<AppState>>, headers: HeaderMap, uri: Uri) -> Response {
    if state.auth.authenticate(&headers, false).await.is_err() {
        return Redirect::to(&crate::auth::login_url(&uri)).into_response();
    }
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/html; charset=utf-8"),
            ),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
        ],
        ui::index_html(),
    )
        .into_response()
}

async fn health() -> &'static str {
    "ok"
}

async fn platform_css(headers: HeaderMap) -> Response {
    asset_response(&headers, "text/css; charset=utf-8", ui::CSS)
}

async fn platform_js(headers: HeaderMap) -> Response {
    asset_response(&headers, "text/javascript; charset=utf-8", ui::JS)
}

fn asset_response(
    headers: &HeaderMap,
    content_type: &'static str,
    content: &'static str,
) -> Response {
    let etag = format!("\"{}\"", ui::asset_version(content));
    let not_modified = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == etag.as_str());
    let status = if not_modified {
        StatusCode::NOT_MODIFIED
    } else {
        StatusCode::OK
    };
    (
        status,
        [
            (header::CONTENT_TYPE, HeaderValue::from_static(content_type)),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("no-cache, must-revalidate"),
            ),
            (
                header::ETAG,
                HeaderValue::from_str(&etag).expect("asset ETag"),
            ),
        ],
        if not_modified { "" } else { content },
    )
        .into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginQuery {
    return_to: Option<String>,
}

async fn login(
    State(state): State<Arc<AppState>>,
    Query(query): Query<LoginQuery>,
) -> Result<Redirect, AppError> {
    let return_to = crate::auth::safe_return_path(query.return_to.as_deref()).to_owned();
    let url = state.auth.begin_login(return_to).await?;
    Ok(Redirect::to(&url))
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

async fn callback(
    State(state): State<Arc<AppState>>,
    Query(query): Query<CallbackQuery>,
) -> Result<Response, AppError> {
    if let Some(error) = query.error {
        return Err(AppError::unauthorized(
            query.error_description.unwrap_or(error),
        ));
    }
    let code = query
        .code
        .ok_or_else(|| AppError::bad_request("OIDC callback requires code"))?;
    let login_state = query
        .state
        .ok_or_else(|| AppError::bad_request("OIDC callback requires state"))?;
    let (session_id, _, return_to) = state.auth.complete_login(&code, &login_state).await?;
    let cookie = HeaderValue::from_str(&state.auth.session_cookie(&session_id))
        .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(([(header::SET_COOKIE, cookie)], Redirect::to(&return_to)).into_response())
}

async fn logout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    state.auth.authenticate(&headers, true).await?;
    let redirect = state
        .auth
        .logout(&headers)
        .await
        .unwrap_or_else(|| "/".to_owned());
    let cookie = HeaderValue::from_str(&state.auth.clear_cookie())
        .map_err(|error| AppError::internal(error.to_string()))?;
    Ok((
        [(header::SET_COOKIE, cookie)],
        Json(serde_json::json!({"redirect": redirect})),
    )
        .into_response())
}
