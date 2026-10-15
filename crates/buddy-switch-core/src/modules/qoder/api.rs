//! Qoder 上游协议客户端 —— **两个通道**：
//!
//! | 通道 | 端点族 | 认证 |
//! |:--|:--|:--|
//! | **COSY 推理通道** | `/algo/api/v2/service/pro/sse/agent_chat_generation`（SSE）、`/algo/api/v2/model/list` | COSY 签名头（见 [`cosy`]） |
//! | **openapi 账号通道** | `/api/v1/deviceToken/refresh`、`/api/v1/jobToken/*`、`/sash/api/v1/me/daily-check-in/*`、`/api/v2/quota/usage`、`/api/v2/user/plan` | `Authorization: Bearer <access_token>` |
//!
//! ## 协议要点（都是实测形态，别凭直觉改）
//!
//! - **请求体过 [`cosy::qoder_encode`] 编码**后发送；**签名与 body 必须一致**
//!   （模型清单这种 GET 也要**带 body** `qoder_encode("{}")`，裸 GET 会 403）。
//! - 聊天响应是**信封 SSE**：`data:{"body":"<内层 OpenAI chunk>","statusCodeValue":200}`，
//!   结束帧 `data:{"body":"[DONE]"}`，`event:finish*` 是计时元数据（忽略）。
//!   解包见 [`unwrap_envelope_line`]，噪声字段清洗见 [`clean_chunk`]。
//! - 请求体以官方 baseprompt 为骨架（[`base_template`]），每次覆写 id / 模型配置 /
//!   会话 / 工具 / 参数 / business。
//! - 刷新通道是**普通 JSON POST**（不签名、不带 Bearer）；`expires_in` 单位是**毫秒**。
//! - 签到通道在 `/sash` 而非 `/api`；**国际版没有该接口**（404/405/410）——
//!   那要如实报「本区域未开放」，不是静默失败。
//!
//! ## 移植口径：**刻意未移植**的机制（每条都写明理由，别当遗漏）
//!
//! 1. **出站指纹清洗**（把客户端自称 "Claude Code"/"Codex CLI" 的提示词改写成中性文本）：
//!    防御性脱敏，涉及多张正则表；未移植期间客户端自带的 system 会**原样上行**。
//! 2. **泄漏回声的「散文切分 / 截断丢弃」**（正文里混着散文 + 工具调用回声时的高级处理）：
//!    [`parse_leaked_tool_calls`] 移植了**严格整段识别**这一主路径；散文切分依赖参考实现的
//!    逐字符状态机，无真机样本可回归，宁缺勿滥。
//! 3. **历史工具结果的暴露面削减**（长历史首尾保留 + 中间省略）：缓解措施而非协议刚需。
//! 4. **每模型思考档位支持表**（把不支持的档位映射到「最近的合法档位」）：
//!    需要一手实测每款模型的档位集合；当前**只透传官方词表内的取值**（见 [`canonical_effort`]），
//!    其余丢弃（上游对不认识的档位是「忽略」而非报错，丢弃不会形成错误请求）。
//! 5. **`validate_public_http_url` 类 SSRF 守卫**：本模块的 URL 全部由区域常量拼出，
//!    没有任何用户输入进 URL 的路径。
//!
//! ## 真机探针
//!
//! 本机无 Qoder 账号 ⇒ 各端点带 `#[ignore]` 探针（设 `QODER_LIVE_ACCESS_TOKEN` /
//! `QODER_LIVE_UID` 后可跑）。**没有真机证据前，不要声称某端点已验证**。

use std::collections::HashSet;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use reqwest::Client;
use serde_json::{Map, Value};
use uuid::Uuid;

use super::cosy::{self, CosyAccountInput};
use super::region::{self, QoderRegion};

/// 工具结果在**文本化历史**里的前缀（逐字节对齐；削减/回读都锚在它上面）。
pub const TOOL_RESULT_MARKER: &str = "[工具结果";
/// 助手工具调用的文本化标记（见 [`flatten_messages`] 的文本模式）。
pub const LEAK_MARKER: &str = "[assistant 请求调用工具]";
/// 上游给 delta 填充的空噪声字段（值为空时剥掉，避免客户端读到空壳字段）。
const NOISE_KEYS: [&str; 3] = ["extra_fields", "refusal", "reasoning_content"];
/// 结构化工具有效历史的作用域白名单（只在 CN 出口、且模型 key 前缀命中时启用）。
const STRUCTURED_TOOL_HISTORY_WHITELIST: [&str; 4] = ["qwen", "glm", "gm4", "gm5"];
/// DeepSeek 族的模型 key：历史多轮一致性与 `reasoning_content` 绑定。
const DEEPSEEK_MODEL_KEYS: [&str; 2] = ["dmodel", "dfmodel"];
/// 模型清单的**签名路径**（不含 query）。
///
/// ⚠️ 签名只覆盖 `urlparse(raw_url).path` —— **query 不参与签名**。拿带
/// `?Encode=1` 的整串去签，服务端按无 query 的 path 复算 ⇒ md5 不等 ⇒ 403。
pub const PATH_MODELS_SIGN: &str = "/algo/api/v2/model/list";
/// 模型清单**请求路径**（带 `Encode=1`；GET 也**必须带签名 body**）。
pub const PATH_MODELS_ENCODED: &str = "/algo/api/v2/model/list?Encode=1";

// ---------------------------------------------------------------------------
// 请求骨架（官方 baseprompt）
// ---------------------------------------------------------------------------

/// 官方请求骨架（`baseprompt.json`）。
///
/// ⚠️ 与官方原件的**两处精简**（都有依据，别还原）：
/// - `tools` 置空：模板自带的 14 个桌面端工具（Bash/Edit/…）**永不随请求发出**
///   （每请求都被客户端工具或 `[]` 覆盖；泄漏给普通客户端是明确要避免的形态）。
/// - `messages` 只留 system：两条空 user 轮次在任何路径上都不存活。
const BASE_TEMPLATE_JSON: &str = include_str!("baseprompt.json");

/// 解析一次、进程内复用的骨架。
pub fn base_template() -> Result<&'static Value, String> {
    static TEMPLATE: OnceLock<Result<Value, String>> = OnceLock::new();
    TEMPLATE
        .get_or_init(|| {
            serde_json::from_str::<Value>(BASE_TEMPLATE_JSON)
                .map_err(|error| format!("解析 Qoder 请求骨架失败: {error}"))
        })
        .as_ref()
        .map_err(|error| error.clone())
}

/// 模型元数据（`model_config` 的取值来源；来自模型清单条目）。
#[derive(Debug, Clone, PartialEq)]
pub struct ModelMeta {
    pub key: String,
    pub display_name: String,
    pub is_vl: bool,
    pub is_reasoning: bool,
    pub max_input_tokens: i64,
}

