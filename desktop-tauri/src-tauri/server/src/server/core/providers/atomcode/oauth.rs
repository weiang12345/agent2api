//! AtomCode OAuth 登录与续期（云直连，不依赖本地 AtomCode 客户端）。

use serde_json::{json, Value};

use crate::server::core::auth_http::{send_request, ApiResponse};
use crate::server::errors::GatewayError;
use crate::server::logging;

use super::credentials::Credentials;

const PLATFORM_BASE_URL: &str = "https://acs.atomgit.com";
#[derive(Clone, Debug)]
pub struct OAuthLogin {
    pub state: String,
    pub login_url: String,
}

pub async fn start() -> Result<OAuthLogin, GatewayError> {
    let url = format!("{PLATFORM_BASE_URL}/auth/login?provider=atomgit");
    let response = send_request("GET", &url, None, &[])
        .await
        .map_err(|error| transport_error("发起 AtomCode 登录", &error.to_string()))?;
    let payload = payload(response, "发起 AtomCode 登录")?;
    let state = text(&payload, &["state"]);
    let login_url = text(&payload, &["login_url", "loginUrl"]);
    if state.is_empty() || login_url.is_empty() {
        return Err(GatewayError::with_status(
            502,
            "AtomCode 登录服务未返回 state 或授权地址",
        ));
    }
    Ok(OAuthLogin { state, login_url })
}

pub async fn poll_once(state: &str) -> Result<Option<Credentials>, GatewayError> {
    let check_url = format!(
        "{PLATFORM_BASE_URL}/auth/check?state={}",
        urlencoding(state)
    );
    let response = send_request("GET", &check_url, None, &[])
        .await
        .map_err(|error| transport_error("检查 AtomCode 登录状态", &error.to_string()))?;
    let check_payload = payload(response, "检查 AtomCode 登录状态")?;
    if !check_payload
        .get("valid")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Ok(None);
    }

    let token_url = format!(
        "{PLATFORM_BASE_URL}/auth/token?state={}",
        urlencoding(state)
    );
    let response = send_request("GET", &token_url, None, &[])
        .await
        .map_err(|error| transport_error("获取 AtomCode 登录凭证", &error.to_string()))?;
    let token_payload = payload(response, "获取 AtomCode 登录凭证")?;
    Some(credentials_from_token(&token_payload)).transpose()
}

pub async fn refresh(credentials: &Credentials) -> Result<Credentials, GatewayError> {
    if credentials.refresh_token.is_empty() {
        return Err(GatewayError::with_status(
            400,
            "AtomCode 账号缺少 refreshToken，无法续期",
        ));
    }
    let url = format!("{PLATFORM_BASE_URL}/oauth/refresh");
    let body = json!({ "refresh_token": credentials.refresh_token });
    let response = send_request("POST", &url, Some(&body), &[])
        .await
        .map_err(|error| transport_error("续期 AtomCode 登录凭证", &error.to_string()))?;
    let payload = payload(response, "续期 AtomCode 登录凭证")?;
    let mut refreshed = credentials_from_token(&payload)?;
    if refreshed.user_id.is_empty() {
        refreshed.user_id = credentials.user_id.clone();
    }
    if refreshed.name.is_empty() {
        refreshed.name = credentials.name.clone();
    }
    if refreshed.email.is_empty() {
        refreshed.email = credentials.email.clone();
    }
    if refreshed.refresh_token.is_empty() {
        refreshed.refresh_token = credentials.refresh_token.clone();
    }
    if refreshed.expires_at.is_none() {
        refreshed.expires_at = Some(logging::now_ms() + 24 * 60 * 60 * 1000);
    }
    Ok(refreshed)
}

fn credentials_from_token(payload: &Value) -> Result<Credentials, GatewayError> {
    let access_token = payload
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| GatewayError::with_status(502, "AtomCode 登录响应缺少 access_token"))?;
    let refresh_token = payload
        .get("refresh_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    let user = payload.get("user").cloned().unwrap_or(Value::Null);
    let expires_at = payload
        .get("expires_in")
        .and_then(Value::as_i64)
        .filter(|seconds| *seconds > 0)
        .map(|seconds| logging::now_ms() + seconds * 1000)
        .or_else(|| super::credentials::timestamp(payload.get("expires_at")));
    let mut credentials = Credentials {
        access_token: access_token.to_string(),
        refresh_token: refresh_token.to_string(),
        expires_at,
        user_id: text(&user, &["id", "userId", "user_id"]),
        name: text(&user, &["name", "username"]),
        email: text(&user, &["email"]),
    };
    credentials.complete_identity()?;
    Ok(credentials)
}

fn payload(response: ApiResponse, action: &str) -> Result<Value, GatewayError> {
    if !response.ok {
        let message = response
            .payload
            .as_ref()
            .and_then(|value| {
                value
                    .get("message")
                    .or_else(|| value.get("msg"))
                    .and_then(Value::as_str)
            })
            .unwrap_or("上游返回非 2xx");
        return Err(GatewayError::with_status(
            i32::from(response.status),
            format!("{action}失败：{message}"),
        ));
    }
    response
        .payload
        .filter(|value| value.is_object())
        .ok_or_else(|| GatewayError::with_status(502, format!("{action}未返回有效 JSON 对象")))
}

fn text(payload: &Value, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|key| payload.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_default()
}

fn transport_error(action: &str, message: &str) -> GatewayError {
    GatewayError::with_status(502, format!("{action}失败：{message}"))
}

fn urlencoding(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}
