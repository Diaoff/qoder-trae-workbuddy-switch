//! Loomy 产品线（`buddy-switch-gateway` 侧）—— **统一网关中继** + **可选独立端口**。
//!
//! ## 两种形态并存（不是二选一）
//!
//! | 形态 | 入口 | Key 前缀 | 何时用 |
//! |:--|:--|:--|:--|
//! | 统一入口（一期既有） | `57892`，模型名写 `loomy-cn:<id>` | `sk-uni-` | 已在用统一网关的用户 |
//! | 独立端口（二期 9.2） | `57896`，模型名写模型 id 即可 | `sk-loomy-` | 只想接 Loomy、不想开统一入口 |
//!
//! 两者**共用同一份** [`LoomyGatewayState`]（池 / 日志 / 禁用 / 粘性）与**同一份**
//! [`routes::chat_for_region`]（认证之后的逻辑只有一份）。差异只在「谁做鉴权、
//! 路径前缀怎么解析」—— 这两件事由 [`router`] 与统一网关各自承担。
//!
//! ## ⚠️ 为什么必须独立端口（不能 merge）
//!
//! 本产品线的 `/v1/models` / `/v1/chat/completions` / `/v1/messages` / `/v1/responses`
//! 与 WB / Trae / Qoder **逐字重叠** —— axum 同路径 `merge` 会在**运行期 panic**。
//! 端口取 `5789x` 一段的最后一位 **57896**（57890 界面 / 91 WB / 92 统一 / 93 hook /
//! 94 Trae / 95 Qoder / **96 Loomy**）。
//!
//! ## 与 Qoder 侧的两处刻意差异
//!
//! 1. **Key 无归属轴**：Loomy 单作用域 `loomy-cn`，给 Key 加一个恒为 `cn` 的字段
//!    就是「数据里多一列永远不变的值」（[`apikey`] 模块头有完整理由）。
//! 2. **默认端口 57896 而不是 5789x 里的空缺**：那一位是给第四条**独立**网关留的；
//!    Loomy 正是第四条。
//!
//! ## 真机状态
//!
//! 上游链路（业务头 / 恒 200 信封 / `100002` 不重试）已按蓝本实现并有协议级单测，
//! 但**未经本机真机验证**：首次真机跑通前，不要把「网关可用」当作已有事实。

pub mod apikey;
pub mod pool;
pub mod routes;

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::sync::{oneshot, Mutex, RwLock};

use buddy_switch_core::modules::config as core_config;
use buddy_switch_core::modules::loomy::paths;
use buddy_switch_core::modules::loomy::region::LoomyRegion;

use crate::loomy::pool::LoomyPool;

/// Loomy 网关的共享状态（统一入口与独立端口 **共用同一份**）。
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
    /// 独立端口网关配置（**独立端口形态**才用；统一入口形态只读它做诊断）。
    pub config: Arc<RwLock<LoomyGatewayConfig>>,
    /// 多 API Key 存储（`sk-loomy-`）。
    pub key_store: Arc<apikey::LoomyApiKeyStore>,
    /// 启动时刻（诊断）。
    pub started_at: i64,
    /// 最近一次上游错误（`/status` 与管理面透出）。
    pub last_error: Arc<RwLock<Option<String>>>,
    /// 请求体上限字节数（**构造期固化**进 axum `DefaultBodyLimit`）。
    pub body_limit_bytes: usize,
    /// 模型清单缓存（单作用域 ⇒ 一格；带 `fetched_at` 与来源说明）。
    pub(crate) catalog: Arc<RwLock<Option<CachedCatalog>>>,
}

