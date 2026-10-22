//! Loomy 的统一网关中继（认证之后的逻辑**只有一份**）。
//!
//! 统一网关（57892）解析 `loomy-cn:模型名` 后把 body 原样转交到这里；
//! 出站走 core `loomy::api` 的协议（业务头 / 恒 200 信封 / `100002` 不重试）。
//! 差异（与 Qoder 中继对照）：
//! - 上游是**标准 OpenAI 兼容** `/chat/completions`（无 COSY 编码）⇒ body 直传；
//! - 业务失败恒 HTTP 200：需 peek `application/json` 响应体判业务错误
//!   （绝不读 SSE 流）。

use std::collections::HashSet;
use std::time::Instant;

use axum::body::Bytes;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use buddy_switch_core::modules::loomy::region::{realm_spec, LoomyRegion};

use super::pool::{classify_http, LoomyErrKind};
use super::LoomyGatewayState;

/// 入站协议（与 `qoder::routes::Protocol` 同语义）。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    OpenAi,
    Anthropic,
    Responses,
}

/// 入站协议 → 出站转换。
fn protocol_error(
    protocol: Protocol,
    status: StatusCode,
    code: &str,
    message: &str,
) -> Response {
    match protocol {
        Protocol::OpenAi | Protocol::Responses => openai_error(status, code, message),
        Protocol::Anthropic => anthropic_error(status, code, message),
    }
}

/// OpenAI 形态错误体。
fn openai_error(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        axum::Json(json!({
            "error": { "message": message, "type": "api_error", "code": code }
        })),
    )
        .into_response()
}

/// Anthropic 形态错误体（`/v1/messages` 的客户端只认这个形状）。
fn anthropic_error(status: StatusCode, kind: &str, message: &str) -> Response {
    (
        status,
        axum::Json(json!({
            "type": "error",
            "error": { "type": kind, "message": message }
        })),
    )
        .into_response()
}

/// 出站 HTTP 客户端（与其它产品线同款构造；测试挂载点在 `LoomyGatewayState`）。
pub fn build_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(std::time::Duration::from_secs(10))
        .read_timeout(std::time::Duration::from_secs(300))
        .pool_max_idle_per_host(20)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// 本次请求要用的模型名：显式 `model`（**不允许回落清单第一条** —— 统一网关的
