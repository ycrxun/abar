use crate::support::default_state_root;
use fs2::FileExt;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn run_state_command(arguments: &[String]) -> ExitCode {
    match arguments {
        [command, archive] if command == "backup" => match backup(Path::new(archive)) {
            Ok(value) => {
                print_json(&value);
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("state backup failed: {error}");
                ExitCode::FAILURE
            }
        },
        [command, archive, option] if command == "restore" && option == "--replace" => {
            restore(Path::new(archive), true)
        }
        [command, archive] if command == "restore" => restore(Path::new(archive), false),
        [command] if command == "reindex-approvals" => match reindex_approvals() {
            Ok(value) => {
                print_json(&value);
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("approval reindex failed: {error}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!("usage: abar state backup <archive.tar.gz>");
            eprintln!("       abar state restore <archive.tar.gz> [--replace]");
            eprintln!("       abar state reindex-approvals");
            ExitCode::from(2)
        }
    }
}

fn reindex_approvals() -> Result<serde_json::Value, String> {
    let state_root = default_state_root();
    if !state_root.is_dir() {
        return Err(format!(
            "state directory not found: {}",
            state_root.display()
        ));
    }
    let _lock = acquire_offline_lock(&state_root)?;
    let count = abar_core::session::SessionStore::new(state_root.join("sessions"))
        .rebuild_approval_index()
        .map_err(|error| error.to_string())?;
    Ok(json!({
        "stateDirectory": state_root.display().to_string(),
        "approvalsReindexed": count
    }))
}

fn backup(archive: &Path) -> Result<serde_json::Value, String> {
    let state_root = default_state_root();
    if !state_root.is_dir() {
        return Err(format!(
            "state directory not found: {}",
            state_root.display()
        ));
    }
    let _lock = acquire_offline_lock(&state_root)?;
    let archive = absolute_existing_parent(archive)?;
    if archive.exists() {
        return Err(format!("archive already exists: {}", archive.display()));
    }
    if is_within(&archive, &state_root)? {
        return Err("archive must be outside the state directory".to_owned());
    }
    run_tar(&[
        "-czf".as_ref(),
        archive.as_os_str(),
        "-C".as_ref(),
        state_root.as_os_str(),
        ".".as_ref(),
    ])?;
    let digest = sha256_file(&archive)?;
    write_checksum(&archive, &digest)?;
    let bytes = fs::metadata(&archive)
        .map_err(|error| error.to_string())?
        .len();
    Ok(json!({
        "archive": archive.display().to_string(),
        "sha256": digest,
        "bytes": bytes
    }))
}

fn restore(archive: &Path, replace: bool) -> ExitCode {
    match restore_inner(archive, replace) {
        Ok(value) => {
            print_json(&value);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("state restore failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn restore_inner(archive: &Path, replace: bool) -> Result<serde_json::Value, String> {
    let archive = absolute_existing_parent(archive)?;
    if !archive.is_file() {
        return Err(format!("archive not found: {}", archive.display()));
    }
    verify_checksum(&archive)?;
    verify_archive_paths(&archive)?;
    let state_root = default_state_root();
    if state_root.exists() && !state_root.is_dir() {
        return Err(format!(
            "state path is not a directory: {}",
            state_root.display()
        ));
    }
    if state_root.is_dir() {
        let _lock = acquire_offline_lock(&state_root)?;
        if !replace && !directory_is_empty_for_restore(&state_root)? {
            return Err(
                "state directory is not empty; stop the Server and pass --replace".to_owned(),
            );
        }
    }
    let parent = state_root
        .parent()
        .ok_or_else(|| "state directory has no parent".to_owned())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let staging = parent.join(format!(
        ".abar-restore-{}-{}",
        std::process::id(),
        unix_nanos()
    ));
    fs::create_dir(&staging).map_err(|error| error.to_string())?;
    let result = (|| {
        run_tar(&[
            "-xzf".as_ref(),
            archive.as_os_str(),
            "-C".as_ref(),
            staging.as_os_str(),
        ])?;
        let stale_connection = staging.join("server.json");
        if stale_connection.exists() {
            fs::remove_file(stale_connection).map_err(|error| error.to_string())?;
        }
        let previous = if state_root.exists() {
            if !replace && !directory_is_empty_for_restore(&state_root)? {
                return Err("state directory is not empty".to_owned());
            }
            let previous = parent.join(format!(
                ".abar-pre-restore-{}-{}",
                std::process::id(),
                unix_nanos()
            ));
            fs::rename(&state_root, &previous).map_err(|error| error.to_string())?;
            Some(previous)
        } else {
            None
        };
        if let Err(error) = fs::rename(&staging, &state_root) {
            if let Some(previous) = &previous {
                let _ = fs::rename(previous, &state_root);
            }
            return Err(error.to_string());
        }
        Ok(previous)
    })();
    match result {
        Ok(previous) => Ok(json!({
            "archive": archive.display().to_string(),
            "stateDirectory": state_root.display().to_string(),
            "previousState": previous.map(|path| path.display().to_string())
        })),
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            Err(error)
        }
    }
}

fn acquire_offline_lock(state_root: &Path) -> Result<File, String> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(state_root.join("server.lock"))
        .map_err(|error| error.to_string())?;
    file.try_lock_exclusive()
        .map_err(|_| "Server appears to be running; stop it before this operation".to_owned())?;
    Ok(file)
}

fn directory_is_empty_for_restore(path: &Path) -> Result<bool, String> {
    for entry in fs::read_dir(path).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry.file_name() != "server.lock" {
            return Ok(false);
        }
    }
    Ok(true)
}

fn verify_checksum(archive: &Path) -> Result<(), String> {
    let checksum_path = checksum_path(archive);
    if !checksum_path.exists() {
        return Ok(());
    }
    let expected = fs::read_to_string(checksum_path)
        .map_err(|error| error.to_string())?
        .split_whitespace()
        .next()
        .ok_or_else(|| "checksum file is empty".to_owned())?
        .to_owned();
    let actual = sha256_file(archive)?;
    if actual != expected {
        return Err("archive checksum does not match".to_owned());
    }
    Ok(())
}

fn verify_archive_paths(archive: &Path) -> Result<(), String> {
    let output = Command::new("tar")
        .args(["-tzf", &archive.display().to_string()])
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    for entry in String::from_utf8_lossy(&output.stdout).lines() {
        if entry.is_empty() {
            continue;
        }
        if !archive_entry_is_safe(entry) {
            return Err(format!("unsafe archive path: {entry}"));
        }
    }
    let verbose = Command::new("tar")
        .args(["-tvzf", &archive.display().to_string()])
        .output()
        .map_err(|error| error.to_string())?;
    if !verbose.status.success() {
        return Err(String::from_utf8_lossy(&verbose.stderr).trim().to_owned());
    }
    for entry in String::from_utf8_lossy(&verbose.stdout).lines() {
        if !entry.starts_with(['-', 'd']) {
            return Err(format!(
                "unsupported archive entry type: {}",
                entry.chars().next().unwrap_or('?')
            ));
        }
    }
    Ok(())
}

fn archive_entry_is_safe(entry: &str) -> bool {
    let path = Path::new(entry);
    !path.is_absolute()
        && !path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
}

fn run_tar(arguments: &[&std::ffi::OsStr]) -> Result<(), String> {
    let status = Command::new("tar")
        .args(arguments)
        .stdin(Stdio::null())
        .status()
        .map_err(|error| error.to_string())?;
    if !status.success() {
        return Err(format!("tar exited with {status}"));
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn write_checksum(archive: &Path, digest: &str) -> Result<(), String> {
    fs::write(
        checksum_path(archive),
        format!("{digest}  {}\n", archive.display()),
    )
    .map_err(|error| error.to_string())
}

fn checksum_path(archive: &Path) -> PathBuf {
    PathBuf::from(format!("{}.sha256", archive.display()))
}

fn absolute_existing_parent(path: &Path) -> Result<PathBuf, String> {
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|error| error.to_string())?
            .join(path)
    };
    let parent = path
        .parent()
        .ok_or_else(|| "path has no parent".to_owned())?;
    if !parent.is_dir() {
        return Err(format!("parent directory not found: {}", parent.display()));
    }
    Ok(path)
}

fn is_within(path: &Path, root: &Path) -> Result<bool, String> {
    let path = path
        .parent()
        .ok_or_else(|| "archive path has no parent".to_owned())?
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    Ok(path.starts_with(root))
}

fn unix_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
}

fn print_json(value: &serde_json::Value) {
    match serde_json::to_string_pretty(value) {
        Ok(value) => println!("{value}"),
        Err(error) => eprintln!("cannot encode response: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::archive_entry_is_safe;

    #[test]
    fn rejects_unsafe_archive_entries() {
        assert!(archive_entry_is_safe("./sessions/session.json"));
        assert!(!archive_entry_is_safe("../outside"));
        assert!(!archive_entry_is_safe("/absolute"));
    }
}
