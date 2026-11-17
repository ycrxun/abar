use crate::client::{ConnectionInfo, read_connection_file, server_healthy, write_connection};
use crate::providers::DefaultProviderFactory;
use crate::support::{block_on, default_state_root};
use abar_api::policy::{ApprovalAuthorizer, AuthProvider};
use abar_auth_oidc::{OidcUserInfoAuthProvider, OidcUserInfoConfig, RoleApprovalAuthorizer};
use abar_core::server::EmbeddedServer;
use std::fs::{self, OpenOptions};
use std::process::{Command, ExitCode, Stdio};
use std::sync::Arc;

type ExternalAuth = (Arc<dyn AuthProvider>, Arc<dyn ApprovalAuthorizer>);

pub(crate) fn run_server_command(arguments: &[String]) -> ExitCode {
    match arguments {
        [command] if command == "start" => block_on(start_server()),
        [command, option] if command == "start" && option == "--foreground" => {
            block_on(start_server())
        }
        [command, option] if command == "start" && option == "--daemon" => block_on(start_daemon()),
        [command] if command == "daemon-child" => block_on(start_server()),
        [command] if command == "status" => block_on(server_status()),
        [command] if command == "stop" => block_on(server_stop()),
        _ => {
            eprintln!("usage: abar server start [--daemon] | status | stop");
            ExitCode::from(2)
        }
    }
}

