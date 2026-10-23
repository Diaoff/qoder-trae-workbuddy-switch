//! webui 管理面的**密码鉴权**（公网部署的最后一道防线）。
//!
//! ## 它保护什么、不保护什么
//!
//! | 范围 | 是否受保护 | 理由 |
//! |:---|:---|:---|
//! | `/api/*`（管理面：账号库、网关 Key、签到、备份） | ✅ | 正是风险面 |
//! | `/v1/*` 与网关独立端口 | ❌ | 已有 `sk-wb-` / `sk-trae-` / `sk-qoder-` / `sk-loomy-` / `sk-uni-` 自鉴权，客户端程序靠它调用 |
//! | 静态资源（`/`、`/assets/*`） | ❌ | 打包产物不含密钥；拦了会让 `index.html` 401 ⇒ 登录页永远进不去 |
//! | `/health`、`/healthz` | ❌ | 容器 healthcheck 必须免鉴权（见 `docs/guides/DOCKER.md`） |
//!
//! 保护边界**由结构保证**，不靠自觉：中间件只 `.layer()` 在 [`crate::api::protected_routes`]
//! 上，而网关路由是 `router()` 里 `merge` 进来的**另一棵树** ⇒ 天然不受影响。
//!
//! ## 为什么是「服务端 session + HttpOnly Cookie」而不是 Bearer token
//!
//! token 放 `localStorage` 会被任何一次 XSS 直接读走；`HttpOnly` 的 Cookie 读不到。
//! 代价是必须防 CSRF —— 本仓的组合已经足够：`SameSite=Strict` + 全站同源（**没有 CORS 层**）
//! + 写操作一律 `POST` + `Content-Type: application/json`（跨站表单发不出这个 MIME，
//! 发得出就得走预检，而预检被同源策略挡下）。
//!
//! ## 会话**不落盘**
//!
//! 进程重启 = 所有登录态失效。这不是偷懒：落盘就得处理「服务被复制到另一台机器」
//! 「备份文件里带着活 token」这类问题，而重启踢人的代价只是重新输一次密码。
//!
//! ## 密码**只存哈希**
//!
//! `webui_auth.json` 里是 `{ salt, hash, updatedAt }`，`hash = sha256(salt || password)`。
//! 明文永不落盘 ⇒ 这个文件可以安全进备份（与 `apikey.rs` 的 `sha256` 范式一致）。
//! ⚠️ 单管理密码 + 非批量撞库场景下加盐 sha256 够用；它**不是**抗 GPU 的 KDF。
//! 若要上 argon2 需另评估（本仓刻意不为此引入新依赖）。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// 首次启动时用这个环境变量**初始化**密码（写入哈希后即被忽略）。
///
/// 之后改密码走 webui 的「设置 → WebUI 密码」，不必改 compose 重启。
pub const PASSWORD_ENV: &str = "BUDDY_SWITCH_PASSWORD";

/// 逃生开关：**只**在启动时由环境变量读取，**绝不**落盘、**绝不**暴露成 API。
///
/// ## 它解决的是「Docker 端口映射对进程不可见」这个信息缺口
///
/// `bind_addr` 是进程唯一能感知的信号，但容器里必须绑 `0.0.0.0`（否则 docker-proxy
/// 连不上），而宿主侧可能用 `-p 127.0.0.1:57890:57890` 只映射到回环 —— 此时
/// 「我在对外裸奔」是**假阳性**。三个合法用法：
///
/// 1. 宿主机反代（Caddy/Nginx）已做 Basic Auth / mTLS / OIDC，反代到 `127.0.0.1:57890`；
/// 2. 容器只在自定义 bridge 网络内暴露，宿主**不映射任何端口**；
/// 3. 升级救急：拉了新镜像忘了配密码，先用它恢复可用性，再补密码。
///
/// ⚠️ **公网直连（`-p 57890:57890` + 公网 IP）禁用本开关** —— 那是真裸奔。
/// 之所以不能做成「界面上的开关」：已登录的攻击者（或任何能写数据目录的途径）
/// 打开它、重启后就永久免鉴权，等于把「要不要鉴权」交给被鉴权保护的那个界面决定。
pub const ALLOW_UNAUTHENTICATED_ENV: &str = "BUDDY_SWITCH_ALLOW_UNAUTHENTICATED";