impl LoomyGatewayState {
    /// 按配置新建（测试与宿主都用它；`shared()` 是进程单例壳）。
    pub fn new(config: LoomyGatewayConfig) -> Self {
        let log = Arc::new(crate::logging::RequestLog::new(
            paths::logs_dir().join("gateway_requests.jsonl"),
            config.log_keep,
            config.log_bodies,
        ));
        Self {
            pools: Arc::new(Mutex::new(std::collections::HashMap::new())),
            disabled: Arc::new(Mutex::new(
                crate::disabled_models::DisabledModels::load(),
            )),
            sticky: Arc::new(Mutex::new(crate::StickyTable::new(config.sticky_ttl_ms))),
            log,
            http: crate::loomy::routes::build_http_client(),
            upstream_override: None,
            total_requests: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            // `log_keep` / `log_bodies` 已在上面固化进 `RequestLog`；config 保留快照供
            // 管理面读（改它们需重启网关，与另三条产品线同一纪律）。
            config: Arc::new(RwLock::new(config.clone())),
            key_store: Arc::new(apikey::LoomyApiKeyStore::new(paths::gateway_keys_file())),
            started_at: core_config::now_ms(),
            last_error: Arc::new(RwLock::new(None)),
            body_limit_bytes: crate::body_limit_bytes(config.max_body_mb),
            catalog: Arc::new(RwLock::new(None)),
        }
    }

    /// 测试用：覆盖出站基址（把出站打到本地 mock 上游）。
    pub fn with_upstream(mut self, upstream: String) -> Self {
        self.upstream_override = Some(upstream);
        self
    }

    /// 进程内单例（与其它产品线同款 OnceLock 模式）。
    pub fn shared() -> Self {
        static SHARED: std::sync::OnceLock<LoomyGatewayState> = std::sync::OnceLock::new();
        SHARED
            .get_or_init(|| Self::new(LoomyGatewayConfig::load()))
            .clone()
    }

    /// 配置快照。
    pub async fn config_snapshot(&self) -> LoomyGatewayConfig {
        self.config.read().await.clone()
    }

    /// 会话粘性查询（已过期视为无绑定）。
    pub async fn sticky_uid(&self, key: Option<&str>) -> Option<String> {
        let key = key?;
        self.sticky
            .lock()
            .await
            .get(key, core_config::now_ms())
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
            "ts": core_config::now_ms(),
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
            "error": error.clone(),
        }));
        if let Some(message) = error {
            *self.last_error.write().await = Some(message);
        }
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

// ---------------------------------------------------------------------------
// 独立端口网关：配置 / 状态视图 / 路由 / 产品线注册（二期 9.2）
// ---------------------------------------------------------------------------

/// Loomy 网关默认端口。
///
/// ★ `5789x` 一段的最后一位：界面 57890 · WB 57891 · 统一 57892 · hook 57893 ·
/// Trae 57894 · Qoder 57895 · **Loomy 57896**。
pub const LOOMY_DEFAULT_API_PORT: u16 = 57896;

fn default_true() -> bool {
    true
}

/// Loomy 网关配置（磁盘：`loomy/gateway_config.json`）。
///
/// 字段与 Qoder / Trae / WB 三条**刻意同形**（管理面前端共用一套归一化），
/// 少掉的只有「归属轴」相关项 —— Loomy 单作用域，没有区域 / 程序位可绑。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoomyGatewayConfig {
    /// 是否启用独立监听。
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 监听地址。
    #[serde(alias = "bindAddr")]
    pub bind_addr: String,
    /// 监听端口，默认 [`LOOMY_DEFAULT_API_PORT`]。
    pub port: u16,
    /// 是否允许非回环监听（默认 false）。
    #[serde(alias = "allowNonLoopback")]
    pub allow_non_loopback: bool,
    /// 请求日志保留条数。
    #[serde(alias = "logKeep")]
    pub log_keep: usize,
    /// 是否记录 prompt / response 正文。
    #[serde(alias = "logBodies")]
    pub log_bodies: bool,
    /// 请求体上限（MB；`0` 视为未设置并回落默认）。
    #[serde(alias = "maxBodyMb")]
    pub max_body_mb: usize,
    /// 未指定模型时的默认模型名（空串 ⇒ **明确报错**，不猜）。
    ///
    /// ⚠️ 与 Qoder 侧**不同**：Qoder 可以回落清单第一条，而 Loomy 的清单需要
    /// 账号凭据现拉（可能拿不到）⇒ 静默挑一个模型发出去会让用户以为「配错了」。
    #[serde(alias = "defaultModel")]
    pub default_model: String,
    /// 单轮最多换号次数。
    #[serde(alias = "maxRotate")]
    pub max_rotate: usize,
    /// 「指定账号」：优先消耗该 uid（不可用时**回落**自动择优，绝不拒绝服务）。
    #[serde(alias = "preferredUid")]
    pub preferred_uid: String,
    /// 会话粘性存活时长（毫秒；`<= 0` 由 [`crate::StickyTable`] 归一为 30 分钟）。
    #[serde(alias = "stickyTtlMs")]
    pub sticky_ttl_ms: i64,
}

