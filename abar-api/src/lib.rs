#![forbid(unsafe_code)]

//! Stable public types shared by the core, CLI, and adapters.
//!
//! Wire contracts are defined by the JSON Schema and OpenAPI files that will
//! live under `schemas/` and `openapi/`. These Rust types are the initial SDK.

pub mod conformance;
pub mod content;
pub mod context;
pub mod data;
pub mod error;
pub mod knowledge;
pub mod message;
pub mod policy;
pub mod provider;
pub mod revision;
pub mod runtime_event;
pub mod session;

pub mod approval;

/// API contract version implemented by this crate.
pub const API_VERSION: &str = env!("CARGO_PKG_VERSION");