impl ModelMeta {
    /// 从模型清单条目（[`parse_model_list`] 的行）取元数据；缺字段用官方默认值兜底。
    pub fn from_catalog_entry(entry: &Value) -> Self {
        let key = entry
            .get("key")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let display_name = entry
            .get("display_name")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or(&key)
            .to_string();
        Self {
            key,
            display_name,
            is_vl: entry.get("is_vl").and_then(Value::as_bool).unwrap_or(false),
            is_reasoning: entry
                .get("is_reasoning")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            max_input_tokens: entry
                .get("max_input_tokens")
                .and_then(Value::as_i64)
                .unwrap_or(180_000),
        }
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// 消息处理
// ---------------------------------------------------------------------------

/// OpenAI 的 `developer` 角色 → `system`（上游只认识 system/user/assistant）。
pub fn normalize_roles(messages: Option<&Vec<Value>>) -> Vec<Value> {
    let mut out = Vec::new();
    for message in messages.into_iter().flatten() {
        let Some(object) = message.as_object() else {
            continue;
        };
        let mut item = object.clone();
        if object.get("role").and_then(Value::as_str) == Some("developer") {
            item.insert("role".to_string(), Value::String("system".to_string()));
        }
        out.push(Value::Object(item));
    }
    out
}

/// 把 OpenAI content（字符串或 parts 列表）压成纯文本；图片单独收集。
pub fn flatten_content_text(content: Option<&Value>) -> (String, Vec<String>) {
    match content {
        None | Some(Value::Null) => (String::new(), Vec::new()),
        Some(Value::String(text)) => (text.clone(), Vec::new()),
        Some(Value::Array(parts)) => {
            let mut texts = Vec::new();
            let mut images = Vec::new();
            for piece in parts {
                match piece {
                    Value::String(text) => texts.push(text.clone()),
                    Value::Object(object) => {
                        let piece_type = object
                            .get("type")
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        if matches!(
                            piece_type,
                            "text" | "input_text" | "output_text" | "summary_text"
                        ) {
                            if let Some(text) = object.get("text").and_then(Value::as_str) {
                                texts.push(text.to_string());
                            }
                        } else if matches!(piece_type, "image_url" | "image" | "input_image")
                            || object.contains_key("image_url")
                        {
                            let mut url = object
                                .get("image_url")
                                .or_else(|| object.get("url"))
                                .and_then(|value| {
                                    if let Some(inner) = value.get("url") {
                                        inner.as_str()
                                    } else {
                                        value.as_str()
                                    }
                                })
                                .map(str::to_string);
                            if url.is_none() {
                                if let Some(data) = object.get("data").and_then(Value::as_str) {
                                    let mime = object
                                        .get("mimeType")
                                        .or_else(|| object.get("mime_type"))
                                        .and_then(Value::as_str)
                                        .unwrap_or("image/png");
                                    url = Some(format!("data:{mime};base64,{data}"));
                                }
                            }
                            if let Some(url) = url {
                                images.push(url);
                            }
                        }
                    }
                    _ => {}
                }
            }
            let text = texts
                .into_iter()
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            (text, images)
        }
        Some(other) => (other.to_string(), Vec::new()),
    }
}

/// 扁平化结果：`(首个 system 文本, 历史消息, 图片 URL 列表)`。
pub struct FlattenedMessages {
    pub system_text: Option<String>,
    pub messages: Vec<Value>,
    pub images: Vec<String>,
}

/// 把客户端会话压平成上游可接受的 `{role, content}` 序列。
///
/// - 首个 system/developer 抽为 `system_text`（模板 system 的替换源）；
/// - `tool` 结果：`structured=true` 保留 `{role:"tool", tool_call_id, content}`
///   （**content 用空字符串而非 null**——null 会让上游丢掉配对的 assistant 消息）；
///   否则降级为 user 文本（`[工具结果 (name)]\n` 前缀）。
/// - assistant 的 `tool_calls`：structured 原样透传（arguments 统一成 JSON 字符串）；
///   否则序列化为 `<LEAK_MARKER>\n<JSON 数组>` 塞进 content。
/// - assistant 的 `reasoning_content` 仅在 `keep_reasoning=true`（DeepSeek 族）时保留。
pub fn flatten_messages(
    messages: &[Value],
    keep_reasoning: bool,
    structured: bool,
) -> FlattenedMessages {
    let mut system_text: Option<String> = None;
    let mut flat: Vec<Value> = Vec::new();
    let mut images: Vec<String> = Vec::new();

    for message in messages {
        let Some(object) = message.as_object() else {
            continue;
        };
        let role = object.get("role").and_then(Value::as_str).unwrap_or("");
        let (text, message_images) = flatten_content_text(object.get("content"));
        images.extend(message_images);

        if matches!(role, "system" | "developer") {
            if system_text.is_none() && !text.is_empty() {
                system_text = Some(text);
            }
            continue;
        }

        if role == "tool" {
            let name = object.get("name").and_then(Value::as_str).unwrap_or("");
            let body = text;
            if structured {
                flat.push(serde_json::json!({
                    "role": "tool",
                    "tool_call_id": object.get("tool_call_id").and_then(Value::as_str).unwrap_or(""),
                    "content": body,
                }));
            } else {
                let suffix = if name.is_empty() {
                    String::new()
                } else {
                    format!(" ({name})")
                };
                flat.push(serde_json::json!({
                    "role": "user",
                    "content": format!("{TOOL_RESULT_MARKER}{suffix}]\n{body}"),
                }));
            }
            continue;
        }

        if role == "assistant" {
            let tool_calls = object.get("tool_calls").and_then(Value::as_array);
            if let Some(calls) = tool_calls.filter(|calls| !calls.is_empty()) {
                if structured {
                    let mut normalized = Vec::new();
                    for call in calls {
                        let call = call.as_object().cloned().unwrap_or_default();
                        let function = call
                            .get("function")
                            .and_then(Value::as_object)
                            .cloned()
                            .unwrap_or_default();
                        let arguments = match function.get("arguments") {
                            Some(Value::String(text)) => text.clone(),
                            Some(other @ (Value::Object(_) | Value::Array(_))) => {
                                serde_json::to_string(other).unwrap_or_default()
                            }
                            None | Some(Value::Null) => String::new(),
                            Some(other) => other.to_string(),
                        };
                        normalized.push(serde_json::json!({
                            "id": call.get("id").and_then(Value::as_str).unwrap_or(""),
                            "type": call.get("type").and_then(Value::as_str).unwrap_or("function"),
                            "function": {
                                "name": function.get("name").and_then(Value::as_str).unwrap_or(""),
                                "arguments": arguments,
                            },
                        }));
                    }
                    let mut item = Map::new();
                    item.insert("role".into(), Value::String("assistant".into()));
                    item.insert("content".into(), Value::String(text));
                    item.insert("tool_calls".into(), Value::Array(normalized));
                    if keep_reasoning {
                        if let Some(reasoning) = object.get("reasoning_content") {
                            item.insert(
                                "reasoning_content".into(),
                                Value::String(
                                    reasoning.as_str().unwrap_or("").to_string(),
                                ),
                            );
                        }
                    }
                    flat.push(Value::Object(item));
                    continue;
                }
                // 文本模式：marker + 紧凑 JSON 数组（逐字对齐）。
                let rendered: Vec<Value> = calls
                    .iter()
                    .filter_map(|call| {
                        let call = call.as_object()?;
                        let function = call.get("function").and_then(Value::as_object)?;
                        let arguments = match function.get("arguments") {
                            Some(Value::String(text)) => text.clone(),
                            Some(other @ (Value::Object(_) | Value::Array(_))) => {
                                serde_json::to_string(other).unwrap_or_default()
                            }
                            None | Some(Value::Null) => String::new(),
                            Some(other) => other.to_string(),
                        };
                        Some(serde_json::json!({
                            "name": function.get("name").and_then(Value::as_str).unwrap_or(""),
                            "arguments": arguments,
                        }))
                    })
                    .collect();
                let echo = serde_json::to_string(&rendered).unwrap_or_else(|_| "[]".to_string());
                let content = if text.is_empty() {
                    format!("{LEAK_MARKER}\n{echo}")
                } else {
                    format!("{text}\n{LEAK_MARKER}\n{echo}")
                };
                flat.push(serde_json::json!({ "role": "assistant", "content": content }));
                continue;
            }

            let mut item = Map::new();
            item.insert("role".into(), Value::String("assistant".into()));
            item.insert("content".into(), Value::String(text));
            if keep_reasoning {
                if let Some(reasoning) = object.get("reasoning_content") {
                    item.insert(
                        "reasoning_content".into(),
                        Value::String(reasoning.as_str().unwrap_or("").to_string()),
                    );
                }
            }
            flat.push(Value::Object(item));
            continue;
        }

        // user 与其它角色原样保留（content 已压平成字符串）。
        flat.push(serde_json::json!({ "role": role, "content": text }));
    }

    FlattenedMessages {
        system_text,
        messages: flat,
        images,
    }
}

/// 出站工具统一**嵌套** `tools[].function` 对象（上游对平铺形态报 400）。
///
/// - 已嵌套（`{type, function:{…}}`）⇒ 规整内部字段；
/// - 平铺 `{type:"function", name, …}` ⇒ 包成嵌套；
/// - 其余形态（`custom` 等上游不认的）⇒ **丢弃**（不发畸形请求）。
pub fn normalize_tools(tools: &[Value]) -> Vec<Value> {
    let mut out = Vec::new();
    for tool in tools {
        let Some(object) = tool.as_object() else {
            continue;
        };
        let mut function = Map::new();
        if let Some(inner) = object.get("function").and_then(Value::as_object) {
            let name = inner.get("name").and_then(Value::as_str).unwrap_or("");
            if name.is_empty() {
                continue;
            }
            function.insert("name".into(), Value::String(name.to_string()));
            if let Some(description) = inner.get("description").and_then(Value::as_str) {
                function.insert("description".into(), Value::String(description.to_string()));
            }
            function.insert(
                "parameters".into(),
                inner
                    .get("parameters")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}})),
            );
        } else {
            let tool_type = object.get("type").and_then(Value::as_str).unwrap_or("function");
            let name = object.get("name").and_then(Value::as_str).unwrap_or("");
            if tool_type != "function" || name.is_empty() {
                continue;
            }
            function.insert("name".into(), Value::String(name.to_string()));
            if let Some(description) = object.get("description").and_then(Value::as_str) {
                function.insert("description".into(), Value::String(description.to_string()));
            }
            function.insert(
                "parameters".into(),
                object
                    .get("parameters")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}})),
            );
        }
        out.push(serde_json::json!({ "type": "function", "function": function }));
    }
    out
}

// ---------------------------------------------------------------------------
// 思考档位
// ---------------------------------------------------------------------------

