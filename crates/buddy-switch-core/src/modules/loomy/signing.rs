//! 讯飞账号（CAccount）端点的 **HMAC-SHA1 签名**（仅登录链路使用）。
//!
//! ## 为什么是这套签名
//!
//! `account.xfinfr.com` 的每个请求都要带 `Authorization: account {ak}:{sig}` ——
//! 签名 = HMAC-SHA1(9 段签名字符串, access_key_secret)。协议事实（9 段的逐段依据、
//! 尾随双换行为什么不能丢）见 `docs/specs/LOOMY-PLATFORM.md`；适配器蓝本为
//! `../free-llm-router/src/providers/loomy.ts` 的 `buildLoomySigningString` /
//! `loomyAuthHeaders`（协议已实测，非本仓文件）。
//!
//! ## 三个不能「顺手优化」掉的细节
//!
//! 1. **空 body 的 `Content-MD5` 是空串**，不是空串的 md5 —— 与官方客户端一致；
//! 2. **9 段签名字符串以两个换行结尾**（后两段恒为空串；`join('\n')` 的自然结果）——
//!    去掉尾随换行签名就不匹配；
//! 3. **body 必须序列化一次、签名与发送共用**（二次序列化会改字节序，签名随即失效）
//!    —— 这是**调用方**的纪律，见 [`sign_caccount_request`] 的文档。
//!
//! ## 依赖
//!
//! HMAC-SHA1 / MD5 用本仓已有的 `sha1` / `md-5` + `hmac` crate（Qoder COSY 在用），
//! 不新增依赖。

use base64::Engine as _;
use hmac::{Hmac, Mac};
use md5::{Digest, Md5};
use sha1::Sha1;

use crate::modules::loomy::region::{realm_spec, LoomyRegion};

type HmacSha1 = Hmac<Sha1>;

/// RFC3986 转义（`encodeURIComponent` 语义 + 补转 `! ' ( ) *`）。
fn escape_rfc3986(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char);
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{byte:02X}"));
            }
        }
    }
    // JS `encodeURIComponent` 不转义 `! ' ( ) *`，RFC3986 要求转。
    out.replace('!', "%21")
        .replace('\'', "%27")
        .replace('(', "%28")
        .replace(')', "%29")
        .replace('*', "%2A")
}

