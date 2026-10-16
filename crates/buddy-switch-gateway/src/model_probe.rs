//! 账号池 × 模型 **限流探测**（一次性作业）—— **按作用域（`Scope`）参数化**。
//!
//! ## 作用域参数化（2026-10 改造）
//!
//! 作业与命令的入参从 `region: String` 升级为 `scope: String`（`wb-cn` / `qoder-cn` …，
//! 由 core `product::Scope::parse` **fail-closed** 解析）。探测循环与平台无关：
//! `start` / `run` 只跟 [`ProbeTarget`] 打交道 —— 一个作用域 = 一套「账号轴 / 模型轴 /
//! 判据 / 发请求 / 回填」。本期实现 WB / Qoder / **Trae**（Trae 采 T1：**只探测、不回填**，
//! 见 [`ProbeTarget::adopt`] 的 Trae 分支）。
//!
//! ⚠️ 三个平台的 **state 类型不同**（`GatewayState` / `QoderGatewayState` /
//! `TraeGatewayState`），故 [`ProbeTarget`] 是 **enum**（每个变体自己持有对应 state），
//! **不是**「方法收 `&GatewayState`」的 trait 形状。
//!
//! ## 它解决什么
//!
//! 「这个模型在我的账号里是不是被限流了？」—— 池的 `model_cooldowns` 只回答
//! 「**经网关的真实流量**打过、且被打成冷却的」，客户端台账只回答「**日志里出现过**的」。
//! 两者对「**没跑过的模型**」都恒为「未知」。
//!
//! 本模块补的就是这一块：对「账号 × 模型」矩阵**逐格发一次最小请求**，把「未知」变成实判。
//!
//! ## 四条硬约束（改动时不得破坏）
//!
//! 1. **不复用会换号的出口** —— `routes::relay::relay` 自带 `max_rotate` 轮换，用它会
//!    把结果**归因到别的账号**上，矩阵立刻是假数据。这里单账号、单次、**零重试**，
//!    直接调 `UpstreamClient::chat_stream_with`。
//! 2. **探测本身不写池治理状态** —— 唯一写动作是收尾时的**显式回填**
//!    （`Pool::adopt_probe_result`，见那里的文档：只置/清冷却，不推进退避计数）。
//! 3. **判据零新造** —— 一律复用 [`crate::pool::classify_event`]（全仓唯一限流判据）；
//!    本模块只做「事件 → 单元格状态」的**纯映射**（[`probe_state_of`]，可单测 + 变异）。
//! 4. **不发空 `Bearer`** —— 缺 `access_token` 的账号一律 `skipped`（见 [`probe_cell`]）。
//!
//! ## 为什么是「作业 + 轮询」而不是同步返回
//!
//! 4 账号 × 12 模型 = 48 格，每格一次真实上游往返（秒级）⇒ 整轮**分钟级**。
//! 同步阻塞 IPC 会卡住界面。形态照抄仓内既有的 `switch_progress`
//! （进程内作业状态 + 轮询），**不引入 SSE**（本仓前端零 `EventSource`）。
//!
//! ## 并发与超时（安全审计 2026-10-16 修正）
//!
//! - **作业位是原子抢占的**（[`busy_flag`] 的 `compare_exchange`）：`start` 里
//!   「检查 Running → 写入作业」横跨多个 `await`，仅靠锁内检查是 **TOCTOU**（并发两次都过检、
//!   后者覆盖前者 ⇒ 双作业 + 取消位被复位）。
//! - **单格有硬超时**（[`PROBE_CELL_TIMEOUT_SECS`]）：宿主客户端的 600s 总超时对一格来说太长，
//!   一格挂住会把整轮拖死（取消只在格间检查）⇒ 一格最多 30s。
//!
//! ## 一期已知的简化（诚实记录）
//!
//! - **账号串行**（方案 §3.3 写的是「跨账号并发 2」）：串行更安全（不会自己把账号推成 429），
//!   代价是整轮更慢。并发留到有真实耗时数据之后再定。
//! - **模型轴**：调用方给了 `models` 就用它；没给则回落**该作用域的目录** ——
//!   WB 用 `catalogs.current(region)`（纯内存），Qoder 用网关的**内存清单缓存**
//!   （[`QoderGatewayState::cached_catalog_models`]），**都不额外联网**。
//!   而不是「逐账号各拉一次 `/models`」——后者要 N 次额外上游往返，收益不抵成本。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};

use buddy_switch_core::modules::account;
use buddy_switch_core::modules::product::{
    scope_of_qoder_region, scope_of_trae_region, scope_of_wb_region, Product, Scope,
};
use buddy_switch_core::modules::qoder::region::QoderRegion;
use buddy_switch_core::modules::qoder::{api as qoder_api, cosy, PATH_CHAT_SSE};
use buddy_switch_core::modules::region::Region;
use buddy_switch_core::modules::trae::region::TraeRegion;
use buddy_switch_core::modules::trae::variant::TraeVariant;
use buddy_switch_core::modules::upstream::UpstreamChatResult;

use crate::outbound::{self, OutboundMeta};
use crate::pool::{classify_event, ProbeOutcome, RealmTag, UpstreamEvent};
use crate::qoder::payload;
use crate::qoder::pool::{classify_http, QoderErrKind};
use crate::qoder::routes::chat_once_for_account;
use crate::qoder::{QoderGateway, QoderGatewayState};
use crate::session_headers::{self, ConversationContext};
use crate::state::GatewayState;
use crate::timeutil;
use crate::trae::payload as trae_payload;
use crate::trae::pool::{
    classify_http as trae_classify_http, classify_solo as trae_classify_solo, TraeErrKind, TraePool,
};
use crate::trae::routes::chat_once_for_account as trae_chat_once_for_account;
use crate::trae::sse as trae_sse;
use crate::trae::{TraeGateway, TraeGatewayState};
use crate::{GatewayProduct, WbGateway};

/// 单次探测的请求体文本：极短、中性（避免触发上游内容审核）。
pub const PROBE_TEXT: &str = "ping";

/// 格数硬上限（超出即 `truncated`，不静默丢掉）。
pub const MAX_CELLS: usize = 400;

/// 单条轴的长度上限（`uids` / `models` 来自 IPC，不能无限长）。
pub const MAX_AXIS: usize = 200;

/// 格间最小间隔：给上游留喘息，避免「探测自己把好号打成 429」。
pub const PROBE_GAP_MS: u64 = 150;

/// 单格硬超时（秒）。见模块文档「并发与超时」。
pub const PROBE_CELL_TIMEOUT_SECS: u64 = 30;

/// 单格的输出状态（**序列化名与前端/原型逐字一致**）。
///
/// ★★ 只有前三档算**限流**。`NoCredits`（402）与 `AuthDead` 必须**分列** ——
/// 把「没积分」报成「限流」会让用户去傻等解锁（`client_limits` 已踩过这个坑）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeState {
    /// 上游 2xx：可用。
    Ok,
    /// 模型级限流（`6004` / `IsModelRateLimit`）。
    RateLimitedModel,
    /// 账号级限流（429 且无模型标记）。
    RateLimitedAccount,
    /// 模型被上游封禁（`11102`）。
    ModelBlocked,
    /// 402 / 余额不足文案。
    NoCredits,
    /// 会话失效（`12153` / 401）。
    AuthDead,
    /// 404：该账号看不到/不支持这个模型。
    ModelUnavailable,
    /// 5xx / 其它 4xx。
    UpstreamError,
    /// 传输层失败（`status == 0`）或单格超时。
    TransportError,
    /// 没探（无凭据 / 信封 / 账号不在库）。
    Skipped,
}

/// ★ **唯一映射点**：上游事件 → 单元格状态。纯函数，单测 + 变异覆盖。
pub fn probe_state_of(event: &UpstreamEvent) -> ProbeState {
    match event {
        UpstreamEvent::SoftRate { model_scoped: true, .. } => ProbeState::RateLimitedModel,
        UpstreamEvent::SoftRate { model_scoped: false, .. } => ProbeState::RateLimitedAccount,
        UpstreamEvent::ModelBlocked => ProbeState::ModelBlocked,
        UpstreamEvent::HardCredit => ProbeState::NoCredits,
        UpstreamEvent::SessionDead => ProbeState::AuthDead,
        UpstreamEvent::NotFound => ProbeState::ModelUnavailable,
        UpstreamEvent::Server | UpstreamEvent::Client => ProbeState::UpstreamError,
    }
}

/// 单元格状态 → **池采纳**语义（回填用）。`Skipped` / 其它档一律 `Other`（**不动冷却**）。
pub fn probe_outcome_of(state: ProbeState, reset_at_ms: Option<i64>) -> ProbeOutcome {
    match state {
        ProbeState::Ok => ProbeOutcome::Available,
        ProbeState::RateLimitedModel => ProbeOutcome::ModelRateLimited { reset_at_ms },
        ProbeState::ModelBlocked => ProbeOutcome::ModelBlocked,
        ProbeState::RateLimitedAccount => ProbeOutcome::AccountRateLimited { reset_at_ms },
        _ => ProbeOutcome::Other,
    }
}

/// 作业阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbePhase {
    /// 从未跑过（或已被换作用域清掉）。
    Idle,
    /// 进行中。
    Running,
    /// 跑完（可能被截断）。
    Done,
    /// 用户取消；`cells` 里是**部分结果**。
    Cancelled,
}

/// 一格结果。
#[derive(Debug, Clone, Serialize)]
pub struct ProbeCell {
    pub uid: String,
    pub nickname: String,
    pub model: String,
    pub state: ProbeState,
    pub http_status: u16,
    pub reset_at_ms: Option<i64>,
    /// 上游原文（截断 400 字符），排查用。
    ///
    /// ⚠️ 前端必须**纯文本**渲染（它是不可信输入：控制字符 / ANSI / 标记都可能出现）。
    pub reason: String,
    pub latency_ms: i64,
}

