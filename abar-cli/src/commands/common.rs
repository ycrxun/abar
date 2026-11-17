use std::process::ExitCode;

pub(crate) async fn print_response(
    operation: &str,
    response: Result<reqwest::Response, reqwest::Error>,
) -> ExitCode {
    match response {
        Ok(response) if response.status().is_success() => {
            match response.json::<serde_json::Value>().await {
                Ok(value) => {
                    print_json(&value);
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("{operation} failed: {error}");
                    ExitCode::FAILURE
                }
            }
        }
        Ok(response) => {
            eprintln!(
                "{operation} failed: {} {}",
                response.status(),
                response.text().await.unwrap_or_default()
            );
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("{operation} failed: {error}");
            ExitCode::FAILURE
        }
    }
}

pub(crate) fn print_json(value: &serde_json::Value) {
    match serde_json::to_string_pretty(value) {
        Ok(value) => println!("{value}"),
        Err(error) => eprintln!("cannot encode response: {error}"),
    }
}

pub(crate) fn option_value(arguments: &[String], name: &str) -> Option<String> {
    arguments
        .iter()
        .position(|value| value == name)
        .and_then(|index| arguments.get(index + 1))
        .cloned()
}

pub(crate) fn option_values(arguments: &[String], name: &str) -> Vec<String> {
    let mut values = Vec::new();
    for (index, value) in arguments.iter().enumerate() {
        if value == name
            && let Some(value) = arguments.get(index + 1)
        {
            values.push(value.clone());
        }
    }
    values
}
