//! Trae SOLO CN 请求头与 OpenAI 请求体归一化。

use serde_json::{json, Map, Value};

use crate::server::errors::GatewayError;

pub const AGENT_HOST: &str = "https://trae-api-cn.mchost.guru";
pub const UG_HOST: &str = "https://api.trae.cn";
pub const OAUTH_HOST: &str = "https://api.trae.com.cn";
pub const CONSOLE_HOST: &str = "https://www.trae.cn";
pub const CLIENT_ID: &str = "en1oxy7wnw8j9n";
pub const APP_ID: &str = "6eefa01c-1036-4c7e-9ca5-d891f63bfcd8";
pub const IDE_VERSION: &str = "0.1.52";
pub const IDE_VERSION_CODE: &str = "20260811";
pub const FUNCTION: &str = "solo_work_lite";
pub const DEFAULT_MODEL: &str = "glm-5.2";

pub fn solo_headers(token: &str, user_id: &str, machine_id: &str, device_id: &str, stream: bool) -> Vec<(String, String)> {
    vec![
        ("Content-Type".to_string(), "application/json".to_string()),
        (
            "Accept".to_string(),
            if stream { "text/event-stream".to_string() } else { "application/json".to_string() },
        ),
        ("User-Agent".to_string(), format!("Trae/{IDE_VERSION}")),
        ("Authorization".to_string(), format!("Cloud-IDE-JWT {token}")),
        ("X-Cloudide-Token".to_string(), token.to_string()),
        ("X-Ide-Token".to_string(), token.to_string()),
        ("X-Uid".to_string(), user_id.to_string()),
        ("X-App-Id".to_string(), APP_ID.to_string()),
        ("X-App-Version".to_string(), "default".to_string()),
        ("X-Ide-Version".to_string(), IDE_VERSION.to_string()),
        ("X-Ide-Version-Code".to_string(), IDE_VERSION_CODE.to_string()),
        ("X-App-Version-Code".to_string(), IDE_VERSION_CODE.to_string()),
        ("X-Ide-Version-Type".to_string(), "stable".to_string()),
        ("X-Device-Type".to_string(), "macos".to_string()),
        ("X-OS-Version".to_string(), "macOS 15.7.4".to_string()),
        ("X-Device-Brand".to_string(), "Apple".to_string()),
        ("Request-Traffic-Type".to_string(), "prod".to_string()),
        ("X-Machine-Id".to_string(), machine_id.to_string()),
        ("X-Device-Id".to_string(), device_id.to_string()),
    ]
}

pub fn oauth_headers() -> Vec<(String, String)> {
    vec![
        ("Content-Type".to_string(), "application/json".to_string()),
        ("Accept".to_string(), "application/json".to_string()),
        ("User-Agent".to_string(), format!("Trae/{IDE_VERSION}")),
    ]
}

pub fn prepare_body(body: &Value, stream: bool) -> Result<Value, GatewayError> {
    let object = body.as_object().ok_or_else(|| {
        GatewayError::with_status(400, "Trae 请求体必须是 JSON 对象")
    })?;
    let mut out = Map::new();
    for (key, value) in object {
        out.insert(key.clone(), value.clone());
    }
    let model = out.get("model").and_then(Value::as_str).map(str::trim).filter(|value| !value.is_empty());
    let model = model.unwrap_or(DEFAULT_MODEL).to_string();
    out.insert("model".to_string(), Value::String(model.clone()));
    out.insert("config_name".to_string(), Value::String(model));
    out.insert("function".to_string(), Value::String(FUNCTION.to_string()));
    out.insert("stream".to_string(), Value::Bool(true));
    if !stream {
        out.insert("client_non_stream".to_string(), Value::Bool(true));
    }
    let messages = normalize_messages(out.get("messages"))?;
    out.insert("messages".to_string(), messages);
    normalize_tools(&mut out);
    normalize_tool_choice(&mut out);
    Ok(Value::Object(out))
}

