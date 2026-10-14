//! `POST /v1/messages`：Anthropic 协议 + tool use 双向转换。
//!
//! 与 `/v1/chat/completions` 共用同一套中继（选号 / 轮换 / 冷却），差别只在协议层：
//! 入站 Anthropic → 上游 OpenAI 形态，出站上游 SSE → Anthropic 事件流。

use std::time::Instant;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;

use buddy_switch_core::modules::region::Region;

use crate::logging::RequestMeta;
use crate::outbound::PromptMode;
use crate::protocol::anthropic::{aggregate_message, to_upstream_request, AnthropicSseStream};
use crate::routes::relay::{self, RelayKind, RelayOutcome, RelayRequest};
use crate::routes::{anthropic_error, authenticate, json_response, parse_body, sse_response};
use crate::session_headers;
use crate::state::GatewayState;
use crate::timeutil;

/// Anthropic messages 处理器。
pub async fn handler(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let record = match authenticate(&state, &headers) {
        Ok(record) => record,
        Err(error) => return anthropic_error(error),
    };
    // ★ 归属域由 API Key 决定；前缀策略走产品线那一套（统一入口走 `AlreadyResolved`）。
    let allow_reroute = state.config_snapshot().await.allow_model_region_prefix;
    messages_for_region(
        state,
        record.region,
        headers,
        body,
        crate::routes::chat::PrefixPolicy::RegionPrefix { allow_reroute },
    )
    .await
}

