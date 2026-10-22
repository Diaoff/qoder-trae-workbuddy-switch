//! Loomy 的**出站 HTTP**（业务端点：models / points + 账号端点登录）。
//!
//! 与 `qoder/api.rs` 同款定位：core 层持有协议（头 / 信封 / 认证），
//! 网关与宿主只做装配。蓝本 `../free-llm-router/src/providers/loomy.ts`（协议已实测）。
//!
//! ## 调用方纪律
//!
//! - 签名与发送共用同一份 body 字符串（`signing.rs` 的调用方纪律，见 `send_sms_code`）；
//! - 业务失败**恒 HTTP 200**，成败只能读 body 的 `code`（`chat.rs` 模块头）；
//! - `100002` 登录失效 ⇒ `UpstreamErrorKind::SessionDead`，**不重试**。

use std::time::Duration;

use serde_json::Value;

use crate::modules::config::now_ms;
use crate::modules::loomy::chat::{business_headers, parse_envelope};
use crate::modules::loomy::region::{realm_spec, LoomyRegion};
use crate::modules::upstream::UpstreamErrorKind;

/// 业务端点请求超时（chat 用 [`CHAT_TIMEOUT`]）。
const BUSINESS_TIMEOUT: Duration = Duration::from_secs(60);
/// chat 请求超时（长回答不能被掐断）。
pub const CHAT_TIMEOUT: Duration = Duration::from_secs(600);

/// 签到端点路径。
pub const PATH_CHECKIN: &str = "/points/first-login";
/// 发送短信验证码端点路径（账号基址）。
pub const PATH_SEND_SMS: &str = "/login/phone/sendMsgCode";
/// 验证码登录端点路径（账号基址）。
pub const PATH_CHECK_SMS: &str = "/login/phone/checkCode";
/// 微信授权第 1 步：用微信 code 换 rcode（账号基址）。
pub const PATH_WECHAT_BIND_AUTH: &str = "/login/thirdAccount/bind/auth";
/// 微信授权第 3a 步：向待绑定的手机号下发验证码（账号基址）。
pub const PATH_WECHAT_BIND_SEND_MSG: &str = "/login/thirdAccount/bind/sendMsg";
/// 微信授权第 3b 步：验证短信验证码，通过即完成绑定 + 登录（账号基址）。
pub const PATH_WECHAT_BIND_CHECK_CODE: &str = "/login/thirdAccount/bind/checkCode";
/// 微信授权第 4 步：已绑手机号时跳过绑定，直接换 session（账号基址）。
pub const PATH_WECHAT_BIND_SKIP: &str = "/login/thirdAccount/bind/skip";

/// 共享出站客户端（SSE 对话不设总超时 —— 时长由模型输出决定）。
pub fn shared_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(300))
        .pool_max_idle_per_host(20)
        .build()
        .map_err(|error| format!("构建 HTTP 客户端失败：{error}"))
}

/// 把 `Vec<(String, String)>` 头应用到请求上。
fn apply_headers(
    mut request: reqwest::RequestBuilder,
    headers: &[(String, String)],
) -> reqwest::RequestBuilder {
    for (name, value) in headers {
        request = request.header(name, value);
    }
    request
}

/// 业务码 → 错误分类（网关按此冷却 / 换号）。
pub fn classify_code(code: &str) -> UpstreamErrorKind {
    let spec = realm_spec(LoomyRegion::Cn);
    if code == spec.auth_error_code {
        return UpstreamErrorKind::SessionDead;
    }
    if code == spec.bad_request_code {
        return UpstreamErrorKind::Client;
    }
    UpstreamErrorKind::Server
}

/// 读取一个业务端点响应：非 2xx 直接 `Err`；2xx 后拆业务信封。
async fn read_business_response(response: reqwest::Response, what: &str) -> Result<Value, String> {
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| format!("读取{what}响应：{error}"))?;
    if !status.is_success() {
        return Err(format!(
            "{what} HTTP {} {}",
            status.as_u16(),
            text.chars().take(160).collect::<String>()
        ));
    }
    let payload: Value =
        serde_json::from_str(&text).map_err(|error| format!("{what}响应不是 JSON：{error}"))?;
    let envelope = parse_envelope(payload);
    if !envelope.ok {
        return Err(format!("Loomy {what}失败：{}", envelope.message));
    }
    Ok(envelope.data)
}

