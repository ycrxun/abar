#![forbid(unsafe_code)]

mod client;
mod commands;
mod providers;
mod support;

use std::process::ExitCode;

const USAGE: &str = "\
abar 0.1 Agent Build And Runtime

Usage:
  abar version
  abar --version
  abar publish [--json] [source]
  abar revision list <agent>
  abar revision current <agent>
  abar revision show <agent:revision>
  abar revision approve <agent:revision> --reviewer <name> [--comment <text>]
  abar revision reject <agent:revision> --reviewer <name> [--comment <text>]
  abar server start [--daemon]
  abar server status
  abar server stop
  abar state backup <archive.tar.gz>
  abar state restore <archive.tar.gz> [--replace]
  abar state reindex-approvals
  abar runtime status
  abar session create --agent <agent>
  abar session list
  abar session get <session-id>
  abar message create <session-id> --input <text> [--skill <name>] [--no-wait]
  abar message get <message-id>
  abar message events <message-id> [--after <sequence>]
  abar message approve <message-id> [--session <id>] [--approval-id <id>] [--scope <scope>] [--comment <text>] [--no-wait]
  abar message reject <message-id> [--session <id>] [--approval-id <id>] [--scope <scope>] [--comment <text>] [--no-wait]
  abar message cancel <message-id>
  abar message reconcile <message-id> --action query|cancel
  abar help
";

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match arguments.first().map(String::as_str) {
        Some("version" | "--version") => {
            println!(
                "abar {} (api {})",
                env!("CARGO_PKG_VERSION"),
                abar_api::API_VERSION
            );
            ExitCode::SUCCESS
        }
        Some("publish") => commands::revision::run_publish(&arguments[1..]),
        Some("revision") => commands::revision::run_revision_command(&arguments[1..]),
        Some("server") => commands::server::run_server_command(&arguments[1..]),
        Some("state") => commands::state::run_state_command(&arguments[1..]),
        Some("runtime") => commands::runtime::run_runtime_command(&arguments[1..]),
        Some("session") => commands::session::run_session_command(&arguments[1..]),
        Some("message") => commands::message::run_message_command(&arguments[1..]),
        Some("help" | "--help") | None => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some(command) => {
            eprintln!("unknown command: {command}");
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
    }
}
