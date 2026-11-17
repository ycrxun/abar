use serde_json::Value;
use sha2::{Digest, Sha256};

pub(super) fn ceil_to_u32(value: f64) -> u32 {
    value.ceil().clamp(0.0, f64::from(u32::MAX)) as u32
}

pub(super) fn round_one(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

pub(super) fn round_two(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

pub(super) fn sha256_json(value: &Value) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{}", hex(hasher.finalize().as_slice()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
