use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore;
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};

pub(super) const SESSION_COOKIE: &str = "abar_platform_session";

pub(super) fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub(super) fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

pub(super) fn request_cookie<'a>(
    headers: &'a axum::http::HeaderMap,
    name: &str,
) -> Option<&'a str> {
    for header in headers.get_all(axum::http::header::COOKIE) {
        let Ok(value) = header.to_str() else {
            continue;
        };
        for cookie in value.split(';') {
            let Some((key, value)) = cookie.trim().split_once('=') else {
                continue;
            };
            if key == name {
                return Some(value);
            }
        }
    }
    None
}

pub(super) fn session_cookie(value: &str, secure: bool, max_age: u64) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!("{SESSION_COOKIE}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}")
}

pub(super) fn clear_cookie(secure: bool) -> String {
    session_cookie("", secure, 0)
}

pub(crate) fn percent_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(hex(byte >> 4));
            encoded.push(hex(byte & 0x0f));
        }
    }
    encoded
}

fn hex(value: u8) -> char {
    match value {
        0..=9 => char::from(b'0' + value),
        _ => char::from(b'A' + value - 10),
    }
}

#[cfg(test)]
mod tests {
    use super::{percent_encode, pkce_challenge, request_cookie};
    use axum::http::{HeaderMap, HeaderValue, header};

    #[test]
    fn computes_rfc_pkce_challenge() {
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn parses_named_cookie() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("one=1; abar_platform_session=abc; two=2"),
        );
        assert_eq!(
            request_cookie(&headers, "abar_platform_session"),
            Some("abc")
        );
    }

    #[test]
    fn encodes_oauth_values() {
        assert_eq!(
            percent_encode("openid profile:read"),
            "openid%20profile%3Aread"
        );
    }
}
