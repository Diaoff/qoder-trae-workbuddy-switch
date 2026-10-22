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

use crate::modules::loomy::region::LoomyRegion;

/// 签到请求体（`{}`，与蓝本一致）。
pub const CHECKIN_BODY: &str = "{}";

/// 签到端点路径。
pub const CHECKIN_PATH: &str = "/points/first-login";

pub use crate::modules::loomy::credits::{parse_checkin_data, CheckinOutcome};

/// 批量签到：遍历账号库里**有 session** 的账号，逐个触发每日额度重置。
///
/// ## 三个刻意的设计（都来自参考实现的踩坑记录）
///
/// 1. **串行，绝不并发**。`../free-llm-router/src/index.ts` 的一键签到循环注释写着
///    「逐账号串行（不是 `Promise.all`）：同时出站连接上限是 6，且并行打上游更容易触发风控」。
///    签到是**写**操作，并发打到讯飞更容易被判异常 —— 宁可慢，不要被风控。
/// 2. **部分失败不中断整轮**。一个死账号（`100002`）不该让其余账号今天签不上；
///    失败照实记进 `failed`，成功的那几个仍然写库。
/// 3. **成功后回写 `last_checkin`**。账号卡的「今日已签」徽章与「上次签到」都读它 ——
///    不回写的话用户签了还是看到「未签」。参考实现这里是**后补的 bug 修复**
///    （注释说「记录签到日」但实际没写，面板标签永远为空），我们不要重蹈。
///
/// 与 [`super::credits_daily::run_sample_cycle_for`] 的区别：那个是**只读**采样
/// （默认开），本函数是**写**动作（替用户领东西）⇒ 排程默认**关**（opt-in）。
pub async fn run_cycle_for(region: LoomyRegion) -> serde_json::Value {
    use serde_json::json;

    let file = crate::modules::loomy::account::load_accounts_for(region);
    let mut ok_count = 0usize;
    let mut already_count = 0usize;
    let mut skipped = 0usize;
    let mut gained_total = 0i64;
    let mut results: Vec<serde_json::Value> = Vec::new();
    let mut failed: Vec<serde_json::Value> = Vec::new();

    for account in &file.accounts {
        // 没有 session 的账号（导入失败 / 已失效被清空）调不动上游：跳过不算失败。
        if account.access_token.trim().is_empty() {
            skipped += 1;
            results.push(json!({
                "uid": account.uid,
                "name": crate::modules::loomy::account::display_name(account),
                "skipped": true,
                "detail": "没有可用的 session（Loomy 无续期端点，请重新登录）",
            }));
            continue;
        }

        match crate::modules::loomy::api::checkin_for(region, &account.access_token).await {
            Ok(outcome) => {
                if outcome.already_done {
                    already_count += 1;
                } else {
                    ok_count += 1;
                }
                gained_total += outcome.gained;

                // ★ 回写签到戳（幂等命中同样要写 —— 它证明「今天这一轮跑过了」）。
                let mut updated = account.clone();
                updated.last_checkin =
                    Some(crate::modules::qoder::store::timestamp());
                let _ = crate::modules::loomy::account::upsert_account_for(region, updated);

                results.push(json!({
                    "uid": account.uid,
                    "name": crate::modules::loomy::account::display_name(account),
                    "ok": true,
                    "alreadyDone": outcome.already_done,
                    "gained": outcome.gained,
                    "detail": outcome.detail,
                }));
            }
            Err(error) => {
                failed.push(json!({ "uid": account.uid, "error": error.clone() }));
                results.push(json!({
                    "uid": account.uid,
                    "name": crate::modules::loomy::account::display_name(account),
                    "ok": false,
                    "error": error,
                }));
            }
        }
    }

    json!({
        "region": region.as_str(),
        "total": file.accounts.len(),
        "ok": ok_count,
        "already": already_count,
        "skipped": skipped,
        "failed": failed.len(),
        "gained": gained_total,
        "results": results,
        "failedUids": failed
            .iter()
            .filter_map(|item| item.get("uid").and_then(|v| v.as_str()).map(str::to_string))
            .collect::<Vec<String>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 批量签到的**默认可执行性**：默认配置里它必须是**关**的。
    ///
    /// 可证伪：把 `loomy_checkin_enabled` 默认改成 `true` ⇒ 本条红。
    /// 这条断言的价值：签到是**写**动作（替用户去上游领东西），默认打开等于
    /// 「升级后凭空拿用户凭据去签到」—— 与 Trae / Qoder 的 `*_checkin` 同一条纪律。
    #[test]
    fn scheduled_checkin_is_opt_in_by_default() {
        let cfg = crate::modules::schedule::default_schedule();
        assert!(
            !cfg.loomy_checkin_enabled,
            "Loomy 自动签到必须**默认关闭**（写动作，用户没要求过）"
        );
        assert!(
            cfg.loomy_points_sample_enabled,
            "Loomy 积分采样应保持默认开启（只读 GET，不替用户领东西）"
        );
        // 两者是**独立**任务：关掉签到不该影响采样，反之亦然。
        let mut only_sample = cfg.clone();
        only_sample.loomy_checkin_enabled = false;
        assert!(
            crate::modules::schedule::ScheduleTask::LoomyPointsSample.enabled(&only_sample),
            "关签到不得连带关掉采样"
        );
    }

    #[test]
    fn endpoint_constants_are_stable() {
        assert_eq!(CHECKIN_PATH, "/points/first-login");
        assert_eq!(CHECKIN_BODY, "{}");
        // 幂等语义的烟囱测试：alreadyProcessed 驱动结果（解析逻辑单测在 credits.rs）。
        let outcome = parse_checkin_data(&json!({"alreadyProcessed": true}));
        assert!(outcome.already_done);
    }
}
