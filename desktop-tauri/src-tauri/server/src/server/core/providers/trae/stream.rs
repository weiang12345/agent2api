//! Trae SOLO SSE → OpenAI SSE / Completion。
use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use crate::server::errors::GatewayError;

#[derive(Debug, Clone)]
pub struct SoloEvent {
    pub kind: String,
    pub payload: Value,
}

#[derive(Debug)]
pub struct SoloError {
    pub code: i64,
    pub message: String,
}

impl SoloError {
    pub fn gateway_error(&self) -> GatewayError {
        let status = match self.code {
            1005 | 4008 | 4010 => 429,
            1001 => 401,
            _ => 502,
        };
        GatewayError::with_status(status, format!("Trae 上游错误 {}: {}", self.code, self.message))
            .with_code(self.code.to_string())
    }
}

#[derive(Default)]
pub struct SoloSseParser {
    pending_event: String,
    pending_data: String,
    pending_line: String,
}

impl SoloSseParser {
    pub fn push(&mut self, chunk: &[u8]) -> Vec<SoloEvent> {
        let mut buffer = self.pending_line.clone();
        buffer.push_str(&String::from_utf8_lossy(chunk));
        let mut events = Vec::new();
        while let Some(index) = buffer.find('\n') {
            let line: String = buffer.drain(..=index).collect();
            let line = line.trim_end_matches(['\n', '\r']).to_string();
            if let Some(value) = line.strip_prefix("event:") {
                self.pending_event = value.trim().to_string();
            } else if let Some(value) = line.strip_prefix("data:") {
                if self.pending_data.is_empty() {
                    self.pending_data = value.trim().to_string();
                } else {
                    self.pending_data.push_str(value.trim());
                }
            } else if line.is_empty() {
                if let Some(event) = self.take_event() {
                    events.push(event);
                }
            }
        }
        self.pending_line = buffer;
        events
    }

    pub fn finish(&mut self) -> Vec<SoloEvent> {
        let line = self.pending_line.clone();
        self.pending_line.clear();
        let line = line.trim_end_matches(['\n', '\r']).to_string();
        if let Some(value) = line.strip_prefix("data:") {
            if self.pending_data.is_empty() {
                self.pending_data = value.trim().to_string();
            } else {
                self.pending_data.push_str(value.trim());
            }
        }
        if let Some(event) = self.take_event() {
            return vec![event];
        }
        Vec::new()
    }

    fn take_event(&mut self) -> Option<SoloEvent> {
        let kind = std::mem::take(&mut self.pending_event);
        let data = std::mem::take(&mut self.pending_data);
        if kind.is_empty() {
            return None;
        }
        let payload = if data.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&data).unwrap_or(Value::Null)
        };
        Some(SoloEvent { kind, payload })
    }
}

pub struct Translator {
    id: String,
    created: i64,
    model: String,
    role_sent: bool,
    usage: Option<Value>,
    finish_reason: Option<String>,
    content: String,
    reasoning: String,
    tool_calls: BTreeMap<i64, Map<String, Value>>,
}

impl Translator {
    pub fn new(model: &str) -> Self {
        Self {
            id: format!("chatcmpl-{}", crate::server::logging::now_ms()),
            created: crate::server::logging::now_ms() / 1000,
            model: model.to_string(),
            role_sent: false,
            usage: None,
            finish_reason: None,
            content: String::new(),
            reasoning: String::new(),
            tool_calls: BTreeMap::new(),
        }
    }

