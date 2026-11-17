#![forbid(unsafe_code)]

mod app;
mod auth;
mod confirmation;
mod connection;
mod handlers;
mod presentation;
mod server_client;
mod stream;
mod ui;
mod view;

#[tokio::main]
async fn main() {
    if let Err(error) = app::run().await {
        eprintln!("abar-webui failed: {error}");
        std::process::exit(1);
    }
}
