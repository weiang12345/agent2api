//! Trae 凭证与设备标识。
use serde_json::{Map, Value};

use crate::server::core::account_store::MAX_TOKEN_LENGTH;
use crate::server::errors::GatewayError;
use crate::server::logging;

/// 预刷新窗口：24 小时。参考实现实测用 24h，比常规 OAuth 5 分钟更积极。
pub const REFRESH_MARGIN_MS: i64 = 24 * 60 * 60 * 1000;

#[derive(Clone, Debug)]
pub struct Credentials {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: Option<i64>,
    pub user_id: String,
    pub nickname: String,
    pub enterprise_id: String,
    pub machine_id: String,
    pub device_id: String,
}

impl Credentials {
    pub fn from_payload(payload: &Value) -> Result<Self, GatewayError> {
        if !payload.is_object() {
            return Err(GatewayError::with_status(400, "Trae 账号内容必须是 JSON 对象"));
        }
        let access_token = secret(payload, &["accessToken", "access_token", "access", "token"])?;
        if access_token.is_empty() {
            return Err(GatewayError::with_status(400, "缺少 Trae accessToken"));
        }
        let refresh_token = secret(payload, &["refreshToken", "refresh_token", "refresh"])?;
        let expires_at = ["expiresAt", "expires_at", "expires"]
            .iter()
            .find_map(|key| timestamp(payload.get(*key)));
        let mut credentials = Self {
            access_token,
            refresh_token,
            expires_at,
            user_id: text(payload, &["userId", "user_id", "uid"]),
            nickname: text(payload, &["nickname", "name", "screenName"]).chars().take(100).collect(),
            enterprise_id: text(payload, &["enterpriseId", "tenantId"]).chars().take(128).collect(),
            machine_id: text(payload, &["machineId", "machine_id"]),
            device_id: text(payload, &["deviceId", "device_id"]),
        };
        credentials.complete_identity()?;
        Ok(credentials)
    }

    pub fn complete_identity(&mut self) -> Result<(), GatewayError> {
        if self.user_id.is_empty() {
            return Err(GatewayError::with_status(400, "缺少 Trae 用户标识，无法保存账号"));
        }
        if self.machine_id.is_empty() {
            self.machine_id = random_hex(16)?;
        }
        if self.device_id.is_empty() {
            self.device_id = random_numeric_device_id()?;
        }
        if self.machine_id.len() > 128
            || self.device_id.len() > 64
            || self.machine_id.chars().any(char::is_control)
            || self.device_id.chars().any(char::is_control)
        {
            return Err(GatewayError::with_status(400, "Trae 设备标识无效"));
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
        let mut object = Map::new();
        let text = |value: &str| -> Value {
            if value.is_empty() { Value::Null } else { Value::String(value.to_string()) }
        };
        object.insert("accessToken".into(), text(&self.access_token));
        object.insert("refreshToken".into(), text(&self.refresh_token));
        object.insert("expiresAt".into(), self.expires_at.map(Value::from).unwrap_or(Value::Null));
        object.insert("userId".into(), text(&self.user_id));
        object.insert("nickname".into(), text(&self.nickname));
        object.insert("enterpriseId".into(), text(&self.enterprise_id));
        object.insert("machineId".into(), text(&self.machine_id));
        object.insert("deviceId".into(), text(&self.device_id));
        Value::Object(object)
    }

    pub fn from_parts(
        access_token: String,
        refresh_token: String,
        expires_at: Option<i64>,
        user_id: String,
        nickname: String,
        enterprise_id: String,
    ) -> Result<Self, GatewayError> {
        let mut credentials = Self {
            access_token,
            refresh_token,
            expires_at,
            user_id,
            nickname,
            enterprise_id,
            machine_id: String::new(),
            device_id: String::new(),
        };
        credentials.complete_identity()?;
        Ok(credentials)
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
        return Err(GatewayError::with_status(400, "Trae 凭证过长或包含非法控制字符"));
    }
    Ok(value.to_string())
}

pub fn timestamp(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    let number = value.as_i64().or_else(|| value.as_str()?.trim().parse::<i64>().ok())?;
    if number <= 0 {
        return None;
    }
    // Agent2API 统一毫秒；Trae ExchangeToken 给的是秒或毫秒。
    if number < 1_000_000_000_000 {
        number.checked_mul(1000)
    } else {
        Some(number)
    }
}

pub fn random_hex(bytes: usize) -> Result<String, GatewayError> {
    let mut data = vec![0u8; bytes];
    getrandom::getrandom(&mut data)
        .map_err(|_| GatewayError::with_status(500, "无法生成 Trae machine_id"))?;
    Ok(data.iter().map(|byte| format!("{byte:02x}")).collect())
}

pub fn random_numeric_device_id() -> Result<String, GatewayError> {
    let mut data = [0u8; 16];
    getrandom::getrandom(&mut data)
        .map_err(|_| GatewayError::with_status(500, "无法生成 Trae device_id"))?;
    let mut digits = String::with_capacity(16);
    digits.push((b'1' + data[0] % 9) as char);
    for byte in &data[1..] {
        digits.push((b'0' + byte % 10) as char);
    }
    Ok(digits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_trae_credentials() {
        let credentials = Credentials::from_payload(&json!({
            "accessToken": "Bearer access-token",
            "refreshToken": "refresh-token",
            "expiresAt": 4102444800_i64,
            "userId": "user-1",
            "nickname": "Trae User",
            "machineId": "0123456789abcdef0123456789abcdef",
            "deviceId": "1234567890123456",
        }))
        .expect("parse Trae credentials");
        assert_eq!(credentials.access_token, "access-token");
        assert_eq!(credentials.expires_at, Some(4102444800000));
        assert!(credentials.can_refresh());
        assert!(!credentials.expiring());
    }

    #[test]
    fn rejects_empty_access_token() {
        let error = Credentials::from_payload(&json!({})).expect_err("missing access token");
        assert_eq!(error.status_code, 400);
    }
}
