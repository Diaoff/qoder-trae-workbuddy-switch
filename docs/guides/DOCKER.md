# Docker 部署

把 Buddy Switch 的**服务端形态**（webui + API 网关）跑在容器里。

---

## 先明确边界：镜像里能做什么、不能做什么

| | 容器里 | 说明 |
|:---|:---|:---|
| webui（账号池 / 签到 / 积分 / 网关管理 / 日志） | ✅ | 完整可用 |
| 三个 API 网关入口（WorkBuddy / Trae / 统一） | ✅ | 需在配置里开启 |
| 账号导入 / 导出（文件） | ✅ | |
| 批量签到、积分刷新、余额保活 | ✅ | 走纯 API |
| **从本机客户端导入账号** | ❌ | 需要同机的 Trae / WorkBuddy 客户端数据目录 |
| **切换客户端登录态、启动客户端** | ❌ | 需要同机的图形客户端 |
| 托盘、悬浮窗、开机自启 | ❌ | 桌面宿主（Tauri）专有 |

一句话：**容器里是「多账号 API 网关 + 管理面板」，不是桌面切换器。**
桌面端与服务端可以**同时**跑，各用各的（数据目录不同即可，见下文）。

---

## 快速开始

### 方式 A · 预构建镜像（推荐）

```bash
docker run -d --name buddy-switch \
  -p 127.0.0.1:57890:57890 \
  -v buddy-switch-data:/data \
  --restart unless-stopped \
  ghcr.io/nextagentx/buddy-switch:latest
# 打开 http://127.0.0.1:57890
```

仓库根目录的 `docker-compose.yml` 默认也用这个镜像（`docker compose up -d`），
它同时保留了 `build:` 作为回落：**先 pull，拉不到才 build**（Compose 的既定语义）。

### 方式 B · 从源码构建

```bash
docker compose up -d --pull build     # 强制本地构建：忽略预构建镜像
docker compose logs -f                # 看启动日志（会打印最终监听地址）
docker compose down                   # 停止（命名卷里的数据保留）
```

`--pull build` 等价于 `docker compose build && docker compose up -d`。本地没有 GHCR
访问条件（内网、离线、要改代码）时走这条。⚠️ 这条路包含 `npm ci` + 前端全量构建
（含 `tsc` 与全部契约门禁）+ Rust release 全量编译，**很慢**。

### 镜像标签

| 标签 | 含义 |
|:---|:---|
| `latest` | 最近一次 **tag 发布**（不等于 `main` 最新代码） |
| `2026.10.191740` | 与 Releases 的 tag 一一对应（去掉 `v` 前缀），镜像内 `buddy-switch version` 报告的就是它 |
| `2026.10` | 该月的最后一次发布 |
| `edge` | **手动触发**工作流构建的当前分支，只用于验证，不保证可用 |

架构：`linux/amd64` + `linux/arm64`，同一个 tag 下是一条多架构 manifest list，
`docker pull` 会按当前机器自动选。**没有** `arm/v7`（32 位 ARM 请自行改 Dockerfile 构建）。

标签由 `.github/workflows/docker.yml` 在推 `v*` tag 时自动发布（两个架构各跑一台**原生**
runner，再由 merge job 拼 manifest）。手动触发该工作流只产出 `edge`。

### 镜像可见性

本仓是公开仓库，包也按 **Public** 发布 —— 2026-10-19 实测匿名 token 可直接拉到 manifest
（`ghcr.io/v2/nextagentx/buddy-switch/manifests/edge` 返回 200）。

若某次之后 `docker pull` 报 `denied` / `unauthorized`，到
`https://github.com/orgs/NextAgentX/packages/container/buddy-switch/settings`
确认 Visibility 仍是 Public。改成 Private 也能用，只是每台机器都要先 `docker login ghcr.io`；
而 `docker compose up -d` 拉不到时会**静默回落到本地构建**（看起来像「卡住了」）。

---

## 端口

| 端口 | 用途 | 容器内是否默认监听 |
|:---|:---|:---|
| `57890` | webui + 管理 API | ✅ 是 |
| `57891` | WorkBuddy API 网关 | ❌ 默认关闭，需在 webui 里开启 |
| `57892` | 统一网关 | ❌ 同上 |
| `57893` | Agent hook 接收端 | 固定绑回环，容器外无法访问 |
| `57894` | Trae API 网关 | ❌ 同上 |
| `57895` | Qoder API 网关 | ❌ 同上 |

`docker-compose.yml` 里几个网关的映射是**注释掉的**。要用它们，先做两件事：

1. 在 webui 的网关页**开启**对应网关；
2. 在网关配置里打开 `allow_non_loopback`（默认**拒绝**非回环监听，这是一条刻意的护栏），
   再取消 compose 里对应那行注释。

