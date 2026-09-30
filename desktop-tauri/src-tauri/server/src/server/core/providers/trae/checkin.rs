//! Trae 的每日签到（`checkin_credits`）。
//!
//! 这是 fork 在上游 Trae 实现上的唯一增量：登录、续期、模型、转发和余额
//! 都复用上游模块。签到走独立的 ug 接口，并用 uid 派生一个 16 位设备号。
//!
//! 9074 表示签到人数过多。旧参考实现曾在同一次请求里轮换设备号并自动重试，
//! 这会把出口 IP 打进风控；这里每次签到最多发 status + claim 两个请求，遇到
//! 9074 只落一个新的 generation，下一次手动点击或定时任务才使用新设备号。

use std::time::Duration;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::server::core::account_store::AccountStore;
use crate::server::core::proxies::ResolvedProxy;
use crate::server::errors::GatewayError;

use super::adapter::{account_proxy, read_record, renew_if_due};
use super::credentials::Credential;
use super::http::{post_json, Reply};
use super::usage::UG_HOST;

const STATUS_PATH: &str = "/trae/api/v2/ug/checkin_credits/status";
const CLAIM_PATH: &str = "/trae/api/v2/ug/checkin_credits/claim";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const BUSY_CODE: i64 = 9074;
const ERROR_BODY_HEAD: usize = 200;

/// 给指定账号领取 Trae 每日签到积分。
pub async fn claim_daily_checkin(
    store: &AccountStore,
    account_id: &str,
) -> Result<Value, GatewayError> {
    let record = read_record(store, account_id)?;
    let credential = Credential::from_payload(&record).map_err(GatewayError::new)?;
    let proxy = account_proxy(&record)?;
    let credential = renew_if_due(store, &record, &credential, proxy.as_ref()).await?;
    if credential.access_token.trim().is_empty() {
        return Err(GatewayError::with_status(
            401,
            "Trae 账号缺少 accessToken，无法签到（请重新登录）",
        ));
    }
    let uid = credential.uid.trim();
    if uid.is_empty() {
        return Err(GatewayError::with_status(
            400,
            "Trae 账号缺少 uid，无法生成签到设备号",
        ));
    }

    let generation = store.trae_checkin_generation(account_id);
    let device_id = checkin_device_id(uid, generation);
    let status = post_checkin(&credential, &device_id, STATUS_PATH, proxy.as_ref()).await?;
    if is_busy(&status) {
        return Ok(busy_result(store, account_id));
    }
    if let Some(result) = status_result(&status) {
        return Ok(result);
    }

    let claim = post_checkin(&credential, &device_id, CLAIM_PATH, proxy.as_ref()).await?;
    if is_busy(&claim) {
        return Ok(busy_result(store, account_id));
    }
    Ok(claim_result(&claim))
}

async fn post_checkin(
    credential: &Credential,
    device_id: &str,
    path: &str,
    proxy: Option<&ResolvedProxy>,
) -> Result<Value, GatewayError> {
    let headers = vec![
        (
            "Authorization",
            format!("Cloud-IDE-JWT {}", credential.access_token.trim()),
        ),
        ("X-Device-Id", device_id.to_string()),
        ("X-Device-Brand", "Apple".to_string()),
        ("X-Device-Type", "windows".to_string()),
    ];
    let reply = post_json(
        &format!("{UG_HOST}{path}"),
        &json!({}),
        &headers,
        REQUEST_TIMEOUT,
        proxy,
    )
    .await?;
    if reply.status >= 400 {
        return Err(reply_error(reply));
    }
    reply
        .json()
        .ok_or_else(|| GatewayError::with_status(502, "Trae 签到接口未返回有效 JSON"))
}

fn reply_error(reply: Reply) -> GatewayError {
    let body = reply.json();
    let message = body
        .as_ref()
        .and_then(|value| upstream_message(value))
        .unwrap_or_else(|| body_head(&reply.body));
    GatewayError::with_status(i32::from(reply.status), format!("Trae 签到失败：{message}"))
}

fn body_head(body: &str) -> String {
    let text: String = body.trim().chars().take(ERROR_BODY_HEAD).collect();
    if text.is_empty() {
        "未返回错误详情".to_string()
    } else {
        text
    }
}

fn is_busy(payload: &Value) -> bool {
    payload.get("code").and_then(Value::as_i64) == Some(BUSY_CODE)
}

