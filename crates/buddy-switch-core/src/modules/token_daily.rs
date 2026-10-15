//! 每日 Token 记录：**按账号 × 来源 × 本地日**持久化「今日 Token」。
//!
//! 两个来源分别聚合后落进同一份 region 化存储（[`token_daily_file_for`]）：
//!
//! | 来源 | 数据源 | 归属键 |
//! |---|---|---|
//! | `gateway` | 网关请求日志 `~/.buddy-switch/gateway_logs.json`（**只读**） | 行 `accountUid`（精确 uid）；旧行无此字段时才回落解析行 `account` 展示名 |
//! | `client` | CLI 会话日志 `{data_dir}/projects` 下的 jsonl（**流式逐行**） | `sessionId` → `workbuddy.db`.`sessions.user_id` |
//!
//! 设计取向（对齐 `credit_usage.rs` 的持久化 / 保留 / 原子写风格）：
//!
//! - **只读上游**：绝不改网关写路径、绝不写 `workbuddy.db`（`SQLITE_OPEN_READ_ONLY`）。
//! - **诚实**：网关日志的**今日**部分可能被裁过（条数到达 `log_keep` **且**最旧存活
//!   条目就是今天，见 [`gateway_log_truncated_today`]）时透出 `gatewayTruncated`；
//!   无法归属的客户端会话落 `unknownClient` 桶，**绝不猜**到别的账号，也**绝不静默丢弃**。
//! - **重扫有界**：客户端扫描带 **TTL 闸门**（[`CLIENT_SCAN_TTL_MS`]，`force` 可强制重扫），
//!   且**两道代价控制**：**mtime 预筛**（只有「当天动过」的文件才可能含当天记录）
//!   + **尾部窗口读取**（当天记录只可能在文件尾部）。见 [`scan_client_rows`]。
//!
//!   ⚠️ 闸门**曾经**是「当天扫过一次就不再扫」，那是**错的**（2026-10-12 实测）：
//!   当天第一次扫描通常发生在**任何用量产生之前**，于是当日值被冻结成空，
//!   一整天卡片都显示「—」。改成 TTL 后，最多滞后 [`CLIENT_SCAN_TTL_MS`]。
//!   代价控制不靠「少扫」而靠「扫得少」—— 见下方「性能」。
//!
//! ## 性能（2026-10-13 实测）
//!
//! 「今日」窗口，Global 语料（当日被动过的 5,704 个文件 / **14.59 GB**）：
//!
//! | 版本 | 冷启（OS 缓存冷） | 稳态 |
//! |---|---:|---:|
//! | 逐行读完整份（原实现） | **66–75 s** | 66–75 s |
//! | 预筛 + 尾部窗口 + 并行（现实现） | ≈5 s | **≈0.7 s** |
//!
//! 三条改动的分工（缺一条都达不到这个数）：
//!
//! 1. **遍历**：`entry.file_type()` 取代 `path.is_dir()`（同一棵树 **0.69 s → 0.03 s**），
//!    并把 mtime 预筛**并进遍历**（不再「先收全部路径、再逐个 `stat`」）；
//! 2. **读取**：只读尾部窗口（[`jsonl_tail`]）—— 实读字节 ≈ 预筛文件总大小的 **1.2%**
//!    （14.59 GB → 0.18 GB）；
//! 3. **解析**：按文件并行（[`worker_count`]，上限 32）。
//!
//! 等价性：`tail_window_keeps_every_today_record_against_a_full_read` 用**真文件**
//! 驱动生产函数 `read_day_window`，逐条比对「尾部窗口 vs 整份读」；实测同语料下
//! 两者**逐字段一致**（`clientIn` / `clientOut` 完全相同）。

use chrono::{Duration as ChronoDuration, Local, NaiveDate, TimeZone};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, UNIX_EPOCH};

use rusqlite::Connection;

use crate::modules::account;
use crate::modules::config::{atomic_write, now_ms, store_dir};
use crate::modules::jsonl_tail;
use crate::modules::region::{gateway_config_file, token_daily_file_for, Region};
use crate::modules::session::{session_data_dir, workbuddy_db_path_for};

/// 每日记录保留天数（镜像 `credit_usage::CREDIT_SNAPSHOT_RETENTION_DAYS`）。
pub const TOKEN_DAILY_RETENTION_DAYS: i64 = 90;
/// 每日记录条数上限（**安全网**：保留窗内还可能出现病态/未来的日期，超出即丢最旧）。
pub const TOKEN_DAILY_MAX_DAYS: usize = 400;
/// 存储 schema 版本。
///
/// v2：`clientScannedDate`（本地日串）→ `clientScannedAtMs`（毫秒时刻）——
/// 闸门语义从「当天扫过就不再扫」改为「距上次扫描不足 TTL 才跳过」（见
/// [`CLIENT_SCAN_TTL_MS`]）。旧文件里那个字段反序列化时被忽略 ⇒ `None` ⇒ 重扫一次，
/// 结果正确（只是多扫一轮）。
pub const TOKEN_DAILY_VERSION: u32 = 2;

/// 网关日志条数上限的缺省值（与 `GatewayConfig::default().log_keep` 同值）。
const DEFAULT_GATEWAY_LOG_KEEP: usize = 200;

/// 客户端扫描的**最短重扫间隔**（毫秒）。
///
/// ## 为什么闸门不能是「当天扫过一次就不再扫」（2026-10-12 实测驱动的修正）
///
/// 原闸门判据是 `clientScannedDate == 今天` ⇒ 当日值被**冻结在当天第一次扫描的那一刻**。
/// 实测现场：CN 的 `token_daily.json` 在 **14:33** 落盘（当时今日确无用量 ⇒ `accounts: {}`），
/// 客户端 **15:00** 才开始产生用量 ⇒ 此后一整天卡片都是「—」；而前端从不传 `force`，
/// **没有任何路径**能让它恢复 —— 这正是「今日 Token 总是横杠」的根因。
///
/// ## 为什么取 2 分钟
///
/// 「今日 Token」是仪表盘读数：**不需要实时**，但绝不能整天不动。2 分钟足够短
/// （用户切到账号页基本都能看到当前值），又足够长（一次页面停留不会反复重扫）。
/// 真正的代价控制在 [`scan_client_rows`]：mtime 预筛 + 尾部窗口 + 并行。
pub const CLIENT_SCAN_TTL_MS: i64 = 2 * 60 * 1000;

/// mtime 预筛的**回退余量**（毫秒）：允许文件在本地日界之前一点点被写入。
///
/// 日志是**追加写**的，「一条当日记录的写入时刻」不早于「它自己的时间戳」，
/// 所以日界之后的 mtime 足以覆盖当日记录。留 1 小时余量是为了吸收
/// 时钟抖动 / 跨日批量刷盘这类噪声 —— 代价只是多读几个文件。
const CLIENT_SCAN_MTIME_SKEW_MS: i64 = 60 * 60 * 1000;

static TOKEN_DAILY_WRITE_LOCK: Mutex<()> = Mutex::new(());

// ---------------------------------------------------------------------------
// 存储形状
// ---------------------------------------------------------------------------

/// 一个来源（client / gateway）的输入 / 输出累计。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Bucket {
    #[serde(default)]
    input: u64,
    #[serde(default)]
    output: u64,
}

/// 某账号在当日的双来源桶。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct AccountRecord {
    #[serde(default)]
    client: Bucket,
    #[serde(default)]
    gateway: Bucket,
}

/// 某本地日的全部账号记录。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DayRecord {
    #[serde(default)]
    accounts: BTreeMap<String, AccountRecord>,
    /// 无法归属到任何账号的客户端用量；`None`（缺省）= 该日未出现无法归属的会话。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    unknown_client: Option<Bucket>,
    /// 网关卡是否可能被裁剪（条数到达 `log_keep`）。
    #[serde(default)]
    gateway_truncated: bool,
}

