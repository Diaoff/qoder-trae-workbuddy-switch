# Loomy 平台 — 事实梳理与建模决策

> 用途：把「Loomy 作为第四个平台接进 Buddy Switch」需要的事实与决策固定下来。
> 状态：**现行**（一期已落地：账号管理 / 短信登录 / 签到 / 积分 / 统一网关中继）。
> 协议事实来源：`../free-llm-router/src/providers/loomy.ts`（协议已实测摸透，非本仓文件）。

---

## 1. 这个平台是什么

讯飞办公助手 **Loomy**，**只有国内一套账号体系**：

| 项 | 值 |
|:---|:---|
| 业务基址（推理 / 模型 / 积分） | `https://loomyad.xunfei.cn/api/v1` |
| 账号基址（仅登录） | `https://account.xfinfr.com` |
| 业务成功码 | `000000` |
| 登录失效码 | `100002`（收到**不得重试** —— 同一死凭据重试无意义） |
| session 有效期 | 14 天（响应不带到期时间，本地按 TTL 推算） |
| 计费单位 | 积分（永久池 + 每日赠送池） |

**结论：单区域平台。**
⇒ 建模为**一个 `Product::Loomy` + 单作用域 `loomy-cn`**。
⚠️ 不硬塞 `loomy-global`（§11-A 的精神）；前端 `Scope` 模板字面量类型上存在
`loomy-global`，但**不在** `SCOPES` 表里，路由永远到不了（状态恒 UNKNOWN 占位）。

---

## 2. 协议事实（全部实测，含坑位）

### 2.1 两套认证头

- chat 端点（`/chat/completions`）认 `Authorization: Bearer`；
  官方客户端**两个头都发**（`Bearer` + 小写 `token`），照抄。
- 业务端点（`/models`、`/points/*`）**只认小写 `token` 头**；
  带错头得到 **HTTP 200** + `{"code":"100002"}`。

### 2.2 业务失败恒 HTTP 200

成败只能读 body 的 `code`。只看状态码会把「登录已失效」误判成成功。
chat 路径只在 content-type 是 `application/json` 时 peek 响应体判业务错误，
**绝不**对 SSE 流做整读。

### 2.3 模型目录的三种响应形态

`GET /models` 实测主形态是**裸对象 + `data[]`**（OpenAI 风格，**没有** `code` 字段）；
另认业务信封与裸数组两种兜底。条目 `type === "chat"` 才可对话
（**不要**用 `input_modalities` 判 —— 那是输入多模态，含 image）。
倍率写在展示名末尾括号里，三种括号混用
（`MiniMax M3 （x4.0）` / `Qwen 3.8 Max (x12.0)` / `GLM 5.3 Flash(x0.8)`）；
展示名规范化为 `{name} · x{rate}`，拆解幂等。

### 2.4 两池积分

`GET /points/records`：永久积分（`balance`）+ 每日赠送池（`dailyBalance`，
当天作废）。两池**分开报告**；缺 `balance` 字段显式报错（不返回 0 冒充「已用尽」）。

### 2.5 签到 = 触发每日额度重置

`POST /points/first-login` **不是**「+5000 积分」，而是触发每日池重置
（`dailyBalance = dailyQuota - dailyConsumed`，消耗后不回补）。
幂等判据是响应体 `alreadyProcessed`（**不是** HTTP 状态码）；
`dailyQuota` 只在该响应里，**不硬编码 5000**。

### 2.6 短信登录（HMAC-SHA1）

纯 HTTP 三步（发码 → 用户输入 → 校验），无本地监听。
讯飞账号（CAccount）端点用 **HMAC-SHA1** 签名：

- 9 段签名字符串以**两个换行结尾**（后两段恒空串）——去掉尾随换行签名不匹配；
- 空 body 的 `Content-MD5` 是**空串**（不是空串的 md5）；
- **body 序列化一次、签名与发送共用**（二次序列化改字节序 ⇒ 签名失效）；
- 认证头 `Authorization: account {ak}:{sig}`；
- AK/SK 是客户端内置常量（`region.rs` 的 `CN_REALM`），只用于账号端点签名，
  与业务凭据无关。

