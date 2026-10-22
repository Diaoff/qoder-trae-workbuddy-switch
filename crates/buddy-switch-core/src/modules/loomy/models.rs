//! Loomy 的模型目录解析（`GET /models` 的三种响应形态）与倍率拆解。
//!
//! ## 🔴 必须认**三种**形态（实测缺陷，free-llm-router 踩过）
//!
//! 1. **业务信封**：`{code:'000000', data:[...]}`；
//! 2. **裸数组**：`[...]`；
//! 3. **裸对象 + `data[]`**（⚠️ 实测 `/models` 的真实主形态，顶层**没有 `code` 字段**，
//!    是 OpenAI 风格列表信封 `object:"list"` + `data[]`）。
//!
//! 只认信封会让第 3 种被「拆信封」路径判「非信封」⇒ 恒得空目录 ⇒
//! 错误文案误导用户去查上游（「目录为空：上游响应形状可能已变化」），
//! 而真实原因是**我们自己少认了一种形态**。
//!
//! ## 倍率拆解（显示层的硬需求）
//!
//! Loomy 的倍率**没有独立字段**，写在 `name` 字符串末尾的括号里，且三种括号风格混用：
//! `MiniMax M3 （x4.0）` / `Qwen 3.8 Max (x12.0)` / `GLM 5.3 Flash(x0.8)`。
//! 展示名规范化为 `{name} · x{rate}`；拆解**幂等**
//! （对规范化名再拆一次结果一致），重复规范化不会累积 ` · `。
//!
//! 蓝本为 `../free-llm-router/src/providers/loomy.ts` 的 `parseLoomyModels` /
//! `splitLoomyRate`（协议已实测，非本仓文件）。

use serde_json::Value;

/// 是否为可对话的 chat 模型：判据是 `type === "chat"` 且有非空 `id`。
///
/// ⚠️ **不要**改用 `input_modalities` —— 实测 chat 模型的输入模态含 `image`
/// （能看图），那是输入多模态，与「是不是生图模型」无关。
fn is_chat_model(entry: &Value) -> bool {
    let Some(obj) = entry.as_object() else {
        return false;
    };
    let has_id = obj
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    has_id && obj.get("type").and_then(|v| v.as_str()) == Some("chat")
}

/// 从展示名（或原始名）里拆出倍率：`(纯名, 倍率)`。
///
/// 认两种形态：末尾括号（全角/半角都认）与已规范化的 `{name} · x1.0`。
/// **幂等**：`split_rate(display_name(x)) == split_rate(x)`。
pub fn split_rate(raw_name: &str) -> (String, String) {
    let original = raw_name.trim();
    if original.is_empty() {
        return (String::new(), String::new());
    }
    // 形态 1：末尾括号（全角/半角都认）。
    if let Some((head, rate)) = extract_trailing_rate_bracket(original) {
        if !head.trim().is_empty() {
            return (head.trim().to_string(), rate);
        }
        return (original.to_string(), String::new());
    }
    // 形态 2：已规范化的 `{name} · x1.0`。
    if let Some(pos) = original.rfind(" · ") {
        let (head, tail) = original.split_at(pos);
        let tail = &tail[" · ".len()..];
        if let Some(rate) = normalize_rate(tail) {
            if !head.trim().is_empty() {
                return (head.trim().to_string(), rate);
            }
        }
    }
    (original.to_string(), String::new())
}

/// 规范化一个倍率 token：`x4.0` / `X 4.0` / `x0.8` ⇒ `x4.0` 形态；不匹配 ⇒ `None`。
fn normalize_rate(token: &str) -> Option<String> {
    let cleaned: String = token.chars().filter(|c| !c.is_whitespace()).collect();
    let lowered = cleaned.to_ascii_lowercase();
    let digits = lowered.strip_prefix('x')?;
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    // 必须能解析成有限正数（防 `x.` / `x..` 这类垃圾通过）。
    let value: f64 = digits.parse().ok()?;
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    Some(format!("x{digits}"))
}

/// 末尾括号形态：`...（x4.0）` / `...(x12.0)`。
fn extract_trailing_rate_bracket(name: &str) -> Option<(String, String)> {
    // 两种闭括号都要试：名字里可能出现半角括号包全角的情况。
    let open_positions = [name.rfind('（'), name.rfind('(')]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    for open in open_positions {
        let open_char = name[open..].chars().next()?;
        let close_char = if open_char == '（' { '）' } else { ')' };
        let inner = &name[open + open_char.len_utf8()..];
        let Some(stripped) = inner.strip_suffix(close_char) else {
            continue;
        };
        // 倍率括号必须是**末尾**（inner 闭括号后无内容 —— strip_suffix 已保证），
        // 且内部无嵌套括号。
        if stripped.contains(['(', '（', ')', '）']) {
            continue;
        }
        if let Some(rate) = normalize_rate(stripped) {
            return Some((name[..open].to_string(), rate));
        }
    }
    None
}

