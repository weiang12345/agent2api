//! Trae 网页 OAuth、回调解析与 token 续期。
use serde_json::{json, Value};

use crate::server::core::auth_http::{send_request_via, ApiResponse};
use crate::server::errors::GatewayError;

use super::credentials::{random_hex, random_numeric_device_id, Credentials};
use super::protocol::{
    oauth_headers, CLIENT_ID, CONSOLE_HOST, IDE_VERSION, OAUTH_HOST,
};

const REQUEST_TIMEOUT_MS: u64 = 30_000;

#[derive(Clone, Debug)]
pub struct Login {
    pub state: String,
    pub auth_url: String,
    pub machine_id: String,
    pub device_id: String,
}

pub fn build_login(callback_url: &str) -> Result<Login, GatewayError> {
    let machine_id = random_hex(16)?;
    let device_id = random_numeric_device_id()?;
    let trace_id = machine_trace_id(&machine_id, &device_id);
    let mut url = url::Url::parse(&format!("{CONSOLE_HOST}/authorization"))
        .map_err(|_| GatewayError::with_status(500, "Trae 登录地址无效"))?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("login_version", "1");
        query.append_pair("auth_from", "solo");
        query.append_pair("login_channel", "native_ide");
        query.append_pair("plugin_version", "2.3.62834");
        query.append_pair("auth_type", "local");
        query.append_pair("client_id", CLIENT_ID);
        query.append_pair("redirect", "0");
        query.append_pair("login_trace_id", &trace_id);
        query.append_pair("auth_callback_url", callback_url);
        query.append_pair("machine_id", &machine_id);
        query.append_pair("device_id", &device_id);
        query.append_pair("x_device_id", &device_id);
        query.append_pair("x_machine_id", &machine_id);
        query.append_pair("x_device_brand", "PC");
        query.append_pair("x_device_type", "PC");
        query.append_pair("x_os_version", "1.0");
        query.append_pair("x_app_version", IDE_VERSION);
        query.append_pair("x_app_type", "stable");
    }
    Ok(Login {
        state: trace_id,
        auth_url: url.to_string(),
        machine_id,
        device_id,
    })
}

pub fn machine_trace_id(machine_id: &str, device_id: &str) -> String {
    let value = format!("{machine_id}{device_id}");
    if value.len() >= 16 {
        value[value.len() - 16..].to_string()
    } else {
        format!("{value:0>16}")
    }
}

#[derive(Debug)]
pub struct Callback {
    pub access_token: String,
    pub refresh_token: String,
    pub user_id: String,
    pub nickname: String,
    pub enterprise_id: String,
    pub expires_at: Option<i64>,
}

pub fn parse_callback(raw_url: &str, expected_state: &str) -> Result<Callback, GatewayError> {
    let url = url::Url::parse(raw_url)
        .map_err(|_| GatewayError::with_status(400, "Trae 登录回调地址无效"))?;
    let query: std::collections::HashMap<String, String> = url
        .query_pairs()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();
    let state = query
        .get("loginTraceID")
        .or_else(|| query.get("login_trace_id"))
        .cloned()
        .unwrap_or_default();
    if state != expected_state {
        return Err(GatewayError::with_status(400, "Trae 登录回调 state 不一致，请重新发起登录"));
    }
    let user_info = parse_json_param(query.get("userInfo").map(String::as_str));
    let user_jwt = parse_json_param(query.get("userJwt").map(String::as_str));
    let text = |value: &Value, keys: &[&str]| -> String {
        keys.iter()
            .find_map(|key| match value.get(*key) {
                Some(Value::String(text)) => Some(text.clone()),
                Some(Value::Number(number)) => Some(number.to_string()),
                _ => None,
            })
            .unwrap_or_default()
    };
    let refresh_token = query
        .get("refreshToken")
        .cloned()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| text(&user_jwt, &["RefreshToken"]));
    let access_token = text(&user_jwt, &["Token"]);
    let expires_at = user_jwt
        .get("TokenExpireAt")
        .and_then(Value::as_i64)
        .and_then(normalize_expire);
    if refresh_token.is_empty() && access_token.is_empty() {
        return Err(GatewayError::with_status(400, "Trae 登录回调缺少凭证"));
    }
    Ok(Callback {
        access_token,
        refresh_token,
        user_id: text(&user_info, &["UserID", "userId"]),
        nickname: text(&user_info, &["ScreenName", "screenName", "name"]),
        enterprise_id: text(&user_info, &["TenantID", "tenantId"]),
        expires_at,
    })
}