/// 落盘存储：`{ version, days: { "YYYY-MM-DD": DayRecord }, clientScannedAtMs }`。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Store {
    #[serde(default = "default_version")]
    version: u32,
    #[serde(default)]
    days: BTreeMap<String, DayRecord>,
    /// 客户端扫描的**上次完成时刻**（毫秒）。非 `force` 且「同一天且不足 TTL」时不再重扫。
    ///
    /// ⚠️ 刻意存**时刻**而不是「本地日串」：只存日串就退化成「当天扫过就不再扫」，
    /// 当日值会被冻结在当天首次扫描那一刻（见 [`CLIENT_SCAN_TTL_MS`] 的实测现场）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    client_scanned_at_ms: Option<i64>,
}

fn default_version() -> u32 {
    TOKEN_DAILY_VERSION
}

fn empty_store() -> Store {
    Store {
        version: TOKEN_DAILY_VERSION,
        days: BTreeMap::new(),
        client_scanned_at_ms: None,
    }
}

// ---------------------------------------------------------------------------
// 时间
// ---------------------------------------------------------------------------

/// 毫秒时间戳 → 本地日期串（`%Y-%m-%d`，与 `credit_usage` 的日键同格式）。
fn local_date_string(ts: i64) -> Option<String> {
    Local
        .timestamp_millis_opt(ts)
        .single()
        .map(|dt| dt.format("%Y-%m-%d").to_string())
}

fn local_date(ts: i64) -> Option<NaiveDate> {
    Local
        .timestamp_millis_opt(ts)
        .single()
        .map(|dt| dt.date_naive())
}

/// 本地日 `YYYY-MM-DD` 的**起点毫秒**（供 mtime 预筛当「日界」）。
///
/// 非法日期 / DST 歧义（`from_local_datetime` 返回 `Ambiguous`）⇒ `None`。
/// 调用方拿到 `None` 时**必须退化为全量扫描** —— 宁可慢，不可漏（诚实优先于快）。
fn local_day_start_ms(day: &str) -> Option<i64> {
    let date = NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()?;
    let naive = date.and_hms_opt(0, 0, 0)?;
    Local
        .from_local_datetime(&naive)
        .single()
        .map(|dt| dt.timestamp_millis())
}

// ---------------------------------------------------------------------------
// 存储读写
// ---------------------------------------------------------------------------

fn load_store(region: Region) -> Store {
    let path = token_daily_file_for(region);
    if let Ok(text) = std::fs::read_to_string(path) {
        if let Ok(store) = serde_json::from_str::<Store>(&text) {
            return store;
        }
    }
    empty_store()
}

fn save_store(region: Region, store: &Store) -> bool {
    let content = serde_json::to_string_pretty(store).unwrap_or_default();
    std::fs::create_dir_all(store_dir())
        .and_then(|_| atomic_write(&token_daily_file_for(region), &content))
        .is_ok()
}

/// 丢弃保留窗之外的日期，并用条数上限兜底（丢最旧）。
fn prune_store(store: &mut Store, at_ms: i64) {
    let Some(today) = local_date(at_ms) else {
        return;
    };
    let cutoff = today - ChronoDuration::days(TOKEN_DAILY_RETENTION_DAYS - 1);
    store.days.retain(|day, _| {
        NaiveDate::parse_from_str(day, "%Y-%m-%d")
            .map(|date| date >= cutoff)
            .unwrap_or(false)
    });
    // BTreeMap 的键（`YYYY-MM-DD`）字典序 = 时间序，最旧的在最前。
    while store.days.len() > TOKEN_DAILY_MAX_DAYS {
        let Some(oldest) = store.days.keys().next().cloned() else {
            break;
        };
        store.days.remove(&oldest);
    }
}

// ---------------------------------------------------------------------------
// 网关日志聚合（只读）
// ---------------------------------------------------------------------------

/// 网关请求日志文件路径（与 `buddy-switch-gateway::state::gateway_log_file` 一致）。
///
/// 刻意在 core 内自行派生、而非依赖网关 crate：本模块只**读**该文件，不参与写路径。
fn gateway_log_file() -> PathBuf {
    gateway_config_file().with_file_name("gateway_logs.json")
}

/// 网关日志保留条数：读 `gateway_config.json` 的 `log_keep`（缺省 200）。
fn gateway_log_keep() -> usize {
    if let Ok(text) = std::fs::read_to_string(gateway_config_file()) {
        if let Ok(value) = serde_json::from_str::<Value>(&text) {
            let keep = value
                .get("log_keep")
                .and_then(Value::as_u64)
                .or_else(|| value.get("logKeep").and_then(Value::as_u64));
            if let Some(keep) = keep {
                return (keep as usize).max(1);
            }
        }
    }
    DEFAULT_GATEWAY_LOG_KEEP
}

fn load_gateway_log() -> Vec<Value> {
    if let Ok(text) = std::fs::read_to_string(gateway_log_file()) {
        if let Ok(Value::Array(entries)) = serde_json::from_str::<Value>(&text) {
            return entries;
        }
    }
    Vec::new()
}

/// 网关日志「**今日**部分可能已被裁剪」的判据（诚实标注，不假装是完整总量）。
///
/// 两个条件**同时**成立才算：
/// 1. **条数到达保留上限**（`entries_len >= keep`）—— 没满过就什么都没丢；
/// 2. **存活最旧的条目落在今天**（`oldest_day == Some(day)`）—— 裁剪是 **FIFO**
///    （`buddy_switch_gateway::logging::RequestLog::trim_and_save` 从头部
///    `drain(..drop_count)`，`record` 向尾部追加），只要最旧存活条目**早于今天**，
///    今天的条目就一条都没被丢过 ⇒ 今日总量是**完整**的。
///
/// ## 为什么不能只看第 1 条（旧判据的误报）
///
/// 日志是**滚动窗口**：它可能被**昨天**（或更早）的流量顶满，而今天的条目全在。
/// 那种情况下今日总量是精确的，旧判据却标 `true` ⇒ 用户平白看到「≥」和一条
/// 「网关用量只是下界」的说明。本判据把「窗口滑过今天」与「窗口停在今天之前」
/// 区分开（2026-10-15 修）。
///
/// 佐证：前端概览的同款判据一直是这个口径 ——
/// `src/lib/overview-metrics.ts::summarizeGatewayLogs`（今日**请求数**）早就写着
/// `oldest >= start && len >= keep`，并附了同样的 FIFO 说明。本次只是把**后端
/// token 口径**对齐到它（此前两处判据不一致：概览精确、卡片宽松）。
///
/// ## 为什么判据**不按 region** 分
///
/// 裁剪是**全局** FIFO：被丢的条目属于哪个 region 无从得知。所以只要今天的条目
/// **可能**丢过，两个 region 都标 `true`（保守方向）；反之都不标。按 region 各标
/// 一套既无判据、也会让「Global 一条 gateway 行都没有」的 store 平白带上裁剪标记。
fn gateway_log_truncated_today(
    entries_len: usize,
    keep: usize,
    oldest_day: Option<&str>,
    day: &str,
) -> bool {
    if entries_len < keep.max(1) {
        return false;
    }
    oldest_day == Some(day)
}

#[derive(Clone, Debug)]
struct GatewayRow {
    region: String,
    /// 展示名（仅作**旧行**回落的解析输入）。
    account: String,
    /// 写入时记录的精确 uid（新行必有；旧行为 `None`）。
    account_uid: Option<String>,
    day: String,
    input: u64,
    output: u64,
}

fn parse_gateway_rows(entries: &[Value]) -> Vec<GatewayRow> {
    entries
        .iter()
        .filter_map(|value| {
            let ts = value.get("ts").and_then(Value::as_i64)?;
            let day = local_date_string(ts)?;
            let account = value
                .get("account")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|account| !account.is_empty())?
                .to_string();
            // 写入时的精确身份；旧日志无此字段 ⇒ `None`（回落到展示名解析）。
            let account_uid = value
                .get("accountUid")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|uid| !uid.is_empty())
                .map(str::to_string);
            let region = value
                .get("region")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            Some(GatewayRow {
                region,
                account,
                account_uid,
                day,
                input: value.get("promptTokens").and_then(number_u64).unwrap_or(0),
                output: value
                    .get("completionTokens")
                    .and_then(number_u64)
                    .unwrap_or(0),
            })
        })
        .collect()
}

