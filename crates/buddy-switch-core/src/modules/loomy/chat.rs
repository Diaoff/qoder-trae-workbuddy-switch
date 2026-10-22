//! Loomy 的**请求头、业务信封与错误分类**。
//!
//! ## 协议纪律（全部实测，蓝本 `../free-llm-router/src/providers/loomy.ts`）
//!
//! 1. **两套认证头**：chat 端点认 `Authorization: Bearer`（照官方客户端两个都发），
//!    业务端点（`/models`、`/points/*`）只认小写 `token` 头 —— 带错头得到
//!    **HTTP 200** + `{"code":"100002"}`。
//! 2. **业务失败恒 HTTP 200**，成败只能读 body 的 `code`。只看状态码会把
//!    「登录已失效」的恒 200 响应误判成成功。
//! 3. **登录失效码 `100002` 收到不得重试**（同一死凭据重试无意义）
//!    ⇒ 映射 [`UpstreamErrorKind::SessionDead`]，由网关换号而不是重放。
//!
//! 端点常量见 [`super::region::realm_spec`]；`/chat/completions` 的流式转发
//! 由 gateway 侧（`buddy-switch-gateway/src/loomy/`）完成，本模块只提供
//! 头构造、信封解析与分类等**纯逻辑**，方便单测。

use serde_json::Value;

use crate::modules::loomy::region::{realm_spec, LoomyRegion};
use crate::modules::upstream::UpstreamErrorKind;

/// 业务响应信封（成败只能读 body 的 `code`）。
#[derive(Debug, Clone)]
pub struct Envelope {
    /// 成功与否（`code === "000000"`）。
    pub ok: bool,
    /// 业务码（响应顶层无 `code` 时空串 —— 见 `models.rs` 的三形态说明）。
    pub code: String,
    /// 可读消息（`desc` / `message`）。
    pub message: String,
    /// 业务数据（成功时）。
    pub data: Value,
}

/// 解析业务信封：业务失败**恒 HTTP 200**，成败只能读 body 的 `code`。
pub fn parse_envelope(payload: Value) -> Envelope {
    let Some(obj) = payload.as_object() else {
        return Envelope {
            ok: false,
            code: String::new(),
            message: "响应不是 JSON 对象".to_string(),
            data: Value::Null,
        };
    };
    let code = obj
        .get("code")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let message = obj
        .get("desc")
        .or_else(|| obj.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let spec = realm_spec(LoomyRegion::Cn);
    let ok = code == spec.ok_code;
    let message = if ok {
        message
    } else {
        describe_code(&code, &message)
    };
    Envelope {
        ok,
        code,
        message,
        data: obj.get("data").cloned().unwrap_or(Value::Null),
    }
}

/// 把业务码映射成可读失败说明（含「该怎么办」）。
fn describe_code(code: &str, message: &str) -> String {
    let spec = realm_spec(LoomyRegion::Cn);
    if code == spec.auth_error_code {
        return format!("登录已失效（{code} {message}）：Loomy 没有续期端点，请重新登录");
    }
    if message.is_empty() {
        format!("上游业务错误 {code}")
    } else {
        format!("上游业务错误 {code}：{message}")
    }
}

/// 把业务码映射到本仓的错误分类（网关按此换号 / 冷却）。
pub fn classify_code(code: &str) -> Option<UpstreamErrorKind> {
    let spec = realm_spec(LoomyRegion::Cn);
    if code.is_empty() {
        return None;
    }
    if code == spec.auth_error_code {
        // 登录失效：同一死凭据重试无意义 ⇒ 网关换号路径。
        return Some(UpstreamErrorKind::SessionDead);
    }
    if code == spec.bad_request_code {
        return Some(UpstreamErrorKind::Client);
    }
    Some(UpstreamErrorKind::Server)
}

/// 业务请求头（只发小写 `token`，**刻意不加** Authorization —— 见模块头第 1 条）。
pub fn business_headers(token: &str) -> Vec<(String, String)> {
    vec![
        ("Accept".to_string(), "application/json".to_string()),
        ("token".to_string(), token.to_string()),
    ]
}

/// chat 请求头（**两个都发**，照官方客户端；`Bearer ` 前缀必需 —— 实测无前缀回 100002）。
pub fn chat_headers(token: &str) -> Vec<(String, String)> {
    vec![
        ("Accept".to_string(), "text/event-stream".to_string()),
        ("Content-Type".to_string(), "application/json".to_string()),
        ("Authorization".to_string(), format!("Bearer {token}")),
        ("token".to_string(), token.to_string()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn envelope_reads_the_code_not_the_status() {
        let ok = parse_envelope(json!({"code": "000000", "desc": "成功", "data": {"a": 1}}));
        assert!(ok.ok);
        assert_eq!(ok.data["a"], 1);

        let dead = parse_envelope(json!({"code": "100002", "desc": "缺少 token"}));
        assert!(!dead.ok);
        assert!(
            dead.message.contains("重新登录"),
            "失效码的文案要给出路：{}",
            dead.message
        );

        // 非对象 / 无 code：如实失败，不冒充成功。
        assert!(!parse_envelope(json!(null)).ok);
        assert!(!parse_envelope(json!({"data": []})).ok);
    }

    #[test]
    fn classify_maps_auth_error_to_session_dead() {
        assert_eq!(
            classify_code("100002"),
            Some(UpstreamErrorKind::SessionDead)
        );
        assert_eq!(classify_code("100001"), Some(UpstreamErrorKind::Client));
        assert_eq!(classify_code("999999"), Some(UpstreamErrorKind::Server));
        assert_eq!(classify_code(""), None, "无 code（如 /models 的裸列表）不算业务失败");
    }

    #[test]
    fn headers_follow_the_two_scheme_discipline() {
        let business = business_headers("tok");
        assert_eq!(business.len(), 2);
        assert_eq!(business[0].0, "Accept");
        assert_eq!(business[1].0, "token");
        assert!(
            !business.iter().any(|(k, _)| k == "Authorization"),
            "业务端点刻意不发 Authorization"
        );

        let chat = chat_headers("tok");
        let has = |name: &str| chat.iter().any(|(k, _)| k == name);
        assert!(has("Authorization"), "chat 端点两个头都发");
        assert!(has("token"));
        let auth = chat
            .iter()
            .find(|(k, _)| k == "Authorization")
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        assert_eq!(auth, "Bearer tok", "Bearer 前缀必需（实测无前缀回 100002）");
    }
}