/// 请求侧的**原始**思考档位（提取顺序：`reasoning_effort` → `reasoning.effort` →
/// `thinking.effort`/`thinking.level`；不做任何归一化）。
pub fn extract_requested_effort(payload: &Value) -> Option<String> {
    let object = payload.as_object()?;
    if let Some(value) = object.get("reasoning_effort").and_then(Value::as_str) {
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    if let Some(value) = object
        .get("reasoning")
        .and_then(|reasoning| reasoning.get("effort"))
        .and_then(Value::as_str)
    {
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    for key in ["effort", "level"] {
        if let Some(value) = object
            .get("thinking")
            .and_then(|thinking| thinking.get(key))
            .and_then(Value::as_str)
        {
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// 官方档位词表内的取值**原样透传**；关闭类取值统一成 `"none"`；其余**丢弃**。
///
/// （每模型的合法档位集合是实测数据，未移植——见模块头「刻意未移植」第 4 条。）
pub fn canonical_effort(effort: &str) -> Option<&'static str> {
    match effort.trim().to_ascii_lowercase().as_str() {
        "none" | "off" | "disabled" | "disable" | "false" | "0" | "no" => Some("none"),
        "minimal" => Some("minimal"),
        "low" => Some("low"),
        "medium" => Some("medium"),
        "high" => Some("high"),
        "xhigh" => Some("xhigh"),
        "max" => Some("max"),
        _ => None,
    }
}

/// 每条 `tool` 有 `tool_call_id`、每个 `assistant.tool_calls` 条目有 `id` —— 结构化的前提。
fn tool_ids_ok(messages: &[Value]) -> bool {
    for message in messages {
        let Some(object) = message.as_object() else {
            continue;
        };
        match object.get("role").and_then(Value::as_str) {
            Some("tool") => {
                let id = object
                    .get("tool_call_id")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if id.trim().is_empty() {
                    return false;
                }
            }
            Some("assistant") => {
                if let Some(calls) = object.get("tool_calls").and_then(Value::as_array) {
                    for call in calls {
                        let id = call.get("id").and_then(Value::as_str).unwrap_or("");
                        if id.trim().is_empty() {
                            return false;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    true
}

/// 本次请求是否走结构化工具历史。
///
/// 与官方客户端的调用姿势对齐：**CN 出口 + 白名单模型前缀 + id 齐备**才启用；
/// 环境变量 `QD_STRUCTURED_TOOL_HISTORY=off/on` 可显式覆盖（`auto` 为默认）。
/// 任何情况下 id 不齐备都回退文本化（fail-safe：宁可少用，不发畸形请求）。
pub fn structured_tool_history_enabled(
    model_key: &str,
    region: QoderRegion,
    messages: &[Value],
) -> bool {
    if !tool_ids_ok(messages) {
        return false;
    }
    let mode = std::env::var("QD_STRUCTURED_TOOL_HISTORY")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    match mode.as_str() {
        "off" => return false,
        "1" | "on" | "true" | "yes" | "enable" | "enabled" | "force" => return true,
        _ => {}
    }
    if region != QoderRegion::Cn {
        return false;
    }
    let key = model_key.trim().to_ascii_lowercase();
    !key.is_empty()
        && STRUCTURED_TOOL_HISTORY_WHITELIST
            .iter()
            .any(|prefix| key.starts_with(prefix))
}

// ---------------------------------------------------------------------------
// 请求体构造（骨架 + 覆写）
// ---------------------------------------------------------------------------

/// 由客户端请求构造 `agent_chat_generation` 上游请求体（随后过 [`cosy::qoder_encode`]）。
///
/// `region` 参与「结构化工具历史」的 auto 判据（只在 CN 出口启用的白名单规则）。
pub fn build_chat_body(
    payload: &Value,
    meta: &ModelMeta,
    region: QoderRegion,
    user_type: &str,
) -> Result<Value, String> {
    let template = base_template()?;
    let mut body = template.clone();

    let messages = normalize_roles(payload.get("messages").and_then(Value::as_array));
    let keep_reasoning = DEEPSEEK_MODEL_KEYS.contains(&meta.key.as_str());
    let structured = structured_tool_history_enabled(&meta.key, region, &messages);
    let flattened = flatten_messages(&messages, keep_reasoning, structured);

    // system：客户端给了就用客户端的；没给就保留模板自带的官方系统提示词。
    if let Some(system_text) = &flattened.system_text {
        body["messages"] = serde_json::json!([{ "role": "system", "content": system_text }]);
    }
    if let Some(array) = body.get_mut("messages").and_then(Value::as_array_mut) {
        array.extend(flattened.messages.iter().cloned());
    }

    // 最新用户提示词（chat_context 高亮 + business 会话名）。
    let mut prompt = String::new();
    for message in flattened.messages.iter().rev() {
        if message.get("role").and_then(Value::as_str) == Some("user") {
            if let Some(content) = message.get("content").and_then(Value::as_str) {
                if !content.is_empty() {
                    prompt = content.to_string();
                    break;
                }
            }
        }
    }

    let request_id = Uuid::new_v4().to_string();
    body["request_id"] = Value::String(request_id.clone());
    body["chat_record_id"] = Value::String(request_id);
    body["request_set_id"] = Value::String(Uuid::new_v4().to_string());
    body["session_id"] = Value::String(Uuid::new_v4().to_string());
    body["stream"] = Value::Bool(true);
    body["agent_id"] = Value::String("agent_common".to_string());
    body["aliyun_user_type"] = Value::String(if user_type.is_empty() {
        cosy::DEFAULT_USER_TYPE.to_string()
    } else {
        user_type.to_string()
    });

    // model_config：以清单元数据为准，动态 key。
    let model_config = serde_json::json!({
        "key": meta.key,
        "display_name": meta.display_name,
        "model": "",
        "format": "openai",
        "is_vl": meta.is_vl,
        "is_reasoning": meta.is_reasoning,
        "api_key": "",
        "url": "",
        "source": "system",
        "max_input_tokens": meta.max_input_tokens,
    });
    body["model_config"] = model_config.clone();

    // chat_context：高亮文本 + 模型配置副本 + 图片。
    {
        let context = body
            .get_mut("chat_context")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| "请求骨架缺 chat_context".to_string())?;
        if let Some(text) = context.get_mut("text").and_then(Value::as_object_mut) {
            text.insert("text".into(), Value::String(prompt.clone()));
        }
        if let Some(extra) = context.get_mut("extra").and_then(Value::as_object_mut) {
            if let Some(original) = extra
                .get_mut("originalContent")
                .and_then(Value::as_object_mut)
            {
                original.insert("text".into(), Value::String(prompt.clone()));
            }
            extra.insert("modelConfig".into(), model_config);
        }
        if !flattened.images.is_empty() {
            context.insert(
                "imageUrls".into(),
                serde_json::json!(flattened.images.clone()),
            );
            body["image_urls"] = serde_json::json!(flattened.images.clone());
        }
    }

    // parameters：max_tokens（客户端没给就保留骨架默认）+ 思考档位。
    {
        let parameters = body
            .get_mut("parameters")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| "请求骨架缺 parameters".to_string())?;
        let max_tokens = payload
            .get("max_tokens")
            .or_else(|| payload.get("max_completion_tokens"))
            .and_then(Value::as_i64);
        if let Some(max_tokens) = max_tokens {
            parameters.insert("max_tokens".into(), Value::from(max_tokens));
        }
        if let Some(effort) = extract_requested_effort(payload) {
            if let Some(canonical) = canonical_effort(&effort) {
                parameters.insert("reasoning_effort".into(), Value::String(canonical.to_string()));
            }
        }
    }

    // tools：客户端给了用客户端的（统一嵌套），否则置空（泄漏桌面工具是明确要避免的形态）。
    let tools = payload
        .get("tools")
        .and_then(Value::as_array)
        .map(|tools| normalize_tools(tools))
        .unwrap_or_default();
    body["tools"] = Value::Array(tools);

    // business 会话卡片。
    {
        let business = body
            .get_mut("business")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| "请求骨架缺 business".to_string())?;
        business.insert("id".into(), Value::String(Uuid::new_v4().to_string()));
        business.insert("begin_at".into(), Value::from(now_millis()));
        let name = if prompt.is_empty() {
            "chat".to_string()
        } else {
            prompt.chars().take(30).collect()
        };
        business.insert("name".into(), Value::String(name));
        business
            .entry("product".to_string())
            .or_insert_with(|| Value::String("cli".to_string()));
        business
            .entry("type".to_string())
            .or_insert_with(|| Value::String("agent".to_string()));
        business
            .entry("stage".to_string())
            .or_insert_with(|| Value::String("start".to_string()));
    }

    Ok(body)
}

// ---------------------------------------------------------------------------
// 信封 SSE 解包
// ---------------------------------------------------------------------------

/// 去掉 `data:` 前缀（多前缀也处理；`:` 注释行与空行返回空串）。
pub fn strip_data_prefix(line: &str) -> String {
    let mut text = line.trim();
    if text.is_empty() || text.starts_with(':') {
        return String::new();
    }
    while let Some(rest) = text.strip_prefix("data:") {
        text = rest.trim();
    }
    if text.is_empty() || text.starts_with(':') {
        return String::new();
    }
    text.to_string()
}

/// 剥掉上游给 delta 填充的空噪声字段；整个 delta 空且无 `finish_reason` ⇒ 空串（跳过）。
pub fn clean_chunk(raw: &str) -> String {
    let Ok(mut object) = serde_json::from_str::<Value>(raw) else {
        return raw.to_string();
    };
    let Some(choices) = object.get_mut("choices").and_then(Value::as_array_mut) else {
        return raw.to_string();
    };
    let mut changed = false;
    let mut empty_choices = 0usize;
    let total = choices.len();
    for choice in choices.iter_mut() {
        let Some(choice_object) = choice.as_object_mut() else {
            continue;
        };
        // `finish_reason` 判断先做完（在可变借 `delta` 之前），避免借用冲突。
        let finish = choice_object
            .get("finish_reason")
            .map(|value| !value.is_null())
            .unwrap_or(false);
        let mut delta_is_empty = false;
        if let Some(delta) = choice_object
            .get_mut("delta")
            .and_then(Value::as_object_mut)
        {
            if let Some(function_call) = delta.get("function_call") {
                let empty = match function_call.as_object() {
                    Some(object) => !object.contains_key("name"),
                    None => function_call.is_null(),
                };
                if empty {
                    delta.remove("function_call");
                    changed = true;
                }
            }
            if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
                if tool_calls.is_empty() {
                    delta.remove("tool_calls");
                    changed = true;
                }
            }
            for key in NOISE_KEYS {
                if let Some(value) = delta.get(key) {
                    if !value_truthy(value) {
                        delta.remove(key);
                        changed = true;
                    }
                }
            }
            delta_is_empty = delta.is_empty();
        }
        if delta_is_empty && !finish {
            empty_choices += 1;
        }
    }
    if total > 0 && empty_choices == total {
        return String::new();
    }
    if changed {
        serde_json::to_string(&object).unwrap_or_else(|_| raw.to_string())
    } else {
        raw.to_string()
    }
}

fn value_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().map(|n| n != 0.0).unwrap_or(false),
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
    }
}

/// 一行上游 SSE 的解包结果。
#[derive(Debug, Clone, PartialEq)]
pub enum ChatFrame {
    /// 注释 / 空行 / 无内容帧 —— 跳过。
    Skip,
    /// 流结束（`body == "[DONE]"`）。
    Done,
    /// 一个内层 OpenAI chunk（已清洗；JSON 字符串）。
    Chunk(String),
    /// 上游报错帧（`statusCodeValue != 200`），`detail` 为错误详情。
    UpstreamError { status: u16, detail: String },
}

/// 解包一行信封 SSE（见模块头的帧形态）。
pub fn unwrap_envelope_line(line: &str) -> ChatFrame {
    let data = strip_data_prefix(line);
    if data.is_empty() {
        return ChatFrame::Skip;
    }
    let Ok(outer) = serde_json::from_str::<Value>(&data) else {
        return ChatFrame::Skip;
    };
    let Some(outer) = outer.as_object() else {
        return ChatFrame::Skip;
    };
    let status = outer
        .get("statusCodeValue")
        .and_then(Value::as_i64)
        .or_else(|| {
            outer
                .get("statusCodeValue")
                .and_then(Value::as_str)
                .and_then(|text| text.parse::<i64>().ok())
        });
    if let Some(status) = status {
        if status != 200 {
            let detail = match outer.get("body") {
                Some(Value::String(text)) => text.chars().take(400).collect(),
                Some(other) => other.to_string().chars().take(400).collect(),
                None => String::new(),
            };
            return ChatFrame::UpstreamError {
                status: status as u16,
                detail,
            };
        }
    }
    let Some(Value::String(body)) = outer.get("body") else {
        return ChatFrame::Skip;
    };
    if body == "[DONE]" {
        return ChatFrame::Done;
    }
    let cleaned = clean_chunk(body);
    if cleaned.is_empty() {
        return ChatFrame::Skip;
    }
    ChatFrame::Chunk(cleaned)
}

// ---------------------------------------------------------------------------
// 泄漏回声还原（严格整段识别）
// ---------------------------------------------------------------------------

/// 把模型复述的「marker + JSON 数组」还原成结构化 `tool_calls`。
///
/// **严格守卫**（避免把「讨论该标记的普通回复」误判为工具调用）：
/// 整段 trim 后必须恰好是 marker + JSON 数组（可带 ``` 围栏）；数组非空；
/// 每项 name 非空、arguments 为合法 JSON；声明了工具时全部命中声明集合。
///
/// 返回 `(tool_calls, clean_text)`；识别成功时 `clean_text` 为空串。
pub fn parse_leaked_tool_calls(
    text: &str,
    allowed_names: Option<&HashSet<String>>,
) -> (Option<Vec<Value>>, String) {
    let mut t = text.trim().to_string();
    if t.starts_with("```") && t.ends_with("```") && t.len() > 6 {
        let mut inner = t[3..t.len() - 3].to_string();
        if inner.len() >= 4 && inner[..4].eq_ignore_ascii_case("json") {
            inner = inner[4..].to_string();
        }
        t = inner.trim().to_string();
    }
    let Some(rest) = t.strip_prefix(LEAK_MARKER) else {
        return (None, text.to_string());
    };
    let rest = rest.trim();
    if !(rest.starts_with('[') && rest.ends_with(']')) {
        return (None, text.to_string());
    }
    let Ok(array) = serde_json::from_str::<Value>(rest) else {
        return (None, text.to_string());
    };
    let Some(items) = array.as_array().filter(|items| !items.is_empty()) else {
        return (None, text.to_string());
    };
    let mut calls = Vec::new();
    for item in items {
        let Some(object) = item.as_object() else {
            return (None, text.to_string());
        };
        let Some(name) = object.get("name").and_then(Value::as_str) else {
            return (None, text.to_string());
        };
        if name.trim().is_empty() {
            return (None, text.to_string());
        }
        let arguments = match object.get("arguments") {
            Some(Value::String(text)) => text.clone(),
            Some(other @ (Value::Object(_) | Value::Array(_))) => {
                serde_json::to_string(other).unwrap_or_default()
            }
            _ => return (None, text.to_string()),
        };
        if !arguments.trim().is_empty() && serde_json::from_str::<Value>(&arguments).is_err() {
            return (None, text.to_string());
        }
        calls.push(serde_json::json!({
            "id": format!("call_{}", Uuid::new_v4().simple()),
            "type": "function",
            "function": { "name": name.trim(), "arguments": arguments },
        }));
    }
    if let Some(allowed) = allowed_names {
        let all_allowed = calls.iter().all(|call| {
            call.get("function")
                .and_then(|function| function.get("name"))
                .and_then(Value::as_str)
                .map(|name| allowed.contains(name))
                .unwrap_or(false)
        });
        if !all_allowed {
            return (None, text.to_string());
        }
    }
    (Some(calls), String::new())
}

// ---------------------------------------------------------------------------
// 模型清单
// ---------------------------------------------------------------------------

/// 解析模型清单响应：`{"chat":[{key, display_name, …}]}` →
/// **官方条目全字段原样透传**（不裁剪 enable/strategies/…），仅补 `id`/`name` 兼容键。
pub fn parse_model_list(payload: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    let Some(chat) = payload.get("chat").and_then(Value::as_array) else {
        return out;
    };
    for entry in chat {
        let Some(object) = entry.as_object() else {
            continue;
        };
        let Some(key) = object.get("key").and_then(Value::as_str) else {
            continue;
        };
        if key.is_empty() {
            continue;
        }
        let mut row = object.clone();
        let display = object
            .get("display_name")
            .and_then(Value::as_str)
            .unwrap_or(key)
            .to_string();
        row.insert("id".into(), Value::String(key.to_string()));
        row.entry("name".to_string())
            .or_insert_with(|| Value::String(display.clone()));
        row.entry("display_name".to_string())
            .or_insert_with(|| Value::String(display));
        out.push(Value::Object(row));
    }
    out
}

// ---------------------------------------------------------------------------
// openapi 通道：刷新 / 签到 / 额度 / 套餐
// ---------------------------------------------------------------------------

/// 刷新族（按**令牌前缀**路由，见 `token.rs`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshKind {
    /// `drt-` → `/api/v1/deviceToken/refresh`
    Device,
    /// `jrt-` → `/api/v1/jobToken/refresh`
    Job,
}

/// 刷新结果。
#[derive(Debug, Clone, PartialEq)]
pub struct TokenRefresh {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// 过期时刻（unix 秒）。`expires_in` 单位是**毫秒**；缺省按 30 天。
    pub expires_at: i64,
}

/// 解析刷新响应（`kind` 决定 token 字段名）。
pub fn parse_refresh_response(kind: RefreshKind, data: &Value, previous_refresh: &str) -> TokenRefresh {
    let access_token = match kind {
        RefreshKind::Device => data
            .get("token")
            .or_else(|| data.get("device_token"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        RefreshKind::Job => data
            .get("token")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    };
    let refresh_token = data
        .get("refresh_token")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            if previous_refresh.is_empty() {
                None
            } else {
                Some(previous_refresh.to_string())
            }
        });
    TokenRefresh {
        access_token,
        refresh_token,
        expires_at: token_expiry(data),
    }
}

/// `expires_in`（毫秒）/ `expires_at`（RFC3339）→ unix 秒；都没有按 30 天。
pub fn token_expiry(data: &Value) -> i64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    if let Some(expires_in) = data.get("expires_in").and_then(Value::as_i64) {
        return now + expires_in / 1000;
    }
    if let Some(text) = data.get("expires_at").and_then(Value::as_str) {
        if let Ok(datetime) = chrono::NaiveDateTime::parse_from_str(
            text.get(..19).unwrap_or(text),
            "%Y-%m-%dT%H:%M:%S",
        ) {
            return datetime.and_utc().timestamp();
        }
    }
    now + 30 * 86_400
}

/// 签到状态。
#[derive(Debug, Clone, PartialEq)]
pub struct CheckinStatus {
    /// 上游原始状态词（`CLAIMABLE` / `CLAIMED` / `DISABLED` …）。
    pub status: String,
    /// 活动是否活跃（CLAIMABLE / CLAIMED）。
    pub active: bool,
    pub today_checked_in: bool,
    pub streak_days: i64,
    pub total_claim_days: i64,
    pub reward_credits: i64,
    pub total_reward_credits: i64,
    pub next_claim_at: i64,
    pub last_claimed_at: i64,
    pub reward_expires_at: i64,
}

/// 解析签到状态响应；`today` 是本地日期 `YYYY-MM-DD`（由调用方注入，纯函数可测）。
pub fn parse_checkin_status(payload: &Value, today: &str) -> CheckinStatus {
    let status = payload
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let last_claimed_at = payload
        .get("lastClaimedAt")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let last_day = if last_claimed_at > 0 {
        chrono::DateTime::from_timestamp(last_claimed_at, 0)
            .map(|datetime| datetime.with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).unwrap()))
            .map(|datetime| datetime.format("%Y-%m-%d").to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };
    let num = |key: &str| payload.get(key).and_then(Value::as_i64).unwrap_or(0);
    CheckinStatus {
        active: status == "CLAIMABLE" || status == "CLAIMED",
        today_checked_in: status == "CLAIMED" && last_day == today,
        streak_days: num("currentStreakDays"),
        total_claim_days: num("totalClaimDays"),
        reward_credits: num("rewardCredits"),
        total_reward_credits: num("totalRewardCredits"),
        next_claim_at: num("nextClaimAt"),
        last_claimed_at,
        reward_expires_at: num("rewardExpiresAt"),
        status,
    }
}

/// 额度视图（userQuota + addOnQuota 的聚合）。
#[derive(Debug, Clone, PartialEq)]
pub struct QuotaView {
    pub remaining: i64,
    pub used: i64,
    pub total: i64,
    pub is_exceeded: bool,
    /// 原始响应透传（页面上「资源包明细」等展示字段的来源）。
    pub raw: Value,
}

/// 解析额度响应。
///
/// **退化守卫**（与官方客户端的判断一致，保守放行）：两个 quota 键都缺失，
/// 或 `isQuotaExceeded=true` 且 remain 与 size 合计全 0（「从没有过额度」）⇒ `Err`
/// —— **拒绝把解析不了的响应写成 0 余额**（0 是谎报，报错是诚实）。
pub fn parse_quota(payload: &Value) -> Result<QuotaView, String> {
    let user_quota = payload.get("userQuota").and_then(Value::as_object);
    let add_on_quota = payload.get("addOnQuota").and_then(Value::as_object);
    if user_quota.is_none() && add_on_quota.is_none() {
        return Err("额度响应缺少 userQuota / addOnQuota（解析不了，拒绝写 0）".to_string());
    }
    let num = |source: Option<&Map<String, Value>>, key: &str| {
        source
            .and_then(|object| object.get(key))
            .and_then(Value::as_i64)
            .unwrap_or(0)
    };
    let remaining = num(user_quota, "remaining") + num(add_on_quota, "remaining");
    let used = num(user_quota, "used") + num(add_on_quota, "used");
    let total = num(user_quota, "total") + num(add_on_quota, "total");
    let is_exceeded = payload
        .get("isQuotaExceeded")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if is_exceeded && remaining == 0 && total == 0 {
        return Err("额度响应为退化形态（exceeded 且全 0）—— 拒绝写入".to_string());
    }
    Ok(QuotaView {
        remaining,
        used,
        total,
        is_exceeded,
        raw: payload.clone(),
    })
}

/// openapi 通道的请求头集（逐字对齐官方客户端）。
pub fn openapi_headers(region: QoderRegion, uid: &str, access_token: &str) -> Vec<(String, String)> {
    let spec = region::realm_spec(region);
    let request_id = format!(
        "{}-{:06}",
        super::device::derive_id(uid, "req"),
        now_millis() % 1_000_000
    );
    vec![
        ("Content-Type".into(), "application/json".into()),
        (
            "Accept".into(),
            "application/json, text/plain, */*".into(),
        ),
        ("User-Agent".into(), spec.user_agent.to_string()),
        ("Authorization".into(), format!("Bearer {access_token}")),
        ("X-Request-ID".into(), request_id),
        ("X-Machine-ID".into(), super::device::derive_id(uid, "machine")),
        ("X-Session-ID".into(), super::device::derive_id(uid, "session")),
        ("Origin".into(), spec.website.to_string()),
        ("Referer".into(), format!("{}/", spec.website)),
    ]
}

// ---------------------------------------------------------------------------
// HTTP 执行层
// ---------------------------------------------------------------------------

/// 进程内复用的 HTTP 客户端（连接池）。
pub fn shared_client() -> Result<&'static Client, String> {
    static CLIENT: OnceLock<Result<Client, String>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            Client::builder()
                .connect_timeout(std::time::Duration::from_secs(15))
                .build()
                .map_err(|error| format!("构造 HTTP 客户端失败: {error}"))
        })
        .as_ref()
        .map_err(|error| error.clone())
}

