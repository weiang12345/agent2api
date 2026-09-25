//! AtomCode 云直连请求构造。

use serde_json::Value;

use crate::server::core::providers::adapter::ChatRequestPlan;
use crate::server::errors::GatewayError;

use super::crypto::{self, SignatureInput};

pub const GATEWAY_BASE_URL: &str = "https://llm-api.atomgit.com/v1";
pub const CLIENT_VERSION: &str = "5.0.2";

pub fn build_chat_request(
    access_token: &str,
    user_id: &str,
    body: &Value,
) -> Result<ChatRequestPlan, GatewayError> {
    if access_token.is_empty() {
        return Err(GatewayError::with_status(
            401,
            "AtomCode 账号缺少 accessToken，无法转发",
        ));
    }
    if user_id.is_empty() {
        return Err(GatewayError::with_status(
            401,
            "AtomCode 账号缺少用户标识，无法签名",
        ));
    }
    let serialized = serde_json::to_vec(body)
        .map_err(|_| GatewayError::with_status(500, "AtomCode 请求体序列化失败"))?;
    let nonce = crypto::random_nonce()?;
    let timestamp = crate::server::logging::now_ms() / 1000;
    let input = SignatureInput {
        method: "POST",
        path: "/v1/chat/completions",
        body: &serialized,
        oauth_token: access_token,
        user_id,
        client_version: CLIENT_VERSION,
        timestamp,
        nonce: &nonce,
    };
    let mut headers = vec![
        ("Content-Type".to_string(), "application/json".to_string()),
        ("Accept".to_string(), "application/json".to_string()),
        (
            "Authorization".to_string(),
            format!("Bearer {access_token}"),
        ),
        (
            "User-Agent".to_string(),
            format!("atomcode/{CLIENT_VERSION}"),
        ),
    ];
    headers.extend(crypto::build_auth_headers(&input)?);
    Ok(ChatRequestPlan {
        url: format!("{GATEWAY_BASE_URL}/chat/completions"),
        headers,
        body: body.clone(),
    })
}
