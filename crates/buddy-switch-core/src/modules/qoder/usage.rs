//! Qoder 的**逐日用量**视图（本地会话文件扫描 → 按日聚合）。
//!
//! ## 为什么统计的是 credits，而不是 token
//!
//! 结论是**取证得来**、不是猜测：Qoder 客户端按 `credits` 计费，逐请求的 token 计数**恒为 0**。
//! 三处互相独立的证据：
//!
//! 1. **会话文件**（`~/.qoder-cn/projects/{project}/{sessionId}.jsonl`）：`type=assistant` 的行里
//!    `message.usage` 的 `input_tokens` / `output_tokens` / `cache_read_input_tokens` /
//!    `cache_creation_input_tokens` **全是 0**，但同一对象里的 `credits` 有真值
//!    （实测形如 `"credits":1.070790688,"billable":true`）。
//! 2. **逐请求日志**（`~/.qoder-cn/logs/**`）：同样是 `input_tokens=0 output_tokens=0`。
//! 3. **客户端本地库**（`%APPDATA%/com.qodercn.app.stable/main.sqlite` 的
//!    `chat_session_context_usage.snapshot_json`）：客户端**自己写着**
//!    `"tokenCountsAvailable":false, "totalTokens":0`。
//!
//! ⇒ **绝不要伪造 token 数字**。本模块如实统计 **credits**（附带请求数与上下文占比），
//! 单位在下发 JSON 里明确标为 `"unit":"credits"`。
//!
//! ## 为什么流式逐行读
//!
//! 会话文件可能很大（单文件可达数十 MB）。用 [`BufReader::lines`] 逐行读，内存占用与文件大小无关；
//! 若先 `read_to_string` 再整体 parse，会在大文件上产生一次不必要的整块分配与拷贝。
//!
//! ## 为什么只认「能解析出 timestamp 且带数值 credits」的行
//!
//! 会话文件是**追加写**的事件流，可能混有：其它类型的行、写入中断留下的半行、字段缺失的行。
//! 只要一行坏了就丢掉整个文件，恰恰会丢掉最需要统计的其余全部数据 —— 因此**按行容错**：
//! 解析不出的行跳过，继续读下一行。
//! 另外，**只有带数值 `credits` 的行才算一次计费请求**：缺失或非数的行不计入。
//! 这一条同时定义了 `requests` 的口径 —— 见下。
//!
//! ## 口径（重要）
//!
//! - **`credits`**：当日所有计费行的 `credits` 之和（浮点，如实累加）。
//! - **`requests`**：当日**带数值 `credits` 的行数**。口径与 `credits` **严格一致** ——
//!   两者都只统计「有 credits 的行」。理由：本平台按 credits 计费，
//!   一次「计费请求」的天然判据就是「产生了 credits 的那一行」；
//!   若把缺失 credits 的行也计为一次请求（当作 0 credits），会让请求数虚高、
//!   并与 credits 的口径分裂，用户无法据此对账。故**缺失/非数 credits 的行既不计 credits、也不计请求**。
//! - **时间分桶用本地时区**（[`chrono::Local`]）：用户看的是「今天」，
//!   与项目其余用量口径一致（本地日期 `YYYY-MM-DD`）。
//! - **窗口含今天**：`days = Some(30)` ⇒ 从「今天往前 29 天」到今天（共 30 天）；
//!   `days = None` ⇒ 全部历史；`days = Some(0)` ⇒ **空序列**（fail-closed，不是「全部」）。
//! - **只输出有数据的天**：窗口内没有事件的那天不出现在 `days` 里（不凭空补 0），
//!   由前端决定是否补齐坐标轴。
//! - **目录不存在不是错误**：客户端没被使用过时 `projects/` 目录并不存在，
//!   此时返回空序列，并在 `source` 里如实标注（`exists:false`）。
//!
//! ## 只读边界
//!
//! 本模块**只读**客户端目录（`~/.qoder-cn/projects` / `~/.qoder/projects`），
//! 绝不写入、绝不删除客户端任何文件。

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Local, NaiveDate};
use serde_json::{json, Value};

