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

    /// 选号用的余额口径 = **每日池**（当天作废的那部分）。
    ///
    /// ## 为什么是每日池，而不是「永久 + 每日」或永久池
    ///
    /// 择优要回答的是「先消耗谁的额度」，而 Loomy 两池的**过期语义不同**：
    /// 每日池**当天作废**（不用就白丢），永久池不清零（留着以后用没损失）。
    /// 而内核的积分项正好是「余额最多者优先」（`credits / max_credits * 10`）⇒
    /// 把每日池放进去，得到的正是**先烧掉今天会作废的那部分**。
    ///
    /// - 相加（永久 + 每日）：会把「永久很多、今日池已空」的号排在前面 —— 恰好把
    ///   「今天不用就作废」的那部分留着，与目标相反；
    /// - 只取永久池：与「优先消耗每日池」完全相反。
    ///
    /// `max_credits == 0`（全池今日池都空）时内核会**跳过**这一项 ⇒ 退回原来的
    /// EMA / uid 次序，**与本次改动前逐字一致**：今天都没额度不会导致乱换号。
    ///
    /// ## 鲜度的诚实说明
    ///
    /// 余额来自**最近一次读数**（排程采样 2 帧/日 或手动刷新），本模块不做逐次请求的
    /// 扣减 ⇒ 排序的鲜度 = 上一次读数的时刻。号真被用空时靠**上游报错 + 冷却换号**
    /// （既有机制）自愈。
    ///
    /// 变异：把这里改回 `0` ⇒ `pick_credit_axis_is_the_daily_pool` /
    /// `pick_prefers_the_account_with_a_daily_pool` /
    /// `pick_orders_by_daily_pool_and_falls_back_when_all_empty` 三条一起红。
    fn credits(&self) -> i64 {
        self.daily_credits.max(0)
    }

    /// Loomy 没有**独立**的「快过期积分绝对值」概念：会作废的那部分（每日池）
    /// 已经由 [`Self::credits`] 承担了。
    ///
    /// 若在这里也报每日池，`expiring / credits` 会恒等于 1（所有有每日池的号一样）——
    /// 一个常数项不改变相对次序，却会让同一个信号被两个因子重复计权。策略里的
    /// `expiring_weight` 因此保持 0.0（见 `loomy_pick_policy`）。
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
    /// ★ 形状与共用池卡 `GatewayPoolAccount` **逐字对齐**（`status` 六态 + `cooling` /
    /// `disabled` / `excluded` 三个布尔 + `credits`），另有 Loomy 自己的第二池 `dailyCredits`。
    /// 统一网关页与 Loomy 独立页读的是**同一个**类型。
    ///
    /// ⚠️ 曾经这里发 `state` / `hasCredential`，而前端读 `status` / `cooling` / `disabled` ——
    /// 于是**每一个账号都被渲染成「可用」**（`status` 缺失 ⇒ 归一化兜底成 `available`），
    /// 冷却中的、凭据失效的账号看起来都能用。这类「字段名对不上 ⇒ 静默显示默认值」
    /// 比直接报错危险得多。
    pub fn accounts_view(&self, now: i64) -> Vec<Value> {
        let excluded = self.excluded_map();
        self.accounts_view_with(now, &excluded)
    }

    /// 与 [Self::accounts_view] 同一实现，排除表由调用方注入。
    ///
    /// 生产路径的排除表来自磁盘上的 gateway_pool_exclusions.json；单测不该依赖
    /// 用户机器上的那一份（并行跑时还可能互相踩），因此把「读哪份表」抽成参数。
    fn accounts_view_with(&self, now: i64, excluded: &BTreeMap<String, String>) -> Vec<Value> {
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
                    // ★ 用户显式排除（硬约束：选号 / 兜底 / 粘性 / 指定全跳过）。
                    //   必须**逐行下发布尔值**，不能只靠 status=excluded：前端池卡用白名单
                    //   重建账号对象，漏了这个字段 ⇒ 按钮文案恒为「排除」、点击恒传 true
                    //   ⇒ 已排除的账号再也取消不掉（WB / Trae / Qoder 在 2026-10-22 报障过
                    //   一次，Loomy 分支落地时又漏了一遍）。
                    "excluded": excluded.contains_key(&entry.uid),
                    "excludedNote": excluded.get(&entry.uid),
                    "hasCredential": entry.has_access(),
                })
            })
            .collect()
    }
}

