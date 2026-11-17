//! Canonical digest and relative-path helpers used by Agent Revision content.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

/// Hashes raw bytes as a SHA-256 digest.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{}", hex_digest(hasher.finalize().as_slice()))
}

/// Hashes a normalized path/content index.
#[must_use]
pub fn content_digest(files: &BTreeMap<PathBuf, Vec<u8>>) -> String {
    let mut hasher = Sha256::new();
    for (path, content) in files {
        let path = path.to_string_lossy();
        hasher.update((path.len() as u64).to_le_bytes());
        hasher.update(path.as_bytes());
        hasher.update((content.len() as u64).to_le_bytes());
        hasher.update(content);
    }
    format!("sha256:{}", hex_digest(hasher.finalize().as_slice()))
}

/// Validates a normalized relative path without parent traversal.
pub fn validate_relative_path(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Err("empty relative path".to_owned());
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            _ => return Err(format!("invalid relative path: {}", path.display())),
        }
    }
    Ok(())
}

fn hex_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(char::from(HEX[usize::from(byte >> 4)]));
        result.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::validate_relative_path;
    use std::path::Path;

    #[test]
    fn rejects_absolute_and_parent_paths() {
        assert!(validate_relative_path(Path::new("/absolute")).is_err());
        assert!(validate_relative_path(Path::new("../escape")).is_err());
        assert!(validate_relative_path(Path::new("skills/demo/SKILL.md")).is_ok());
    }
}