use super::paths;
use super::region::QoderRegion;

/// 逐日用量的一行。
#[derive(Debug, Clone, PartialEq)]
pub struct UsageDay {
    /// 本地日期 `YYYY-MM-DD`。
    pub date: String,
    /// 当日 credits 合计。
    pub credits: f64,
    /// 当日请求数（= 当日带数值 credits 的行数）。
    pub requests: u64,
    /// 模型 → credits（按 credits 降序；同值按模型名升序，保证确定性）。
    pub by_model: Vec<(String, f64)>,
}

/// 从会话文件解析出的单条计费事件（纯结构，便于单测）。
#[derive(Debug, Clone, PartialEq)]
pub struct UsageEvent {
    /// 本地日期 `YYYY-MM-DD`（由 `timestamp` + 本地时区派生）。
    pub date: String,
    /// 该行的 credits（已确认是有限数值）。
    pub credits: f64,
    /// 模型名；行里缺失时回落为 [`UNKNOWN_MODEL`]。
    pub model: String,
    /// 会话 id；行里缺失时为空串。
    pub session_id: String,
}

/// 模型名缺失时的回落标签（不凭空捏造模型名）。
pub const UNKNOWN_MODEL: &str = "unknown";

/// `source.kind` 的固定值：数据来自客户端本地会话文件。
pub const SOURCE_KIND: &str = "client-sessions";

/// `source.note`：如实说明数据来源与局限（前端要展示给用户）。
pub const SOURCE_NOTE: &str = "数据来自本机 Qoder 客户端的会话文件（projects/**/*.jsonl）。\
该客户端按 credits 计费，逐请求的 token 计数恒为 0（input/output/cache 全为 0），\
故本视图统计 credits 而非 token。仅统计能解析出 timestamp 且带数值 credits 的行；\
目录不存在时返回空序列，而非报错。";

// ---------------------------------------------------------------------------
// 解析（纯函数）
// ---------------------------------------------------------------------------

/// 解析一行 JSONL → 计费事件。
///
/// 返回 `None`（跳过该行）的几种情形，**都是刻意的**：
/// - 空行 / 坏 JSON：写入中断或非事件行；
/// - 缺 `timestamp` 或时间无法解析：无法参与按日分桶；
/// - 缺 `credits` 或 `credits` 非数值/非有限：不是一次计费请求（口径见模块头）。
pub fn parse_line(line: &str) -> Option<UsageEvent> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let value: Value = serde_json::from_str(line).ok()?;

    // 只认能解析出 timestamp 的行 —— 否则无法分桶。
    let timestamp = value.get("timestamp").and_then(Value::as_str)?;
    let date = local_date_of(timestamp)?;

    // 只认带数值 credits 的行 —— 这是「计费请求」的唯一判据。
    let credits = find_credits(&value)?;

    // 模型：优先顶层 `model`，回落 `message.model`，都没有则用 UNKNOWN_MODEL。
    let model = value
        .get("model")
        .and_then(Value::as_str)
        .or_else(|| {
            value
                .get("message")
                .and_then(|message| message.get("model"))
                .and_then(Value::as_str)
        })
        .filter(|name| !name.is_empty())
        .unwrap_or(UNKNOWN_MODEL)
        .to_string();

    // 会话 id：顶层 `sessionId`，回落 `session_id`，都没有则为空串。
    let session_id = value
        .get("sessionId")
        .and_then(Value::as_str)
        .or_else(|| value.get("session_id").and_then(Value::as_str))
        .unwrap_or("")
        .to_string();

    Some(UsageEvent {
        date,
        credits,
        model,
        session_id,
    })
}

/// ISO-8601 时间串 → 本地日期 `YYYY-MM-DD`（分桶用本地时区）。
fn local_date_of(timestamp: &str) -> Option<String> {
    let parsed = DateTime::parse_from_rfc3339(timestamp).ok()?;
    Some(parsed.with_timezone(&Local).format("%Y-%m-%d").to_string())
}