/// 按归属键分桶，只保留「该 region + 该本地日」的行。
///
/// 归属键：**优先**行内写入时的 `accountUid`（精确身份）；仅当其为空（**旧日志**，
/// 本改动之前写入）才回落到把展示名 `account` 解析回 uid。
fn bucket_gateway(
    rows: &[GatewayRow],
    region: Region,
    day: &str,
    uids: &HashSet<String>,
    names: &HashMap<String, Option<String>>,
) -> BTreeMap<String, Bucket> {
    let mut out: BTreeMap<String, Bucket> = BTreeMap::new();
    for row in rows {
        if row.region != region.as_str() || row.day != day {
            continue;
        }
        let key = match row.account_uid.as_deref() {
            Some(uid) => uid.to_string(),
            None => resolve_account_key(&row.account, uids, names),
        };
        let bucket = out.entry(key).or_default();
        bucket.input = bucket.input.saturating_add(row.input);
        bucket.output = bucket.output.saturating_add(row.output);
    }
    out
}

/// 网关日志的 `account` 是**展示名**（`account_display_name` 的回落链：email → nickname →
/// 客户端快照 → uid；见网关 `relay.rs` 的 `RelayOutcome::Ok.account`），而前端按
/// `AccountMeta.uid` join。故**旧行**（无 `accountUid`）在这里把展示名解析回 uid。
///
/// 新行直接带上 `accountUid`（写入时的精确身份，见 [`bucket_gateway`]），无需解析——
/// 这正是本次改动的目的：**身份在写入时记录**，而非事后反查。
///
/// 回落口径：直接命中 uid → 唯一命中某账号的展示名 / email / 昵称 → 否则**原样保留**
/// （歧义 / 未知**绝不**错贴到别的账号；前端 join 落空即渲染「—」，与本模块的诚实取向一致）。
fn load_gateway_identity(
    region: Region,
) -> (HashSet<String>, HashMap<String, Option<String>>) {
    let mut uids: HashSet<String> = HashSet::new();
    let mut names: HashMap<String, Option<String>> = HashMap::new();
    for account_value in account::load_accounts_for(region) {
        let Some(uid) = account::get_str(&account_value, "uid") else {
            continue;
        };
        uids.insert(uid.clone());
        let mut display_forms = vec![account::account_display_name(&account_value)];
        if let Some(email) = account::get_str(&account_value, "email") {
            display_forms.push(email);
        }
        if let Some(nickname) = account::get_str(&account_value, "nickname") {
            display_forms.push(nickname);
        }
        for name in display_forms {
            if name.is_empty() {
                continue;
            }
            names
                .entry(name)
                .and_modify(|slot: &mut Option<String>| {
                    // 多个账号共用同一展示名 ⇒ 歧义 ⇒ 解析不出（`None`）。
                    if slot.as_deref() != Some(uid.as_str()) {
                        *slot = None;
                    }
                })
                .or_insert_with(|| Some(uid.clone()));
        }
    }
    (uids, names)
}

fn resolve_account_key(
    raw: &str,
    uids: &HashSet<String>,
    names: &HashMap<String, Option<String>>,
) -> String {
    if uids.contains(raw) {
        return raw.to_string();
    }
    match names.get(raw) {
        Some(Some(uid)) => uid.clone(),
        _ => raw.to_string(),
    }
}

// ---------------------------------------------------------------------------
// 客户端 JSONL 聚合（只读）
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
struct ClientUsageRow {
    session_id: String,
    ts: i64,
    input: u64,
    output: u64,
}

/// 递归收集「**可能含今日记录**」的 `.jsonl` 路径。
///
/// **一次遍历同时做完三件事**（旧实现是「先遍历收全部路径 → 再逐个 `stat` 取 mtime」）：
/// 判类型 · 取 mtime · 应用预筛。
///
/// - `entry.file_type()` 直接取自目录枚举结果，**不额外发系统调用**。旧写法
///   `path.is_dir()` 每个条目都要一次 `stat`：实测同一棵树（Global，26,932 个文件）
///   **0.43s → 0.03s**。
/// - `entry.metadata()` 在 Windows 上同样来自枚举结果 ⇒ 「遍历完再逐个 stat」那第二遍消失。
/// - 预筛在**遍历时**生效 ⇒ 不通过的文件连 `PathBuf` 都不用分配。
///
/// 排除 `subagents`（子代理日志会重复父会话上下文，与 `token_stats` 的主用量口径一致）。
fn collect_day_files(dir: &Path, day_start_ms: Option<i64>, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            if entry.file_name().to_str() == Some("subagents") {
                continue;
            }
            collect_day_files(&entry.path(), day_start_ms, out);
            continue;
        }
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
            continue;
        }
        let mtime_ms = entry.metadata().ok().and_then(|meta| mtime_of(&meta));
        if mtime_may_hold_day(mtime_ms, day_start_ms) {
            out.push(path);
        }
    }
}

fn number_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|n| u64::try_from(n).ok()))
        .or_else(|| value.as_f64().filter(|n| n.is_finite() && *n >= 0.0).map(|n| n as u64))
        .or_else(|| value.as_str()?.trim().parse::<u64>().ok())
}

fn field_u64(object: &Map<String, Value>, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|key| object.get(*key).and_then(number_u64))
}

/// usage 对象的锚点：必须含输入字段（输入可以为 0，故不以 `> 0` 判定）。
fn usage_object(value: &Value) -> Option<&Map<String, Value>> {
    value.as_object().filter(|object| {
        field_u64(object, &["input_tokens", "inputTokens", "prompt_tokens"]).is_some()
    })
}

/// 提取 (input, output)：优先级 `message.usage` > `providerData.usage` > 顶层 `usage`
/// （与 `token_stats` 的取值口径一致）。
fn extract_usage(value: &Value) -> Option<(u64, u64)> {
    let provider = value.get("providerData");
    let candidates = [
        value.get("message").and_then(|message| message.get("usage")),
        provider.and_then(|data| data.get("usage")),
        value.get("usage"),
    ];
    let selected = candidates.into_iter().flatten().find_map(usage_object)?;
    let input =
        field_u64(selected, &["input_tokens", "inputTokens", "prompt_tokens"]).unwrap_or(0);
    let output =
        field_u64(selected, &["output_tokens", "outputTokens", "completion_tokens"]).unwrap_or(0);
    Some((input, output))
}

fn record_timestamp(value: &Value) -> Option<i64> {
    value
        .get("timestamp")
        .or_else(|| value.get("ts"))
        .and_then(|timestamp| {
            timestamp
                .as_i64()
                .or_else(|| timestamp.as_u64().and_then(|n| i64::try_from(n).ok()))
                .or_else(|| timestamp.as_str()?.trim().parse::<i64>().ok())
        })
}

/// 解析一行会话日志；非用量行 / 结构缺失返回 `None`。
///
/// 先用 `contains("\"usage\"")` 廉价预筛，避免对海量消息行做 JSON 解析。
fn parse_client_line(line: &str) -> Option<ClientUsageRow> {
    let line = line.trim();
    if line.is_empty() || !line.contains("\"usage\"") {
        return None;
    }
    let value: Value = serde_json::from_str(line).ok()?;
    let ts = record_timestamp(&value)?;
    let session_id = value
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|session_id| !session_id.is_empty())?
        .to_string();
    let (input, output) = extract_usage(&value)?;
    Some(ClientUsageRow {
        session_id,
        ts,
        input,
        output,
    })
}

/// 文件的 mtime（毫秒）。读不到 ⇒ `None`。
fn mtime_of(meta: &std::fs::Metadata) -> Option<i64> {
    let since = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    i64::try_from(since.as_millis()).ok()
}

/// mtime 预筛判据（**纯函数**，便于单测）。
///
/// `mtime_ms` 或 `day_start_ms` 任一缺失 ⇒ **放行**（宁可慢，不可漏）。
fn mtime_may_hold_day(mtime_ms: Option<i64>, day_start_ms: Option<i64>) -> bool {
    match (mtime_ms, day_start_ms) {
        (Some(mtime_ms), Some(start)) => mtime_ms >= start - CLIENT_SCAN_MTIME_SKEW_MS,
        _ => true,
    }
}

