//! Loomy 账号池 —— 复用共享选号内核（[`PoolEntryLike`]）。
//!
//! 与 Qoder 池同款纪律：条目实现跨产品线契约，选号调
//! [`pick_with_preference`]；缺的数据一律中性降级。差异：
//! - Loomy 无 refresh 端点 ⇒ `auth_dead` 的唯一出口是**重新登录**（诚实文案）；
//! - 无模型级冷却的特殊语义（上游没有「按模型限流」的实证）⇒ 429 账号级短冷却。

use std::collections::{BTreeMap, HashSet};

use serde_json::{json, Value};

use buddy_switch_core::modules::loomy::{
    account::{self, RawAccount},
    region::LoomyRegion,
};

use crate::pool::entry::CostTier;
use crate::pool::entry_like::PoolEntryLike;
use crate::pool::pick::{pick_with_preference, PickPolicy};
use crate::pool::RealmTag;

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

/// 上游错误分类（决定冷却时长与是否换号）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoomyErrKind {
    /// 429：账号级短冷却 + 换号。
    RateLimited,
    /// 401/403 或业务码 `100002`（登录失效）⇒ 长冷却 + 标记，换号。
    ///
    /// ⚠️ Loomy **无 refresh 端点**：本状态的唯一出口是重新登录（文案要给出路）。
    AuthDead,
    /// 5xx / 传输层：短冷却 + 换号。
    Transient,
    /// 4xx（非上述）：客户端参数错 ⇒ **不换号**。
    ClientError,
}

impl LoomyErrKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LoomyErrKind::RateLimited => "rate_limited",
            LoomyErrKind::AuthDead => "auth_dead",
            LoomyErrKind::Transient => "transient",
            LoomyErrKind::ClientError => "client_error",
        }
    }

    /// 是否值得换号重试。
    pub fn should_rotate(self) -> bool {
        !matches!(self, LoomyErrKind::ClientError)
    }
}

/// HTTP 状态码分类。
pub fn classify_http(status: u16) -> LoomyErrKind {
    match status {
        429 => LoomyErrKind::RateLimited,
        401 | 403 => LoomyErrKind::AuthDead,
        500..=599 => LoomyErrKind::Transient,
        400..=499 => LoomyErrKind::ClientError,
        _ => LoomyErrKind::Transient,
    }
}

/// 池中的一条 Loomy 账号。
#[derive(Debug, Clone)]
pub struct LoomyPoolEntry {
    pub uid: String,
    pub name: String,
    pub access_token: String,
    /// 凭据死（401/403 / `100002`）：Loomy 无 refresh ⇒ 唯一出口是重新登录。
    pub auth_dead: bool,
    /// 账号级冷却解冻时刻（毫秒；0 = 无）。
    pub cooldown_until: i64,
    /// 冷却原因（诊断）。
    pub cooldown_reason: Option<String>,
    pub last_error: Option<String>,
    /// **展示快照**：永久积分（永久池，不清零）。
    ///
    /// `sync_for` 在**每次读池视图前**都会从账号库重建（见其注释与三个调用点）⇒
    /// 这里不是启动时的陈旧值。「刷新积分」写完库、下一次视图就是新数。
    ///
    /// ⚠️ 它与 [`PoolEntryLike::credits`]（选号用的积分）**刻意分开**：那里恒为 0，
    /// 否则会给 Loomy 的选号打开一条按余额加权的路径（见那条 impl 的注释）。
    pub credits: i64,
    /// **展示快照**：每日赠送池余额（当天作废）。
    ///
    /// 与 `credits` 并列而不是相加：两池**不可通约**（永久池不清零、每日池当天作废），
    /// 合并成一个数会让「今天用完就没了」那部分无从辨认（与 `LoomyUsagePage` 两池分列同一条口径）。
    pub daily_credits: i64,
    success_ema: f64,
    error_ema: f64,
    last_used_ms: i64,
    used_seq: u64,
}