impl Default for LoomyGatewayConfig {
    fn default() -> Self {
        Self {
            // ★ **默认关闭** —— 与另三条产品线同一口径（P12 D29「默认关」）：
            //   网关会把本机的上游额度暴露成接口，不声不响地默认监听是危险默认值。
            enabled: false,
            bind_addr: "127.0.0.1".to_string(),
            port: LOOMY_DEFAULT_API_PORT,
            allow_non_loopback: false,
            log_keep: 200,
            log_bodies: false,
            max_body_mb: 0,
            default_model: String::new(),
            max_rotate: 3,
            preferred_uid: String::new(),
            sticky_ttl_ms: 30 * 60 * 1000,
        }
    }
}

impl LoomyGatewayConfig {
    /// 从 `loomy/gateway_config.json` 读取；缺失 / 损坏回落默认。
    pub fn load() -> Self {
        std::fs::read_to_string(paths::gateway_config_file())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// 原子写回。
    pub fn save(&self) -> Result<(), String> {
        let file = paths::gateway_config_file();
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|error| error.to_string())?;
        core_config::atomic_write(&file, &text).map_err(|error| error.to_string())
    }

    /// 独立监听的基址。
    pub fn base_url(&self) -> String {
        format!("http://{}:{}", self.bind_addr, self.port)
    }
}

/// 模型清单缓存有效期（毫秒；10 分钟 —— 与 Qoder 侧同值）。
const CATALOG_TTL_MS: i64 = 10 * 60 * 1000;

#[derive(Debug, Clone)]
pub(crate) struct CachedCatalog {
    pub fetched_at: i64,
    pub models: Vec<Value>,
}

/// 缓存的模型清单（**带来源** —— 「拿不到」必须能与「拿到了」区分）。
#[derive(Debug, Clone, Default)]
pub struct CatalogView {
    pub models: Vec<Value>,
    /// `live`（最近一次拉取成功 / 10 分钟内缓存命中）/ `cached`（本次失败，退回上次成功）
    /// / `unavailable`（失败且无缓存）。
    pub source: &'static str,
    pub fetched_at: Option<i64>,
    /// 取数说明（失败 / 无账号时的可读原因；`live` 为 `None`）。
    pub note: Option<String>,
}

impl LoomyGatewayState {
    /// 取模型清单（带缓存；无账号 / 拉取失败时给**可读 note**，绝不假装有数据）。
    pub async fn catalog_for(&self) -> CatalogView {
        {
            let cache = self.catalog.read().await;
            if let Some(cached) = cache.as_ref() {
                if core_config::now_ms() - cached.fetched_at < CATALOG_TTL_MS
                    && !cached.models.is_empty()
                {
                    return CatalogView {
                        models: cached.models.clone(),
                        source: "live",
                        fetched_at: Some(cached.fetched_at),
                        note: None,
                    };
                }
            }
        }
        self.fetch_catalog().await
    }

    /// 强制重拉（管理面「重新识别」；忽略 TTL 缓存）。
    ///
    /// 失败时仍退回上次成功的结果（`source = "cached"` + note），**不**清空已有清单。
    pub async fn refresh_catalog(&self) -> CatalogView {
        self.fetch_catalog().await
    }

    /// 只读内存里的清单缓存（**不触发任何上游往返**）—— 限流探测的模型轴用它。
    pub async fn cached_catalog_models(&self) -> Vec<Value> {
        self.catalog
            .read()
            .await
            .as_ref()
            .map(|cached| cached.models.clone())
            .unwrap_or_default()
    }

