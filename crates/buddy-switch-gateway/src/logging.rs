//! 请求日志（**默认只记元数据**，可配置正文开关，保留 N 条；P1-1 / Q5）。
//!
//! ## 写入时机：流式请求**推迟到流结束**
//!
//! 非流式请求在 handler 里就地写日志（响应体已读全，用量已知）。
//! **流式请求不一样**：`usage` 只在 SSE 末帧才到达，而日志条目要在响应返回前写
//! —— 此前只能把 `promptTokens` / `completionTokens` **写死 0**（2026-10-14 实测本机
//! 200/200 条全为 0）。⇒ 改用 [`StreamingLogSink`]：日志挂在 `UsageTap` 的结束回调上，
//! 流结束时才落盘，此时用量已知。
//!
//! 副作用（有意）：流式条目的 `latencyMs` 从「首字节前耗时」变成**整次请求总耗时** ——
//! 对用户而言后者才是可感知的耗时。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde_json::{json, Value};

use buddy_switch_core::modules::config as core_config;
use buddy_switch_core::modules::region::Region;

use crate::protocol::usage_tap::{UsageReport, UsageSink};

/// 一次请求的元数据（不含 prompt/response 正文）。
#[derive(Debug, Clone)]
pub struct RequestMeta {
    pub endpoint: &'static str,
    pub method: &'static str,
    pub region: Region,
    /// 命中账号的**展示名**（可能重名 / 为空；仅供人读，不作身份）。
    pub account: String,
    /// 命中账号的 **uid**（写入时的**精确身份**，供 `token_daily` 精确 join）。
    pub account_uid: String,
    pub model: String,
    pub status: u16,
    pub latency_ms: i64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub stream: bool,
}

impl RequestMeta {
    /// 序列化为日志条目。
    pub fn to_value(&self) -> Value {
        json!({
            "ts": core_config::now_ms(),
            "endpoint": self.endpoint,
            "method": self.method,
            "region": self.region,
            "account": self.account,
            "accountUid": self.account_uid,
            "model": self.model,
            "status": self.status,
            "latencyMs": self.latency_ms,
            "promptTokens": self.prompt_tokens,
            "completionTokens": self.completion_tokens,
            "stream": self.stream,
        })
    }
}

/// 流式请求的**延迟日志写入器**（见模块头「写入时机」）。
///
/// ## 为什么需要它（2026-10-14 实测驱动）
///
/// 流式路径此前在**流开始前**就把日志写了，那时 `usage` 还没到达 ⇒
/// `promptTokens` / `completionTokens` **只能写死 0**。而出站管线强制上游流式、
/// 客户端（WorkBuddy 桌面 / CLI）默认也流式 ⇒ **实际流量 100% 命中该分支**：
/// 请求日志的 Token 列与「今日 Token」的网关列**恒为 0**（本机 200/200 条全 0）。
///
/// ## 语义
///
/// - **拿不到用量也要写**（填 0）：这次请求**发生过**（状态码 / 账号 / 模型 / 耗时
///   都是事实），不该因为拿不到用量就从日志里消失。
/// - `latency_ms` 由本 sink 在流结束时按**总耗时**覆盖 —— `template` 里的那个值是占位，
///   不参与输出。
pub struct StreamingLogSink {
    log: Arc<RequestLog>,
    /// 除 token 与耗时外的全部字段（流开始时即已确定）。
    template: RequestMeta,
    started: Instant,
}

impl StreamingLogSink {
    /// `template` 的 `prompt_tokens` / `completion_tokens` / `latency_ms` 会被覆盖，
    /// 传 0 即可（其余字段必须填实）。
    pub fn new(log: Arc<RequestLog>, template: RequestMeta, started: Instant) -> Self {
        Self {
            log,
            template,
            started,
        }
    }
}

