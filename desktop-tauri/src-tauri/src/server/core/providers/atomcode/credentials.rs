//! AtomCode 凭证：OAuth access / refresh token 与账号身份。

use serde_json::{json, Value};

use crate::server::core::account_store::MAX_TOKEN_LENGTH;
use crate::server::errors::GatewayError;
use crate::server::logging;

/// token 临期刷新窗口（秒）。与 Atom2Api 保持一致：提前 5 分钟续期。
pub const REFRESH_MARGIN_MS: i64 = 5 * 60 * 1000;

#[derive(Clone, Debug)]
pub struct Credentials {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: Option<i64>,
    pub user_id: String,
    pub name: String,
    pub email: String,
}

impl Credentials {
    pub fn from_payload(payload: &Value) -> Result<Self, GatewayError> {
        if !payload.is_object() {
            return Err(GatewayError::with_status(400, "AtomCode 账号内容必须是 JSON 对象"));
        }
        let access_token = secret(payload, &["accessToken", "access_token", "access", "token"])?;
        if access_token.is_empty() {
            return Err(GatewayError::with_status(400, "缺少 AtomCode access token"));
        }
        let refresh_token = secret(payload, &["refreshToken", "refresh_token", "refresh"])?;
        let expires_at = ["expiresAt", "expires_at", "expires", "tokenExpiresAt"]
            .iter()
            .find_map(|key| timestamp(payload.get(*key)));
        let user_id = text(payload, &["userId", "user_id", "uid"]);
        if user_id.len() > 256 || user_id.chars().any(char::is_control) {
            return Err(GatewayError::with_status(400, "AtomCode 用户标识无效"));
        }
        Ok(Self {
            access_token,
            refresh_token,
            expires_at,
            user_id,
            name: text(payload, &["name", "nickname"]).chars().take(100).collect(),
            email: text(payload, &["email"]).chars().take(320).collect(),
        })
    }

    pub fn complete_identity(&mut self) -> Result<(), GatewayError> {
        if self.user_id.is_empty() {
            return Err(GatewayError::with_status(400, "缺少 AtomCode 用户标识，无法保存账号"));
        }
        Ok(())
    }

    pub fn can_refresh(&self) -> bool {
        !self.refresh_token.is_empty()
    }

    pub fn expiring(&self) -> bool {
        self.can_refresh()
            && self.expires_at.is_some_and(|expiry| expiry <= logging::now_ms() + REFRESH_MARGIN_MS)
    }

    pub fn to_value(&self) -> Value {
        let text_or_null = |value: &str| -> Value {
            if value.is_empty() { Value::Null } else { Value::String(value.to_string()) }
        };
        json!({
            "accessToken": text_or_null(&self.access_token),
            "refreshToken": text_or_null(&self.refresh_token),
            "expiresAt": self.expires_at,
            "userId": text_or_null(&self.user_id),
            "email": text_or_null(&self.email),
            "nickname": text_or_null(&self.name),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_atomcode_credentials() {
        let credentials = Credentials::from_payload(&json!({
            "accessToken": "Bearer access-token",
            "refreshToken": "refresh-token",
            "expiresAt": 4102444800000_i64,
            "userId": "user-1",
            "name": "AtomCode User",
            "email": "user@example.com",
        }))
        .expect("parse AtomCode credentials");

        assert_eq!(credentials.access_token, "access-token");
        assert_eq!(credentials.refresh_token, "refresh-token");
        assert_eq!(credentials.expires_at, Some(4102444800000));
        assert_eq!(credentials.user_id, "user-1");
        assert_eq!(credentials.name, "AtomCode User");
        assert_eq!(credentials.email, "user@example.com");
        assert!(credentials.can_refresh());
        assert!(!credentials.expiring());
    }

    #[test]
    fn rejects_empty_access_token() {
        let error = Credentials::from_payload(&json!({})).expect_err("missing access token");
        assert_eq!(error.status_code, 400);
    }
}

pub fn text(payload: &Value, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|key| match payload.get(*key) {
            Some(Value::String(value)) if !value.trim().is_empty() => {
                Some(value.trim().to_string())
            }
            Some(Value::Number(value)) => Some(value.to_string()),
            _ => None,
        })
        .unwrap_or_default()
}

pub fn secret(payload: &Value, keys: &[&str]) -> Result<String, GatewayError> {
    let mut value = keys
        .iter()
        .find_map(|key| payload.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("");
    if value.get(..7).is_some_and(|prefix| prefix.eq_ignore_ascii_case("Bearer ")) {
        value = value[7..].trim();
    }
    if value.len() > MAX_TOKEN_LENGTH || value.chars().any(char::is_control) {
        return Err(GatewayError::with_status(400, "AtomCode 凭证过长或包含非法控制字符"));
    }
    Ok(value.to_string())
}

pub fn timestamp(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    let number = value.as_i64().or_else(|| value.as_str()?.trim().parse::<i64>().ok());
    if let Some(number) = number.filter(|value| *value > 0) {
        return if number < 100_000_000_000 {
            number.checked_mul(1000)
        } else {
            Some(number)
        };
    }
    chrono::DateTime::parse_from_rfc3339(value.as_str()?)
        .ok()
        .map(|time| time.timestamp_millis())
}
