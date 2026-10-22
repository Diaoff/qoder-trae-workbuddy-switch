//! Loomy 的微信扫码登录 —— **纯函数层**（URL 拼接 / uuid 提取 / 长轮询帧解析）。
//!
//! ## 为什么微信扫码不需要浏览器与本地监听
//!
//! 微信授权页（`open.weixin.qq.com/connect/qrconnect`）**直接内嵌**二维码 uuid，
//! 无需执行 JS；长轮询端点（`long.open.weixin.qq.com`）用纯 HTTP 拿结果。
//! 因此整条链路在容器 / 服务端形态同样可用 —— 用户手机扫码，面板只做两件事：
//! 展示二维码图片、定时轮询状态帧。
//!
//! ## 状态语义以微信授权页内嵌 JS 为准（**曾把 404/405 读反**）
//!
//! | errcode | 含义 | 本模块 |
//! |:--|:--|:--|
//! | 408 | 待扫码（常态） | [`WechatPollStatus::Waiting`] |
//! | 404 | **已扫码待确认**（继续轮询） | [`WechatPollStatus::Scanned`] |
//! | 405 | **已确认**，`wx_code` 就在这一帧 | [`WechatPollStatus::Confirmed`] |
//! | 403 | 用户取消 | [`WechatPollStatus::Cancelled`] |
//! | 402 | 二维码失效 | [`WechatPollStatus::Expired`] |
//!
//! ⚠️ **405 但 `wx_code` 为空时不判成功**（保守判 `Scanned` 继续轮询）——
//! 否则会拿空 code 去换 session，症状是「扫码后必然登录失败」。
//!
//! 蓝本为 `../free-llm-router/src/providers/loomy.ts` 的
//! `buildLoomyWechatAuthUrl` / `extractLoomyWechatUuid` / `pollLoomyWechatOnce`（协议已实测）。

use crate::modules::loomy::region::{realm_spec, LoomyRegion};

/// 二维码 uuid 的字符集与长度（微信未承诺长度，下限取 6 —— 收紧只会让格式微调时静默失效）。
fn is_plausible_uuid(candidate: &str) -> bool {
    (6..=64).contains(&candidate.len())
        && candidate
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'=' | b'+' | b'/'))
}

/// 微信授权页 URL（页面内嵌二维码 uuid，无需执行 JS）。
pub fn auth_url(region: LoomyRegion, state: &str) -> String {
    let spec = realm_spec(region);
    format!(
        "https://open.weixin.qq.com/connect/qrconnect?appid={}\
         &redirect_uri={}&response_type=code&scope=snsapi_login&state={}#wechat_redirect",
        urlencoding::encode(spec.wechat_app_id),
        urlencoding::encode(spec.wechat_redirect_uri),
        urlencoding::encode(state),
    )
}

/// 二维码图片 URL（前端 `<img src>` 直用）。
///
/// ⚠️ **纯字符串拼接**（照蓝本）：uuid 字符集本身含 `= + /`，转义反而会破坏图片地址。
pub fn qr_image_url(uuid: &str) -> String {
    format!("https://open.weixin.qq.com/connect/qrcode/{uuid}")
}

/// 长轮询 URL（带 `last` 游标与时间戳防缓存）。
pub fn poll_url(uuid: &str, last_errcode: &str) -> String {
    let mut query = format!("uuid={}", urlencoding::encode(uuid));
    if !last_errcode.is_empty() {
        query.push_str(&format!("&last={}", urlencoding::encode(last_errcode)));
    }
    query.push_str(&format!("&_={}", crate::modules::config::now_ms()));
    format!("https://long.open.weixin.qq.com/connect/l/qrconnect?{query}")
}

/// 从授权页 HTML 提取二维码 uuid。
///
/// 两条路径互为兜底（蓝本实测）：主路径 `<img class="js_qrcode_img" src="/connect/qrcode/<uuid>">`，
/// 兜底 `.../connect/l/qrconnect?uuid=<uuid>`。
///
/// 提取不到返回 `None` —— **调用方必须显式报错**（返回空串会让轮询永远 waiting，
/// 用户看不到任何原因）。
pub fn extract_uuid(html: &str) -> Option<String> {
    if html.is_empty() {
        return None;
    }
    // uuid 的字符集终止符（字符集外的第一个字符即结束）。
    let terminator = |c: char| {
        !(c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '=' | '+' | '/'))
    };
    if let Some(pos) = html.find("/connect/qrcode/") {
        let rest = &html[pos + "/connect/qrcode/".len()..];
        let end = rest.find(terminator).unwrap_or(rest.len());
        let candidate = &rest[..end];
        if is_plausible_uuid(candidate) {
            return Some(candidate.to_string());
        }
    }
    if let Some(pos) = html.find("l/qrconnect?uuid=") {
        let rest = &html[pos + "l/qrconnect?uuid=".len()..];
        let end = rest.find(terminator).unwrap_or(rest.len());
        let candidate = &rest[..end];
        if is_plausible_uuid(candidate) {
            return Some(candidate.to_string());
        }
    }
    None
}

