//! Loomy 的凭据解析与**判别特征**。
//!
//! ## 为什么需要判别特征（不能只看「有没有 access_token」）
//!
//! 多家凭据形状高度重叠（JWT 三段式是好几家共用），靠令牌形状无法区分是哪一家。
//! Loomy 的判别特征是**独有**的：
//!
//! 1. token 是 **32 位小写 hex**（讯飞 session 的固定形态）；
//! 2. 或有 **15–20 位纯数字 `userid`**；
//! 3. 或有 **11 位大陆手机号**。
//!
//! 三者满足其一才算「像是 Loomy 的」；只有 access_token 而无任何特征 ⇒ 拒绝，
//! **不**把它吞成 Loomy 账号（宁可要求用户显式指定，也不要把凭据存错家）。
//!
//! ## uid 口径（**必须无歧义**）
//!
//! `uid` 是账号库的唯一键，**必须对同一账号恒为同一个值**：
//!
//! - 有 15–20 位 `userid` ⇒ 直接用（讯飞侧稳定）；
//! - 否则 ⇒ 从 token（与手机号，若提供）确定性派生 `loomy-<md5(phone \0 token) 前 16 hex>`；
//!   派生值单向且稳定，**绝不**把凭据本体写进存储 key。
//! - **绝不用手机号当 uid**：同一手机号可以对应多个讯飞账号，会把两个账号合并成一条记录。
//!
//! 判别/解析的蓝本为 `../free-llm-router/src/providers/loomy.ts` 的
//! `looksLikeLoomyCredential` / `parseCredential`（协议已实测，非本仓文件）。

use serde::{Deserialize, Serialize};

use crate::modules::loomy::region::{realm_spec, LoomyRegion};