fn normalize_messages(value: Option<&Value>) -> Result<Value, GatewayError> {
    let Some(list) = value.and_then(Value::as_array).cloned() else {
        return Err(GatewayError::with_status(400, "Trae 请求缺少 messages 数组"));
    };
    let mut out = Vec::with_capacity(list.len());
    for mut message in list {
        if let Some(object) = message.as_object_mut() {
            if let Some(Value::String(text)) = object.get("content").cloned() {
                object.insert(
                    "content".to_string(),
                    json!([{ "type": "text", "text": text }]),
                );
            }
            if object.get("role").and_then(Value::as_str) == Some("assistant") {
                if let Some(Value::Array(tool_calls)) = object.get_mut("tool_calls") {
                    for tool_call in tool_calls.iter_mut() {
                        if let Some(item) = tool_call.as_object_mut() {
                            if let Some(function) = item.get("function").cloned() {
                                item.insert("function_call".to_string(), function);
                                item.remove("function");
                            }
                        }
                    }
                    tool_calls.retain(|item| {
                        item.get("function_call")
                            .and_then(|function| function.get("name"))
                            .and_then(Value::as_str)
                            .map(|name| !name.trim().is_empty())
                            .unwrap_or(false)
                    });
                }
            }
        }
        out.push(message);
    }
    Ok(Value::Array(out))
}

fn normalize_tools(out: &mut Map<String, Value>) {
    let Some(Value::Array(tools)) = out.get_mut("tools") else {
        return;
    };
    tools.retain(|tool| tool.get("function").is_some());
    for tool in tools.iter_mut() {
        let Some(function) = tool.get_mut("function").and_then(Value::as_object_mut) else {
            continue;
        };
        if let Some(parameters) = function.get("parameters").cloned() {
            if parameters.is_object() {
                if let Ok(encoded) = serde_json::to_string(&parameters) {
                    function.insert("parameters".to_string(), Value::String(encoded));
                }
            }
        }
    }
    if tools.is_empty() {
        out.remove("tools");
    }
}

fn normalize_tool_choice(out: &mut Map<String, Value>) {
    let Some(choice) = out.get("tool_choice").cloned() else {
        return;
    };
    match choice {
        Value::String(value) => {
            if value.eq_ignore_ascii_case("none") {
                out.remove("tool_choice");
                out.remove("tools");
            } else {
                out.insert("tool_choice".to_string(), Value::String(value));
            }
        }
        Value::Object(value) => match value.get("type").and_then(Value::as_str).unwrap_or_default() {
            "none" => {
                out.remove("tool_choice");
                out.remove("tools");
            }
            kind @ ("auto" | "required") => {
                out.insert("tool_choice".to_string(), Value::String(kind.to_string()));
            }
            _ => {
                let name = value
                    .get("function")
                    .and_then(|function| function.get("name"))
                    .and_then(Value::as_str)
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or("auto");
                out.insert("tool_choice".to_string(), Value::String(name.to_string()));
            }
        },
        _ => {
            out.remove("tool_choice");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepares_body_for_trae_solo() {
        let body = json!({
            "model": "glm-5.2",
            "stream": false,
            "messages": [{ "role": "user", "content": "你好" }],
            "tools": [{ "function": { "name": "demo", "parameters": { "type": "object" } } }],
            "tool_choice": { "type": "function", "function": { "name": "demo" } }
        });
        let prepared = prepare_body(&body, false).expect("prepare body");
        assert_eq!(prepared["function"], FUNCTION);
        assert_eq!(prepared["stream"], true);
        assert_eq!(prepared["client_non_stream"], true);
        assert_eq!(prepared["config_name"], "glm-5.2");
        assert_eq!(prepared["messages"][0]["content"][0]["type"], "text");
        assert_eq!(prepared["tool_choice"], "demo");
        assert!(prepared["tools"][0]["function"]["parameters"].is_string());
    }
}
