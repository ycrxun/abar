use sha2::{Digest, Sha256};

pub(crate) const HTML: &str = include_str!("ui/index.html");
pub(crate) const CSS: &str = include_str!("ui/assets/platform.css");
pub(crate) const JS: &str = include_str!("ui/assets/platform.js");

pub(crate) fn index_html() -> String {
    HTML.replace("__CSS_VERSION__", &asset_version(CSS))
        .replace("__JS_VERSION__", &asset_version(JS))
}

pub(crate) fn asset_version(content: &str) -> String {
    let digest = Sha256::digest(content.as_bytes());
    format!("{digest:x}")[..16].to_owned()
}