/// 一次长轮询的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WechatPollStatus {
    /// 待扫码（408 与未知值，保守归此）。
    Waiting,
    /// 已扫码待确认（404，或 405 但没带 code 的异常形态）。
    Scanned,
    /// 已确认：`code` 即微信一次性授权码（**只在这一步用一次**）。
    Confirmed {
        /// 微信 `wx_code`。
        code: String,
    },
    /// 用户取消（403）。
    Cancelled,
    /// 二维码失效（402）。
    Expired,
}

impl WechatPollStatus {
    /// 稳定标识（下发给前端的状态机取值）。
    pub fn as_str(&self) -> &'static str {
        match self {
            WechatPollStatus::Waiting => "waiting",
            WechatPollStatus::Scanned => "scanned",
            WechatPollStatus::Confirmed { .. } => "confirmed",
            WechatPollStatus::Cancelled => "cancelled",
            WechatPollStatus::Expired => "expired",
        }
    }

    /// 终态（轮询循环该停下来了）。
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            WechatPollStatus::Confirmed { .. }
                | WechatPollStatus::Cancelled
                | WechatPollStatus::Expired
        )
    }
}

/// 从长轮询响应体里取 `wx_errcode = <digits>`。
fn pick_errcode(body: &str) -> String {
    let Some(pos) = body.find("wx_errcode") else {
        return String::new();
    };
    let rest = &body[pos + "wx_errcode".len()..];
    rest.chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect()
}

/// 从长轮询响应体里取 `wx_code = '<code>'`。
fn pick_code(body: &str) -> String {
    let Some(pos) = body.find("wx_code") else {
        return String::new();
    };
    let rest = &body[pos + "wx_code".len()..];
    let Some(start) = rest.find('\'') else {
        return String::new();
    };
    let after = &rest[start + 1..];
    let Some(end) = after.find('\'') else {
        return String::new();
    };
    after[..end].to_string()
}

/// 解析一帧长轮询响应。
///
/// ⚠️ 405 但 `wx_code` 为空 ⇒ 保守判 [`WechatPollStatus::Scanned`]（继续轮询），
/// **绝不**拿空 code 去换 session。
pub fn parse_poll(body: &str) -> WechatPollStatus {
    let errcode = pick_errcode(body);
    let code = pick_code(body);
    match errcode.as_str() {
        "405" if !code.is_empty() => WechatPollStatus::Confirmed { code },
        "405" | "404" => WechatPollStatus::Scanned,
        "403" => WechatPollStatus::Cancelled,
        "402" => WechatPollStatus::Expired,
        _ => WechatPollStatus::Waiting,
    }
}

/// 第 1 步（`bind/auth`）的解析结果：`bound` 决定后续走 skip 还是绑定手机号。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindAuth {
    /// `true` = 讯飞侧已绑手机号（可直接 skip 换 session）；`false` = 需绑定手机号。
    pub bound: bool,
    /// 后续三步都要用的会话标识。
    pub rcode: String,
    /// 微信侧昵称（可空，仅展示用）。
    pub nickname: Option<String>,
}

