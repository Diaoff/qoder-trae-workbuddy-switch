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
                entry.auth_dead =
                    previous.auth_dead && entry.access_token == previous.access_token;
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
                entry.cooldown_reason = Some("登录已失效（Loomy 无续期端点，需重新登录）".to_string());
            }
            LoomyErrKind::Transient => {
                entry.cooldown_until = now + 10_000;
                entry.cooldown_reason = Some("上游暂时不可用".to_string());
            }
            LoomyErrKind::ClientError => {}
        }
    }

    /// 池摘要（统一网关管理面）。
    pub fn summary(&self, now: i64) -> Value {
        let total = self.entries.len();
        let usable = self
            .entries
            .values()
            .filter(|entry| entry.healthy_for_request(now, ""))
            .count();
        json!({
            "product": "loomy",
            "total": total,
            "usable": usable,
            "cooling": self.entries.values().filter(|e| e.cooldown_until > now).count(),
            "authDead": self.entries.values().filter(|e| e.auth_dead).count(),
        })
    }

    /// 账号视图列表（管理面）。
    pub fn accounts_view(&self, now: i64) -> Vec<Value> {
        let excluded = self.excluded_map();
        self.entries
            .values()
            .map(|entry| {
                json!({
                    "uid": entry.uid,
                    "name": entry.name,
                    "hasCredential": entry.has_access(),
                    "state": if entry.auth_dead {
                        "auth_dead"
                    } else if entry.cooldown_until > now {
                        "cooling"
                    } else if !entry.has_access() {
                        "no_credential"
                    } else {
                        "usable"
                    },
                    "cooldownReason": entry.cooldown_reason,
                    "lastError": entry.last_error,
                    "excludedNote": excluded.get(&entry.uid),
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

