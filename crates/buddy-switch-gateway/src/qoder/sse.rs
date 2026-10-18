//! Qoder 信封流 → 下游帧：**SSE 转发**（逐帧解包）与**聚合**（非流式）。
//!
//! ## 上游帧形态（见 `core::modules::qoder::api::unwrap_envelope_line`）
//!
//! ```text
//! data:{"headers":{…},"body":"<内层 OpenAI chunk>","statusCodeValue":200}
//! data:{"body":"[DONE]"}
//! event:finish{…}   ← 计时元数据（忽略）
//! ```
//!
//! ## 两条硬约束（与 Trae / WorkBuddy 两侧同一条纪律）
//!
//! 1. **绝不悬挂连接**：无论上游正常结束、中途断流、还是报错，都必须补发
//!    `data: [DONE]` —— VS Code 系客户端会一直等到它为止。
//! 2. **`stream: false` 也走上游的流式**：上游只有 SSE 端点；非流式请求由
//!    [`aggregate`] 在本地折叠成单个 `chat.completion`。

use std::collections::VecDeque;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::body::Bytes;
use futures_util::{Stream, StreamExt};
use serde_json::{json, Map, Value};

use buddy_switch_core::modules::qoder::api::{self, ChatFrame};

/// 一行是否含 `[DONE]` 哨兵（转发层不需要精确判断，聚合层走结构化解包）。
fn process_line(line: &str) -> (Vec<Bytes>, bool) {
    match api::unwrap_envelope_line(line) {
        ChatFrame::Skip => (Vec::new(), false),
        ChatFrame::Chunk(cleaned) => (vec![Bytes::from(format!("data: {cleaned}\n\n"))], false),
        ChatFrame::Done => (Vec::new(), true),
        ChatFrame::UpstreamError { status, detail } => {
            // 错误帧带**可操作信息**（状态码 + 上游详情）再收尾 —— 只补 [DONE] 会让
            // 客户端把「上游 429」显示成「回答就这么短」。
            let frame = json!({
                "error": {
                    "message": format!("上游错误（HTTP {status}）：{detail}"),
                    "type": "upstream_error",
                    "code": status,
                }
            });
            (vec![Bytes::from(format!("data: {frame}\n\n"))], true)
        }
    }
}

/// 把上游字节流适配为**下游 OpenAI SSE 字节流**（逐帧解包 + [DONE] 兜底）。
pub struct QoderOpenAiStream<S> {
    inner: Pin<Box<S>>,
    buffer: Vec<u8>,
    pending: VecDeque<Bytes>,
    upstream_done: bool,
    done_sent: bool,
    terminated: bool,
}

impl<S> QoderOpenAiStream<S> {
    pub fn new(inner: S) -> Self {
        Self {
            inner: Box::pin(inner),
            buffer: Vec::new(),
            pending: VecDeque::new(),
            upstream_done: false,
            done_sent: false,
            terminated: false,
        }
    }

    fn drain_lines(&mut self) {
        while let Some(position) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.buffer.drain(..=position).collect();
            let text = String::from_utf8_lossy(&line);
            let (frames, done) = process_line(&text);
            self.pending.extend(frames);
            if done {
                self.upstream_done = true;
            }
        }
    }

    fn push_done(&mut self) {
        if !self.done_sent {
            self.pending
                .push_back(Bytes::from_static(b"data: [DONE]\n\n"));
            self.done_sent = true;
        }
    }
}