    async fn fetch_catalog(&self) -> CatalogView {
        let token = buddy_switch_core::modules::loomy::account::load_accounts_for(LoomyRegion::Cn)
            .accounts
            .into_iter()
            .find(|account| !account.access_token.is_empty())
            .map(|account| account.access_token);
        let Some(token) = token else {
            let note =
                "没有可用账号：模型清单需要账号凭据向官方拉取（去「账号」页导入）".to_string();
            let cache = self.catalog.read().await;
            return match cache.as_ref() {
                Some(cached) if !cached.models.is_empty() => CatalogView {
                    models: cached.models.clone(),
                    source: "cached",
                    fetched_at: Some(cached.fetched_at),
                    note: Some(note),
                },
                _ => CatalogView {
                    models: Vec::new(),
                    source: "unavailable",
                    fetched_at: None,
                    note: Some(note),
                },
            };
        };
        let base = match &self.upstream_override {
            Some(host) => host.clone(),
            None => realm_spec(LoomyRegion::Cn).api_base.to_string(),
        };
        match buddy_switch_core::modules::loomy::api::fetch_models_at(&base, &token).await {
            Ok(models) => {
                let fetched_at = core_config::now_ms();
                {
                    let mut cache = self.catalog.write().await;
                    *cache = Some(CachedCatalog {
                        fetched_at,
                        models: models.clone(),
                    });
                }
                CatalogView {
                    models,
                    source: "live",
                    fetched_at: Some(fetched_at),
                    note: None,
                }
            }
            Err(error) => {
                let note = Some(format!("模型清单拉取失败：{error}"));
                let cache = self.catalog.read().await;
                match cache.as_ref() {
                    Some(cached) if !cached.models.is_empty() => CatalogView {
                        models: cached.models.clone(),
                        source: "cached",
                        fetched_at: Some(cached.fetched_at),
                        note,
                    },
                    _ => CatalogView {
                        models: Vec::new(),
                        source: "unavailable",
                        fetched_at: None,
                        note,
                    },
                }
            }
        }
    }
}

/// `realm_spec` 在这里只用于上游基址，单独引入以免与本模块其它同名项冲突。
use buddy_switch_core::modules::loomy::region::realm_spec;

/// 状态视图里**由运行时现算**的那部分（拆出来是为了让键集合护栏能测纯函数）。
#[derive(Debug, Clone, Default)]
pub struct StatusParts {
    pub running: bool,
    pub addr: Option<String>,
    pub version: String,
    pub total_requests: u64,
    pub last_error: Option<String>,
    pub api_key_prefix: String,
    pub pool: Value,
    pub accounts: Vec<Value>,
    pub diagnose: Vec<String>,
}

/// 组装状态视图（**纯函数**）。
///
/// ## 形状与 Qoder 侧**逐键同款**
///
/// 管理面前端复用同一套归一化（`src/lib/loomy-gateway.ts`），少一个键就会静默
/// 退化成默认值。键集合被 `status_view_keys_are_pinned` 钉住。
///
/// ## 唯一少掉的键：`region`
///
/// Loomy 单作用域 ⇒ 回显区域只会让人以为「有两本账号库」（Qoder 侧不回显区域的
/// 理由之一在这里更强：那边至少真的有两个区域）。
pub fn status_view_value(config: &LoomyGatewayConfig, parts: StatusParts) -> Value {
    json!({
        "enabled": config.enabled,
        "running": parts.running,
        "addr": parts.addr,
        "base_url": config.base_url(),
        "bind_addr": config.bind_addr,
        "port": config.port,
        "allow_non_loopback": config.allow_non_loopback,
        "version": parts.version,
        "total_requests": parts.total_requests,
        "last_error": parts.last_error,
        "api_key_prefix": parts.api_key_prefix,
        "pool": parts.pool,
        "accounts": parts.accounts,
        "diagnose": parts.diagnose,
        // 上游基址（管理面要写出「请求实际打到哪」）。
        "upstream": buddy_switch_core::modules::loomy::region::realm_spec(LoomyRegion::Cn).api_base,
    })
}

/// 管理面模型清单（**未过滤**禁用 + 逐条 `disabled` 标记 + 来源三级）—— 两宿主共用。
///
/// 与 [`status_view`] 同理：形状只有一处，宿主不拼装。
pub fn admin_models_response(
    catalog: &CatalogView,
    disabled: &std::collections::BTreeSet<String>,
) -> Value {
    let data: Vec<Value> = catalog
        .models
        .iter()
        .map(|entry| {
            let id = entry.get("id").and_then(Value::as_str).unwrap_or("");
            json!({
                "id": id,
                "object": "model",
                "owned_by": "loomy",
                "display_name": entry
                    .get("display_name")
                    .cloned()
                    .unwrap_or_else(|| Value::String(id.to_string())),
                "context_length": entry.get("context_length").cloned(),
                "supports_image": entry.get("supports_image").cloned(),
                "disabled": crate::disabled_models::is_disabled_in(disabled, id),
            })
        })
        .collect();
    json!({
        "object": "list",
        "source": catalog.source,
        "fetchedAt": catalog.fetched_at,
        "note": catalog.note,
        "data": data,
    })
}

