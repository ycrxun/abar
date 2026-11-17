#![forbid(unsafe_code)]

//! Deterministic merchandise-operations MCP fixture.

mod catalog;
mod data;
mod orders;
mod profiles;
mod queries;
mod recommendations;
mod reports;
mod tools;
mod util;

pub use catalog::MockCatalog;
pub use profiles::MockProfile;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