impl LoomyPoolEntry {
    fn new(account: &RawAccount) -> Self {
        Self {
            uid: account.uid.clone(),
            name: display_name(account),
            access_token: account.access_token.clone(),
            auth_dead: false,
            cooldown_until: 0,
            cooldown_reason: None,
            last_error: None,
            credits: account.credits,
            daily_credits: account.daily_credits,
            success_ema: 0.0,
            error_ema: 0.0,
            last_used_ms: 0,
            used_seq: 0,
        }
    }

    /// 是否有可调用的凭据。
    pub fn has_access(&self) -> bool {
        !self.access_token.is_empty()
    }

    /// 是否需要**人工干预**：无凭据 / 凭据死（重新登录是唯一出口）。
    pub fn needs_intervention(&self) -> bool {
        !self.has_access() || self.auth_dead
    }
}

/// 展示名回落：昵称 > 手机号 > userid > uid（与 core `account::display_name` 同口径）。
fn display_name(account: &RawAccount) -> String {
    if let Some(nickname) = account.nickname.as_deref().filter(|s| !s.is_empty()) {
        return nickname.to_string();
    }
    if !account.phone.is_empty() {
        return format!("Loomy {}", account.phone);
    }
    if !account.userid.is_empty() {
        return format!("Loomy {}", account.userid);
    }
    format!("Loomy {}", account.uid)
}

impl PoolEntryLike for LoomyPoolEntry {
    fn uid(&self) -> &str {
        &self.uid
    }

    fn credits(&self) -> i64 {
        /* ⚠️ **恒 0，刻意为之**：选号里的 `credits()` 会经 `weight_with_ema` 变成
        `credits / max_credits * 10` 的权重项、并在同权重时当决胜键（`pick.rs`）。
        Loomy 的策略是按「闲置 / 冷却」轮转（`expiring_weight = 0.0`、`cost_tier = Unknown`），
        一旦在这里回报真实余额，就相当于**顺手改了选号优先级**（总是先把余额最多的号用光）。
        展示用的余额在 [`LoomyPoolEntry::credits`] 字段里，与这里分开。

        变异：把这里改成 `self.credits` ⇒ 选号行为变（护栏见 `display_credits_do_not_leak_into_pick`）。 */
        0
    }

    fn expiring_credits(&self, _now_ms: i64) -> i64 {
        0
    }

    fn success_ema(&self) -> f64 {
        self.success_ema
    }

    fn error_ema(&self) -> f64 {
        self.error_ema
    }

    fn last_used_ms(&self) -> i64 {
        self.last_used_ms
    }

    fn used_seq(&self) -> u64 {
        self.used_seq
    }

    fn in_flight(&self) -> i64 {
        0
    }

    fn realm(&self) -> Option<RealmTag> {
        None
    }

    fn is_disabled(&self) -> bool {
        false
    }

    fn cool_expiry_ms(&self) -> i64 {
        self.cooldown_until
    }

    fn is_hard_cooled(&self, _now_ms: i64) -> bool {
        false
    }

    fn healthy_for_request(&self, now: i64, _model: &str) -> bool {
        self.has_access() && !self.auth_dead && self.cooldown_until <= now
    }

    fn prune_model_cooldowns(&mut self, _now: i64) {}

    fn mark_picked(&mut self, now: i64, seq: u64) {
        self.last_used_ms = now;
        self.used_seq = seq;
    }

    fn mark_probed(&mut self, now: i64) {
        self.last_used_ms = now;
    }

    fn cost_per_1k(&self, _now_ms: i64, _model: &str, _ttl_ms: i64) -> f64 {
        0.0
    }

    fn cost_tier(&self, _now_ms: i64, _model: &str, _ttl_ms: i64) -> CostTier {
        CostTier::Unknown
    }
}

