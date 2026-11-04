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
use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use buddy_switch_core::modules::loomy::region::{realm_spec, LoomyRegion};

use crate::pool::UpstreamEvent;
use crate::protocol::error_sniffer::{self, SniffedError};

use super::pool::{classify_http, LoomyErrKind};
use super::{LoomyGatewayState, LoomyKey};

/// 入站协议（与 `qoder::routes::Protocol` 同语义）。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    OpenAi,
    Anthropic,
    Responses,
}

/// 入站协议 → 出站转换。
fn protocol_error(protocol: Protocol, status: StatusCode, code: &str, message: &str) -> Response {
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

/// 一趟 Loomy chat 中继（认证之后的**全部逻辑**）—— 统一网关复用这一份。
///
/// 本函数只做**模型降级链的 hop 循环**（与 Trae / Qoder 同一份骨架）；
/// 真正的一次中继在 [`chat_for_region_once`]。没有配链时**逐字**走原路径。
pub async fn chat_for_region(
    state: LoomyGatewayState,
    region: LoomyRegion,
    body: Bytes,
    protocol: Protocol,
    key: Option<LoomyKey>,
) -> Response {
    let parsed: Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        // 非法 JSON 交给 `_once` 去报同一个 400（错误文案只有一份）。
        Err(_) => return chat_for_region_once(state, region, body, protocol, key).await,
    };
    let origin_model = model_of(&parsed);
    if origin_model.is_empty() {
        return chat_for_region_once(state, region, body, protocol, key).await;
    }

    let scope = buddy_switch_core::modules::product::scope_of_loomy_region(region);
    let fallback = crate::routes::fallback::FallbackLoop::plan(scope, &origin_model).await;
    // 没有可用链（绝大多数用户）⇒ 走原路径。
    if fallback.len() <= 1 {
        return chat_for_region_once(state, region, body, protocol, key).await;
    }

    let raw_body = String::from_utf8_lossy(&body).to_string();
    let mut hop = 0usize;
    let mut last: Option<Response> = None;
    let mut last_trigger: Option<crate::model_fallback::Trigger> = None;
    loop {
        // 越界**不回落主模型**：备选全被禁用时不能把主模型原样重发一次。
        let Some(hop_model) = fallback.model_at(hop) else {
            return last.expect("第 0 跳必然执行过");
        };
        /* 被用户禁用的目标直接跳过（一次请求都不发）。Loomy 的禁用表是 tokio Mutex，
        与 `FallbackLoop::should_skip`（收 RwLock）不同型 ⇒ 用同一份判定函数就地判。 */
        if hop > 0 {
            let skip = {
                let disabled = state.disabled.lock().await;
                crate::disabled_models::is_disabled(&disabled, scope, hop_model)
            };
            if skip {
                hop += 1;
                continue;
            }
        }
        let Some(hop_body) = fallback.body_for(hop, &raw_body) else {
            return last.expect("第 0 跳必然执行过");
        };
        let hop_model = hop_model.to_string();
        let hop_body = Bytes::from(hop_body);
        let hop_scope = fallback.scope_at(hop).unwrap_or(scope);
        /* 跨平台目标交给 `cross::relay_on_scope`（同进程内调目标产品线入口）；
        同作用域才走本产品线的一次中继。Loomy 单区域 ⇒ 同平台跨区域不存在。 */
        let response = if hop_scope.product() != scope.product() {
            Box::pin(crate::routes::cross::relay_on_scope(
                hop_scope,
                axum::http::HeaderMap::new(),
                hop_body,
                None,
            ))
            .await
        } else {
            chat_for_region_once(state.clone(), region, hop_body, protocol, key.clone()).await
        };

        if response.status().is_success() {
            let mut response = response;
            if hop > 0 {
                if let Some(trigger) = last_trigger {
                    fallback.apply_headers(hop, &mut response, trigger);
                    crate::model_fallback::record_success(
                        scope,
                        fallback.origin(),
                        &hop_model,
                        trigger,
                        // Loomy 的 hop 起点在 `_once` 内部，这里给不出精确耗时 ⇒ 0。
                        0,
                    );
                }
            }
            return response;
        }

        // 能不能继续降级：状态码能映射到触发条件，且该条件在用户配的白名单里。
        match fallback.can_fallback(&response) {
            Some(trigger) => last_trigger = Some(trigger),
            None => return response,
        }
        last = Some(response);
        hop += 1;
    }
}

