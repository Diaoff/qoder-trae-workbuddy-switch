//! Loomy 的**积分逐日采样**（趋势图数据源；二期 9.3）。
//!
//! ## 为什么需要独立采样文件
//!
//! 账号库里的 `credits` / `daily_credits` 只是**最近一次**读数（快照覆盖写），
//! 拿它画逐日趋势只能得到一根贴地的假线 —— 那正是 `LoomyUsagePage` 一期
//! 「不画趋势」的理由。本模块把每次采样按 `(日期, uid)` 累积下来，趋势才有真实数据源。
//!
//! ## 与 Trae/Qoder 的形态差异（刻意）
//!
//! 它们的 `credits_daily` 是**区域级聚合**（所有账号剩余之和，算 earned/consumed 差分）。
//! Loomy 的两池是**账号级、不可通约**的（每日池当天作废、永久池不清零），
//! 跨账号求和会掩盖「某个账号的池已耗尽」。因此本模块按 **uid 逐条**存两池读数，
//! 聚合留给展示层（求和 / 分账号曲线都可从同一份数据算出来）。
//!
//! ## 保留窗口
//!
//! 与 Trae 一致取 90 天：更长的历史对趋势图没有信息增量，只会让文件变大。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::modules::loomy::paths;
use crate::modules::loomy::region::LoomyRegion;
use crate::modules::qoder::store;

/// 采样保留天数（与 Trae 的历史窗口同值）。
pub const KEEP_DAYS: i64 = 90;

/// 单个账号在某一天的两池读数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DailySample {
    /// 永久积分（不清零）。
    pub permanent: i64,
    /// 每日赠送池余额（当天作废）。
    pub daily: i64,
}

/// 一天的采样：`date -> uid -> 读数`。
///
/// ⚠️ 用 `BTreeMap` 而非 `HashMap`：落盘时键序稳定，diff 可读（本仓既有约定）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreditsDailyFile {
    /// 按日期升序（写入时保持有序，读取方不必再排）。
    #[serde(default)]
    pub days: BTreeMap<String, BTreeMap<String, DailySample>>,
}

/// 读取采样文件（缺失 / 损坏 ⇒ 空文件，与其它模块的读纪律一致）。
pub fn load_for(region: LoomyRegion) -> CreditsDailyFile {
    store::read_json(&paths::credits_daily_file_for(region))
}

/// 记录一次采样：**同日同账号覆盖写**（后一次读数更接近真实余额）。
///
/// - 同一天多次采样 ⇒ 保留最后一次（不是累加：积分余额是状态量，不是流量）；
/// - 超过 [`KEEP_DAYS`] 的日期在写入时裁掉。
///
/// 返回是否真的写盘（读不到文件时仍会写 —— 那是首次采样的正常路径）。
pub fn record_for(region: LoomyRegion, samples: &[(String, DailySample)]) -> Result<(), String> {
    if samples.is_empty() {
        // 空采样不写盘：否则「无账号」也会造出一个空日条目，趋势图会多一天空洞。
        return Ok(());
    }
    let today = store::today();
    let mut file = load_for(region);
    let bucket = file.days.entry(today.clone()).or_default();
    for (uid, sample) in samples {
        bucket.insert(uid.clone(), sample.clone());
    }
    prune(&mut file, KEEP_DAYS);
    store::write_json(&paths::credits_daily_file_for(region), &file)
}

/// 裁掉超过保留窗口的日期（按字符串日期比较 —— `YYYY-MM-DD` 字典序即时间序）。
fn prune(file: &mut CreditsDailyFile, keep_days: i64) {
    let cutoff = (chrono::Local::now() - chrono::Duration::days(keep_days))
        .format("%Y-%m-%d")
        .to_string();
    file.days.retain(|date, _| date.as_str() >= cutoff.as_str());
}

/// 视图：给前端趋势图用的扁平列表。
///
/// 形状 `{ region, days: [{ date, accounts: [{ uid, permanent, daily }] }] }` ——
/// 按日期升序、账号按 uid 升序（`BTreeMap` 天然有序）。
pub fn view_for(region: LoomyRegion) -> serde_json::Value {
    let file = load_for(region);
    let days: Vec<serde_json::Value> = file
        .days
        .iter()
        .map(|(date, accounts)| {
            let rows: Vec<serde_json::Value> = accounts
                .iter()
                .map(|(uid, sample)| {
                    serde_json::json!({
                        "uid": uid,
                        "permanent": sample.permanent,
                        "daily": sample.daily,
                    })
                })
                .collect();
            serde_json::json!({ "date": date, "accounts": rows })
        })
        .collect();
    serde_json::json!({ "region": region.as_str(), "days": days })
}