/// 解析第 1 步（`bind/auth`）响应。
///
/// ⚠️ `bind` 缺失时归为 `false`（走绑定流程）：保守方向 ——
/// 若实际已绑，用户最多多填一次手机号；若实际未绑却跳过，会拿到一个没有手机号的账号。
pub fn parse_bind_auth(data: &serde_json::Value) -> Result<BindAuth, String> {
    let rcode = data
        .get("rcode")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    if rcode.is_empty() {
        // 没有 rcode 后续三步全做不了 ⇒ 必须在此明确报错。
        return Err("微信授权响应缺少 rcode".to_string());
    }
    let bound = data.get("bind").and_then(serde_json::Value::as_i64) == Some(1);
    let nickname = data
        .get("nickname")
        .and_then(|v| v.as_str())
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    Ok(BindAuth {
        bound,
        rcode,
        nickname,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_url_carries_appid_redirect_and_state() {
        let url = auth_url(LoomyRegion::Cn, "abc-123");
        assert!(
            url.starts_with("https://open.weixin.qq.com/connect/qrconnect?appid=wx18d60be432287cf8"),
            "{url}"
        );
        // redirect_uri 必须 RFC3986 转义（冒号 / 斜杠不能裸出现）。
        assert!(
            url.contains("redirect_uri=https%3A%2F%2Floomy.xunfei.cn%2Foauth%2Fwechat%2Fcallback"),
            "{url}"
        );
        assert!(url.contains("response_type=code"));
        assert!(url.contains("scope=snsapi_login"));
        assert!(url.ends_with("#wechat_redirect"));
        // state 带空格 / 斜杠的场景也要安全。
        let spaced = auth_url(LoomyRegion::Cn, "a b/c");
        assert!(spaced.contains("state=a%20b%2Fc"), "{spaced}");
    }

    #[test]
    fn qr_image_url_is_a_plain_concatenation() {
        assert_eq!(
            qr_image_url("AbC123xyz_-="),
            "https://open.weixin.qq.com/connect/qrcode/AbC123xyz_-="
        );
    }

    #[test]
    fn poll_url_includes_last_cursor_only_when_present() {
        let bare = poll_url("UUID1", "");
        assert!(
            bare.starts_with("https://long.open.weixin.qq.com/connect/l/qrconnect?uuid=UUID1"),
            "{bare}"
        );
        assert!(!bare.contains("&last="));
        assert!(bare.contains("&_="));

        let cursor = poll_url("UUID1", "408");
        assert!(cursor.contains("&last=408"));
    }

    #[test]
    fn extract_uuid_takes_the_primary_img_path_first() {
        // 主路径：授权页内嵌的 <img src="/connect/qrcode/<uuid>">。
        let clean = r#"<img class="js_qrcode_img" src="/connect/qrcode/0f3aB9x-_Q">"#;
        assert_eq!(extract_uuid(clean), Some("0f3aB9x-_Q".to_string()));
        // 字符集外的字符（如 `"`、`:`）是终止符，不会吞进 uuid。
        let with_quote = r#"<img src="/connect/qrcode/0f3aB9x-_Q:extra">"#;
        assert_eq!(extract_uuid(with_quote), Some("0f3aB9x-_Q".to_string()));
    }

    #[test]
    fn extract_uuid_falls_back_to_the_poll_query() {
        let html = r#"<iframe src="https://long.open.weixin.qq.com/connect/l/qrconnect?uuid=FallbackUuid1&_=1">"#;
        assert_eq!(extract_uuid(html), Some("FallbackUuid1".to_string()));
    }

    #[test]
    fn extract_uuid_returns_none_when_page_shape_changed() {
        assert_eq!(extract_uuid(""), None);
        assert_eq!(extract_uuid("<html>nothing here</html>"), None);
        // 太短的候选（<6）不算 —— 宁可显式报错也不拿垃圾 uuid 去轮询。
        assert_eq!(extract_uuid(r#"src="/connect/qrcode/ab""#), None);
    }

    #[test]
    fn poll_status_maps_errcodes_per_the_official_js() {
        assert_eq!(
            parse_poll("window.wx_errcode=408;window.wx_code='';"),
            WechatPollStatus::Waiting
        );
        // ★ 404 = 已扫码待确认（**不是**已确认 —— 蓝本曾把两者读反）。
        assert_eq!(
            parse_poll("window.wx_errcode=404;window.wx_code='';"),
            WechatPollStatus::Scanned
        );
        // ★ 405 = 已确认，wx_code 就在这一帧。
        assert_eq!(
            parse_poll("window.wx_errcode=405;window.wx_code='CODE123';"),
            WechatPollStatus::Confirmed {
                code: "CODE123".to_string()
            }
        );
        // ⚠️ 405 却没带 code ⇒ 保守判 scanned（绝不拿空 code 换 session）。
        assert_eq!(
            parse_poll("window.wx_errcode=405;window.wx_code='';"),
            WechatPollStatus::Scanned
        );
        assert_eq!(
            parse_poll("window.wx_errcode=403;window.wx_code='';"),
            WechatPollStatus::Cancelled
        );
        assert_eq!(
            parse_poll("window.wx_errcode=402;window.wx_code='';"),
            WechatPollStatus::Expired
        );
        // 未知 / 空 errcode 一律归 waiting（保守：绝不误判成功）。
        assert_eq!(parse_poll("garbage"), WechatPollStatus::Waiting);
        assert_eq!(
            parse_poll("window.wx_errcode=999;"),
            WechatPollStatus::Waiting
        );
    }

    #[test]
    fn only_confirmed_cancelled_expired_are_terminal() {
        assert!(!WechatPollStatus::Waiting.is_terminal());
        assert!(!WechatPollStatus::Scanned.is_terminal());
        assert!(
            WechatPollStatus::Confirmed {
                code: "c".to_string()
            }
            .is_terminal()
        );
        assert!(WechatPollStatus::Cancelled.is_terminal());
        assert!(WechatPollStatus::Expired.is_terminal());
        assert_eq!(WechatPollStatus::Waiting.as_str(), "waiting");
        assert_eq!(
            WechatPollStatus::Confirmed {
                code: "c".to_string()
            }
            .as_str(),
            "confirmed"
        );
    }

    #[test]
    fn bind_auth_requires_rcode_and_defaults_bind_to_unbound() {
        let ok = parse_bind_auth(&serde_json::json!({
            "bind": 1,
            "rcode": "RC1",
            "nickname": "小明"
        }))
        .expect("应解析成功");
        assert!(ok.bound);
        assert_eq!(ok.rcode, "RC1");
        assert_eq!(ok.nickname.as_deref(), Some("小明"));

        // bind 缺失 ⇒ 保守走绑定流程（反方向会让用户拿到无手机号账号）。
        let missing_bind =
            parse_bind_auth(&serde_json::json!({ "rcode": "RC2" })).expect("应解析成功");
        assert!(!missing_bind.bound);
        assert_eq!(missing_bind.nickname, None);

        // 没有 rcode ⇒ 显式报错。
        let no_rcode = parse_bind_auth(&serde_json::json!({ "bind": 1 }));
        assert!(no_rcode.is_err());
    }
}
