//! `GET /v1/models`：按 Key 绑定 region 返回模型列表 + 来源标注（P0-4）。

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use serde_json::json;

use crate::disabled_models::is_disabled;
use crate::routes::{authenticate, json_response, openai_error};
use crate::state::GatewayState;
use buddy_switch_core::modules::catalog::CatalogModel;
use buddy_switch_core::modules::product::scope_of_wb_region;

/// 一条 WB 模型在 OpenAI `/v1/models` 里的形状（**唯一产出点**：独立网关与统一网关共用）。
///
/// ★ `context_length` 必须与 Trae 侧同名字段对齐 —— OpenAI 兼容客户端读的是它；
/// `context_window` 保留给老客户端与管理面（WB 目录原本就用这个名字）。
pub fn model_entry(model: &CatalogModel) -> serde_json::Value {
    json!({
        "id": model.id,
        "object": "model",
        "created": 0,
        "owned_by": "workbuddy",
        "context_length": model.context_window,
        "context_window": model.context_window,
        "max_tokens": model.max_tokens,
        "supports_images": model.supports_images,
        "credits": model.credits,
        "badges": model.badges,
        "free": model.free,
    })
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
    let data: Vec<serde_json::Value> = snapshot
        .models
        .iter()
        .filter(|model| !is_disabled(&disabled, scope_of_wb_region(record.region), &model.id))
        .map(model_entry)
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
            max_tokens: 64_000,
            supports_images: true,
            credits: None,
            badges: vec![],
            free: false,
        };
        let entry = model_entry(&model);
        assert_eq!(entry["context_length"], 960_000);
        assert_eq!(entry["context_window"], 960_000);
    }
}