/// 会话 Cookie 名。
const COOKIE_NAME: &str = "bs_session";

/// 会话存活时长（12 小时，**滑动续期**：每次有效请求都会推后）。
const SESSION_TTL_MS: i64 = 12 * 60 * 60 * 1000;

/// 连续失败多少次后锁定。
const MAX_FAILURES: u32 = 8;

/// 锁定时长（10 分钟）。
///
/// ⚠️ 计数是**全局**而非按 IP：反代后面所有请求的 peer 都是反代地址，按 IP 分桶毫无意义。
/// 代价是攻击者可以故意连错 8 次把管理员锁在外面 10 分钟（有界 DoS）——
/// 与「无限次暴力破解」相比，这个取舍是值得的。
const LOCKOUT_MS: i64 = 10 * 60 * 1000;

/// 密码文件相对 store 的文件名。
const CREDENTIAL_FILE: &str = "webui_auth.json";

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 密码文件形状（**只存哈希**，无明文）。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Credential {
    /// 随机盐（16 字节 hex）。
    salt: String,
    /// `sha256(salt_bytes || password_bytes)` 的 hex。
    hash: String,
    /// 最后修改时间（毫秒），仅用于界面展示。
    #[serde(rename = "updatedAt")]
    updated_at: i64,
}

impl Credential {
    fn new(password: &str) -> Self {
        let salt = random_hex(16);
        let hash = hash_password(&salt, password);
        Self {
            salt,
            hash,
            updated_at: now_ms(),
        }
    }
}

