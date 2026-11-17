use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub(crate) struct ConnectionInfo {
    pub(crate) endpoint: String,
    pub(crate) token: String,
}

pub(crate) fn load_connection() -> Result<ConnectionInfo, Box<dyn std::error::Error>> {
    let state_root = std::env::var_os("ABAR_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".abar")
                .join("state")
        });
    let file = std::fs::read(state_root.join("server.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<ConnectionInfo>(&bytes).ok());
    resolve_connection(
        file,
        std::env::var("ABAR_ENDPOINT").ok(),
        std::env::var("ABAR_TOKEN").ok(),
    )
}

pub(crate) fn load_endpoint() -> Result<String, Box<dyn std::error::Error>> {
    let state_root = state_root();
    let file = std::fs::read(state_root.join("server.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<ConnectionInfo>(&bytes).ok());
    std::env::var("ABAR_ENDPOINT")
        .ok()
        .or_else(|| file.map(|value| value.endpoint))
        .ok_or_else(|| "Agent Server connection is not configured".into())
}

fn state_root() -> PathBuf {
    std::env::var_os("ABAR_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".abar")
                .join("state")
        })
}

fn resolve_connection(
    file: Option<ConnectionInfo>,
    endpoint_override: Option<String>,
    token_override: Option<String>,
) -> Result<ConnectionInfo, Box<dyn std::error::Error>> {
    if endpoint_override.is_some() && token_override.is_none() {
        return Err("ABAR_TOKEN is required when ABAR_ENDPOINT is set".into());
    }
    let endpoint_overridden = endpoint_override.is_some();
    let endpoint = endpoint_override
        .or_else(|| file.as_ref().map(|value| value.endpoint.clone()))
        .ok_or("Agent Server connection is not configured")?;
    let token = token_override
        .or_else(|| {
            (!endpoint_overridden)
                .then(|| file.as_ref().map(|value| value.token.clone()))
                .flatten()
        })
        .filter(|value| !value.is_empty())
        .ok_or("ABAR_TOKEN is required for this Agent Server")?;
    Ok(ConnectionInfo { endpoint, token })
}

#[cfg(test)]
mod tests {
    use super::{ConnectionInfo, resolve_connection};

    fn file_connection(token: &str) -> ConnectionInfo {
        ConnectionInfo {
            endpoint: "http://127.0.0.1:4242".to_owned(),
            token: token.to_owned(),
        }
    }

    #[test]
    fn local_connection_uses_file_token() {
        assert_eq!(
            resolve_connection(Some(file_connection("local-token")), None, None)
                .unwrap()
                .token,
            "local-token"
        );
    }

    #[test]
    fn endpoint_override_requires_explicit_token() {
        let error = resolve_connection(
            Some(file_connection("local-token")),
            Some("https://abar".into()),
            None,
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "ABAR_TOKEN is required when ABAR_ENDPOINT is set"
        );
    }

    #[test]
    fn endpoint_override_does_not_reuse_file_token() {
        assert_eq!(
            resolve_connection(
                Some(file_connection("local-token")),
                Some("https://abar".into()),
                Some("oidc-token".into()),
            )
            .unwrap()
            .token,
            "oidc-token"
        );
    }

    #[test]
    fn oidc_file_requires_supplied_token() {
        let error = resolve_connection(Some(file_connection("")), None, None).unwrap_err();
        assert_eq!(
            error.to_string(),
            "ABAR_TOKEN is required for this Agent Server"
        );
    }
}
