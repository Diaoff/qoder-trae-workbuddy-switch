# Loomy 集成 — 模块 / 接口 / 依赖梳理

> 状态：**二期完成**（9.1–9.5 全部落地；9.5 的 hooks 机制取证仍在册未做，见下）。
> 事实依据：`docs/specs/LOOMY-PLATFORM.md`（平台事实与建模决策）。

## 0. 一句话方案

Loomy = 1 个 `Product::Loomy` + 单作用域 `loomy-cn`，能力 `["checkin","credits","oauth","gateway"]`（与 Qoder 同集）。按 `UNIFIED-GATEWAY-ARCH.md` §11 契约分层落地（L0→L5）。

## 1. 依赖顺序

L0 core 平台基础（product.rs / gateway_namespace.rs / loomy::region）→ 命名空间 loomy-cn 自动派生
L1 core 平台模块（modules/loomy/ 13 文件）→ 账号库 / 判别 / 签名 / 协议解析 / 积分逐日采样
L2 gateway 产品线（gateway/src/loomy/ + unified 分派）→ 无独立端口（§11-D）
L3 两处宿主（server api.rs + main.rs；tauri commands.rs + lib.rs）
L4 前端（registry / scope / App.tsx / 5 页面 / api.ts / 词表）
L5 门禁与文档

## 2. 落地状态（2026-10-22）

| 层 | 状态 | 说明 |
|:--|:--|:--|
| L0 | 完成 | 跨语言护栏绿；命名空间表 6→7 |
| L1 | 完成 | 30 单测；IO 测试隔离 home + 串行（同文件并行写竞态） |
| L2 | 完成 | gateway 623 测试全绿；RelayTarget::Loomy 中继 + 清单分支 + 管理面投影 + model_probe 真机位 |
| L3 | 完成 | server 16 条 /api/loomy/* 端点（9 账号 + 6 微信 + 1 采样视图）；tauri 16 条 loomy_* 命令；check:api 通过 |
| L4 | 完成 | 5 页面（含用量页趋势 + 设置页采样开关）+ 2 张 UsagePage 表 + overview-metrics 3 张表 + backup-section + use-scope-status + 词表 + demo case |
| L5 | 完成 | check-scope-registry 308 全绿；check-api-contract / check-store-selectors / check-agent-sources 绿；npm run build 成功 |

## 3. 硬约束（已全部落实）

1. 同批约束：product.rs 与 registry.ts 同批改，跨语言护栏绿。
2. Scope::parse fail-closed（坏例表含 loomy-glboal / loomy-cn-x 等）。
3. 所有 Product / RelayTarget / Scope match 穷举无兜底（编译器点名补齐）。
4. 失败必须显式：空目录 / 缺 balance / 缺 msgid / 无回收站 → 可诊断报错。
5. 100002 登录失效 → SessionDead 换号路径，不重试。
6. 前端永不下发令牌明文（只给掩码）。
7. uid 口径：userid 优先，否则派生（绝不用手机号）。

## 4. 二期清单

- ~~微信扫码登录（纯 HTTP 长轮询，二维码由面板渲染）~~ ✅ 2026-10-22（`modules/loomy/wechat.rs` + 6 条宿主命令 + 登录 Dialog；协议事实见 `LOOMY-PLATFORM.md`）。
- ~~独立端口 + impl GatewayProduct 监听（从 57896 起）+ sk-loomy- Key 库~~ ✅ 2026-10-22（二期 9.2）
  `gateway/src/loomy/{apikey,mod}.rs`（`LoomyGatewayConfig` / `LoomyApiKeyStore` /
  `impl GatewayProduct for LoomyGateway` / `status_view` / `admin_models_response`）+
  `gateway/src/loomy/routes.rs` 的四条端点与 `bearer_auth` + 两宿主监听
  （`{buddy-switch-server/src,src-tauri/src}/loomy_gateway*.rs`）+ 宿主的
  `/api/loomy/gateway/*` 与 `loomy_*_api_key` 一族 + `LoomyApiServicePage`。
  ★ 与统一入口**并存**（共用同一份 state），关掉任一个不影响另一个。
- ~~定期采样积分 → 用量页「趋势」视图~~ ✅ 2026-10-22（二期 9.3）
  `modules/loomy/credits_daily.rs`（按 `(日期, uid)` 存两池读数，90 天窗口）+ 排程
  `ScheduleTask::LoomyPointsSample`（默认开，09:00 / 21:00 两帧，登记进 `background::TASKS`）
  + `GET /api/loomy/credits/daily` / `loomy_credits_daily` + 用量页「每日积分」双柱图
  + 设置页「积分逐日采样」开关与时间表。手动「刷新积分」成功后也补记一帧。
- ~~model_probe 的 Loomy 探针真机位接线~~ ✅ 2026-10-22（二期 9.4）
  `gateway/src/model_probe.rs`：`probe_cell_loomy`（单账号单次、零重试、30s 硬超时、**不写池**）
  + `probe_state_of_loomy` / `probe_state_of_loomy_code` 两层判据 + `loomy_envelope_code`
  （SSE 首帧与裸 JSON 两种形态）。模型轴读网关**内存清单缓存**（不额外联网）；
  `adopt` 保持显式空操作（池无模型级冷却轴，回填会过度冷却）。
- ~~agent_watch 源登记~~ ✅ 2026-10-22（二期 9.5）
  `SOURCE_LOOMY_CN = "loomy-cn"` 进 `agent_watch::HOOK_SOURCES` +
  `companion-row.tsx` 的 `SOURCE_LABEL_KEY` + `shared.{zh,en}` 词条
  （`check-agent-sources.cjs` 已钉住三处一致，现 10 个来源）。
  **只作来源、不代装**：`agent_hook_install` 里没有 Loomy 的安装档 —— 与
  `codex` / `trae-*` / `qoder-*` 同一判据（本仓未取证它们的 hook 配置 schema）。

  ⚠️ **在册的未取证项**：Loomy 客户端是否真有「执行外部命令 / hooks」机制，
  本仓**零取证**。若有，用户可自行把 `buddy-switch hook --source loomy-cn`
  配进客户端；若没有，本条登记就是一个永不触发事件的来源（不造成错误显示，
  因为 `canonical_source` 只认显式传入的 id，不会把别的来源错标成它）。
  取证后应回来更新本节结论。