/// **余额下限**：每日池低于这个数就换号（选号口径 = 每日池，见 `PoolEntryLike::credits`）。
///
/// ## 为什么是 100
///
/// 它是一个「还能跑几个请求」的量级余量，不是精确阈值（上游按用量扣减，本地不知道
/// 单次消耗）。取它的目的：别把一个只剩几十积分的号排在前面 —— 那会在大请求上
/// 刚发出去就撞额度不足。
///
/// ⚠️ 它不是硬过滤：**全池都低于 100 时照旧用它们**（有额度就不拒服务），
/// 见 [`crate::pool::pick::PickPolicy::min_credits`]。
pub const LOOMY_MIN_DAILY_CREDITS: i64 = 100;

/// Loomy 的选号策略（独立函数，别动 `PickPolicy` 默认值 —— 那是 WB 的行为基线）。
///
/// ## 为什么四个权重都是 0，而「优先消耗每日池」仍然生效
///
/// Loomy 的择优**只靠积分项**（`credits / max_credits * 10`）：
///
/// - `expiring_weight`：本产品线的「会作废的余额」就是每日池，已由
///   [`LoomyPoolEntry`] 的 `credits()` 承担 ⇒ 再用 `expiring / credits` 只会得到恒为 1 的常数；
/// - `expiry_urgency_weight`：那是「这批额度在**某个时刻**到期」（Trae）的语义。
///   Loomy 的每日池**全池在同一个时刻作废**（次日零点）⇒ 所有号一样，对**排座次**没信息量；
/// - 闲置补偿 / 防惊群：保持 **0**（本次不动）—— 它们会让「先把每日池烧完」变成「大概 60% 先烧」，
///   而不是确定性的先烧。真要防「同一个号被打满」是另一个决定（目前靠上游报错 + 冷却换号自愈）。
///
/// ⇒ 效果：有每日池的号排在没每日池的号前面，且每日池越多越靠前（`deterministic = true`
/// 下即严格择优）；**且每日池低于 [`LOOMY_MIN_DAILY_CREDITS`] 的号在「有得选」时被剔除**；
/// 全池今日池都空（或全都不足下限）时该权重项被跳过 / 不过滤，次序回到改动前。
pub fn loomy_pick_policy() -> PickPolicy {
    PickPolicy {
        // 「每日池 < 100 就换号」（全池都不足时照旧用，见该常量的注释）。
        min_credits: LOOMY_MIN_DAILY_CREDITS,
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
            for key in [
                "uid", "name", "status", "cooling", "disabled", "excluded", "credits",
            ] {
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

    /// ★ 排除必须**逐行下发布尔值**，`status = "excluded"` 不够。
    ///
    /// 前端池卡（`normalizeLoomyGatewayStatus` / `normalizeGatewayPoolAccounts`）用白名单
    /// 重建账号对象：漏了 `excluded` ⇒ 按钮文案恒为「排除」、`onExclude(uid, !undefined)`
    /// 恒传 `true` ⇒ 账号被排除后再也取消不掉，删号重加也一样（排除记录按 uid 保留）。
    ///
    /// 变异：删掉 `accounts_view` 里的 `"excluded"` 一行 ⇒ 本条红。
    #[test]
    fn accounts_view_carries_excluded_flag_per_row() {
        let now = 1_000_000;
        let pool = pool_with(vec![
            entry("blocked", "tok", false, 0),
            entry("ok", "tok", false, 0),
        ]);
        let excluded = BTreeMap::from([("blocked".to_string(), "留给客户端".to_string())]);

        let rows = pool.accounts_view_with(now, &excluded);
        let by_uid = |uid: &str| {
            rows.iter()
                .find(|row| row["uid"] == uid)
                .unwrap_or_else(|| panic!("缺账号 {uid}"))
                .clone()
        };

        assert_eq!(by_uid("blocked")["status"], "excluded");
        assert_eq!(
            by_uid("blocked")["excluded"],
            true,
            "已排除的账号必须带 excluded=true —— 否则前端永远只能再点一次「排除」"
        );
        assert_eq!(by_uid("blocked")["excludedNote"], "留给客户端");
        assert_eq!(
            by_uid("ok")["excluded"],
            false,
            "未排除的账号要明确下发 false"
        );
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

    /// ★★ 选号的余额口径 = **每日池**（不是永久池、也不是两池相加）。
    ///
    /// 「先烧掉今天会作废的那部分」全靠这一条：内核的积分项是「余额最多者优先」，
    /// 喂给它每日池才能得到「每日池最多的号先用完」。
    ///
    /// 变异：改成 `self.credits`（永久池）或 `self.credits + self.daily_credits` ⇒ 本条红。
    #[test]
    fn pick_credit_axis_is_the_daily_pool() {
        let mut funded = entry("funded", "tok", false, 0);
        funded.credits = 9_999;
        funded.daily_credits = 30;

        assert_eq!(
            PoolEntryLike::credits(&funded),
            30,
            "选号口径必须是每日池；取永久池（9999）或两池相加（10029）都是错的方向"
        );
        assert_eq!(
            PoolEntryLike::expiring_credits(&funded, 1_000_000),
            0,
            "「会作废的余额」已由 credits() 承担 ⇒ 该因子不再重复计权"
        );

        // 今日池为 0 的号：选号口径也是 0（不得被永久池抬高）。
        let mut permanent_only = entry("permanent-only", "tok", false, 0);
        permanent_only.credits = 9_999;
        permanent_only.daily_credits = 0;
        assert_eq!(PoolEntryLike::credits(&permanent_only), 0);
    }

    /// ★★ 自动择优必须**先烧每日池**：今日池还有额度的号胜出，哪怕它永久池为空、
    /// uid 次序上排在后面（旧行为下按 uid 升序会选中 `a-permanent-only`）。
    ///
    /// 变异：把 `PoolEntryLike::credits` 改回 `0` ⇒ 两者权重相同 ⇒ 本条红。
    #[test]
    fn pick_prefers_the_account_with_a_daily_pool() {
        let mut permanent_only = entry("a-permanent-only", "tok", false, 0);
        permanent_only.credits = 9_999;
        permanent_only.daily_credits = 0;
        let mut daily_rich = entry("z-daily-rich", "tok", false, 0);
        daily_rich.credits = 0;
        daily_rich.daily_credits = 500;

        let mut pool = pool_with(vec![permanent_only, daily_rich]);
        let picked = pool.pick("m", &HashSet::new(), None);
        assert_eq!(
            picked.as_deref(),
            Some("z-daily-rich"),
            "应当先消耗每日池（它在 uid 次序上排在后面，必须靠权重胜出）"
        );
    }

    /// ★★ 「每日池 < 100 就换号」——低于下限的号在「有得选」时被**硬保证**剔除。
    ///
    /// ## 为什么这条护栏必须用「成功率 EMA 会翻盘」的场景来钉
    ///
    /// 权重本身已经按每日池**单调**排序（`credits / max_credits × 10`），所以在「健康的那个
    /// 每日池明显更多」的场景下，**阈值开不开结果都一样** ⇒ 那样的用例是**假护栏**
    /// （2026-10-26 实测：把 `min_credits` 改成 0，那种写法仍然绿）。
    ///
    /// 阈值真正的作用是**硬保证**：当低于下限的号靠别的因子（成功率 EMA 最多差 3.0，
    /// 而两个都 ≥ 下限的号之间的比值差可能不到 0.1）本会胜出时，它仍然不许被选中。
    ///
    /// 变异：把 `loomy_pick_policy().min_credits` 改成 0 ⇒ 本条红（`z-below` 靠 EMA 胜出）。
    #[test]
    fn pick_skips_accounts_below_the_daily_floor() {
        let mut healthy = entry("a-healthy", "tok", false, 0);
        healthy.daily_credits = LOOMY_MIN_DAILY_CREDITS; // 刚好达下限
        healthy.success_ema = 0.0;
        healthy.error_ema = 10.0; // 成功率 0 ⇒ EMA 项 +0
        let mut below = entry("z-below-floor", "tok", false, 0);
        below.daily_credits = LOOMY_MIN_DAILY_CREDITS - 1;
        below.success_ema = 10.0;
        below.error_ema = 0.0; // 成功率 1 ⇒ EMA 项 +3

        let mut pool = pool_with(vec![healthy, below]);
        assert_eq!(
            pool.pick("m", &HashSet::new(), None).as_deref(),
            Some("a-healthy"),
            "每日池不足下限的号即便 EMA 更好，也不该在有得选时被用"
        );

        // 全都低于下限 ⇒ 照旧用（有额度就不拒服务），且仍按每日池多者优先。
        let mut low_a = entry("a-low", "tok", false, 0);
        low_a.daily_credits = 90;
        let mut low_z = entry("z-low", "tok", false, 0);
        low_z.daily_credits = 80;
        let mut all_low = pool_with(vec![low_a, low_z]);
        assert_eq!(
            all_low.pick("m", &HashSet::new(), None).as_deref(),
            Some("a-low"),
            "全池都不足下限时必须照旧选（不得 503），且仍按每日池多者优先"
        );
    }

    /// ★★ 随机模式（Loomy 的下拉里真有这一项）也必须守住下限。
    ///
    /// 权重项对随机模式**完全无效**（它是均匀抽签）⇒ 下限是唯一的屏障：没有它，
    /// 一个只剩几积分的号会被等概率抽中。
    ///
    /// 变异：把 `min_credits` 改成 0 ⇒ 候选变 2 个，多次抽签必然出现 `z-below` ⇒ 本条红。
    #[test]
    fn random_mode_never_draws_a_below_floor_account_when_a_better_one_exists() {
        let mut healthy = entry("a-healthy", "tok", false, 0);
        healthy.daily_credits = 500;
        let mut below = entry("z-below-floor", "tok", false, 0);
        below.daily_credits = 1;
        let mut pool = pool_with(vec![healthy, below]);

        for _ in 0..24 {
            let picked = pool.pick("m", &HashSet::new(), Some("__random__"));
            assert_eq!(
                picked.as_deref(),
                Some("a-healthy"),
                "随机模式也不得抽中每日池不足下限的号（那时还有更好的选择）"
            );
        }
    }

    /// ★ 每日池**更多**的号优先（不是「只要有就行」），且全池今日池都空时
    /// 退回改动前的 uid 次序（不因「今天没额度」乱换号）。
    #[test]
    fn pick_orders_by_daily_pool_and_falls_back_when_all_empty() {
        let mut small = entry("a-small-daily", "tok", false, 0);
        small.daily_credits = 10;
        let mut big = entry("z-big-daily", "tok", false, 0);
        big.daily_credits = 800;

        let mut pool = pool_with(vec![small, big]);
        assert_eq!(
            pool.pick("m", &HashSet::new(), None).as_deref(),
            Some("z-big-daily"),
            "每日池多者优先（它 uid 更大，故必须靠权重胜出）"
        );

        // 两号的今日池都是 0 ⇒ `max_credits == 0` ⇒ 积分项被跳过 ⇒ uid 升序。
        let mut a = entry("a-empty", "tok", false, 0);
        a.credits = 5_000;
        let mut z = entry("z-empty", "tok", false, 0);
        z.credits = 1;
        let mut empty_pool = pool_with(vec![a, z]);
        assert_eq!(
            empty_pool.pick("m", &HashSet::new(), None).as_deref(),
            Some("a-empty"),
            "今日池都为 0 时必须退回 uid 次序（永久池不参与排座次）"
        );
    }
}
