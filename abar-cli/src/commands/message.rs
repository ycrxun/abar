use crate::client::load_connection;
use crate::commands::common::{option_value, option_values, print_json, print_response};
use std::process::ExitCode;

pub(crate) fn run_message_command(arguments: &[String]) -> ExitCode {
    let Some(command) = arguments.first().map(String::as_str) else {
        eprintln!("usage: abar message create|get|events|approve|reject ...");
        return ExitCode::from(2);
    };
    crate::support::block_on(async move {
        match command {
            "create" => message_create(&arguments[1..]).await,
            "get" => message_get(&arguments[1..]).await,
            "events" => message_events(&arguments[1..]).await,
            "approve" => message_approval("approved", &arguments[1..]).await,
            "reject" => message_approval("rejected", &arguments[1..]).await,
            "cancel" => message_cancel(&arguments[1..]).await,
            "reconcile" => message_reconcile(&arguments[1..]).await,
            _ => {
                eprintln!("unknown message command: {command}");
                ExitCode::from(2)
            }
        }
    })
}

async fn message_cancel(arguments: &[String]) -> ExitCode {
    let Some(message_id) = arguments.first() else {
        eprintln!("usage: abar message cancel <message-id>");
        return ExitCode::from(2);
    };
    let connection = match load_connection() {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!("message cancel failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let response = reqwest::Client::new()
        .post(format!(
            "{}/v1/messages/{message_id}/cancel",
            connection.endpoint
        ))
        .bearer_auth(connection.token)
        .send()
        .await;
    print_response("message cancel", response).await
}

async fn message_reconcile(arguments: &[String]) -> ExitCode {
    let Some(message_id) = arguments.first() else {
        eprintln!("usage: abar message reconcile <message-id> --action query|cancel");
        return ExitCode::from(2);
    };
    let Some(action) = option_value(arguments, "--action") else {
        eprintln!("--action is required");
        return ExitCode::from(2);
    };
    if !matches!(action.as_str(), "query" | "cancel") {
        eprintln!("--action must be query or cancel");
        return ExitCode::from(2);
    }
    let connection = match load_connection() {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!("message reconcile failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let response = reqwest::Client::new()
        .post(format!(
            "{}/v1/messages/{message_id}/reconcile",
            connection.endpoint
        ))
        .bearer_auth(connection.token)
        .json(&serde_json::json!({"action": action}))
        .send()
        .await;
    print_response("message reconcile", response).await
}

async fn message_create(arguments: &[String]) -> ExitCode {
    let Some(session_id) = arguments.first() else {
        eprintln!("usage: abar message create <session-id> --input <text>");
        return ExitCode::from(2);
    };
    let Some(input) = option_value(arguments, "--input") else {
        eprintln!("--input is required");
        return ExitCode::from(2);
    };
    let requested_skills = option_values(arguments, "--skill");
    let no_wait = arguments.iter().any(|value| value == "--no-wait");
    let connection = match load_connection() {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!("message create failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let client = reqwest::Client::new();
    let response = client
        .post(format!(
            "{}/v1/sessions/{session_id}/messages",
            connection.endpoint
        ))
        .bearer_auth(&connection.token)
        .json(&serde_json::json!({
            "input": input,
            "requestedSkills": requested_skills
        }))
        .send()
        .await;
    let message = match response {
        Ok(response) if response.status().is_success() => {
            match response.json::<serde_json::Value>().await {
                Ok(value) => value,
                Err(error) => {
                    eprintln!("message create failed: {error}");
                    return ExitCode::FAILURE;
                }
            }
        }
        Ok(response) => {
            eprintln!(
                "message create failed: {} {}",
                response.status(),
                response.text().await.unwrap_or_default()
            );
            return ExitCode::FAILURE;
        }
        Err(error) => {
            eprintln!("message create failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    if no_wait {
        print_json(&message);
        return ExitCode::SUCCESS;
    }
    let message_id = message["id"].as_str().unwrap_or_default();
    match wait_message(&client, &connection, message_id).await {
        Ok(message) => {
            print_json(&message);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("message create failed: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn message_get(arguments: &[String]) -> ExitCode {
    let Some(message_id) = arguments.first() else {
        eprintln!("usage: abar message get <message-id>");
        return ExitCode::from(2);
    };
    let connection = match load_connection() {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!("message get failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let response = reqwest::Client::new()
        .get(format!("{}/v1/messages/{message_id}", connection.endpoint))
        .bearer_auth(&connection.token)
        .send()
        .await;
    print_response("message get", response).await
}

async fn message_events(arguments: &[String]) -> ExitCode {
    let Some(message_id) = arguments.first() else {
        eprintln!("usage: abar message events <message-id> [--after <sequence>]");
        return ExitCode::from(2);
    };
    let after = option_value(arguments, "--after").unwrap_or_else(|| "0".to_owned());
    let connection = match load_connection() {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!("message events failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let response = reqwest::Client::new()
        .get(format!(
            "{}/v1/messages/{message_id}/events?after={after}",
            connection.endpoint
        ))
        .bearer_auth(&connection.token)
        .send()
        .await;
    print_response("message events", response).await
}

async fn message_approval(decision: &str, arguments: &[String]) -> ExitCode {
    let Some(message_id) = arguments.first() else {
        eprintln!("usage: abar message approve|reject <message-id>");
        return ExitCode::from(2);
    };
    let connection = match load_connection() {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!("message approval failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let client = reqwest::Client::new();
    let explicit_session = option_value(arguments, "--session");
    let message = if explicit_session.is_some() {
        None
    } else {
        match client
            .get(format!("{}/v1/messages/{message_id}", connection.endpoint))
            .bearer_auth(&connection.token)
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {
                match response.json::<serde_json::Value>().await {
                    Ok(value) => Some(value),
                    Err(error) => {
                        eprintln!("message approval failed: {error}");
                        return ExitCode::FAILURE;
                    }
                }
            }
            _ => {
                eprintln!(
                    "message approval failed: cannot load Message; use --session with --approval-id"
                );
                return ExitCode::FAILURE;
            }
        }
    };
    let session_id = explicit_session.as_deref().unwrap_or_else(|| {
        message
            .as_ref()
            .and_then(|value| value["sessionId"].as_str())
            .unwrap_or_default()
    });
    let approval_id = option_value(arguments, "--approval-id")
        .or_else(|| {
            message
                .as_ref()
                .and_then(|value| value["pendingApproval"]["approvalId"].as_str())
                .map(str::to_owned)
        })
        .unwrap_or_default();
    if session_id.is_empty() || approval_id.is_empty() {
        eprintln!("message approval failed: --session and --approval-id are required");
        return ExitCode::FAILURE;
    }
    let scopes = option_values(arguments, "--scope");
    let comment = option_value(arguments, "--comment");
    let no_wait = arguments.iter().any(|value| value == "--no-wait");
    let response = client
        .post(format!(
            "{}/v1/sessions/{session_id}/messages/{message_id}/approval",
            connection.endpoint
        ))
        .bearer_auth(&connection.token)
        .json(&serde_json::json!({
            "approvalId": approval_id,
            "decision": decision,
            "scopes": scopes,
            "comment": comment
        }))
        .send()
        .await;
    let message = match response {
        Ok(response) if response.status().is_success() => {
            match response.json::<serde_json::Value>().await {
                Ok(value) => value,
                Err(error) => {
                    eprintln!("message approval failed: {error}");
                    return ExitCode::FAILURE;
                }
            }
        }
        Ok(response) => {
            eprintln!(
                "message approval failed: {} {}",
                response.status(),
                response.text().await.unwrap_or_default()
            );
            return ExitCode::FAILURE;
        }
        Err(error) => {
            eprintln!("message approval failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    if no_wait {
        print_json(&message);
        return ExitCode::SUCCESS;
    }
    match wait_message(&client, &connection, message_id).await {
        Ok(message) => {
            print_json(&message);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("message approval failed: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn wait_message(
    client: &reqwest::Client,
    connection: &crate::client::ConnectionInfo,
    message_id: &str,
) -> Result<serde_json::Value, String> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(900);
    loop {
        let message: serde_json::Value = client
            .get(format!("{}/v1/messages/{message_id}", connection.endpoint))
            .bearer_auth(&connection.token)
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?
            .json()
            .await
            .map_err(|error| error.to_string())?;
        if !matches!(
            message["state"].as_str().unwrap_or_default(),
            "queued" | "running"
        ) {
            return Ok(message);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("timed out waiting for Message".to_owned());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}
