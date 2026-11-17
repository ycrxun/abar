#![forbid(unsafe_code)]

//! DingTalk custom robot notifications with Workbench deep links.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2::Sha256;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

/// DingTalk custom robot configuration.
#[derive(Clone)]
pub struct DingTalkRobotConfig {
    pub webhook_url: String,
    pub secret: Option<String>,
    pub allow_http: bool,
}

impl fmt::Debug for DingTalkRobotConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DingTalkRobotConfig")
            .field("webhook_url", &self.webhook_url)
            .field("secret", &self.secret.as_ref().map(|_| "<redacted>"))
            .field("allow_http", &self.allow_http)
            .finish()
    }
}

impl DingTalkRobotConfig {
    /// Validates the webhook transport.
    pub fn validate(&self) -> Result<(), String> {
        if !self.webhook_url.starts_with("https://")
            && !(self.allow_http && self.webhook_url.starts_with("http://"))
        {
            return Err("DingTalk webhook must use HTTPS".to_owned());
        }
        Ok(())
    }
}

/// DingTalk robot client.
#[derive(Clone, Debug)]
pub struct DingTalkRobot {
    config: DingTalkRobotConfig,
    client: reqwest::Client,
}

impl DingTalkRobot {
    /// Creates a robot client after validating configuration.
    pub fn new(config: DingTalkRobotConfig) -> Result<Self, String> {
        config.validate()?;
        Ok(Self {
            config,
            client: reqwest::Client::new(),
        })
    }

    /// Sends one action card linking to the Agent Workbench.
    pub async fn send_action_card(
        &self,
        title: &str,
        markdown: &str,
        button_title: &str,
        workbench_url: &str,
    ) -> Result<(), String> {
        if !workbench_url.starts_with("https://")
            && !(self.config.allow_http && workbench_url.starts_with("http://"))
        {
            return Err("Workbench URL must use HTTPS".to_owned());
        }
        let response = self
            .client
            .post(self.signed_webhook_url()?)
            .json(&action_card(title, markdown, button_title, workbench_url))
            .send()
            .await
            .map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Err(format!("DingTalk webhook returned {}", response.status()));
        }
        Ok(())
    }

    fn signed_webhook_url(&self) -> Result<String, String> {
        let Some(secret) = &self.config.secret else {
            return Ok(self.config.webhook_url.clone());
        };
        let timestamp = unix_millis()?;
        let to_sign = format!("{timestamp}\n{secret}");
        let mut mac =
            Hmac::<Sha256>::new_from_slice(secret.as_bytes()).map_err(|error| error.to_string())?;
        mac.update(to_sign.as_bytes());
        let sign = STANDARD.encode(mac.finalize().into_bytes());
        let separator = if self.config.webhook_url.contains('?') {
            '&'
        } else {
            '?'
        };
        Ok(format!(
            "{}{separator}timestamp={timestamp}&sign={}",
            self.config.webhook_url,
            percent_encode(&sign)
        ))
    }
}

/// Builds the DingTalk actionCard payload.
#[must_use]
pub fn action_card(title: &str, markdown: &str, button_title: &str, workbench_url: &str) -> Value {
    json!({
        "msgtype": "actionCard",
        "actionCard": {
            "title": title,
            "text": markdown,
            "btnOrientation": "0",
            "singleTitle": button_title,
            "singleURL": workbench_url
        }
    })
}

fn unix_millis() -> Result<u128, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|error| error.to_string())
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::{DingTalkRobot, DingTalkRobotConfig};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn posts_action_card_to_webhook() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = vec![0_u8; 8192];
            let read = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..read]).into_owned();
            let response = "HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}";
            stream.write_all(response.as_bytes()).await.unwrap();
            request.contains(r#""msgtype":"actionCard""#)
                && request.contains(r#""singleURL":"http://127.0.0.1:19090""#)
        });
        let robot = DingTalkRobot::new(DingTalkRobotConfig {
            webhook_url: format!("http://{address}/robot"),
            secret: None,
            allow_http: true,
        })
        .unwrap();
        robot
            .send_action_card(
                "商品运营提醒",
                "### 库存缺失 TopN",
                "打开 Agent 工作台",
                "http://127.0.0.1:19090",
            )
            .await
            .unwrap();
        assert!(server.await.unwrap());
    }
}