微信扫码（纯 HTTP 长轮询，`modules/loomy/wechat.rs`）已落地（二期 2026-10-22）：

| 项 | 事实 |
|:--|:--|
| 授权页 | `open.weixin.qq.com/connect/qrconnect`（HTML 内嵌 uuid，无需执行 JS）；AppID `wx18d60be432287cf8` |
| 长轮询 | `long.open.weixin.qq.com/connect/l/qrconnect`（40s 超时；`last` 游标带上一帧 errcode） |
| 二维码图 | `open.weixin.qq.com/connect/qrcode/<uuid>`（**纯字符串拼接** —— uuid 字符集含 `= + /`，转义反而坏） |
| errcode 语义 | 408=待扫码 / **404=已扫码待确认** / **405=已确认（wx_code 在这一帧）** / 403=取消 / 402=失效。⚠️ 蓝本曾把 404/405 读反；405 但 wx_code 空时保守判已扫不判成功 |
| 四步 bind | `bind/auth`（微信 code 换 rcode，**code 只用一次**）→ `bind===1` 走 `bind/skip` 直接换 session；`bind===0` 走 `bind/sendMsg` + `bind/checkCode` 绑定手机号后登录 |
| bind 缺失语义 | `bind` 字段缺失归为未绑（保守）：误跳过会拿到无手机号账号，多填一次手机号无害 |

---

### 2.7 积分逐日采样（二期 9.3）

账号库里的 `credits` / `dailyCredits` 只是**最近一次**读数（覆盖写），拿它画逐日趋势
只能得到一根贴地的假线。趋势的真实数据源是本地台账：

| 项 | 事实 |
|:--|:--|
| 文件 | `~/.buddy-switch/loomy/credits_daily.cn.json`（按 `(日期, uid)` 存两池读数，**不做区域级聚合**） |
| 为什么按 uid 分列 | 两池是**账号属性**且不可通约（每日池当天作废、永久池不清零）；跨账号求和会掩盖「某个账号的池已耗尽」。聚合留给展示层 |
| 写入时机 | ① 排程 `loomy_points_sample`（默认开，09:00 / 21:00 两帧）；② 手动「刷新积分」成功后补一帧 |
| 同日多次 | **覆盖写**（余额是状态量不是流量）；窗口 90 天，与 Trae 同名文件同口径 |
| 读取 | `GET /api/loomy/credits/daily` / `loomy_credits_daily` —— **只读本地文件，不联网**（打开用量页不替用户发上游请求） |

### 2.8 独立端口（二期 9.2）

默认端口 **57896**（`5789x` 一段的最后一位），默认**关**（与另四条同一口径：不声不响地
默认监听是危险默认值）。

| 项 | 事实 |
|:--|:--|
| 与统一入口的关系 | **并存，不是替代**。两者共用同一份 `LoomyGatewayState`（池 / 日志 / 禁用 / 粘性），关掉任一个不影响另一个 |
| Key 库 | `loomy/gateway_keys.json`，前缀 `sk-loomy-`，**无归属轴**（单作用域；多一列恒 `cn` 的字段就是假字段） |
| 鉴权 | `bearer_auth` 中间件（健康检查免鉴权）；错误体按端点分派 OpenAI / Anthropic 形状 |
| 端点 | `/health`(+`/healthz`) / `/status` / `/v1/models` / `/v1/chat/completions` / `/v1/messages` / `/v1/responses` |
| 认证之后 | 仍走**同一份** `chat_for_region`（与统一入口零分叉） |
| 备份 | `gateway` 分区从空操作改为真实读写 config + keys（清单由网关现取 ⇒ 不入备份） |

### 2.9 限流探测（二期 9.4）

