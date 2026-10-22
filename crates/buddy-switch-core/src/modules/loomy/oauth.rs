//! Loomy 的短信验证码登录（讯飞账号 CAccount 三步：发码 → 校验 → 落库）。
//!
//! ## 为什么可以在面板里完成登录
//!
//! 短信路径是**纯 HTTP 三步**（发码 → 用户输入 → 校验），不需要任何本地监听或浏览器。
//! 签名见 [`super::signing`]；端点常量见 [`super::region::realm_spec`]。
//! 微信扫码（纯 HTTP 长轮询）列为**二期** —— 刻意不实现「半套」：二维码渲染属于
//! 前端职责，协议本体可后补而无需改动本模块的公共接口。
//!
//! 蓝本为 `../free-llm-router/src/providers/loomy.ts` 的 `sendLoomySmsCode` /
//! `loginLoomyBySmsCode` / `buildLoomyAccountBody`（协议已实测，非本仓文件）。

use serde_json::{json, Value};
use uuid::Uuid;

use crate::modules::loomy::account::{RawAccount, SOURCE_SMS_LOGIN};
use crate::modules::loomy::credential::{parse_credential, session_expires_at};
use crate::modules::loomy::region::{realm_spec, LoomyRegion};

/// 构造讯飞账号端点的请求体信封 `{base, param}`。
///
/// ⚠️ `ua` **硬编码 macOS**：官方客户端在 Windows 上发的也是这个值，照抄不要改。
pub fn build_account_body(region: LoomyRegion, param: Value) -> Value {
    let spec = realm_spec(region);
    json!({
        "base": {
            "appid": spec.app_id,
            "modelid": "Web",
            "version": "1.0.0",
            "devid": "web",
            "ua": spec.client_ua,
            // traceid 每次调用重新生成（去连字符的 uuid = 32 位 hex，与客户端一致）。
            "traceid": Uuid::new_v4().simple().to_string(),
        },
        "param": param,
    })
}

/// 解析账号端点响应里的登录字段（session / userid / phone）。
///
/// ⚠️ **失败必须显式**：session 或 userid 缺失 ⇒ 可读 `Err`，
/// 不产出永远 401 的凭据。
pub fn parse_login_response(data: &Value) -> Result<(String, String, String), String> {
    let session = data
        .get("session")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let userid = data
        .get("userid")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    if session.is_empty() {
        return Err("登录响应缺少 session".to_string());
    }
    if userid.is_empty() {
        return Err("登录响应缺少 userid".to_string());
    }
    let phone = data
        .get("phone")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    Ok((session, userid, phone))
}

/// 把登录成功后的凭据组装成可入库的账号（uid 口径见 `credential.rs`）。
pub fn account_from_login(
    region: LoomyRegion,
    session: String,
    userid: String,
    phone: String,
) -> Result<(RawAccount, String), String> {
    let expires_at = session_expires_at(region);
    let parsed = parse_credential(&json!({
        "access_token": session,
        "userid": userid,
        "phone": phone,
        "expires_at": expires_at,
    }))?;
    let account = RawAccount {
        uid: parsed.uid.clone(),
        access_token: parsed.session,
        userid: parsed.userid,
        phone: parsed.phone,
        nickname: if parsed.nickname.is_empty() {
            None
        } else {
            Some(parsed.nickname)
        },
        expires_at: parsed.expires_at,
        source: Some(SOURCE_SMS_LOGIN.to_string()),
        remark: None,
        group: None,
        credits: 0,
        daily_credits: 0,
        last_checkin: None,
        added_at: None,
        updated_at: None,
    };
    Ok((account, parsed.uid))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn account_body_has_the_envelope_shape() {
        let body = build_account_body(LoomyRegion::Cn, json!({"phone": "13800000000"}));
        // 信封 `{base, param}`：appid / modelid / version / devid / ua / traceid。
        assert_eq!(body["base"]["appid"], "GM3LOOMY");
        assert_eq!(body["base"]["modelid"], "Web");
        assert_eq!(body["base"]["ua"], "Loomy|Desktop|Electron|macOS");
        // traceid = 去连字符的 uuid（32 位 hex）。
        let traceid = body["base"]["traceid"].as_str().unwrap_or_default();
        assert_eq!(traceid.len(), 32);
        assert!(!traceid.contains('-'));
        assert_eq!(body["param"]["phone"], "13800000000");
    }

    #[test]
    fn login_response_missing_fields_fail_loudly() {
        let missing_session = json!({"userid": "123456789012345678"});
        assert!(parse_login_response(&missing_session).is_err());
        let missing_userid = json!({"session": &"a".repeat(32)});
        assert!(parse_login_response(&missing_userid).is_err());
        let ok = json!({"session": &"a".repeat(32), "userid": "123456789012345678", "phone": "13812345678"});
        let (session, userid, phone) = parse_login_response(&ok).expect("应解析成功");
        assert_eq!(session.len(), 32);
        assert_eq!(userid, "123456789012345678");
        assert_eq!(phone, "13812345678");
    }

    #[test]
    fn login_account_gets_sms_source_and_14d_expiry() {
        let (account, _uid) = account_from_login(
            LoomyRegion::Cn,
            "a".repeat(32),
            "123456789012345678".to_string(),
            "13812345678".to_string(),
        )
        .expect("应组装成功");
        assert_eq!(account.source.as_deref(), Some(SOURCE_SMS_LOGIN));
        assert_eq!(account.phone, "13812345678");
        // 14 天 TTL：expires_at 在未来。
        assert!(account.expires_at > crate::modules::config::now_ms());
    }
}