/// 一趟 Anthropic 中继（**认证之后的全部逻辑**）—— 产品线入口与统一网关**共用**这一份实现。
///
/// `state` **按值**传入（`Arc` 句柄，克隆极廉价）⇒ 函数体内既有的 `&state` 写法一行都不用改。
pub async fn messages_for_region(
    state: GatewayState,
    region: Region,
    headers: HeaderMap,
    body: Bytes,
    policy: crate::routes::chat::PrefixPolicy,
) -> Response {
    let started = Instant::now();
    let mut region = region;

    let parsed = match parse_body(&body) {
        Ok(value) => value,
        Err(error) => return anthropic_error(error),
    };
    let requested_model = parsed
        .get("model")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let wants_stream = parsed
        .get("stream")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    // 模型名前缀路由：与 `/v1/chat/completions` **同一策略对象**（见 `PrefixPolicy`）——
    // 统一入口传 `AlreadyResolved`，此处**不再**解析 `cn:` / `global:`。
    let model = match policy {
        crate::routes::chat::PrefixPolicy::AlreadyResolved => requested_model.clone(),
        crate::routes::chat::PrefixPolicy::RegionPrefix { allow_reroute } => {
            let route = crate::model_route::split_model_prefix(&requested_model);
            match crate::model_route::decide(region, allow_reroute, &route) {
                crate::model_route::PrefixDecision::NoPrefix => requested_model.clone(),
                crate::model_route::PrefixDecision::StripOnly => route.bare_model.clone(),
                crate::model_route::PrefixDecision::Reroute(target) => {
                    region = target;
                    route.bare_model.clone()
                }
                crate::model_route::PrefixDecision::Rejected {
                    requested,
                    key_region,
                } => {
                    return anthropic_error(crate::error::GatewayError::BadRequest(
                        crate::model_route::rejection_message(requested, key_region),
                    ))
                }
            }
        }
    };

    // ★ 出口 ②（P11）：与 `/v1/chat/completions` 同源 —— 被禁用的模型直接拒绝（读同一份 `disabled`）。
    {
        let disabled = state.disabled.read().await;
        if crate::disabled_models::is_disabled(&disabled, region, &model) {
            return anthropic_error(crate::error::GatewayError::ModelDisabled { region, model });
        }
    }

    // Anthropic 请求 → 上游（OpenAI 形态）。
    let upstream_body = to_upstream_request(&parsed);
    let upstream_raw = serde_json::to_string(&upstream_body).unwrap_or_else(|_| "{}".to_string());
    let raw = if model == requested_model {
        upstream_raw
    } else {
        crate::model_route::rewrite_body_model(&upstream_raw, &model)
    };

    let conversation_id = relay::conversation_id_of(&parsed);
    let turn_key = session_headers::turn_key(&parsed);
    let inbound_request_id = header_of(&headers, "x-conversation-request-id");
    let trace_id = header_of(&headers, "x-trace-id");

    let mut attempt = run(
        &state,
        region,
        &raw,
        &model,
        conversation_id.clone(),
        turn_key.as_deref(),
        inbound_request_id.as_deref(),
        trace_id.clone(),
    )
    .await;

    // 与 chat 端点同一套降级策略。
    if let Err(failure) = &attempt {
        let status = failure.error.status();
        let mode = state.outbound_options().await.prompt.mode;
        let already_degraded = state.degrade_active(timeutil::now_ms()).await;
        if !already_degraded
            && mode == PromptMode::Passthrough
            && relay::is_content_blocked(status, &failure.upstream_message)
            && relay::trip_degrade(&state).await
        {
            attempt = run(
                &state,
                region,
                &raw,
                &model,
                conversation_id,
                turn_key.as_deref(),
                inbound_request_id.as_deref(),
                trace_id,
            )
            .await;
        }
    }

    match attempt {
        Err(failure) => {
            let http_status = failure.error.status();
            state.log.record(
                RequestMeta {
                    endpoint: "/v1/messages",
                    method: "POST",
                    region,
                    account: failure.account,
                    account_uid: failure.uid,
                    model,
                    status: http_status,
                    latency_ms: started.elapsed().as_millis() as i64,
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    stream: wants_stream,
                }
                .to_value(),
            );
            anthropic_error(failure.error)
        }
        Ok(RelayOutcome::Ok {
            response,
            account,
            uid,
        }) => {
            if wants_stream {
                state.log.record(
                    RequestMeta {
                        endpoint: "/v1/messages",
                        method: "POST",
                        region,
                        account,
                        account_uid: uid.clone(),
                        model: model.clone(),
                        status: 200,
                        latency_ms: started.elapsed().as_millis() as i64,
                        prompt_tokens: 0,
                        completion_tokens: 0,
                        stream: true,
                    }
                    .to_value(),
                );
                let sink = relay::pool_usage_sink(&state, &uid, &model);
                sse_response(AnthropicSseStream::new(
                    crate::protocol::usage_tap::UsageTap::new(response.bytes_stream(), sink),
                    model,
                ))
            } else {
                let text = response.text().await.unwrap_or_default();
                let message = aggregate_message(&text, &model);
                let prompt_tokens = message
                    .get("usage")
                    .and_then(|usage| usage.get("input_tokens"))
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                let completion_tokens = message
                    .get("usage")
                    .and_then(|usage| usage.get("output_tokens"))
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                state.log.record(
                    RequestMeta {
                        endpoint: "/v1/messages",
                        method: "POST",
                        region,
                        account,
                        account_uid: uid,
                        model,
                        status: 200,
                        latency_ms: started.elapsed().as_millis() as i64,
                        prompt_tokens,
                        completion_tokens,
                        stream: false,
                    }
                    .to_value(),
                );
                json_response(StatusCode::OK, message)
            }
        }
    }
}

/// 改写 + 中继一趟（供首次尝试与降级重试共用）。
#[allow(clippy::too_many_arguments)]
async fn run(
    state: &GatewayState,
    region: Region,
    raw: &str,
    model: &str,
    conversation_id: Option<String>,
    turn_key: Option<&str>,
    inbound_request_id: Option<&str>,
    trace_id: Option<String>,
) -> Result<RelayOutcome, relay::RelayFailure> {
    let (prepared_body, conversation_request_id) = relay::prepare_body(
        state,
        region,
        raw,
        "",
        conversation_id.as_deref(),
        turn_key,
        inbound_request_id,
    )
    .await;

    relay::relay(
        state,
        RelayRequest {
            region,
            kind: RelayKind::Llm,
            model: model.to_string(),
            prepared_body,
            conversation_request_id,
            conversation_id,
            trace_id,
        },
    )
    .await
}

/// 读取请求头（去空白；空值视为未提供）。
fn header_of(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}
