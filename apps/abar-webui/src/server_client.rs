use crate::app::{AppError, AppState};
use serde_json::{Value, json};

pub(crate) async fn server_json(
    state: &AppState,
    token: &str,
    method: reqwest::Method,
    path: &str,
    body: Option<Value>,
) -> Result<Value, AppError> {
    let mut request = state
        .client
        .request(method, format!("{}{}", state.endpoint, path))
        .bearer_auth(token)
        .header(reqwest::header::ACCEPT, "application/json");
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request
        .send()
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    let status = response.status();
    let body: Value = response.json().await.unwrap_or_else(|_| json!({}));
    if !status.is_success() {
        return Err(AppError {
            status,
            message: body
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
                .map_or_else(|| format!("server returned {status}"), str::to_owned),
        });
    }
    Ok(body)
}