/// 回填的**逐条**明细（每项 = 一格真的被写了的冷却）。
///
/// ## 为什么需要它
///
/// 完成条只写「已回填 3 条」是**不可核对**的 —— 方案 §5.3「回填反馈」明确要求
/// 「可展开看明细」：用户得能看到「**哪个账号的哪个模型**被置了 / 被清了，到什么时候」。
///
/// ⚠️ 明细**必须由回填侧记录**，不能让前端拿 `cells` 反推：反推的结果与
/// `adopted_added` / `adopted_cleared` 计数可能不一致（例如该模型早已冷却到更晚的时刻），
/// 那就成了**编数据**。
#[derive(Debug, Clone, Serialize)]
pub struct ProbeAdoptRecord {
    pub uid: String,
    pub nickname: String,
    pub model: String,
    /// `"added"` = 置了冷却；`"cleared"` = 清掉了冷却。
    pub action: String,
    /// 回填**之后**该模型的冷却截止（`"cleared"` 恒为 `0`）。
    pub until_ms: i64,
}

/// 一次探测作业。
#[derive(Debug, Clone, Serialize)]
pub struct ProbeJob {
    /// 作用域字面量（`Scope::as_str()` 的稳定值，**直接进 wire**）：`"wb-cn"` … `"qoder-global"`。
    pub scope: String,
    pub phase: ProbePhase,
    pub started_at_ms: i64,
    pub finished_at_ms: Option<i64>,
    /// 总格数（`plan.len()`）。
    pub total: usize,
    /// 已探格数。
    pub done: usize,
    /// 正在探的「账号 · 模型」。
    pub current: String,
    /// 是否因 [`MAX_CELLS`] 被截断。
    pub truncated: bool,
    pub adopt: bool,
    /// 回填：置了几条模型冷却。
    pub adopted_added: usize,
    /// 回填：清了几条（`ok` 的格）。
    pub adopted_cleared: usize,
    /// 回填**逐条明细**（唯一来源；前端展开看的就是它）。
    pub adopted: Vec<ProbeAdoptRecord>,
    pub cells: Vec<ProbeCell>,
    /// 待探清单（**不序列化**，只服务本轮循环）。
    #[serde(skip)]
    pub plan: Vec<(String, String)>,
}

fn job_slot() -> &'static Mutex<Option<ProbeJob>> {
    static SLOT: OnceLock<Mutex<Option<ProbeJob>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// 取消位（进程级；逐格检查）。
fn cancel_flag() -> &'static AtomicBool {
    static FLAG: AtomicBool = AtomicBool::new(false);
    &FLAG
}

/// **作业位**（进程级）：`start` 用它原子抢占，杜绝 TOCTOU。
///
/// ⚠️ 抢到之后由 [`run`] 的 **RAII 归位器**释放（正常结束 / 取消结束 / 早退 / **panic 展开**
/// 四种退出都覆盖）；[`cancel`] 另有一条**兜底自愈**（没有真在跑的作业时顺手释放）。
/// `start` 成功但调用方**没 spawn `run`** 会让作业位一直占着（下一次 `start` 报
/// 「已有探测在进行中」）—— 此时用户可调一次 `cancel` 自救，不必重启进程。
fn busy_flag() -> &'static AtomicBool {
    static FLAG: AtomicBool = AtomicBool::new(false);
    &FLAG
}

/// 读作业快照（`{phase:"idle"}` 表示从未跑过）。
pub fn status() -> Value {
    let guard = job_slot().lock().unwrap();
    match guard.as_ref() {
        Some(job) => serde_json::to_value(job).unwrap_or(Value::Null),
        None => json!({ "phase": "idle" }),
    }
}

/// 请求取消（逐格检查；已探到的格**保留**）。
///
/// ★ **兜底自愈**：如果没有真在跑的作业（作业槽为空 / `phase != Running`），顺手释放作业位。
/// 覆盖「`run` panic / 宿主没 spawn `run` ⇒ 作业位永久占着」这一死角 —— 让用户能用「取消」
/// 自救，而不是只能重启进程。
///
/// ⚠️ **作业真在跑时绝不能释放**：否则第二次 `start` 会与第一个作业同时往同一个作业槽写。
pub fn cancel() -> Value {
    cancel_flag().store(true, Ordering::SeqCst);
    let running = job_slot()
        .lock()
        .unwrap()
        .as_ref()
        .map(|job| job.phase == ProbePhase::Running)
        .unwrap_or(false);
    if !running {
        busy_flag().store(false, Ordering::SeqCst);
    }
    json!({ "ok": true })
}

/// 测试专用：复位作业槽 / 取消位 / 作业位。**只在测试构建里存在**（别在生产误清在跑作业）。
#[cfg(test)]
pub fn reset_for_test() {
    *job_slot().lock().unwrap() = None;
    cancel_flag().store(false, Ordering::SeqCst);
    busy_flag().store(false, Ordering::SeqCst);
}

/// 测试专用：直接放一份作业进去（用于断言 wire 形状）。
#[cfg(test)]
pub fn set_job_for_test(job: ProbeJob) {
    *job_slot().lock().unwrap() = Some(job);
}

#[cfg(test)]
fn test_cell(uid: &str, model: &str, state: ProbeState) -> ProbeCell {
    ProbeCell {
        uid: uid.to_string(),
        nickname: format!("n-{uid}"),
        model: model.to_string(),
        state,
        http_status: 200,
        reset_at_ms: None,
        reason: String::new(),
        latency_ms: 1,
    }
}

fn realm_of(region: Region) -> RealmTag {
    match region {
        Region::Cn => RealmTag::Cn,
        Region::Global => RealmTag::Global,
    }
}

// ---------------------------------------------------------------------------
// 探测目标（平台无关的探测循环的抽象）
// ---------------------------------------------------------------------------

/// ★ **唯一映射点（Qoder）**：Qoder 上游 HTTP 状态 → 单元格状态。纯函数，单测 + 变异覆盖。
///
/// ## 为什么不能复用 [`probe_state_of`]（WB 专属）
///
/// WB 的判据来自**响应体**（[`classify_event`]：`6004` / `IsModelRateLimit` / `11102` / 余额文案…）。
/// Qoder **没有 body 判据**，唯一判据是状态码 [`classify_http`]（`qoder/pool.rs`）。
///
/// ## `QoderErrKind` 只有四档 ⇒ 在 `ClientError` 里单独把 404 判成 `ModelUnavailable`
///
/// [`classify_http`] 把「非 429 / 非 401·403 的 4xx」一律归为 `ClientError`，**不区分 404**。
/// 故 404 在 `ClientError` 分支里单独识别（「该账号看不到这个模型」），其余 4xx 归 `UpstreamError`。
/// 429 ⇒ `RateLimitedModel`（Qoder 上游本来就**按模型**报限流，故**没有账号级细分**）；
/// 401/403 ⇒ `AuthDead`；5xx / 传输 ⇒ `UpstreamError`。
pub fn probe_state_of_qoder(status: u16) -> ProbeState {
    match classify_http(status) {
        QoderErrKind::RateLimited => ProbeState::RateLimitedModel,
        QoderErrKind::AuthDead => ProbeState::AuthDead,
        QoderErrKind::Transient => ProbeState::UpstreamError,
        QoderErrKind::ClientError => {
            if status == 404 {
                ProbeState::ModelUnavailable
            } else {
                ProbeState::UpstreamError
            }
        }
    }
}

/// ★ **唯一映射点（Trae）**：Trae 上游错误类别 → 单元格状态。纯函数，单测 + 变异覆盖。
///
/// ## Trae **无法细分**模型级 / 账号级 ⇒ 一律 `RateLimitedModel`
///
/// Trae 池只有**账号级**冷却（`TraePoolEntry` 没有 `model_cooldowns`），上游也不区分
/// 「哪个模型被限」与「整个账号被限」⇒ 三档限流里只能落 `RateLimitedModel`。
/// **取舍**：把账号级限流也记成模型级，是**探测呈现**层面的近似；因为 Trae 探测**不回填**
/// （见 [`ProbeTarget::adopt`] 的 Trae 分支），这个近似**不会**影响路由 / 冷却。
///
/// ## 判据零新造
///
/// 复用 `crate::trae::pool` 的两个分类入口：`classify_http`（HTTP 层）与 `classify_solo`
/// （SSE `event:error` 层）—— 全仓唯一的 Trae 错误分类入口，本模块只做「类别 → 单元格状态」映射。
///
/// ## 红线：`PlanLimit`（套餐额度用尽）**不是限流**
///
/// 方案 §1.2 的 Trae 列把 `PlanLimit`（`1005` / 含 `plan`）列为 `no_credits`，与限流**分列**
/// —— 把「额度用尽」报成「限流」会让用户去傻等解锁（`client_limits` 已踩过这个坑）。
pub fn probe_state_of_trae(kind: TraeErrKind) -> ProbeState {
    match kind {
        // 限流（429 / 4008 / 含 quota·exceeded·rate）：Trae 无法细分 ⇒ 模型级。
        TraeErrKind::SoftRate => ProbeState::RateLimitedModel,
        // 套餐额度用尽（1005 / 含 plan）：**不是限流**，独立成 `no_credits`。
        TraeErrKind::PlanLimit => ProbeState::NoCredits,
        // 会话失效（401，Trae 侧为永久禁用）。
        TraeErrKind::SessionDead => ProbeState::AuthDead,
        // 404：该账号看不到 / 不支持这个模型。
        TraeErrKind::NotFound => ProbeState::ModelUnavailable,
        // 5xx / 4xx / 业务码 / 「不归因于账号」（如 code 0 断流、4001 模型配置为空）：
        // 中性归「上游异常」—— 都不是限流。
        TraeErrKind::Server
        | TraeErrKind::Client
        | TraeErrKind::Business
        | TraeErrKind::None => ProbeState::UpstreamError,
    }
}

