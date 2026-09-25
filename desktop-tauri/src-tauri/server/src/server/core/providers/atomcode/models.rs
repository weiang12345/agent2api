//! AtomCode CodingPlan 模型目录、套餐状态与用量。

use std::sync::{OnceLock, RwLock};

use serde_json::{json, Value};

use crate::server::core::auth_http::{send_request_via, ApiResponse};
use crate::server::core::providers::adapter::ModelRefreshOutcome;
use crate::server::core::proxies::ResolvedProxy;
use crate::server::errors::GatewayError;
use crate::server::logging;

use super::credentials::Credentials;

const CODINGPLAN_BASE_URL: &str = "https://api.gitcode.com/api/v5";
const REQUEST_TIMEOUT_MS: u64 = 20_000;
const DEFAULT_CONTEXT_WINDOW: i64 = 64_000;
const DEFAULT_OUTPUT_TOKENS: i64 = 8_192;

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
    let status = match status(credentials, proxy).await {
        Ok(value) => value,
        Err(error) => return ModelRefreshOutcome::failed(error.message),
    };
    let plan_type = plan_type(&status);
    let path = format!(
        "/coding-plan/models-v2?plan_type={}",
        url_encode(&plan_type)
    );
    let payload = match request("GET", &path, None, credentials, proxy).await {
        Ok(value) => value,
        Err(error) => return ModelRefreshOutcome::failed(error.message),
    };
    let models = parse_models(&payload);
    if models.is_empty() {
        return ModelRefreshOutcome::failed("AtomCode 未返回可用模型");
    }
    let count = models.len();
    let state = CatalogState {
        models,
        fetched_at: logging::now_ms(),
    };
    if let Ok(mut guard) = catalog().write() {
        *guard = state;
    } else if let Err(poisoned) = catalog().write() {
        *poisoned.into_inner() = state;
    }
    logging::log(
        "[Models]",
        &format!("✅ AtomCode 模型目录已更新（{count} 个）"),
    );
    ModelRefreshOutcome::refreshed(count)
}

pub async fn status(
    credentials: &Credentials,
    proxy: Option<&ResolvedProxy>,
) -> Result<Value, GatewayError> {
    request("GET", "/coding-plan/status-v2", None, credentials, proxy).await
}

pub async fn usage(
    credentials: &Credentials,
    proxy: Option<&ResolvedProxy>,
) -> Result<Value, GatewayError> {
    let raw = request("GET", "/coding-plan/usage", None, credentials, proxy).await?;
    Ok(normalize_usage(&raw))
}

pub async fn claim(
    credentials: &Credentials,
    proxy: Option<&ResolvedProxy>,
) -> Result<Value, GatewayError> {
    for tier in ["Max", "Pro", "Lite"] {
        let body = json!({ "plan_type": tier });
        let response = request_raw(
            "POST",
            "/coding-plan/claim-v2",
            Some(&body),
            credentials,
            proxy,
        )
        .await?;
        if response.ok {
            let payload = response.payload.unwrap_or(Value::Null);
            if payload.get("success").and_then(Value::as_bool) == Some(true)
                || payload.get("duplicate").and_then(Value::as_bool) == Some(true)
            {
                return Ok(payload);
            }
        }
    }
    Err(GatewayError::with_status(
        502,
        "AtomCode CodingPlan 领取失败",
    ))
}

fn parse_models(payload: &Value) -> Vec<Value> {
    let Some(models) = payload.as_array() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for model in models {
        let Some(id) = model
            .get("display_model_name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        if model.get("plan_available").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        let context_window = model
            .get("context_window")
            .and_then(Value::as_i64)
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_CONTEXT_WINDOW);
        let supports_images = model
            .get("supports_vision")
            .or_else(|| model.get("is_vl"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        out.push(json!({
            "id": id,
            "name": id,
            "maxInputTokens": context_window,
            "maxOutputTokens": DEFAULT_OUTPUT_TOKENS,
            "supportsImages": supports_images,
            "supportsReasoning": false,
            "supportsToolCall": true,
            "enabled": true,
            "isDefault": false,
            "kind": "chat",
        }));
    }
    out
}

fn normalize_usage(raw: &Value) -> Value {
    let plan = raw.get("codingplan_free").cloned().unwrap_or(Value::Null);
    let current_usage = raw.get("current_usage").cloned().unwrap_or(Value::Null);
    let available = current_usage
        .get("window_token_limit")
        .and_then(Value::as_i64)
        .zip(
            current_usage
                .get("window_tokens_used")
                .and_then(Value::as_i64),
        )
        .map(|(limit, used)| (limit - used).max(0));
    let remain_quota = plan.get("remaining_days").and_then(Value::as_i64);
    let total_quota = plan.get("total_days").and_then(Value::as_i64);
    json!({
        "available": available,
        "unit": "tokens",
        "wallets": [],
        "subscription": {
            "planName": plan.get("plan_name").cloned().unwrap_or(Value::Null),
            "status": plan.get("status").cloned().unwrap_or(Value::Null),
            "expireAt": plan.get("expires_at").cloned().unwrap_or(Value::Null),
            "remainQuota": remain_quota,
            "totalQuota": total_quota,
        },
        "raw": raw,
    })
}

fn plan_type(status: &Value) -> String {
    let name = status
        .pointer("/codingplan_free/plan_name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_lowercase();
    if name.contains("max") {
        "Max".to_string()
    } else if name.contains("pro") {
        "Pro".to_string()
    } else {
        "Lite".to_string()
    }
}

async fn request(
    method: &str,
    path: &str,
    body: Option<&Value>,
    credentials: &Credentials,
    proxy: Option<&ResolvedProxy>,
) -> Result<Value, GatewayError> {
    let response = request_raw(method, path, body, credentials, proxy).await?;
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
            format!("AtomCode 请求失败：{message}"),
        ));
    }
    response
        .payload
        .ok_or_else(|| GatewayError::with_status(502, "AtomCode 接口未返回有效 JSON"))
}

async fn request_raw(
    method: &str,
    path: &str,
    body: Option<&Value>,
    credentials: &Credentials,
    proxy: Option<&ResolvedProxy>,
) -> Result<ApiResponse, GatewayError> {
    let url = format!("{CODINGPLAN_BASE_URL}{path}");
    let headers = vec![
        (
            "Authorization".to_string(),
            format!("Bearer {}", credentials.access_token),
        ),
        ("User-Agent".to_string(), "atomcode/5.0.2".to_string()),
    ];
    send_request_via(
        method,
        &url,
        body,
        &headers,
        proxy,
        Some(REQUEST_TIMEOUT_MS),
    )
    .await
    .map_err(|error| GatewayError::with_status(502, format!("AtomCode 请求失败：{error}")))
}

fn url_encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}
