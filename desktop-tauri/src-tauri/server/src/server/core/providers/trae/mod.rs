//! Trae SOLO CN 云直连 provider。

pub mod credentials;
pub mod models;
pub mod oauth;
pub mod protocol;
pub mod stream;

use axum::http::HeaderMap;
use serde_json::Value;

use crate::server::core::account_store::AccountStore;
use crate::server::core::egress;
use crate::server::core::providers::adapter::{
    ChatRequestPlan, ModelRefreshOutcome, ProviderAdapter, UpstreamErrorClass,
};
use crate::server::core::providers::ProviderKind;
use crate::server::core::proxies::{resolve_account_proxy, ProxyResolution, ResolvedProxy};
use crate::server::core::upstream::usage::RequestTelemetry;
use crate::server::errors::GatewayError;
use crate::server::logging;

use self::credentials::Credentials;

pub struct TraeAdapter;
pub static TRAE_ADAPTER: TraeAdapter = TraeAdapter;

impl ProviderAdapter for TraeAdapter {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Trae
    }

    fn supports_chat(&self) -> bool {
        true
    }

    fn is_stateful(&self) -> bool {
        true
    }

    fn list_models(&self) -> Vec<Value> {
        models::list()
    }

    fn build_chat_request(
        &self,
        account: &Value,
        body: &Value,
        _client_headers: &HeaderMap,
    ) -> Result<ChatRequestPlan, GatewayError> {
        let token = account
            .get("auth")
            .and_then(|auth| auth.get("accessToken"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let user_id = account
            .get("account")
            .and_then(|account| account.get("uid"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        if token.is_empty() {
            return Err(GatewayError::with_status(401, "Trae 账号缺少 accessToken，无法转发"));
        }
        let machine_id = account
            .get("auth")
            .and_then(|auth| auth.get("machineId"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let device_id = account
            .get("auth")
            .and_then(|auth| auth.get("deviceId"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let prepared = protocol::prepare_body(body, false)?;
        let headers = protocol::solo_headers(token, user_id, machine_id, device_id, true);
        Ok(ChatRequestPlan {
            url: format!("{}/api/agent/v3/llm_utils_chat", protocol::AGENT_HOST),
            headers,
            body: prepared,
        })
    }

    fn classify_error(&self, status: u16, error_body: &Value) -> UpstreamErrorClass {
        let code = error_body.get("code").and_then(Value::as_i64);
        let raw = error_body
            .get("message")
            .or_else(|| error_body.get("msg"))
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .unwrap_or("上游错误");
        let message = format!("上游返回 {status}: {raw}");
        if status == 401 || code == Some(1001) {
            return UpstreamErrorClass::TokenExpired { message };
        }
        if status == 429 || code == Some(4008) || code == Some(4010) {
            return UpstreamErrorClass::QuotaLimited {
                reset_at: None,
                message,
                upstream_code: code,
                status: if status == 0 { 429 } else { status },
            };
        }
        UpstreamErrorClass::Fatal { status, message, upstream_code: code }
    }

    fn ensure_access_token<'a>(
        &'a self,
        store: &'a AccountStore,
        account_id: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, GatewayError>> + Send + 'a>> {
        Box::pin(async move {
            let credentials = credentials_for(store, account_id)?;
            let refreshed = refresh_if_needed(store, account_id, &credentials, false).await?;
            Ok(refreshed.access_token)
        })
    }

    fn refresh_access_token<'a>(
        &'a self,
        store: &'a AccountStore,
        account_id: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, GatewayError>> + Send + 'a>> {
        Box::pin(async move {
            let credentials = credentials_for(store, account_id)?;
            let refreshed = refresh_if_needed(store, account_id, &credentials, true).await?;
            Ok(refreshed.access_token)
        })
    }

    fn supports_refresh(&self) -> bool {
        true
    }

    fn credentials_expiring(&self, store: &AccountStore, account_id: &str) -> bool {
        store
            .trae_account_record(account_id)
            .and_then(|record| Credentials::from_payload(&record).ok())
            .is_some_and(|credentials| credentials.expiring())
    }

    fn supports_usage(&self) -> bool {
        true
    }


    fn query_usage<'a>(
        &'a self,
        store: &'a AccountStore,
        account_id: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, GatewayError>> + Send + 'a>> {
        Box::pin(async move {
            let credentials = credentials_for(store, account_id)?;
            let credentials = refresh_if_needed(store, account_id, &credentials, false).await?;
            let proxy = account_proxy(store, account_id)?;
            models::usage(&credentials, proxy.as_ref()).await
        })
    }

    fn supports_model_refresh(&self) -> bool {
        true
    }

    fn refresh_models<'a>(
        &'a self,
        store: &'a AccountStore,
        force: bool,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ModelRefreshOutcome> + Send + 'a>> {
        Box::pin(async move {
            let Some(record) = store.trae_account_record("") else {
                logging::verbose("[Models]", "Trae 模型目录刷新跳过：尚未添加账号");
                return ModelRefreshOutcome::unchanged();
            };
            let Ok(credentials) = Credentials::from_payload(&record) else {
                return ModelRefreshOutcome::failed("Trae 账号凭证无效，请重新登录");
            };
            let credentials = match refresh_if_needed(store, "", &credentials, false).await {
                Ok(value) => value,
                Err(error) => return ModelRefreshOutcome::failed(error.message),
            };
            let proxy = match account_proxy(store, "") {
                Ok(value) => value,
                Err(error) => return ModelRefreshOutcome::failed(error.message),
            };
            models::refresh(&credentials, proxy.as_ref(), force).await
        })
    }

    fn supports_default_model(&self) -> bool {
        true
    }

    fn sse_model_rewrite(&self) -> bool {
        false
    }

    fn forward_conversation<'a>(
        &'a self,
        store: &'a AccountStore,
        account_id: &'a str,
        body: &'a Value,
        _client_headers: &'a HeaderMap,
        proxy: Option<crate::server::core::proxies::ResolvedProxy>,
        stream: bool,
        _telemetry: &'a std::sync::Arc<RequestTelemetry>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                Output = Result<crate::server::core::upstream::ForwardOutcome, GatewayError>,
            > + Send
            + 'a,
        >,
    > {
        Box::pin(async move {
            let credentials = credentials_for(store, account_id)?;
            let credentials = refresh_if_needed(store, account_id, &credentials, false).await?;
            let plan = self.chat_plan(&credentials, body)?;
            let mut builder = egress::client_for(proxy.as_ref())
                .post(plan.url)
                .body(serde_json::to_vec(&plan.body).map_err(|_| {
                    GatewayError::with_status(500, "Trae 请求体序列化失败")
                })?);
            for (key, value) in plan.headers {
                builder = builder.header(key, value);
            }
            let response = builder.send().await.map_err(|error| {
                GatewayError::with_status(
                    502,
                    format!(
                        "Trae 上游请求失败：{}",
                        egress::describe_error_detail(&error)
                    ),
                )
            })?;
            if !response.status().is_success() {
                let status = response.status().as_u16();
                let text = response.text().await.unwrap_or_default();
                return Err(GatewayError::with_status(
                    i32::from(status),
                    format!("Trae 上游返回 {status}: {}", text.chars().take(300).collect::<String>()),
                ));
            }

            let mut parser = stream::SoloSseParser::default();
            let mut translator = stream::Translator::new(&plan.model);
            let mut source = response.bytes_stream();
            let mut frames: Vec<Value> = Vec::new();
            while let Some(chunk) = futures::StreamExt::next(&mut source).await {
                let chunk = chunk.map_err(|error| {
                    GatewayError::with_status(
                        502,
                        format!(
                            "Trae 上游流中断：{}",
                            egress::describe_error_detail(&error)
                        ),
                    )
                })?;
                for event in parser.push(&chunk) {
                    frames.extend(translator.consume(event)?);
                }
            }
            for event in parser.finish() {
                frames.extend(translator.consume(event)?);
            }
            frames.extend(translator.finish_frames());

            if stream {
                let (sender, receiver) =
                    tokio::sync::mpsc::channel::<Result<bytes::Bytes, std::io::Error>>(64);
                tokio::spawn(async move {
                    for frame in frames {
                        let text = stream::sse_frame(&frame);
                        if sender.send(Ok(bytes::Bytes::from(text))).await.is_err() {
                            return;
                        }
                    }
                    let _ = sender
                        .send(Ok(bytes::Bytes::from(stream::sse_done())))
                        .await;
                });
                return Ok(crate::server::core::upstream::ForwardOutcome::Stream {
                    status: 200,
                    stream: Box::new(tokio_stream::wrappers::ReceiverStream::new(receiver)),
                });
            }
            Ok(crate::server::core::upstream::ForwardOutcome::Completion {
                body: translator.completion(),
            })
        })
    }
}

struct TraeChatPlan {
    url: String,
    headers: Vec<(String, String)>,
    body: Value,
    model: String,
}

impl TraeAdapter {
    fn chat_plan(
        &self,
        credentials: &Credentials,
        body: &Value,
    ) -> Result<TraeChatPlan, GatewayError> {
        let model = body
            .get("model")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(protocol::DEFAULT_MODEL)
            .to_string();
        let prepared = protocol::prepare_body(body, true)?;
        let headers = protocol::solo_headers(
            &credentials.access_token,
            &credentials.user_id,
            &credentials.machine_id,
            &credentials.device_id,
            true,
        );
        Ok(TraeChatPlan {
            url: format!("{}/api/agent/v3/llm_utils_chat", protocol::AGENT_HOST),
            headers,
            body: prepared,
            model,
        })
    }
}

pub fn credentials_for(store: &AccountStore, account_id: &str) -> Result<Credentials, GatewayError> {
    let record = store
        .trae_account_record(account_id)
        .ok_or_else(|| GatewayError::with_status(401, "没有可用的 Trae 账号"))?;
    Credentials::from_payload(&record)
}

pub async fn refresh_if_needed(
    store: &AccountStore,
    account_id: &str,
    credentials: &Credentials,
    force: bool,
) -> Result<Credentials, GatewayError> {
    if !force && !credentials.expiring() {
        return Ok(credentials.clone());
    }
    let refreshed = oauth::refresh(credentials).await?;
    if let Some(expected) = store.trae_account_record(account_id) {
        let _ = store.update_trae_credentials_if_current(&expected, &refreshed);
    }
    Ok(refreshed)
}

pub fn account_proxy(
    store: &AccountStore,
    account_id: &str,
) -> Result<Option<ResolvedProxy>, GatewayError> {
    let record = store
        .trae_account_record(account_id)
        .ok_or_else(|| GatewayError::with_status(401, "没有可用的 Trae 账号"))?;
    match resolve_account_proxy(record.get("proxy")) {
        Some(ProxyResolution::Resolved(proxy)) => Ok(Some(proxy)),
        Some(ProxyResolution::Failed(reason)) => Err(GatewayError::with_status(400, reason)),
        None => Ok(None),
    }
}