/// 探测目标：一个作用域 = 一套「账号轴 / 模型轴 / 判据 / 发请求 / 回填」。
///
/// ## 为什么是 enum 而不是 trait
///
/// 三个平台的 **state 类型不同**（`GatewayState` / `QoderGatewayState` / `TraeGatewayState`），
/// 无法用「方法收 `&GatewayState`」的统一引用参数表达。故每个变体**自己持有**对应平台的
/// state，方法实现按平台分叉。新增平台 = 加一个变体。
///
/// `start` / `run` 只跟本类型打交道，**不再出现 `Region`**。
#[derive(Clone)]
pub enum ProbeTarget {
    /// WorkBuddy（region 决定账号库 / 目录 / 判据）。
    Wb(GatewayState, Region),
    /// Qoder（region 决定账号库 / 内存清单缓存 / 判据）。
    Qoder(QoderGatewayState, QoderRegion),
    /// Trae（variant 决定账号池 / 客户端清单 / 判据；**只探测、不回填**）。
    Trae(TraeGatewayState, TraeVariant),
}

/// 手写 `Debug`（**只打作用域**，不打印 state）。
///
/// 理由：两个平台 state 类型**不都**实现 `Debug`（`GatewayState` 只 derive `Clone`），
/// 而 `ProbeTarget` 需要在单测里配合 `unwrap_err()` 等 API ⇒ 手写一个轻量实现，
/// 顺带避免把整个 state（含密钥 / 池）打进日志。
impl std::fmt::Debug for ProbeTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ProbeTarget({})", self.scope())
    }
}

impl ProbeTarget {
    /// 作业里记的**作用域字面量**（进 wire）：`"wb-cn"` … `"qoder-global"`。
    ///
    /// 走 core 的 `scope_of_*_region`（穷举、无回落），**不**手工拼字符串 —— 免得平台短标识漂移。
    pub fn scope(&self) -> &'static str {
        match self {
            ProbeTarget::Wb(_, region) => scope_of_wb_region(*region).as_str(),
            ProbeTarget::Qoder(_, region) => scope_of_qoder_region(*region).as_str(),
            ProbeTarget::Trae(_, variant) => scope_of_trae_region(variant.region()).as_str(),
        }
    }

    /// 账号轴：同步该作用域的账号库进池，返回 **uid 列表**（昵称在 [`Self::probe_cell`] 里现取）。
    async fn account_uids(&self) -> Vec<String> {
        match self {
            ProbeTarget::Wb(state, region) => {
                let realm = realm_of(*region);
                // 与账号库对齐 —— 否则「刚导入、还没发过请求」的账号不在池里，探测会漏掉它们。
                let accounts = account::load_accounts_for(*region);
                let mut pool = state.pool.write().await;
                pool.sync_accounts(&accounts, Some(realm));
                pool.entries()
                    .filter(|entry| entry.realm == Some(realm))
                    .map(|entry| entry.uid.clone())
                    .collect()
            }
            ProbeTarget::Qoder(state, region) => {
                let mut pools = state.pools.lock().await;
                let pool = pools.entry(*region).or_default();
                // `sync_for` 从账号库重建（保留运行态），保证「导入即入池」。
                pool.sync_for(*region);
                pool.entries().map(|entry| entry.uid.clone()).collect()
            }
            ProbeTarget::Trae(state, variant) => {
                let mut pools = state.pools.lock().await;
                let pool = pools
                    .entry(*variant)
                    .or_insert_with(|| TraePool::for_variant(*variant));
                // `sync_for` 按**变体**（其实按区域）从账号库重建 —— 与网关选号同源，
                // 保证「导入即入池」；`sync_for` 会滤掉空 JWT 的账号。
                pool.sync_for(*variant);
                pool.entries().iter().map(|entry| entry.uid.clone()).collect()
            }
        }
    }

    /// 模型轴：该作用域目录里的模型 id（**纯内存、不联网**；见模块头「模型轴」）。
    async fn catalog_models(&self) -> Vec<String> {
        match self {
            ProbeTarget::Wb(state, region) => state
                .catalogs
                .current(*region)
                .models
                .into_iter()
                .map(|model| model.id)
                .collect(),
            ProbeTarget::Qoder(state, region) => state
                .cached_catalog_models(*region)
                .await
                .into_iter()
                .filter_map(|entry| {
                    entry
                        .get("id")
                        .or_else(|| entry.get("key"))
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .collect(),
            ProbeTarget::Trae(_, variant) => {
                // 按**变体**读客户端（上游下发）清单 —— 所有账号共用同一份 ⇒ **全行同列**
                // （方案 §1.1 的坑）。复用网关 `/v1/models` 的同一份选择逻辑
                // （`models_response_for`：只取该变体 `function` 真能调的模型；客户端缓存
                // 读不到时它自己回落静态清单），保证「探测的模型 == 网关会列的模型」。
                //
                // ⚠️ 读客户端 `state.vscdb` 是**同步阻塞** IO（网关 `/v1/models` 同样走
                // `spawn_blocking`）⇒ 这里也搬进阻塞线程池，别卡住异步运行时。
                let variant = *variant;
                tokio::task::spawn_blocking(move || {
                    let body =
                        trae_payload::models_response_for(variant, &std::collections::BTreeSet::new());
                    body.get("data")
                        .and_then(Value::as_array)
                        .map(|rows| {
                            rows.iter()
                                .filter_map(|row| {
                                    row.get("id").and_then(Value::as_str).map(str::to_string)
                                })
                                .collect::<Vec<String>>()
                        })
                        .unwrap_or_default()
                })
                .await
                .unwrap_or_default()
            }
        }
    }

    /// 探一格（单账号、单次、零重试、有硬超时）。
    async fn probe_cell(&self, uid: &str, model: &str) -> ProbeCell {
        match self {
            ProbeTarget::Wb(state, region) => probe_cell_wb(state, *region, uid, model).await,
            ProbeTarget::Qoder(state, region) => probe_cell_qoder(state, *region, uid, model).await,
            ProbeTarget::Trae(state, variant) => probe_cell_trae(state, *variant, uid, model).await,
        }
    }

    /// 回填（唯一写动作）：把探测结论写进池冷却。返回 `(added, cleared, 明细)`。
    ///
    /// ★ **Trae 是显式空操作**（T1：只探测、不回填）—— 理由见该分支的注释。
    async fn adopt(&self, cells: &[ProbeCell], now: i64) -> (usize, usize, Vec<ProbeAdoptRecord>) {
        match self {
            ProbeTarget::Wb(state, region) => adopt_wb(state, *region, cells, now).await,
            ProbeTarget::Qoder(state, region) => adopt_qoder(state, *region, cells, now).await,
            ProbeTarget::Trae(_, _) => {
                // ★★ **显式空操作**（T1：Trae 只探测、不回填）。
                //
                // ## 为什么必须是空操作（方案 §4.5.4 的 T1）
                //
                // Trae 池**没有「模型级冷却」这个概念** —— `TraePoolEntry` 只有账号级
                // `cooldown_until` / `cooldown_reason`，`apply_error(uid, kind, reason)`
                // **连 `model` 参数都没有**。若把「模型 M 被限流」回填成**账号级**冷却，
                // 会把该账号的**其它模型一起打死**（过度冷却）—— 正是方案 §4.5.4 明确
                // **不建议**的 T3。故本期 Trae 采 T1：探测结果**只呈现**，绝不写池。
                //
                // ⚠️ 返回空计数 + 空明细（不是 `todo!()`）—— 前端据此把 `adopted` 恒显示
                // 为空，与「Trae 卡片上隐藏/禁用回填开关」配套，**不让用户以为回填生效了**。
                (0, 0, Vec::new())
            }
        }
    }
}

/// 由作用域字符串构造探测目标（**fail-closed**：未知 / 空 / 拼错一律 `Err`）。
///
/// `Scope::parse` 的错误用 `ScopeError::message()`（可读、列出合法取值）。
pub fn target_for_scope(raw: &str) -> Result<ProbeTarget, String> {
    let scope = Scope::parse(raw).map_err(|error| error.message())?;
    match scope.product() {
        Product::Workbuddy => Ok(ProbeTarget::Wb(
            <WbGateway as GatewayProduct>::shared_state(),
            scope.wb_region().expect("WB 作用域必有 WB 区域"),
        )),
        Product::Qoder => Ok(ProbeTarget::Qoder(
            <QoderGateway as GatewayProduct>::shared_state(),
            scope.qoder_region().expect("Qoder 作用域必有 Qoder 区域"),
        )),
        Product::Trae => Ok(ProbeTarget::Trae(
            <TraeGateway as GatewayProduct>::shared_state(),
            trae_variant_of(scope.trae_region().expect("Trae 作用域必有 Trae 区域")),
        )),
    }
}

/// Trae 区域 → 该区域**当前客户端**的变体（`trae-cn` → `TraeMerged`，
/// `trae-global` → `GlobalTraeMerged`）。
///
/// ## 为什么是**合并版**
///
/// 账号库 / 冷却 / 积分都按 `variant.region()` 分家 ⇒ 区域内**任一**变体读到的账号轴
/// 完全相同；但**模型清单**按变体分家（`read_client_model_list`），且 `function_for`
/// 决定请求体里的 `function` 白名单。官方已把两条老程序位合并成单一客户端
/// （CN `Trae CN` / 国际 `Trae`，见 `TraeVariant::all` 与 `parse("cn"/"global")`），
/// 因此探测用**合并版**变体 —— 与前端 `regionPrograms` 给的 `trae_merged` /
/// `global_trae_merged` 逐字一致。
fn trae_variant_of(region: TraeRegion) -> TraeVariant {
    match region {
        TraeRegion::Cn => TraeVariant::TraeMerged,
        TraeRegion::Global => TraeVariant::GlobalTraeMerged,
    }
}

/// WB 回填：只置/清冷却，**绝不推 EMA**（唯一入口 [`crate::pool::Pool::adopt_probe_result`]）。
async fn adopt_wb(
    state: &GatewayState,
    region: Region,
    cells: &[ProbeCell],
    now: i64,
) -> (usize, usize, Vec<ProbeAdoptRecord>) {
    let realm = realm_of(region);
    let mut pool = state.pool.write().await;
    // 与账号库再对齐一次（探测期间用户可能导入/删了账号）。
    let accounts = account::load_accounts_for(region);
    pool.sync_accounts(&accounts, Some(realm));
    let (mut added, mut cleared) = (0usize, 0usize);
    let mut records: Vec<ProbeAdoptRecord> = Vec::new();
    for cell in cells {
        if cell.state == ProbeState::Skipped {
            continue;
        }
        // 墙钟缺失 / 过期时的**冷却下限**落在 `adopt_probe_result` 内部（回填的唯一入口）——
        // 这里原样透传，**不要**在调用方再补一遍（换个调用方就漏）。
        let outcome = probe_outcome_of(cell.state, cell.reset_at_ms);
        if pool.adopt_probe_result(&cell.uid, Some(realm), &cell.model, &outcome, now) {
            let was_cleared = matches!(outcome, ProbeOutcome::Available);
            let until_ms = if was_cleared {
                0
            } else {
                pool.get(&cell.uid)
                    .and_then(|entry| entry.model_cooldowns.get(&cell.model))
                    .map(|cool| cool.until_ms)
                    .unwrap_or(0)
            };
            records.push(ProbeAdoptRecord {
                uid: cell.uid.clone(),
                nickname: cell.nickname.clone(),
                model: cell.model.clone(),
                action: if was_cleared { "cleared" } else { "added" }.to_string(),
                until_ms,
            });
            if was_cleared {
                cleared += 1;
            } else {
                added += 1;
            }
        }
    }
    (added, cleared, records)
}

/// Qoder 回填：只置/清 `model_cooldowns`（[`crate::qoder::pool::QoderPool::adopt_probe_result`]），
/// **绝不推 EMA**。语义与 [`adopt_wb`] 逐条对齐。
async fn adopt_qoder(
    state: &QoderGatewayState,
    region: QoderRegion,
    cells: &[ProbeCell],
    now: i64,
) -> (usize, usize, Vec<ProbeAdoptRecord>) {
    let mut pools = state.pools.lock().await;
    let pool = pools.entry(region).or_default();
    // 与账号库再对齐一次（探测期间用户可能导入/刷新了账号）。
    pool.sync_for(region);
    let (mut added, mut cleared) = (0usize, 0usize);
    let mut records: Vec<ProbeAdoptRecord> = Vec::new();
    for cell in cells {
        if cell.state == ProbeState::Skipped {
            continue;
        }
        let outcome = probe_outcome_of(cell.state, cell.reset_at_ms);
        if pool.adopt_probe_result(&cell.uid, &cell.model, &outcome, now) {
            let was_cleared = matches!(outcome, ProbeOutcome::Available);
            let until_ms = if was_cleared {
                0
            } else {
                pool.entries()
                    .find(|entry| entry.uid == cell.uid)
                    .and_then(|entry| entry.model_cooldowns.get(&cell.model))
                    .copied()
                    .unwrap_or(0)
            };
            records.push(ProbeAdoptRecord {
                uid: cell.uid.clone(),
                nickname: cell.nickname.clone(),
                model: cell.model.clone(),
                action: if was_cleared { "cleared" } else { "added" }.to_string(),
                until_ms,
            });
            if was_cleared {
                cleared += 1;
            } else {
                added += 1;
            }
        }
    }
    (added, cleared, records)
}

/// 去重保序。
fn dedup_keep_order(items: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for item in items {
        let item = item.trim().to_string();
        if item.is_empty() || !seen.insert(item.clone()) {
            continue;
        }
        out.push(item);
    }
    out
}

/// 展开待探清单：**账号为外层**（与原型/前端的矩阵行序一致），超出 [`MAX_CELLS`] 截断。
///
/// 返回 `(plan, truncated)`。纯函数（可单测：顺序、截断）。
pub(crate) fn build_plan(accounts: &[String], models: &[String]) -> (Vec<(String, String)>, bool) {
    let mut plan = Vec::new();
    'outer: for uid in accounts {
        for model in models {
            plan.push((uid.clone(), model.clone()));
            if plan.len() >= MAX_CELLS {
                break 'outer;
            }
        }
    }
    let truncated = plan.len() < accounts.len() * models.len();
    (plan, truncated)
}