pub async fn exchange(callback: Callback, machine_id: &str, device_id: &str) -> Result<Credentials, GatewayError> {
    let mut callback = callback;
    if !callback.refresh_token.is_empty() {
        let response = send_request_via(
            "POST",
            &format!("{OAUTH_HOST}/cloudide/api/v3/trae/oauth/ExchangeToken"),
            Some(&json!({
                "ClientID": CLIENT_ID,
                "RefreshToken": callback.refresh_token,
                "ClientSecret": "-",
                "UserID": "",
            })),
            &oauth_headers(),
            None,
            Some(REQUEST_TIMEOUT_MS),
        )
        .await
        .map_err(|error| GatewayError::with_status(502, format!("Trae 登录凭证兑换失败：{error}")))?;
        let payload = response_payload(response, "Trae 登录凭证兑换")?;
        let result = payload.get("Result").cloned().unwrap_or(payload);
        callback.access_token = result
            .get("Token")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| GatewayError::with_status(502, "Trae 登录凭证兑换缺少 Token"))?;
        if let Some(refresh) = result.get("RefreshToken").and_then(Value::as_str).filter(|value| !value.is_empty()) {
            callback.refresh_token = refresh.to_string();
        }
        callback.expires_at = result
            .get("TokenExpireAt")
            .and_then(Value::as_i64)
            .and_then(normalize_expire)
            .or_else(|| {
                result
                    .get("TokenExpireDuration")
                    .and_then(Value::as_i64)
                    .filter(|value| *value > 0)
                    .map(|seconds| crate::server::logging::now_ms() + seconds * 1000)
            });
    }
    if callback.user_id.is_empty() {
        fill_user_info(&mut callback).await?;
    }
    Credentials::from_parts(
        callback.access_token,
        callback.refresh_token,
        callback.expires_at,
        callback.user_id,
        callback.nickname,
        callback.enterprise_id,
    )
    .map(|mut credentials| {
        credentials.machine_id = machine_id.to_string();
        credentials.device_id = device_id.to_string();
        credentials
    })
}

pub async fn refresh(credentials: &Credentials) -> Result<Credentials, GatewayError> {
    if credentials.refresh_token.is_empty() {
        return Err(GatewayError::with_status(400, "Trae 账号缺少 refreshToken，无法续期"));
    }
    let response = send_request_via(
        "POST",
        &format!("{OAUTH_HOST}/cloudide/api/v3/trae/oauth/ExchangeToken"),
        Some(&json!({
            "ClientID": CLIENT_ID,
            "RefreshToken": credentials.refresh_token,
            "ClientSecret": "-",
            "UserID": "",
        })),
        &oauth_headers(),
        None,
        Some(REQUEST_TIMEOUT_MS),
    )
    .await
    .map_err(|error| GatewayError::with_status(502, format!("Trae 凭证续期失败：{error}")))?;
    let payload = response_payload(response, "Trae 凭证续期")?;
    let result = payload.get("Result").cloned().unwrap_or(payload);
    let token = result
        .get("Token")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| GatewayError::with_status(502, "Trae 凭证续期缺少 Token"))?;
    let refresh_token = result
        .get("RefreshToken")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| credentials.refresh_token.clone());
    let expires_at = result
        .get("TokenExpireAt")
        .and_then(Value::as_i64)
        .and_then(normalize_expire)
        .or_else(|| {
            result
                .get("TokenExpireDuration")
                .and_then(Value::as_i64)
                .filter(|value| *value > 0)
                .map(|seconds| crate::server::logging::now_ms() + seconds * 1000)
        });
    Ok(Credentials {
        access_token: token,
        refresh_token,
        expires_at,
        user_id: credentials.user_id.clone(),
        nickname: credentials.nickname.clone(),
        enterprise_id: credentials.enterprise_id.clone(),
        machine_id: credentials.machine_id.clone(),
        device_id: credentials.device_id.clone(),
    })
}