async fn start_server() -> ExitCode {
    let state_root = default_state_root();
    let external_auth = match external_auth() {
        Ok(value) => value,
        Err(error) => {
            eprintln!("server start failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let (server, auth_mode) = if let Some((auth, authorizer)) = external_auth {
        (
            EmbeddedServer::start_with_services(
                state_root.clone(),
                Arc::new(DefaultProviderFactory),
                auth,
                authorizer,
                None,
                env_or("ABAR_CONTROL_PLANE_ROLE", "platform-admin"),
            )
            .await,
            "oidc",
        )
    } else {
        (
            EmbeddedServer::start(state_root.clone(), Arc::new(DefaultProviderFactory)).await,
            "local",
        )
    };
    let server = match server {
        Ok(server) => server,
        Err(error) => {
            eprintln!("server start failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let connection = ConnectionInfo {
        endpoint: server.endpoint().to_owned(),
        token: server.token().to_owned(),
        pid: Some(std::process::id()),
        mode: std::env::var("ABAR_DAEMON_CHILD")
            .ok()
            .map_or_else(|| "foreground".to_owned(), |_| "daemon".to_owned()),
        log_file: std::env::var("ABAR_DAEMON_LOG").ok(),
        auth_mode: auth_mode.to_owned(),
    };
    if let Err(error) = write_connection(&state_root, &connection) {
        eprintln!("server start failed: {error}");
        return ExitCode::FAILURE;
    }
    if connection.mode == "daemon" {
        eprintln!("endpoint: {}", connection.endpoint);
        eprintln!("state: {}", state_root.display());
        eprintln!("auth: {}", connection.auth_mode);
    } else {
        println!("endpoint: {}", connection.endpoint);
        println!("state: {}", state_root.display());
        println!("auth: {}", connection.auth_mode);
    }
    if auth_mode == "oidc" {
        eprintln!("set ABAR_TOKEN to the OIDC access token for CLI and WebUI requests");
    }
    std::future::pending::<()>().await;
    ExitCode::SUCCESS
}

fn external_auth() -> Result<Option<ExternalAuth>, String> {
    let Some(endpoint) = std::env::var("ABAR_OIDC_USERINFO_ENDPOINT").ok() else {
        return Ok(None);
    };
    let config = OidcUserInfoConfig {
        user_info_endpoint: endpoint,
        subject_claim: env_or("ABAR_OIDC_SUBJECT_CLAIM", "sub"),
        tenant_claim: env_or("ABAR_OIDC_TENANT_CLAIM", "tenant"),
        roles_claim: env_or("ABAR_OIDC_ROLES_CLAIM", "roles"),
        allow_http: env_bool("ABAR_OIDC_ALLOW_HTTP"),
        timeout_seconds: std::env::var("ABAR_OIDC_TIMEOUT_SECONDS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(5),
        issuer: env_optional("ABAR_OIDC_ISSUER"),
        audience: env_optional("ABAR_OIDC_AUDIENCE"),
        introspection_endpoint: env_optional("ABAR_OIDC_INTROSPECTION_ENDPOINT"),
        client_id: env_optional("ABAR_OIDC_CLIENT_ID"),
        client_secret: env_optional("ABAR_OIDC_CLIENT_SECRET"),
        cache_seconds: std::env::var("ABAR_OIDC_CACHE_SECONDS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(30),
    };
    let auth: Arc<dyn AuthProvider> = Arc::new(OidcUserInfoAuthProvider::new(config)?);
    let required_role = env_or("ABAR_OIDC_APPROVER_ROLE", "approver");
    let authorizer: Arc<dyn ApprovalAuthorizer> =
        if let Some(scope_field) = env_optional("ABAR_OIDC_SCOPE_FIELD") {
            Arc::new(RoleApprovalAuthorizer::with_scope_field(
                required_role,
                scope_field,
            ))
        } else {
            Arc::new(RoleApprovalAuthorizer::new(required_role))
        };
    Ok(Some((auth, authorizer)))
}

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_owned())
}

fn env_optional(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn env_bool(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes"))
}

async fn start_daemon() -> ExitCode {
    if std::env::var_os("ABAR_ENDPOINT").is_some() {
        eprintln!("daemon start failed: ABAR_ENDPOINT is set; refusing to start a local server");
        return ExitCode::from(3);
    }
    let state_root = default_state_root();
    if let Err(error) = fs::create_dir_all(&state_root) {
        eprintln!("daemon start failed: {error}");
        return ExitCode::FAILURE;
    }
    let connection_path = state_root.join("server.json");
    if let Ok(existing) = read_connection_file(&connection_path) {
        if server_healthy(&existing.endpoint).await {
            println!("state: running");
            println!("endpoint: {}", existing.endpoint);
            if let Some(pid) = existing.pid {
                println!("pid: {pid}");
            }
            return ExitCode::SUCCESS;
        }
        if let Err(error) = fs::remove_file(&connection_path) {
            eprintln!("daemon start failed: cannot remove stale server state: {error}");
            return ExitCode::FAILURE;
        }
    }
    let log_path = state_root.join("server.log");
    let log = match OpenOptions::new().create(true).append(true).open(&log_path) {
        Ok(file) => file,
        Err(error) => {
            eprintln!("daemon start failed: cannot open log file: {error}");
            return ExitCode::FAILURE;
        }
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&log_path, fs::Permissions::from_mode(0o600));
    }
    let executable = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("daemon start failed: cannot locate abar executable: {error}");
            return ExitCode::FAILURE;
        }
    };
    let stdout = match log.try_clone() {
        Ok(file) => file,
        Err(error) => {
            eprintln!("daemon start failed: cannot clone log handle: {error}");
            return ExitCode::FAILURE;
        }
    };
    let mut command = Command::new("nohup");
    command
        .arg(executable)
        .arg("server")
        .arg("daemon-child")
        .env("ABAR_DAEMON_CHILD", "1")
        .env("ABAR_DAEMON_LOG", &log_path)
        .env_remove("ABAR_ENDPOINT")
        .env_remove("ABAR_TOKEN")
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(log));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            eprintln!("daemon start failed: cannot spawn server: {error}");
            return ExitCode::FAILURE;
        }
    };
    for _ in 0..100 {
        if let Ok(connection) = read_connection_file(&connection_path)
            && connection.pid == Some(child.id())
            && server_healthy(&connection.endpoint).await
        {
            println!("state: running");
            println!("endpoint: {}", connection.endpoint);
            println!("pid: {}", child.id());
            println!("log: {}", log_path.display());
            return ExitCode::SUCCESS;
        }
        if let Ok(Some(status)) = child.try_wait() {
            eprintln!("daemon start failed: server exited with {status}");
            eprintln!("log: {}", log_path.display());
            return ExitCode::FAILURE;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let _ = child.kill();
    eprintln!("daemon start timed out; see {}", log_path.display());
    ExitCode::FAILURE
}

async fn server_status() -> ExitCode {
    let state_root = default_state_root();
    let path = state_root.join("server.json");
    let connection = match read_connection_file(&path) {
        Ok(connection) => connection,
        Err(_) => {
            println!("state: stopped");
            return ExitCode::from(3);
        }
    };
    if !server_healthy(&connection.endpoint).await {
        println!("state: stopped");
        println!("detail: stale connection state at {}", path.display());
        return ExitCode::from(3);
    }
    println!("state: running");
    println!("endpoint: {}", connection.endpoint);
    if let Some(pid) = connection.pid {
        println!("pid: {pid}");
    }
    if let Some(log_file) = connection.log_file {
        println!("log: {log_file}");
    }
    ExitCode::SUCCESS
}

async fn server_stop() -> ExitCode {
    if std::env::var_os("ABAR_ENDPOINT").is_some() {
        eprintln!("server stop failed: ABAR_ENDPOINT points to a remote server");
        return ExitCode::from(3);
    }
    let state_root = default_state_root();
    let path = state_root.join("server.json");
    let connection = match read_connection_file(&path) {
        Ok(connection) => connection,
        Err(_) => {
            println!("state: stopped");
            return ExitCode::SUCCESS;
        }
    };
    let Some(pid) = connection.pid else {
        eprintln!("server stop failed: server.json has no pid");
        return ExitCode::FAILURE;
    };
    if pid == std::process::id() {
        eprintln!("server stop failed: refusing to stop the current process");
        return ExitCode::FAILURE;
    }
    if !process_alive(pid) {
        if let Err(error) = fs::remove_file(&path) {
            eprintln!("server stopped but state cleanup failed: {error}");
            return ExitCode::FAILURE;
        }
        println!("state: stopped");
        return ExitCode::SUCCESS;
    }
    if !pid_matches_server(pid) {
        eprintln!("server stop failed: pid {pid} is not the current abar server");
        return ExitCode::FAILURE;
    }
    let status = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if !matches!(status, Ok(status) if status.success()) && process_alive(pid) {
        eprintln!("server stop failed: cannot signal pid {pid}");
        return ExitCode::FAILURE;
    }
    for _ in 0..100 {
        if !process_alive(pid) {
            if let Err(error) = fs::remove_file(&path) {
                eprintln!("server stopped but state cleanup failed: {error}");
                return ExitCode::FAILURE;
            }
            println!("state: stopped");
            return ExitCode::SUCCESS;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    eprintln!("server stop timed out for pid {pid}");
    ExitCode::FAILURE
}

fn pid_matches_server(pid: u32) -> bool {
    let Ok(executable) = std::env::current_exe() else {
        return false;
    };
    let Some(binary_name) = executable.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let Ok(output) = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
    else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let command = String::from_utf8_lossy(&output.stdout);
    command.contains(&format!("/{binary_name} server"))
        || command.starts_with(&format!("{binary_name} server"))
}

fn process_alive(pid: u32) -> bool {
    matches!(
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status(),
        Ok(status) if status.success()
    )
}