fn status_result(payload: &Value) -> Option<Value> {
    let code = payload.get("code").and_then(Value::as_i64).unwrap_or(0);
    if code != 0 {
        return Some(failed_result(payload, "签到状态查询失败"));
    }
    if payload.get("checked_in").and_then(Value::as_bool) == Some(true) {
        return Some(json!({
            "success": false,
            "alreadyCompleted": true,
            "msg": "今日已签到",
        }));
    }
    if payload.get("enable").and_then(Value::as_bool) == Some(false) {
        return Some(json!({
            "success": false,
            "msg": "当前账号暂未开启签到活动",
        }));
    }
    None
}

fn claim_result(payload: &Value) -> Value {
    let code = payload.get("code").and_then(Value::as_i64).unwrap_or(0);
    if code == 0 {
        return json!({
            "success": true,
            "msg": "签到成功",
        });
    }
    failed_result(payload, "签到未领取")
}

fn failed_result(payload: &Value, fallback: &str) -> Value {
    json!({
        "success": false,
        "msg": upstream_message(payload).unwrap_or_else(|| fallback.to_string()),
    })
}

fn upstream_message(payload: &Value) -> Option<String> {
    payload
        .get("message")
        .or_else(|| payload.get("msg"))
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .map(str::to_string)
}

fn busy_result(store: &AccountStore, account_id: &str) -> Value {
    let generation = store.bump_trae_checkin_generation(account_id);
    if generation > 0 {
        crate::server::logging::log(
            "[Accounts]",
            &format!("Trae 签到人数过多，已轮换到第 {generation} 代签到设备"),
        );
    } else {
        crate::server::logging::verbose(
            "[Accounts]",
            "Trae 签到人数过多，但签到设备 generation 未能落盘",
        );
    }
    json!({
        "success": false,
        "code": BUSY_CODE,
        "msg": "签到人数过多，稍后再试",
    })
}

fn checkin_device_id(identity: &str, generation: u64) -> String {
    if identity.is_empty() {
        return String::new();
    }
    let material = if generation == 0 {
        identity.to_string()
    } else {
        format!("{identity}#gen{generation}")
    };
    let digest: [u8; 32] = Sha256::digest(material.as_bytes()).into();
    let modulus: u64 = 10_000_000_000_000_000;
    let mut remainder: u64 = 0;
    for byte in digest {
        remainder = (remainder << 8) | u64::from(byte);
        remainder %= modulus;
    }
    format!("{remainder:016}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_id_matches_the_reference_vector() {
        assert_eq!("4302850041909017", checkin_device_id("u1", 0));
    }

    #[test]
    fn each_generation_gets_a_different_device_id() {
        assert_ne!(checkin_device_id("u1", 0), checkin_device_id("u1", 1));
    }

    #[test]
    fn status_parser_handles_completed_and_failure() {
        let completed = status_result(&json!({
            "code": 0,
            "checked_in": true,
            "enable": true,
        }))
        .expect("completed is terminal");
        assert_eq!(completed["alreadyCompleted"], json!(true));

        let failure = status_result(&json!({
            "code": 1001,
            "message": "invalid token",
        }))
        .expect("non-zero code is terminal");
        assert_eq!(failure["msg"], json!("invalid token"));
    }

    #[test]
    fn disabled_checkin_is_terminal() {
        let result = status_result(&json!({
            "code": 0,
            "checked_in": false,
            "enable": false,
        }))
        .expect("disabled checkin should not send claim");
        assert_eq!(result["msg"], json!("当前账号暂未开启签到活动"));
    }

    #[test]
    fn claim_parser_handles_success_and_failure() {
        let success = claim_result(&json!({ "code": 0, "message": "ok" }));
        assert_eq!(success["success"], json!(true));

        let failure = claim_result(&json!({ "code": 1002, "msg": "not allowed" }));
        assert_eq!(failure["success"], json!(false));
        assert_eq!(failure["msg"], json!("not allowed"));
    }

    #[test]
    fn busy_payloads_are_recognized_without_retrying() {
        assert!(is_busy(&json!({ "code": 9074, "message": "busy" })));
        assert!(!is_busy(&json!({ "code": 0 })));
    }

    #[test]
    fn non_json_error_bodies_are_truncated() {
        let long = "x".repeat(300);
        assert_eq!(200, body_head(&long).chars().count());
        assert_eq!("未返回错误详情", body_head("  "));
    }
}