/// 从一行事件里取 credits。
///
/// 依次尝试 `message.usage.credits` / `usage.credits` / 顶层 `credits`；
/// 必须能解析成**有限**浮点数，否则视为「没有 credits」。
fn find_credits(value: &Value) -> Option<f64> {
    let raw = value
        .get("message")
        .and_then(|message| message.get("usage"))
        .and_then(|usage| usage.get("credits"))
        .or_else(|| value.get("usage").and_then(|usage| usage.get("credits")))
        .or_else(|| value.get("credits"))?;
    let number = raw.as_f64()?;
    number.is_finite().then_some(number)
}

// ---------------------------------------------------------------------------
// 聚合（纯函数）
// ---------------------------------------------------------------------------

/// 判断某本地日期是否落在回看窗口内。
///
/// - `None` ⇒ 全部历史（恒真）；
/// - `Some(0)` ⇒ 空窗口（恒假，fail-closed）；
/// - `Some(n)` ⇒ `[today - (n-1), today]`（含今天）。
fn in_window(date: &str, today: NaiveDate, days: Option<u32>) -> bool {
    match days {
        None => true,
        Some(0) => false,
        Some(n) => {
            let Ok(day) = NaiveDate::parse_from_str(date, "%Y-%m-%d") else {
                return false;
            };
            let cutoff = today - Duration::days((n as i64) - 1);
            day >= cutoff && day <= today
        }
    }
}

/// 事件列表 → 逐日序列（纯函数；`today` 由调用方注入，便于单测固定时间）。
///
/// 只输出窗口内**有事件的天**，按日期升序。
fn aggregate(events: &[UsageEvent], today: NaiveDate, days: Option<u32>) -> Vec<UsageDay> {
    // BTreeMap 保证日期升序输出（无需再排序）。
    let mut per_day: BTreeMap<String, (f64, u64, BTreeMap<String, f64>)> = BTreeMap::new();
    for event in events {
        if !in_window(&event.date, today, days) {
            continue;
        }
        let bucket = per_day
            .entry(event.date.clone())
            .or_insert_with(|| (0.0, 0, BTreeMap::new()));
        bucket.0 += event.credits;
        bucket.1 += 1;
        *bucket.2.entry(event.model.clone()).or_insert(0.0) += event.credits;
    }

    per_day
        .into_iter()
        .map(|(date, (credits, requests, models))| UsageDay {
            date,
            credits,
            requests,
            by_model: sort_models_desc(models),
        })
        .collect()
}

/// `模型 → credits` 映射 → 降序 `Vec`（同值按模型名升序，保证输出确定性）。
fn sort_models_desc(models: BTreeMap<String, f64>) -> Vec<(String, f64)> {
    let mut rows: Vec<(String, f64)> = models.into_iter().collect();
    rows.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.0.cmp(&right.0))
    });
    rows
}

// ---------------------------------------------------------------------------
// IO（外层；只读客户端目录）
// ---------------------------------------------------------------------------

/// 一次扫描的结果（含聚合所需的原始事件与来源元信息）。
struct Scan {
    events: Vec<UsageEvent>,
    /// 扫到的 `.jsonl` 文件数（≈ 会话数）。
    files: usize,
    /// 被扫描的 `projects/` 目录。
    projects_dir: PathBuf,
    /// 该目录是否存在（用于 `source.exists` 如实标注）。
    exists: bool,
}

/// 递归收集目录下的 `.jsonl` 文件（目录不存在时返回空）。
fn collect_jsonl(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl(&path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
            out.push(path);
        }
    }
}

/// 流式逐行读取单个文件，把能解析出的计费事件追加到 `out`。
///
/// **逐行容错**：单行读取失败或解析失败只跳过该行，不影响文件其余内容。
fn read_events_from_file(path: &Path, out: &mut Vec<UsageEvent>) {
    let Ok(file) = File::open(path) else {
        return;
    };
    for line in BufReader::new(file).lines() {
        let Ok(line) = line else {
            continue;
        };
        if let Some(event) = parse_line(&line) {
            out.push(event);
        }
    }
}