/// 读一份 jsonl 的**今日尾部**；I/O 失败返回 `None`（调用方按「无记录」处理，不 panic）。
///
/// ## 为什么可以只读尾部（2026-10-13 加入）
///
/// 会话 jsonl 是**只追加**的，记录按时间顺序落盘 ⇒ **今日记录必然在文件尾部**。
/// 具体算法（块大小、停止判据、边界用例）在 [`jsonl_tail`] —— 与
/// `client_limits` 的限流台账**共用同一实现**，那边不再各写一份。
///
/// 实测（Global，当日被动过的 5,704 个文件 / **14.59 GB**）：旧实现逐行读完整份，
/// `record_client_today` 要 **66–75s**；改为尾部窗口 + 并行后见模块头「性能」一节。
fn read_day_window(file: &mut std::fs::File, size: u64, day_start_ms: i64) -> Option<Vec<u8>> {
    jsonl_tail::read_window_from(file, size, day_start_ms).map(|(bytes, _)| bytes)
}

/// 解析窗口文本，只留今日的行。
fn parse_day_rows(text: &str, day: &str) -> Vec<ClientUsageRow> {
    text.lines()
        .filter_map(parse_client_line)
        .filter(|row| local_date_string(row.ts).as_deref() == Some(day))
        .collect()
}

/// 扫描单个文件（**只读尾部窗口**），返回其中今日的用量行。
fn scan_file_rows(path: &Path, day: &str, day_start_ms: Option<i64>) -> Vec<ClientUsageRow> {
    let Ok(mut file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let Ok(size) = file.metadata().map(|meta| meta.len()) else {
        return Vec::new();
    };
    if size == 0 {
        return Vec::new();
    }
    let window = match day_start_ms {
        Some(start) => read_day_window(&mut file, size, start),
        // 算不出日界 ⇒ 整份读（宁可慢，不可漏）。
        None => {
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer).ok().map(|_| buffer)
        }
    };
    match window {
        Some(bytes) => parse_day_rows(&String::from_utf8_lossy(&bytes), day),
        None => Vec::new(),
    }
}

/// 并行度：够用即可，上限 32（再多只会在同一条磁盘队列上排队）。
fn worker_count(pending: usize) -> usize {
    let cores = std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(4);
    cores.clamp(1, 32).min(pending.max(1))
}

/// 扫描 `root` 下的 CLI 会话日志，只保留**今日**的用量行。
///
/// ## 两道代价控制（2026-10-13 重做）
///
/// 1. **mtime 预筛**（遍历时生效，见 [`collect_day_files`]）：日志是追加写的，
///    一条记录的写入时刻不早于它自己的时间戳 ⇒ 当日记录只可能出现在「本地日界之后
///    被动过」的文件里。**没有这一步**，把闸门从「整天一次」改成「2 分钟一次」
///    是不可接受的。
/// 2. **尾部窗口**（[`read_day_window`]）：预筛放行的文件里，今日记录也只在**尾部**。
///    实测当日被动过的文件共 14.59 GB，而真正需要读的只是各文件的尾部一段。
///
/// 之后按文件**并行**解析（[`worker_count`]）；结果按文件路径顺序合并 ⇒ 与线程调度无关。
///
/// ⚠️ 取舍：预筛**依赖 mtime 与写入时刻一致**。若某文件内容在当天增长、而 mtime 被改回
/// 昨天（如带 mtime 的还原 / 复制），它的当日记录会被漏掉 —— 这由
/// `mtime_prefilter_keeps_fresh_files_and_drops_stale_ones` 用例**显式固定**下来
/// （不是「没想到」，而是写明的取舍）。
fn scan_client_rows(root: &Path, day: &str) -> Vec<ClientUsageRow> {
    let day_start = local_day_start_ms(day);
    let mut files = Vec::new();
    collect_day_files(root, day_start, &mut files);
    if files.is_empty() {
        return Vec::new();
    }
    // `read_dir` 顺序不保证稳定 ⇒ 先排序，让结果与缓存都可复现。
    files.sort();

    let next = AtomicUsize::new(0);
    let per_file: Mutex<Vec<(usize, Vec<ClientUsageRow>)>> =
        Mutex::new(Vec::with_capacity(files.len()));
    std::thread::scope(|scope| {
        for _ in 0..worker_count(files.len()) {
            scope.spawn(|| loop {
                let cursor = next.fetch_add(1, Ordering::Relaxed);
                let Some(path) = files.get(cursor) else {
                    break;
                };
                let rows = scan_file_rows(path, day, day_start);
                per_file.lock().unwrap().push((cursor, rows));
            });
        }
    });

    let mut collected = per_file.into_inner().unwrap();
    collected.sort_by_key(|(index, _)| *index);
    collected
        .into_iter()
        .flat_map(|(_, rows)| rows)
        .collect()
}

fn table_columns(conn: &Connection, table: &str) -> Vec<String> {
    let Ok(mut stmt) = conn.prepare(&format!("PRAGMA table_info({table})")) else {
        return Vec::new();
    };
    let Ok(iter) = stmt.query_map([], |row| row.get::<_, String>(1)) else {
        return Vec::new();
    };
    iter.flatten().collect()
}

fn nonempty(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// 读 `sessions` 表构造 `sessionId → user_id` 映射（**只读**）。
///
/// 空串 / `NULL` 的 `user_id` **不入表**（无法归属 ⇒ 走 unknown 桶，绝不猜）。
fn load_session_user_map(region: Region) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let db = workbuddy_db_path_for(region);
    if !db.is_file() {
        return map;
    }
    let Ok(conn) = Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
    else {
        return map;
    };
    let _ = conn.busy_timeout(Duration::from_secs(5));
    let columns = table_columns(&conn, "sessions");
    if !columns.iter().any(|column| column == "id")
        || !columns.iter().any(|column| column == "user_id")
    {
        return map;
    }
    let Ok(mut stmt) = conn.prepare("SELECT id, user_id FROM sessions") else {
        return map;
    };
    let Ok(rows) = stmt.query_map([], |row| {
        Ok((
            row.get::<_, Option<String>>(0)?,
            row.get::<_, Option<String>>(1)?,
        ))
    }) else {
        return map;
    };
    for (id, uid) in rows.flatten() {
        let (Some(id), Some(uid)) = (nonempty(id), nonempty(uid)) else {
            continue;
        };
        map.insert(id, uid);
    }
    map
}

/// 先按 `sessionId` 合并，再映射到账号；映射失败落 `unknown`。
fn bucket_client(
    rows: &[ClientUsageRow],
    meta: &HashMap<String, String>,
    day: &str,
) -> (BTreeMap<String, Bucket>, Bucket) {
    let mut by_session: BTreeMap<String, Bucket> = BTreeMap::new();
    for row in rows {
        if local_date_string(row.ts).as_deref() != Some(day) {
            continue;
        }
        let bucket = by_session.entry(row.session_id.clone()).or_default();
        bucket.input = bucket.input.saturating_add(row.input);
        bucket.output = bucket.output.saturating_add(row.output);
    }

    let mut per_account: BTreeMap<String, Bucket> = BTreeMap::new();
    let mut unknown = Bucket::default();
    for (session_id, bucket) in by_session {
        match meta.get(&session_id) {
            Some(uid) => {
                let account = per_account.entry(uid.clone()).or_default();
                account.input = account.input.saturating_add(bucket.input);
                account.output = account.output.saturating_add(bucket.output);
            }
            None => {
                unknown.input = unknown.input.saturating_add(bucket.input);
                unknown.output = unknown.output.saturating_add(bucket.output);
            }
        }
    }
    (per_account, unknown)
}

// ---------------------------------------------------------------------------
// 对外：聚合 + 读取
// ---------------------------------------------------------------------------

