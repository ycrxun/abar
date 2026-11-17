#![forbid(unsafe_code)]

//! Agent Build And Runtime core implementation.

pub mod config;
pub mod context;
pub mod digest;
pub mod message;
pub mod provider_bundle;
pub mod revision;
pub mod revision_content;
pub mod server;
pub mod session;

pub use abar_api;

/// Core crate version.
pub const CORE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn core_version_matches_workspace_release() {
        assert_eq!(super::CORE_VERSION, "0.1.0-alpha.1");
    }
}
