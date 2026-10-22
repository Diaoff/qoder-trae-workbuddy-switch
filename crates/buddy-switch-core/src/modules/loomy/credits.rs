//! Loomy 的两池积分解析（`GET /points/records`）与签到结果解析。
//!
//! ## 两个池为什么必须分开报告
//!
//! - **永久积分**：注册奖励 + 新手任务（不过期）；
//! - **每日赠送池**：每天重置，**消耗后不回补**，当天作废。
//!
//! 合并成一个数字会让用户看不出「哪部分今天会作废」。
//!
//! ## 签到语义（容易写错的一点）
//!
//! 「签到」**不是**「+5000 积分」，而是**触发每日额度重置**：
//! `dailyBalance = dailyQuota - dailyConsumed`。幂等判据是响应体的 `alreadyProcessed`
//! （**不是** HTTP 状态码 —— 重复调用同样返回 200）；`dailyQuota` 只在该响应里，
//! **不硬编码 5000**（额度会随活动变化）。
//!
//! 蓝本为 `../free-llm-router/src/providers/loomy.ts` 的 `balance` / `checkin`（协议已实测）。

use serde_json::Value;

/// 两池积分快照。
#[derive(Debug, Clone, PartialEq)]
pub struct CreditsSnapshot {
    /// 永久积分（`balance`）。
    pub permanent: i64,
    /// 每日池余额（`dailyBalance` = `dailyQuota - dailyConsumed`；当天作废）。
    pub daily: i64,
    /// 永久 + 每日（`availableBalance`；缺省时本地现算）。
    pub total: i64,
}

/// 签到结果。
#[derive(Debug, Clone, PartialEq)]
pub struct CheckinOutcome {
    /// 今天是否已处理（幂等命中）。
    pub already_done: bool,
    /// 本次可用的每日额度（`dailyQuota - dailyConsumed`；未知 ⇒ 0）。
    pub gained: i64,
    /// 可读说明（含「该怎么办」）。
    pub detail: String,
}

/// 把任意 JSON 值读成数字；非法返回 `None`（不编造 0）。
fn read_number(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// 解析 `/points/records` 业务信封的 `data`。
///
/// ⚠️ `balance` 是核心字段：缺它说明响应形状不对 —— 返回错误，**不**返回 0
/// 冒充「已用尽」（那会把「查不到」伪装成「没有」）。
pub fn parse_credits_data(data: &Value) -> Result<CreditsSnapshot, String> {
    let Some(permanent) = read_number(data.get("balance")) else {
        return Err(
            "Loomy 余额响应缺少 `balance` 字段：无法确认额度（不返回 0 以免误报为「已用尽」）"
                .to_string(),
        );
    };
    let daily = read_number(data.get("dailyBalance")).unwrap_or(0);
    let total = read_number(data.get("availableBalance")).unwrap_or(permanent.saturating_add(daily));
    Ok(CreditsSnapshot {
        permanent,
        daily,
        total,
    })
}

/// 解析 `/points/first-login` 业务信封的 `data`（签到）。
pub fn parse_checkin_data(data: &Value) -> CheckinOutcome {
    let quota = read_number(data.get("dailyQuota"));
    let balance = read_number(data.get("dailyBalance"));
    let consumed = read_number(data.get("dailyConsumed"));
    let already = data.get("alreadyProcessed").and_then(|v| v.as_bool()) == Some(true);

    if already {
        return CheckinOutcome {
            already_done: true,
            gained: 0,
            detail: match (balance, quota) {
                (Some(balance), Some(quota)) => format!(
                    "今日每日额度已初始化：{balance}/{quota}（消耗后不回补）"
                ),
                _ => "今日每日额度已初始化（幂等命中，未重复发放）".to_string(),
            },
        };
    }
    // 首次处理：`gained` 是本次可用的每日额度（服务端不给差值，现算；缺 ⇒ 0 不编造）。
    let gained = match (quota, consumed) {
        (Some(quota), Some(consumed)) => (quota - consumed).max(0),
        (Some(quota), None) => quota,
        _ => 0,
    };
    CheckinOutcome {
        already_done: false,
        gained,
        detail: match quota {
            Some(quota) => format!("已触发今日每日赠送额度：可用 {gained}/{quota}（消耗后不回补，次日重置）"),
            None => "已触发今日每日赠送额度（服务端未回报 quota，具体数额未知）".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn credits_missing_balance_is_an_error_not_zero() {
        let empty = json!({});
        assert!(parse_credits_data(&empty).is_err(), "缺 balance ⇒ 报错");
        let ok = json!({"balance": 15000, "dailyBalance": 4992, "availableBalance": 19992});
        let snap = parse_credits_data(&ok).expect("应解析成功");
        assert_eq!(snap.permanent, 15000);
        assert_eq!(snap.daily, 4992);
        assert_eq!(snap.total, 19992);
        // availableBalance 缺 ⇒ 本地现算。
        let partial = json!({"balance": 100});
        assert_eq!(parse_credits_data(&partial).unwrap().total, 100);
    }

    #[test]
    fn checkin_idempotence_reads_already_processed_not_http() {
        // 已处理：gained = 0，detail 说明幂等命中。
        let done = json!({"alreadyProcessed": true, "dailyQuota": 5000, "dailyBalance": 4992});
        let outcome = parse_checkin_data(&done);
        assert!(outcome.already_done);
        assert_eq!(outcome.gained, 0);
        // 首次：gained = quota - consumed。
        let first = json!({"alreadyProcessed": false, "dailyQuota": 5000, "dailyConsumed": 8});
        let outcome = parse_checkin_data(&first);
        assert!(!outcome.already_done);
        assert_eq!(outcome.gained, 4992);
        // quota 缺 ⇒ gained 0 且文案说明未知（不硬编码 5000）。
        let unknown = json!({"alreadyProcessed": false});
        let outcome = parse_checkin_data(&unknown);
        assert_eq!(outcome.gained, 0);
        assert!(outcome.detail.contains("未知"), "{}", outcome.detail);
    }
}