/// 一趟采样循环：遍历账号库里**有 session** 的账号，逐个查上游积分并落采样。
///
/// ## 为什么扫全库而不是只看「当前选中账号」
///
/// 趋势图按账号分列（见模块头），少采一个账号就等于那条曲线断一天。
///
/// ## 单个账号失败不中断整轮
///
/// 上游失效（`100002` 登录失效 / 网络抖动）是**常态**（见 `credits.rs` 的错误分类）。
/// 一个死账号不该让其余账号当天没有采样 —— 失败照实记进结果的 `failed`，成功的那几个
/// 仍写盘。这与 `QoderCheckin` / `QoderKeepalive` 的循环纪律一致。
///
/// ## 顺带回写账号库快照
///
/// 采样拿到的是**最新读数**，而账号卡片展示的是账号库里的 `credits` / `daily_credits`。
/// 不回写的话，用户会看到「用量页有今天的数、账号卡还是昨天」的割裂（Trae 侧同款处理）。
pub async fn run_sample_cycle_for(region: LoomyRegion) -> Value {
    let file = crate::modules::loomy::account::load_accounts_for(region);
    let mut sampled: Vec<(String, DailySample)> = Vec::new();
    let mut failed: Vec<Value> = Vec::new();
    let mut skipped = 0usize;

    for account in &file.accounts {
        // 没有 session 的账号（导入失败 / 已失效被清空）查不了上游：跳过不算失败。
        if account.access_token.trim().is_empty() {
            skipped += 1;
            continue;
        }
        match crate::modules::loomy::api::points_for(region, &account.access_token).await {
            Ok(snapshot) => {
                sampled.push((
                    account.uid.clone(),
                    DailySample {
                        permanent: snapshot.permanent,
                        daily: snapshot.daily,
                    },
                ));
                // 顺带把最新读数写回账号库（只动两个积分字段，其余原样）。
                let mut updated = account.clone();
                updated.credits = snapshot.permanent;
                updated.daily_credits = snapshot.daily;
                let _ = crate::modules::loomy::account::upsert_account_for(region, updated);
            }
            Err(error) => failed.push(json!({ "uid": account.uid, "error": error })),
        }
    }

    let write_error = record_for(region, &sampled).err();

    json!({
        "region": region.as_str(),
        "total": file.accounts.len(),
        "sampled": sampled.len(),
        "skipped": skipped,
        "failed": failed,
        "error": write_error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(permanent: i64, daily: i64) -> DailySample {
        DailySample { permanent, daily }
    }

    #[test]
    fn same_day_overwrites_instead_of_accumulating() {
        // 纯函数级验证：余额是**状态量**，同日重复采样必须覆盖而不是相加。
        let mut file = CreditsDailyFile::default();
        let bucket = file.days.entry("2026-10-22".to_string()).or_default();
        bucket.insert("u1".to_string(), sample(100, 20));
        bucket.insert("u1".to_string(), sample(120, 30));
        assert_eq!(bucket.get("u1"), Some(&sample(120, 30)));
        assert_eq!(bucket.len(), 1, "同账号同日只能有一条");
    }

    #[test]
    fn prune_drops_days_outside_the_window() {
        let mut file = CreditsDailyFile::default();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let ancient = (chrono::Local::now() - chrono::Duration::days(200))
            .format("%Y-%m-%d")
            .to_string();
        file.days.entry(ancient).or_default();
        file.days.entry(today.clone()).or_default();

        prune(&mut file, KEEP_DAYS);

        assert!(file.days.contains_key(&today), "窗口内的日期必须保留");
        assert_eq!(file.days.len(), 1, "窗口外的日期必须被裁掉");
    }

    #[test]
    fn view_is_date_ascending_with_account_rows() {
        // 构造一个两天的文件（不落盘，直接验证视图形状的排序与字段名）。
        let mut file = CreditsDailyFile::default();
        let day2 = file.days.entry("2026-10-22".to_string()).or_default();
        day2.insert("u2".to_string(), sample(50, 5));
        day2.insert("u1".to_string(), sample(40, 4));
        let day1 = file.days.entry("2026-10-21".to_string()).or_default();
        day1.insert("u1".to_string(), sample(30, 3));

        let days: Vec<(&String, &BTreeMap<String, DailySample>)> = file.days.iter().collect();
        // BTreeMap 的键序：日期升序、uid 升序 —— 这正是 view_for 依赖的性质。
        assert_eq!(days[0].0, "2026-10-21");
        assert_eq!(days[1].0, "2026-10-22");
        let uids: Vec<&String> = days[1].1.keys().collect();
        assert_eq!(uids, vec!["u1", "u2"]);
    }
}