    pub fn consume(&mut self, event: SoloEvent) -> Result<Vec<Value>, GatewayError> {
        let mut frames = Vec::new();
        match event.kind.as_str() {
            "output" => {
                let response = event.payload.get("response").and_then(Value::as_str).unwrap_or("");
                let reasoning = event
                    .payload
                    .get("reasoning_content")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                self.content.push_str(response);
                self.reasoning.push_str(reasoning);
                self.merge_tool_calls(event.payload.get("tool_calls"));
                let mut delta = Map::new();
                if !response.is_empty() {
                    delta.insert("content".to_string(), Value::String(response.to_string()));
                }
                if !reasoning.is_empty() {
                    delta.insert(
                        "reasoning_content".to_string(),
                        Value::String(reasoning.to_string()),
                    );
                }
                if let Some(tools) = self.tool_call_delta(event.payload.get("tool_calls")) {
                    delta.insert("tool_calls".to_string(), tools);
                }
                if !delta.is_empty() {
                    self.ensure_role(&mut frames);
                    frames.push(self.chunk(Value::Object(delta), None));
                }
            }
            "token_usage" => {
                self.usage = Some(event.payload.clone());
            }
            "done" => {
                self.finish_reason = Some(
                    event
                        .payload
                        .get("finish_reason")
                        .and_then(Value::as_str)
                        .unwrap_or("stop")
                        .to_string(),
                );
            }
            "error" => {
                let code = event.payload.get("code").and_then(Value::as_i64).unwrap_or_default();
                let message = event
                    .payload
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("未知错误")
                    .to_string();
                return Err(SoloError { code, message }.gateway_error());
            }
            _ => {}
        }
        Ok(frames)
    }

    pub fn finish_frames(&mut self) -> Vec<Value> {
        let mut frames = Vec::new();
        self.ensure_role(&mut frames);
        let finish = self.finish_reason.clone().unwrap_or_else(|| "stop".to_string());
        frames.push(self.chunk(json!({}), Some(finish)));
        if let Some(usage) = self.usage.clone() {
            let mut value = self.chunk(json!({}), None);
            if let Some(object) = value.as_object_mut() {
                object.insert("usage".to_string(), usage);
            }
            frames.push(value);
        }
        frames
    }

    pub fn completion(&self) -> Value {
        let mut message = Map::new();
        message.insert("role".to_string(), Value::String("assistant".to_string()));
        message.insert("content".to_string(), Value::String(self.content.clone()));
        if !self.reasoning.is_empty() {
            message.insert(
                "reasoning_content".to_string(),
                Value::String(self.reasoning.clone()),
            );
        }
        if !self.tool_calls.is_empty() {
            let calls: Vec<Value> = self.tool_calls.values().cloned().map(Value::Object).collect();
            message.insert("tool_calls".to_string(), Value::Array(calls));
        }
        json!({
            "id": self.id,
            "object": "chat.completion",
            "created": self.created,
            "model": self.model,
            "choices": [{
                "index": 0,
                "message": Value::Object(message),
                "finish_reason": self.finish_reason.clone().unwrap_or_else(|| "stop".to_string()),
            }],
            "usage": self.usage.clone().unwrap_or(Value::Null),
        })
    }

    fn ensure_role(&mut self, frames: &mut Vec<Value>) {
        if self.role_sent {
            return;
        }
        self.role_sent = true;
        frames.push(self.chunk(json!({ "role": "assistant", "content": "" }), None));
    }

    fn chunk(&self, delta: Value, finish: Option<String>) -> Value {
        let mut choice = Map::new();
        choice.insert("index".to_string(), Value::from(0));
        choice.insert("delta".to_string(), delta);
        if let Some(finish) = finish {
            choice.insert("finish_reason".to_string(), Value::String(finish));
        }
        json!({
            "id": self.id,
            "object": "chat.completion.chunk",
            "created": self.created,
            "model": self.model,
            "choices": [Value::Object(choice)],
        })
    }

    fn merge_tool_calls(&mut self, raw: Option<&Value>) {
        let Some(list) = raw.and_then(Value::as_array).filter(|list| !list.is_empty()) else {
            return;
        };
        for item in list {
            let Some(object) = item.as_object() else { continue };
            let index = object.get("index").and_then(Value::as_i64).unwrap_or(0);
            let merged = self.tool_calls.entry(index).or_insert_with(|| {
                let mut initial = Map::new();
                initial.insert("index".to_string(), Value::from(index));
                initial
            });
            for (key, value) in object {
                if key == "function_call" || key == "function" {
                    if let Some(function) = value.as_object() {
                        let target = merged
                            .entry("function".to_string())
                            .or_insert_with(|| Value::Object(Map::new()));
                        if let Some(target) = target.as_object_mut() {
                            for (function_key, function_value) in function {
                                if function_key == "namespace"
                                    || function_key == "partial_arguments"
                                    || function_key == "partial_arguments"
                                {
                                    continue;
                                }
                                if function_key == "arguments" {
                                    let previous = target
                                        .get("arguments")
                                        .and_then(Value::as_str)
                                        .unwrap_or_default();
                                    let addition = function_value.as_str().unwrap_or_default();
                                    target.insert(
                                        "arguments".to_string(),
                                        Value::String(format!("{previous}{addition}")),
                                    );
                                } else if !function_value.is_null()
                                    && !matches!(function_value, Value::String(text) if text.is_empty())
                                {
                                    target.insert(function_key.clone(), function_value.clone());
                                }
                            }
                        }
                    }
                } else if key != "partial_arguments"
                    && key != "namespace"
                    && !value.is_null()
                    && !matches!(value, Value::String(text) if text.is_empty())
                {
                    merged.insert(key.clone(), value.clone());
                }
            }
        }
    }