/// 一次中继（不降级）：选号 → 发请求 → 换号。
async fn chat_for_region_once(
    state: LoomyGatewayState,
    region: LoomyRegion,
    body: Bytes,
    protocol: Protocol,
    key: Option<LoomyKey>,
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

    /* Responses 的 **custom（freeform）工具**名只从**原始**请求体取：
    Loomy 的 body 是直传上游的，但 `type: "custom"` 只有 Responses 入口才有 ——
    不取这一份，出站后客户端会收到 `function_call` 而不是它声明的 `custom_tool_call`
    （Codex 的 `apply_patch` 这类工具就会「静默不执行」）。
    与 Qoder 侧同一处取法（`protocol::responses::custom_tool_names`）。 */
    let responses_custom =
        std::sync::Arc::new(crate::protocol::responses::custom_tool_names(&parsed));

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

        let response = match chat_once_for_account(&state, region, &access_token, &outbound).await {
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
                        key.as_ref(),
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
                stream_relay(
                    state,
                    response,
                    protocol,
                    region,
                    &model,
                    &uid,
                    started,
                    responses_custom.clone(),
                    sticky_key.clone(),
                    key,
                )
                .await
            } else {
                aggregate_relay(
                    state,
                    response,
                    protocol,
                    region,
                    &model,
                    &uid,
                    started,
                    responses_custom.clone(),
                    key,
                )
                .await
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
                    key.as_ref(),
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
        format!(
            "全部账号都失败（{} 个）：{}",
            failures.len(),
            failures.join("；")
        )
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
            key.as_ref(),
        )
        .await;
    protocol_error(
        protocol,
        StatusCode::SERVICE_UNAVAILABLE,
        "no_available_account",
        &message,
    )
}