/// 32 位小写 hex —— 讯飞 session 的固定形态。
fn is_loomy_session(token: &str) -> bool {
    let bytes = token.as_bytes();
    bytes.len() == 32
        && bytes
            .iter()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// 15–20 位纯数字 —— 讯飞 `userid` 的形态。
fn is_loomy_userid(value: &str) -> bool {
    let bytes = value.as_bytes();
    (15..=20).contains(&bytes.len()) && bytes.iter().all(|b| b.is_ascii_digit())
}

/// 11 位大陆手机号。
fn is_phone_number(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 11 && bytes[0] == b'1' && bytes[1..].iter().all(|b| b.is_ascii_digit())
}

/// 从任意形状里读字符串（camelCase / snake_case 都认，取第一个非空）。
fn pick_string<'a>(source: &'a serde_json::Value, keys: &[&str]) -> Option<String> {
    let obj = source.as_object()?;
    for key in keys {
        if let Some(value) = obj.get(*key).and_then(|v| v.as_str()) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// 解析前先展开一层嵌套包装（`credential` / `credentials` / `auth`）。
///
/// ⚠️ 内层**优先**，外层只作兜底（`{...outer, ...inner}` 顺序不能反）——
/// 与 free-llm-router 同款语义。
fn unwrap_wrappers(value: serde_json::Value) -> serde_json::Value {
    if !value.is_object() {
        return value;
    }
    for wrapper in ["credential", "credentials", "auth"] {
        let inner = value.get(wrapper).filter(|v| v.is_object()).cloned();
        if let Some(inner_obj) = inner.and_then(|v| v.as_object().cloned()) {
            if let Some(mut outer) = value.as_object().cloned() {
                for (k, v) in inner_obj {
                    outer.insert(k, v);
                }
                return serde_json::Value::Object(outer);
            }
        }
    }
    value
}

/// 解析结果：一份可直接入库的凭据四元组。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedCredential {
    /// 账号唯一 id（userid 或派生值）。
    pub uid: String,
    /// 讯飞 session（业务凭据）。
    pub session: String,
    /// 讯飞 userid（有则原样；可空）。
    pub userid: String,
    /// 手机号（有则原样；可空）。
    pub phone: String,
    /// 昵称（可空）。
    pub nickname: String,
    /// 过期时刻（毫秒时间戳；0 = 未知）。
    pub expires_at: i64,
}

/// 判定一份**原始凭据对象**是否具备 Loomy 的判别特征。
///
/// ⚠️ **必须与 [`parse_credential`] 共用同一判据**（本函数即 `parse_credential` 内部
/// 复用的同一实现），否则会出现「识别说是我、导入却报错」的自相矛盾。
pub fn looks_like_loomy_credential(input: &serde_json::Value) -> bool {
    let source = unwrap_wrappers(input.clone());
    let Some(token) =
        pick_string(&source, &["access_token", "accessToken", "session", "token"])
    else {
        return false;
    };
    let userid = pick_string(&source, &["userid", "user_id", "userId"]).unwrap_or_default();
    let phone = pick_string(&source, &["phone", "mobile", "phone_number"]).unwrap_or_default();
    is_loomy_session(&token) || is_loomy_userid(&userid) || is_phone_number(&phone)
}

/// 解析凭据（宽容形状 + 严格判别）。
///
/// 接受扁平 / 嵌套 `{auth:{…}}` / 数组首项；camelCase 与 snake_case 都认。
/// 缺 token 或无判别特征 ⇒ 可读 `Err`（fail-closed，不产出永远 401 的凭据）。
pub fn parse_credential(input: &serde_json::Value) -> Result<ParsedCredential, String> {
    let fail = |message: &str| -> Result<ParsedCredential, String> {
        Err(message.to_string())
    };

    if !input.is_object() && !input.is_array() {
        return fail("Loomy 凭据必须是一个 JSON 对象");
    }
    // 数组取首项。
    let root = if let Some(first) = input.as_array().and_then(|a| a.first()) {
        first.clone()
    } else {
        input.clone()
    };
    if !root.is_object() {
        return fail("Loomy 凭据必须是一个 JSON 对象（不能是空数组）");
    }
    let source = unwrap_wrappers(root);

    let Some(token) = pick_string(&source, &["access_token", "accessToken", "session", "token"])
    else {
        return fail("缺少访问令牌：请提供 Loomy 的 `access_token`（讯飞 session）字段");
    };
    let userid = pick_string(&source, &["userid", "user_id", "userId"]).unwrap_or_default();
    let phone = pick_string(&source, &["phone", "mobile", "phone_number"]).unwrap_or_default();
    // ⚠️ 判别用**展开后**的 source（数组 / 嵌套形先归一化），与 pick 口径一致。
    if !looks_like_loomy_credential(&source) {
        return fail(
            "这不像是 Loomy 凭据：`access_token` 不是 32 位十六进制的讯飞 session，\
             且没有 15–20 位 `userid` 或 11 位手机号可用于识别账号。\
             请在面板粘贴 Loomy 登录后的凭据。",
        );
    }
    let nickname = pick_string(&source, &["nickname", "name", "user_name"]).unwrap_or_default();
    let expires_at = source
        .get("expires_at")
        .or_else(|| source.get("expiresAt"))
        .and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
        .filter(|v| *v > 0)
        .unwrap_or(0);

    // uid 口径：userid 优先，否则从 token 派生（md5(phone\0token) 前 16 hex）。
    let uid = if is_loomy_userid(&userid) {
        userid.clone()
    } else {
        derive_loomy_uid(&phone, &token)
    };

    Ok(ParsedCredential {
        uid,
        session: token,
        userid,
        phone,
        nickname,
        expires_at,
    })
}

/// 从 token（与手机号，若提供）确定性派生一个**不含凭据本体**的 uid。
///
/// MD5 在这里只做「确定性短标识」，不是安全边界 —— uid 不是秘密，只需要它
/// **单向**（不能从 uid 反推 token）且**稳定**。取 16 个 hex 字符（64 bit）。
pub fn derive_loomy_uid(phone: &str, token: &str) -> String {
    use md5::{Digest, Md5};
    let mut hasher = Md5::new();
    hasher.update(phone.as_bytes());
    hasher.update(b"\0");
    hasher.update(token.as_bytes());
    let digest = hasher.finalize();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("loomy-{}", &hex[..16])
}

/// 按 14 天 TTL 本地推算 `expires_at`（毫秒）。
///
/// ⚠️ 服务端**只接受**登录请求里的 `expire` 参数，响应里不带到期时间 ——
/// 由登录路径在成功后调用本函数落盘。
pub fn session_expires_at(region: LoomyRegion) -> i64 {
    let ttl = realm_spec(region).session_ttl_seconds;
    crate::modules::config::now_ms() + ttl * 1000
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn shape_hints_discriminate_loomy_forms() {
        assert!(is_loomy_session(&"a".repeat(32)));
        assert!(!is_loomy_session(&"A".repeat(32)), "大写不算（session 是小写 hex）");
        assert!(!is_loomy_session(&"a".repeat(31)));
        assert!(is_loomy_userid("123456789012345"));
        assert!(is_loomy_userid("12345678901234567890"));
        assert!(!is_loomy_userid("12345678901234"), "14 位不算");
        assert!(!is_loomy_userid("123456789012345678901"));
        assert!(is_phone_number("13812345678"));
        assert!(!is_phone_number("23812345678"));
        assert!(!is_phone_number("1381234567"));
    }

    #[test]
    fn accepts_flat_and_nested_and_array_forms() {
        let flat = json!({"access_token": "a".repeat(32), "userid": "123456789012345678"});
        let nested = json!({"auth": {"access_token": "a".repeat(32), "userid": "123456789012345678"}});
        let array = json!([{"access_token": "a".repeat(32), "userid": "123456789012345678"}]);
        for input in [flat, nested, array] {
            let parsed = parse_credential(&input).expect("应解析成功");
            assert_eq!(parsed.uid, "123456789012345678");
            assert_eq!(parsed.session, "a".repeat(32));
        }
        // snake_case 同样认。
        let snake = json!({"access_token": "b".repeat(32), "user_id": "223456789012345678"});
        assert!(parse_credential(&snake).is_ok());
    }

    #[test]
    fn rejects_foreign_credentials_without_discriminating_features() {
        // JWT 三段式 + 只有 token 字段：别家的典型形状，不能吞。
        let jwt = json!({"access_token": format!("eyJhbGciOi{}.{}.x", "a".repeat(16), "b".repeat(16))});
        assert!(parse_credential(&jwt).is_err());
        assert!(!looks_like_loomy_credential(&jwt));
        // 数字串 token 但长度不对。
        let digits = json!({"access_token": "1".repeat(40)});
        assert!(parse_credential(&digits).is_err());
    }

    #[test]
    fn uid_falls_back_to_derived_value_and_never_the_phone() {
        // 无 userid ⇒ 派生；派生值稳定且不含凭据本体。
        let input = json!({"access_token": "c".repeat(32), "phone": "13812345678"});
        let parsed = parse_credential(&input).expect("应解析成功");
        assert!(parsed.uid.starts_with("loomy-"));
        assert_eq!(parsed.uid.len(), "loomy-".len() + 16);
        assert!(!parsed.uid.contains("13812345678"), "uid 不得含手机号");
        assert!(!parsed.uid.contains(&"c".repeat(32)), "uid 不得含 token");
        // 同输入 ⇒ 同 uid（稳定）。
        let again = parse_credential(&input).expect("应解析成功");
        assert_eq!(parsed.uid, again.uid);
        // 手机号相同、token 不同 ⇒ uid 不同（避免两个讯飞账号被合并）。
        let other = json!({"access_token": "d".repeat(32), "phone": "13812345678"});
        let other_parsed = parse_credential(&other).expect("应解析成功");
        assert_ne!(parsed.uid, other_parsed.uid);
    }

    #[test]
    fn wrapper_unwrap_prefers_the_inner_layer() {
        // 内层优先：同键时取内层的值。
        let wrapped = json!({
            "access_token": "e".repeat(32),
            "auth": {"access_token": "f".repeat(32)}
        });
        let parsed = parse_credential(&wrapped).expect("应解析成功");
        assert_eq!(parsed.session, "f".repeat(32));
    }
}