fn apply_headers(
    mut request: reqwest::RequestBuilder,
    headers: &[(String, String)],
) -> reqwest::RequestBuilder {
    for (name, value) in headers {
        request = request.header(name, value);
    }
    request
}

/// 刷新令牌：按 `kind` 路由到 device / job 刷新端点（普通 JSON POST，不签名）。
pub async fn refresh_token(
    region: QoderRegion,
    kind: RefreshKind,
    refresh_token: &str,
) -> Result<TokenRefresh, String> {
    let spec = region::realm_spec(region);
    let path = match kind {
        RefreshKind::Device => super::PATH_DEVICE_TOKEN_REFRESH,
        RefreshKind::Job => super::PATH_JOB_TOKEN_REFRESH,
    };
    let client = shared_client()?;
    let response = apply_headers(
        client
            .post(format!("{}{path}", spec.openapi_base))
            .json(&serde_json::json!({ "refresh_token": refresh_token })),
        &[
            ("Content-Type".into(), "application/json".into()),
            ("Accept".into(), "application/json".into()),
            ("User-Agent".into(), spec.user_agent.to_string()),
        ],
    )
    .timeout(std::time::Duration::from_secs(30))
    .send()
    .await
    .map_err(|error| format!("刷新令牌请求失败: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("读取刷新响应失败: {error}"))?;
    if !status.is_success() {
        let detail = body.chars().take(160).collect::<String>();
        return Err(format!("刷新令牌失败: HTTP {} {detail}", status.as_u16()));
    }
    let data: Value = serde_json::from_str(&body)
        .map_err(|error| format!("刷新响应不是 JSON: {error}"))?;
    Ok(parse_refresh_response(kind, &data, refresh_token))
}

/// PAT → jobToken 交换（`pt-` 的刷新路径：PAT 换 jobToken）。
pub async fn exchange_pat(region: QoderRegion, personal_token: &str) -> Result<TokenRefresh, String> {
    let spec = region::realm_spec(region);
    let client = shared_client()?;
    let response = apply_headers(
        client
            .post(format!("{}{}", spec.openapi_base, super::PATH_JOB_TOKEN_EXCHANGE))
            .json(&serde_json::json!({ "personal_token": personal_token })),
        &[
            ("Content-Type".into(), "application/json".into()),
            ("Accept".into(), "application/json".into()),
            ("User-Agent".into(), spec.user_agent.to_string()),
        ],
    )
    .timeout(std::time::Duration::from_secs(30))
    .send()
    .await
    .map_err(|error| format!("PAT 交换请求失败: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("读取交换响应失败: {error}"))?;
    if !status.is_success() {
        let detail = body.chars().take(160).collect::<String>();
        return Err(format!("PAT 交换失败: HTTP {} {detail}", status.as_u16()));
    }
    let data: Value = serde_json::from_str(&body)
        .map_err(|error| format!("交换响应不是 JSON: {error}"))?;
    Ok(parse_refresh_response(RefreshKind::Job, &data, ""))
}

/// 签到通道的可用性判定：404 / 405 / 410 ⇒ 本区域未开放该接口。
pub const CHECKIN_REASON_NOT_FOUND: &str = "checkin_endpoint_not_found";

/// 签到状态查询。返回 `Err(CHECKIN_REASON_NOT_FOUND...)` 表示该区域没有此接口。
pub async fn checkin_status(
    region: QoderRegion,
    uid: &str,
    access_token: &str,
) -> Result<CheckinStatus, String> {
    let spec = region::realm_spec(region);
    let client = shared_client()?;
    let response = apply_headers(
        client
            .get(format!("{}{}", spec.openapi_base, super::PATH_CHECKIN_STATUS))
            .timeout(std::time::Duration::from_secs(15)),
        &openapi_headers(region, uid, access_token),
    )
    .send()
    .await
    .map_err(|error| format!("签到状态请求失败: {error}"))?;
    let status_code = response.status();
    let body = response.text().await.unwrap_or_default();
    if matches!(status_code.as_u16(), 404 | 405 | 410) {
        return Err(format!("{CHECKIN_REASON_NOT_FOUND} (HTTP {})", status_code.as_u16()));
    }
    if !status_code.is_success() {
        return Err(format!(
            "签到状态失败: HTTP {} {}",
            status_code.as_u16(),
            body.chars().take(160).collect::<String>()
        ));
    }
    let data: Value = serde_json::from_str(&body)
        .map_err(|error| format!("签到状态响应不是 JSON: {error}"))?;
    let today = chrono::Utc::now()
        .with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).unwrap())
        .format("%Y-%m-%d")
        .to_string();
    Ok(parse_checkin_status(&data, &today))
}

/// 签到领取（`POST` 空对象 body；调用前应先查状态确认 `CLAIMABLE`）。
pub async fn checkin_claim(
    region: QoderRegion,
    uid: &str,
    access_token: &str,
) -> Result<Value, String> {
    let spec = region::realm_spec(region);
    let client = shared_client()?;
    let response = apply_headers(
        client
            .post(format!("{}{}", spec.openapi_base, super::PATH_CHECKIN_CLAIM))
            .json(&serde_json::json!({}))
            .timeout(std::time::Duration::from_secs(15)),
        &openapi_headers(region, uid, access_token),
    )
    .send()
    .await
    .map_err(|error| format!("签到领取请求失败: {error}"))?;
    let status_code = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status_code.is_success() {
        return Err(format!(
            "签到领取失败: HTTP {} {}",
            status_code.as_u16(),
            body.chars().take(160).collect::<String>()
        ));
    }
    serde_json::from_str(&body).map_err(|error| format!("签到响应不是 JSON: {error}"))
}

/// 额度查询（聚合 userQuota + addOnQuota；退化响应返回 `Err`，**不写 0**）。
pub async fn quota_usage(
    region: QoderRegion,
    uid: &str,
    access_token: &str,
) -> Result<QuotaView, String> {
    let spec = region::realm_spec(region);
    let client = shared_client()?;
    let response = apply_headers(
        client
            .get(format!("{}{}", spec.openapi_base, super::PATH_QUOTA_USAGE))
            .timeout(std::time::Duration::from_secs(30)),
        &openapi_headers(region, uid, access_token),
    )
    .send()
    .await
    .map_err(|error| format!("额度查询失败: {error}"))?;
    let status_code = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status_code.is_success() {
        return Err(format!(
            "额度查询失败: HTTP {} {}",
            status_code.as_u16(),
            body.chars().take(160).collect::<String>()
        ));
    }
    let data: Value = serde_json::from_str(&body)
        .map_err(|error| format!("额度响应不是 JSON: {error}"))?;
    parse_quota(&data)
}

/// 套餐名（如 `Pro Trial`）。
pub async fn user_plan(
    region: QoderRegion,
    uid: &str,
    access_token: &str,
) -> Result<Value, String> {
    let spec = region::realm_spec(region);
    let client = shared_client()?;
    let response = apply_headers(
        client
            .get(format!("{}{}", spec.openapi_base, super::PATH_USER_PLAN))
            .timeout(std::time::Duration::from_secs(15)),
        &openapi_headers(region, uid, access_token),
    )
    .send()
    .await
    .map_err(|error| format!("套餐查询失败: {error}"))?;
    let status_code = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status_code.is_success() {
        return Err(format!(
            "套餐查询失败: HTTP {} {}",
            status_code.as_u16(),
            body.chars().take(160).collect::<String>()
        ));
    }
    serde_json::from_str(&body).map_err(|error| format!("套餐响应不是 JSON: {error}"))
}

/// 拉取模型清单（COSY 签名；**GET 也带 body** `qoder_encode("{}")`，裸 GET 会 403）。
///
/// 主机按官方的候选顺序尝试（国际版 api1 → api2 → api3；签名只覆盖 path，换主机不影响签名）。
pub async fn fetch_model_list(
    region: QoderRegion,
    input: &CosyAccountInput<'_>,
) -> Result<Vec<Value>, String> {
    fetch_model_list_at(&region::gateway_candidates(region), input).await
}

/// 同 [`fetch_model_list`]，但由调用方指定候选主机（**测试挂载点**：指向 mock 上游；
/// 生产代码不要绕过 [`fetch_model_list`]）。
pub async fn fetch_model_list_at(
    hosts: &[&str],
    input: &CosyAccountInput<'_>,
) -> Result<Vec<Value>, String> {
    let sign_body = cosy::qoder_encode(b"{}");
    let session = cosy::shared_cache().get(input)?;
    let client = shared_client()?;
    let mut last_error = String::new();
    for host in hosts {
        let raw_url = format!("{host}{PATH_MODELS_ENCODED}");
        // ★ 签名用**无 query** 的 path（见 [`PATH_MODELS_SIGN`]）。
        let headers = session.headers(&sign_body, PATH_MODELS_SIGN, None, false);
        let request = apply_headers(
            client
                .get(&raw_url)
                .body(sign_body.clone())
                .timeout(std::time::Duration::from_secs(15)),
            &headers,
        );
        match request.send().await {
            Ok(response) => {
                let status_code = response.status();
                let body = response.text().await.unwrap_or_default();
                if !status_code.is_success() {
                    last_error = format!(
                        "HTTP {} {}",
                        status_code.as_u16(),
                        body.chars().take(160).collect::<String>()
                    );
                    continue;
                }
                let data: Value = serde_json::from_str(&body)
                    .map_err(|error| format!("模型清单不是 JSON: {error}"))?;
                return Ok(parse_model_list(&data));
            }
            Err(error) => {
                last_error = error.to_string();
            }
        }
    }
    Err(format!("模型清单拉取失败（已试全部候选主机）: {last_error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> ModelMeta {
        ModelMeta {
            key: "ultimate".into(),
            display_name: "Ultimate".into(),
            is_vl: false,
            is_reasoning: true,
            max_input_tokens: 200_000,
        }
    }

    #[test]
    fn base_template_parses_and_is_trimmed() {
        let template = base_template().expect("骨架必须可解析");
        // tools 置空（桌面工具永不随请求发出）。
        assert_eq!(template["tools"], serde_json::json!([]));
        // 只保留 system 一条。
        let messages = template["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "system");
        assert!(messages[0]["content"].as_str().unwrap().len() > 1000, "官方系统提示词应在");
        // 关键字段在位。
        assert_eq!(template["agent_id"], "agent_common");
        assert_eq!(template["chat_task"], "FREE_INPUT");
        assert_eq!(template["session_type"], "qodercli");
    }

    #[test]
    fn normalize_roles_maps_developer_to_system() {
        let messages = vec![
            serde_json::json!({"role": "developer", "content": "d"}),
            serde_json::json!({"role": "user", "content": "u"}),
        ];
        let out = normalize_roles(Some(&messages));
        assert_eq!(out[0]["role"], "system");
        assert_eq!(out[1]["role"], "user");
    }

    #[test]
    fn flatten_content_handles_text_and_images() {
        let (text, images) = flatten_content_text(Some(&serde_json::json!([
            {"type": "text", "text": "看这张"},
            {"type": "image_url", "image_url": {"url": "https://x/1.png"}},
            "plain",
            {"type": "input_image", "data": "AAAA", "mimeType": "image/jpeg"},
        ])));
        assert_eq!(text, "看这张\nplain");
        assert_eq!(images.len(), 2);
        assert_eq!(images[0], "https://x/1.png");
        assert_eq!(images[1], "data:image/jpeg;base64,AAAA");
    }

    #[test]
    fn flatten_tool_results_text_and_structured_modes() {
        let messages = vec![serde_json::json!({
            "role": "tool", "tool_call_id": "call_1", "name": "get_weather",
            "content": "晴 25 度",
        })];
        let text_mode = flatten_messages(&messages, false, false);
        assert_eq!(
            text_mode.messages[0]["content"],
            "[工具结果 (get_weather)]\n晴 25 度"
        );
        assert_eq!(text_mode.messages[0]["role"], "user");

        let structured = flatten_messages(&messages, false, true);
        assert_eq!(structured.messages[0]["role"], "tool");
        assert_eq!(structured.messages[0]["tool_call_id"], "call_1");
        // ★ content 是空字符串而非 null（null 会让上游丢掉配对消息）。
        assert_eq!(structured.messages[0]["content"], "晴 25 度");
    }

    #[test]
    fn flatten_assistant_tool_calls_both_modes() {
        let messages = vec![serde_json::json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": "call_9", "type": "function",
                "function": {"name": "search", "arguments": {"q": "北京"}},
            }],
        })];
        let text_mode = flatten_messages(&messages, false, false);
        let content = text_mode.messages[0]["content"].as_str().unwrap();
        assert!(content.starts_with(LEAK_MARKER), "{content}");
        assert!(content.contains("\"name\":\"search\""), "{content}");
        // 参数被序列化成 JSON 字符串。
        assert!(content.contains("{\\\"q\\\":\\\"北京\\\"}"), "{content}");

        let structured = flatten_messages(&messages, false, true);
        let call = &structured.messages[0]["tool_calls"][0];
        assert_eq!(call["id"], "call_9");
        assert_eq!(call["function"]["arguments"], "{\"q\":\"北京\"}");
    }

    #[test]
    fn normalize_tools_nests_flat_and_drops_unknown() {
        let tools = vec![
            // 平铺 → 嵌套。
            serde_json::json!({"type": "function", "name": "a", "description": "d"}),
            // 已嵌套 → 规整。
            serde_json::json!({"type": "function", "function": {"name": "b", "parameters": {"type": "object"}}}),
            // 未知形态 → 丢弃。
            serde_json::json!({"type": "custom", "name": "c"}),
            // 无名字 → 丢弃。
            serde_json::json!({"type": "function"}),
        ];
        let out = normalize_tools(&tools);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0]["function"]["name"], "a");
        assert_eq!(out[0]["function"]["parameters"]["type"], "object");
        assert_eq!(out[1]["function"]["name"], "b");
    }

    #[test]
    fn requested_effort_sources_and_canonical_map() {
        assert_eq!(
            extract_requested_effort(&serde_json::json!({"reasoning_effort": "high"})),
            Some("high".into())
        );
        assert_eq!(
            extract_requested_effort(&serde_json::json!({"reasoning": {"effort": "low"}})),
            Some("low".into())
        );
        assert_eq!(
            extract_requested_effort(&serde_json::json!({"thinking": {"level": "max"}})),
            Some("max".into())
        );
        assert_eq!(extract_requested_effort(&serde_json::json!({})), None);
        // 关闭类统一成 none；官方词表外的值丢弃。
        assert_eq!(canonical_effort("off"), Some("none"));
        assert_eq!(canonical_effort("HIGH"), Some("high"));
        assert_eq!(canonical_effort("ultra"), None);
    }

    #[test]
    fn build_chat_body_overwrites_ids_and_keeps_client_system() {
        let payload = serde_json::json!({
            "model": "Ultimate",
            "messages": [
                {"role": "system", "content": "你是测试助手"},
                {"role": "user", "content": "你好"},
            ],
            "max_tokens": 4096,
            "reasoning_effort": "max",
        });
        let body = build_chat_body(&payload, &meta(), QoderRegion::Cn, "personal_professional_trial").unwrap();
        assert_eq!(body["stream"], true);
        assert_eq!(body["aliyun_user_type"], "personal_professional_trial");
        // 客户端 system 替换模板 system。
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages[0]["content"], "你是测试助手");
        assert_eq!(messages[1]["content"], "你好");
        // id 是 uuid 且 request/chat_record 同源。
        let request_id = body["request_id"].as_str().unwrap();
        assert_eq!(body["chat_record_id"].as_str().unwrap(), request_id);
        assert_ne!(body["session_id"].as_str().unwrap(), request_id);
        assert!(Uuid::parse_str(request_id).is_ok());
        // model_config 来自元数据。
        assert_eq!(body["model_config"]["key"], "ultimate");
        assert_eq!(body["model_config"]["display_name"], "Ultimate");
        assert_eq!(body["model_config"]["is_reasoning"], true);
        assert_eq!(body["model_config"]["max_input_tokens"], 200_000);
        // chat_context 高亮 = 最新用户提示词；business.name 取前 30 字。
        assert_eq!(body["chat_context"]["text"]["text"], "你好");
        assert_eq!(body["chat_context"]["extra"]["originalContent"]["text"], "你好");
        assert_eq!(body["business"]["name"], "你好");
        // parameters：客户端给的 max_tokens + 归一化档位。
        assert_eq!(body["parameters"]["max_tokens"], 4096);
        assert_eq!(body["parameters"]["reasoning_effort"], "max");
        // tools 置空（客户端没给）。
        assert_eq!(body["tools"], serde_json::json!([]));
    }

    #[test]
    fn build_chat_body_without_system_keeps_template_prompt() {
        let payload = serde_json::json!({
            "model": "lite",
            "messages": [{"role": "user", "content": "hi"}],
        });
        let body = build_chat_body(&payload, &meta(), QoderRegion::Cn, "").unwrap();
        let messages = body["messages"].as_array().unwrap();
        // 模板 system（官方提示词）被保留。
        assert!(messages[0]["content"].as_str().unwrap().len() > 1000);
        // user_type 空 ⇒ 官方默认。
        assert_eq!(body["aliyun_user_type"], cosy::DEFAULT_USER_TYPE);
        // 客户端没给 max_tokens ⇒ 保留骨架默认 32768。
        assert_eq!(body["parameters"]["max_tokens"], 32768);
    }

    #[test]
    fn envelope_frames_unwrap_correctly() {
        // 正常 chunk。
        let frame = unwrap_envelope_line(
            "data:{\"headers\":{},\"body\":\"{\\\"choices\\\":[{\\\"delta\\\":{\\\"content\\\":\\\"你好\\\"}}]}\",\"statusCodeValue\":200}",
        );
        match frame {
            ChatFrame::Chunk(cleaned) => {
                assert!(cleaned.contains("你好"), "{cleaned}");
            }
            other => panic!("应解出 chunk: {other:?}"),
        }
        // [DONE]。
        assert_eq!(
            unwrap_envelope_line("data:{\"body\":\"[DONE]\"}"),
            ChatFrame::Done
        );
        // 非 200 ⇒ UpstreamError。
        match unwrap_envelope_line("data:{\"body\":\"boom\",\"statusCodeValue\":429}") {
            ChatFrame::UpstreamError { status, detail } => {
                assert_eq!(status, 429);
                assert_eq!(detail, "boom");
            }
            other => panic!("应是错误帧: {other:?}"),
        }
        // 计时元数据 / 注释 / 空行 ⇒ Skip。
        assert_eq!(unwrap_envelope_line("event:finish{\"x\":1}"), ChatFrame::Skip);
        assert_eq!(unwrap_envelope_line(": keep-alive"), ChatFrame::Skip);
        assert_eq!(unwrap_envelope_line(""), ChatFrame::Skip);
    }

    #[test]
    fn clean_chunk_strips_noise_and_skips_empty_deltas() {
        // 空噪声字段被剥。
        let cleaned = clean_chunk(
            "{\"choices\":[{\"delta\":{\"content\":\"x\",\"reasoning_content\":\"\",\"refusal\":\"\"}}]}",
        );
        assert!(!cleaned.contains("reasoning_content"), "{cleaned}");
        assert!(!cleaned.contains("refusal"), "{cleaned}");
        assert!(cleaned.contains("\"content\":\"x\""));
        // 整个 delta 空且无 finish_reason ⇒ 空串。
        let empty = clean_chunk("{\"choices\":[{\"delta\":{\"extra_fields\":{}}}]}");
        assert_eq!(empty, "");
        // finish_reason 单独存在 ⇒ 保留。
        let finish = clean_chunk("{\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}");
        assert!(finish.contains("finish_reason"));
    }

    #[test]
    fn leaked_tool_calls_strict_recovery() {
        let text = format!(
            "{LEAK_MARKER}\n[{{\"name\":\"search\",\"arguments\":\"{{\\\"q\\\":\\\"北京\\\"}}\"}}]"
        );
        let (calls, clean) = parse_leaked_tool_calls(&text, None);
        let calls = calls.expect("严格形态必须还原");
        assert_eq!(calls[0]["function"]["name"], "search");
        assert_eq!(clean, "");

        // 围栏包裹。
        let fenced = format!("```json\n{text}\n```");
        assert!(parse_leaked_tool_calls(&fenced, None).0.is_some());

        // 声明集合不命中 ⇒ 不还原。
        let allowed: HashSet<String> = ["other".to_string()].into_iter().collect();
        assert!(parse_leaked_tool_calls(&text, Some(&allowed)).0.is_none());

        // 散文 + 标记（非整段）⇒ 不还原（严格守卫）。
        let prose = format!("先说说工具。\n{text}");
        assert!(parse_leaked_tool_calls(&prose, None).0.is_none());
        // 普通正文 ⇒ 不还原。
        assert!(parse_leaked_tool_calls("普通回答", None).0.is_none());
    }

    #[test]
    fn model_list_passthrough_with_compat_keys() {
        let payload = serde_json::json!({
            "chat": [
                {"key": "ultimate", "display_name": "Ultimate", "is_reasoning": true, "strategies": [1]},
                {"key": "", "display_name": "缺 key 的行"},
                "not-a-dict",
            ]
        });
        let rows = parse_model_list(&payload);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["id"], "ultimate");
        assert_eq!(rows[0]["name"], "Ultimate");
        // 原始字段原样透传。
        assert_eq!(rows[0]["strategies"], serde_json::json!([1]));
    }

    #[test]
    fn refresh_response_parsing() {
        // device：token / 缺刷新令牌沿用旧值 / expires_in 毫秒。
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let refresh = parse_refresh_response(
            RefreshKind::Device,
            &serde_json::json!({"token": "dt-new", "expires_in": 86_400_000}),
            "drt-old",
        );
        assert_eq!(refresh.access_token, "dt-new");
        assert_eq!(refresh.refresh_token.as_deref(), Some("drt-old"));
        assert!((refresh.expires_at - (now + 86_400)).abs() <= 5);
        // job：expires_at RFC3339。
        let refresh = parse_refresh_response(
            RefreshKind::Job,
            &serde_json::json!({"token": "jt-new", "expires_at": "2030-01-02T03:04:05Z"}),
            "jrt-old",
        );
        assert_eq!(refresh.expires_at, 1_893_553_445);
    }

    #[test]
    fn checkin_status_parsing() {
        let payload = serde_json::json!({
            "status": "CLAIMED", "lastClaimedAt": 1_789_000_000,
            "currentStreakDays": 3, "totalClaimDays": 12,
            "rewardCredits": 100, "totalRewardCredits": 1200,
            "nextClaimAt": 1_789_086_400, "rewardExpiresAt": 1_790_000_000,
        });
        // 1_789_000_000（UTC+8）= 2026-09-10 左右，用它自己算出来的日期断言。
        let day = chrono::DateTime::from_timestamp(1_789_000_000, 0)
            .unwrap()
            .with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).unwrap())
            .format("%Y-%m-%d")
            .to_string();
        let status = parse_checkin_status(&payload, &day);
        assert_eq!(status.status, "CLAIMED");
        assert!(status.active);
        assert!(status.today_checked_in);
        assert_eq!(status.streak_days, 3);
        // 不同「今天」⇒ 不算今日已签。
        let status = parse_checkin_status(&payload, "1999-01-01");
        assert!(!status.today_checked_in);
    }

    #[test]
    fn quota_parsing_and_degenerate_guard() {
        let view = parse_quota(&serde_json::json!({
            "userQuota": {"remaining": 700, "used": 300, "total": 1000},
            "addOnQuota": {"remaining": 50, "used": 10, "total": 60},
        }))
        .unwrap();
        assert_eq!(view.remaining, 750);
        assert_eq!(view.used, 310);
        assert_eq!(view.total, 1060);

        // 两键都缺失 ⇒ 拒绝（不写 0）。
        assert!(parse_quota(&serde_json::json!({"something": 1})).is_err());
        // 退化形态（exceeded 且全 0）⇒ 拒绝。
        assert!(parse_quota(&serde_json::json!({
            "isQuotaExceeded": true,
            "userQuota": {"remaining": 0, "used": 0, "total": 0},
            "addOnQuota": {"remaining": 0, "used": 0, "total": 0},
        }))
        .is_err());
        // 真·耗尽（size>0 而 remain=0）⇒ 放行（0 是真实余额）。
        let exhausted = parse_quota(&serde_json::json!({
            "isQuotaExceeded": true,
            "userQuota": {"remaining": 0, "used": 1000, "total": 1000},
            "addOnQuota": {"remaining": 0, "used": 0, "total": 0},
        }))
        .unwrap();
        assert_eq!(exhausted.remaining, 0);
    }

    #[test]
    fn openapi_headers_carry_auth_and_machine_ids() {
        let headers = openapi_headers(QoderRegion::Cn, "q1", "dt-abc");
        let lookup = |name: &str| {
            headers
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        };
        assert_eq!(lookup("Authorization").as_deref(), Some("Bearer dt-abc"));
        assert_eq!(
            lookup("X-Machine-ID").as_deref(),
            Some("abec85ce8c7ed7ebd0041ee4ca63e701")
        );
        assert!(lookup("X-Request-ID").unwrap().starts_with(&super::super::device::derive_id("q1", "req")));
        assert!(lookup("Origin").unwrap().contains("qoder"));
    }

    #[test]
    fn structured_tool_history_rules() {
        let good = vec![
            serde_json::json!({"role": "assistant", "tool_calls": [{"id": "c1"}]}),
            serde_json::json!({"role": "tool", "tool_call_id": "c1", "content": "x"}),
        ];
        let bad_ids = vec![serde_json::json!({"role": "tool", "content": "x"})];
        // 默认 auto：非 CN ⇒ false；缺 id ⇒ false。
        assert!(!structured_tool_history_enabled("qwen-max", QoderRegion::Global, &good));
        assert!(!structured_tool_history_enabled("qwen-max", QoderRegion::Cn, &bad_ids));
        std::env::set_var("QD_STRUCTURED_TOOL_HISTORY", "on");
        assert!(structured_tool_history_enabled("whatever", QoderRegion::Global, &good));
        std::env::set_var("QD_STRUCTURED_TOOL_HISTORY", "off");
        assert!(!structured_tool_history_enabled("qwen-max", QoderRegion::Cn, &good));
        std::env::remove_var("QD_STRUCTURED_TOOL_HISTORY");
        // 默认 auto：CN + 白名单前缀 ⇒ true。
        assert!(structured_tool_history_enabled("qwen3.8-max", QoderRegion::Cn, &good));
    }

    // ── 真机探针（无账号无法验证，刻意保留位） ──

    fn live_account() -> (String, String) {
        (
            std::env::var("QODER_LIVE_UID").expect("需 QODER_LIVE_UID"),
            std::env::var("QODER_LIVE_ACCESS_TOKEN").expect("需 QODER_LIVE_ACCESS_TOKEN"),
        )
    }

    #[tokio::test]
    #[ignore = "真机探针：需 QODER_LIVE_UID / QODER_LIVE_ACCESS_TOKEN（本机无账号）"]
    async fn live_model_list() {
        let (uid, token) = live_account();
        let input = CosyAccountInput {
            uid: &uid,
            nickname: "",
            access_token: &token,
            refresh_token: "",
            user_type: None,
            org_id: None,
            org_name: None,
        };
        let models = fetch_model_list(QoderRegion::Cn, &input).await.unwrap();
        assert!(!models.is_empty());
    }

    #[tokio::test]
    #[ignore = "真机探针：需 QODER_LIVE_UID / QODER_LIVE_ACCESS_TOKEN（本机无账号）"]
    async fn live_quota_and_checkin() {
        let (uid, token) = live_account();
        let quota = quota_usage(QoderRegion::Cn, &uid, &token).await.unwrap();
        println!("remaining={} total={}", quota.remaining, quota.total);
        let _ = checkin_status(QoderRegion::Cn, &uid, &token).await;
    }
}
