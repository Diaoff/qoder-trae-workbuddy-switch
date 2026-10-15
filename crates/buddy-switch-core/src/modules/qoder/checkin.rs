//! Qoder **签到**：状态查询与领取（`/sash` 通道）。
//!
//! ## 语义要点
//!
//! - **国际版没有该接口**（404/405/410）⇒ 不是错误，是「本区域未开放」——
//!   状态视图给 `unavailable` + 可读原因，领取**不发起**无意义的请求。
//! - 领取是**状态驱动**的（与官方客户端同构）：先查状态 → 已签 ⇒ 幂等返回；
//!   活动未开放（非 CLAIMABLE/CLAIMED）⇒ 成功跳过；否则 claim。
//! - claim 返回 `success=false` 时**复查一次状态**（上游可能已记账）——
//!   复查确认已签 ⇒ 如实报「今日已签到（复查确认）」，而不是把已签名额报成失败。
//!
//! ## 为什么没有本地「盖章」
//!
//! 官方客户端的参考实现会在本地记「今日已签」供其**调度器**去重；本仓暂无 Qoder
//! 签到调度，签到与否以**上游 `lastClaimedAt` 为准**——不引入会与上游漂移的第二份状态。

use serde_json::{json, Value};

use super::account::{self, RawAccount};
use super::api;
use super::region::QoderRegion;

/// 领取决策（纯函数，可测）：由状态推出下一步。
#[derive(Debug, Clone, PartialEq)]
pub enum ClaimDecision {
    /// 今日已签 ⇒ 幂等成功。
    Already,
    /// 活动未开放（DISABLED 等）⇒ 成功跳过并给出上游状态词。
    Disabled(String),
    /// 可以领取。
    ClaimNow,
}

/// 由签到状态推出领取决策（见模块头）。
pub fn claim_decision(status: &api::CheckinStatus) -> ClaimDecision {
    if status.today_checked_in {
        return ClaimDecision::Already;
    }
    if !status.active {
        return ClaimDecision::Disabled(status.status.clone());
    }
    ClaimDecision::ClaimNow
}

/// 一条账号的凭据是否够发起 openapi 调用（需要 `access`；PAT 不适用签到端点的 Bearer 头）。
fn access_token_of(account: &RawAccount) -> Option<&str> {
    account
        .access_token
        .as_deref()
        .filter(|token| !token.is_empty())
}

/// 区域签到状态视图：逐账号状态 + 汇总。`unavailable` 是**区域级**结论。
pub async fn status_for(region: QoderRegion) -> Value {
    let file = account::load_accounts_for(region);
    let mut rows = Vec::new();
    let mut unavailable = false;
    let mut unavailable_reason: Option<String> = None;

    for account in &file.accounts {
        let name = account::display_name(account);
        let Some(access) = access_token_of(account) else {
            rows.push(json!({
                "uid": account.uid,
                "name": name,
                "ok": false,
                "error": "没有可调用的 access token（PAT 不能用于签到端点）",
                "unavailable": false,
            }));
            continue;
        };
        match api::checkin_status(region, &account.uid, access).await {
            Ok(status) => {
                rows.push(json!({
                    "uid": account.uid,
                    "name": name,
                    "ok": true,
                    "status": status.status,
                    "active": status.active,
                    "todayCheckedIn": status.today_checked_in,
                    "streakDays": status.streak_days,
                    "totalClaimDays": status.total_claim_days,
                    "rewardCredits": status.reward_credits,
                    "totalRewardCredits": status.total_reward_credits,
                    "nextClaimAt": status.next_claim_at,
                    "lastClaimedAt": status.last_claimed_at,
                    "unavailable": false,
                }));
            }
            Err(error) if error.starts_with(api::CHECKIN_REASON_NOT_FOUND) => {
                // 区域级结论：接口不存在 ⇒ 整区标记，逐账号不再重复报。
                unavailable = true;
                unavailable_reason.get_or_insert(error.clone());
                rows.push(json!({
                    "uid": account.uid,
                    "name": name,
                    "ok": false,
                    "error": error,
                    "unavailable": true,
                }));
            }
            Err(error) => {
                rows.push(json!({
                    "uid": account.uid,
                    "name": name,
                    "ok": false,
                    "error": error,
                    "unavailable": false,
                }));
            }
        }
    }

    let checked = rows
        .iter()
        .filter(|row| row["todayCheckedIn"] == Value::Bool(true))
        .count();
    json!({
        "region": region.as_str(),
        "unavailable": unavailable,
        "reason": unavailable_reason,
        "summary": {
            "total": file.accounts.len(),
            "checked": checked,
            "claimable": rows.iter().filter(|row| row["active"] == Value::Bool(true) && row["todayCheckedIn"] != Value::Bool(true)).count(),
        },
        "accounts": rows,
    })
}