impl LoomyPool {
    /// 从账号库重建（保留同 uid 的运行时状态：冷却 / EMA / used_seq）。
    pub fn sync_for(&mut self, region: LoomyRegion) -> usize {
        self.region = Some(region);
        let file = account::load_accounts_for(region);
        let mut next: BTreeMap<String, LoomyPoolEntry> = BTreeMap::new();
        for account in &file.accounts {
            let mut entry = LoomyPoolEntry::new(account);
            if let Some(previous) = self.entries.get(&entry.uid) {
                // 保留运行态；令牌与名字以库为准。
                entry.auth_dead = previous.auth_dead && entry.access_token == previous.access_token;
                entry.cooldown_until = previous.cooldown_until;
                entry.cooldown_reason = previous.cooldown_reason.clone();
                entry.last_error = previous.last_error.clone();
                entry.success_ema = previous.success_ema;
                entry.error_ema = previous.error_ema;
                entry.last_used_ms = previous.last_used_ms;
                entry.used_seq = previous.used_seq;
            }
            next.insert(entry.uid.clone(), entry);
        }
        self.entries = next;
        self.dirty = false;
        self.entries.len()
    }

    /// 排除表的作用域键。
    fn region_key(&self) -> &'static str {
        self.region.unwrap_or(LoomyRegion::Cn).as_str()
    }

    /// 用户显式排除的 uid → 备注。
    fn excluded_map(&self) -> BTreeMap<String, String> {
        crate::pool::exclusions::load_for(&format!("loomy:{}", self.region_key()))
    }

    pub fn entries(&self) -> impl Iterator<Item = &LoomyPoolEntry> {
        self.entries.values()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 选中一个可用于 `model` 的账号（排除 `tried`；`preferred` 优先但可回落）。
    pub fn pick(
        &mut self,
        model: &str,
        tried: &HashSet<String>,
        preferred_uid: Option<&str>,
    ) -> Option<String> {
        let now = now_ms();
        let policy = loomy_pick_policy();
        let seed = now as u64 ^ (self.pick_seq.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let excluded = crate::pool::exclusions::uid_set(&format!("loomy:{}", self.region_key()));
        let (mode, preferred_uid) = crate::pool::parse_preferred(preferred_uid);
        let input = crate::pool::PickInput {
            realm: None,
            model,
            tried,
            excluded: &excluded,
            // ★ issue #76（合并 main 时带入）：本密钥的允许名单。
            //   Loomy 的 Key 库目前还没有「限定账号 / 分组」字段（另三条产品线已接），
            //   因此这里是 `None` = 全部账号 —— 与合并前行为**一致**，不是静默降级：
            //   Loomy 的 Key 只有吊销 / 恢复 / 删除三态，没有授权范围可填。
            allowed: None,
            mode,
            seed,
        };
        pick_with_preference(
            &mut self.entries,
            &mut self.pick_seq,
            &policy,
            now,
            &input,
            preferred_uid,
        )
    }

    /// 成功收尾：复位错误态 + EMA 上升。
    pub fn note_success(&mut self, uid: &str) {
        if let Some(entry) = self.entries.get_mut(uid) {
            entry.success_ema = entry.success_ema * 0.9 + 0.1;
            entry.error_ema *= 0.9;
            entry.cooldown_until = 0;
            entry.cooldown_until = 0;
            entry.cooldown_reason = None;
            entry.last_error = None;
            entry.auth_dead = false;
        }
    }

    /// 记一次错误并按类型冷却。
    pub fn apply_error(&mut self, uid: &str, kind: LoomyErrKind, reason: &str) {
        let now = now_ms();
        let Some(entry) = self.entries.get_mut(uid) else {
            return;
        };
        entry.success_ema *= 0.9;
        entry.error_ema = entry.error_ema * 0.9 + 0.1;
        entry.last_error = Some(format!("{}: {reason}", kind.as_str()));
        match kind {
            LoomyErrKind::RateLimited => {
                entry.cooldown_until = now + 60_000;
                entry.cooldown_reason = Some("上游限流".to_string());
            }
            LoomyErrKind::AuthDead => {
                entry.auth_dead = true;
                entry.cooldown_until = now + 30 * 60_000;
                entry.cooldown_reason =
                    Some("登录已失效（Loomy 无续期端点，需重新登录）".to_string());
            }
            LoomyErrKind::Transient => {
                entry.cooldown_until = now + 10_000;
                entry.cooldown_reason = Some("上游暂时不可用".to_string());
            }
            LoomyErrKind::ClientError => {}
        }
    }

    /// 池摘要（统一网关管理面）。
    ///
    /// ★ 字段名必须与另外三条产品线**逐字一致**（`available` / `cooling` / `disabled` /
    /// `expired` / `zero_credits` / `total_credits`）：统一网关的 `pool_object` 只做
    /// **键值摊平**、不做改名，前端 `UnifiedGatewayPool` 也只认这套名字。
    ///
    /// ⚠️ 曾经这里用 `usable` / `authDead` —— 统一网关页渲染成「**undefined 可用**」，
    /// 而账号状态、冷却、禁用数全被读成 `undefined`（池卡片看起来一切正常，数字全是假的）。
    /// Loomy 是第四个产品线，最晚落地，没有跟上这套契约。
    ///
    /// Loomy 的**两池不可通约** ⇒ 摘要里只合计**永久池**：`total_credits` 与账号行的
    /// `credits`（永久池）同口径；每日池的合计由消费方按 `accounts[].dailyCredits` 自行求和。
    /// 相加出一个合并数就是发明一种不存在的度量（与 `LoomyUsagePage` 两池分列同一条口径）。
    ///
    /// `expired` / `zero_credits` 保持 0，**不是占位而是事实**：
    /// - `expired`（积分过期）：Loomy 的永久池**不过期**；每日池是**每日重置**（今日额度），
    ///   不是「一批积分到期作废」⇒ 没有这个状态；
    /// - `zero_credits`：它在 WB 侧是一个**互斥路由态**（`status = no_credits` 的账号数，
    ///   与 `available` 不重叠）。而 Loomy 的选号**根本不看余额**（`credits()` 恒 0、
    ///   余额为 0 的账号仍然 `available`）⇒ 汇报一个 >0 的计数会让
    ///   「可用 + 冷却 + 禁用 + 已排除 == total」这条不变量直接破。
    ///   零余额的真实读数在账号行的 `credits` / `dailyCredits` 里逐条看得到。
    pub fn summary(&self, now: i64) -> Value {
        // 排除是**互斥第六态**：必须从 `available` 里扣掉，否则「N 可用」与实际可选数不符
        //（与 WB / Trae / Qoder 同一条不变量：六态之和 == total）。
        let excluded = self.excluded_map();
        let mut available = 0usize;
        let mut cooling = 0usize;
        let mut disabled = 0usize;
        let mut excluded_count = 0usize;
        let mut total_credits = 0i64;
        for entry in self.entries.values() {
            // 余额合计口径与 WB 一致：**不按状态过滤**（被排除 / 禁用账号的余额也是余额）。
            total_credits += entry.credits.max(0);
            if excluded.contains_key(&entry.uid) {
                excluded_count += 1;
            } else if entry.auth_dead || !entry.has_access() {
                // 无凭据与凭据失效都属「需人工干预」⇒ 归 `disabled`（与 `accounts_view` 的
                // `status = disabled` 同一口径；两个出口必须说同一件事）。
                disabled += 1;
            } else if entry.cooldown_until > now {
                cooling += 1;
            } else {
                available += 1;
            }
        }
        json!({
            "product": "loomy",
            "total": self.entries.len(),
            "available": available,
            "cooling": cooling,
            "disabled": disabled,
            "expired": 0,
            "zero_credits": 0,
            "total_credits": total_credits,
            "excluded": excluded_count,
        })
    }

    /// ★ 账号视图列表（管理面）。
    ///
    /// ★ 形状与共用池卡 `GatewayPoolAccount` **逐字对齐**（`status` 五态 + `cooling` /
    /// `disabled` 两个布尔 + `credits`），另有 Loomy 自己的第二池 `dailyCredits`。
    /// 统一网关页与 Loomy 独立页读的是**同一个**类型。
    ///
    /// ⚠️ 曾经这里发 `state` / `hasCredential`，而前端读 `status` / `cooling` / `disabled` ——
    /// 于是**每一个账号都被渲染成「可用」**（`status` 缺失 ⇒ 归一化兜底成 `available`），
    /// 冷却中的、凭据失效的账号看起来都能用。这类「字段名对不上 ⇒ 静默显示默认值」
    /// 比直接报错危险得多。
    pub fn accounts_view(&self, now: i64) -> Vec<Value> {
        let excluded = self.excluded_map();
        self.entries
            .values()
            .map(|entry| {
                let cooling = entry.cooldown_until > now;
                let disabled = entry.auth_dead || !entry.has_access();
                let status = if excluded.contains_key(&entry.uid) {
                    "excluded"
                } else if disabled {
                    "disabled"
                } else if cooling {
                    "cooling"
                } else {
                    "available"
                };
                json!({
                    "uid": entry.uid,
                    "name": entry.name,
                    "status": status,
                    // ★ 两池**分列下发**（不合并）：`credits` = 永久池、`dailyCredits` = 每日池。
                    // 只发永久池会让「永久 0、今日池还有额度」的账号渲染成「0 积分」——
                    // 那会被读成「这个号空了」，而它今天完全可用。
                    //
                    // ⚠️ 与 WB / Trae / Qoder 的 `credits: null`（未刷新过 ⇒ 前端「—」）**不同**：
                    // Loomy 的账号库里 `credits` 只是「最近一次读数」且**没有读数的时刻字段** ⇒
                    // 0 既可能是真实读数、也可能是尚未刷过，两者查不出区别。因此照实发读数
                    // （与 Loomy 账号卡「`0` 是真实读数，恒显示」同一条口径），不发明一个
                    // 拿不到依据的「未知」—— 那是另一种造数。
                    "credits": entry.credits,
                    "dailyCredits": entry.daily_credits,
                    "cooling": cooling,
                    "cooldownUntil": (cooling || entry.cooldown_until > 0)
                        .then_some(entry.cooldown_until),
                    "cooldownReason": entry.cooldown_reason,
                    "disabled": disabled,
                    "lastError": entry.last_error,
                    "excludedNote": excluded.get(&entry.uid),
                    "hasCredential": entry.has_access(),
                })
            })
            .collect()
    }
}

/// Loomy 的选号策略（独立函数，别动 `PickPolicy` 默认值 —— 那是 WB 的行为基线）。
pub fn loomy_pick_policy() -> PickPolicy {
    PickPolicy {
        expiring_weight: 0.0,
        expiry_urgency_weight: 0.0,
        idle_weight_per_hour: 0.0,
        idle_weight_max: 0.0,
        min_pick_gap_ms: 0,
        model_cost_ttl_ms: 0,
        max_in_flight: 0,
        max_in_flight_global: 0,
        deterministic: true,
    }
}

/// 一个区域的账号池。
#[derive(Debug, Default)]
pub struct LoomyPool {
    entries: BTreeMap<String, LoomyPoolEntry>,
    pick_seq: u64,
    dirty: bool,
    region: Option<LoomyRegion>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一个池（不碰磁盘：直接塞 entries，绕过 `sync_for`）。
    fn pool_with(entries: Vec<LoomyPoolEntry>) -> LoomyPool {
        LoomyPool {
            entries: entries.into_iter().map(|e| (e.uid.clone(), e)).collect(),
            ..LoomyPool::default()
        }
    }

    fn entry(uid: &str, token: &str, auth_dead: bool, cooldown_until: i64) -> LoomyPoolEntry {
        let account = RawAccount {
            uid: uid.to_string(),
            access_token: token.to_string(),
            ..RawAccount::default()
        };
        let mut entry = LoomyPoolEntry::new(&account);
        entry.auth_dead = auth_dead;
        entry.cooldown_until = cooldown_until;
        entry
    }

    /// ★ 池摘要的键集合必须与**另外三条产品线逐字一致**。
    ///
    /// 统一网关的 `pool_object` 只做键值摊平、**不改名**，前端 `UnifiedGatewayPool`
    /// 也只认这套名字。Loomy 曾用 `usable` / `authDead` ⇒ 统一网关页渲染成
    /// 「**undefined 可用**」，冷却与禁用数也全是 `undefined`。
    ///
    /// 变异：把 `available` 改回 `usable` ⇒ 本条红。
    #[test]
    fn summary_keys_match_other_product_lines() {
        let now = 1_000_000;
        let pool = pool_with(vec![entry("a", "tok", false, 0)]);
        let summary = pool.summary(now);

        for key in [
            "total",
            "available",
            "cooling",
            "disabled",
            "expired",
            "zero_credits",
            "total_credits",
            "excluded",
        ] {
            assert!(
                summary.get(key).is_some(),
                "池摘要缺 `{key}` —— 统一网关页会把它渲染成 undefined。实际键：{:?}",
                summary.as_object().map(|o| o.keys().collect::<Vec<_>>())
            );
        }
        // 旧名字不得残留：两套名字并存 = 迟早有人读到错的那套。
        assert!(
            summary.get("usable").is_none(),
            "`usable` 已废弃，应为 `available`"
        );
        assert!(
            summary.get("authDead").is_none(),
            "`authDead` 已废弃，应为 `disabled`"
        );
    }

    /// 六态互斥且**和等于 total**（与 WB / Trae / Qoder 同一条不变量）。
    ///
    /// 变异：把冷却中的账号同时算进 `available` ⇒ 本条红。
    #[test]
    fn summary_states_are_mutually_exclusive() {
        let now = 1_000_000;
        let pool = pool_with(vec![
            entry("ok", "tok", false, 0),                 // available
            entry("cooling", "tok", false, now + 60_000), // cooling
            entry("dead", "tok", true, 0),                // disabled（凭据失效）
            entry("no-token", "", false, 0),              // disabled（无凭据）
        ]);
        let summary = pool.summary(now);
        let n = |k: &str| summary[k].as_u64().unwrap_or(0);

        assert_eq!(n("total"), 4);
        assert_eq!(n("available"), 1, "只有 ok 可用");
        assert_eq!(n("cooling"), 1);
        assert_eq!(n("disabled"), 2, "凭据失效 + 无凭据都需人工干预");
        assert_eq!(
            n("available")
                + n("cooling")
                + n("disabled")
                + n("expired")
                + n("zero_credits")
                + n("excluded"),
            n("total"),
            "六态之和必须等于 total —— 否则卡片会自相矛盾"
        );
    }

    /// ★ 账号行的形状必须与共用池卡 `GatewayPoolAccount` 对齐。
    ///
    /// 曾发 `state` / `hasCredential`，而前端读 `status` / `cooling` / `disabled` ⇒
    /// **每个账号都被渲染成「可用」**（缺失字段被归一化兜底成默认值）。
    ///
    /// 变异：把 `status` 改回 `state` ⇒ 本条红。
    #[test]
    fn accounts_view_matches_shared_pool_card() {
        let now = 1_000_000;
        let pool = pool_with(vec![
            entry("ok", "tok", false, 0),
            entry("cooling", "tok", false, now + 60_000),
            entry("dead", "tok", true, 0),
        ]);
        let rows = pool.accounts_view(now);

        for row in &rows {
            for key in ["uid", "name", "status", "cooling", "disabled", "credits"] {
                assert!(row.get(key).is_some(), "账号行缺 `{key}`：{row}");
            }
            assert!(
                row.get("state").is_none(),
                "`state` 已废弃，应为 `status`：{row}"
            );
        }

        let by_uid = |uid: &str| {
            rows.iter()
                .find(|row| row["uid"] == uid)
                .unwrap_or_else(|| panic!("缺账号 {uid}"))
        };
        assert_eq!(by_uid("ok")["status"], "available");
        assert_eq!(by_uid("cooling")["status"], "cooling");
        assert_eq!(by_uid("cooling")["cooling"], true);
        assert_eq!(by_uid("dead")["status"], "disabled");
        assert_eq!(by_uid("dead")["disabled"], true);
    }

    /// ★ 两池积分必须**逐账号下发**（统一网关与 Loomy 独立页的账号池卡都读这两个字段）。
    ///
    /// 回归：这俩字段曾经不存在 —— 池视图恒发 `credits: null`（那时确实「不建模积分」），
    /// 而账号库里早就有读数 ⇒ 统一网关的账号池卡对 loomy-cn **整列显示「—」**。
    ///
    /// 变异：把 `credits` 改回 `Value::Null`（或只发 `credits` 不发 `dailyCredits`）⇒ 本条红。
    #[test]
    fn accounts_view_carries_both_credit_pools() {
        let now = 1_000_000;
        let mut funded = entry("funded", "tok", false, 0);
        funded.credits = 1_200;
        funded.daily_credits = 30;
        /* 永久池空、今日池还有额度的账号：只发永久池会被渲染成「0 积分」⇒
        读起来像「这个号空了」，而它今天完全可用。 */
        let mut daily_only = entry("daily-only", "tok", false, 0);
        daily_only.credits = 0;
        daily_only.daily_credits = 500;

        let rows = pool_with(vec![funded, daily_only]).accounts_view(now);
        let by_uid = |uid: &str| {
            rows.iter()
                .find(|row| row["uid"] == uid)
                .unwrap_or_else(|| panic!("缺账号 {uid}"))
                .clone()
        };

        assert_eq!(by_uid("funded")["credits"], 1_200);
        assert_eq!(by_uid("funded")["dailyCredits"], 30);
        assert_eq!(
            by_uid("daily-only")["dailyCredits"],
            500,
            "每日池必须单独下发 —— 只给 `credits` 会让它显示成 0 积分"
        );
        assert!(
            !by_uid("funded")["credits"].is_null(),
            "`credits` 不得再是 null（那正是本轮报障：整列显示「—」）"
        );
    }

    /// ★ 摘要里的总积分 = **永久池**之和；`expired` / `zero_credits` 仍是 0。
    ///
    /// 后两项不是占位而是事实：Loomy 永久池不过期；而 `zero_credits` 在 WB 侧是一个
    /// **互斥路由态**，Loomy 选号根本不看余额（`credits()` 恒 0）⇒ 报 >0 会直接破
    /// 「五态 + 排除 == total」这条不变量。
    ///
    /// 变异：把每日池也加进 `total_credits` ⇒ 本条红（两池不可通约，不得相加）。
    #[test]
    fn summary_totals_only_the_permanent_pool() {
        let now = 1_000_000;
        let mut a = entry("a", "tok", false, 0);
        a.credits = 100;
        a.daily_credits = 7;
        let mut b = entry("b", "tok", false, 0);
        b.credits = 25;
        b.daily_credits = 0;
        // 被排除的账号余额也是余额（与 WB 的合计口径一致：不按状态过滤）。
        let mut c = entry("c", "tok", false, 0);
        c.credits = 5;

        let pool = pool_with(vec![a, b, c]);
        let summary = pool.summary(now);

        assert_eq!(
            summary["total_credits"], 130,
            "总积分只合计永久池（100 + 25 + 5），不得把每日池加进来"
        );
        assert_eq!(summary["expired"], 0, "Loomy 永久池不过期（每日池是重置）");
        assert_eq!(
            summary["zero_credits"], 0,
            "该维度对 Loomy 不适用：余额不参与路由，报 >0 会破互斥不变量"
        );
        let n = |k: &str| summary[k].as_u64().unwrap_or(0);
        assert_eq!(
            n("available") + n("cooling") + n("disabled") + n("expired") + n("excluded"),
            n("total"),
            "互斥态（五态 + 排除）必须等于 total"
        );
    }

    /// ★★ 展示用的余额**不得**泄入选号（两者刻意分开）。
    ///
    /// 选号里的 `credits()` 会变成 `credits / max_credits * 10` 权重项与同权重决胜键
    /// （`pick.rs`）⇒ 一旦回报真实余额，就相当于**顺手改了选号优先级**（总先挑余额最多的号）。
    /// 本仓的 Loomy 策略是按「闲置 / 冷却」轮转。
    ///
    /// 变异：把 `PoolEntryLike::credits` 改成 `self.credits` ⇒ 本条红。
    #[test]
    fn display_credits_do_not_leak_into_pick() {
        let mut funded = entry("funded", "tok", false, 0);
        funded.credits = 999_999;

        assert_eq!(
            PoolEntryLike::credits(&funded),
            0,
            "选号用的积分必须恒 0（本字段只供展示）"
        );
        assert_eq!(funded.credits, 999_999, "展示字段本身要保留真实读数");
        assert_eq!(
            PoolEntryLike::expiring_credits(&funded, 1_000_000),
            0,
            "Loomy 没有「快过期积分」这个量"
        );
    }
}