/// 拉取模型目录：`GET {api_base}/models`（业务头；三形态解析见 `models.rs`）。
///
/// 返回规范化后的模型条目数组（供网关清单出口直接用）。**空目录显式失败**
/// （不冒充成功 —— 上游响应形状可能已变化）。
pub async fn fetch_models_at(api_base: &str, token: &str) -> Result<Vec<Value>, String> {
    let client = shared_client()?;
    let url = format!("{api_base}/models");
    let request = apply_headers(
        client.get(&url).timeout(BUSINESS_TIMEOUT),
        &business_headers(token),
    );
    let response = request
        .send()
        .await
        .map_err(|error| format!("请求 Loomy 模型目录：{error}"))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| format!("读取模型目录响应：{error}"))?;
    if !status.is_success() {
        return Err(format!(
            "模型目录 HTTP {} {}",
            status.as_u16(),
            text.chars().take(160).collect::<String>()
        ));
    }
    let payload: Value =
        serde_json::from_str(&text).map_err(|error| format!("模型目录不是 JSON：{error}"))?;
    let models = crate::modules::loomy::models::parse_models(&payload);
    if models.is_empty() {
        return Err(
            "Loomy 模型目录为空：上游响应形状可能已变化（期望 data[] 且条目 type === \"chat\"）"
                .to_string(),
        );
    }
    Ok(models
        .into_iter()
        .map(|model| {
            serde_json::json!({
                "id": model.id,
                "display_name": model.name,
                "context_length": model.context_window,
                "supports_image": model.supports_image,
            })
        })
        .collect())
}

/// 查询两池积分：`GET {api_base}/points/records?...`。
pub async fn fetch_points(
    api_base: &str,
    token: &str,
) -> Result<crate::modules::loomy::credits::CreditsSnapshot, String> {
    let client = shared_client()?;
    let url = format!("{api_base}/points/records?pageNo=1&pageSize=1&recordType=all");
    let request = apply_headers(
        client.get(&url).timeout(BUSINESS_TIMEOUT),
        &business_headers(token),
    );
    let response = request
        .send()
        .await
        .map_err(|error| format!("请求 Loomy 积分：{error}"))?;
    let data = read_business_response(response, "积分").await?;
    crate::modules::loomy::credits::parse_credits_data(&data)
}

/// 触发每日签到：`POST {api_base}/points/first-login`（幂等）。
pub async fn post_checkin(
    api_base: &str,
    token: &str,
) -> Result<crate::modules::loomy::credits::CheckinOutcome, String> {
    let client = shared_client()?;
    let url = format!("{api_base}{PATH_CHECKIN}");
    let request = apply_headers(
        client
            .post(&url)
            .timeout(BUSINESS_TIMEOUT)
            .header("Content-Type", "application/json"),
        &business_headers(token),
    )
    .body(crate::modules::loomy::checkin::CHECKIN_BODY);
    let response = request
        .send()
        .await
        .map_err(|error| format!("请求 Loomy 签到：{error}"))?;
    let data = read_business_response(response, "签到").await?;
    Ok(crate::modules::loomy::credits::parse_checkin_data(&data))
}

/// UTC 的 HTTP `Date` 头值（RFC 7231 形态，与官方客户端一致）。
fn http_date_now() -> String {
    let now = chrono::Utc::now();
    now.format("%a, %d %b %Y %H:%M:%S GMT").to_string()
}

/// 新的 `Nonce`（uuid）。
fn new_nonce() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// 讯飞账号端点的请求体信封 `{base, param}`（登录链路专用）。
fn build_account_body(region: LoomyRegion, param: Value) -> Value {
    crate::modules::loomy::oauth::build_account_body(region, param)
}

/// 发送一次讯飞账号端点请求并拆业务信封（签名与发送共用同一份 body 字符串）。
async fn post_caccount(
    region: LoomyRegion,
    path: &str,
    param: Value,
) -> Result<Value, String> {
    let spec = realm_spec(region);
    let body_value = build_account_body(region, param);
    let body = serde_json::to_string(&body_value).map_err(|error| format!("序列化失败：{error}"))?;
    let headers = crate::modules::loomy::signing::sign_caccount_request(
        region,
        &crate::modules::loomy::signing::SignInput {
            method: "POST",
            path,
            query_params: &[],
            body: &body,
            content_type: "application/json",
            date: &http_date_now(),
            nonce: &new_nonce(),
        },
    );
    let client = shared_client()?;
    let url = format!("{}{path}", spec.account_base);
    let mut header_pairs: Vec<(String, String)> = vec![
        ("Authorization".to_string(), headers.authorization.clone()),
        ("Date".to_string(), headers.date.clone()),
        ("Nonce".to_string(), headers.nonce.clone()),
        ("Content-Type".to_string(), "application/json".to_string()),
    ];
    if !headers.content_md5.is_empty() {
        header_pairs.push(("Content-MD5".to_string(), headers.content_md5.clone()));
    }
    let request = apply_headers(client.post(&url).timeout(BUSINESS_TIMEOUT), &header_pairs)
        .body(body);
    let response = request
        .send()
        .await
        .map_err(|error| format!("请求讯飞账号端点：{error}"))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| format!("读取账号端点响应：{error}"))?;
    if !status.is_success() {
        return Err(format!(
            "讯飞账号端点 HTTP {} {}",
            status.as_u16(),
            text.chars().take(160).collect::<String>()
        ));
    }
    let payload: Value = serde_json::from_str(&text)
        .map_err(|error| format!("账号端点响应不是 JSON：{error}"))?;
    let envelope = parse_envelope(payload);
    if !envelope.ok {
        return Err(envelope.message);
    }
    Ok(envelope.data)
}