impl<S, E> Stream for QoderOpenAiStream<S>
where
    S: Stream<Item = Result<Bytes, E>>,
    E: std::fmt::Debug,
{
    type Item = Result<Bytes, io::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        loop {
            if let Some(frame) = this.pending.pop_front() {
                return Poll::Ready(Some(Ok(frame)));
            }
            if this.terminated {
                return Poll::Ready(None);
            }
            if this.upstream_done {
                this.push_done();
                this.terminated = true;
                continue;
            }
            match this.inner.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(chunk))) => {
                    this.buffer.extend_from_slice(&chunk);
                    this.drain_lines();
                }
                Poll::Ready(Some(Err(_error))) => {
                    // 中途异常：补 [DONE] 收尾（不悬挂），错误详情由请求侧日志记录。
                    this.upstream_done = true;
                }
                Poll::Ready(None) => {
                    // 上游自然结束：处理残余缓冲（末行可能没有换行）。
                    if !this.buffer.is_empty() {
                        let text = String::from_utf8_lossy(&this.buffer).into_owned();
                        this.buffer.clear();
                        let (frames, done) = process_line(&text);
                        this.pending.extend(frames);
                        if done {
                            this.upstream_done = true;
                        }
                    }
                    if this.upstream_done {
                        this.push_done();
                        this.terminated = true;
                    } else {
                        // 没见到 [DONE] 就断了（或末尾就是最后一帧）——补上，绝不悬挂。
                        this.upstream_done = true;
                    }
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// 非流式聚合结果。
#[derive(Debug, Default, Clone)]
pub struct AggregatedChat {
    pub id: Option<String>,
    pub model: Option<String>,
    pub created: Option<i64>,
    pub content: String,
    pub reasoning: String,
    pub tool_calls: Vec<Value>,
    pub finish_reason: String,
    pub usage: Option<Value>,
    /// 上游中途报错时的可读信息（与 `content` 并存时以错误为准）。
    pub upstream_error: Option<String>,
}

/// 把上游信封流折叠成一个 `chat.completion` 数据（与官方客户端语义对齐）。
///
/// - `content` 支持字符串或 parts 数组两种 delta 形态；
/// - `tool_calls` 按 `index` 归并（分片到达的参数做字符串拼接）；
/// - `reasoning_content` 单独收集（不进正文）；
/// - **泄漏回声回读**：上游没给结构化 `tool_calls` 但正文恰好是
///   `[assistant 请求调用工具]` + JSON 数组形态 ⇒ 还原为结构化并清空正文
///   （严格整段识别，见 `core::modules::qoder::api::parse_leaked_tool_calls`）。
pub async fn aggregate<S, E>(
    upstream: S,
    allowed_tool_names: Option<&std::collections::HashSet<String>>,
) -> AggregatedChat
where
    S: Stream<Item = Result<Bytes, E>>,
    E: std::fmt::Debug,
{
    let mut out = AggregatedChat {
        finish_reason: "stop".to_string(),
        ..Default::default()
    };
    let mut tool_map: std::collections::BTreeMap<i64, Map<String, Value>> =
        std::collections::BTreeMap::new();
    let stream = QoderOpenAiStream::new(upstream);
    futures_util::pin_mut!(stream);

    while let Some(item) = stream.next().await {
        let Ok(frame) = item else { break };
        let text = String::from_utf8_lossy(&frame);
        let Some(data) = text.strip_prefix("data: ") else {
            continue;
        };
        let data = data.trim();
        if data == "[DONE]" {
            break;
        }
        let Ok(chunk) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        if let Some(error) = chunk.get("error") {
            out.upstream_error = Some(
                error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("上游错误")
                    .to_string(),
            );
            continue;
        }
        if let Some(id) = chunk.get("id").and_then(Value::as_str) {
            out.id = Some(id.to_string());
        }
        if let Some(model) = chunk.get("model").and_then(Value::as_str) {
            out.model = Some(model.to_string());
        }
        if let Some(created) = chunk.get("created").and_then(Value::as_i64) {
            out.created = Some(created);
        }
        if let Some(usage) = chunk.get("usage") {
            if !usage.is_null() {
                out.usage = Some(usage.clone());
            }
        }
        let Some(choices) = chunk.get("choices").and_then(Value::as_array) else {
            continue;
        };
        for choice in choices {
            if let Some(delta) = choice.get("delta") {
                match delta.get("content") {
                    Some(Value::String(text)) => out.content.push_str(text),
                    Some(Value::Array(parts)) => {
                        for part in parts {
                            match part {
                                Value::String(text) => out.content.push_str(text),
                                Value::Object(object) => {
                                    if let Some(text) = object.get("text").and_then(Value::as_str) {
                                        out.content.push_str(text);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
                if let Some(reasoning) = delta.get("reasoning_content").and_then(Value::as_str) {
                    out.reasoning.push_str(reasoning);
                }
                if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
                    for call in calls {
                        let index = call
                            .get("index")
                            .and_then(Value::as_i64)
                            .unwrap_or(tool_map.len() as i64);
                        let entry = tool_map.entry(index).or_insert_with(|| {
                            let mut map = Map::new();
                            map.insert(
                                "id".into(),
                                call.get("id").cloned().unwrap_or_else(|| {
                                    Value::String(format!("call_{}", uuid::Uuid::new_v4().simple()))
                                }),
                            );
                            map.insert(
                                "type".into(),
                                Value::String(
                                    call.get("type")
                                        .and_then(Value::as_str)
                                        .unwrap_or("function")
                                        .to_string(),
                                ),
                            );
                            map.insert("function".into(), json!({"name": "", "arguments": ""}));
                            map
                        });
                        if let Some(id) = call.get("id").and_then(Value::as_str) {
                            entry.insert("id".into(), Value::String(id.to_string()));
                        }
                        if let Some(function) = call.get("function") {
                            let slot = entry
                                .get_mut("function")
                                .and_then(Value::as_object_mut)
                                .expect("function 槽位已在上面建立");
                            if let Some(name) = function.get("name").and_then(Value::as_str) {
                                let current = slot
                                    .get("name")
                                    .and_then(Value::as_str)
                                    .unwrap_or("")
                                    .to_string();
                                slot.insert(
                                    "name".into(),
                                    Value::String(format!("{current}{name}")),
                                );
                            }
                            if let Some(arguments) =
                                function.get("arguments").and_then(Value::as_str)
                            {
                                let current = slot
                                    .get("arguments")
                                    .and_then(Value::as_str)
                                    .unwrap_or("")
                                    .to_string();
                                slot.insert(
                                    "arguments".into(),
                                    Value::String(format!("{current}{arguments}")),
                                );
                            }
                        }
                    }
                }
                if let Some(function_call) = delta.get("function_call") {
                    if function_call.get("name").and_then(Value::as_str).is_some() {
                        let entry = tool_map.entry(0).or_insert_with(|| {
                            json!({
                                "id": format!("call_{}", uuid::Uuid::new_v4().simple()),
                                "type": "function",
                                "function": {"name": "", "arguments": ""},
                            })
                            .as_object()
                            .cloned()
                            .unwrap_or_default()
                        });
                        let slot = entry
                            .get_mut("function")
                            .and_then(Value::as_object_mut)
                            .expect("function 槽位已在上面建立");
                        if let Some(name) = function_call.get("name").and_then(Value::as_str) {
                            slot.insert("name".into(), Value::String(name.to_string()));
                        }
                        if let Some(arguments) =
                            function_call.get("arguments").and_then(Value::as_str)
                        {
                            let current = slot
                                .get("arguments")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            slot.insert(
                                "arguments".into(),
                                Value::String(format!("{current}{arguments}")),
                            );
                        }
                    }
                }
            }
            if let Some(finish) = choice.get("finish_reason").and_then(Value::as_str) {
                out.finish_reason = finish.to_string();
            }
        }
    }

    out.tool_calls = tool_map.into_values().map(Value::Object).collect();

    // 泄漏回声回读：只在**没有**结构化调用、且正文非空时尝试（严格整段识别）。
    if out.tool_calls.is_empty() && !out.content.is_empty() {
        let (recovered, cleaned) = api::parse_leaked_tool_calls(&out.content, allowed_tool_names);
        if let Some(calls) = recovered {
            out.tool_calls = calls;
            out.content = cleaned;
            if out.finish_reason == "stop" {
                out.finish_reason = "tool_calls".to_string();
            }
        }
    }
    out
}

/// 旁路取 `usage` / 上游错误的流适配器：逐帧透传，顺手把含 `usage` 或 `error`
/// 的 chunk 记进槽位。
///
/// **必须挂在 Anthropic 转换之前**（转换后帧是 Anthropic 事件，取不到 OpenAI 的
/// `usage` 字段）。放在流链里而不是放进消费循环 —— 两种入口（OpenAI / Anthropic）
/// 共用同一条链，取数自然同源。
///
/// ★ 错误槽位是「故障不隐身」的一半：流式请求的账本在**流结束**时才落，若上游中途
/// 发了错误帧而我们只记 200，日志就会显示「200 + 0 token + 无错误」——正是
/// issue #46 里最难排查的那种形态。
pub struct UsageTapStream<S> {
    inner: S,
    slot: std::sync::Arc<std::sync::Mutex<Option<Value>>>,
    error_slot: std::sync::Arc<std::sync::Mutex<Option<String>>>,
}

impl<S> UsageTapStream<S> {
    pub fn new(
        inner: S,
        slot: std::sync::Arc<std::sync::Mutex<Option<Value>>>,
        error_slot: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    ) -> Self {
        Self {
            inner,
            slot,
            error_slot,
        }
    }
}

/// 从一帧下游数据里解析 `usage` / `error`（不含这两个键的帧直接跳过）。
fn tap_frame(
    frame: &Bytes,
    slot: &std::sync::Mutex<Option<Value>>,
    error_slot: &std::sync::Mutex<Option<String>>,
) {
    let Ok(text) = std::str::from_utf8(frame) else {
        return;
    };
    let Some(data) = text.strip_prefix("data: ") else {
        return;
    };
    let data = data.trim();
    if data == "[DONE]" || !(data.contains("usage") || data.contains("error")) {
        return;
    }
    if let Ok(chunk) = serde_json::from_str::<Value>(data) {
        // 错误帧（`{"error":{"message":…}}`）单独留痕：客户端已经收到它，账本也要记。
        if let Some(error) = chunk.get("error") {
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| error.to_string());
            if let Ok(mut guard) = error_slot.lock() {
                if guard.is_none() {
                    *guard = Some(message.chars().take(400).collect());
                }
            }
        }
        if let Some(usage) = chunk.get("usage") {
            if !usage.is_null() {
                if let Ok(mut guard) = slot.lock() {
                    *guard = Some(usage.clone());
                }
            }
        }
    }
}

impl<S, E> Stream for UsageTapStream<S>
where
    S: Stream<Item = Result<Bytes, E>> + Unpin,
{
    type Item = Result<Bytes, E>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        match Pin::new(&mut this.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(frame))) => {
                tap_frame(&frame, &this.slot, &this.error_slot);
                Poll::Ready(Some(Ok(frame)))
            }
            other => other,
        }
    }
}

/// mpsc 接收端 → 流。
pub fn stream_from_receiver(
    rx: tokio::sync::mpsc::Receiver<Result<Bytes, io::Error>>,
) -> impl Stream<Item = Result<Bytes, io::Error>> {
    futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|item| (item, rx))
    })
}