/// 组装管理面状态视图（Tauri 命令与 webui 路由**共用**）。
pub async fn status_view(
    state: &LoomyGatewayState,
    running: bool,
    addr: Option<String>,
    version: &str,
) -> Value {
    let now = core_config::now_ms();
    let config = state.config_snapshot().await;

    let (pool, accounts, diagnose) = {
        let mut pools = state.pools.lock().await;
        let pool = pools.entry(LoomyRegion::Cn).or_default();
        pool.sync_for(LoomyRegion::Cn);
        (
            pool.summary(now),
            pool.accounts_view(now),
            // Loomy 池**没有** `diagnose`（Qoder 那份带逐模型冷却表，这里没有该轴）⇒
            // 逐账号给「为什么不能路由」的等价信息，由池视图的 `status` 字段承载。
            Vec::new(),
        )
    };

    let api_key_prefix = state
        .key_store
        .list()
        .into_iter()
        .filter(|record| !record["revoked"].as_bool().unwrap_or(false))
        .max_by_key(|record| record["createdAt"].as_i64().unwrap_or(0))
        .and_then(|record| record["prefix"].as_str().map(str::to_string))
        .unwrap_or_default();

    status_view_value(
        &config,
        StatusParts {
            running,
            addr,
            version: version.to_string(),
            total_requests: state.total_requests.load(std::sync::atomic::Ordering::Relaxed),
            last_error: state.last_error.read().await.clone(),
            api_key_prefix,
            pool,
            accounts,
            diagnose,
        },
    )
}

/// 组装 Loomy 网关路由（**不含 fallback**）。
pub fn router(state: LoomyGatewayState) -> axum::Router {
    use axum::routing::{get, post};
    let body_limit = state.body_limit_bytes;
    axum::Router::new()
        .merge(crate::health_routes(routes::health))
        .route("/status", get(routes::status))
        .route("/v1/models", get(routes::models))
        .route("/v1/chat/completions", post(routes::chat_completions))
        .route("/v1/messages", post(routes::messages))
        .route("/v1/responses", post(routes::responses))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            routes::bearer_auth,
        ))
        .layer(axum::extract::DefaultBodyLimit::max(body_limit))
        .with_state(state)
}

/// 按配置启动独立监听（只回环，除非显式允许；端口占用给可读错误）。
pub async fn spawn_listener(state: LoomyGatewayState) -> anyhow::Result<crate::GatewayHandle> {
    let config = state.config_snapshot().await;
    let bind_addr = config.bind_addr.clone();
    let port = config.port;

    let ip: IpAddr = bind_addr.parse().map_err(|_| {
        anyhow::anyhow!("Loomy 网关监听地址无效：{bind_addr}（应为 IPv4/IPv6 字面量）")
    })?;
    if !ip.is_loopback() && !config.allow_non_loopback {
        return Err(anyhow::anyhow!(
            "已拒绝监听非回环地址 {bind_addr}：局域网内任何设备都可消耗你的 Loomy 额度，\
             请确认可信网络后，在设置中显式开启『允许局域网访问』再重试"
        ));
    }
    let addr = SocketAddr::new(ip, port);
    let listener = TcpListener::bind(addr).await.map_err(|error| {
        anyhow::anyhow!(
            "无法监听 {addr}：端口可能被占用或不可用（{error}）。请在设置中改用其他端口。"
        )
    })?;
    let local = listener.local_addr().unwrap_or(addr);
    let app = router(state);
    let (tx, rx) = oneshot::channel::<()>();
    let join = tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = rx.await;
            })
            .await;
    });
    Ok(crate::GatewayHandle::new(local, tx, join))
}

/// Loomy 网关产品线（[`crate::GatewayProduct`] 实现）。
pub struct LoomyGateway;

static LOOMY_SHARED_STATE: std::sync::OnceLock<LoomyGatewayState> = std::sync::OnceLock::new();