/// 下发短信验证码；返回 `msgid`（校验时必须原样带回；缺失 ⇒ 显式报错）。
pub async fn send_sms_code(region: LoomyRegion, phone: &str) -> Result<String, String> {
    let spec = realm_spec(region);
    let param = serde_json::json!({
        "ccode": "86",
        "phone": phone,
        "expire": spec.sms_code_ttl_seconds,
    });
    let data = post_caccount(region, PATH_SEND_SMS, param).await?;
    let msgid = data
        .get("msgid")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if msgid.is_empty() {
        // ⚠️ 不返回空串：上层会拿它去登录，服务端必然报「msgid 无效」。
        return Err("短信验证码响应缺少 msgid".to_string());
    }
    Ok(msgid)
}

/// 用短信验证码登录；成功返回 session / userid / phone。
pub async fn check_sms_code(
    region: LoomyRegion,
    phone: &str,
    code: &str,
    msgid: &str,
) -> Result<(String, String, String), String> {
    let spec = realm_spec(region);
    let param = serde_json::json!({
        "ccode": "86",
        "phone": phone,
        "mcode": code,
        "msgid": msgid,
        "expire": spec.session_ttl_seconds,
    });
    let data = post_caccount(region, PATH_CHECK_SMS, param).await?;
    let fields = crate::modules::loomy::oauth::parse_login_response(&data)?;
    Ok(fields)
}

/// 按官方基址拉模型目录（e2e 请走 `fetch_models_at` 挂载点）。
pub async fn fetch_models(region: LoomyRegion, token: &str) -> Result<Vec<Value>, String> {
    fetch_models_at(realm_spec(region).api_base, token).await
}

// ── 微信扫码（二期）：拉授权页 / 长轮询 / 四步 bind ──────────────────
//
// 纯函数部分在 [`super::wechat`]（可合成夹具单测）；本节只做网络发送。
// 授权页与长轮询打微信开放平台（不是 account_base）；四步 bind 打账号基址、
// 复用 [`post_caccount`] 的 HMAC-SHA1 签名通道（与短信登录同一份签名逻辑）。

/// 拉取微信授权页并提取二维码 uuid。
///
/// 授权页直接内嵌 uuid（无需执行 JS），因此纯 GET 即可；提取不到时显式报错
/// （绝不下发空 uuid —— 那会让前端轮询永远 waiting，用户看不到任何原因）。
pub async fn fetch_wechat_uuid(region: LoomyRegion, state: &str) -> Result<(String, String), String> {
    let spec = realm_spec(region);
    let url = crate::modules::loomy::wechat::auth_url(region, state);
    let client = shared_client()?;
    let response = client
        .get(&url)
        .timeout(Duration::from_secs(30))
        .header("User-Agent", spec.wechat_http_ua)
        .header("Referer", "https://open.weixin.qq.com/")
        .send()
        .await
        .map_err(|error| format!("微信授权页拉取失败：{error}"))?;
    let status = response.status();
    let html = response
        .text()
        .await
        .map_err(|error| format!("读取微信授权页响应：{error}"))?;
    if !status.is_success() {
        return Err(format!("微信授权页 HTTP {}", status.as_u16()));
    }
    let uuid = crate::modules::loomy::wechat::extract_uuid(&html)
        .ok_or_else(|| "微信授权页未包含二维码 uuid（页面结构可能已变化）".to_string())?;
    let qr = crate::modules::loomy::wechat::qr_image_url(&uuid);
    Ok((uuid, qr))
}

