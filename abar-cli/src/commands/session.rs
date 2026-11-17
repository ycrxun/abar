use crate::client::load_connection;
use crate::commands::common::print_response;
use crate::support::block_on;
use std::process::ExitCode;

pub(crate) fn run_session_command(arguments: &[String]) -> ExitCode {
    match arguments {
        [command, option, agent]
            if command == "create" && option == "--agent" && !agent.is_empty() =>
        {
            block_on(create_session(agent))
        }
        [command] if command == "list" => block_on(list_sessions()),
        [command, id] if command == "get" && !id.is_empty() => block_on(get_session(id)),
        _ => {
            eprintln!("usage: abar session create --agent <agent> | list | get <session-id>");
            ExitCode::from(2)
        }
    }
}

async fn create_session(agent: &str) -> ExitCode {
    let connection = match load_connection() {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!("session create failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let response = reqwest::Client::new()
        .post(format!("{}/v1/sessions", connection.endpoint))
        .bearer_auth(&connection.token)
        .json(&serde_json::json!({"agent": agent}))
        .send()
        .await;
    print_response("session create", response).await
}

async fn list_sessions() -> ExitCode {
    let connection = match load_connection() {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!("session list failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let response = reqwest::Client::new()
        .get(format!("{}/v1/sessions", connection.endpoint))
        .bearer_auth(&connection.token)
        .send()
        .await;
    print_response("session list", response).await
}

async fn get_session(session_id: &str) -> ExitCode {
    let connection = match load_connection() {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!("session get failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let response = reqwest::Client::new()
        .get(format!("{}/v1/sessions/{session_id}", connection.endpoint))
        .bearer_auth(&connection.token)
        .send()
        .await;
    print_response("session get", response).await
}
