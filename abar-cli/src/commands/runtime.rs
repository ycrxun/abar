use crate::client::load_connection;
use std::process::ExitCode;

pub(crate) fn run_runtime_command(arguments: &[String]) -> ExitCode {
    match arguments.first().map(String::as_str) {
        Some("status") if arguments.len() == 1 => crate::support::block_on(runtime_status()),
        _ => {
            eprintln!("usage: abar runtime status");
            ExitCode::from(2)
        }
    }
}

async fn runtime_status() -> ExitCode {
    let connection = match load_connection() {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!("runtime status failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let response = reqwest::Client::new()
        .get(format!("{}/v1/runtime/status", connection.endpoint))
        .bearer_auth(connection.token)
        .send()
        .await;
    match response {
        Ok(response) if response.status().is_success() => {
            match response.json::<serde_json::Value>().await {
                Ok(value) => {
                    if let Ok(value) = serde_json::to_string_pretty(&value) {
                        println!("{value}");
                    }
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("runtime status failed: {error}");
                    ExitCode::FAILURE
                }
            }
        }
        Ok(response) => {
            eprintln!(
                "runtime status failed: {} {}",
                response.status(),
                response.text().await.unwrap_or_default()
            );
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("runtime status failed: {error}");
            ExitCode::FAILURE
        }
    }
}