> ⚠️ 网关端口暴露出去等于把**上游额度**暴露出去（持有 API Key 即可消耗账号额度）。
> 网关自带 Bearer Key 鉴权，但请确认 Key 强度，并优先走内网或反代。

---

## 配置：三个环境变量

| 变量 | 默认 | 说明 |
|:---|:---|:---|
| `BUDDY_SWITCH_HOME` | `/data`（镜像内置） | **数据根目录**。账号库、网关 Key、签到/积分记录、备份都在它下面。 |
| `BUDDY_SWITCH_HOST` | `0.0.0.0`（镜像内置） | 监听地址。CLI 原生默认是 `127.0.0.1`，容器里必须改成 `0.0.0.0` 才能从宿主访问。 |
| `BUDDY_SWITCH_PORT` | `57890` | 监听端口。 |

优先级：**命令行参数 > 环境变量 > 内置默认值**。即 `docker run … buddy-switch serve --port 8080`
会覆盖 `BUDDY_SWITCH_PORT`。

### `BUDDY_SWITCH_HOME` 是 fail-closed 的

它**必须**是一个**已存在的绝对目录**，否则 CLI **拒绝启动**（退出码 2）并说明理由。

这不是过度谨慎 —— 曾经的事故形态正是「调用方以为自己在隔离目录里跑，
实际把账号 / 网关 Key / 签到与积分记录写到了用户真实数据上」。
容器入口脚本（`docker/entrypoint.sh`）会先 `mkdir -p`，所以正常情况你不用管它。

---

## 数据持久化

```yaml
volumes:
  - buddy-switch-data:/data        # 命名卷（推荐）
  # - ./data:/data                 # bind mount
```

- **命名卷**：首次创建时继承镜像里 `/data` 的属主（uid `10001`），开箱即用。
- **bind mount**：宿主目录的属主通常是 `root`，容器里的 `buddy` 用户（uid `10001`）写不进去，
  会表现为**启动失败**（`mkdir` 失败）。先修属主：

  ```bash
  sudo mkdir -p ./data && sudo chown -R 10001:10001 ./data
  ```

**备份**：整个 `BUDDY_SWITCH_HOME` 目录就是全部状态，直接打包即可：

```bash
docker run --rm -v buddy-switch-data:/data -v "$PWD:/backup" alpine \
  tar czf /backup/buddy-switch-$(date +%F).tar.gz -C /data .
```

> ⚠️ 备份里含**明文凭据**（账号 JWT / refresh token、网关 API Key）。妥善保管。

---

## 与桌面端共存

两边各自有 `BUDDY_SWITCH_HOME`，互不干扰：

| | 数据目录 |
|:---|:---|
| 桌面端 | `%USERPROFILE%\.buddy-switch`（Windows）/ `~/.buddy-switch` |
| 容器 | `/data`（卷内） |

**刻意不做共享**：桌面端的目录里有本机客户端相关的文件（exe 路径、快照），
容器里那些能力不可用；共享只会让两边对同一份数据做不同假设。

---

## 安全

**webui 没有登录，能读写账号库与网关 Key。** 因此：

- 默认只发布到宿主机回环 `127.0.0.1:57890`（`docker-compose.yml` 里写死的）。
- 服务端在监听**非回环**地址时会打印一条显式警告。
- 要跨机访问，二选一：
  1. **SSH 隧道**（推荐）：`ssh -L 57890:127.0.0.1:57890 <host>`，然后本地开
     `http://127.0.0.1:57890`。**不要**改 compose 的端口映射。
  2. **反代加鉴权**：Nginx / Caddy + Basic Auth 或 mTLS，再把映射改成 `"57890:57890"`。
- **不要**在无鉴权的情况下把 `57890` 直接暴露到公网或不可信局域网。

容器内的加固已经做了：非 root 用户（uid `10001`）运行、只装 `ca-certificates` 与 `curl`、
`/data` 由 `buddy` 持有。

---

## 构建细节（排查构建问题时读）

镜像是**三阶段**构建，顺序不可颠倒：

```
frontend (node:22)  →  npm ci && npm run build  →  dist/
builder  (rust:1.98) →  cargo build --release -p buddy-switch-server  →  二进制
runtime  (debian:12) →  二进制 + ca-certificates + curl
```

**为什么前端必须先构建**：`crates/buddy-switch-server/src/api.rs` 用
`#[folder = "../../dist/"]`（rust-embed）把前端**编译期**嵌进二进制。
顺序反了会嵌进一个空目录 —— 构建**全绿**，但容器起来后页面 404。