/// 一条解析后的模型条目。
#[derive(Debug, Clone, PartialEq)]
pub struct LoomyModel {
    /// 模型 id（原样，上游用）。
    pub id: String,
    /// 规范化展示名：`{name} · x{rate}`（无倍率时为纯名）。
    pub name: String,
    /// 上下文窗口（0 = 未知）。
    pub context_window: i64,
    /// 是否支持图片输入（`capabilities.input_modalities` 含 `image`）。
    pub supports_image: bool,
}

/// 展示名规范化：`{name} · x{rate}`；无倍率时原样（**不**追加分隔符）。
pub fn display_name(raw_name: &str) -> String {
    let (name, rate) = split_rate(raw_name);
    if rate.is_empty() {
        name
    } else {
        format!("{name} · {rate}")
    }
}

/// 把远端 `GET /models` 的响应解析成模型列表。
///
/// ⚠️ 同时认三种形态（见模块头）；空列表**不算错**（由调用方决定「目录为空」
/// 是否要显式报错 —— 网关路径会显式报，管理面路径给 note）。
pub fn parse_models(payload: &Value) -> Vec<LoomyModel> {
    // 形态 1：业务信封 `code === "000000"` 且 data 是数组。
    let list = if payload.as_array().is_some() {
        payload.as_array().unwrap().clone()
    } else if let Some(data) = payload.get("data").and_then(|v| v.as_array()) {
        data.clone()
    } else {
        Vec::new()
    };

    let mut models = Vec::new();
    for item in &list {
        if !is_chat_model(item) {
            continue;
        }
        let Some(obj) = item.as_object() else {
            continue;
        };
        let id = obj.get("id").and_then(|v| v.as_str()).unwrap_or_default();
        let raw_name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(id);
        let context_window = obj
            .get("context_length")
            .and_then(|v| v.as_i64())
            .filter(|n| *n > 0)
            .unwrap_or(0);
        let supports_image = obj
            .get("capabilities")
            .and_then(|caps| caps.get("input_modalities"))
            .and_then(|v| v.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|v| v.as_str())
                    .any(|s| s.eq_ignore_ascii_case("image"))
            })
            .unwrap_or(false);
        models.push(LoomyModel {
            id: id.to_string(),
            name: display_name(raw_name),
            context_window,
            supports_image,
        });
    }
    models
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn split_rate_handles_all_three_bracket_styles() {
        assert_eq!(
            split_rate("MiniMax M3 （x4.0）"),
            ("MiniMax M3".to_string(), "x4.0".to_string())
        );
        assert_eq!(
            split_rate("Qwen 3.8 Max (x12.0)"),
            ("Qwen 3.8 Max".to_string(), "x12.0".to_string())
        );
        assert_eq!(
            split_rate("GLM 5.3 Flash(x0.8)"),
            ("GLM 5.3 Flash".to_string(), "x0.8".to_string())
        );
        // 无倍率：原样返回。
        assert_eq!(split_rate("DeepSeek V4.1"), ("DeepSeek V4.1".to_string(), String::new()));
        assert_eq!(split_rate(""), (String::new(), String::new()));
        // 括号里不是倍率 ⇒ 不拆。
        assert_eq!(
            split_rate("Model (beta)"),
            ("Model (beta)".to_string(), String::new())
        );
    }

    #[test]
    fn split_rate_is_idempotent_over_display_names() {
        let raw = "MiniMax M3 （x4.0）";
        let display = display_name(raw);
        assert_eq!(display, "MiniMax M3 · x4.0");
        // 对已规范化名再拆一次 ⇒ 一致（不累积 ` · `）。
        assert_eq!(split_rate(&display), ("MiniMax M3".to_string(), "x4.0".to_string()));
        assert_eq!(display_name(&display), display);
    }

    #[test]
    fn parse_models_accepts_all_three_response_shapes() {
        let entries = json!([
            {"id": "m1", "name": "MiniMax M3 （x4.0）", "type": "chat",
             "context_length": 128000,
             "capabilities": {"input_modalities": ["text", "image"]}},
            {"id": "m2", "name": "GLM 5.3 Flash(x0.8)", "type": "chat"},
            {"id": "img1", "name": "画图", "type": "image"}
        ]);
        let envelope = json!({"code": "000000", "desc": "ok", "data": entries});
        let bare_object = json!({"object": "list", "data": entries});
        for payload in [entries.clone(), envelope, bare_object] {
            let models = parse_models(&payload);
            assert_eq!(models.len(), 2, "只认 chat 模型；形态 {payload}");
            assert_eq!(models[0].id, "m1");
            assert_eq!(models[0].name, "MiniMax M3 · x4.0");
            assert_eq!(models[0].context_window, 128000);
            assert!(models[0].supports_image);
            assert_eq!(models[1].name, "GLM 5.3 Flash · x0.8");
            assert!(!models[1].supports_image, "无 modalities ⇒ false");
        }
    }
}
