//! Loomy（讯飞办公助手）的**区域（realm）**与全部上游常量。
//!
//! ## 为什么只有一个 variant
//!
//! Loomy 只有国内一套账号体系（`loomyad.xunfei.cn`），没有国际版。
//! 保留枚举（而非裸常量）是为了与 [`QoderRegion`](crate::modules::qoder::region::QoderRegion)
//! 对称，让「区域 → 作用域」「区域 → 命名空间」等派生保持**穷举 match** 的编译期纪律 ——
//! 将来若讯飞真出了国际版，加 variant 即可，所有落点由编译器点名。
//!
//! ## 表驱动
//!
//! 全部上游差异集中在 [`RealmSpec`] 一张表里，**调用方一律经 [`realm_spec`] 查表**，
//! 不得在别处手写端点。协议事实（含各坑位的实测依据）见
//! `docs/specs/LOOMY-PLATFORM.md`；适配器蓝本为
//! `../free-llm-router/src/providers/loomy.ts`（协议已实测摸透，非本仓文件）。

/// Loomy 的区域（realm）—— **持久化轴**（当前仅国内）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoomyRegion {
    /// 国内版（唯一区域）。
    Cn,
}

impl LoomyRegion {
    /// 稳定标识（文件名后缀、序列化、前端传参）。
    pub fn as_str(self) -> &'static str {
        match self {
            LoomyRegion::Cn => "cn",
        }
    }

    /// 展示名 —— 取**客户端自己的名字**。
    pub fn display_name(self) -> &'static str {
        "Loomy"
    }

    /// 从字符串解析区域，**宽容**（供诊断与旧值兼容；`scope` 路径请用
    /// [`Scope::parse`](crate::modules::product::Scope::parse)，那条是 fail-closed）。
    pub fn parse(s: &str) -> Option<LoomyRegion> {
        match s.trim().to_ascii_lowercase().as_str() {
            "cn" | "china" | "domestic" | "loomy" => Some(LoomyRegion::Cn),
            _ => None,
        }
    }

    /// 全部区域（遍历用，顺序稳定）。
    pub fn all() -> [LoomyRegion; 1] {
        [LoomyRegion::Cn]
    }
}

impl Default for LoomyRegion {
    fn default() -> Self {
        LoomyRegion::Cn
    }
}

/// 一个区域的**全部上游差异**。
///
/// ⚠️ 协议事实的来源与依据见 `docs/specs/LOOMY-PLATFORM.md`。
pub struct RealmSpec {
    /// 该表属于哪个区域。
    pub region: LoomyRegion,
    /// 业务基址（推理 / 模型列表 / 积分）。
    pub api_base: &'static str,
    /// 讯飞账号（CAccount）基址 —— **仅登录用**，业务路径绝不碰它。
    pub account_base: &'static str,
    /// 业务成功码。
    pub ok_code: &'static str,
    /// 登录失效码（收到不得重试 —— 同一死凭据重试无意义）。
    pub auth_error_code: &'static str,
    /// 参数错误码（如未知字段形状）。
    pub bad_request_code: &'static str,
    /// 登录会话有效期（秒）= 14 天。这是向服务端**声明**的有效期；响应不带到期时间戳，
    /// `expires_at` 由本地按此推算。
    pub session_ttl_seconds: i64,
    /// 短信验证码有效期（秒）。
    pub sms_code_ttl_seconds: i64,
    /// 讯飞账号 AccessKey（仅用于账号端点 HMAC-SHA1 签名，与业务凭据无关）。
    pub access_key_id: &'static str,
    /// 讯飞账号 AccessKeySecret（HMAC-SHA1 的密钥）。
    pub access_key_secret: &'static str,
    /// 讯飞 appId（账号请求体 `base.appid`）。
    pub app_id: &'static str,
    /// 客户端 UA（账号请求体 `base.ua`；官方客户端在 Windows 上发的也是这个值，照抄）。
    pub client_ua: &'static str,
}

/// 国内版。
pub const CN_REALM: RealmSpec = RealmSpec {
    region: LoomyRegion::Cn,
    api_base: "https://loomyad.xunfei.cn/api/v1",
    account_base: "https://account.xfinfr.com",
    ok_code: "000000",
    auth_error_code: "100002",
    bad_request_code: "100001",
    session_ttl_seconds: 1_209_600,
    sms_code_ttl_seconds: 300,
    // ⚠️ 与 free-llm-router 同值（取自讯飞客户端 `.env.prod`）。它只用于讯飞账号端点的
    // HMAC-SHA1 签名，与业务/推理端点（用的是用户登录后的 session）无关。
    access_key_id: "2thryby66wxi53sk",
    access_key_secret: "zsak6eadrbawz683wf5r3m2snrwj868r",
    app_id: "GM3LOOMY",
    client_ua: "Loomy|Desktop|Electron|macOS",
};

/// 区域 → 上游差异表（**唯一查表入口**）。
pub fn realm_spec(region: LoomyRegion) -> &'static RealmSpec {
    match region {
        LoomyRegion::Cn => &CN_REALM,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 单区域平台的枚举纪律：`all()` 与查表往返一致。
    #[test]
    fn region_round_trips_through_realm_spec() {
        for region in LoomyRegion::all() {
            let spec = realm_spec(region);
            assert_eq!(spec.region, region);
            assert_eq!(LoomyRegion::parse(region.as_str()), Some(region));
        }
        // 宽容解析的负面用例。
        assert_eq!(LoomyRegion::parse("global"), None);
        assert_eq!(LoomyRegion::parse(""), None);
    }

    /// 上游常量与蓝本逐字对齐（free-llm-router/src/providers/loomy.ts）。
    #[test]
    fn realm_constants_match_the_reference_adapter() {
        let spec = realm_spec(LoomyRegion::Cn);
        assert_eq!(spec.api_base, "https://loomyad.xunfei.cn/api/v1");
        assert_eq!(spec.account_base, "https://account.xfinfr.com");
        assert_eq!(spec.ok_code, "000000");
        assert_eq!(spec.auth_error_code, "100002");
        assert_eq!(spec.bad_request_code, "100001");
        assert_eq!(spec.session_ttl_seconds, 1_209_600);
        assert_eq!(spec.sms_code_ttl_seconds, 300);
        assert_eq!(spec.app_id, "GM3LOOMY");
    }
}