/// 启动一次探测：**原子抢占作业位** + 建作业（快），逐格执行由调用方 spawn [`run`]。
///
/// 返回本轮格数。作业位被占时返回 `Err`。
pub async fn start(
    target: &ProbeTarget,
    uids: Option<Vec<String>>,
    models: Option<Vec<String>>,
    adopt: bool,
) -> Result<usize, String> {
    // ★ 原子抢占：`compare_exchange` 让并发两次 `start` **只有一个**能进。
    // 只靠「锁内查 phase」是 TOCTOU —— 检查与写入横跨 `pool.write().await`，锁早就放了。
    if busy_flag()
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("已有探测在进行中，请等它结束或先取消".to_string());
    }
    match prepare(target, uids, models, adopt).await {
        Ok(total) => Ok(total),
        Err(error) => {
            busy_flag().store(false, Ordering::SeqCst);
            Err(error)
        }
    }
}

/// [`start`] 的主体（校验 + 建作业）。任何 `Err` 由 [`start`] 负责释放作业位。
async fn prepare(
    target: &ProbeTarget,
    uids: Option<Vec<String>>,
    models: Option<Vec<String>>,
    adopt: bool,
) -> Result<usize, String> {
    // 账号轴：同步账号库进池后取 uid 列表（池为空 ⇒ 明确报错）。
    let pool_rows = target.account_uids().await;
    if pool_rows.is_empty() {
        return Err(format!("{} 的账号池里没有账号", target.scope()));
    }

    let accounts: Vec<String> = match uids {
        Some(list) => {
            if list.len() > MAX_AXIS {
                return Err(format!("账号数超过上限（{MAX_AXIS}）"));
            }
            let wanted = dedup_keep_order(list);
            if wanted.is_empty() {
                return Err("没有选中任何账号".to_string());
            }
            let picked: Vec<String> = wanted
                .iter()
                .filter(|uid| pool_rows.iter().any(|row| row == *uid))
                .cloned()
                .collect();
            if picked.is_empty() {
                return Err("选中的账号都不在该作用域的池里".to_string());
            }
            picked
        }
        None => pool_rows,
    };

    let models = match models {
        Some(list) => {
            if list.len() > MAX_AXIS {
                return Err(format!("模型数超过上限（{MAX_AXIS}）"));
            }
            dedup_keep_order(list)
        }
        // 回落该作用域的**目录**（纯内存、不联网；见模块头「模型轴」）。
        None => dedup_keep_order(target.catalog_models().await),
    };
    if models.is_empty() {
        return Err("没有可探测的模型（清单为空，请先「重新识别」）".to_string());
    }

    let (plan, truncated) = build_plan(&accounts, &models);
    let total = plan.len();

    cancel_flag().store(false, Ordering::SeqCst);
    *job_slot().lock().unwrap() = Some(ProbeJob {
        scope: target.scope().to_string(),
        phase: ProbePhase::Running,
        started_at_ms: timeutil::now_ms(),
        finished_at_ms: None,
        total,
        done: 0,
        current: String::new(),
        truncated,
        adopt,
        adopted_added: 0,
        adopted_cleared: 0,
        adopted: Vec::new(),
        cells: Vec::with_capacity(total),
        plan,
    });
    Ok(total)
}

/// 逐格执行（调用方 spawn）。结束时会按 `adopt` 决定是否回填池冷却。
///
/// ⚠️ **作业位的释放由顶部的 RAII 归位器（[`ReleaseBusy`]）保证** —— 覆盖四种退出：
/// 正常结束 / 取消结束 / 早退（作业被清）/ **panic 展开**。函数体里**不再**有任何零散的
/// `busy_flag().store(false, …)`：「两处各自负责释放」这种写法迟早会漂移，且漏掉 panic 那条出口
/// （一旦 `run` panic 或宿主压根没 spawn 它，作业位就永久 `true` —— 之后每次 `start` 都报
/// 「已有探测在进行中」，**用户可见却不可自愈**，只能重启进程）。
pub async fn run(target: ProbeTarget) {
    /// 作业位的 **RAII 归位器**：`run` 返回或 panic 展开时都会释放。
    ///
    /// 放在 `run` 内部而不是 `start`：`start` 只是**抢占**，作业位的生命周期属于这一轮执行。
    struct ReleaseBusy;
    impl Drop for ReleaseBusy {
        fn drop(&mut self) {
            busy_flag().store(false, Ordering::SeqCst);
        }
    }
    let _release = ReleaseBusy;

    loop {
        if cancel_flag().load(Ordering::SeqCst) {
            finish(ProbePhase::Cancelled, &target).await;
            return;
        }
        let next = {
            let mut guard = job_slot().lock().unwrap();
            let Some(job) = guard.as_mut() else {
                // 作业被清（例如换作用域）⇒ 直接退出；作业位由上面的 RAII guard 释放。
                return;
            };
            match job.plan.pop() {
                Some(pair) => {
                    job.current = format!("{} · {}", pair.0, pair.1);
                    Some(pair)
                }
                None => None,
            }
        };
        let Some((uid, model)) = next else {
            finish(ProbePhase::Done, &target).await;
            return;
        };

        // 作业在两次加锁之间被清（例如 `cancel` 自愈 / 换作用域）⇒ 直接退出（与改动前一致）。
        if job_slot().lock().unwrap().is_none() {
            return;
        }

        let cell = target.probe_cell(&uid, &model).await;

        {
            let mut guard = job_slot().lock().unwrap();
            let Some(job) = guard.as_mut() else {
                return;
            };
            job.done += 1;
            job.cells.push(cell);
        }
        tokio::time::sleep(Duration::from_millis(PROBE_GAP_MS)).await;
    }
}