impl UsageSink for StreamingLogSink {
    fn finish(&self, usage: Option<UsageReport>) {
        // 与 `PoolUsageSink` 相反：**没有用量也要落日志**（用量填 0）。
        let (prompt_tokens, completion_tokens) = match usage {
            Some(usage) => (
                u64::try_from(usage.prompt_tokens).unwrap_or(0),
                u64::try_from(usage.completion_tokens).unwrap_or(0),
            ),
            None => (0, 0),
        };
        self.log.record(
            RequestMeta {
                prompt_tokens,
                completion_tokens,
                latency_ms: self.started.elapsed().as_millis() as i64,
                ..self.template.clone()
            }
            .to_value(),
        );
    }
}

/// 请求日志：内存 + 落盘，保留最近 N 条。
pub struct RequestLog {
    path: PathBuf,
    keep: Mutex<usize>,
    log_bodies: AtomicBool,
    entries: Mutex<Vec<Value>>,
}

impl RequestLog {
    /// 新建日志（读取既有落盘条目）。
    pub fn new(path: PathBuf, keep: usize, log_bodies: bool) -> Self {
        let entries = match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str::<Vec<Value>>(&text).unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        Self {
            path,
            keep: Mutex::new(keep.max(1)),
            log_bodies: AtomicBool::new(log_bodies),
            entries: Mutex::new(entries),
        }
    }

    /// 是否记录正文。
    pub fn log_bodies(&self) -> bool {
        self.log_bodies.load(Ordering::SeqCst)
    }

    /// 更新保留条数并裁剪。
    pub fn set_keep(&self, keep: usize) {
        *self.keep.lock().unwrap() = keep.max(1);
        self.trim_and_save();
    }

    /// 更新正文开关。
    pub fn set_log_bodies(&self, enabled: bool) {
        self.log_bodies.store(enabled, Ordering::SeqCst);
    }

    /// 追加一条日志（元数据），并裁剪到保留上限。
    pub fn record(&self, entry: Value) {
        {
            let mut entries = self.entries.lock().unwrap();
            entries.push(entry);
        }
        self.trim_and_save();
    }

    /// 返回最近 N 条（按时间升序）。
    pub fn list(&self) -> Vec<Value> {
        self.entries.lock().unwrap().clone()
    }

    /// 清空日志（内存 + 落盘）。
    pub fn clear(&self) {
        self.entries.lock().unwrap().clear();
        self.save(&[]);
    }

    fn trim_and_save(&self) {
        let keep = *self.keep.lock().unwrap();
        let snapshot = {
            let mut entries = self.entries.lock().unwrap();
            if entries.len() > keep {
                let drop_count = entries.len() - keep;
                entries.drain(..drop_count);
            }
            entries.clone()
        };
        self.save(&snapshot);
    }

