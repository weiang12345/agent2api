//! Trae SOLO 模型目录与额度。
use std::sync::{OnceLock, RwLock};

use serde_json::{json, Value};

use crate::server::core::auth_http::{send_request_via, ApiResponse};
use crate::server::core::providers::adapter::ModelRefreshOutcome;
use crate::server::core::proxies::ResolvedProxy;
use crate::server::errors::GatewayError;
use crate::server::logging;

use super::credentials::Credentials;
use super::protocol::{solo_headers, AGENT_HOST, FUNCTION, UG_HOST};

const REQUEST_TIMEOUT_MS: u64 = 30_000;

#[derive(Clone, Default)]
struct CatalogState {
    models: Vec<Value>,
    fetched_at: i64,
}

fn catalog() -> &'static RwLock<CatalogState> {
    static CATALOG: OnceLock<RwLock<CatalogState>> = OnceLock::new();
    CATALOG.get_or_init(|| RwLock::new(CatalogState::default()))
}

fn read_state() -> CatalogState {
    match catalog().read() {
        Ok(guard) => guard.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}

pub fn list() -> Vec<Value> {
    read_state().models
}

pub fn remote_refreshed() -> bool {
    !read_state().models.is_empty()
}

pub fn last_refreshed_at() -> i64 {
    read_state().fetched_at
}

pub async fn refresh(
    credentials: &Credentials,
    proxy: Option<&ResolvedProxy>,
    force: bool,
) -> ModelRefreshOutcome {
    if !force {
        let state = read_state();
        if !state.models.is_empty() && logging::now_ms() - state.fetched_at < 60 * 60 * 1000 {
            return ModelRefreshOutcome::unchanged();
        }
    }
    let payload = match request(
        "POST",
        &format!("{AGENT_HOST}/api/ide/v1/get_detail_param"),
        Some(&json!({
            "function": FUNCTION,
            "config_names": null,
            "need_prompt": false,
            "current_config_info": null,
            "poly_prompt": true,
            "mode_type": null,
            "agent_type": null,
        })),
        credentials,
        proxy,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return ModelRefreshOutcome::failed(error.message),
    };
    let models = parse_models(&payload);
    if models.is_empty() {
        return ModelRefreshOutcome::failed("Trae 未返回可用模型");
    }
    let count = models.len();
    let state = CatalogState { models, fetched_at: logging::now_ms() };
    match catalog().write() {
        Ok(mut guard) => *guard = state,
        Err(poisoned) => *poisoned.into_inner() = state,
    }
    logging::log("[Models]", &format!("✅ Trae 模型目录已更新（{count} 个）"));
    ModelRefreshOutcome::refreshed(count)
}

pub async fn usage(
    credentials: &Credentials,
    proxy: Option<&ResolvedProxy>,
) -> Result<Value, GatewayError> {
    let raw = request(
        "POST",
        &format!("{UG_HOST}/trae/api/v2/pay/ide_user_ent_usage"),
        Some(&json!({})),
        credentials,
        proxy,
    )
    .await?;
    let (remain, limit, used, packs) = parse_usage(&raw);
    Ok(json!({
        "available": remain,
        "unit": "积分",
        "wallets": [],
        "subscription": {
            "remainQuota": remain,
            "totalQuota": limit,
            "usedQuota": used,
            "packs": packs,
        },
        "raw": raw,
    }))
}

fn parse_models(payload: &Value) -> Vec<Value> {
    let Some(list) = payload.get("config_info_list").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in list {
        let Some(id) = item.get("config_name").and_then(Value::as_str).map(str::trim).filter(|value| !value.is_empty()) else {
            continue;
        };
        let name = item
            .pointer("/display_config/display_name")
            .and_then(Value::as_str)
            .unwrap_or(id);
        let detail = item
            .get("model_detail_list")
            .and_then(Value::as_array)
            .and_then(|list| list.first())
            .cloned()
            .unwrap_or(Value::Null);
        let max_input = detail
            .get("prompt_max_tokens")
            .and_then(Value::as_i64)
            .filter(|value| *value > 0)
            .unwrap_or(128_000);
        let max_output = detail
            .get("max_tokens")
            .and_then(Value::as_i64)
            .filter(|value| *value > 0)
            .unwrap_or(8_192);
        out.push(json!({
            "id": id,
            "name": name,
            "maxInputTokens": max_input,
            "maxOutputTokens": max_output,
            "supportsImages": false,
            "supportsReasoning": false,
            "supportsToolCall": true,
            "enabled": true,
            "isDefault": id == super::protocol::DEFAULT_MODEL,
            "kind": "chat",
        }));
    }
    out
}

fn parse_usage(raw: &Value) -> (i64, i64, i64, i64) {
    let Some(packs) = raw
        .get("user_entitlement_pack_list")
        .and_then(Value::as_array)
    else {
        return (0, 0, 0, 0);
    };
    let mut remain = 0;
    let mut limit = 0;
    let mut used = 0;
    let mut pack_count = 0;
    for pack in packs {
        let current_limit = pack.pointer("/entitlement_base_info/quota/credits_limit").and_then(Value::as_i64).unwrap_or(0);
        if current_limit <= 0 {
            continue;
        }
        let current_used = pack.pointer("/usage/credits_amount").and_then(Value::as_f64).map(|value| value as i64).unwrap_or(0);
        limit += current_limit;
        used += current_used;
        remain += current_limit - current_used;
        pack_count += 1;
    }
    (remain, limit, used, pack_count)
}

async fn request(
    method: &str,
    url: &str,
    body: Option<&Value>,
    credentials: &Credentials,
    proxy: Option<&ResolvedProxy>,
) -> Result<Value, GatewayError> {
    let response = request_raw(method, url, body, credentials, proxy).await?;
    if !response.ok {
        let message = response
            .payload
            .as_ref()
            .and_then(|value| value.get("message").or_else(|| value.get("msg")))
            .and_then(Value::as_str)
            .unwrap_or("上游返回非 2xx");
        return Err(GatewayError::with_status(
            i32::from(response.status),
            format!("Trae 请求失败：{message}"),
        ));
    }
    response
        .payload
        .ok_or_else(|| GatewayError::with_status(502, "Trae 接口未返回有效 JSON"))
}

async fn request_raw(
    method: &str,
    url: &str,
    body: Option<&Value>,
    credentials: &Credentials,
    proxy: Option<&ResolvedProxy>,
) -> Result<ApiResponse, GatewayError> {
    let headers = solo_headers(
        &credentials.access_token,
        &credentials.user_id,
        &credentials.machine_id,
        &credentials.device_id,
        false,
    );
    send_request_via(method, url, body, &headers, proxy, Some(REQUEST_TIMEOUT_MS))
        .await
        .map_err(|error| GatewayError::with_status(502, format!("Trae 请求失败：{error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_model_catalog() {
        let payload = json!({
            "config_info_list": [
                {
                    "config_name": "glm-5.2",
                    "display_config": { "display_name": "GLM 5.2" },
                    "model_detail_list": [{ "prompt_max_tokens": 200000, "max_tokens": 8192 }]
                },
                { "config_name": "" }
            ]
        });
        let models = parse_models(&payload);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0]["id"], "glm-5.2");
        assert_eq!(models[0]["maxInputTokens"], 200000);
        assert_eq!(models[0]["isDefault"], true);
    }

    #[test]
    fn parses_usage() {
        let raw = json!({
            "user_entitlement_pack_list": [
                {
                    "entitlement_base_info": { "quota": { "credits_limit": 100 } },
                    "usage": { "credits_amount": 25.5 }
                }
            ]
        });
        assert_eq!(parse_usage(&raw), (75, 100, 25, 1));
    }
}
