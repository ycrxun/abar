use crate::support::default_state_root;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConnectionInfo {
    pub(crate) endpoint: String,
    pub(crate) token: String,
    #[serde(default)]
    pub(crate) pid: Option<u32>,
    #[serde(default)]
    pub(crate) mode: String,
    #[serde(default)]
    pub(crate) log_file: Option<String>,
    #[serde(default)]
    pub(crate) auth_mode: String,
}

pub(crate) fn write_connection(
    state_root: &Path,
    connection: &ConnectionInfo,
) -> Result<(), String> {
    fs::create_dir_all(state_root).map_err(|error| error.to_string())?;
    let path = state_root.join("server.json");
    let bytes = serde_json::to_vec_pretty(connection).map_err(|error| error.to_string())?;
    fs::write(&path, bytes).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(crate) fn load_connection() -> Result<ConnectionInfo, String> {
    let file = read_connection_file(&default_state_root().join("server.json")).ok();
    resolve_connection(
        file,
        std::env::var("ABAR_ENDPOINT").ok(),
        std::env::var("ABAR_TOKEN").ok(),
    )
}

fn resolve_connection(
    file: Option<ConnectionInfo>,
    endpoint_override: Option<String>,
    token_override: Option<String>,
) -> Result<ConnectionInfo, String> {
    if endpoint_override.is_some() && token_override.is_none() {
        return Err("ABAR_TOKEN is required when ABAR_ENDPOINT is set".to_owned());
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
    Ok(ConnectionInfo {
        endpoint,
        token,
        pid: None,
        mode: "remote".to_owned(),
        log_file: None,
        auth_mode: if endpoint_overridden {
            "external".to_owned()
        } else {
            file.map_or_else(|| "external".to_owned(), |value| value.auth_mode)
        },
    })
}

pub(crate) fn read_connection_file(path: &Path) -> Result<ConnectionInfo, String> {
    let bytes = fs::read(path).map_err(|error| format!("read server connection: {error}"))?;
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

pub(crate) async fn server_healthy(endpoint: &str) -> bool {
    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(1))
        .build()
    else {
        return false;
    };
    matches!(
        client.get(format!("{endpoint}/healthz")).send().await,
        Ok(response) if response.status().is_success()
    )
}

#[cfg(test)]
mod tests {
    use super::{ConnectionInfo, resolve_connection};

    fn file_connection(token: &str) -> ConnectionInfo {
        ConnectionInfo {
            endpoint: "http://127.0.0.1:4242".to_owned(),
            token: token.to_owned(),
            pid: Some(1),
            mode: "daemon".to_owned(),
            log_file: None,
            auth_mode: "local".to_owned(),
        }
    }

    #[test]
    fn local_connection_uses_file_token() {
        let connection =
            resolve_connection(Some(file_connection("local-token")), None, None).unwrap();
        assert_eq!(connection.token, "local-token");
        assert_eq!(connection.auth_mode, "local");
    }

    #[test]
    fn endpoint_override_requires_explicit_token() {
        let error = resolve_connection(
            Some(file_connection("local-token")),
            Some("https://abar".into()),
            None,
        )
        .unwrap_err();
        assert_eq!(error, "ABAR_TOKEN is required when ABAR_ENDPOINT is set");
    }

    #[test]
    fn endpoint_override_does_not_reuse_file_token() {
        let connection = resolve_connection(
            Some(file_connection("local-token")),
            Some("https://abar".into()),
            Some("oidc-token".into()),
        )
        .unwrap();
        assert_eq!(connection.token, "oidc-token");
        assert_eq!(connection.auth_mode, "external");
    }

    #[test]
    fn oidc_file_requires_supplied_token() {
        let error = resolve_connection(Some(file_connection("")), None, None).unwrap_err();
        assert_eq!(error, "ABAR_TOKEN is required for this Agent Server");
        let connection =
            resolve_connection(Some(file_connection("")), None, Some("oidc-token".into())).unwrap();
        assert_eq!(connection.token, "oidc-token");
    }
}