/// `sha256(salt || password)` → hex。
fn hash_password(salt: &str, password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update(password.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// 常量时间字符串比较（长度不同直接 false）。
///
/// 与 `buddy-switch-gateway::apikey` 里那份同款、但**刻意各写一份**：那个是
/// `pub(crate)`（跨 crate 不可见），而「鉴权比较必须常量时间」这条属性在本仓要
/// 在两个 crate 各自成立。任一处被改成短路比较，另一处不会替它兜底 —— 因此
/// 两处都有**独立的可证伪单测**（见本模块 `constant_time_eq_*`）。
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// 取 `n` 字节的密码学随机数 → hex。
///
/// ## 失败时返回**全零**，并且这是刻意的
///
/// `getrandom` 在 Linux / macOS / Windows 上不会失败（失败意味着内核 RNG 不可用）。
/// 真走到那条路时只有两种选择：
///
/// - **panic** —— 把「内核 RNG 坏了」升级成「每个请求都 500」，且 panic 发生在
///   async handler 里会变成断连，排查成本远高于收益；
/// - **返回全零 + 打一条 stderr** —— 会话 token 可预测（严重），但**进程仍在**，
///   运维能看到那条告警并换机器。
///
/// 选后者，因为这条路径实际上不可达；而一个「因为不可能发生就 panic」的分支，
/// 一旦真的发生，会把「能用但危险」变成「完全不能用」，掩盖掉真正的原因。
/// 告警文案必须说清后果（可预测），不能只说「随机源异常」。
fn random_hex(n: usize) -> String {
    let mut buffer = vec![0u8; n];
    if getrandom::getrandom(&mut buffer).is_err() {
        eprintln!("[auth] ⚠️ 系统随机源不可用 —— 会话 token 的不可预测性无法保证");
    }
    buffer.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// 进程内共享的鉴权状态。
pub struct WebuiAuth {
    inner: Mutex<Inner>,
    /// 是否把密码哈希落盘。
    ///
    /// 生产恒为 `true`；测试实例为 `false` —— 否则每个用例都会往临时 home 写
    /// `webui_auth.json`，并让「改密码」用例之间通过磁盘互相污染。
    persist: bool,
}

struct Inner {
    /// `None` = **未设密码**（免密模式，见 [`Self::is_enabled`]）。
    credential: Option<Credential>,
    /// token → 过期时间（毫秒）。**不落盘**。
    sessions: HashMap<String, i64>,
    /// 连续失败次数（成功即清零）。
    failures: u32,
    /// 锁定截止时间（毫秒；`0` = 未锁定）。
    locked_until: i64,
}

static SHARED: OnceLock<std::sync::Arc<WebuiAuth>> = OnceLock::new();

/// 进程内共享实例（首次调用时**读盘 + 按需用环境变量初始化**）。
///
/// 返回 `Arc` 而非 `&'static`：中间件与处理器都通过 `State` 拿它，
/// 而 `from_fn_with_state` 要求 state 可 `Clone`。顺带让测试可以注入**独立实例**
/// （见 [`WebuiAuth::for_test`]），不必与进程级单例抢同一个 `OnceLock`。
pub fn shared() -> std::sync::Arc<WebuiAuth> {
    SHARED
        .get_or_init(|| std::sync::Arc::new(WebuiAuth::load()))
        .clone()
}

impl WebuiAuth {
    /// 测试用：一个**内存态、未设密码**的独立实例（不读盘、不落盘）。
    ///
    /// 生产路径只有 [`shared`]；`for_test` 存在的原因是 `shared()` 是进程级 `OnceLock`，
    /// 一个用例把密码设上后，同进程内其余用例会集体变红且顺序相关。
    #[cfg(test)]
    pub fn for_test() -> Self {
        Self {
            inner: Mutex::new(Inner {
                credential: None,
                sessions: HashMap::new(),
                failures: 0,
                locked_until: 0,
            }),
            persist: false,
        }
    }

    /// 读盘；文件不存在时尝试用 [`PASSWORD_ENV`] 初始化。
    fn load() -> Self {
        let credential = read_credential().or_else(|| {
            // 首次启动：env 给了密码就落盘成哈希。**读完即从进程环境里抹掉**，
            // 避免它出现在后续可能被转储的 `environ` / 子进程环境里。
            let raw = std::env::var(PASSWORD_ENV).ok()?;
            let password = raw.trim().to_string();
            if password.is_empty() {
                return None;
            }
            let credential = Credential::new(&password);
            if let Err(error) = write_credential(&credential) {
                // ⚠️ 这条告警必须说清**后果**，不能只说「写不了」。
                //
                // 落盘失败 = 密码只活在这次进程的内存里 ⇒ 重启后密码**消失**，
                // 于是「非回环 + 无密码」会在下次启动时直接拒绝启动（或退回免密，
                // 若当时是回环）。用户看到「本次仍按该密码鉴权」会以为已经设好了，
                // 重启才发现要么起不来、要么密码不对 —— 那才是最难排查的形态。
                //
                // 最常见成因：bind mount 的宿主目录属主是 root，容器里 uid 10001 写不进去
                //（见 `docs/guides/DOCKER.md` 的「数据持久化」）。用命名卷则无此问题。
                eprintln!(
                    "[auth] ⚠️ 密码无法写入 {}（{error}）—— \
                     本次运行仍认这个密码，但**重启后会丢失**：届时若无密码且监听非回环，\
                     进程会拒绝启动。请修好数据目录属主（或改用命名卷）后重启一次。",
                    credential_path().display()
                );
            }
            // 从环境里移除：`std::env::remove_var` 在 Rust 2021 里是安全的（无并发读）。
            std::env::remove_var(PASSWORD_ENV);
            Some(credential)
        });
        Self {
            inner: Mutex::new(Inner {
                credential,
                sessions: HashMap::new(),
                failures: 0,
                locked_until: 0,
            }),
            persist: true,
        }
    }

    /// 是否**已启用**鉴权（= 设过密码）。
    ///
    /// ⚠️ 与「是否强制要求」是两件事：本函数只回答「有没有密码」。
    /// 「非回环 + 无密码是否拒绝启动」由 [`startup_refusal`] 决定。
    pub fn is_enabled(&self) -> bool {
        self.lock().credential.is_some()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        // 中毒（某次 panic 时持锁）不 recover 成「放行」：`into_inner` 拿到的状态
        // 仍然是完整的，鉴权判定继续有效，比 panic 传播到每个请求更可用。
        self.inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// 校验密码（**含锁定判定**）。成功会清零失败计数。
    ///
    /// `pub` 而非私有：改密码接口（`api.rs::api_change_password`）要验旧密码，
    /// 而它不该自己再写一遍「比对哈希 + 计数」的逻辑（鉴权判定必须只定义一次）。
    pub fn check_password(&self, password: &str) -> bool {
        matches!(self.verify(password), VerifyOutcome::Ok)
    }

    fn verify(&self, password: &str) -> VerifyOutcome {
        let mut inner = self.lock();
        let now = now_ms();
        if inner.locked_until > now {
            return VerifyOutcome::Locked(inner.locked_until - now);
        }
        let Some(credential) = inner.credential.clone() else {
            // 免密模式下没有「登录」这回事；调用点不该走到这里。
            return VerifyOutcome::Wrong;
        };
        let expected = credential.hash.as_bytes();
        let presented = hash_password(&credential.salt, password);
        if constant_time_eq(expected, presented.as_bytes()) {
            inner.failures = 0;
            inner.locked_until = 0;
            VerifyOutcome::Ok
        } else {
            inner.failures = inner.failures.saturating_add(1);
            if inner.failures >= MAX_FAILURES {
                inner.failures = 0;
                inner.locked_until = now + LOCKOUT_MS;
                VerifyOutcome::Locked(LOCKOUT_MS)
            } else {
                VerifyOutcome::Wrong
            }
        }
    }

    /// 签发一个新会话，返回 token。
    fn issue_session(&self) -> String {
        let token = random_hex(32);
        let mut inner = self.lock();
        inner
            .sessions
            .insert(token.clone(), now_ms() + SESSION_TTL_MS);
        token
    }

    /// 校验会话 token；有效则**滑动续期**并返回 `true`。
    fn validate_session(&self, token: &str) -> bool {
        let mut inner = self.lock();
        let now = now_ms();
        match inner.sessions.get(token).copied() {
            Some(expires_at) if expires_at > now => {
                inner
                    .sessions
                    .insert(token.to_string(), now + SESSION_TTL_MS);
                true
            }
            Some(_) => {
                inner.sessions.remove(token);
                false
            }
            None => false,
        }
    }

    /// 撤销单条会话（登出）。
    fn drop_session(&self, token: &str) {
        self.lock().sessions.remove(token);
    }

    /// 撤销**全部**会话（改密码时用：旧登录态必须立刻作废）。
    fn revoke_all_sessions(&self) {
        self.lock().sessions.clear();
    }

    /// 设置 / 修改密码并落盘，随后撤销全部会话。
    pub fn set_password(&self, password: &str) -> Result<(), String> {
        let credential = Credential::new(password);
        if self.persist {
            write_credential(&credential)?;
        }
        {
            let mut inner = self.lock();
            inner.credential = Some(credential);
            inner.failures = 0;
            inner.locked_until = 0;
        }
        self.revoke_all_sessions();
        Ok(())
    }
}

enum VerifyOutcome {
    Ok,
    Wrong,
    /// 已锁定，附带剩余毫秒。
    Locked(i64),
}

fn credential_path() -> std::path::PathBuf {
    buddy_switch_core::modules::config::store_dir().join(CREDENTIAL_FILE)
}

fn read_credential() -> Option<Credential> {
    let text = std::fs::read_to_string(credential_path()).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_credential(credential: &Credential) -> Result<(), String> {
    let path = credential_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let text = serde_json::to_string_pretty(credential).map_err(|error| error.to_string())?;
    buddy_switch_core::modules::config::atomic_write(&path, &text)
        .map_err(|error| error.to_string())
}

// ---------------------------------------------------------------------------
// 启动护栏
// ---------------------------------------------------------------------------

/// 启动决策：**拒绝启动** / 放行。
#[derive(Debug, PartialEq, Eq)]
pub enum StartupDecision {
    /// 放行；`warn` 为需要在启动横幅里打出的告警（`None` = 无需提示）。
    Allow { warn: Option<String> },
    /// 拒绝启动，附可读理由。
    Refuse(String),
}

/// 纯决策：非回环监听 + 未设密码 + 未显式豁免 ⇒ 拒绝启动。
///
/// ## 为什么是 fail-closed
///
/// 这个仓库对「以为在安全位置、实际不是」的场景一律拒绝启动（对照
/// `refuse_when_home_override_is_invalid`）。`BUDDY_SWITCH_HOST=0.0.0.0` 而
/// `/api/*` 全裸，正是这种场景：唯一的提示是一行可能被 `| head` 吞掉的 stderr 警告。
///
/// ## 入参即取值（可并行单测）
///
/// 不直接读 `std::env`：环境变量是进程级全局状态，并行测试会互相踩。
pub fn startup_refusal(
    host_is_loopback: bool,
    password_configured: bool,
    allow_unauthenticated: bool,
) -> StartupDecision {
    if password_configured {
        return StartupDecision::Allow { warn: None };
    }
    if host_is_loopback {
        // 本机 / 桌面形态：零摩擦，行为与加鉴权之前**完全一致**。
        return StartupDecision::Allow { warn: None };
    }
    if allow_unauthenticated {
        return StartupDecision::Allow {
            warn: Some(format!(
                "⚠️  webui 正对外监听且**未设密码**（已由 {ALLOW_UNAUTHENTICATED_ENV} 显式豁免）\
                 —— 任何能连到本端口的人都能读写账号库与网关 Key。\
                 仅当已有反代鉴权或容器不对外映射端口时才可接受；公网直连请立即停用该变量并设置 {PASSWORD_ENV}。"
            )),
        };
    }
    StartupDecision::Refuse(format!(
        "webui 正对外监听（非回环地址）却**没有设置密码** —— 任何能连到本端口的人\
         都能读写账号库与网关 Key。请二选一：\n\
         · 设置环境变量 {PASSWORD_ENV}=<你的密码>（首次启动写入哈希，之后可在 webui 修改）；\n\
         · 若已有反代鉴权 / 容器不对外映射端口，显式设置 {ALLOW_UNAUTHENTICATED_ENV}=1 承认这一风险。"
    ))
}

/// 环境变量值是否算「真」（`1` / `true` / `yes` / `on`，大小写不敏感）。
fn env_flag(value: Option<&str>) -> bool {
    matches!(
        value.map(str::trim).map(str::to_ascii_lowercase).as_deref(),
        Some("1") | Some("true") | Some("yes") | Some("on")
    )
}

/// 从环境变量读取逃生开关（**唯一**读取点）。
pub fn allow_unauthenticated_from_env() -> bool {
    env_flag(std::env::var(ALLOW_UNAUTHENTICATED_ENV).ok().as_deref())
}

// ---------------------------------------------------------------------------
// HTTP：鉴权路由（**公开**，不受中间件保护）
// ---------------------------------------------------------------------------

/// 三条公开鉴权路由。
///
/// ⚠️ 这三条**绝不能**进 `src/lib/api.ts` 的 `ROUTES` 表：那张表要求每个命令都在
/// `src-tauri/src/lib.rs` 的 `invoke_handler` 登记，而登录对桌面端是无意义的概念
/// （桌面端走 `invoke`，不经过 HTTP）。误加会让 `check-api-contract.cjs` 变红，
/// 且桌面端报 `command not found`。前端因此单开 `src/lib/webui-auth.ts` 自带 `fetch`。
pub fn routes(state: std::sync::Arc<WebuiAuth>) -> Router {
    Router::new()
        .route("/api/auth/state", get(api_auth_state))
        .route("/api/auth/login", post(api_auth_login))
        .route("/api/auth/logout", post(api_auth_logout))
        .with_state(state)
}

fn json_ok(value: Value) -> Response {
    Json(value).into_response()
}

fn json_err(error: impl Into<String>, code: StatusCode) -> Response {
    (code, Json(json!({ "ok": false, "error": error.into() }))).into_response()
}

/// `GET /api/auth/state` —— 前端启动时的**第一个**请求，决定渲染登录页还是应用。
///
/// 只泄露「有没有设密码」，不泄露任何关于密码本身的信息。
async fn api_auth_state(
    State(auth): State<std::sync::Arc<WebuiAuth>>,
    headers: HeaderMap,
) -> Response {
    let enabled = auth.is_enabled();
    let authed =
        !enabled || cookie_token(&headers).is_some_and(|token| auth.validate_session(&token));
    json_ok(json!({
        "ok": true,
        "authRequired": enabled,
        "authed": authed,
    }))
}

/// `POST /api/auth/login` —— `{ "password": "…" }` → `Set-Cookie`。
async fn api_auth_login(
    State(auth): State<std::sync::Arc<WebuiAuth>>,
    Json(body): Json<Value>,
) -> Response {
    if !auth.is_enabled() {
        // 免密模式：没有密码可校验。**不**签发会话（否则会给前端一个「已登录」的假象）。
        return json_ok(json!({ "ok": true, "authRequired": false, "authed": true }));
    }
    let password = body.get("password").and_then(Value::as_str).unwrap_or("");
    if password.is_empty() {
        return json_err("请输入密码", StatusCode::BAD_REQUEST);
    }
    match auth.verify(password) {
        VerifyOutcome::Ok => {
            let token = auth.issue_session();
            let mut response = json_ok(json!({ "ok": true, "authed": true }));
            response.headers_mut().insert(
                header::SET_COOKIE,
                HeaderValue::from_str(&session_cookie(&token))
                    .unwrap_or_else(|_| HeaderValue::from_static("bs_session=; Path=/; Max-Age=0")),
            );
            response
        }
        VerifyOutcome::Wrong => json_err("密码不正确", StatusCode::UNAUTHORIZED),
        VerifyOutcome::Locked(remaining_ms) => {
            let minutes = (remaining_ms / 60_000).max(1);
            json_err(
                format!("尝试次数过多，请 {minutes} 分钟后再试"),
                StatusCode::TOO_MANY_REQUESTS,
            )
        }
    }
}

/// `POST /api/auth/logout` —— 清服务端会话 + 让浏览器丢弃 Cookie。
///
/// **公开**（不要求已登录）：否则会话已过期时前端连「登出」都调不动，
/// 只能手动清 Cookie —— 那正是最需要登出的场景。
async fn api_auth_logout(
    State(auth): State<std::sync::Arc<WebuiAuth>>,
    headers: HeaderMap,
) -> Response {
    if let Some(token) = cookie_token(&headers) {
        // 只删自己那一条：`revoke_all_sessions` 是改密码语义（踢掉所有人）。
        auth.drop_session(&token);
    }
    let mut response = json_ok(json!({ "ok": true, "authed": false }));
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_static("bs_session=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0"),
    );
    response
}

/// 构造会话 Cookie。
///
/// ## 三个属性为什么是这几个值
///
/// - **`HttpOnly`** —— XSS 读不到 token（这正是选 Cookie 而非 `localStorage` 的全部收益）。
/// - **`SameSite=Strict`** —— 跨站请求一律不带 Cookie。代价是从外部链接直接打开某个
///   深链时，首个请求不带 Cookie（会先落登录页）⇒ 前端用 `?next=` 回跳补偿。
///   ⚠️ 已确认不打断任何既有登录链路：本仓的 OAuth 全是「起流程 + 前端轮询 +
///   **手工粘回**回调链接」，服务端**没有**第三方跳回来的回调端点。
/// - **`Secure`** —— **刻意不加**。纯 HTTP 部署下带 `Secure` 的 Cookie 会被浏览器
///   直接丢弃 ⇒ 永远登录不上，且现象是「密码明明对，就是进不去」，极难排查。
///   真正的加密要靠前置 TLS 反代（见 `docs/guides/DOCKER.md` 的安全段）。
fn session_cookie(token: &str) -> String {
    format!("{COOKIE_NAME}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age=43200")
}

/// 从 `Cookie` 头里取会话 token（只认 `bs_session`）。
fn cookie_token(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in raw.split(';') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix(&format!("{COOKIE_NAME}=")) {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// HTTP：保护中间件
// ---------------------------------------------------------------------------

/// 管理面鉴权中间件。
///
/// 未设密码 ⇒ **直接放行**（本机 / 桌面形态行为不变）。
/// 已设密码 ⇒ 必须带有效会话 Cookie，否则 `401`。
///
/// 401 的响应体形状与 `api.rs::json_err` **逐字一致**（`{ ok: false, error }`），
/// 因此前端 `httpCall` 的 `!res.ok` 分支能直接读到 `message`，不必为鉴权单开一条错误路径。
pub async fn require_session(
    State(auth): State<std::sync::Arc<WebuiAuth>>,
    request: Request,
    next: Next,
) -> Response {
    if !auth.is_enabled() {
        return next.run(request).await;
    }
    if let Some(token) = cookie_token(request.headers()) {
        if auth.validate_session(&token) {
            return next.run(request).await;
        }
    }
    json_err("未登录或登录已过期，请重新登录", StatusCode::UNAUTHORIZED)
}

// ---------------------------------------------------------------------------
// 单测
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 常量时间比较必须**长度不同即 false**，且相同内容为 true。
    #[test]
    fn constant_time_eq_is_correct() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
        assert!(constant_time_eq(b"", b""));
    }

    /// 同密码 + 不同盐 ⇒ 哈希不同（防彩虹表的前提）。
    #[test]
    fn salt_changes_the_hash() {
        let a = Credential::new("same-password");
        let b = Credential::new("same-password");
        assert_ne!(a.salt, b.salt);
        assert_ne!(a.hash, b.hash);
    }

    /// 哈希是可复现的（否则重启后密码永远对不上）。
    #[test]
    fn hash_is_reproducible() {
        let credential = Credential::new("p@ssw0rd");
        assert_eq!(credential.hash, hash_password(&credential.salt, "p@ssw0rd"));
        assert_ne!(
            credential.hash,
            hash_password(&credential.salt, "p@ssw0rd2")
        );
    }

    /// ★ 本需求本身：非回环 + 无密码 + 无豁免 ⇒ **必须拒绝启动**。
    #[test]
    fn refuses_non_loopback_without_password() {
        match startup_refusal(false, false, false) {
            StartupDecision::Refuse(reason) => {
                assert!(reason.contains(PASSWORD_ENV), "{reason}");
                assert!(reason.contains(ALLOW_UNAUTHENTICATED_ENV), "{reason}");
            }
            other => panic!("必须拒绝启动，实际 {other:?}"),
        }
    }

    /// 回环 + 无密码 ⇒ 放行（本机 / 桌面零摩擦）。
    #[test]
    fn allows_loopback_without_password() {
        assert_eq!(
            startup_refusal(true, false, false),
            StartupDecision::Allow { warn: None }
        );
    }

    /// 非回环 + 有密码 ⇒ 放行且无告警。
    #[test]
    fn allows_non_loopback_with_password() {
        assert_eq!(
            startup_refusal(false, true, false),
            StartupDecision::Allow { warn: None }
        );
    }

    /// ★ 逃生开关只把「拒绝」变成「放行 + 红色告警」，**绝不静默**。
    #[test]
    fn escape_hatch_allows_but_warns_loudly() {
        match startup_refusal(false, false, true) {
            StartupDecision::Allow { warn: Some(warn) } => {
                assert!(warn.contains("未设密码"), "{warn}");
                assert!(warn.contains(ALLOW_UNAUTHENTICATED_ENV), "{warn}");
            }
            other => panic!("必须放行并告警，实际 {other:?}"),
        }
    }

    /// 环境变量真值解析：常见写法都认，`0` / 空 / 乱码都不认。
    #[test]
    fn env_flag_parses_common_truthy_values() {
        for truthy in ["1", "true", "TRUE", " yes ", "on"] {
            assert!(env_flag(Some(truthy)), "{truthy:?} 应判为真");
        }
        for falsy in ["0", "false", "", "  ", "nope", "2"] {
            assert!(!env_flag(Some(falsy)), "{falsy:?} 应判为假");
        }
        assert!(!env_flag(None));
    }

    /// Cookie 解析：只认 `bs_session`，且能在一串 Cookie 里挑出来。
    #[test]
    fn cookie_token_picks_only_the_session_cookie() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("other=1; bs_session=abc123; theme=dark"),
        );
        assert_eq!(cookie_token(&headers).as_deref(), Some("abc123"));

        let mut empty = HeaderMap::new();
        empty.insert(header::COOKIE, HeaderValue::from_static("bs_session="));
        assert_eq!(cookie_token(&empty), None);

        assert_eq!(cookie_token(&HeaderMap::new()), None);
    }

    /// 会话 Cookie 必须带 `HttpOnly` + `SameSite=Strict`，且**不得**带 `Secure`。
    ///
    /// ⚠️ 最后一条是**故意的**：纯 HTTP 部署下 `Secure` 会让浏览器丢弃 Cookie，
    /// 表现为「密码对却永远登录不上」。这条断言防的是「好心加安全属性」把功能改坏。
    #[test]
    fn session_cookie_has_the_documented_attributes() {
        let cookie = session_cookie("deadbeef");
        assert!(cookie.contains("HttpOnly"), "{cookie}");
        assert!(cookie.contains("SameSite=Strict"), "{cookie}");
        assert!(cookie.contains("Path=/"), "{cookie}");
        assert!(!cookie.contains("Secure"), "不得带 Secure：{cookie}");
    }
}