    fn save(&self, entries: &[Value]) {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let content = serde_json::to_string_pretty(entries).unwrap_or_else(|_| "[]".to_string());
        let _ = core_config::atomic_write(&self.path, &content);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 日志条目必须携带写入时的**精确 uid**（`accountUid`），且 `account`（展示名）语义不变。
    #[test]
    fn request_meta_carries_account_uid_alongside_display_name() {
        let meta = RequestMeta {
            endpoint: "/v1/chat/completions",
            method: "POST",
            region: Region::Cn,
            account: "重名展示名".to_string(),
            account_uid: "uid-b".to_string(),
            model: "hy4".to_string(),
            status: 200,
            latency_ms: 12,
            prompt_tokens: 10,
            completion_tokens: 2,
            stream: false,
        };

        let value = meta.to_value();
        assert_eq!(value["accountUid"], "uid-b", "必须写入选中账号的精确 uid");
        assert_eq!(value["account"], "重名展示名", "account 仍是展示名，语义不变");
    }

    /// 造一个「流式请求」的日志模板（token / 耗时留 0，由 sink 覆盖）。
    fn streaming_template() -> RequestMeta {
        RequestMeta {
            endpoint: "/v1/chat/completions",
            method: "POST",
            region: Region::Cn,
            account: "一号".to_string(),
            account_uid: "uid-1".to_string(),
            model: "hy4".to_string(),
            status: 200,
            latency_ms: 0,
            prompt_tokens: 0,
            completion_tokens: 0,
            stream: true,
        }
    }

    /// 独立临时日志文件（各测各的路径，避免并跑互相踩）。
    fn temp_log(tag: &str) -> (Arc<RequestLog>, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("bs-log-sink-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("创建隔离目录");
        let path = dir.join("gateway_logs.json");
        let _ = std::fs::remove_file(&path);
        (Arc::new(RequestLog::new(path, 50, false)), dir)
    }

    /// ★★ 流式路径的日志必须在**流结束时**带上**真实** token。
    ///
    /// 缺陷现场（2026-10-14）：流式分支把 token 写死 0，本机 200/200 条日志全 0，
    /// 请求日志的 Token 列与「今日 Token」的网关列因此永远是 0。
    ///
    /// 可证伪：把 `StreamingLogSink::finish` 的 `Some(usage)` 分支去掉（一律写 0）
    /// ⇒ 本用例在 `promptTokens` 上变红。
    #[tokio::test]
    async fn streaming_log_sink_writes_real_tokens_at_stream_end() {
        use crate::protocol::usage_tap::UsageTap;
        use futures_util::StreamExt;

        let (log, dir) = temp_log("tokens");
        let sink: Arc<dyn UsageSink> =
            Arc::new(StreamingLogSink::new(log.clone(), streaming_template(), Instant::now()));
        let frames = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n",
            "data: {\"choices\":[],\"usage\":{\"credit\":1.5,\"prompt_tokens\":10,\"completion_tokens\":5}}\n\n",
            "data: [DONE]\n\n",
        );
        let stream = futures_util::stream::iter(vec![Ok::<_, std::io::Error>(
            axum::body::Bytes::from_static(frames.as_bytes()),
        )]);
        let _ = UsageTap::new(stream, vec![sink]).collect::<Vec<_>>().await;

        let entries = log.list();
        assert_eq!(entries.len(), 1, "流结束应恰好写一条日志（幂等，不多不少）");
        assert_eq!(
            entries[0]["promptTokens"], 10,
            "流式路径必须写**真实** prompt token —— 写死 0 正是本次修的缺陷"
        );
        assert_eq!(entries[0]["completionTokens"], 5);
        assert_eq!(entries[0]["accountUid"], "uid-1", "静态字段不得丢");
        assert_eq!(entries[0]["stream"], true);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// ★ 流被**掐断**（客户端读到一半断开 / 响应体从未被消费）时也必须留下一条日志。
    ///
    /// 这是「日志推迟到流结束」这个改动的**安全网**：少了它，被掐断的请求会从日志里
    /// 彻底消失（比原来「写一条 token=0」更糟 —— 连「发生过」都看不到）。
    ///
    /// 可证伪：删掉 `impl Drop for UsageTap` ⇒ 本用例变红（`entries` 为空）。
    #[tokio::test]
    async fn streaming_log_sink_still_writes_when_the_stream_is_dropped_early() {
        use crate::protocol::usage_tap::UsageTap;

        let (log, dir) = temp_log("dropped");
        let sink: Arc<dyn UsageSink> =
            Arc::new(StreamingLogSink::new(log.clone(), streaming_template(), Instant::now()));
        {
            // 空流且从未被 poll ⇒ 只能靠 Drop 兜底。
            let _tap = UsageTap::new(
                futures_util::stream::iter(Vec::<Result<axum::body::Bytes, std::io::Error>>::new()),
                vec![sink],
            );
        }

        let entries = log.list();
        assert_eq!(entries.len(), 1, "被丢弃的流也必须留下一条日志");
        assert_eq!(entries[0]["promptTokens"], 0, "用量未知 ⇒ 记 0（而不是丢日志）");
        assert_eq!(entries[0]["accountUid"], "uid-1");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