/// 单账号签到（状态驱动；见模块头的三步语义）。
pub async fn claim_for(region: QoderRegion, uid: &str) -> Result<Value, String> {
    let account = account::find_account_for(region, uid)
        .ok_or_else(|| format!("账号不存在: {uid}"))?;
    let access = access_token_of(&account)
        .ok_or_else(|| "这条账号没有可调用的 access token（PAT 不能用于签到端点）".to_string())?;

    let status = api::checkin_status(region, uid, access).await?;
    match claim_decision(&status) {
        ClaimDecision::Already => Ok(json!({
            "ok": true, "already": true, "uid": uid,
            "streakDays": status.streak_days, "rewardCredits": status.reward_credits,
        })),
        ClaimDecision::Disabled(upstream_status) => Ok(json!({
            "ok": true, "disabled": true, "uid": uid, "status": upstream_status,
        })),
        ClaimDecision::ClaimNow => {
            let result = api::checkin_claim(region, uid, access).await?;
            // 上游把失败放在 body 里（`success=false`）而不是 HTTP 状态。
            if result.get("success") == Some(&Value::Bool(false)) {
                // 复查一次：上游可能已记账（先扣后报错等形态）。
                if let Ok(recheck) = api::checkin_status(region, uid, access).await {
                    if recheck.today_checked_in {
                        return Ok(json!({ "ok": true, "already": true, "uid": uid, "rechecked": true }));
                    }
                }
                let detail = result
                    .get("error")
                    .map(|error| error.to_string())
                    .unwrap_or_else(|| result.to_string());
                let detail: String = detail.chars().take(160).collect();
                return Err(format!("签到失败: {detail}"));
            }
            let reward = result
                .get("rewardCredits")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            Ok(json!({ "ok": true, "uid": uid, "rewardCredits": reward }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(status: &str, today: bool) -> api::CheckinStatus {
        api::CheckinStatus {
            status: status.into(),
            active: status == "CLAIMABLE" || status == "CLAIMED",
            today_checked_in: today,
            streak_days: 3,
            total_claim_days: 12,
            reward_credits: 100,
            total_reward_credits: 1200,
            next_claim_at: 0,
            last_claimed_at: 0,
            reward_expires_at: 0,
        }
    }

    /// 变异：把「已签」判据从 `today_checked_in` 改成只看 status ⇒ 断言红
    /// （跨日时 CLAIMED 但今天没签的账号会被误判成已签而漏签）。
    #[test]
    fn claim_decision_covers_three_states() {
        assert_eq!(claim_decision(&status("CLAIMED", true)), ClaimDecision::Already);
        assert_eq!(
            claim_decision(&status("DISABLED", false)),
            ClaimDecision::Disabled("DISABLED".into())
        );
        assert_eq!(claim_decision(&status("CLAIMABLE", false)), ClaimDecision::ClaimNow);
        // CLAIMED 但今天没签（跨日）⇒ 仍要尝试领取（不能只看状态词）。
        assert_eq!(claim_decision(&status("CLAIMED", false)), ClaimDecision::ClaimNow);
    }
}