/// 收尾：可选回填 + 置终态。**不再负责释放作业位** —— 那由 [`run`] 的 RAII 归位器统一保证。
async fn finish(phase: ProbePhase, target: &ProbeTarget) {
    let (adopt, cells) = {
        let guard = job_slot().lock().unwrap();
        match guard.as_ref() {
            Some(job) => (job.adopt, job.cells.clone()),
            None => return,
        }
    };

    let (mut added, mut cleared) = (0usize, 0usize);
    let mut records: Vec<ProbeAdoptRecord> = Vec::new();
    if adopt {
        let now = timeutil::now_ms();
        let outcome = target.adopt(&cells, now).await;
        added = outcome.0;
        cleared = outcome.1;
        records = outcome.2;
    }

    {
        let mut guard = job_slot().lock().unwrap();
        if let Some(job) = guard.as_mut() {
            job.phase = phase;
            job.finished_at_ms = Some(timeutil::now_ms());
            job.current = String::new();
            job.adopted_added = added;
            job.adopted_cleared = cleared;
            job.adopted = records;
        }
    }
}

/// 探一格（WB）：**单账号、单次、零重试、有硬超时**。
///
/// 判据一律复用 [`classify_event`]（全仓唯一 WB 限流判据），只做「事件 → 单元格状态」的映射。
async fn probe_cell_wb(state: &GatewayState, region: Region, uid: &str, model: &str) -> ProbeCell {
    let lookup = account::find_account_for(region, uid);
    let nickname = lookup
        .as_ref()
        .map(account::account_display_name)
        .unwrap_or_default();
    let skipped = |reason: &str| ProbeCell {
        uid: uid.to_string(),
        nickname: nickname.clone(),
        model: model.to_string(),
        state: ProbeState::Skipped,
        http_status: 0,
        reset_at_ms: None,
        reason: reason.to_string(),
        latency_ms: 0,
    };

    let Some(acc) = lookup else {
        return skipped("账号不在库里");
    };
    // ① 信封凭据送进网关会被折成**空 `Bearer`** ⇒ 上游必回 401，那是假阴性。
    if account::is_envelope(&acc, "access_token") {
        return skipped("凭据是加密信封，无法直接调用");
    }
    // ② 空 / 缺失 token 同样会发出 `Authorization: Bearer `（空）—— 方案 §8.6 的红线。
    //    真实流量那条路会因选号失败提前退出，探测是自己造请求，必须在这里拦。
    let token = account::get_str(&acc, "access_token").unwrap_or_default();
    if token.trim().is_empty() {
        return skipped("没有可用的 access_token");
    }

    let options = state.outbound_options().await;
    let raw = json!({
        "model": model,
        "stream": true,
        "max_tokens": 1,
        "messages": [{ "role": "user", "content": PROBE_TEXT }],
    })
    .to_string();
    let body = outbound::prepare_outbound_body(
        region,
        &raw,
        &options,
        &OutboundMeta {
            uid: uid.to_string(),
            conversation_id: None,
        },
        false,
    );

    let context = ConversationContext::new(None, None, None, None, None);
    let mut extra = session_headers::conversation_headers(&context, &session_headers::new_hex_id());
    extra.extend(session_headers::device_headers(uid));

    let started = Instant::now();
    // ★ 单格硬超时：宿主客户端的总超时是 600s，对一格太长（一格挂住会拖死整轮）。
    let result = tokio::time::timeout(
        Duration::from_secs(PROBE_CELL_TIMEOUT_SECS),
        state
            .upstream
            .chat_stream_with(region, &acc, &body, None, Some(&extra)),
    )
    .await;
    let latency_ms = started.elapsed().as_millis() as i64;

    let result = match result {
        Ok(inner) => inner,
        Err(_elapsed) => {
            return ProbeCell {
                uid: uid.to_string(),
                nickname,
                model: model.to_string(),
                state: ProbeState::TransportError,
                http_status: 0,
                reset_at_ms: None,
                reason: format!("单格超时（{}s）", PROBE_CELL_TIMEOUT_SECS),
                latency_ms,
            }
        }
    };

    match result {
        // 2xx ⇒ **立刻丢弃响应体**（不聚合、不落账本），只留状态与耗时。
        UpstreamChatResult::Ok(_) => ProbeCell {
            uid: uid.to_string(),
            nickname,
            model: model.to_string(),
            state: ProbeState::Ok,
            http_status: 200,
            reset_at_ms: None,
            reason: String::new(),
            latency_ms,
        },
        UpstreamChatResult::Err {
            status, message, ..
        } => {
            if status == 0 {
                return ProbeCell {
                    uid: uid.to_string(),
                    nickname,
                    model: model.to_string(),
                    state: ProbeState::TransportError,
                    http_status: 0,
                    reset_at_ms: None,
                    reason: message.chars().take(400).collect(),
                    latency_ms,
                };
            }
            let event = classify_event(status, &message);
            let state_key = probe_state_of(&event);
            let reset_at_ms = match &event {
                UpstreamEvent::SoftRate { reset_at_ms, .. } => *reset_at_ms,
                _ => None,
            };
            ProbeCell {
                uid: uid.to_string(),
                nickname,
                model: model.to_string(),
                state: state_key,
                http_status: status,
                reset_at_ms,
                reason: message.chars().take(400).collect(),
                latency_ms,
            }
        }
    }
}

/// 探一格（Qoder）：**单账号、单次、零重试、有硬超时**。
///
/// ## 判据零新造
///
/// Qoder **没有 body 判据**，唯一判据是 HTTP 状态码（[`probe_state_of_qoder`]）。
///
/// ## 发请求：单账号、单次、零重试（本轮最难的一处）
///
/// 用 [`chat_once_for_account`]（从 `qoder::routes::chat_for_region` 抽出的**单账号单次**入口）——
/// **绝不**用 `chat_for_region` 本身（它带 `max_rotate` 换号轮换，会把结果归因到别的账号）。
/// 请求体走 Qoder 现成构造：`build_chat_body` + `qoder_encode` + `cosy::shared_cache`。
/// 2xx ⇒ **立刻丢弃响应体**（不聚合、不解析 SSE），只留状态与耗时。
async fn probe_cell_qoder(
    state: &QoderGatewayState,
    region: QoderRegion,
    uid: &str,
    model: &str,
) -> ProbeCell {
    let entry = {
        let pools = state.pools.lock().await;
        pools
            .get(&region)
            .and_then(|pool| pool.entries().find(|entry| entry.uid == uid).cloned())
    };
    let nickname = entry
        .as_ref()
        .map(|entry| entry.name.clone())
        .unwrap_or_default();
    let skipped = |reason: &str| ProbeCell {
        uid: uid.to_string(),
        nickname: nickname.clone(),
        model: model.to_string(),
        state: ProbeState::Skipped,
        http_status: 0,
        reset_at_ms: None,
        reason: reason.to_string(),
        latency_ms: 0,
    };
    let Some(entry) = entry else {
        return skipped("账号不在库里");
    };
    // Qoder 无「加密信封」概念：PAT 不能推理 ⇒ 池里 `has_access() == false`（与选号同判据）。
    if !entry.has_access() {
        return skipped("没有可调用的访问令牌（PAT 只能用于非推理链路）");
    }

    // 模型元数据：清单里查不到 ⇒ 中性合成（与中继 `payload::meta_for` 同源）。
    let catalog = state.cached_catalog_models(region).await;
    let meta = payload::meta_for(&catalog, model);

    // 最小请求体（`max_tokens: 1` + 探测文案）：与中继同一套构造。
    let payload_value = json!({
        "model": model,
        "stream": true,
        "max_tokens": 1,
        "messages": [{ "role": "user", "content": PROBE_TEXT }],
    });
    let encoded = match qoder_api::build_chat_body(&payload_value, &meta, region, "") {
        Ok(body_value) => cosy::qoder_encode(body_value.to_string().as_bytes()),
        Err(error) => {
            return skipped(&format!("请求体构造失败：{error}"));
        }
    };

    let account_input = cosy::CosyAccountInput {
        uid: &entry.uid,
        nickname: &entry.name,
        access_token: &entry.access_token,
        refresh_token: &entry.refresh_token,
        user_type: None,
        org_id: None,
        org_name: None,
    };
    let session = match cosy::shared_cache().get(&account_input) {
        Ok(session) => session,
        Err(error) => return skipped(&format!("签名会话创建失败：{error}")),
    };
    let headers = session.headers(&encoded, PATH_CHAT_SSE, Some(&meta.key), true);

    let started = Instant::now();
    // ★ 单格硬超时（宿主客户端的总超时是 600s，对一格太长）。
    let result = tokio::time::timeout(
        Duration::from_secs(PROBE_CELL_TIMEOUT_SECS),
        chat_once_for_account(state, region, &encoded, &headers),
    )
    .await;
    let latency_ms = started.elapsed().as_millis() as i64;

    let response = match result {
        Ok(Ok(response)) => response,
        Ok(Err(transport_error)) => {
            return ProbeCell {
                uid: uid.to_string(),
                nickname,
                model: model.to_string(),
                state: ProbeState::TransportError,
                http_status: 0,
                reset_at_ms: None,
                reason: transport_error.chars().take(400).collect(),
                latency_ms,
            };
        }
        Err(_elapsed) => {
            return ProbeCell {
                uid: uid.to_string(),
                nickname,
                model: model.to_string(),
                state: ProbeState::TransportError,
                http_status: 0,
                reset_at_ms: None,
                reason: format!("单格超时（{}s）", PROBE_CELL_TIMEOUT_SECS),
                latency_ms,
            };
        }
    };

    let status = response.status().as_u16();
    if response.status().is_success() {
        // ★ 2xx ⇒ **立刻丢弃响应体**（`return` 即 drop，不聚合、不解析 SSE），只留状态与耗时。
        return ProbeCell {
            uid: uid.to_string(),
            nickname,
            model: model.to_string(),
            state: ProbeState::Ok,
            http_status: status,
            reset_at_ms: None,
            reason: String::new(),
            latency_ms,
        };
    }
    // 非 2xx：读**短错误体**作 `reason`（截断 400）—— 错误体不是 SSE 流，不会阻塞。
    let detail: String = response
        .text()
        .await
        .unwrap_or_default()
        .chars()
        .take(400)
        .collect();
    ProbeCell {
        uid: uid.to_string(),
        nickname,
        model: model.to_string(),
        state: probe_state_of_qoder(status),
        http_status: status,
        reset_at_ms: None,
        reason: detail,
        latency_ms,
    }
}