impl crate::GatewayProduct for LoomyGateway {
    type State = LoomyGatewayState;

    const OWNED_PATHS: &'static [&'static str] = &[
        "/health",
        "/healthz",
        "/status",
        "/v1/models",
        "/v1/chat/completions",
        "/v1/messages",
        "/v1/responses",
    ];

    fn shared_state() -> LoomyGatewayState {
        LOOMY_SHARED_STATE
            .get_or_init(|| LoomyGatewayState::new(LoomyGatewayConfig::load()))
            .clone()
    }

    fn is_enabled(state: &LoomyGatewayState) -> impl std::future::Future<Output = bool> + Send {
        let state = state.clone();
        async move { state.config_snapshot().await.enabled }
    }

    fn router(state: LoomyGatewayState) -> axum::Router {
        self::router(state)
    }

    fn spawn_listener(
        state: LoomyGatewayState,
    ) -> impl std::future::Future<Output = anyhow::Result<crate::GatewayHandle>> + Send {
        self::spawn_listener(state)
    }
}

/// 渠道日志文件（供管理命令透出）。
pub fn log_file() -> PathBuf {
    paths::logs_dir().join("gateway_requests.jsonl")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_uses_dedicated_port() {
        let config = LoomyGatewayConfig::default();
        assert_eq!(config.port, 57896, "Loomy 网关默认端口应为 57896");
        for taken in [
            57890u16,                                                // 界面本体
            57891,                                                   // WorkBuddy 网关
            57892,                                                   // 统一网关
            57893,                                                   // 客户端会话监听 hook
            buddy_switch_core::modules::trae::TRAE_DEFAULT_API_PORT, // Trae 网关
            crate::qoder::QODER_DEFAULT_API_PORT,                    // Qoder 网关
            8899,                                                    // Trae 登录代理
        ] {
            assert_ne!(config.port, taken, "不得与既有端口 {taken} 撞车");
        }
        assert_eq!(config.base_url(), "http://127.0.0.1:57896");
        assert!(config.default_model.is_empty());
        // 变异：改成 true ⇒ 本条红。
        assert!(
            !config.enabled,
            "网关默认必须关闭 —— 不声不响地默认监听是危险默认值"
        );
        assert!(!config.allow_non_loopback);
        assert_eq!(config.log_keep, 200, "与另三条产品线同值");
    }

    /// 路径所有权必须与既有产品线**重叠**（独立端口的前提）。
    #[test]
    fn owned_paths_overlap_with_other_products() {
        use crate::GatewayProduct;
        let mine = LoomyGateway::OWNED_PATHS;
        let qoder = crate::qoder::QoderGateway::OWNED_PATHS;
        for path in ["/v1/models", "/v1/chat/completions"] {
            assert!(mine.contains(&path), "Loomy 应拥有 {path}");
            assert!(
                qoder.contains(&path),
                "与 Qoder 的重叠是本产品线必须独立端口的依据：{path}"
            );
        }
    }

    /// ★ 状态视图的键集合被**钉住**（增删键要同时改前端归一化）。
    #[test]
    fn status_view_keys_are_pinned() {
        let config = LoomyGatewayConfig::default();
        let value = status_view_value(
            &config,
            StatusParts {
                version: "0.0.0-test".to_string(),
                ..StatusParts::default()
            },
        );
        let expected: std::collections::BTreeSet<&str> = [
            "enabled",
            "running",
            "addr",
            "base_url",
            "bind_addr",
            "port",
            "allow_non_loopback",
            "version",
            "total_requests",
            "last_error",
            "api_key_prefix",
            "pool",
            "accounts",
            "diagnose",
            "upstream",
        ]
        .into_iter()
        .collect();
        let keys: std::collections::BTreeSet<&str> = value
            .as_object()
            .expect("status view 是对象")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, expected, "status key set must be pinned");
        assert!(!value.as_object().unwrap().contains_key("baseUrl"));
        // 单作用域 ⇒ 区域**不回显**（回显会造出「两本账号库」的错觉）。
        assert!(!value.as_object().unwrap().contains_key("region"));
        assert!(
            value["upstream"].as_str().unwrap().contains("loomy"),
            "upstream 应给出上游基址：{}",
            value["upstream"]
        );
    }
}
