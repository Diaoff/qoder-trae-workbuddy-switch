//! Loomy 产品线（`buddy-switch-gateway` 侧）—— **统一网关中继**形态。
//!
//! 与 Qoder 的差异（§11-D 契约）：**默认不开独立端口 / Key 库**，只走统一入口
//! （`loomy-cn:模型名` 经 57892）。因此本模块没有自己的 axum router / apikey，
//! 只提供：
//!
//! - [`pool`]：账号池（实现 [`crate::pool::entry_like::PoolEntryLike`]，复用选号内核）；
//! - [`routes`]：`chat_for_region`（认证之后的逻辑**只有一份**，统一网关复用）；
//! 协议实现在 `buddy_switch_core::modules::loomy`（头 / 信封 / 签名），
//! 本模块只做网关装配。
//!
//! ## 真机状态
//!
//! 本机无 Loomy 账号 ⇒ 上游链路**未经真机验证**：所有断言都是协议级单测；
//! 首次真机跑通前，不要把「网关可用」当作已有事实。

pub mod pool;
pub mod routes;

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::Mutex;

use buddy_switch_core::modules::loomy::region::LoomyRegion;

use crate::loomy::pool::LoomyPool;

/// Loomy 中继的共享状态（**无独立端口** ⇒ 没有 config / key store / router）。
#[derive(Clone)]
pub struct LoomyGatewayState {
    /// 区域 → 账号池。
    pub pools: Arc<Mutex<std::collections::HashMap<LoomyRegion, LoomyPool>>>,
    /// 用户禁用的模型（键 = 作用域 `loomy-cn`）。
    pub disabled: Arc<Mutex<crate::disabled_models::DisabledModels>>,
    /// 会话粘性绑定表。
    pub sticky: Arc<Mutex<crate::StickyTable>>,
    /// 请求日志（与其它产品线同一份机制）。
    pub log: Arc<crate::logging::RequestLog>,
    /// 出站 HTTP 客户端。
    pub http: reqwest::Client,
    /// 出站基址覆盖（**测试挂载点**：把出站打到本地 mock 上游；生产恒 `None`）。
    pub upstream_override: Option<String>,
    /// 总请求数（诊断）。
    pub total_requests: Arc<std::sync::atomic::AtomicU64>,
}

impl LoomyGatewayState {
    /// 进程内单例（与其它产品线同款 OnceLock 模式）。
    pub fn shared() -> Self {
        static SHARED: std::sync::OnceLock<LoomyGatewayState> = std::sync::OnceLock::new();
        SHARED
            .get_or_init(|| Self {
                pools: Arc::new(Mutex::new(std::collections::HashMap::new())),
                disabled: Arc::new(Mutex::new(
                    crate::disabled_models::DisabledModels::load(),
                )),
                sticky: Arc::new(Mutex::new(crate::StickyTable::default())),
                log: Arc::new(crate::logging::RequestLog::new(
                    buddy_switch_core::modules::loomy::paths::loomy_dir()
                        .join("gateway_requests.jsonl"),
                    200,
                    false,
                )),
                http: crate::loomy::routes::build_http_client(),
                upstream_override: None,
                total_requests: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            })
            .clone()
    }

    /// 会话粘性查询（已过期视为无绑定）。
    pub async fn sticky_uid(&self, key: Option<&str>) -> Option<String> {
        let key = key?;
        self.sticky
            .lock()
            .await
            .get(key, buddy_switch_core::modules::config::now_ms())
            .map(str::to_string)
    }

    /// 绑定会话粘性（只在上游成功之后调用）。
    pub async fn bind_sticky(&self, key: Option<&str>, uid: &str) {
        let Some(key) = key else { return };
        self.sticky
            .lock()
            .await
            .bind(
                key,
                uid,
                buddy_switch_core::modules::config::now_ms(),
            );
    }

    /// 请求日志（中继记账用）。
    pub async fn record_request(
        &self,
        endpoint: &str,
        region: LoomyRegion,
        model: &str,
        status: u16,
        uid: &str,
        elapsed_ms: i64,
        stream: bool,
        prompt_tokens: i64,
        completion_tokens: i64,
        error: Option<String>,
    ) {
        self.total_requests
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.log.record(serde_json::json!({
            "ts": buddy_switch_core::modules::config::now_ms(),
            "endpoint": endpoint,
            "method": "POST",
            "region": region.as_str(),
            "account": uid,
            "accountUid": uid,
            "model": model,
            "status": status,
            "latencyMs": elapsed_ms,
            "promptTokens": prompt_tokens,
            "completionTokens": completion_tokens,
            "stream": stream,
            "error": error,
        }));
    }
}

/// 统一网关管理面的池投影（`unified::mod` 调用）。
pub async fn pool_view_for(region: LoomyRegion) -> (Value, Vec<Value>) {
    let state = LoomyGatewayState::shared();
    let now = buddy_switch_core::modules::config::now_ms();
    let mut pools = state.pools.lock().await;
    let pool = pools.entry(region).or_default();
    pool.sync_for(region);
    (pool.summary(now), pool.accounts_view(now))
}