/// SSE 响应（`text/event-stream` + 无缓存头；客户端断开即 drop）。
pub fn response_from_receiver(
    rx: tokio::sync::mpsc::Receiver<Result<Bytes, io::Error>>,
) -> axum::response::Response {
    axum::response::Response::builder()
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .body(axum::body::Body::from_stream(stream_from_receiver(rx)))
        .unwrap_or_else(|_| axum::response::Response::new(axum::body::Body::empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upstream_from(frames: &[String]) -> impl Stream<Item = Result<Bytes, io::Error>> {
        let items: Vec<Result<Bytes, io::Error>> = frames
            .iter()
            .map(|frame| Ok(Bytes::from(frame.clone())))
            .collect();
        futures_util::stream::iter(items)
    }

    /// 用 serde 构造信封帧（body 是内层 JSON 的**字符串**）—— 免去手写转义。
    fn frame(body: &Value) -> String {
        format!(
            "data:{}\n",
            json!({ "body": body.to_string(), "statusCodeValue": 200 })
        )
    }

    async fn collect(
        stream: &mut (impl Stream<Item = Result<Bytes, io::Error>> + Unpin),
    ) -> String {
        let mut out = String::new();
        while let Some(item) = stream.next().await {
            out.push_str(&String::from_utf8_lossy(&item.unwrap()));
        }
        out
    }

    /// 信封 → 下游帧：chunk 透传（已清洗）、[DONE] 兜底、错误帧带信息。
    ///
    /// 变异：删掉 [DONE] 兜底 ⇒ 第二条断言红（客户端会悬挂）。
    #[tokio::test]
    async fn stream_relays_frames_and_always_sends_done() {
        // ① 正常流：chunk + [DONE]。
        let frames = vec![
            frame(&json!({"choices": [{"delta": {"content": "你好"}}]})),
            "data:{\"body\":\"[DONE]\"}\n".to_string(),
        ];
        let mut stream = QoderOpenAiStream::new(upstream_from(&frames));
        let text = collect(&mut stream).await;
        assert!(text.contains("\"content\":\"你好\""), "{text}");
        assert!(text.trim_end().ends_with("data: [DONE]"), "{text}");

        // ② 上游**没发** [DONE] 就断流 ⇒ 仍然补上。
        let frames = vec![frame(&json!({"choices": [{"delta": {"content": "x"}}]}))];
        let mut stream = QoderOpenAiStream::new(upstream_from(&frames));
        let text = collect(&mut stream).await;
        assert!(
            text.trim_end().ends_with("data: [DONE]"),
            "断流必须补 [DONE]: {text}"
        );

        // ③ 错误帧带状态码与详情，然后 [DONE]。
        let frames = vec!["data:{\"body\":\"boom\",\"statusCodeValue\":429}\n".to_string()];
        let mut stream = QoderOpenAiStream::new(upstream_from(&frames));
        let text = collect(&mut stream).await;
        assert!(text.contains("\"code\":429"), "{text}");
        assert!(text.contains("boom"), "{text}");
        assert!(text.trim_end().ends_with("data: [DONE]"), "{text}");
    }

    /// 聚合：content/reasoning/tool_calls 归并 + usage + finish_reason。
    #[tokio::test]
    async fn aggregate_folds_chunks() {
        let frames = vec![
            frame(&json!({"id": "c1", "created": 123, "choices": [{"delta": {"content": "你"}}]})),
            frame(
                &json!({"id": "c1", "choices": [{"delta": {"content": "好", "reasoning_content": "想"}}]}),
            ),
            frame(
                &json!({"choices": [{"delta": {"tool_calls": [{"index": 0, "id": "call_1", "function": {"name": "sea", "arguments": "{\"q\":\""}}]}}]}),
            ),
            frame(
                &json!({"choices": [{"delta": {"tool_calls": [{"index": 0, "function": {"arguments": "bj\""}}]}, "finish_reason": "tool_calls"}]}),
            ),
            frame(
                &json!({"usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}}),
            ),
        ];
        let out = aggregate(upstream_from(&frames), None).await;
        assert_eq!(out.content, "你好");
        assert_eq!(out.reasoning, "想");
        assert_eq!(out.id.as_deref(), Some("c1"));
        assert_eq!(out.created, Some(123));
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(out.tool_calls[0]["id"], "call_1");
        // 分片拼接：name 分片 `sea`；arguments `{"q":"` + `bj"`。
        assert_eq!(out.tool_calls[0]["function"]["name"], "sea");
        assert_eq!(out.tool_calls[0]["function"]["arguments"], "{\"q\":\"bj\"");
        assert_eq!(out.finish_reason, "tool_calls");
        assert_eq!(out.usage.unwrap()["total_tokens"], 15);
    }

    /// 泄漏回声：严格形态被还原成结构化调用并清空正文。
    #[tokio::test]
    async fn aggregate_recovers_leaked_tool_calls() {
        let content = "[assistant 请求调用工具]\n[{\"name\":\"search\",\"arguments\":\"{}\"}]";
        let frames = vec![frame(
            &json!({"choices": [{"delta": {"content": content}}]}),
        )];
        let out = aggregate(upstream_from(&frames), None).await;
        assert_eq!(out.content, "", "回声必须从正文清掉");
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(out.tool_calls[0]["function"]["name"], "search");
        assert_eq!(out.finish_reason, "tool_calls");
    }

    /// 上游错误帧：聚合结果里带可读错误。
    #[tokio::test]
    async fn aggregate_surfaces_upstream_error() {
        let frames =
            vec!["data:{\"body\":\"quota exceeded\",\"statusCodeValue\":429}\n".to_string()];
        let out = aggregate(upstream_from(&frames), None).await;
        let error = out.upstream_error.expect("必须带上游错误");
        assert!(error.contains("429"), "{error}");
        assert!(error.contains("quota exceeded"), "{error}");
    }

    /// 错误帧必须被**旁路留痕**：流式账本在流结束才落，没有这一槽就只能记
    /// 「200 + 0 token + error:null」——正是 issue #46 里隐身的那条日志。
    ///
    /// 可证伪：去掉 `tap_frame` 里的 error 分支 ⇒ 第一条断言红。
    #[tokio::test]
    async fn usage_tap_records_error_frames() {
        use std::sync::{Arc, Mutex};

        let frames = vec!["data:{\"body\":\"boom\",\"statusCodeValue\":429}\n".to_string()];
        let usage = Arc::new(Mutex::new(None::<Value>));
        let error = Arc::new(Mutex::new(None::<String>));
        let mut tap = UsageTapStream::new(
            QoderOpenAiStream::new(upstream_from(&frames)),
            usage.clone(),
            error.clone(),
        );
        let text = collect(&mut tap).await;
        assert!(
            text.contains("\"code\":429"),
            "错误帧仍要透传给客户端: {text}"
        );
        let captured = error
            .lock()
            .expect("锁未中毒")
            .clone()
            .expect("错误必须留痕");
        assert!(captured.contains("429"), "{captured}");
        assert!(
            usage.lock().expect("锁未中毒").is_none(),
            "错误帧不带 usage"
        );

        // 正常流：usage 被取到，错误槽保持空（别把正常请求记成失败）。
        let frames = vec![
            frame(&json!({"choices": [{"delta": {"content": "好"}}]})),
            frame(
                &json!({"usage": {"prompt_tokens": 3, "completion_tokens": 4, "total_tokens": 7}}),
            ),
        ];
        let usage = Arc::new(Mutex::new(None::<Value>));
        let error = Arc::new(Mutex::new(None::<String>));
        let mut tap = UsageTapStream::new(
            QoderOpenAiStream::new(upstream_from(&frames)),
            usage.clone(),
            error.clone(),
        );
        let _ = collect(&mut tap).await;
        assert_eq!(
            usage.lock().expect("锁未中毒").as_ref().unwrap()["total_tokens"],
            7
        );
        assert!(error.lock().expect("锁未中毒").is_none(), "正常流不得记错");
    }
}