| 项 | 事实 |
|:--|:--|
| 判据**两层** | HTTP 非 2xx ⇒ `classify_http`；**2xx 但信封 `code != "000000"`** ⇒ 按业务码（`100002`→`AuthDead`、`100001`→`ModelUnavailable`）。⚠️ 只看状态码会把死凭据报成 `ok` |
| 单格 | 单账号、单次、零重试、**30s 硬超时**；走 `loomy::routes::chat_once_for_account`（不选号、不换号） |
| 不写池 | 显式空操作：Loomy 池只有**账号级**冷却，把「某模型被限」回填成账号级会误伤同账号其它模型（方案 §4.5.4 的 T1） |
| 模型轴 | 读网关的**内存清单缓存**（10 分钟 TTL）—— 探测不额外联网；缓存为空时作业层提示「先去模型页重新识别」 |

---

## 3. 建模决策

| 决策 | 内容 | 理由 |
|:---|:---|:---|
| 作用域 | 单作用域 `loomy-cn` | 只有一套账号体系；单段命名空间会破坏前端 `Scope` 模板字面量与门禁管线 |
| 客户端 | **有**官方 Electron 客户端（macOS / Windows），但本仓**不依赖**它 | 一期起就是「无客户端可用」形态（issue #62：账号与网关只依赖凭据）；能力集 `checkin/credits/oauth/gateway` 无一需要本地目录或程序位。`client_ua` 常量取自它的包内值，不等于「要用它的文件」 |
| 端口 | **统一入口 + 可选独立端口 57896**（二期 9.2 起并存） | 一期按 §11-D 只走统一入口；二期补独立端口是因为「只想接 Loomy、不想开统一入口」是真实诉求，而 `/v1/*` 路径与另四条逐字重叠 ⇒ 必须独立端口（不能 merge） |
| 能力集 | `["checkin","credits","oauth","gateway"]`（与 Qoder 同集） | `oauth` = 面板内短信登录；`sessionCopy`/`travel`/`autoRotate` 无 Loomy 等价物 |
| refresh | **刻意不实现** | Loomy 无续期端点（session 14 天，过期只能重登）；网关按 `SessionDead` 换号，面板提示重新登录（诚实标记） |
| uid 口径 | userid 优先；无 userid 时派生 `loomy-<md5(phone\0token) 前16hex>` | 同一账号恒同 uid；**绝不用手机号**（同号多账号会合并）；token 不进存储 key |
| 回收站 | 一期不做 | 单作用域、误删可重导；Qoder 的回收站属多区客户端会话同步场景 |
| 判别特征 | 32-hex session ∨ 15–20 位 userid ∨ 11 位手机号 | 防止把别家 JWT 凭据吞成 Loomy（宁要显式指定，不要存错家） |

---

## 4. 落点速查

| 层 | 文件 |
|:--|:--|
| core 区域与常量 | `crates/buddy-switch-core/src/modules/loomy/region.rs` |
| core 协议 | `…/loomy/{chat,models,credits,credits_daily,checkin,oauth,signing,credential,api,account,paths}.rs` |
| gateway 中继 | `crates/buddy-switch-gateway/src/loomy/{mod,pool,routes}.rs` |
| 统一网关分派 | `unified/routes.rs` 的 `RelayTarget::Loomy` 分支 + `collect_namespace_models` |
| server 命令面 | `crates/buddy-switch-server/src/api.rs` 的 `/api/loomy/*` 段 + `Product::Loomy` 分支；独立网关的监听宿主在 `crates/buddy-switch-server/src/loomy_gateway_host.rs` |
| 桌面命令 | `src-tauri/src/commands.rs` 的 `loomy_*` 段 + `generate_handler!`；独立网关句柄在 `src-tauri/src/loomy_gateway.rs` |
| 前端 | `src/pages/Loomy*.tsx` × 6（含 `LoomyApiServicePage`）、`api.ts` 的 loomy 段、`lib/loomy-gateway.ts`、`use-scope-status.ts` 的 loomy 快照 |

密钥前缀：独立端口用 **`sk-loomy-`**（与 sk-wb-/sk-trae-/sk-qoder-/sk-uni- 同族）；
走统一入口时仍是 `sk-uni-`（两条形态并存，各有自己的 Key 库）。
