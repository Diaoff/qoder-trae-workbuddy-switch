//! webui / CLI 形态的 **Loomy** 网关宿主（与 Qoder 宿主平行；默认 `127.0.0.1:57896`）。
//!
//! 与 [`crate::gateway_host`] / [`crate::trae_gateway_host`] / [`crate::qoder_gateway_host`]
//! 平行、**各持一个泛型 [`GatewayHost`]**（原因见 `buddy_switch_gateway::loomy` 模块文档
//! 与 [`buddy_switch_gateway::GatewayProduct`]）。
//!
//! ## 为什么不把 Loomy 路由 merge 进 webui 的 API Router
//!
//! `buddy_switch_gateway::loomy::router()` 也挂 `/v1/models` 与 `/v1/chat/completions`，
//! 与 WB / Trae / Qoder 网关的路径**逐字相同**。axum 的 `merge` 遇到同路径会直接 panic，
//! 所以 Loomy 网关只能走**自己的端口**（57896）；webui 侧只暴露 `/api/loomy/gateway/*` 管理面。
//!
//! ## 与统一入口并存
//!
//! Loomy 一期就走统一入口（`loomy-cn:模型名` 经 57892）。本独立端口是**可选**的第二形态：
//! 两者共用同一份 `LoomyGatewayState`（池 / 日志 / 禁用 / 粘性），关掉任一个不影响另一个。

use std::sync::OnceLock;

use tokio::sync::Mutex;

use buddy_switch_gateway::loomy::{LoomyGateway, LoomyGatewayState};
use buddy_switch_gateway::{GatewayHost, GatewayProduct};

static HOST: OnceLock<Mutex<GatewayHost<LoomyGateway>>> = OnceLock::new();

/// 进程内共享的 Loomy 网关状态（惰性初始化，读一次配置）。
pub fn shared_state() -> LoomyGatewayState {
    <LoomyGateway as GatewayProduct>::shared_state()
}

fn host() -> &'static Mutex<GatewayHost<LoomyGateway>> {
    HOST.get_or_init(|| Mutex::new(GatewayHost::new()))
}

/// 按当前配置应用（启动 / 重启）独立监听。
pub async fn apply() -> Result<Option<String>, String> {
    host().lock().await.start().await
}

/// 查询运行状态（运行中, 监听地址）。
pub async fn status() -> (bool, Option<String>) {
    let host = host().lock().await;
    (host.is_running(), host.addr())
}