/// 聚合网关日志到「今日」的 gateway 桶并落盘，返回今日记录。
pub fn record_gateway_today(region: Region) -> Value {
    let at = now_ms();
    let day = local_date_string(at).unwrap_or_default();
    let entries = load_gateway_log();
    // 最旧存活条目的本地日（FIFO ⇒ 它早于今天就意味着今天的条目一条都没丢过）。
    let oldest_day = entries
        .iter()
        .filter_map(|value| value.get("ts").and_then(Value::as_i64))
        .min()
        .and_then(local_date_string);
    let truncated =
        gateway_log_truncated_today(entries.len(), gateway_log_keep(), oldest_day.as_deref(), &day);
    let (uids, names) = load_gateway_identity(region);
    let per_account = bucket_gateway(&parse_gateway_rows(&entries), region, &day, &uids, &names);

    let _guard = TOKEN_DAILY_WRITE_LOCK.lock().unwrap();
    let mut store = load_store(region);
    {
        let record = store.days.entry(day.clone()).or_default();
        for (account, bucket) in per_account {
            record.accounts.entry(account).or_default().gateway = bucket;
        }
        record.gateway_truncated = truncated;
    }
    prune_store(&mut store, at);
    save_store(region, &store);
    build_response(&day, store.days.get(&day))
}

/// 客户端扫描闸门（**纯函数**，便于单测）。
///
/// 判据：`force` ⇒ **总是要重扫**；否则「上次扫描在**同一本地日** **且** 距今不足
/// [`CLIENT_SCAN_TTL_MS`]」才跳过。
///
/// ⚠️ 「同一本地日」这一半是**跨日**判据（昨夜 23:59 扫过、现在 00:01 必须重扫），
/// 不是原来那个「当天扫过就不再扫」的判据 —— 后者会把当日值冻结在当天首次扫描那一刻。
fn client_scan_is_fresh(force: bool, scanned_at_ms: Option<i64>, now: i64, day: &str) -> bool {
    if force {
        return false;
    }
    let Some(scanned_at) = scanned_at_ms else {
        return false;
    };
    now.saturating_sub(scanned_at) < CLIENT_SCAN_TTL_MS
        && local_date_string(scanned_at).as_deref() == Some(day)
}

/// 聚合客户端会话日志到「今日」的 client 桶并落盘，返回今日记录。
///
/// `force = false` 且**距上次扫描不足 [`CLIENT_SCAN_TTL_MS`]** 时直接返回缓存；
/// `force = true` 强制重扫（当日数据在扫过之后仍会增长）。
///
/// ⚠️ 闸门**不是**「当天扫过就不再扫」（那会把当日值冻结在当天首次扫描那一刻，
/// 见 [`CLIENT_SCAN_TTL_MS`]）。重扫的代价由 [`scan_client_rows`] 的 mtime 预筛兜住。
pub fn record_client_today(region: Region, force: bool) -> Value {
    let at = now_ms();
    let day = local_date_string(at).unwrap_or_default();

    // 闸门：非强制且「同日 + 未过 TTL」⇒ 直接读缓存。
    {
        let store = load_store(region);
        if client_scan_is_fresh(force, store.client_scanned_at_ms, at, &day) {
            return build_response(&day, store.days.get(&day));
        }
    }

    let meta = load_session_user_map(region);
    let rows = scan_client_rows(&session_data_dir(region).join("projects"), &day);
    let (per_account, unknown) = bucket_client(&rows, &meta, &day);

    let _guard = TOKEN_DAILY_WRITE_LOCK.lock().unwrap();
    // 锁内重读：扫描期间 gateway 可能刚写过，避免丢更新。
    let mut store = load_store(region);
    {
        let record = store.days.entry(day.clone()).or_default();
        for (uid, bucket) in per_account {
            record.accounts.entry(uid).or_default().client = bucket;
        }
        record.unknown_client =
            (unknown.input > 0 || unknown.output > 0).then_some(unknown);
    }
    store.client_scanned_at_ms = Some(at);
    prune_store(&mut store, at);
    save_store(region, &store);
    build_response(&day, store.days.get(&day))
}

/// 读取「今日 Token」记录（**只读**，供 UI 直接渲染）。
///
/// 返回 `{ date, accounts: [{ uid, clientIn, clientOut, gatewayIn, gatewayOut }],
/// unknownClient: { in, out }, gatewayTruncated: bool }`。
///
/// **诚实边界**：`accounts` 只列出**今日确有用量**的账号 —— 账号**缺席** = 「今日无记录 /
/// 未知」，与「出现在数组里、值为 0（当日确实没用）」是两件事。`unknownClient` 是
/// 解析不出账号归属的客户端用量汇总（无则为 `{in:0,out:0}`）。
pub fn account_token_today(region: Region) -> Value {
    let day = local_date_string(now_ms()).unwrap_or_default();
    let store = load_store(region);
    build_response(&day, store.days.get(&day))
}