/// 扫描某区域的本地会话文件。
fn scan_for(region: QoderRegion) -> Scan {
    let projects_dir = paths::cli_home_dir(region).join("projects");
    let mut files = Vec::new();
    if projects_dir.is_dir() {
        collect_jsonl(&projects_dir, &mut files);
    }
    let mut events = Vec::new();
    for file in &files {
        read_events_from_file(file, &mut events);
    }
    Scan {
        events,
        files: files.len(),
        exists: projects_dir.exists(),
        projects_dir,
    }
}

// ---------------------------------------------------------------------------
// 公开接口
// ---------------------------------------------------------------------------

/// 扫描某区域的本地会话文件，按日聚合。
///
/// `days` = 回看窗口（**包含今天**）；`None` ⇒ 全部历史。
/// 返回**按日期升序**的逐日序列（只含窗口内**有数据**的天）。
pub fn scan_days_for(region: QoderRegion, days: Option<u32>) -> Vec<UsageDay> {
    let scan = scan_for(region);
    aggregate(&scan.events, Local::now().date_naive(), days)
}

/// 下发形态：
/// `{ region, unit:"credits", days:[...], totals:{credits,requests,sessions,models}, byModel:[...], source:{...} }`。
///
/// - `days[]` 每项：`{ date, credits, requests, byModel:[{model,credits}] }`；
/// - `totals.credits` / `totals.requests` 与 `days[]` 逐项累加**严格一致**；
/// - `totals.sessions` = 窗口内**不同会话 id** 的个数；`totals.models` = 窗口内不同模型数；
/// - `byModel[]` = 窗口内**跨天**汇总的模型拆分（降序），省得前端自己把逐日数组再合起来；
/// - `source` 如实说明来源与局限（含 `tokenCountsAvailable:false`）。
pub fn usage_view_for(region: QoderRegion, days: Option<u32>) -> Value {
    let scan = scan_for(region);
    let today = Local::now().date_naive();
    let aggregated = aggregate(&scan.events, today, days);

    // 窗口内事件 → totals / byModel（与 days 同一套窗口口径）。
    let mut credits_sum = 0.0f64;
    let mut requests = 0u64;
    let mut sessions: BTreeSet<&str> = BTreeSet::new();
    let mut models: BTreeMap<String, f64> = BTreeMap::new();
    for event in &scan.events {
        if !in_window(&event.date, today, days) {
            continue;
        }
        credits_sum += event.credits;
        requests += 1;
        if !event.session_id.is_empty() {
            sessions.insert(event.session_id.as_str());
        }
        *models.entry(event.model.clone()).or_insert(0.0) += event.credits;
    }

    let model_rows = sort_models_desc(models);
    let by_model: Vec<Value> = model_rows
        .iter()
        .map(|(model, credits)| json!({ "model": model, "credits": credits }))
        .collect();

    let day_rows: Vec<Value> = aggregated
        .iter()
        .map(|day| {
            json!({
                "date": day.date,
                "credits": day.credits,
                "requests": day.requests,
                "byModel": day
                    .by_model
                    .iter()
                    .map(|(model, credits)| json!({ "model": model, "credits": credits }))
                    .collect::<Vec<Value>>(),
            })
        })
        .collect();

    json!({
        "region": region.as_str(),
        "unit": "credits",
        "days": day_rows,
        "totals": {
            "credits": credits_sum,
            "requests": requests,
            "sessions": sessions.len() as u64,
            "models": model_rows.len() as u64,
        },
        "byModel": by_model,
        "source": {
            "kind": SOURCE_KIND,
            "paths": [scan.projects_dir.to_string_lossy().to_string()],
            "sessions": scan.files as u64,
            "exists": scan.exists,
            "tokenCountsAvailable": false,
            "note": SOURCE_NOTE,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一行 `type=assistant` 的事件。
    ///
    /// `credits = None` 时**整个 `usage` 里不放 `credits` 字段**（模拟缺失）。
    fn assistant_line(timestamp: &str, credits: Option<f64>, model: &str, session: &str) -> String {
        let usage = match credits {
            Some(value) => json!({
                "input_tokens": 0,
                "output_tokens": 0,
                "cache_read_input_tokens": 0,
                "cache_creation_input_tokens": 0,
                "credits": value,
                "billable": true,
            }),
            None => json!({ "input_tokens": 0, "output_tokens": 0 }),
        };
        json!({
            "type": "assistant",
            "message": { "role": "assistant", "model": model, "usage": usage },
            "sessionId": session,
            "timestamp": timestamp,
            "version": "1.1.69",
        })
        .to_string()
    }

    fn approx(left: f64, right: f64) -> bool {
        (left - right).abs() < 1e-9
    }

    fn event(date: &str, credits: f64, model: &str) -> UsageEvent {
        UsageEvent {
            date: date.to_string(),
            credits,
            model: model.to_string(),
            session_id: "s".to_string(),
        }
    }

    // -- 解析 ---------------------------------------------------------------

    /// 正常行：token 全 0 但 credits 有真值 ⇒ 认 credits，不认 token。
    ///
    /// 反例（改坏会红）：把 `find_credits` 改成读 `input_tokens`/`output_tokens`，
    /// 本用例的 `credits` 会变成 0（token 恒为 0），断言失败。
    #[test]
    fn parse_line_reads_credits_and_ignores_zero_tokens() {
        let line = assistant_line(
            "2026-10-15T09:03:29.465Z",
            Some(1.070790688),
            "dfmodel",
            "sess-1",
        );
        let event = parse_line(&line).expect("应解析成功");
        assert!(
            approx(event.credits, 1.070790688),
            "credits={}",
            event.credits
        );
        assert_eq!(event.model, "dfmodel");
        assert_eq!(event.session_id, "sess-1");
        // 本地日期是 10 位 `YYYY-MM-DD`（不钉死具体日，避免依赖测试机时区）。
        assert_eq!(event.date.len(), 10, "date={}", event.date);
        assert!(event.date.starts_with("2026-"), "date={}", event.date);
    }

    /// credits 缺失 / 非数 ⇒ 该行不计入。
    ///
    /// 反例（改坏会红）：把 `find_credits` 的缺失分支改成 `unwrap_or(0.0)`，
    /// 缺 credits 的行会被当成 0 计入，本用例的 `is_none` 断言失败。
    #[test]
    fn parse_line_requires_a_numeric_credits() {
        // 缺 credits 字段。
        let missing = assistant_line("2026-10-15T09:03:29.465Z", None, "dfmodel", "s");
        assert!(parse_line(&missing).is_none(), "缺 credits 不应计入");

        // credits 是字符串（非数）。
        let non_numeric = json!({
            "timestamp": "2026-10-15T09:03:29.465Z",
            "sessionId": "s",
            "message": { "model": "dfmodel", "usage": { "credits": "1.07" } },
        })
        .to_string();
        assert!(parse_line(&non_numeric).is_none(), "非数 credits 不应计入");
    }

    /// 坏 JSON / 无 timestamp ⇒ 跳过该行。
    ///
    /// 反例（改坏会红）：去掉 `serde_json::from_str` 的 `.ok()?` 改为 `unwrap`，
    /// 遇到坏 JSON 会 panic；去掉 timestamp 的 `?` 检查，缺时间的行会被计入。
    #[test]
    fn parse_line_skips_bad_json_and_missing_timestamp() {
        assert!(parse_line("").is_none(), "空行跳过");
        assert!(parse_line("   ").is_none(), "空白行跳过");
        assert!(parse_line("{ not json").is_none(), "坏 JSON 跳过");

        let no_timestamp = json!({
            "sessionId": "s",
            "message": { "model": "dfmodel", "usage": { "credits": 1.0 } },
        })
        .to_string();
        assert!(parse_line(&no_timestamp).is_none(), "无 timestamp 跳过");

        // timestamp 存在但不是可解析的 ISO-8601。
        let bad_timestamp = json!({
            "timestamp": "not-a-time",
            "message": { "usage": { "credits": 1.0 } },
        })
        .to_string();
        assert!(parse_line(&bad_timestamp).is_none(), "坏 timestamp 跳过");
    }

    /// 模型缺失 ⇒ 回落 `UNKNOWN_MODEL`（不捏造）。
    ///
    /// 反例（改坏会红）：把 `unwrap_or(UNKNOWN_MODEL)` 改成 `unwrap_or_default()`，
    /// 模型名会变成空串，本用例断言失败。
    #[test]
    fn parse_line_falls_back_to_unknown_model() {
        let line = json!({
            "timestamp": "2026-10-15T09:03:29.465Z",
            "message": { "usage": { "credits": 2.5 } },
        })
        .to_string();
        assert_eq!(parse_line(&line).unwrap().model, UNKNOWN_MODEL);
    }

    // -- 聚合 ---------------------------------------------------------------

    /// 跨天分桶正确，且输出按日期升序。
    ///
    /// 反例（改坏会红）：把 `aggregate` 里的 `BTreeMap` 换成 `HashMap`，
    /// 输出顺序不再保证升序，本用例的日期序列断言失败。
    #[test]
    fn aggregate_buckets_by_date_in_ascending_order() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 15).unwrap();
        let events = vec![
            event("2026-10-15", 1.0, "a"),
            event("2026-10-14", 2.0, "a"),
            event("2026-10-15", 3.0, "b"),
        ];
        let days = aggregate(&events, today, None);
        let dates: Vec<&str> = days.iter().map(|day| day.date.as_str()).collect();
        assert_eq!(dates, vec!["2026-10-14", "2026-10-15"]);
        assert!(approx(days[0].credits, 2.0));
        assert!(approx(days[1].credits, 4.0));
        assert_eq!(days[1].requests, 2);
    }

    /// 模型拆分按 credits 降序；同值按模型名升序。
    ///
    /// 反例（改坏会红）：把 `sort_models_desc` 的比较子左右对调，会变成升序，断言失败。
    #[test]
    fn aggregate_splits_models_descending() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 15).unwrap();
        let events = vec![
            event("2026-10-15", 1.0, "alpha"),
            event("2026-10-15", 3.0, "beta"),
            event("2026-10-15", 2.0, "gamma"),
        ];
        let days = aggregate(&events, today, None);
        let models: Vec<&str> = days[0]
            .by_model
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();
        assert_eq!(models, vec!["beta", "gamma", "alpha"]);
        assert!(approx(days[0].by_model[0].1, 3.0));
    }

    /// 窗口裁剪：`days = Some(3)` ⇒ `[今天-2, 今天]`（含今天，共 3 天）。
    ///
    /// 反例（改坏会红）：把 `(n as i64) - 1` 改成 `n as i64`，
    /// 边界会少算一天，`2026-10-13` 会被排除，断言失败。
    #[test]
    fn aggregate_window_trims_days_outside_the_window() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 15).unwrap();
        let events = vec![
            event("2026-10-12", 1.0, "a"), // 窗口外（太旧）
            event("2026-10-13", 1.0, "a"), // 边界内
            event("2026-10-15", 1.0, "a"), // 今天
        ];
        let days = aggregate(&events, today, Some(3));
        let dates: Vec<&str> = days.iter().map(|day| day.date.as_str()).collect();
        assert_eq!(dates, vec!["2026-10-13", "2026-10-15"]);
    }

    /// `days = Some(0)` ⇒ 空序列（fail-closed，不是「全部」）。
    ///
    /// 反例（改坏会红）：把 `in_window` 的 `Some(0)` 分支并到 `None`（视为全部），
    /// 本用例会返回今天的事件，断言失败。
    #[test]
    fn aggregate_zero_days_is_empty_not_all() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 15).unwrap();
        let events = vec![event("2026-10-15", 5.0, "a")];
        assert!(aggregate(&events, today, Some(0)).is_empty());
    }

    /// `days = None` ⇒ 全部历史（不做裁剪）。
    ///
    /// 反例（改坏会红）：把 `None` 分支当成 `Some(0)`（空），本用例会返回空，断言失败。
    #[test]
    fn aggregate_none_keeps_all_history() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 15).unwrap();
        let events = vec![event("2020-01-01", 1.0, "a"), event("2026-10-15", 1.0, "a")];
        assert_eq!(aggregate(&events, today, None).len(), 2);
    }

    // -- 隔离扫描（临时 home） ----------------------------------------------

    /// 建一个临时 home 并写入若干 jsonl；返回 (guard, 根目录)。
    fn isolated_home(tag: &str) -> (crate::modules::config::HomeOverrideGuard, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "qoder-usage-{tag}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&root).expect("建临时 home");
        let guard = crate::modules::config::HomeOverrideGuard::set(&root);
        (guard, root)
    }

    /// 往 `<root>/.qoder-cn/projects/<project>/<session>.jsonl` 写内容。
    fn write_cn_session(root: &Path, project: &str, session: &str, lines: &[String]) {
        let dir = root.join(".qoder-cn").join("projects").join(project);
        std::fs::create_dir_all(&dir).expect("建 projects 目录");
        let mut text = lines.join("\n");
        text.push('\n');
        std::fs::write(dir.join(format!("{session}.jsonl")), text).expect("写会话文件");
    }

    /// 隔离扫描：自己铺 jsonl 再扫（不依赖真机数据）。
    ///
    /// 反例（改坏会红）：把 `scan_for` 的目录从 `<home>/<cli_home>/projects` 改成别处，
    /// 本用例扫不到自铺的文件，断言失败。
    #[test]
    fn scan_days_for_reads_isolated_sessions() {
        let (_guard, root) = isolated_home("scan");
        let now = Local::now();
        let today = now.format("%Y-%m-%d").to_string();
        let stamp = now.to_rfc3339();
        write_cn_session(
            &root,
            "proj-a",
            "sess-1",
            &[
                assistant_line(&stamp, Some(1.0), "dfmodel", "sess-1"),
                // 坏行：应被跳过，不影响其余行。
                "{ broken".to_string(),
                // 缺 credits：不计入。
                assistant_line(&stamp, None, "dfmodel", "sess-1"),
            ],
        );
        write_cn_session(
            &root,
            "proj-a",
            "sess-2",
            &[assistant_line(&stamp, Some(2.5), "other-model", "sess-2")],
        );

        let days = scan_days_for(QoderRegion::Cn, None);
        assert_eq!(days.len(), 1, "两文件同一天应聚成一行");
        assert_eq!(days[0].date, today);
        assert!(approx(days[0].credits, 3.5), "credits={}", days[0].credits);
        assert_eq!(days[0].requests, 2, "坏行与缺 credits 的行都不计入");
        assert_eq!(days[0].by_model.len(), 2);
    }

    /// 区域隔离：只铺 CN 目录时，Global 扫描必须为空。
    ///
    /// 反例（改坏会红）：把 `scan_for` 的目录改成两区共用（忽略 region），
    /// Global 会读到 CN 的文件，断言失败。
    #[test]
    fn scan_is_isolated_per_region() {
        let (_guard, root) = isolated_home("region");
        let stamp = Local::now().to_rfc3339();
        write_cn_session(
            &root,
            "proj-a",
            "sess-1",
            &[assistant_line(&stamp, Some(1.0), "dfmodel", "sess-1")],
        );

        assert_eq!(scan_days_for(QoderRegion::Cn, None).len(), 1);
        assert!(scan_days_for(QoderRegion::Global, None).is_empty());
    }

    /// 目录不存在 ⇒ 空序列 + `source.exists=false`（不是错误）。
    ///
    /// 反例（改坏会红）：把 `scan_for` 里对不存在目录的处理改成 panic / 报错，
    /// 本用例无法返回，断言失败。
    #[test]
    fn missing_directory_yields_empty_with_honest_source() {
        let (_guard, _root) = isolated_home("missing");
        let view = usage_view_for(QoderRegion::Global, None);
        assert!(view["days"].as_array().unwrap().is_empty());
        assert_eq!(view["source"]["exists"], json!(false));
        assert_eq!(view["source"]["sessions"], json!(0));
        assert_eq!(view["totals"]["requests"], json!(0));
    }

    /// 下发形状：键集合齐备、`unit == "credits"`、`totals` 与 `days` 一致。
    ///
    /// 反例（改坏会红）：
    /// - 把 `unit` 改成 `"tokens"` ⇒ 第一条断言失败；
    /// - 让 `totals.credits` 走另一套口径（例如只统计最后一天）⇒ 与 days 累加不一致，断言失败。
    #[test]
    fn usage_view_shape_and_totals_match_days() {
        let (_guard, root) = isolated_home("shape");
        let now = Local::now();
        let today = now.format("%Y-%m-%d").to_string();
        let yesterday = (now - Duration::days(1)).format("%Y-%m-%d").to_string();
        write_cn_session(
            &root,
            "proj-a",
            "sess-1",
            &[
                assistant_line(&now.to_rfc3339(), Some(1.5), "m1", "sess-1"),
                assistant_line(
                    &(now - Duration::days(1)).to_rfc3339(),
                    Some(2.0),
                    "m2",
                    "sess-1",
                ),
            ],
        );

        let view = usage_view_for(QoderRegion::Cn, None);

        // 键集合（含额外提供的 byModel）。
        for key in ["region", "unit", "days", "totals", "byModel", "source"] {
            assert!(view.get(key).is_some(), "缺少顶层字段 {key}");
        }
        for key in ["credits", "requests", "sessions", "models"] {
            assert!(view["totals"].get(key).is_some(), "缺少 totals.{key}");
        }
        for key in [
            "kind",
            "paths",
            "sessions",
            "exists",
            "tokenCountsAvailable",
            "note",
        ] {
            assert!(view["source"].get(key).is_some(), "缺少 source.{key}");
        }

        // 单位必须如实是 credits。
        assert_eq!(view["unit"], json!("credits"));
        assert_eq!(view["region"], json!("cn"));
        assert_eq!(view["source"]["kind"], json!(SOURCE_KIND));
        assert_eq!(view["source"]["tokenCountsAvailable"], json!(false));

        // totals 与 days 严格一致。
        let days = view["days"].as_array().unwrap();
        let sum_credits: f64 = days.iter().map(|d| d["credits"].as_f64().unwrap()).sum();
        let sum_requests: u64 = days.iter().map(|d| d["requests"].as_u64().unwrap()).sum();
        assert!(approx(
            view["totals"]["credits"].as_f64().unwrap(),
            sum_credits
        ));
        assert_eq!(view["totals"]["requests"].as_u64().unwrap(), sum_requests);

        // 两天都有数据，且 byModel 跨天汇总为 2 个模型。
        assert_eq!(days.len(), 2);
        assert_eq!(view["byModel"].as_array().unwrap().len(), 2);
        assert_eq!(
            view["totals"]["sessions"].as_u64().unwrap(),
            1,
            "同一会话只算一次"
        );
        assert_eq!(view["totals"]["models"].as_u64().unwrap(), 2);
        assert!(days.iter().any(|d| d["date"] == json!(today)));
        assert!(days.iter().any(|d| d["date"] == json!(yesterday)));
    }

    /// `days = Some(0)` 在下发形态里也是空：days 空、totals 归零。
    ///
    /// 反例（改坏会红）：让 `usage_view_for` 对 `Some(0)` 退化成「全部」，
    /// days 会非空、totals 非零，断言失败。
    #[test]
    fn usage_view_zero_days_is_empty() {
        let (_guard, root) = isolated_home("zerodays");
        let stamp = Local::now().to_rfc3339();
        write_cn_session(
            &root,
            "proj-a",
            "sess-1",
            &[assistant_line(&stamp, Some(9.0), "m1", "sess-1")],
        );

        let view = usage_view_for(QoderRegion::Cn, Some(0));
        assert!(view["days"].as_array().unwrap().is_empty());
        assert_eq!(view["totals"]["credits"], json!(0.0));
        assert_eq!(view["totals"]["requests"], json!(0));
        // source 仍如实反映扫到的文件数（不是 0 —— 文件确实在）。
        assert_eq!(view["source"]["sessions"], json!(1));
    }
}
