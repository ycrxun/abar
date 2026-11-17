#![forbid(unsafe_code)]

mod app;
mod auth;
mod connection;
mod handlers;
mod server_client;
mod ui;

#[tokio::main]
async fn main() {
    if let Err(error) = app::run().await {
        eprintln!("abar-platform-console failed: {error}");
        std::process::exit(1);
    }
}