`.dockerignore` **排除 `dist/`**，阶段 2 只接受阶段 1 的产物，
从根上堵住「把宿主机那份旧前端编进镜像」。

**为什么只编 `-p buddy-switch-server`**：`src-tauri`（Tauri 桌面壳）在 Linux 上需要
`webkit2gtk` 等系统库，编它既没必要也会让构建失败。cargo 仍会解析整个工作区。

**构建慢**：当前没有做「先拷清单编依赖」的层缓存优化 —— 本工作区 4 个 crate 相互依赖，
造桩写法很脆（bin 要 `main.rs`、lib 要 `lib.rs`，漏一个就 `E0583`）。
改 Rust 代码会触发全量重编（约几分钟）。要提速可挂载 `target/` 缓存或改用
`cargo-chef`，但请先用 `scripts/check-docker.cjs` 确认没有破坏契约。

### 发布路径（CI）

`.github/workflows/docker.yml` 在推 `v*` tag 时自动发布 `ghcr.io/nextagentx/buddy-switch`：

1. **版本戳只在 tag 触发时盖**（`stamp-version.sh --tag "$GITHUB_REF_NAME"`）：两个架构各跑一次，
   但取值来自同一个 tag，结果一致。手动触发**不盖** —— `gen-release-notes.mjs` 是按 tag 取提交
   区间的，时间戳版本在 git 里没有对应 ref，它只会打一条 warning 并**跳过写文件**，随后
   `check:scope` 在 docker build 里红（错误信息只说「内置说明是 vA，应用版本是 vB」）。
2. `build` 在两个**原生** runner 上构建（`ubuntu-latest` = amd64、`ubuntu-24.04-arm` = arm64），
   各自 `push-by-digest` 推一个无名 digest。**不用 QEMU**：Rust release 编译在模拟下慢一个
   数量级，而本项目 `rusqlite` 走 `bundled`，等于用模拟器再编一份 SQLite 的 C 源码。
3. `merge` 用 `docker buildx imagetools create` 把两个 digest 拼成 manifest list，再打 tag。
   ⚠️ 同一个 tag 不能由两个 job 各推一次，否则后到的会覆盖先到的、`latest` 只剩一个架构。

镜像名是**字符串契约**，同时出现在本文件、`README*.md`、`docker-compose.yml` 与 workflow 里；
`npm run check:docker` 会校验四处一致（`.github` 不在 docker 构建上下文里，那一侧会自动跳过）。

---

## 排障

| 症状 | 原因 | 处理 |
|:---|:---|:---|
| `up -d` 不下载镜像、直接开始**编译**（几分钟后才起来） | 预构建镜像没拉到（包私有 / 网络不通 / 该 tag 还没发布），Compose 回落成本地构建 | 先单独 `docker pull ghcr.io/nextagentx/buddy-switch:latest` 看真实报错；私有包需 `docker login ghcr.io`，或把包改成 Public |
| `docker pull` 报 `denied` / `unauthorized` | 包被改成 Private；或登录用的 Token 没有 `read:packages` | 见「镜像可见性」一节 |
| `no matching manifest for linux/arm/v7` | 只发布了 `amd64` / `arm64` | 32 位 ARM 自行 `docker compose up -d --pull build` 本地构建 |
| 镜像版本与 tag 对不上 | 用了 `latest`，而它不是这次的 tag | 显式拉 `ghcr.io/nextagentx/buddy-switch:<版本>` |
| 容器起来后**立刻退出**，日志说「拒绝启动」 | `BUDDY_SWITCH_HOME` 不存在 / 非绝对路径 / 无写权限 | 检查卷挂载与属主（见「数据持久化」） |
| 宿主访问不到 `57890` | 容器内仍绑 `127.0.0.1` | 确认 `BUDDY_SWITCH_HOST=0.0.0.0` 已生效 |
| 页面 **404** 或空白 | 前端没编进二进制 | 检查构建顺序；`docker build` 有无跳过阶段 1 |
| 页面能开但**内容是旧的** | 宿主机旧 `dist/` 进了上下文 | 确认 `.dockerignore` 里有 `dist` |
| 网关请求全 **502** | 缺 CA 证书 | 镜像已装 `ca-certificates`；若自定义基础镜像需自行安装 |
| 日志里看不到任何出站错误 | 同上 | 同上 |

---

## 无 Docker 环境下的验证

本地没有 Docker 时，`npm run check:docker` 仍会校验四处字符串契约（阶段名引用、
二进制名、环境变量名、`.dockerignore` 覆盖），见 `scripts/check-docker.cjs`。
它**不能**替代真实的 `docker build`，但能挡住「本地没装 Docker 时根本发现不了」的那类漂移。
