use std::path::PathBuf;

pub(crate) fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("create Tokio runtime")
        .block_on(future)
}

pub(crate) fn default_state_root() -> PathBuf {
    if let Some(path) = std::env::var_os("ABAR_STATE_DIR") {
        return PathBuf::from(path);
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".abar")
        .join("state")
}

pub(crate) fn default_revision_store() -> PathBuf {
    if let Some(path) = std::env::var_os("ABAR_REVISION_STORE") {
        return PathBuf::from(path);
    }
    default_state_root().join("revisions")
}