/// `loomy-cn:` 前缀已剥掉，客户端显式传了什么就用什么；空 ⇒ 报错不猜）。
fn model_of(parsed: &Value) -> String {
    parsed
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// 是否流式。
fn wants_stream(parsed: &Value) -> bool {
    parsed
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// 出站请求体：`model` 已剥前缀、恒 `stream: true`（本网关只做流式透传，
/// 非流式由聚合层本地折叠 —— 与 Qoder 中继同一纪律）。
fn prepare_body(parsed: &Value, model: &str) -> Value {
    let mut out = parsed.clone();
    if let Some(object) = out.as_object_mut() {
        object.insert("model".to_string(), Value::String(model.to_string()));
        object.insert("stream".to_string(), Value::Bool(true));
    }
    out
}

/// 一趟 Loomy chat 中继（认证之后的全部逻辑）—— 统一网关复用这一份。
pub async fn chat_for_region(
    state: LoomyGatewayState,
    region: LoomyRegion,
    body: Bytes,
    protocol: Protocol,
) -> Response {
    let started = Instant::now();
    let parsed: Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(error) => {
            return protocol_error(
                protocol,
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                &format!("请求体不是合法 JSON：{error}"),
            )
        }
    };

    let model = model_of(&parsed);
    if model.is_empty() {
        return protocol_error(
            protocol,
            StatusCode::BAD_REQUEST,
            "invalid_request_error",
            "未指定模型：统一网关请写 `loomy-cn:<模型id>`",
        );
    }

    // ★ 出口 ②：被禁用的模型直接拒绝（与 /v1/models 同源）。
    {
        let scope = buddy_switch_core::modules::product::scope_of_loomy_region(region);
        let disabled = state.disabled.lock().await;
        if crate::disabled_models::is_disabled(&disabled, scope, &model) {
            return protocol_error(
                protocol,
                StatusCode::FORBIDDEN,
                "model_disabled",
                &crate::disabled_models::disabled_message(scope, &model),
            );
        }
    }

    let sticky_key = parsed
        .get("metadata")
        .and_then(|m| {
            m.get("conversation_id")
                .or_else(|| m.get("session_id"))
                .and_then(Value::as_str)
        })
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let preferred = state.sticky_uid(sticky_key.as_deref()).await;
    let max_rotate = 3usize;

    let outbound = prepare_body(&parsed, &model).to_string();
    let mut tried: HashSet<String> = HashSet::new();
    let mut failures: Vec<String> = Vec::new();

    for _ in 0..max_rotate {
        // 池随账号库同步（导入后即时生效），然后选号。
        let picked = {
            let mut pools = state.pools.lock().await;
            let pool = pools.entry(region).or_default();
            pool.sync_for(region);
            pool.pick(&model, &tried, preferred.as_deref())
        };
        let Some(uid) = picked else {
            break;
        };
        tried.insert(uid.clone());

        let access_token = {
            let pools = state.pools.lock().await;
            pools
                .get(&region)
                .and_then(|pool| {
                    pool.entries()
                        .find(|entry| entry.uid == uid)
                        .map(|entry| entry.access_token.clone())
                })
                .unwrap_or_default()
        };
        if access_token.is_empty() {
            continue;
        }

        let spec = realm_spec(region);
        let url = match &state.upstream_override {
            Some(host) => format!("{host}/chat/completions"),
            None => format!("{}/chat/completions", spec.api_base),
        };
        let mut request = state
            .http
            .post(&url)
            .timeout(buddy_switch_core::modules::loomy::api::CHAT_TIMEOUT)
            .body(outbound.clone());
        // 两套认证头都发（chat 端点认 Bearer；照官方客户端两个都发）。
        for (name, value) in buddy_switch_core::modules::loomy::chat::chat_headers(&access_token) {
            request = request.header(name, value);
        }
        let response = match request.send().await {
            Ok(response) => response,
            Err(transport_error) => {
                failures.push(format!("{uid}：{transport_error}"));
                let mut pools = state.pools.lock().await;
                if let Some(pool) = pools.get_mut(&region) {
                    pool.apply_error(&uid, LoomyErrKind::Transient, &transport_error.to_string());
                }
                continue;
            }
        };

        let status = response.status();
        if status.is_success() {
            // ★ 恒 200 业务信封：只在 content-type 是 JSON 时 peek（绝不读 SSE 流）。
            let content_type = response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .to_ascii_lowercase();
            if content_type.contains("application/json") {
                // 非 SSE：完整读取（短 JSON 体）。reqwest 的 Response 不可 clone，
                // 读出后若不是业务错误，直接把 JSON 体透传给调用方（上游非流式响应）。
                let text = response.text().await.unwrap_or_default();
                if let Some((kind, message)) = peek_business_error(&text) {
                    let mut pools = state.pools.lock().await;
                    if let Some(pool) = pools.get_mut(&region) {
                        pool.apply_error(&uid, kind, &message);
                    }
                    let status_code = if kind == LoomyErrKind::AuthDead {
                        StatusCode::UNAUTHORIZED
                    } else {
                        StatusCode::BAD_GATEWAY
                    };
                    if kind.should_rotate() {
                        failures.push(format!("{uid}：{message}"));
                        continue;
                    }
                    // ClientError：换号无意义，直接回给调用方。
                    return protocol_error(
                        protocol,
                        status_code,
                        "upstream_error",
                        &format!("Loomy 业务错误：{message}"),
                    );
                }
                // 无业务错误 ⇒ 上游真的给了非流式 JSON 响应：透传。
                let usage = serde_json::from_str::<Value>(&text)
                    .ok()
                    .and_then(|body| body.get("usage").cloned());
                let (prompt_tokens, completion_tokens) = match &usage {
                    Some(usage) => usage_counts(usage),
                    None => (0, 0),
                };
                state
                    .record_request(
                        "chat",
                        region,
                        &model,
                        200,
                        &uid,
                        started.elapsed().as_millis() as i64,
                        false,
                        prompt_tokens as i64,
                        completion_tokens as i64,
                        None,
                    )
                    .await;
                {
                    let mut pools = state.pools.lock().await;
                    if let Some(pool) = pools.get_mut(&region) {
                        pool.note_success(&uid);
                    }
                }
                state.bind_sticky(sticky_key.as_deref(), &uid).await;
                return (
                    StatusCode::OK,
                    [(header::CONTENT_TYPE, "application/json")],
                    text,
                )
                    .into_response();
            }
            {
                let mut pools = state.pools.lock().await;
                if let Some(pool) = pools.get_mut(&region) {
                    pool.note_success(&uid);
                }
            }
            state.bind_sticky(sticky_key.as_deref(), &uid).await;
            return if wants_stream(&parsed) {
                stream_relay(state, response, protocol, region, &model, &uid, started).await
            } else {
                aggregate_relay(state, response, protocol, region, &model, &uid, started).await
            };
        }

        let status_code = status.as_u16();
        let detail: String = response
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(300)
            .collect();
        let kind = classify_http(status_code);
        {
            let mut pools = state.pools.lock().await;
            if let Some(pool) = pools.get_mut(&region) {
                pool.apply_error(&uid, kind, &detail);
            }
        }
        failures.push(format!("{uid}：HTTP {status_code} {detail}"));
        if !kind.should_rotate() {
            state
                .record_request(
                    "chat",
                    region,
                    &model,
                    status_code,
                    &uid,
                    started.elapsed().as_millis() as i64,
                    wants_stream(&parsed),
                    0,
                    0,
                    Some(detail.clone()),
                )
                .await;
            return protocol_error(
                protocol,
                StatusCode::from_u16(status_code).unwrap_or(StatusCode::BAD_GATEWAY),
                "upstream_error",
                &format!("上游拒绝（HTTP {status_code}）：{detail}"),
            );
        }
    }

    // 轮换耗尽 / 无可用账号。
    let message = if failures.is_empty() {
        "Loomy 没有可用账号：请在「账号」页导入凭据或短信登录".to_string()
    } else {
        format!("全部账号都失败（{} 个）：{}", failures.len(), failures.join("；"))
    };
    state
        .record_request(
            "chat",
            region,
            &model,
            503,
            "",
            started.elapsed().as_millis() as i64,
            wants_stream(&parsed),
            0,
            0,
            Some(message.clone()),
        )
        .await;
    protocol_error(
        protocol,
        StatusCode::SERVICE_UNAVAILABLE,
        "no_available_account",
        &message,
    )
}

/// 从非 SSE 的 JSON 响应体里抽业务错误：`(分类, 可读说明)`；不是错误时 `None`。
fn peek_business_error(text: &str) -> Option<(LoomyErrKind, String)> {
    let parsed: Value = serde_json::from_str(text).ok()?;
    // OpenAI 风格错误体。
    if let Some(error) = parsed.get("error").filter(|e| e.is_object()) {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("上游错误");
        return Some((LoomyErrKind::Transient, message.to_string()));
    }
    // Loomy 业务信封。
    let code = parsed.get("code").and_then(Value::as_str).unwrap_or("");
    if code.is_empty() {
        return None;
    }
    let spec = realm_spec(LoomyRegion::Cn);
    if code == spec.ok_code {
        return None;
    }
    let message = parsed
        .get("desc")
        .and_then(Value::as_str)
        .unwrap_or("业务错误");
    let kind = if code == spec.auth_error_code {
        LoomyErrKind::AuthDead
    } else if code == spec.bad_request_code {
        LoomyErrKind::ClientError
    } else {
        LoomyErrKind::Transient
    };
    Some((kind, format!("{code} {message}")))
}

fn usage_counts(usage: &Value) -> (u64, u64) {
    let prompt = usage
        .get("prompt_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let completion = usage
        .get("completion_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    (prompt, completion)
}

/// 流式中继：上游是标准 OpenAI SSE ⇒ 直接逐帧透传（含 usage 旁路记账）。
async fn stream_relay(
    state: LoomyGatewayState,
    response: reqwest::Response,
    protocol: Protocol,
    region: LoomyRegion,
    model: &str,
    uid: &str,
    started: Instant,
) -> Response {
    let stream_model = model.to_string();
    let usage_slot = std::sync::Arc::new(std::sync::Mutex::new(None::<Value>));
    let state_for_task = state.clone();
    let uid = uid.to_string();
    let model_for_task = model.to_string();

    // 协议转换（Anthropic / Responses 入口）排在外层 —— 上游是标准 OpenAI SSE。
    let raw = response.bytes_stream();
    let tapped = OpenAiUsageTap::new(raw, usage_slot.clone());
    let converter: std::pin::Pin<
        Box<dyn futures_util::Stream<Item = Result<axum::body::Bytes, std::io::Error>> + Send>,
    > = match protocol {
        Protocol::OpenAi => Box::pin(tapped),
        Protocol::Anthropic => Box::pin(crate::protocol::anthropic::AnthropicSseStream::new(
            tapped,
            stream_model.clone(),
        )),
        Protocol::Responses => Box::pin(crate::protocol::responses::ResponsesSseStream::new(
            tapped,
            stream_model.clone(),
        )),
    };

    let (tx, rx) = mpsc::channel::<Result<axum::body::Bytes, std::io::Error>>(32);
    tokio::spawn(async move {
        use futures_util::StreamExt;
        let mut converter = converter;
        while let Some(item) = converter.next().await {
            match item {
                Ok(frame) => {
                    if tx.send(Ok(frame)).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let usage = usage_slot.lock().ok().and_then(|guard| guard.clone());
        let (prompt_tokens, completion_tokens) = match &usage {
            Some(usage) => usage_counts(usage),
            None => (0, 0),
        };
        state_for_task
            .record_request(
                "chat",
                region,
                &model_for_task,
                200,
                &uid,
                started.elapsed().as_millis() as i64,
                true,
                prompt_tokens as i64,
                completion_tokens as i64,
                None,
            )
            .await;
    });

    crate::qoder::sse::response_from_receiver(rx)
}

/// 非流式中继：聚合上游 OpenAI SSE 流 → 单个 completion。
async fn aggregate_relay(
    state: LoomyGatewayState,
    response: reqwest::Response,
    protocol: Protocol,
    region: LoomyRegion,
    model: &str,
    uid: &str,
    started: Instant,
) -> Response {
    // 上游是标准 OpenAI SSE ⇒ 复用 Qoder 聚合器（无 leak-guard 的工具回声问题 ——
    // Loomy 上游本来就给结构化 tool_calls）。
    let chat_id = format!("chatcmpl-{}", uuid::Uuid::new_v4().simple());
    let aggregated = crate::qoder::sse::aggregate(response.bytes_stream(), None).await;
    let usage = aggregated.usage.clone();
    let (prompt_tokens, completion_tokens) = match &usage {
        Some(usage) => usage_counts(usage),
        None => (0, 0),
    };
    let (log_status, log_error) = match &aggregated.upstream_error {
        Some(detail) => (502u16, Some(detail.clone())),
        None => (200u16, None),
    };
    state
        .record_request(
            "chat",
            region,
            model,
            log_status,
            uid,
            started.elapsed().as_millis() as i64,
            false,
            prompt_tokens as i64,
            completion_tokens as i64,
            log_error,
        )
        .await;

    if let Some(error) = &aggregated.upstream_error {
        return protocol_error(protocol, StatusCode::BAD_GATEWAY, "upstream_error", error);
    }

    let mut message = serde_json::Map::new();
    message.insert("role".into(), Value::String("assistant".into()));
    message.insert("content".into(), Value::String(aggregated.content.clone()));
    if !aggregated.reasoning.is_empty() {
        message.insert(
            "reasoning_content".into(),
            Value::String(aggregated.reasoning.clone()),
        );
    }
    if !aggregated.tool_calls.is_empty() {
        message.insert(
            "tool_calls".into(),
            Value::Array(aggregated.tool_calls.clone()),
        );
    }
    let chat = json!({
        "id": chat_id,
        "object": "chat.completion",
        "created": buddy_switch_core::modules::config::now_ms() / 1000,
        "model": model,
        "choices": [{ "index": 0, "message": Value::Object(message), "finish_reason": aggregated.finish_reason }],
        "usage": usage,
    });
    match protocol {
        Protocol::OpenAi => axum::Json(chat).into_response(),
        Protocol::Anthropic => {
            let anthropic = crate::protocol::anthropic::message_from_response(&chat, model);
            axum::Json(anthropic).into_response()
        }
        Protocol::Responses => {
            axum::Json(crate::protocol::responses::to_response_object_from(
                &crate::protocol::responses::ResponsesParts {
                    content: &aggregated.content,
                    reasoning: &aggregated.reasoning,
                    tool_calls: &aggregated.tool_calls,
                    model: aggregated.model.as_deref(),
                    created_at: aggregated.created,
                    input_tokens: prompt_tokens,
                    output_tokens: completion_tokens,
                },
                model,
            ))
            .into_response()
        }
    }
}

/// 标准 OpenAI SSE 的 usage 旁路：逐帧透传，同时记录末帧的 `usage`。
pub struct OpenAiUsageTap<S> {
    inner: S,
    usage_slot: std::sync::Arc<std::sync::Mutex<Option<Value>>>,
}

impl<S> OpenAiUsageTap<S> {
    pub fn new(inner: S, usage_slot: std::sync::Arc<std::sync::Mutex<Option<Value>>>) -> Self {
        Self { inner, usage_slot }
    }
}

impl<S, E> futures_util::Stream for OpenAiUsageTap<S>
where
    S: futures_util::Stream<Item = Result<axum::body::Bytes, E>> + Unpin,
    E: std::fmt::Debug + Send + Sync + Unpin + 'static,
{
    type Item = Result<axum::body::Bytes, std::io::Error>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        match std::pin::Pin::new(&mut self.inner).poll_next(cx) {
            std::task::Poll::Ready(Some(Ok(frame))) => {
                let text = String::from_utf8_lossy(&frame);
                for line in text.lines() {
                    let Some(data) = line.strip_prefix("data: ") else {
                        continue;
                    };
                    let data = data.trim();
                    if data == "[DONE]" {
                        continue;
                    }
                    if let Ok(chunk) = serde_json::from_str::<Value>(data) {
                        if let Some(usage) = chunk.get("usage").filter(|u| u.is_object()) {
                            if let Ok(mut guard) = self.usage_slot.lock() {
                                *guard = Some(usage.clone());
                            }
                        }
                    }
                }
                std::task::Poll::Ready(Some(Ok(frame)))
            }
            other => match other {
                std::task::Poll::Ready(Some(Err(error))) => std::task::Poll::Ready(Some(Err(
                    std::io::Error::other(format!("{error:?}")),
                ))),
                done => done.map(|opt| {
                    opt.map(|item| {
                        item.map_err(|error| std::io::Error::other(format!("{error:?}")))
                    })
                }),
            },
        }
    }
}