    fn tool_call_delta(&self, raw: Option<&Value>) -> Option<Value> {
        let list = raw
            .and_then(Value::as_array)
            .filter(|list| !list.is_empty())?;
        let mut out = Vec::new();
        for item in list {
            let Some(object) = item.as_object() else { continue };
            let mut delta = Map::new();
            for (key, value) in object {
                if key == "function_call" {
                    delta.insert("function".to_string(), value.clone());
                } else if key != "namespace" && key != "partial_arguments" {
                    delta.insert(key.clone(), value.clone());
                }
            }
            out.push(Value::Object(delta));
        }
        Some(Value::Array(out))
    }
}

pub fn sse_frame(value: &Value) -> String {
    format!("data: {}\n\n", serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string()))
}

pub fn sse_done() -> String {
    "data: [DONE]\n\n".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_solo_events_and_translates_completion() {
        let raw = "event:output\n\
                   data:{\"response\":\"你好\",\"reasoning_content\":\"思考\",\"tool_calls\":null}\n\n\
                   event:output\n\
                   data:{\"response\":\"世界\",\"reasoning_content\":null,\"tool_calls\":null}\n\n\
                   event:token_usage\n\
                   data:{\"prompt_tokens\":10,\"completion_tokens\":2,\"total_tokens\":12}\n\n\
                   event:done\n\
                   data:{\"finish_reason\":\"stop\"}\n\n";
        let mut parser = SoloSseParser::default();
        let mut translator = Translator::new("glm-5.2");
        for event in parser.push(raw.as_bytes()) {
            translator.consume(event).expect("translate event");
        }
        for event in parser.finish() {
            translator.consume(event).expect("translate final event");
        }
        let completion = translator.completion();
        assert_eq!(completion["choices"][0]["message"]["content"], "你好世界");
        assert_eq!(completion["choices"][0]["message"]["reasoning_content"], "思考");
        assert_eq!(completion["usage"]["total_tokens"], 12);
        assert_eq!(completion["choices"][0]["finish_reason"], "stop");
    }

    #[test]
    fn merges_tool_call_fragments() {
        let mut parser = SoloSseParser::default();
        let mut translator = Translator::new("glm-5.2");
        for event in parser.push(
            "event:output\n\
             data:{\"response\":\"\",\"tool_calls\":[{\"index\":0,\"id\":\"call-1\",\"type\":\"function\",\"function_call\":{\"name\":\"demo\",\"arguments\":\"\"}}]}\n\n\
             event:output\n\
             data:{\"response\":\"\",\"tool_calls\":[{\"index\":0,\"id\":\"\",\"type\":\"\",\"function_call\":{\"name\":\"\",\"arguments\":\"{\\\"a\\\":\"}}]}\n\n\
             event:output\n\
             data:{\"response\":\"\",\"tool_calls\":[{\"index\":0,\"id\":\"\",\"type\":\"\",\"function_call\":{\"name\":\"\",\"arguments\":\"1}\"}}]}\n\n\
             event:done\n\
             data:{\"finish_reason\":\"tool_calls\"}\n\n".as_bytes(),
        ) {
            translator.consume(event).expect("translate tool event");
        }
        let completion = translator.completion();
        assert_eq!(completion["choices"][0]["message"]["tool_calls"][0]["id"], "call-1");
        assert_eq!(
            completion["choices"][0]["message"]["tool_calls"][0]["function"]["name"],
            "demo"
        );
        assert_eq!(
            completion["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"],
            "{\"a\":1}"
        );
    }
}