async fn fill_user_info(callback: &mut Callback) -> Result<(), GatewayError> {
    if callback.access_token.is_empty() {
        return Ok(());
    }
    let response = send_request_via(
        "POST",
        &format!("{OAUTH_HOST}/cloudide/api/v3/trae/GetUserInfo"),
        Some(&json!({ "ReqSource": "IDE", "IDEVersion": IDE_VERSION })),
        &oauth_headers_with_token(&callback.access_token),
        None,
        Some(REQUEST_TIMEOUT_MS),
    )
    .await
    .map_err(|error| GatewayError::with_status(502, format!("Trae 用户信息查询失败：{error}")))?;
    let payload = response_payload(response, "Trae 用户信息查询")?;
    let result = payload.get("Result").cloned().unwrap_or(payload);
    callback.user_id = result
        .get("UserID")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| GatewayError::with_status(502, "Trae 未返回用户标识"))?;
    if callback.nickname.is_empty() {
        callback.nickname = result
            .get("ScreenName")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
    }
    if callback.enterprise_id.is_empty() {
        callback.enterprise_id = result
            .get("EnterpriseID")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
    }
    Ok(())
}

fn oauth_headers_with_token(token: &str) -> Vec<(String, String)> {
    let mut headers = oauth_headers();
    headers.push(("X-Cloudide-Token".to_string(), token.to_string()));
    headers
}

fn parse_json_param(raw: Option<&str>) -> Value {
    let Some(raw) = raw else { return Value::Null };
    if let Ok(value) = serde_json::from_str::<Value>(raw) {
        return value;
    }
    if let Ok(decoded) = urlencoding_decode(raw) {
        if let Ok(value) = serde_json::from_str::<Value>(&decoded) {
            return value;
        }
    }
    Value::Null
}

fn urlencoding_decode(value: &str) -> Result<String, GatewayError> {
    let mut out = String::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let high = (bytes[index + 1] as char).to_digit(16);
                let low = (bytes[index + 2] as char).to_digit(16);
                if let (Some(high), Some(low)) = (high, low) {
                    out.push(((high << 4) | low) as u8 as char);
                    index += 3;
                } else {
                    out.push(bytes[index] as char);
                    index += 1;
                }
            }
            b'+' => {
                out.push(' ');
                index += 1;
            }
            byte => {
                out.push(byte as char);
                index += 1;
            }
        }
    }
    Ok(out)
}

fn response_payload(response: ApiResponse, action: &str) -> Result<Value, GatewayError> {
    if !response.ok {
        let message = response
            .payload
            .as_ref()
            .and_then(|value| value.get("message").or_else(|| value.get("msg")))
            .and_then(Value::as_str)
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

fn normalize_expire(value: i64) -> Option<i64> {
    if value <= 0 {
        return None;
    }
    if value > 1_000_000_000_000 {
        Some(value)
    } else {
        value.checked_mul(1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_login_url_and_parses_callback() {
        let login = build_login("http://127.0.0.1:1234/authorize")
            .expect("build login");
        assert!(login.auth_url.starts_with("https://www.trae.cn/authorization?"));
        assert!(login.auth_url.contains("auth_callback_url=http%3A%2F%2F127.0.0.1%3A1234%2Fauthorize"));
        assert_eq!(login.machine_id.len(), 32);
        assert_eq!(login.device_id.len(), 16);
        assert!(login.device_id.chars().all(|ch| ch.is_ascii_digit()));

        let user_info_json = serde_json::json!({
            "UserID": "user-1",
            "ScreenName": "Trae User",
        })
        .to_string();
        let user_info: String = url::form_urlencoded::byte_serialize(user_info_json.as_bytes()).collect();
        let user_jwt_json = serde_json::json!({ "Token": "jwt-token" }).to_string();
        let user_jwt: String = url::form_urlencoded::byte_serialize(user_jwt_json.as_bytes()).collect();
        let callback_url = format!(
            "http://127.0.0.1:1234/authorize?refreshToken=refresh&userInfo={}&userJwt={}&loginTraceID={}",
            user_info, user_jwt, login.state
        );
        let callback = parse_callback(&callback_url, &login.state).expect("parse callback");
        assert_eq!(callback.refresh_token, "refresh");
        assert_eq!(callback.user_id, "user-1");
        assert_eq!(callback.nickname, "Trae User");
    }
}