/// **单账号、单次**发一次 chat（不选号、不换号、不写池）。
///
/// 从 [`chat_for_region`] 的换号循环里抽出来的最小发送入口 —— **限流探测**必须用它：
/// 用 `chat_for_region` 会让探测自己触发换号轮换，并把失败**写进池冷却**
/// （探测自己把号打成冷却 = 自我实现的限流，方案红线 2）。
pub(crate) async fn chat_once_for_account(
    state: &LoomyGatewayState,
    region: LoomyRegion,
    access_token: &str,
    outbound: &str,
) -> Result<reqwest::Response, String> {
    let spec = realm_spec(region);
    let url = match &state.upstream_override {
        Some(host) => format!("{host}/chat/completions"),
        None => format!("{}/chat/completions", spec.api_base),
    };
    let mut request = state
        .http
        .post(&url)
        .timeout(buddy_switch_core::modules::loomy::api::CHAT_TIMEOUT)
        .body(outbound.to_string());
    // 两套认证头都发（chat 端点认 Bearer；照官方客户端两个都发）。
    for (name, value) in buddy_switch_core::modules::loomy::chat::chat_headers(access_token) {
        request = request.header(name, value);
    }
    request.send().await.map_err(|error| error.to_string())
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

// ---------------------------------------------------------------------------
// 独立端口（57896）的端点与鉴权 —— 认证之后的逻辑仍走 [`chat_for_region`]
// ---------------------------------------------------------------------------
//
// ⚠️ **认证之后的逻辑只有一份**：统一网关（57892）与独立端口（57896）的区别只在于
// 「谁做鉴权、模型名怎么解析」。路径与另三条产品线**逐字重叠** ⇒ 必须各自端口
// （axum 同路径 merge 会运行期 panic）。

/// `GET /health`（免鉴权；`/healthz` 为兼容别名，见 [`crate::HEALTH_PATHS`]）。
pub async fn health() -> Response {
    axum::Json(json!({ "ok": true })).into_response()
}

/// 鉴权中间件：健康检查免鉴权；其余端点校验 `Bearer <API Key>`。
///
/// 错误体形状按**端点**分派（`/v1/messages` 用 Anthropic 形状）—— 给 Anthropic 客户端
/// OpenAI 形状的话，它报的是「响应格式错误」而不是「凭据无效」，排查方向被带偏。
pub async fn bearer_auth(
    State(state): State<LoomyGatewayState>,
    mut request: Request,
    next: Next,
) -> Response {
    if crate::HEALTH_PATHS.contains(&request.uri().path()) {
        return next.run(request).await;
    }
    let protocol = if request.uri().path() == "/v1/messages" {
        Protocol::Anthropic
    } else {
        Protocol::OpenAi
    };

    let presented = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .and_then(|value| {
            value
                .strip_prefix("Bearer ")
                .or_else(|| value.strip_prefix("bearer "))
                .map(str::trim)
        })
        .filter(|value| !value.is_empty());

    let Some(key) = presented else {
        return protocol_error(
            protocol,
            StatusCode::UNAUTHORIZED,
            "invalid_request_error",
            "缺少凭据：请在 Authorization 头携带 `Bearer <API Key>`（在「网关 → 密钥」页创建）",
        );
    };
    if let Some(record) = state.key_store.verify(key) {
        // ★ 请求日志要记「哪把 Key、哪套台账」——与 WB / Trae / Qoder 同形。
        //   独立端口（57896）来的记 `loomy`；统一网关（57892）转发时由它对侧注入 `unified`。
        request.extensions_mut().insert(LoomyKey {
            id: record.id,
            scope: "loomy",
        });
        next.run(request).await
    } else {
        protocol_error(
            protocol,
            StatusCode::UNAUTHORIZED,
            "authentication_error",
            "凭据无效或已吊销：请在「网关 → 密钥」页确认 API Key",
        )
    }
}

/// `GET /status`（管理面；走同一套鉴权）。
pub async fn status(State(state): State<LoomyGatewayState>) -> Response {
    let config = state.config_snapshot().await;
    let now = buddy_switch_core::modules::config::now_ms();
    // 管理面读取顺手刷新读数（**不阻塞本响应**，自带闸门）——
    // ⚠️ 只限管理面：`relay`（推理路径）绝不调它，见 `loomy::request_credits_refresh`。
    crate::loomy::request_credits_refresh(&state);
    let entries = {
        let mut pools = state.pools.lock().await;
        let pool = pools.entry(LoomyRegion::Cn).or_default();
        pool.sync_for(LoomyRegion::Cn);
        pool.accounts_view(now)
    };
    axum::Json(json!({
        "enabled": config.enabled,
        "port": config.port,
        "defaultModel": config.default_model,
        "totalRequests": state.total_requests.load(std::sync::atomic::Ordering::Relaxed),
        "lastError": state.last_error.read().await.clone(),
        "startedAt": state.started_at,
        "keyCount": state.key_store.load().iter().filter(|key| !key.is_revoked()).count(),
        "accounts": entries,
    }))
    .into_response()
}

/// `GET /v1/models`：上游清单 + 禁用过滤（**唯一判断点**）。
pub async fn models(State(state): State<LoomyGatewayState>) -> Response {
    let catalog = state.catalog_for().await;
    let scope = buddy_switch_core::modules::product::scope_of_loomy_region(LoomyRegion::Cn);
    let disabled = state.disabled.lock().await;
    let filtered = models_response(&catalog.models, disabled.get(scope));
    drop(disabled);
    let mut body = filtered;
    if let Some(note) = catalog.note {
        body["note"] = Value::String(note);
    }
    body["source"] = Value::String(catalog.source.to_string());
    axum::Json(body).into_response()
}

/// 模型清单出口（OpenAI `{object:"list", data:[…]}`）。
///
/// Loomy 的清单条目已经是 core 归一后的形状（`id` / `display_name` / `context_length`），
/// 这里只做**禁用过滤**与字段补齐 —— 不重新解释上游字段（那会与 `/models` 解析漂移）。
fn models_response(catalog: &[Value], disabled: &std::collections::BTreeSet<String>) -> Value {
    let data: Vec<Value> = catalog
        .iter()
        .filter(|entry| {
            let id = entry.get("id").and_then(Value::as_str).unwrap_or("");
            !crate::disabled_models::is_disabled_in(disabled, id)
        })
        .map(|entry| {
            let id = entry.get("id").and_then(Value::as_str).unwrap_or("");
            json!({
                "id": id,
                "object": "model",
                "created": 0,
                "owned_by": "loomy",
                "display_name": entry
                    .get("display_name")
                    .cloned()
                    .unwrap_or_else(|| Value::String(id.to_string())),
                "context_length": entry.get("context_length").cloned(),
                "supports_image": entry.get("supports_image").cloned(),
            })
        })
        .collect();
    json!({ "object": "list", "data": data })
}

/// `POST /v1/chat/completions`。
pub async fn chat_completions(
    State(state): State<LoomyGatewayState>,
    // 中间件校验通过后写进扩展的 Key 归属（用于请求日志的 `keyId` / `keyScope`）。
    key: Option<axum::Extension<LoomyKey>>,
    body: Bytes,
) -> Response {
    chat_for_region(
        state,
        LoomyRegion::Cn,
        body,
        Protocol::OpenAi,
        key.map(|axum::Extension(key)| key),
    )
    .await
}

/// `POST /v1/messages`。
pub async fn messages(
    State(state): State<LoomyGatewayState>,
    key: Option<axum::Extension<LoomyKey>>,
    body: Bytes,
) -> Response {
    chat_for_region(
        state,
        LoomyRegion::Cn,
        body,
        Protocol::Anthropic,
        key.map(|axum::Extension(key)| key),
    )
    .await
}

/// `POST /v1/responses`（OpenAI Responses API；Codex CLI 的 `wire_api = "responses"`）。
pub async fn responses(
    State(state): State<LoomyGatewayState>,
    key: Option<axum::Extension<LoomyKey>>,
    body: Bytes,
) -> Response {
    chat_for_region(
        state,
        LoomyRegion::Cn,
        body,
        Protocol::Responses,
        key.map(|axum::Extension(key)| key),
    )
    .await
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
    custom: std::sync::Arc<std::collections::HashSet<String>>,
    sticky_key: Option<String>,
    key: Option<LoomyKey>,
) -> Response {
    let stream_model = model.to_string();
    let usage_slot = std::sync::Arc::new(std::sync::Mutex::new(None::<Value>));
    /* ★ issue #129：流中途的失败必须能**回写治理**（冷却 / 粘性解绑 / 日志状态）。
    改造前这里只旁听 usage：上游在流里报错，日志仍写「200 / 无错误」、账号照旧
    留在池里 ⇒ 客户端重试再撞同一个号（「日志全 429、池卡全绿」）。

    ⚠️ 本改动**不改对外字节**：错误帧仍按上游原样透传（客户端本来就能看到），
    我们只是把同一次失败如实记进池与日志 —— 因此不需要 `auto_failover` 开关。 */
    let error_slot: std::sync::Arc<std::sync::Mutex<Option<SniffedError>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));
    let state_for_task = state.clone();
    let uid = uid.to_string();
    let model_for_task = model.to_string();

    // 协议转换（Anthropic / Responses 入口）排在外层 —— 上游是标准 OpenAI SSE。
    let raw = response.bytes_stream();
    let tapped = OpenAiUsageTap::new(raw, usage_slot.clone(), error_slot.clone());
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
            custom.clone(),
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
        let failure = error_slot.lock().ok().and_then(|guard| guard.clone());
        let (log_status, log_error) = match &failure {
            Some(error) => (error.status, Some(error.message.clone())),
            None => (200u16, None),
        };
        /* ★ issue #129：流内失败必须**写冷却 + 解绑粘性**。改造前这里恒写 200、
        账号在池里始终干净 ⇒ 客户端重试再撞同一个号，表现为「日志全 429、池卡全绿」。
        与「不发错误帧」并不冲突：客户端看到的是上游原文，治理只影响本地池。 */
        if let Some(error) = &failure {
            let kind = classify_http(error.status);
            {
                let mut pools = state_for_task.pools.lock().await;
                if let Some(pool) = pools.get_mut(&region) {
                    pool.apply_error(&uid, kind, &error.message);
                }
            }
            if let Some(key) = sticky_key.as_deref() {
                state_for_task.sticky.lock().await.unbind(key);
            }
        }
        state_for_task
            .record_request(
                "chat",
                region,
                &model_for_task,
                log_status,
                &uid,
                started.elapsed().as_millis() as i64,
                true,
                prompt_tokens as i64,
                completion_tokens as i64,
                log_error,
                key.as_ref(),
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
    custom: std::sync::Arc<std::collections::HashSet<String>>,
    key: Option<LoomyKey>,
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
            key.as_ref(),
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
        Protocol::Responses => axum::Json(crate::protocol::responses::to_response_object_from(
            &crate::protocol::responses::ResponsesParts {
                content: &aggregated.content,
                reasoning: &aggregated.reasoning,
                tool_calls: &aggregated.tool_calls,
                model: aggregated.model.as_deref(),
                created_at: aggregated.created,
                input_tokens: prompt_tokens,
                output_tokens: completion_tokens,
                custom: custom.as_ref(),
                /* 上游原始 usage：Responses 出口从这里取缓存命中与思考细节，
                否则这些字段在 Loomy 侧永远缺失（与 WB/Trae/Qoder 的出口同口径）。 */
                raw_usage: usage.as_ref(),
            },
            model,
        ))
        .into_response(),
    }
}