/// 构建 ESCAPED_PATH：补前导 `/`、剥末尾 `/`（长度 > 1 时）、逐段转义再拼回。
fn build_escaped_path(raw_path: &str) -> String {
    let mut clean = if raw_path.starts_with('/') {
        raw_path.to_string()
    } else {
        format!("/{raw_path}")
    };
    if clean.len() > 1 && clean.ends_with('/') {
        clean.pop();
    }
    clean
        .split('/')
        .map(|segment| {
            if segment.is_empty() {
                String::new()
            } else {
                escape_rfc3986(segment)
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// 构建 ESCAPED_QUERY_STRING：`key=value` 用 `&` 连接，**不排序**（保持传入顺序）。
fn build_escaped_query_string(query_params: &[(String, String)]) -> String {
    query_params
        .iter()
        .map(|(key, value)| {
            format!(
                "{}={}",
                escape_rfc3986(key),
                escape_rfc3986(value)
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// 计算 `Content-MD5`（base64）。
///
/// ⚠️ 空 body 返回**空串**而**不是**空串的 md5 —— 与官方客户端一致。
pub fn content_md5(body: &str) -> String {
    if body.is_empty() {
        return String::new();
    }
    let mut hasher = Md5::new();
    hasher.update(body.as_bytes());
    let digest = hasher.finalize();
    base64::engine::general_purpose::STANDARD.encode(digest)
}

/// 一次签名的全部输入。
pub struct SignInput<'a> {
    /// HTTP 方法（`POST` 等；签名用大写）。
    pub method: &'a str,
    /// 路径（如 `/login/phone/sendMsgCode`）。
    pub path: &'a str,
    /// 查询参数（保持传入顺序，不排序）。
    pub query_params: &'a [(String, String)],
    /// **已序列化**的请求体字符串（与发送时用的必须是同一个）。
    pub body: &'a str,
    /// `Content-Type`（缺省 `application/json`）。
    pub content_type: &'a str,
    /// `Date` 头的值（UTC 字符串）。
    pub date: &'a str,
    /// `Nonce` 头的值（uuid）。
    pub nonce: &'a str,
}

/// 构建待签名字符串（9 段，`\n` 连接，**以两个换行结尾**）。
///
/// ```text
/// {METHOD}\n{ESCAPED_PATH}\n{ESCAPED_QUERY}\n{Content-MD5}\n
/// {Content-Type}\n{Date}\n{Nonce}\n{SignedHeaders}\n{CanonicalizedHeaders}
/// ```
///
/// 后两段（SignedHeaders / CanonicalizedHeaders）恒为空串 —— 我们不发任何 `x-*` 头。
pub fn build_signing_string(input: &SignInput<'_>) -> String {
    let content_md5 = content_md5(input.body);
    [
        input.method.to_ascii_uppercase(),
        build_escaped_path(input.path),
        build_escaped_query_string(input.query_params),
        content_md5,
        input.content_type.to_string(),
        input.date.to_string(),
        input.nonce.to_string(),
        String::new(),
        String::new(),
    ]
    .join("\n")
}

/// 签名结果（要放进请求的头）。
pub struct AuthHeaders {
    /// `Authorization: account {ak}:{sig}`。
    pub authorization: String,
    /// `Date`（UTC 字符串，与签名字符串里的同一份）。
    pub date: String,
    /// `Nonce`（uuid）。
    pub nonce: String,
    /// `Content-MD5`（body 非空时才有）。
    pub content_md5: String,
}

/// 对一次讯飞账号请求做 HMAC-SHA1 签名，产出全部认证头。
///
/// ⚠️ **调用方纪律**：`input.body` 必须与实际发送的请求体是**同一个字符串**
/// （先 `serde_json::to_string` 一次，签名与发送共用）—— 二次序列化会改变字节
/// （键序 / 空格），签名随即失效。
pub fn sign_caccount_request(
    region: LoomyRegion,
    input: &SignInput<'_>,
) -> AuthHeaders {
    let spec = realm_spec(region);
    let string_to_sign = build_signing_string(input);
    let mut mac = HmacSha1::new_from_slice(spec.access_key_secret.as_bytes())
        .expect("HMAC-SHA1 接受任意长度密钥");
    mac.update(string_to_sign.as_bytes());
    let signature = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());
    let md5 = content_md5(input.body);
    AuthHeaders {
        authorization: format!("account {}:{}", spec.access_key_id, signature),
        date: input.date.to_string(),
        nonce: input.nonce.to_string(),
        content_md5: md5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC3986 转义：保留 unreserved，补转 JS `encodeURIComponent` 漏掉的五个字符。
    #[test]
    fn escape_rfc3986_matches_the_reference_implementation() {
        assert_eq!(escape_rfc3986("login/phone"), "login%2Fphone");
        assert_eq!(escape_rfc3986("a b"), "a%20b");
        assert_eq!(escape_rfc3986("GM3LOOMY"), "GM3LOOMY");
        // JS encodeURIComponent 保留这五个，RFC3986 要求转 —— 补丁的验证点。
        assert_eq!(escape_rfc3986("a!b'c(d)e*f"), "a%21b%27c%28d%29e%2Af");
        // 非 ASCII 走 UTF-8 字节。
        assert_eq!(escape_rfc3986("中"), "%E4%B8%AD");
    }

    /// ESCAPED_PATH：补前导斜杠、剥末尾斜杠、逐段转义。
    #[test]
    fn escaped_path_normalizes_leading_and_trailing_slashes() {
        assert_eq!(build_escaped_path("/login/phone"), "/login/phone");
        assert_eq!(build_escaped_path("login/phone"), "/login/phone");
        assert_eq!(build_escaped_path("/login/"), "/login");
        assert_eq!(build_escaped_path("/"), "/");
    }

    /// 空 body 的 Content-MD5 是**空串**，不是空串的 md5。
    #[test]
    fn content_md5_of_empty_body_is_empty_string() {
        assert_eq!(content_md5(""), "");
        // RFC 1321 标准向量（md5("abc") = 90015098...）锁实现，
        // 期望值是「abc 的 md5 的 base64」，与 JS 参考实现 `md5Base64` 同口径。
        assert_eq!(
            content_md5("abc"),
            base64::engine::general_purpose::STANDARD.encode({
                let mut hasher = Md5::new();
                hasher.update(b"abc");
                hasher.finalize()
            })
        );
    }

    /// 9 段签名字符串：尾随双换行**不能丢**。
    #[test]
    fn signing_string_keeps_the_trailing_double_newline() {
        let input = SignInput {
            method: "POST",
            path: "/login/phone/sendMsgCode",
            query_params: &[],
            body: "{}",
            content_type: "application/json",
            date: "Mon, 21 Oct 2026 00:00:00 GMT",
            nonce: "0123456789abcdef",
        };
        let string_to_sign = build_signing_string(&input);
        // 9 段 join('\n') ⇒ 8 个换行；后两段为空 ⇒ 字符串以 "\n\n" 结尾。
        assert!(string_to_sign.ends_with("\n\n"), "{string_to_sign:?}");
        assert_eq!(string_to_sign.matches('\n').count(), 8);
        assert!(string_to_sign.starts_with("POST\n/login/phone/sendMsgCode\n"));
    }

    /// 端到端签名形状：`account {ak}:{base64}`。
    #[test]
    fn auth_headers_have_the_account_scheme() {
        let headers = sign_caccount_request(
            LoomyRegion::Cn,
            &SignInput {
                method: "POST",
                path: "/login/phone/checkCode",
                query_params: &[],
                body: "{}",
                content_type: "application/json",
                date: "Mon, 21 Oct 2026 00:00:00 GMT",
                nonce: "nonce",
            },
        );
        assert!(headers.authorization.starts_with("account 2thryby66wxi53sk:"));
        assert_eq!(headers.date, "Mon, 21 Oct 2026 00:00:00 GMT");
        assert_eq!(headers.nonce, "nonce");
        // "{}" 的 Content-MD5 非空。
        assert!(!headers.content_md5.is_empty());
    }
}
