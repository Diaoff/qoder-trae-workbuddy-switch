//! Loomy 的每日签到（`POST /points/first-login`）—— **触发每日额度重置**。
//!
//! ## 语义（容易写错的一点）
//!
//! 「签到」不是「+5000 积分」，而是**触发每日赠送池的重置**：
//! `dailyBalance = thinQuota - dailyConsumed`，消耗后不回补、次日重置。
//! 幂等判据是响应体 `alreadyProcessed`（**不是** HTTP 状态码 —— 重复调用同样返回 200）。
//!
//! 蓝本为 `../free-llm-router/src/providers/loomy.ts` 的 `checkin`（协议已实测）。
//! 网络发送在宿主 / 网关侧组头发送；本模块提供**纯解析逻辑**（`parse_checkin_data`）
//! 与端点常量，方便单测与复用。

/// 签到请求体（`{}`，与蓝本一致）。
pub const CHECKIN_BODY: &str = "{}";

/// 签到端点路径。
pub const CHECKIN_PATH: &str = "/points/first-login";

pub use crate::modules::loomy::credits::{parse_checkin_data, CheckinOutcome};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn endpoint_constants_are_stable() {
        assert_eq!(CHECKIN_PATH, "/points/first-login");
        assert_eq!(CHECKIN_BODY, "{}");
        // 幂等语义的烟囱测试：alreadyProcessed 驱动结果（解析逻辑单测在 credits.rs）。
        let outcome = parse_checkin_data(&json!({"alreadyProcessed": true}));
        assert!(outcome.already_done);
    }
}