/// 标准 OpenAI SSE 的 usage 旁路：逐帧透传，同时记录末帧的 `usage`。
pub struct OpenAiUsageTap<S> {
    inner: S,
    usage_slot: std::sync::Arc<std::sync::Mutex<Option<Value>>>,
    /// 流内观测到的上游错误（第一条为准；`None` = 正常收尾）。
    error_slot: std::sync::Arc<std::sync::Mutex<Option<SniffedError>>>,
}

impl<S> OpenAiUsageTap<S> {
    pub fn new(
        inner: S,
        usage_slot: std::sync::Arc<std::sync::Mutex<Option<Value>>>,
        error_slot: std::sync::Arc<std::sync::Mutex<Option<SniffedError>>>,
    ) -> Self {
        Self {
            inner,
            usage_slot,
            error_slot,
        }
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
                    /* ★ issue #129：上游在流里报错时先留证（不改字节）。 */
                    if let Some(error) = error_sniffer::sniff_payload(data) {
                        if let Ok(mut guard) = self.error_slot.lock() {
                            if guard.is_none() {
                                *guard = Some(error);
                            }
                        }
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
                std::task::Poll::Ready(Some(Err(error))) => {
                    /* 传输层中断不是「模型说完了」：同样留证，供收尾时写冷却与日志。 */
                    if let Ok(mut guard) = self.error_slot.lock() {
                        if guard.is_none() {
                            *guard = Some(SniffedError {
                                event: UpstreamEvent::Server,
                                message: format!("上游连接中断：{error:?}"),
                                status: 502,
                            });
                        }
                    }
                    std::task::Poll::Ready(Some(Err(std::io::Error::other(format!("{error:?}")))))
                }
                done => done.map(|opt| {
                    opt.map(|item| {
                        item.map_err(|error| std::io::Error::other(format!("{error:?}")))
                    })
                }),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;

    /// ★ issue #129：流内错误帧必须被**留证**（收尾据此写冷却与日志），
    /// 且透传字节**逐字不变** —— 错误帧本来就交给客户端，治理只影响本地池。
    #[tokio::test]
    async fn usage_tap_records_upstream_error_frames() {
        let usage_slot = std::sync::Arc::new(std::sync::Mutex::new(None::<Value>));
        let error_slot = std::sync::Arc::new(std::sync::Mutex::new(None::<SniffedError>));
        let frames: Vec<Result<axum::body::Bytes, std::io::Error>> = vec![
            Ok(axum::body::Bytes::from_static(
                b"data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n",
            )),
            Ok(axum::body::Bytes::from_static(
                b"data: {\"choices\":[{\"delta\":{}}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":4}}\n\n",
            )),
            Ok(axum::body::Bytes::from_static(
                b"data: {\"error\":{\"message\":\"rate limited\",\"type\":\"rate_limit_error\"}}\n\n",
            )),
        ];
        let expected = frames.len();
        let mut tap = OpenAiUsageTap::new(
            futures_util::stream::iter(frames),
            usage_slot.clone(),
            error_slot.clone(),
        );

        let mut passed = 0usize;
        while let Some(item) = tap.next().await {
            item.expect("tap 不得改写上游错误");
            passed += 1;
        }

        assert_eq!(passed, expected, "透传帧数必须不变（不改对外字节）");
        let usage = usage_slot
            .lock()
            .unwrap()
            .clone()
            .expect("usage 帧仍要记账");
        assert_eq!(usage["prompt_tokens"], 3);

        let error = error_slot
            .lock()
            .unwrap()
            .clone()
            .expect("错误帧必须被留证");
        assert_eq!(error.status, 429, "限流错误应归到 429");
        assert!(error.message.contains("rate limited"));
    }
}
