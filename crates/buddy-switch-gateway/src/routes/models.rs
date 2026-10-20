//! `GET /v1/models`：按 Key 绑定 region 返回模型列表 + 来源标注（P0-4）。

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use serde_json::json;

use crate::disabled_models::is_disabled;
use crate::routes::{authenticate, json_response, openai_error};
use crate::state::GatewayState;
use buddy_switch_core::modules::catalog::{CatalogModel, CatalogSnapshot};
use buddy_switch_core::modules::product::scope_of_wb_region;

/// 一条 WB 模型在 OpenAI `/v1/models` 里的形状（**唯一产出点**：独立网关与统一网关共用）。
///
/// ★ `context_length` 必须与 Trae 侧同名字段对齐 —— OpenAI 兼容客户端读的是它；
/// `context_window` 保留给老客户端与管理面（WB 目录原本就用这个名字）。
pub fn model_entry(model: &CatalogModel, max_context: bool) -> serde_json::Value {
    // 开关决定声明「上限档」还是「默认档」（上游两档：如 300k 默认 / 1M 上限）。
    let window = if max_context {
        model.context_window
    } else {
        model.context_window_default.unwrap_or(model.context_window)
    };
    json!({
        "id": model.id,
        "object": "model",
        "created": 0,
        "owned_by": "workbuddy",
        "context_length": window,
        "context_window": window,
        "max_tokens": model.max_tokens,
        "supports_images": model.supports_images,
        "credits": model.credits,
        "badges": model.badges,
        "free": model.free,
    })
}

/// 管理面用的目录快照视图：把每个模型的 `context_window` 按开关换成**生效窗口**，
/// 并补上 `context_length`（与网关出口 ① 同源同值）。
///
/// 没有这一步，管理面会一直显示目录原始字段（上限档），开关在界面上看起来「没反应」。
pub fn snapshot_view(snapshot: &CatalogSnapshot, max_context: bool) -> serde_json::Value {
    let mut snapshot = snapshot.clone();
    for model in &mut snapshot.models {
        model.context_window = model.effective_window(max_context);
    }
    serde_json::to_value(&snapshot).unwrap_or(serde_json::Value::Null)
}

/// 模型列表处理器。
pub async fn handler(State(state): State<GatewayState>, headers: HeaderMap) -> Response {
    let record = match authenticate(&state, &headers) {
        Ok(record) => record,
        Err(error) => return openai_error(error),
    };

    let snapshot = state.catalogs.current(record.region);
    // ★ 出口 ①（P11）：语义是「**可用的**模型清单」——不可服务与被用户禁用的都**不出现**。
    //   WB 侧 `available` 恒 true（无「非可服务」概念，故此处无 no-op 过滤）；
    //   禁用过滤读的是**与出口 ② 同一份** `disabled`（`is_disabled` 是唯一判断点）。
    let disabled = state.disabled.read().await;
    let max_context = state.config.read().await.max_context;
    let data: Vec<serde_json::Value> = snapshot
        .models
        .iter()
        .filter(|model| !is_disabled(&disabled, scope_of_wb_region(record.region), &model.id))
        .map(|model| model_entry(model, max_context))
        .collect();
    drop(disabled);

    json_response(
        StatusCode::OK,
        json!({
            "object": "list",
            "data": data,
            "wb_region": snapshot.region,
            "wb_source": snapshot.source,
            "wb_fetched_at": snapshot.fetched_at,
            "wb_note": snapshot.note,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ★ 出口①必须同时给 `context_length`（OpenAI 客户端读）与 `context_window`（老字段）。
    #[test]
    fn wb_model_entry_declares_openai_context_length() {
        let model = CatalogModel {
            id: "hy4-preview".into(),
            name: "Hy4 preview".into(),
            context_window: 960_000,
            context_window_default: Some(300_000),
            max_tokens: 64_000,
            supports_images: true,
            credits: None,
            badges: vec![],
            free: false,
        };
        let max = model_entry(&model, true);
        assert_eq!(max["context_length"], 960_000, "默认用上限档");
        assert_eq!(max["context_window"], 960_000);

        // 开关关 ⇒ 声明默认档（上游 contextWindow.defaultLength）。
        let lower = model_entry(&model, false);
        assert_eq!(lower["context_length"], 300_000);
        assert_eq!(lower["context_window"], 300_000);
    }

    /// 没有默认档的上游（如国内目录只给一个数）⇒ 关开关也回落到上限，不能变 0。
    #[test]
    fn wb_model_entry_falls_back_when_no_default_window() {
        let model = CatalogModel {
            id: "glm-5.3".into(),
            name: "GLM-5.3".into(),
            context_window: 1_000_000,
            context_window_default: None,
            max_tokens: 48_000,
            supports_images: true,
            credits: None,
            badges: vec![],
            free: false,
        };
        assert_eq!(model_entry(&model, false)["context_length"], 1_000_000);
    }
}