/// 一次微信长轮询（单帧）。
///
/// 网络异常**不抛错**（返回 `Err` 由调用方决定是否继续轮询 —— 长轮询偶发失败
/// 不该终止整个登录流程）；协议层状态用 [`WechatPollStatus`] 表达。
pub async fn poll_wechat_once(
    region: LoomyRegion,
    uuid: &str,
    last_errcode: &str,
) -> Result<crate::modules::loomy::wechat::WechatPollStatus, String> {
    let spec = realm_spec(region);
    let url = crate::modules::loomy::wechat::poll_url(uuid, last_errcode);
    let client = shared_client()?;
    let response = client
        .get(&url)
        .timeout(Duration::from_millis(spec.wechat_poll_timeout_ms))
        .header("User-Agent", spec.wechat_http_ua)
        .header("Referer", "https://open.weixin.qq.com/")
        .send()
        .await
        .map_err(|error| format!("微信长轮询失败：{error}"))?;
    let body = response
        .text()
        .await
        .map_err(|error| format!("读取微信长轮询响应：{error}"))?;
    Ok(crate::modules::loomy::wechat::parse_poll(&body))
}

/// 微信授权第 1 步：用微信 code 换 rcode，并得知是否已绑手机号。
///
/// ⚠️ 微信 code **只在这一步用一次**，后续三步只用 `rcode`。
pub async fn wechat_bind_auth(
    region: LoomyRegion,
    code: &str,
) -> Result<crate::modules::loomy::wechat::BindAuth, String> {
    let param = serde_json::json!({ "tcode": { "code": code }, "type": "wx" });
    let data = post_caccount(region, PATH_WECHAT_BIND_AUTH, param).await?;
    crate::modules::loomy::wechat::parse_bind_auth(&data)
}

/// 微信授权第 3a 步：向待绑定的手机号下发验证码；返回 `msgid`。
pub async fn wechat_bind_send_msg(
    region: LoomyRegion,
    rcode: &str,
    phone: &str,
) -> Result<String, String> {
    let spec = realm_spec(region);
    let param = serde_json::json!({
        "rcode": rcode,
        "phone": phone,
        "ccode": "86",
        "expire": spec.sms_code_ttl_seconds,
    });
    let data = post_caccount(region, PATH_WECHAT_BIND_SEND_MSG, param).await?;
    let msgid = data
        .get("msgid")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if msgid.is_empty() {
        // ⚠️ 不返回空串：上层会拿它去校验，服务端必然报「msgid 无效」。
        return Err("绑定手机号响应缺少 msgid".to_string());
    }
    Ok(msgid)
}

/// 微信授权第 3b 步：验证短信验证码，通过即完成绑定 + 登录；返回 session / userid / phone。
pub async fn wechat_bind_check_code(
    region: LoomyRegion,
    rcode: &str,
    code: &str,
    msgid: &str,
) -> Result<(String, String, String), String> {
    let spec = realm_spec(region);
    let param = serde_json::json!({
        "rcode": rcode,
        "mcode": code,
        "msgid": msgid,
        "expire": spec.session_ttl_seconds,
    });
    let data = post_caccount(region, PATH_WECHAT_BIND_CHECK_CODE, param).await?;
    crate::modules::loomy::oauth::parse_login_response(&data)
}

/// 微信授权第 4 步（已绑手机号时走）：跳过绑定，直接换 session。
///
/// ⚠️ 本路径**拿不到手机号** —— uid 优先用 userid（本路径一定有），凭据判据看 session，两不相干。
pub async fn wechat_bind_skip(
    region: LoomyRegion,
    rcode: &str,
) -> Result<(String, String), String> {
    let spec = realm_spec(region);
    let param = serde_json::json!({ "rcode": rcode, "expire": spec.session_ttl_seconds });
    let data = post_caccount(region, PATH_WECHAT_BIND_SKIP, param).await?;
    let (session, userid, _phone) = crate::modules::loomy::oauth::parse_login_response(&data)?;
    Ok((session, userid))
}

/// 按官方基址查积分。
pub async fn points_for(region: LoomyRegion, token: &str) -> Result<crate::modules::loomy::credits::CreditsSnapshot, String> {
    fetch_points(realm_spec(region).api_base, token).await
}

/// 按官方基址签到。
pub async fn checkin_for(region: LoomyRegion, token: &str) -> Result<crate::modules::loomy::credits::CheckinOutcome, String> {
    post_checkin(realm_spec(region).api_base, token).await
}

/// 供宿主把登录账号落库（薄封装：组装 + upsert）。
pub fn account_from_login_fields(
    region: LoomyRegion,
    session: String,
    userid: String,
    phone: String,
) -> Result<crate::modules::loomy::account::RawAccount, String> {
    crate::modules::loomy::oauth::account_from_login(region, session, userid, phone)
        .map(|(account, _uid)| account)
}

/// 供日志 / 诊断：当前毫秒时间戳（避免各调用方重复 std::time 样板）。
pub fn timestamp_ms() -> i64 {
    now_ms()
}

/// 生成一次性的 state / trace 标识（去连字符 uuid，32 位 hex）。
///
/// server 宿主不直接依赖 `uuid` crate；微信 `state` 等一次性标识统一从这里取。
pub fn new_trace_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}
