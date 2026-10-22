//! 桌面宿主（Tauri）的 **Loomy** 网关接入：独立监听句柄 + 进程内共享状态。
//!
//! 与 [`crate::qoder_gateway`] **同构**（各持一个泛型
//! [`GatewayHost`](buddy_switch_gateway::GatewayHost)，只是产品线参数不同：
//! [`QoderGateway`](buddy_switch_gateway::qoder::QoderGateway) vs
//! [`LoomyGateway`](buddy_switch_gateway::loomy::LoomyGateway)）。
//!
//! ## 与 Qoder 侧的两处差异
//!
//! 1. **单作用域**：Loomy 只有 `loomy-cn`，池 / Key 都没有归属轴 ⇒ 管理命令不带区域参数。
//! 2. 与**统一入口并存**：Loomy 一期就走 `loomy-cn:` 命名空间（57892），本独立端口是
//!    可选第二形态；两者共用同一份 state，关掉任一个不影响另一个。
//!
//! ⚠️ 四条网关各持一个 router、各自端口（Loomy 57896 / Qoder 57895 / Trae 57894 /
//! WorkBuddy 57891），**绝不 merge**（axum 同路径 merge 会 panic）。

use std::sync::Mutex;

use buddy_switch_gateway::loomy::{LoomyGateway, LoomyGatewayState};
use buddy_switch_gateway::{GatewayHost, GatewayProduct};

/// 进程内共享的 Loomy 网关状态（惰性初始化，读一次配置）。
///
/// 独立监听与管理命令复用**同一份**状态，因此「API 服务」页看到的请求日志、
/// 账号池与真实打进来的请求完全一致。
pub fn shared_state() -> LoomyGatewayState {
    <LoomyGateway as GatewayProduct>::shared_state()
}

/// 独立监听生命周期（Tauri managed state）。
///
/// 锁策略与 [`crate::qoder_gateway::QoderGatewayRuntime`] 同款
/// （`std::sync::Mutex` + 锁外 `await`，因 `loomy_gateway_status` 需同步读运行态）。
pub struct LoomyGatewayRuntime {
    host: Mutex<GatewayHost<LoomyGateway>>,
}

impl Default for LoomyGatewayRuntime {
    fn default() -> Self {
        Self {
            host: Mutex::new(GatewayHost::new()),
        }
    }
}

impl LoomyGatewayRuntime {
    /// 新建（未启动）。
    pub fn new() -> Self {
        Self::default()
    }

    /// 按当前配置启动 / 重启独立监听。返回实际监听地址；未启用时返回 `None`。
    pub async fn apply(&self) -> Result<Option<String>, String> {
        let mut host = self.take_host();
        let result = host.start().await;
        self.put_host(host);
        result
    }

    /// 是否运行中。
    pub fn is_running(&self) -> bool {
        self.host.lock().unwrap().is_running()
    }

    /// 运行中的监听地址。
    pub fn addr(&self) -> Option<String> {
        self.host.lock().unwrap().addr()
    }

    /// 取出宿主（在**锁外** `await` 用；[`GatewayHost`] 实现了 `Default`）。
    fn take_host(&self) -> GatewayHost<LoomyGateway> {
        std::mem::take(&mut *self.host.lock().unwrap())
    }

    /// 放回宿主（与 [`LoomyGatewayRuntime::take_host`] 配对）。
    fn put_host(&self, host: GatewayHost<LoomyGateway>) {
        *self.host.lock().unwrap() = host;
    }
}