/// 探一格（Trae）：**单账号、单次、零重试、有硬超时、不写池**。
///
/// ## 判据：Trae 有**两套**分类入口，都要用（`probe_state_of_trae` 统一映射）
///
/// - HTTP **非 2xx**（含传输层 `status == 0`）⇒ `trae::pool::classify_http`；
/// - HTTP **2xx 但流内 `event:error`** ⇒ `trae::pool::classify_solo`。
///
/// ⚠️ **不能只看状态码**：Trae 上游把**业务错误放在 SSE 的 `event:error` 里，此时 HTTP
/// 仍是 200**。方案 §1.2 的 Trae 列明确把 `4008` / 含 `quota`·`exceeded`·`rate` 计入限流
/// —— 那些**只**出现在 SSE 错误事件里 ⇒ 若「2xx 即丢响应体」，SSE 级限流会被**误报成 `ok`**。
/// 故这里对 2xx **读一次**（`max_tokens: 1` ⇒ 极短）只为取出该事件，随后**丢弃**正文
/// （不聚合、不落账本、不写日志）。
///
/// ## 发请求：单账号、单次、零重试（本轮最难的一处）
///
/// 用 [`trae_chat_once_for_account`]（从 `trae::routes::chat_for_variant` 的换号循环里抽出的
/// **单账号单次**入口）—— **绝不**用 `chat_for_variant` / `attempt_once` 本身（它们带
/// `max_rotate` 换号轮换，且会把失败**写回池冷却**）。请求体走 Trae 现成构造
/// （`prepare_llm_chat_body`，内含 `TRAE_LLM_CHAT_PATH` 那套字段）。
///
/// ## 探测**不写池**（方案红线 2）
///
/// 这里只调 `chat_once_for_account`（纯出站）+ `sse::aggregate`（只读流），**不**调
/// `TraePool::apply_error` / `note_success` —— 探测自己把号打成冷却 = 自我实现的限流。
async fn probe_cell_trae(
    state: &TraeGatewayState,
    variant: TraeVariant,
    uid: &str,
    model: &str,
) -> ProbeCell {
    // 按 uid **直取**（不选号）：`pick` 在账号不可用时**会换一个号**，探测的「一格」
    // 必须只打这一个 uid。
    let account = {
        let mut pools = state.pools.lock().await;
        let pool = pools
            .entry(variant)
            .or_insert_with(|| TraePool::for_variant(variant));
        pool.sync_for(variant);
        pool.account_for(uid)
    };
    let nickname = account
        .as_ref()
        .map(|entry| entry.name.clone())
        .unwrap_or_default();
    let skipped = |reason: &str| ProbeCell {
        uid: uid.to_string(),
        nickname: nickname.clone(),
        model: model.to_string(),
        state: ProbeState::Skipped,
        http_status: 0,
        reset_at_ms: None,
        reason: reason.to_string(),
        latency_ms: 0,
    };
    let Some(account) = account else {
        return skipped("账号不在池里（无可用 JWT）");
    };
    // `sync_for` 已滤掉空 JWT，这里再拦一次（方案 §3.2：绝不发空凭据）。
    if account.jwt.trim().is_empty() {
        return skipped("没有可用的 JWT");
    }

    // 最小请求体（`max_tokens: 1` + 探测文案）：与中继同一套构造（`prepare_llm_chat_body`）。
    let raw = json!({
        "model": model,
        "stream": true,
        "max_tokens": 1,
        "messages": [{ "role": "user", "content": PROBE_TEXT }],
    });
    let raw = serde_json::to_vec(&raw).unwrap_or_default();

    let started = Instant::now();
    // ★ 单格硬超时：**发请求 + 读流都算在内** —— Trae 的业务错误在 SSE 里，必须读到
    //   `event:error` 或流结束；只把「拿响应头」纳入超时的话，上游中途挂住会永久卡住整轮。
    let outcome = tokio::time::timeout(Duration::from_secs(PROBE_CELL_TIMEOUT_SECS), async {
        let response = trae_chat_once_for_account(state, &account, variant, model, &raw).await?;
        let status = response.status().as_u16();
        // ★ HTTP 2xx **不代表成功**：Trae 上游把业务错误放在 SSE 的 `event:error` 里。
        //   读一次（`max_tokens: 1` ⇒ 极短）只为取出该事件；正文**立刻丢弃**（不聚合、不落账本）。
        let (_payload, error, _usage) = trae_sse::aggregate(response, "", model).await;
        Ok::<(u16, Option<(i64, String)>), (u16, String)>((status, error))
    })
    .await;
    let latency_ms = started.elapsed().as_millis() as i64;

    let (status, error) = match outcome {
        Ok(Ok(parts)) => parts,
        Ok(Err((status, detail))) => {
            // `status == 0` 是传输层失败（连不上）；其余是 HTTP 非 2xx。
            let state_key = if status == 0 {
                ProbeState::TransportError
            } else {
                probe_state_of_trae(trae_classify_http(status))
            };
            return ProbeCell {
                uid: uid.to_string(),
                nickname,
                model: model.to_string(),
                state: state_key,
                http_status: status,
                reset_at_ms: None,
                reason: detail.chars().take(400).collect(),
                latency_ms,
            };
        }
        Err(_elapsed) => {
            return ProbeCell {
                uid: uid.to_string(),
                nickname,
                model: model.to_string(),
                state: ProbeState::TransportError,
                http_status: 0,
                reset_at_ms: None,
                reason: format!("单格超时（{}s）", PROBE_CELL_TIMEOUT_SECS),
                latency_ms,
            };
        }
    };

    if let Some((code, message)) = error {
        return ProbeCell {
            uid: uid.to_string(),
            nickname,
            model: model.to_string(),
            state: probe_state_of_trae(trae_classify_solo(code, &message)),
            http_status: status,
            reset_at_ms: None,
            reason: message.chars().take(400).collect(),
            latency_ms,
        };
    }
    ProbeCell {
        uid: uid.to_string(),
        nickname,
        model: model.to_string(),
        state: ProbeState::Ok,
        http_status: status,
        reset_at_ms: None,
        reason: String::new(),
        latency_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn soft(model_scoped: bool) -> UpstreamEvent {
        UpstreamEvent::SoftRate {
            reset_at_ms: Some(1_700_000_000_000),
            model_scoped,
        }
    }

    fn job_of(phase: ProbePhase, cells: Vec<ProbeCell>) -> ProbeJob {
        ProbeJob {
            scope: "wb-cn".to_string(),
            phase,
            started_at_ms: 1_700_000_000_000,
            finished_at_ms: None,
            total: cells.len(),
            done: cells.len(),
            current: String::new(),
            truncated: false,
            adopt: false,
            adopted_added: 0,
            adopted_cleared: 0,
            adopted: Vec::new(),
            cells,
            plan: Vec::new(),
        }
    }

    /// ★ 前三档才是「限流」；402 积分不足与登录失效必须**分列**。
    ///
    /// 变异（QA 已实测）：把 `HardCredit` 也映射成 `RateLimitedModel` ⇒ 本用例红
    /// （那正是「把没积分报成限流、用户去傻等解锁」的坑，`client_limits` 已踩过）。
    #[test]
    fn only_three_states_count_as_rate_limited() {
        assert_eq!(probe_state_of(&soft(true)), ProbeState::RateLimitedModel);
        assert_eq!(probe_state_of(&soft(false)), ProbeState::RateLimitedAccount);
        assert_eq!(probe_state_of(&UpstreamEvent::ModelBlocked), ProbeState::ModelBlocked);

        assert_eq!(probe_state_of(&UpstreamEvent::HardCredit), ProbeState::NoCredits);
        assert_eq!(probe_state_of(&UpstreamEvent::SessionDead), ProbeState::AuthDead);
        assert_eq!(probe_state_of(&UpstreamEvent::NotFound), ProbeState::ModelUnavailable);
        assert_eq!(probe_state_of(&UpstreamEvent::Server), ProbeState::UpstreamError);
        assert_eq!(probe_state_of(&UpstreamEvent::Client), ProbeState::UpstreamError);
    }

    /// 模型级与账号级**不能混**（判反了会让「换模型可用」的用户白等）。
    #[test]
    fn model_scoped_flag_decides_model_vs_account() {
        assert_ne!(
            probe_state_of(&soft(true)),
            probe_state_of(&soft(false)),
            "模型级与账号级必须是两个不同的格"
        );
    }

    /// 采纳语义：只有三档限流 + `ok` 会动冷却，其余一律 `Other`。
    #[test]
    fn only_limit_states_and_ok_touch_cooling() {
        assert_eq!(probe_outcome_of(ProbeState::Ok, None), ProbeOutcome::Available);
        assert!(matches!(
            probe_outcome_of(ProbeState::RateLimitedModel, Some(1)),
            ProbeOutcome::ModelRateLimited { reset_at_ms: Some(1) }
        ));
        assert_eq!(probe_outcome_of(ProbeState::ModelBlocked, None), ProbeOutcome::ModelBlocked);
        assert!(matches!(
            probe_outcome_of(ProbeState::RateLimitedAccount, None),
            ProbeOutcome::AccountRateLimited { reset_at_ms: None }
        ));
        for state in [
            ProbeState::NoCredits,
            ProbeState::AuthDead,
            ProbeState::ModelUnavailable,
            ProbeState::UpstreamError,
            ProbeState::TransportError,
            ProbeState::Skipped,
        ] {
            assert_eq!(
                probe_outcome_of(state, None),
                ProbeOutcome::Other,
                "{state:?} 不得动冷却 —— 402/登录失效/网络错误都不是「限流」"
            );
        }
    }

    /// ★ Qoder 判据映射：唯一判据是**状态码**（Qoder 没有 body 判据）。
    ///
    /// 变异①：把 429 映射成 `RateLimitedAccount` ⇒ 第一条断言红（Qoder 上游本来就按模型报，无账号级细分）。
    /// 变异②：把 404 也归 `UpstreamError` ⇒ 404 那条断言红（「该账号看不到这个模型」≠ 上游错误）。
    #[test]
    fn qoder_status_maps_to_probe_state() {
        assert_eq!(probe_state_of_qoder(429), ProbeState::RateLimitedModel);
        assert_eq!(probe_state_of_qoder(401), ProbeState::AuthDead);
        assert_eq!(probe_state_of_qoder(403), ProbeState::AuthDead);
        assert_eq!(probe_state_of_qoder(404), ProbeState::ModelUnavailable);
        assert_eq!(probe_state_of_qoder(400), ProbeState::UpstreamError);
        assert_eq!(probe_state_of_qoder(500), ProbeState::UpstreamError);
        assert_eq!(probe_state_of_qoder(503), ProbeState::UpstreamError);
        // 429 与 401/403 必须落在**不同**档（限流 ≠ 凭据死）。
        assert_ne!(probe_state_of_qoder(429), probe_state_of_qoder(401));
        // 模型不可见与上游错误必须分列。
        assert_ne!(probe_state_of_qoder(404), probe_state_of_qoder(500));
    }

    /// `target_for_scope` **fail-closed**：未知 / 空 / 拼错一律 `Err`（可读文案，列出合法取值）。
    ///
    /// 只测 `Err` 分支（不构造真实 state，保持测试无副作用）；合法作用域的接线另见
    /// [`target_for_scope_wires_trae`] / [`trae_region_maps_to_current_variant`]。
    #[test]
    fn target_for_scope_is_fail_closed() {
        for bad in ["", "wb", "trae-glboal", "WB-CN", "qoder-cn ", "wb-cn-x"] {
            let error = target_for_scope(bad).unwrap_err();
            assert!(error.contains("wb-cn"), "{error}");
            assert!(error.contains("qoder-global"), "{error}");
        }
    }

    /// ★ Trae 已接线：`trae-cn` / `trae-global` 必须能构造出探测目标（不再是「未实现」）。
    ///
    /// 变异：把 `target_for_scope` 的 `Product::Trae` 分支改回 `Err(...)` ⇒ 本用例红。
    #[test]
    fn target_for_scope_wires_trae() {
        for scope in ["trae-cn", "trae-global"] {
            let target = target_for_scope(scope).unwrap_or_else(|error| panic!("{scope}: {error}"));
            assert_eq!(target.scope(), scope);
        }
    }

    /// Trae 区域 → 变体（纯函数）：CN → 合并版、国际 → 国际合并版。
    ///
    /// 与 `TraeVariant::parse` 的**区域标识**口径必须一致（前端 `regionPrograms` 同源）——
    /// 否则探测读到的客户端清单 / `function` 白名单会与前端展示的程序位对不上。
    #[test]
    fn trae_region_maps_to_current_variant() {
        assert_eq!(trae_variant_of(TraeRegion::Cn), TraeVariant::TraeMerged);
        assert_eq!(trae_variant_of(TraeRegion::Global), TraeVariant::GlobalTraeMerged);
        assert_eq!(trae_variant_of(TraeRegion::Cn), TraeVariant::parse("cn").unwrap());
        assert_eq!(
            trae_variant_of(TraeRegion::Global),
            TraeVariant::parse("global").unwrap()
        );
    }

    /// ★ Trae 判据映射：复用 `trae::pool` 的两个分类入口，全枚举覆盖。
    ///
    /// 变异①：把 `PlanLimit` 也映射成 `RateLimitedModel` ⇒ 第二条断言红
    ///   （那正是「把额度用尽报成限流、用户去傻等解锁」的坑）。
    /// 变异②：把 `SoftRate` 映射成 `RateLimitedAccount` ⇒ 首条断言红
    ///   （Trae **无法细分**账号级 ⇒ 一律模型级）。
    #[test]
    fn trae_err_kind_maps_to_probe_state() {
        assert_eq!(probe_state_of_trae(TraeErrKind::SoftRate), ProbeState::RateLimitedModel);
        assert_eq!(probe_state_of_trae(TraeErrKind::PlanLimit), ProbeState::NoCredits);
        assert_eq!(probe_state_of_trae(TraeErrKind::SessionDead), ProbeState::AuthDead);
        assert_eq!(probe_state_of_trae(TraeErrKind::NotFound), ProbeState::ModelUnavailable);
        assert_eq!(probe_state_of_trae(TraeErrKind::Server), ProbeState::UpstreamError);
        assert_eq!(probe_state_of_trae(TraeErrKind::Client), ProbeState::UpstreamError);
        assert_eq!(probe_state_of_trae(TraeErrKind::Business), ProbeState::UpstreamError);
        assert_eq!(probe_state_of_trae(TraeErrKind::None), ProbeState::UpstreamError);

        // ★ 红线：PlanLimit（套餐额度用尽）**不是限流**，必须与 SoftRate 分列。
        assert_ne!(
            probe_state_of_trae(TraeErrKind::PlanLimit),
            probe_state_of_trae(TraeErrKind::SoftRate),
            "额度用尽与限流必须分列（否则用户去傻等解锁）"
        );
        // Trae **无**账号级 / 模型封禁概念 ⇒ 全枚举都不得落这两档。
        for kind in [
            TraeErrKind::SoftRate,
            TraeErrKind::PlanLimit,
            TraeErrKind::SessionDead,
            TraeErrKind::NotFound,
            TraeErrKind::Server,
            TraeErrKind::Client,
            TraeErrKind::Business,
            TraeErrKind::None,
        ] {
            let state = probe_state_of_trae(kind);
            assert_ne!(state, ProbeState::RateLimitedAccount, "{kind:?} 不得落账号级");
            assert_ne!(state, ProbeState::ModelBlocked, "{kind:?} 不得落模型封禁");
        }
    }

    /// 序列化名必须与前端/原型**逐字一致**（改名会让前端整列变「未知」）。
    #[test]
    fn state_names_are_the_wire_contract() {
        let names: Vec<String> = [
            ProbeState::Ok,
            ProbeState::RateLimitedModel,
            ProbeState::RateLimitedAccount,
            ProbeState::ModelBlocked,
            ProbeState::NoCredits,
            ProbeState::AuthDead,
            ProbeState::ModelUnavailable,
            ProbeState::UpstreamError,
            ProbeState::TransportError,
            ProbeState::Skipped,
        ]
        .into_iter()
        .map(|state| serde_json::to_value(state).unwrap().as_str().unwrap().to_string())
        .collect();
        assert_eq!(
            names,
            vec![
                "ok",
                "rate_limited_model",
                "rate_limited_account",
                "model_blocked",
                "no_credits",
                "auth_dead",
                "model_unavailable",
                "upstream_error",
                "transport_error",
                "skipped",
            ]
        );
    }

    /// 去重保序：重复/空白项被剔除，**顺序不变**（否则矩阵列序会乱跳）。
    #[test]
    fn dedup_keeps_order_and_drops_blanks() {
        let out = dedup_keep_order(vec![
            "b".to_string(),
            " a ".to_string(),
            "b".to_string(),
            "".to_string(),
            "  ".to_string(),
            "c".to_string(),
        ]);
        assert_eq!(out, vec!["b".to_string(), "a".to_string(), "c".to_string()]);
    }

    /// 清单**账号为外层**（矩阵行序），且在上限处截断并**如实报告** `truncated`。
    ///
    /// 变异：把外层换成 `models` ⇒ 顺序断言红；把截断去掉 ⇒ `truncated` 断言红。
    #[test]
    fn build_plan_is_account_major_and_truncates_loudly() {
        let accounts: Vec<String> = (0..30).map(|i| format!("u{i}")).collect();
        let models: Vec<String> = (0..20).map(|i| format!("m{i}")).collect();
        let (plan, truncated) = build_plan(&accounts, &models);
        assert_eq!(plan.len(), MAX_CELLS, "必须在上限处截断");
        assert!(truncated, "被截断必须如实报出，不能静默丢");
        assert_eq!(plan[0], ("u0".to_string(), "m0".to_string()));
        assert_eq!(plan[1], ("u0".to_string(), "m1".to_string()), "外层是账号");
        assert_eq!(plan[20], ("u1".to_string(), "m0".to_string()));

        let (small, truncated) = build_plan(&["u0".to_string()], &["m0".to_string()]);
        assert_eq!(small.len(), 1);
        assert!(!truncated, "没超上限就不能报截断");
    }

    /// 前端要读的 wire 形状：`idle` / `running` / `done` 三种都必须**是对象且带 phase**。
    ///
    /// 变异：把 `status()` 的 idle 分支改成 `Value::Null` ⇒ red（前端 `job.phase` 直接炸）。
    #[test]
    fn status_wire_shape_is_stable() {
        reset_for_test();
        assert_eq!(status()["phase"], json!("idle"));

        set_job_for_test(job_of(ProbePhase::Running, Vec::new()));
        let running = status();
        assert_eq!(running["phase"], json!("running"));
        assert_eq!(running["scope"], json!("wb-cn"));
        assert!(running["total"].is_number(), "total 必须是数字");
        assert!(running["cells"].is_array());

        set_job_for_test(job_of(
            ProbePhase::Done,
            vec![test_cell("u1", "glm-5.3", ProbeState::RateLimitedModel)],
        ));
        let done = status();
        assert_eq!(done["phase"], json!("done"));
        assert_eq!(done["cells"][0]["state"], json!("rate_limited_model"));
        assert_eq!(done["cells"][0]["uid"], json!("u1"));
        // 回填明细：**必须恒为数组**（前端要展开它；`null` 会让前端直接炸）。
        assert!(done["adopted"].is_array(), "`adopted` 必须是数组");
        reset_for_test();
    }

    /// ★ 三个进程级标志（取消位 / 作业位 / 作业槽）的抢占与复位 —— **合并成一条**。
    ///
    /// ⚠️ 它们是**进程级静态**：拆成多条测试会互相 reset/cancel
    /// （QA 已指出这是真实的 flake 窗口，故此处刻意只留一条测试碰它们）。
    ///
    /// 涵盖三个修复点的回归：
    /// - **作业位原子抢占**（TOCTOU 修复）：并发第二次必须失败；
    /// - **释放后可再抢**：否则一次异常就永久起不来；
    /// - ★ **`cancel()` 兜底自愈**：没有真在跑的作业时释放作业位（覆盖「`run` panic /
    ///   宿主没 spawn `run` ⇒ 作业位永久占着，用户只能重启进程」的死角），
    ///   但作业**真在跑**时**绝不**释放（否则双作业并发写同一槽）。
    #[test]
    fn global_flags_round_trip_and_reset() {
        reset_for_test();
        assert!(!cancel_flag().load(Ordering::SeqCst));
        assert!(!busy_flag().load(Ordering::SeqCst));
        assert_eq!(status()["phase"], json!("idle"), "复位后应是 idle");

        // ── 作业位原子抢占（TOCTOU 修复点）──
        assert!(
            busy_flag().compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok(),
            "空位应能抢占"
        );
        assert!(
            busy_flag().compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err(),
            "★ 已被占时第二次必须失败（TOCTOU 修复点）"
        );
        busy_flag().store(false, Ordering::SeqCst);
        assert!(
            busy_flag().compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok(),
            "释放后必须能再抢（否则一次异常就永久起不来）"
        );

        // ── ★ cancel() 自愈①：作业槽**为空**时，取消应顺手释放作业位 ──
        reset_for_test(); // 作业槽 = None、三个位归零
        busy_flag().store(true, Ordering::SeqCst); // 模拟「没有作业，却占着作业位」
        cancel();
        assert!(cancel_flag().load(Ordering::SeqCst), "cancel() 仍应置取消位");
        assert!(
            !busy_flag().load(Ordering::SeqCst),
            "★ 无在跑作业时 cancel() 必须释放作业位（用户能自救，不必重启进程）"
        );

        // ── ★ cancel() 自愈②：作业存在但**不在 Running**（如已 done）同样释放 ──
        set_job_for_test(job_of(ProbePhase::Done, Vec::new()));
        busy_flag().store(true, Ordering::SeqCst);
        cancel();
        assert!(
            !busy_flag().load(Ordering::SeqCst),
            "★ 作业已结束（phase=done）时 cancel() 也应释放作业位"
        );

        // ── ★ cancel() 红线：作业**正在跑**（phase=running）时**绝不能**释放 ──
        set_job_for_test(job_of(ProbePhase::Running, Vec::new()));
        busy_flag().store(true, Ordering::SeqCst);
        cancel();
        assert!(
            busy_flag().load(Ordering::SeqCst),
            "★ 作业在跑时 cancel() 不得释放作业位（否则第二次 start 会与它并发写同一槽）"
        );

        // ── ★ run() 的 RAII 归位器：作业槽为空 ⇒ 走早退分支，返回时也必须释放作业位 ──
        //
        // 用**内联单线程 runtime** 跑 `run`（而不是另开一条 `#[tokio::test]`）：这三个是
        // **进程级静态**，只允许本测试碰它们；另开一条会在并行 runner 下与本条互相
        // reset/cancel —— 那正是 QA 点名的真实 flake 窗口。
        reset_for_test(); // 取消位 = false、作业槽 = None
        busy_flag().store(true, Ordering::SeqCst); // 模拟「run 已抢占作业位」
        {
            let state = crate::state::GatewayState::new(crate::GatewayConfig::default());
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .build()
                .expect("建单线程 runtime");
            // 作业槽为空 ⇒ `run` 走早退分支（不碰 target、不发请求）。
            runtime.block_on(run(ProbeTarget::Wb(state, Region::Cn)));
        }
        assert!(
            !busy_flag().load(Ordering::SeqCst),
            "★ run() 退出（含早退 / panic 展开）必须经 RAII guard 释放作业位"
        );

        reset_for_test();
        assert!(!cancel_flag().load(Ordering::SeqCst), "复位后必须能再跑一轮");
        assert!(!busy_flag().load(Ordering::SeqCst));
        assert_eq!(status()["phase"], json!("idle"));
    }

    /// ★★ **接线护栏**：两个宿主、四处的调用点都在。
    ///
    /// 形态照抄 [`crate::catalog_probe`] 的 `both_hosts_wire_the_catalog_probe` —— 本仓对
    /// 「**函数存在、类型全对、构建全绿、单测全绿，但生产上没人调用**」那类事故的标准守卫。
    ///
    /// ## 为什么必须单独有一条
    ///
    /// 命令面分散在四处（`api.ts` 的 ROUTES / `api.rs` 的路由与 handler / `commands.rs` 的实现 /
    /// `lib.rs` 的登记）。`check-api-contract.cjs` 只管「**名字**四处一致」，管不了**语义**上最要命
    /// 的那一条：**`start` 之后必须 spawn `run`** —— 少这一句，作业位（原子抢占）永远不释放，
    /// 之后每一次「开始探测」都只报「已有探测在进行中」，而**构建与门禁全绿**。
    ///
    /// ## 标记一律带**实参**（不是裸函数名）
    ///
    /// 裸 `model_probe::start` 在**定义行**（`pub async fn start(`）也会命中 ⇒ 删掉调用点后
    /// 护栏会**空转**。故断言全部带实参 / 路径 / 方法形态。
    ///
    /// ## 变异可证伪
    ///
    /// 任一处删除即红：删 `commands.rs` 的 `start(...)`、或那行 `spawn(...)`；删 `lib.rs` 里
    /// 三个 `commands::probe_*` 任一；删 `api.rs` 的三条 `.route(...)`、或把 `post` 写成 `get`、
    /// 或删那行 `spawn`。
    #[test]
    fn both_hosts_wire_every_probe_exit() {
        const TAURI_COMMANDS_RS: &str =
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../src-tauri/src/commands.rs");
        const TAURI_LIB_RS: &str =
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../src-tauri/src/lib.rs");
        const SERVER_API_RS: &str =
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../crates/buddy-switch-server/src/api.rs");

        let read = |path: &str| {
            std::fs::read_to_string(path).unwrap_or_else(|error| panic!("读不到 {path}: {error}"))
        };

        // ① 桌面端：三个出口 + ★ 那条最容易漏的 `spawn(run)`。
        let desktop = read(TAURI_COMMANDS_RS);
        for (marker, why) in [
            (
                "model_probe::target_for_scope(&scope)",
                "桌面端 `probe_start` 没有按作用域构造探测目标（fail-closed 入口空转）",
            ),
            (
                "model_probe::start(&target, uids, models, adopt)",
                "桌面端 `probe_start` 没有真的启动探测",
            ),
            (
                "tauri::async_runtime::spawn(buddy_switch_gateway::model_probe::run(target))",
                "★ 桌面端少了 spawn run —— 作业位会永久占住，之后每次探测都报「已有探测在进行中」",
            ),
            ("model_probe::status()", "桌面端 `probe_status` 没有接线"),
            ("model_probe::cancel()", "桌面端 `probe_cancel` 没有接线"),
        ] {
            assert!(
                desktop.contains(marker),
                "{TAURI_COMMANDS_RS} 缺 `{marker}`（{why}）"
            );
        }

        // ② 桌面端登记表：漏登记 ⇒ invoke 报 `command not found`（而构建全绿）。
        let desktop_lib = read(TAURI_LIB_RS);
        for name in [
            "commands::probe_start",
            "commands::probe_status",
            "commands::probe_cancel",
        ] {
            assert!(
                desktop_lib.contains(name),
                "{TAURI_LIB_RS} 的 `generate_handler!` 缺 `{name}` —— 桌面端会报 command not found"
            );
        }

        // ③ 服务端：**连通 HTTP 方法一起断言**（method 写反会打到错的 handler，且构建全绿）。
        let server = read(SERVER_API_RS);
        for marker in [
            r#".route("/api/gateway/probe/start", post(api_probe_start))"#,
            r#".route("/api/gateway/probe/status", get(api_probe_status))"#,
            r#".route("/api/gateway/probe/cancel", post(api_probe_cancel))"#,
            "model_probe::target_for_scope(&scope)",
            "model_probe::start(&target, uids, models, adopt)",
            "tokio::spawn(buddy_switch_gateway::model_probe::run(target))",
        ] {
            assert!(server.contains(marker), "{SERVER_API_RS} 缺 `{marker}`");
        }
    }
}
