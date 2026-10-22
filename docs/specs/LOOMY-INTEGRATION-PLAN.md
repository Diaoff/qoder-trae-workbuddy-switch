# Loomy 集成 — 模块 / 接口 / 依赖梳理

> 状态：**实施中**（一期 + 二期微信扫码已落地；独立端口 / 积分趋势待做）。
> 事实依据：`docs/specs/LOOMY-PLATFORM.md`（平台事实与建模决策）。

## 0. 一句话方案

Loomy = 1 个 `Product::Loomy` + 单作用域 `loomy-cn`，能力 `["checkin","credits","oauth","gateway"]`（与 Qoder 同集）。按 `UNIFIED-GATEWAY-ARCH.md` §11 契约分层落地（L0→L5）。

## 1. 依赖顺序

L0 core 平台基础（product.rs / gateway_namespace.rs / loomy::region）→ 命名空间 loomy-cn 自动派生
L1 core 平台模块（modules/loomy/ 11 文件，30 单测）→ 账号库 / 判别 / 签名 / 协议解析
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
| L3 | 完成 | server 9 条 /api/loomy/* 端点 + 15 处 Product::Loomy 分支；tauri 9 条 loomy_* 命令；check:api 通过 |
| L4 | 完成 | 5 页面 + 2 张 UsagePage 表 + overview-metrics 3 张表 + backup-section + use-scope-status + 词表 + demo case |
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
- 独立端口 + impl GatewayProduct 监听（从 57896 起）+ sk-loomy- Key 库。
- 定期采样积分 → 用量页「趋势」视图。
- model_probe 的 Loomy 探针真机位接线（账号就位后）。