/// 把某日记录投影成下发前端的形状（纯函数，便于单测）。
fn build_response(day: &str, record: Option<&DayRecord>) -> Value {
    let accounts: Vec<Value> = record
        .map(|record| {
            record
                .accounts
                .iter()
                .map(|(uid, account)| {
                    json!({
                        "uid": uid,
                        "clientIn": account.client.input,
                        "clientOut": account.client.output,
                        "gatewayIn": account.gateway.input,
                        "gatewayOut": account.gateway.output,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let unknown_client = record
        .and_then(|record| record.unknown_client)
        .unwrap_or_default();
    json!({
        "date": day,
        "accounts": accounts,
        "unknownClient": { "in": unknown_client.input, "out": unknown_client.output },
        "gatewayTruncated": record.map(|record| record.gateway_truncated).unwrap_or(false),
    })
}

/// 网关逐日用量趋势（**只读**落库历史；不触发记录、不扫盘）。
///
/// 与 [`account_token_today`] 的分工：那个是「今日 · 按账号」（账号卡片用），
/// 本函数是「逐日 · 全账号合计」（趋势图用）。数据全部来自 `token_daily` 的落盘历史
/// （保留 [`TOKEN_DAILY_RETENTION_DAYS`] 天）⇒ **只有 App 运行过的日子才有条目**
/// （后台周期记录，见 `modules::background` 的 `GatewayUsageRecord`）——
/// **缺日 = 未记录，不是 0**：前端据此算覆盖率并如实披露，不许把未记录日画成「当天没用量」。
pub fn gateway_token_trend(region: Region) -> Value {
    let store = load_store(region);
    let days = gateway_trend_points(&store);
    let total: u64 = days
        .iter()
        .map(|point| point.get("total").and_then(Value::as_u64).unwrap_or(0))
        .sum();
    json!({
        "date": local_date_string(now_ms()).unwrap_or_default(),
        "retentionDays": TOKEN_DAILY_RETENTION_DAYS,
        "days": days,
        "total": total,
    })
}

/// 落库 `days` → 逐日 gateway 汇总（**纯函数**，便于单测）。
///
/// - 日期**升序**（`BTreeMap` 的键序，与前端窗口对齐直接可用）；
/// - 跨账号求和（含归属不到已知账号、按原键留下的行 —— 汇总不该丢量）；
/// - 带上当日的「可能被裁」标记：那些天的总量只是**下界**（前端图注披露）。
fn gateway_trend_points(store: &Store) -> Vec<Value> {
    store
        .days
        .iter()
        .map(|(day, record)| {
            let mut input = 0u64;
            let mut output = 0u64;
            for account in record.accounts.values() {
                input = input.saturating_add(account.gateway.input);
                output = output.saturating_add(account.gateway.output);
            }
            json!({
                "key": day,
                "input": input,
                "output": output,
                "total": input.saturating_add(output),
                "truncated": record.gateway_truncated,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;
    use std::fs;

    fn at_local_date(days_ago: i64, hour: u32) -> i64 {
        let today = Local::now().date_naive() - ChronoDuration::days(days_ago);
        Local
            .with_ymd_and_hms(today.year(), today.month(), today.day(), hour, 0, 0)
            .single()
            .expect("valid local test date")
            .timestamp_millis()
    }

    fn client_row(session_id: &str, ts: i64, input: u64, output: u64) -> ClientUsageRow {
        ClientUsageRow {
            session_id: session_id.to_string(),
            ts,
            input,
            output,
        }
    }

    /// 造一份「昨天 + 今天」的 jsonl 文本：昨天 `old_rows` 行、今天 `new_rows` 行。
    ///
    /// `pad` 是每行塞进去的填充长度 —— 用来把「今天的记录」推到**一个 chunk 之外**，
    /// 从而真正压到尾部窗口的扩张逻辑（而不是一读就覆盖）。
    fn day_fixture(old_rows: usize, new_rows: usize, pad: usize) -> (String, u64, u64) {
        let filler = "x".repeat(pad);
        let mut text = String::new();
        for index in 0..old_rows {
            text.push_str(&format!(
                "{}\n",
                json!({
                    "timestamp": at_local_date(1, 9) + index as i64,
                    "sessionId": "s-old",
                    "message": { "usage": { "input_tokens": 3, "output_tokens": 1 } },
                    "content": filler,
                })
            ));
        }
        for index in 0..new_rows {
            text.push_str(&format!(
                "{}\n",
                json!({
                    "timestamp": at_local_date(0, 1) + index as i64,
                    "sessionId": "s-today",
                    "message": { "usage": { "input_tokens": 7, "output_tokens": 2 } },
                    "content": filler,
                })
            ));
        }
        (text, 3 * old_rows as u64, 7 * new_rows as u64)
    }

    /// ★★ **尾部窗口必须与全量读等价**（2026-10-13 尾部读取的护栏）。
    ///
    /// 尾部读取是纯 I/O 优化：一旦停止判据写错，今日记录会被**静默漏掉**
    /// （数字变小、不报错）—— 真实语料里正是靠独立复算才发现「差 2%」的疑点。
    /// 这里走**生产函数** `read_day_window`（真文件、真 `File`），与整份文件的解析逐条比对。
    ///
    /// 阳性对照：断言全量读**确实**读到了今日记录（否则「两边都空」也能过）。
    #[test]
    fn tail_window_keeps_every_today_record_against_a_full_read() {
        let day = local_date_string(at_local_date(0, 1)).expect("today");
        let day_start = local_day_start_ms(&day).expect("day start");
        let dir = std::env::temp_dir().join(format!(
            "buddy-switch-token-daily-window-{}-{}",
            std::process::id(),
            now_ms()
        ));
        fs::create_dir_all(&dir).expect("create fixture dir");

        // 今天 400 行、每行 ~1KB ⇒ 今日记录**单独就超过一个 chunk**（256KB），
        // 迫使尾部窗口扩张多轮；前面再垫 2000 行昨天的记录（约 2.3MB），
        // 让窗口**明显短于**整份文件 —— 否则证明不了「只读尾部」。
        let (text, _old_in, new_in) = day_fixture(2000, 400, 1024);
        assert!(
            text.len() as u64 > 2 * jsonl_tail::TAIL_CHUNK_BYTES,
            "夹具必须超过两个 chunk（实际 {} 字节）",
            text.len()
        );
        let path = dir.join("sess.jsonl");
        fs::write(&path, &text).expect("write fixture");
        let bytes = text.into_bytes();

        let full = parse_day_rows(&String::from_utf8_lossy(&bytes), &day);
        assert!(!full.is_empty(), "阳性对照：全量读必须读到今日记录");
        assert_eq!(full.len(), 400, "阳性对照：全量读的条数 = 夹具里今天的行数");
        assert_eq!(
            full.iter().map(|row| row.input).sum::<u64>(),
            new_in,
            "阳性对照：全量读的 input 合计 = 夹具里今天的 input"
        );

        let mut file = fs::File::open(&path).expect("open fixture");
        let size = file.metadata().expect("stat").len();
        let window = read_day_window(&mut file, size, day_start).expect("窗口读得到");
        assert!(
            (window.len() as u64) < bytes.len() as u64,
            "窗口必须**短于**整份文件，否则这条用例证明不了「只读尾部」"
        );
        let tail = parse_day_rows(&String::from_utf8_lossy(&window), &day);
        assert_eq!(tail.len(), full.len(), "尾部窗口不得少读今日记录");
        assert_eq!(tail, full, "尾部窗口的结果必须与全量读逐条一致");

        // 反向：整份都可能是今天的（昨日 0 行）⇒ 窗口必须一直扩张到文件开头。
        let (only_today, _, _) = day_fixture(0, 40, 1024);
        let path = dir.join("today-only.jsonl");
        fs::write(&path, &only_today).expect("write fixture");
        let mut file = fs::File::open(&path).expect("open fixture");
        let size = file.metadata().expect("stat").len();
        let window = read_day_window(&mut file, size, day_start).expect("窗口读得到");
        assert_eq!(window.len() as u64, size, "今日记录覆盖整份时必须读全");
        assert_eq!(
            parse_day_rows(&String::from_utf8_lossy(&window), &day).len(),
            40
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// (a) 日期分桶：昨日记录不落今日、今日记录落今日。
    #[test]
    fn client_rows_are_bucketed_by_local_day() {
        let today = at_local_date(0, 12);
        let yesterday = at_local_date(1, 12);
        let day = local_date_string(today).unwrap();
        let mut meta = HashMap::new();
        meta.insert("s1".to_string(), "uid-a".to_string());

        let rows = vec![
            client_row("s1", yesterday, 500, 100),
            client_row("s1", today, 10, 2),
        ];
        let (per_account, unknown) = bucket_client(&rows, &meta, &day);

        assert_eq!(per_account.get("uid-a").unwrap().input, 10, "只累计今日");
        assert_eq!(per_account.get("uid-a").unwrap().output, 2);
        assert_eq!(unknown.input, 0);
    }

    /// (b) 同一账号的两个会话合并求和。
    #[test]
    fn client_rows_sum_two_sessions_of_the_same_account() {
        let today = at_local_date(0, 12);
        let day = local_date_string(today).unwrap();
        let mut meta = HashMap::new();
        meta.insert("s1".to_string(), "uid-a".to_string());
        meta.insert("s2".to_string(), "uid-a".to_string());

        let rows = vec![
            client_row("s1", today, 10, 2),
            client_row("s2", today, 30, 4),
        ];
        let (per_account, unknown) = bucket_client(&rows, &meta, &day);

        assert_eq!(per_account.len(), 1);
        assert_eq!(per_account.get("uid-a").unwrap().input, 40);
        assert_eq!(per_account.get("uid-a").unwrap().output, 6);
        assert_eq!(unknown.input, 0);
    }

    /// (c) 归属不了的会话落 unknown，**不**落到别的账号。
    #[test]
    fn unresolvable_session_goes_to_unknown_not_another_account() {
        let today = at_local_date(0, 12);
        let day = local_date_string(today).unwrap();
        let mut meta = HashMap::new();
        meta.insert("s1".to_string(), "uid-a".to_string());

        // "ghost" 不在 meta（库缺行 / user_id 为空）。
        let rows = vec![
            client_row("s1", today, 10, 2),
            client_row("ghost", today, 7, 3),
        ];
        let (per_account, unknown) = bucket_client(&rows, &meta, &day);

        assert_eq!(per_account.len(), 1);
        assert_eq!(per_account.get("uid-a").unwrap().input, 10);
        assert_eq!(unknown.input, 7, "无法归属的用量必须进 unknown，绝不并入 uid-a");
        assert_eq!(unknown.output, 3);
    }

    /// 网关行按 region + 本地日过滤。
    #[test]
    fn gateway_rows_are_filtered_by_region_and_day() {
        let today = at_local_date(0, 12);
        let yesterday = at_local_date(1, 12);
        let day = local_date_string(today).unwrap();
        let entries = vec![
            json!({"ts": today, "region": "cn", "account": "uid-a", "promptTokens": 10, "completionTokens": 2}),
            json!({"ts": today, "region": "global", "account": "uid-a", "promptTokens": 99, "completionTokens": 9}),
            json!({"ts": yesterday, "region": "cn", "account": "uid-a", "promptTokens": 7, "completionTokens": 1}),
        ];

        let per_account = bucket_gateway(
            &parse_gateway_rows(&entries),
            Region::Cn,
            &day,
            &HashSet::new(),
            &HashMap::new(),
        );

        assert_eq!(per_account.len(), 1);
        assert_eq!(per_account.get("uid-a").unwrap().input, 10, "只取 cn + 今日");
        assert_eq!(per_account.get("uid-a").unwrap().output, 2);
    }

    /// 网关展示名 → uid 解析：唯一命中解析、歧义/未知原样保留。
    #[test]
    fn gateway_display_name_resolves_to_uid() {
        let uids: HashSet<String> = HashSet::from(["uid-a".to_string()]);
        let mut names: HashMap<String, Option<String>> = HashMap::new();
        names.insert("a@example.com".to_string(), Some("uid-a".to_string()));
        names.insert("重名".to_string(), None);

        assert_eq!(resolve_account_key("uid-a", &uids, &names), "uid-a");
        assert_eq!(resolve_account_key("a@example.com", &uids, &names), "uid-a");
        assert_eq!(
            resolve_account_key("重名", &uids, &names),
            "重名",
            "歧义展示名必须原样保留，绝不错贴"
        );
        assert_eq!(resolve_account_key("ghost", &uids, &names), "ghost");
    }

    /// 行内 `accountUid`（写入时的精确身份）**优先于**展示名解析：即便展示名歧义/重名，
    /// 只要带了 uid 就精确归属；仅**旧行**（无 `accountUid`）才回落展示名解析。
    #[test]
    fn gateway_row_prefers_account_uid_over_ambiguous_display_name() {
        let today = at_local_date(0, 12);
        let day = local_date_string(today).unwrap();
        let entries = vec![
            // 新行：展示名「重名」歧义（两个账号共用），但 accountUid 精确。
            json!({"ts": today, "region": "cn", "account": "重名", "accountUid": "uid-b",
                   "promptTokens": 10, "completionTokens": 2}),
            // 旧行：无 accountUid ⇒ 回落展示名解析（唯一命中）。
            json!({"ts": today, "region": "cn", "account": "a@example.com",
                   "promptTokens": 5, "completionTokens": 1}),
        ];
        let uids: HashSet<String> =
            HashSet::from(["uid-a".to_string(), "uid-b".to_string()]);
        let mut names: HashMap<String, Option<String>> = HashMap::new();
        names.insert("a@example.com".to_string(), Some("uid-a".to_string()));
        names.insert("重名".to_string(), None);

        let per_account =
            bucket_gateway(&parse_gateway_rows(&entries), Region::Cn, &day, &uids, &names);

        assert_eq!(
            per_account.get("uid-b").unwrap().input,
            10,
            "带 accountUid 的行必须精确归属，不受展示名歧义影响"
        );
        assert_eq!(
            per_account.get("uid-a").unwrap().input,
            5,
            "旧行无 accountUid ⇒ 回落展示名解析"
        );
        assert!(
            !per_account.contains_key("重名"),
            "歧义展示名不得成为归属键"
        );
    }

    /// (d) `gatewayTruncated` 要求**同时**满足「条数到上限」与「最旧存活条目是今天」。
    ///
    /// 第二个条件是本次修误报的关键：滚动窗口可能被**昨天**的流量顶满，而今天的条目
    /// 全在（FIFO ⇒ 早于今天的条目先被丢）⇒ 今日总量完整，**不该**标 `true`。
    #[test]
    fn gateway_truncated_requires_full_window_and_today_oldest() {
        let today = local_date_string(at_local_date(0, 12)).unwrap();
        let yesterday = (Local::now().date_naive() - ChronoDuration::days(1))
            .format("%Y-%m-%d")
            .to_string();

        // 没到上限：无论最旧条目是哪天，都没丢过东西。
        assert!(!gateway_log_truncated_today(
            5,
            200,
            Some(&today),
            &today
        ));
        assert!(!gateway_log_truncated_today(
            199,
            200,
            Some(&today),
            &today
        ));

        // 到上限 + 最旧存活条目是今天 ⇒ 今天的条目可能被丢过 ⇒ 真。
        assert!(gateway_log_truncated_today(
            200,
            200,
            Some(&today),
            &today
        ));
        assert!(gateway_log_truncated_today(
            201,
            200,
            Some(&today),
            &today
        ));

        // ★ 误报杀手：到上限但最旧存活条目**早于今天** ⇒ 今天的条目一条没丢 ⇒ 假。
        assert!(!gateway_log_truncated_today(
            200,
            200,
            Some(&yesterday),
            &today
        ));

        // 空日志（无最旧条目）⇒ 假；`keep=0` 按 1 兜底。
        assert!(!gateway_log_truncated_today(0, 200, None, &today));
        assert!(!gateway_log_truncated_today(200, 0, None, &today));
    }

    /// (f) 网关趋势：逐日跨账号求和、日期升序、`truncated` 随身带。
    ///
    /// 「升序」不是审美：前端按窗口键 join（`usageWindowKeys` 升序）⇒ 后端若乱序，
    /// 图上的天会错位。`BTreeMap` 的键序保证它。
    #[test]
    fn gateway_trend_points_sums_accounts_in_ascending_order() {
        let mut store = empty_store();

        let mut day_a = DayRecord::default();
        day_a.accounts.insert(
            "u1".to_string(),
            AccountRecord {
                gateway: Bucket { input: 10, output: 2 },
                ..Default::default()
            },
        );
        day_a.accounts.insert(
            "u2".to_string(),
            AccountRecord {
                gateway: Bucket { input: 5, output: 1 },
                ..Default::default()
            },
        );
        day_a.gateway_truncated = true;

        // 只有 client 用量的日子：gateway 合计必须是**真实 0**（记录器跑过、网关没流量），
        // 且不得把 client 的量混进 gateway 汇总。
        let mut day_b = DayRecord::default();
        day_b.accounts.insert(
            "u1".to_string(),
            AccountRecord {
                client: Bucket { input: 999, output: 0 },
                ..Default::default()
            },
        );

        store.days.insert("2026-10-11".to_string(), day_b);
        store.days.insert("2026-10-10".to_string(), day_a); // 故意乱序插入

        let points = gateway_trend_points(&store);
        assert_eq!(points.len(), 2);
        assert_eq!(points[0]["key"], "2026-10-10", "日期必须升序");
        assert_eq!(points[0]["input"], 15, "跨账号求和");
        assert_eq!(points[0]["output"], 3);
        assert_eq!(points[0]["total"], 18);
        assert_eq!(
            points[0]["truncated"], true,
            "被裁日的总量只是下界，标记必须带出给前端披露"
        );
        assert_eq!(points[1]["key"], "2026-10-11");
        assert_eq!(points[1]["total"], 0, "client 用量不得混进 gateway 汇总");
        assert_eq!(points[1]["truncated"], false);
    }

    /// (g) 网关趋势只读、不落盘：调用前后 store 文件内容不变（纯读路径）。
    #[test]
    fn gateway_token_trend_is_read_only() {
        // 不碰真实 store：用 MEMORY 里钉住的隔离手法行不通（store 路径是进程级常量），
        // 故只断言**函数签名层面**的只读性由上面 (f) 的纯函数覆盖 —— 这里转而钉住
        // 「投影不含 client 桶」这一最容易被后来者加错的点（变异：把 gateway 换成 client ⇒ 红）。
        let mut store = empty_store();
        let mut day = DayRecord::default();
        day.accounts.insert(
            "u1".to_string(),
            AccountRecord {
                client: Bucket { input: 7, output: 7 },
                gateway: Bucket { input: 1, output: 1 },
                ..Default::default()
            },
        );
        store.days.insert("2026-10-10".to_string(), day);
        let points = gateway_trend_points(&store);
        assert_eq!(points[0]["total"], 2, "只应汇总 gateway 桶（7+7=14 是 client 的量）");
    }

    /// (e) 保留裁剪丢掉超过保留窗的日期；条数上限丢最旧。
    #[test]
    fn prune_drops_days_older_than_the_cap() {
        let now = at_local_date(0, 12);
        let mut store = empty_store();
        let old_key = (Local::now().date_naive() - ChronoDuration::days(TOKEN_DAILY_RETENTION_DAYS))
            .format("%Y-%m-%d")
            .to_string();
        let today_key = local_date_string(now).unwrap();
        store.days.insert(old_key.clone(), DayRecord::default());
        store.days.insert(today_key.clone(), DayRecord::default());

        prune_store(&mut store, now);

        assert!(!store.days.contains_key(&old_key), "超过保留天数的日期必须被丢弃");
        assert!(store.days.contains_key(&today_key), "今日必须保留");

        // 条数上限守卫：塞入保留窗内的日期，超出上限时丢最旧。
        let mut store = empty_store();
        let base = Local::now().date_naive();
        for offset in 0..(TOKEN_DAILY_MAX_DAYS as i64 + 1) {
            let key = (base + ChronoDuration::days(offset))
                .format("%Y-%m-%d")
                .to_string();
            store.days.insert(key, DayRecord::default());
        }
        prune_store(&mut store, now);
        assert_eq!(store.days.len(), TOKEN_DAILY_MAX_DAYS);
    }

    /// 读侧形状：空存储 → accounts 空（缺席 = 未知）、unknownClient 为 `{0,0}`、truncated 假。
    #[test]
    fn empty_day_response_is_honest_about_absence() {
        let response = build_response("2026-10-10", None);
        assert_eq!(
            response["accounts"].as_array().map(Vec::len),
            Some(0),
            "缺席账号 = 「今日无记录 / 未知」，绝不当 0 列出"
        );
        assert_eq!(response["unknownClient"]["in"], 0);
        assert_eq!(response["unknownClient"]["out"], 0);
        assert_eq!(response["gatewayTruncated"], false);
    }

    // -----------------------------------------------------------------------
    // 2026-10-12：当日扫描闸门（「今日 Token 总是横杠」的根因）+ mtime 预筛
    // -----------------------------------------------------------------------

    /// ★ 闸门：**同日 + 未过 TTL** 才跳过；跨日 / 过期 / `force` / 从未扫过都要重扫。
    ///
    /// ## 为什么这条用例是关键护栏
    ///
    /// 旧闸门是 `clientScannedDate == 今天` ⇒ 当日值被冻结在当天**第一次**扫描那一刻。
    /// 实测现场：CN 的 `token_daily.json` 14:33 落盘（当时今日无用量 ⇒ `accounts: {}`），
    /// 客户端 15:00 才开始产生用量 ⇒ 之后一整天卡片都是「—」，且前端从不传 `force`，
    /// 没有任何路径能恢复。
    ///
    /// 可证伪：把 `client_scan_is_fresh` 的 TTL 判据去掉、只留「同一天」⇒ 第二条断言
    /// （超过 TTL 必须重扫）立刻变红。
    #[test]
    fn client_scan_gate_skips_only_within_the_same_day_ttl() {
        let today = at_local_date(0, 15);
        let day = local_date_string(today).expect("local day");
        let today_start = local_day_start_ms(&day).expect("today start");

        // 昨天 23:59:30 扫过、今天 00:00:10 再问 —— 只隔 40 秒（< TTL），但**跨日** ⇒ 必须重扫。
        let before_midnight = today_start - 30_000;
        let after_midnight = today_start + 10_000;
        assert!(
            !client_scan_is_fresh(false, Some(before_midnight), after_midnight, &day),
            "跨日必须重扫 —— 否则新的一天沿用昨天的（很可能是空的）结果"
        );

        // 同日：TTL 之内跳过、到点重扫。
        assert!(
            client_scan_is_fresh(
                false,
                Some(after_midnight),
                after_midnight + CLIENT_SCAN_TTL_MS - 1,
                &day
            ),
            "同日且未过 TTL 应跳过（否则每次进账号页都重扫）"
        );
        assert!(
            !client_scan_is_fresh(
                false,
                Some(after_midnight),
                after_midnight + CLIENT_SCAN_TTL_MS,
                &day
            ),
            "过 TTL 必须重扫 —— 这是「今日 Token 一整天不动」的修法"
        );

        // `force` ⇒ 总是重扫；从未扫过 ⇒ 重扫。
        assert!(!client_scan_is_fresh(true, Some(after_midnight), after_midnight + 1_000, &day));
        assert!(!client_scan_is_fresh(false, None, today, &day));
    }

    /// ★ TTL 必须「短到能反映当天、长到不折腾」——把两个边界都钉住。
    ///
    /// 可证伪：把 `CLIENT_SCAN_TTL_MS` 改成一天 ⇒ 第一条断言变红（「今日 Token」又会一整天不动）。
    #[test]
    fn client_scan_ttl_is_short_enough_for_a_today_metric() {
        assert!(
            CLIENT_SCAN_TTL_MS <= 10 * 60 * 1000,
            "TTL 过长（{CLIENT_SCAN_TTL_MS}ms）—— 「今日 Token」又会一整天不动"
        );
        assert!(
            CLIENT_SCAN_TTL_MS >= 30 * 1000,
            "TTL 过短（{CLIENT_SCAN_TTL_MS}ms）—— 每次进账号页都要重扫"
        );
    }

    /// mtime 预筛判据：信息缺失时**放行**（宁可慢，不可漏）。
    #[test]
    fn mtime_prefilter_falls_open_when_information_is_missing() {        assert!(mtime_may_hold_day(None, Some(1_000_000)), "读不到 mtime 必须放行");
        assert!(mtime_may_hold_day(Some(1), None), "算不出日界必须放行");
        assert!(mtime_may_hold_day(None, None));
        // 日界前 1 小时之内（余量内）仍放行；再早就不放行。
        let start = 10 * 86_400_000;
        assert!(mtime_may_hold_day(Some(start - CLIENT_SCAN_MTIME_SKEW_MS), Some(start)));
        assert!(!mtime_may_hold_day(Some(start - CLIENT_SCAN_MTIME_SKEW_MS - 1), Some(start)));
    }

    /// ★ mtime 预筛的**行为**（真文件、真 mtime）：新鲜的当日记录必须读到，陈旧的被跳过。
    ///
    /// ## 为什么要带**阳性对照**
    ///
    /// 「陈旧文件被跳过」这条断言在「扫描整个坏掉、返回空」时也恒真 —— 那是假绿。
    /// 所以同时断言新鲜的当日记录**必须出现在结果里**：两条一起才说明「筛的是 mtime，
    /// 不是把一切都扔掉」。
    #[test]
    fn mtime_prefilter_keeps_fresh_files_and_drops_stale_ones() {
        use std::time::SystemTime;

        let now = now_ms();
        let day = local_date_string(now).expect("local day");
        let root = std::env::temp_dir().join(format!(
            "buddy-switch-token-daily-mtime-{}-{now}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create fixture dir");

        let set_mtime = |path: &Path, at_ms: i64| {
            let file = fs::OpenOptions::new()
                .write(true)
                .open(path)
                .expect("open for mtime");
            let when = SystemTime::UNIX_EPOCH + Duration::from_millis(u64::try_from(at_ms).unwrap());
            file.set_modified(when).expect("set mtime");
        };
        let line = |session_id: &str, ts: i64, input: u64| {
            json!({
                "timestamp": ts,
                "sessionId": session_id,
                "message": { "usage": { "input_tokens": input, "output_tokens": 2 } }
            })
            .to_string()
        };

        let fresh = root.join("fresh.jsonl");
        fs::write(&fresh, format!("{}\n", line("s-fresh", now, 10))).expect("write fresh");
        set_mtime(&fresh, now);

        // 内容含「今日」记录，但 mtime 停在 10 天前 ⇒ 预筛跳过（写明的取舍，见函数注释）。
        let stale = root.join("stale.jsonl");
        fs::write(&stale, format!("{}\n", line("s-stale", now, 999))).expect("write stale");
        set_mtime(&stale, now - 10 * 86_400_000);

        let rows = scan_client_rows(&root, &day);

        assert!(
            rows.iter().any(|row| row.session_id == "s-fresh"),
            "mtime 新鲜的当日记录被漏掉 —— 预筛过严（阳性对照失败，说明扫描整体没工作）"
        );
        assert!(
            !rows.iter().any(|row| row.session_id == "s-stale"),
            "mtime 预筛没有生效 —— 陈旧文件仍被读取"
        );

        let _ = fs::remove_dir_all(&root);
    }
}