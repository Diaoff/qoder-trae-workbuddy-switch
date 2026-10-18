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

/// 「**今日已签**」这一档的领取响应体（纯函数，可测）。
///
/// ★ 与 [`disabled_claim_response`] 成对：两个出口的 `ok` 值必须**明确相反**且各有测试，
///   否则「把未开放误报成成功」这种回归没有任何测试能接住（`claim_for` 本身要账号库，
///   测不了；出口提成纯函数才测得到）。
/// `Already` 是**真的成功**（`today_checked_in` 有上游依据），必须继续算成功。
pub fn already_claim_response(uid: &str, status: &api::CheckinStatus) -> Value {
    json!({
        "ok": true,
        "already": true,
        "uid": uid,
        "streakDays": status.streak_days,
        "rewardCredits": status.reward_credits,
    })
}

/// 「**通道未开放**」这一档的领取响应体（纯函数，可测）。
///
/// ## ⚠️ 为什么 `ok` 必须是 `false`（issue #46 的 D 项）
///
/// Qoder 的 legacy 签到端点对**任何** campaignKey 都回 409 `AlreadyExists`
/// —— 编造的 `campaignKey: totally_bogus_key` 同样 409，而 `status` 端点同时报
/// `DISABLED` + `totalClaimDays=0` ⇒ **这个 409 不携带幂等语义**，
/// 「通道未开放」**不等于**「领到了」。
///
/// 旧实现回 `{"ok": true, "disabled": true}`，而前端按 `res.ok` 计数
/// ⇒ 点「一键签到」显示「N 个成功」，而 `totalClaimDays=0`、额度纹丝不动。
/// 用户于是反复觉得「签到不能用」。
///
/// ## 为什么不是折进「失败」
///
/// 「重试失败项」会把这些账号反复重试，而重试**永远不会成功**（端点已关停）——
/// 那是在教用户做一件没用的操作。故单独成第三档，由前端如实呈现。
///
/// `Already`（今日已签）**继续**算成功：那条有 `today_checked_in` 作依据。
pub fn disabled_claim_response(uid: &str, upstream_status: &str) -> Value {
    json!({
        "ok": false,
        "disabled": true,
        "uid": uid,
        "status": upstream_status,
    })
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
    let account =
        account::find_account_for(region, uid).ok_or_else(|| format!("账号不存在: {uid}"))?;
    let access = access_token_of(&account)
        .ok_or_else(|| "这条账号没有可调用的 access token（PAT 不能用于签到端点）".to_string())?;

    let status = api::checkin_status(region, uid, access).await?;
    match claim_decision(&status) {
        ClaimDecision::Already => Ok(already_claim_response(uid, &status)),
        ClaimDecision::Disabled(upstream_status) => {
            Ok(disabled_claim_response(uid, &upstream_status))
        }
        ClaimDecision::ClaimNow => {
            let result = api::checkin_claim(region, uid, access).await?;
            // 上游把失败放在 body 里（`success=false`）而不是 HTTP 状态。
            if result.get("success") == Some(&Value::Bool(false)) {
                // 复查一次：上游可能已记账（先扣后报错等形态）。
                if let Ok(recheck) = api::checkin_status(region, uid, access).await {
                    if recheck.today_checked_in {
                        return Ok(
                            json!({ "ok": true, "already": true, "uid": uid, "rechecked": true }),
                        );
                    }
                }
                let detail = result
                    .get("error")
                    .map(|error| error.to_string())
                    .unwrap_or_else(|| result.to_string());
                let detail: String = detail.chars().take(160).collect();
                return Err(format!("签到失败: {detail}"));
            }
            // ⚠️ 走 `api::as_num`（同时接受浮点）：上游把额度写成 `100.0` 时 `as_i64` 返回
            //    None ⇒ 签到成功却报「+0」，用户会以为没领到。
            let reward = result
                .get("rewardCredits")
                .and_then(api::as_num)
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
    /// ★ 「今日已签」**仍然**是成功（issue #46 的 D 项的另一半）。
    ///
    /// 与 `disabled_is_not_reported_as_success` 成对：两个出口的 `ok` 必须相反，
    /// 否则「把已签说成失败」与「把未开放说成成功」同样会骗到用户。
    #[test]
    fn already_is_reported_as_success() {
        let mut state = status("CLAIMED", true);
        state.streak_days = 3;
        state.reward_credits = 100;
        assert_eq!(
            claim_decision(&state),
            ClaimDecision::Already,
            "today_checked_in ⇒ Already"
        );
        let response = already_claim_response("u1", &state);
        assert_eq!(response["ok"], Value::Bool(true), "已签是真成功");
        assert_eq!(response["already"], Value::Bool(true));
        assert_eq!(response["streakDays"], 3);
        assert_eq!(response["rewardCredits"], 100);
        // 两档的 ok 必须相反（防止有人顺手把两处都写成 true / 都写成 false）。
        assert_ne!(
            response["ok"],
            disabled_claim_response("u1", "DISABLED")["ok"],
            "「已签」与「未开放」的 ok 必须相反"
        );
    }

    /// ★ 「通道未开放」**不得**报成功（issue #46 的 D 项的回归钉子）。
    ///
    /// 可证伪：把 `disabled_claim_response` 里的 `ok` 改回 `true` ⇒ 第一条断言红。
    #[test]
    fn disabled_is_not_reported_as_success() {
        let response = disabled_claim_response("u1", "DISABLED");
        assert_eq!(
            response["ok"],
            Value::Bool(false),
            "上游 409 不携带幂等语义（编造的 campaignKey 也 409）⇒ 未开放 ≠ 领到了"
        );
        assert_eq!(
            response["disabled"],
            Value::Bool(true),
            "第三档要有自己的标记"
        );
        assert_eq!(response["status"], "DISABLED", "上游状态词如实透出");
        // 前端按 `ok` 计数 ⇒ 这里必须是 false，否则批量计数会说谎。
        assert_eq!(
            claim_decision(&status("DISABLED", false)),
            ClaimDecision::Disabled("DISABLED".into())
        );
    }

    #[test]
    fn claim_decision_covers_three_states() {
        assert_eq!(
            claim_decision(&status("CLAIMED", true)),
            ClaimDecision::Already
        );
        assert_eq!(
            claim_decision(&status("DISABLED", false)),
            ClaimDecision::Disabled("DISABLED".into())
        );
        assert_eq!(
            claim_decision(&status("CLAIMABLE", false)),
            ClaimDecision::ClaimNow
        );
        // CLAIMED 但今天没签（跨日）⇒ 仍要尝试领取（不能只看状态词）。
        assert_eq!(
            claim_decision(&status("CLAIMED", false)),
            ClaimDecision::ClaimNow
        );
    }
}
