use super::cookie::percent_encode;
use axum::http::Uri;

const MAX_RETURN_PATH_BYTES: usize = 2_048;

pub(crate) fn login_url(uri: &Uri) -> String {
    let return_to = uri.path_and_query().map_or("/", |path| path.as_str());
    format!(
        "/auth/login?returnTo={}",
        percent_encode(safe_return_path(Some(return_to)))
    )
}

pub(crate) fn safe_return_path(value: Option<&str>) -> &str {
    value
        .filter(|value| value.starts_with('/'))
        .filter(|value| !value.starts_with("//"))
        .filter(|value| !value.contains('\\'))
        .filter(|value| !value.chars().any(char::is_control))
        .filter(|value| value.len() <= MAX_RETURN_PATH_BYTES)
        .unwrap_or("/")
}

#[cfg(test)]
mod tests {
    use super::safe_return_path;

    #[test]
    fn accepts_same_origin_workbench_paths() {
        assert_eq!(safe_return_path(Some("/")), "/");
        assert_eq!(
            safe_return_path(Some("/?agent=example-assistant")),
            "/?agent=example-assistant"
        );
    }

    #[test]
    fn rejects_external_and_ambiguous_paths() {
        for value in [
            None,
            Some(""),
            Some("https://evil.example"),
            Some("//evil.example"),
            Some("/\\evil.example"),
            Some("/\r\nlocation:https://evil.example"),
        ] {
            assert_eq!(safe_return_path(value), "/");
        }
    }
}
