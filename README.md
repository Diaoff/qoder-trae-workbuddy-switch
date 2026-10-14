<p align="center">
  <img src="public/icon-transparent.png" alt="Buddy Switch 图标" width="128" />
</p>

<p align="center">
  <strong>Buddy Switch</strong><br />
  WorkBuddy / CodeBuddy / Trae 账号切换工具
</p>

<p align="center">
  <strong>简体中文</strong> · <a href="README.en.md">English</a>
</p>

# Buddy Switch

**WorkBuddy / TraeWork 多账号管理工具**：一个概览工作台把「待处理」摆到眼前，OAuth 登录与一键切换登录态，签到、积分到期、Token 与积分用量统计合并成一个「用量」工作台，跨账号关联同一份会话的副本并做安全同步，账号删除先进回收站，并能把模型额度以 OpenAI / Anthropic 兼容接口提供给本机其它工具。

> ⚠️ **免责声明**：本项目是**非官方的第三方源码公开工具**（source-available，非商业许可），与 WorkBuddy、CodeBuddy、Trae / TraeWork 及其各自权利人不存在隶属、授权或背书关系。它会读写本机第三方客户端的认证数据、按你的配置自动发起请求，并可将你的模型额度通过本地接口转发给其它工具。使用前请完整阅读[免责声明](docs/DISCLAIMER.md)，自行评估风险并确保使用方式符合相关服务条款。

同一套界面提供三种形态：

| 形态 | 获取方式 | 说明 |
| --- | --- | --- |
| **桌面 App** | 从 [GitHub Releases](https://github.com/NextAgentX/trae-workbuddy-switch/releases/latest) 下载安装包 | Tauri 打包，推荐日常使用 |
| **webui（浏览器）** | 从源码构建，见 [webui 形态](#webui-形态源码构建) | 与桌面 App 同一份前端产物，走本地 HTTP 通道 |
| **在线演示** | [GitHub Pages](https://nextagentx.github.io/trae-workbuddy-switch/) | 只读演示；账号、积分与请求记录均为虚构数据，所有业务操作已禁用 |

> 「**概览**」是默认目的地：先看有没有账号需要重新登录、有没有积分要过期、今天有没有签到，再决定去哪一页处理。
>
> **暂未提供 npm 安装**：npm 包 `@nextagentx/buddy-switch` 尚未发布，`npm i -g …` 还装不到它。需要 webui 形态请按下方[从源码构建](#webui-形态源码构建)；发布后会在这里补上安装命令。


## 作用域：平台 × 区域

顶栏左上角的**作用域选择器**决定「现在管的是哪一套账号」。作用域 = 平台 × 区域，共四个：

| 作用域 | 管理的客户端 | 区域 |
| --- | --- | --- |
| **WorkBuddy 国内版** | WorkBuddy 桌面客户端、CodeBuddy CN IDE、CodeBuddy CLI、VS Code / JetBrains 的 CodeBuddy 扩展 | 国内版 |
| **WorkBuddy 国际版** | 同上（国际版客户端） | 国际版（WorkBuddy AI） |
| **TraeWork 国内版** | Trae Work（客户端 `TRAE SOLO CN`）、Trae IDE（`Trae CN`） | 国内版 |
| **TraeWork 国际版** | Trae Work（`TRAE SOLO`）、Trae IDE（`Trae`） | 国际版 |

四个作用域**彼此独立**：独立账号库、独立客户端、独立配置。切换后侧栏、内容区与统计口径全部跟随；作用域写进 URL（`/s/wb-cn/accounts`），**刷新、分享链接、前进后退都保持当前位置**。`⌘K` / `Ctrl+K` 打开命令面板可跨作用域搜索跳转，`⌘1`–`⌘4` 直接切换作用域。

侧栏固定 **6 个目的地，两个平台逐条同构**：**概览 · 账号 · 用量 · 会话 · 网关 · 设置**。

> **Trae 侧不渲染「会话」入口**：会话复制依赖该平台自身的会话存储与云端归属机制，目前只有 WorkBuddy 具备。这是**能力门控**而非平台分支——URL 直达时给的是「暂不支持」的结构化空态，不会列出 WorkBuddy 的会话。
>
> **Trae 的程序位在账号卡上**：Trae Work 与 Trae IDE 可同机并存，它们在 Trae 作用域**内部**切换（账号卡上的程序位按钮），不占侧栏。程序位是数据维度，区域才是作用域的组成部分。

## 快速开始

### 桌面 App

前往 [GitHub Releases](https://github.com/NextAgentX/trae-workbuddy-switch/releases/latest) 下载对应平台的安装包：

| 平台 | 安装包 | 安装方式 |
| --- | --- | --- |
| macOS Apple Silicon（M 系列，arm64） | `BuddySwitch_<版本>_aarch64.dmg` | 打开 DMG，将 `BuddySwitch.app` 拖入「应用程序」 |
| macOS Intel（x86_64） | `BuddySwitch_<版本>_x86_64.dmg` | 打开 DMG，将 `BuddySwitch.app` 拖入「应用程序」 |
| Windows x64 | `BuddySwitch_<版本>_x64-setup.exe` | 运行安装程序并按提示完成安装 |
| Linux x64 | `BuddySwitch_<版本>_amd64.deb` / `BuddySwitch_<版本>_amd64.AppImage` | Debian/Ubuntu 安装 `.deb`；其他发行版可给 AppImage 添加执行权限后直接运行 |

macOS 首次启动若提示无法验证开发者，先在 Finder 中按住 Control 点击应用并选择「打开」，或前往「系统设置 → 隐私与安全性」选择「仍要打开」。仅当安装包来自上述官方 Releases、且系统仍提示「已损坏」时，再执行：

```bash
xattr -rd com.apple.quarantine "/Applications/BuddySwitch.app"
```

应用能启动但切换账号时提示无权限，请参阅下方 [macOS 权限说明](#macos-权限说明)。

### webui 形态（源码构建）

本项目暂未通过 npm 分发，因此 webui 形态需要自己编译。**注意顺序**：前端产物是被服务端二进制在**编译期**嵌入的（`rust-embed`），所以必须先构建前端，再编译服务端：

```bash
npm install
npm run build                                    # 1) 构建前端 → dist/
cargo build --release -p buddy-switch-server     # 2) 编译服务端（把 dist/ 嵌进去）
./target/release/buddy-switch                    # 3) 启动本地服务 + 自动打开浏览器（Windows 为 buddy-switch.exe）
```

启动后可用的子命令：

```bash
buddy-switch              # 启动本地服务 + 自动打开浏览器
buddy-switch serve        # 只起服务，不开浏览器（--port 指定端口）
buddy-switch status       # 终端查看当前账号
buddy-switch version      # 版本号
```

webui 默认监听 `127.0.0.1:57890`。界面与桌面 App 完全一致——同一份前端产物，桌面端走 Tauri 命令通道，webui 走本地 HTTP 通道。

> 改了前端但没重新编译服务端时，界面不会变（嵌入的是旧产物）。重编前请先停掉正在运行的 `buddy-switch`，否则 Windows 上会因二进制被占用而链接失败。

## 功能

侧栏 6 个目的地，两个平台逐条同构。

### 概览（两个平台同一页）

| 模块 | 说明 |
| --- | --- |
| 待处理清单 | 一次列清当前作用域该处理的事：账号需要重新登录 / 凭据已过期 / 今日未签到 / 有即将过期的积分 / 客户端未运行 / 还没有账号。每项都带**行内动作**，未签到可直接一键补齐 |
| 统计卡 | 账号总数、客户端 Token（今日）、今日积分消耗、网关请求（今日）；「较昨日」真算，昨日无数据时不给百分比（不编假增长） |
| 近 14 天 Token 用量 | 逐日柱状图，与统计卡同源 |
| 其他作用域 | 四个作用域的登录态与账号数一览，不必切过去就能看出哪个作用域有事要办 |
| 能力清单 | 按平台能力渲染（自动签到 / 自动旅行 / 积分统计 / OAuth 登录 / API 网关 / 自动轮换 / 会话复制） |

### 账号（WorkBuddy）

| 模块 | 说明 |
| --- | --- |
| 账号管理 | OAuth 扫码登录、从本机导入、从备份文件导入、手动添加 token、删除账号、导出账号备份 |
| 卡片 / 表格两种视图 | 同一批数据的两种密度；另有搜索、排序、分组筛选、批量条与整卡点开的详情抽屉 |
| 程序位切换 | 每张卡上 5 个可切换程序位：WorkBuddy 桌面（`WB`）、CodeBuddy CN IDE（`IDE`）、CodeBuddy CLI（`CLI`）、VS Code CodeBuddy 扩展（`VSC`）、JetBrains CodeBuddy 插件（`JB`）；另有 1 个**只读探测位**（VS Code CodeBuddy 插件），只观测不切换 |
| 账号切换 | 备份认证文件 → 关闭客户端 → 写入目标账号 → 重启，切换过程实时进度反馈 |
| 会话复制 | 将当前账号勾选的会话以新 id 复制给目标账号（jsonl 正文 + `workbuddy.db` 索引 + edge-sync 注册） |
| 自动签到 | 默认开启；启动即检查，并按排程自动补签；支持一键全部签到与签到日志 |
| 自动旅行 | 自动派发 / 领取「猫猫旅行」任务 |
| Token 保活 | 惰性刷新（操作前不足阈值才刷新）+ 每日保活（默认每天无条件刷新一次），避免 refresh token 过期 |
| 自动轮换 | 后台定时把 CodeBuddy CLI 后续启动账号设为积分最紧迫的账号；当前会话保持原账号（策略见下） |
| 切换时迁移数据 | 把当前账号的长期记忆（Memory）与连接器配置合并到目标账号（同名条目递归合并、按内容去重），支持同版本与跨版本迁移；改写前自动备份原文 |
| 回收站 | 删除的账号先进回收站，**保留 30 天**，期间可随时恢复；也可永久删除或清空（永久删除会一并删除其凭据与快照） |
| 权限检测 | macOS 授权引导（App 管理 / 完全磁盘访问拖拽授权 + 自动检测） |
| 客户端限流台账 | 从客户端本地数据**只读**汇总「账号 × 模型 × 恢复时刻」：哪个账号在哪个模型上被上游限流、何时恢复；仍受限的置顶高亮，已恢复的灰显。不依赖网关、不改动账号状态；排队类错误码（`6020` / `6022`）按 code 原样展示并标注「排队中」 |

### 账号（TraeWork）

| 模块 | 说明 |
| --- | --- |
| 账号管理 | OAuth 网页登录（浏览器回调）、手动粘贴 `Cloud-IDE-JWT`、从本机导入、从备份文件导入、删除账号、导出账号备份 |
| 程序位与分组 | 账号卡上按程序位切换（TraeWork / TraeCode / Trae，区域决定具体客户端变体）；账号可分组，列表支持按分组筛选 |
| 签到 | 一键签到并刷新积分；支持「跳过今日已签到账号」与自动续期；账号卡片展示签到状态与冷却 |
| 积分 | 逐账号展示剩余积分、逐包明细与到期时间 |
| 登录态切换 | 把选定账号写入 Trae 客户端的登录态（`profiles/`），切换前自动备份；重启客户端后生效 |
| 登录态快照 | 保存 / 恢复 / 删除客户端登录态快照，便于回滚 |
| JWT 刷新 | 手动刷新账号 JWT 并查看剩余有效期（临期高亮）；**也可按排程自动续期**（见「设置」的 `trae_jwt_refresh`），不必再靠手动点 |
| 设备标识 | 查看与重置设备标识 |
| 环境探测 | 自动探测本机安装的产品线、客户端版本、运行状态与 userData 目录 |
| 运行日志 | 客户端数据目录、网关请求日志与操作日志集中查看 |

### 用量（两个平台同一工作台）

一级四视图：**趋势 · 模型 · 会话 · 积分**。Trae 无会话副本能力 ⇒ **不渲染「会话」**，只保留趋势 / 模型 / 积分（缺数据源就不画空壳，空 Tab 会让人以为「数据是 0」）。视图参数条全页只有一条——统计范围（近 7 / 14 / 30 天）与数据来源（仅本作用域 / 合并全部作用域），**切子视图不重置**。

| 视图 | 说明 |
| --- | --- |
| 趋势 | 统计卡（区间总 Token / 日均 / 峰值 / 模型数）+ 逐日堆叠柱状图（按模型拆分）+ 累计消耗曲线 + 模型占比环形图 + 活跃热力图 |
| 模型 | 模型占比与模型排行，附与趋势同源的迷你趋势线 |
| 会话 | 会话排行与会话散点，按 Token 与调用次数定位「哪个会话最贵」 |
| 积分 | 汇总 WorkBuddy 官方请求用量：每日趋势、模型分布、账号消耗与请求明细；官方数据不可用时明确回退到本地余额快照观察。含积分余额变化与积分到期分布 |

> 两个平台的图表集**同源**：Trae 的用量数据经适配层映射成共用形状后复用同一批图表组件，块序与口径逐条一致——看起来一样不是巧合。

### 会话（仅 WorkBuddy）

「关联会话」把**同一份会话在各账号里的副本**摆到一屏：谁落后、谁分叉、同步给谁。

| 能力 | 说明 |
| --- | --- |
| 副本归属 | 按会话组列出「N 个副本」及各自属于哪个账号；来源账号已删时显示 `uid（未知账号）`，**不隐藏该行** |
| 内容级状态 | **已一致 / 待同步 / 有分歧 / 状态未知**由会话 jsonl 的**内容指纹**派生（**不是**「谁的时间新」），并给出「共同旧版 N 条」「自上次一致 +N 条」 |
| 安全同步 | **加法**语义：只把来源的新增内容**追加**到目标，不删除、不覆盖目标已有内容（页面上常驻写明这一点） |
| 处理分歧 | 分歧无法自动合并 ⇒ 必须先选保留哪一份，其余副本被它覆盖（覆盖前自动备份，并明确告知会丢弃多少条） |
| 插件会话复制 | 复制 VS Code 插件 / CodeBuddy IDE 侧的历史会话到本作用域其它账号。与桌面版**不是同一套存储**，因此「未检测到客户端」与「这个账号没有会话」是两段不同的话 |

### 网关（四个作用域各自独立，另有跨作用域的统一入口）

把当前账号的模型额度以**兼容接口**暴露给本机其它 AI 工具（Cursor、Claude Code、OpenWebUI 等）。Key 与账号池按作用域隔离。页面分**七个分区**：**概览 · 密钥 · 模型 · 账号池 · 请求代理 · 请求日志 · 统一入口**（Trae 侧没有「请求代理」，故只渲染六项）。

| 能力 | 说明 |
| --- | --- |
| 兼容协议 | OpenAI 兼容（`/v1/models`、`/v1/chat/completions`、`/v1/responses`）与 Anthropic 兼容（`/v1/messages`），均支持 SSE 流式 |
| 监听配置 | 默认仅监听 `127.0.0.1`；改为 `0.0.0.0` 需二次风险确认 |
| 默认端口 | WorkBuddy 网关 `57891`；Trae 网关 `7864`；**统一入口 `57892`**（默认**关**，需在「统一入口」分区手动启用）。Trae 另有本地 MITM 代理端口 `8899`，登录态捕获依赖它 |
| API Key | 可创建多个 Key，归属到具体区域；列表只显示前缀，**不落明文、不回显 hash**；**三态动作**：吊销（置 `revoked_at`）· **恢复**（清 `revoked_at`）· 删除（**仅已吊销可删**）。已吊销的行必须同时给「恢复」和「删除」两个出口——只留删除等于逼用户「删掉重建 + 改所有客户端配置」 |
| 模型目录 | 拉取并展示可用模型清单，可手动刷新，也可逐个**禁用** |
| 账号策略 | 配置账号选择策略与账号池余额刷新间隔；账号池可校验上游凭据有效性 |
| 请求代理 | 配置网关出站代理 |
| 请求日志 | 记录每次请求的模型、账号、Token 与状态，可清空 |
| 接入指引 | 页面内直接给出常见工具的配置片段，可一键复制 |
| 统一入口 | **第三个入口，跨作用域**：把两个产品线的模型合成**一个 provider**，模型 id 自带来源前缀（命名空间），解决「同名模型在不同平台/版本互相覆盖」。见下 |

#### 统一入口（第三个产品线，跨作用域）

两个既有网关的模型 id **不含来源** —— 同一个 `deepseek-v4.1-flash` 在 WB 国内 / WB 国际 / Trae 国内 / Trae 国际是四条不同上游、不同倍率，而客户端按 id 去重 ⇒ 同名模型只能存在一条、会被互相覆盖。统一入口的做法是**让 id 自己携带来源**：模型名写成 `<命名空间>:<模型id>`。

| 能力 | 说明 |
| --- | --- |
| 一个 provider 用遍全部平台 | 单端口 `57892`、单套 Key（前缀 `sk-uni-`）、单份 `/v1/models`。客户端只需配一个 Base URL |
| 命名空间 = `{客户端名}-{区域}` | 默认启用 4 条：`wb-cn` · `wb-global` · `trae-cn` · `trae-global`；另有 4 条**可选**（`traework-*` / `traecode-*`，两条老程序线），默认关、勾选即开 |
| 裸模型名一律拒绝 | 不带前缀的请求返回 400，并给出可用前缀清单——不做「猜一个默认命名空间」这种会在表变化时静默走错池的兜底 |
| 一把 Key 可跨平台 | 同一个 Key 可同时授权 `wb-*` 与 `trae-*`；业务转发（`/biz/*path`）的命名空间来自 Key 自身字段（业务请求没有模型名，只能这样定） |
| 三个入口互不影响 | 57891 / 7864 / 57892 各自独立监听；关掉统一入口不影响任何既有客户端。`/v1/responses` 在 Trae 侧本就不支持，统一入口返回 **501**（不静默降级到 chat） |
| 跨作用域 | 与「Key 表 / 请求日志不做版本过滤」同性质：**上方的作用域选择器对「统一入口」不生效**，卡 1 顶部有一行明确说明 |

> 统一入口的命名空间**派生自作用域注册表**：新增平台/区域时命名空间自动出现，UI 零改动。架构与决策依据见 [`docs/redesign/UNIFIED-GATEWAY-ARCH.md`](docs/redesign/UNIFIED-GATEWAY-ARCH.md)，交互规格见 [`docs/redesign/UNIFIED-GATEWAY-UI.md`](docs/redesign/UNIFIED-GATEWAY-UI.md)，客户端接入配置见 [`docs/GATEWAY-CLIENT-INTEGRATION.md`](docs/GATEWAY-CLIENT-INTEGRATION.md)。
>
> ⚠️ 命名空间解析**只做整段精确匹配、禁止前缀匹配**：`trae` / `traework` / `traecode` 互为前缀，任何 `starts_with` 式推断都会把请求发给另一套账号池。

### 设置（两个平台同一张九项子导航）

**通用 · 账号与登录 · 签到与积分 · 网关 · 计划任务 · 客户端 · 权限与诊断 · 日志 · 备份与恢复**。分区由 URL 承载（`?section=`），不再是一屏长滚动。其中计划任务排程按平台分两组、共 **8 类**：WorkBuddy 侧六类——签到、猫猫旅行、活跃地图、token 保活、开学季、夜猫子；Trae 侧两类——`trae_checkin`（签到）与 `trae_jwt_refresh`（JWT 自动续期）。各自可独立开关并配置执行小时。

### 应用级能力（四个作用域共用）

侧栏底部与应用设置**不随作用域切换**，四个作用域共用同一份实现。

| 能力 | 说明 |
| --- | --- |
| 应用设置（顶栏 ⚙） | 右侧抽屉，非下拉菜单：主题（浅色 / 深色 / 跟随系统）、界面语言（简体中文 / English）、开机静默启动到托盘、关闭窗口保持后台运行、自动更新（含更新代理地址），以及「更多入口」 |
| 全局备份与恢复 | 一次覆盖**全部作用域**：可按账号 / 配置 / 网关三类分区勾选，默认**不含凭据**（勾选含凭据时二次确认并警告明文风险）；导入前可预览内容差异 |
| Skill 管理 | 扫描 `{data_dir}/skills/<dir>/SKILL.md` 的 frontmatter，清单化展示并逐项**启用 / 停用**（一并写齐 `user-invocable` / `disable-model-invocation` 两个字段，**行级重写**、其余字节原样保留，含 CRLF/BOM）；支持跨区域（CN ↔ Global）搬迁补充（冲突时保留目标侧）与从本地目录 / zip 导入 |
| 客户端设置托管 | 统一读写**被代理的上游客户端**（WorkBuddy 两版 + Trae 四个程序位）各自的 `settings.json`：6 个槽位的声明式规格表、格式指纹（缩进单位 + 行尾）、**手术式字节级补丁**（改动最小化）、备份 / 恢复与跨槽位差异对比；写入前有进程运行守卫 |
| 会话悬浮窗（仅桌面端） | 独立小窗展示跨来源的会话 / 任务状态（运行中 / 待确认 / 已完成 / 失败），只读、可点击聚焦回原会话；侧栏与托盘是同一开关的两面。webui 形态不渲染该入口。数据源 = 本工具的切号进度 + **已开启监听的客户端会话**（见下一行） |
| 客户端会话监听（**默认关**） | 在「设置 → 客户端」开启后，往 WorkBuddy（国内版 / 国际版）的 `settings.json` 装 hook，把客户端里发起的任务状态实时送进悬浮窗。需用户**显式开启**、随时可关闭还原；装 / 卸都走客户端设置托管（写前备份 + 格式保持 + **客户端运行中拒写**），不覆盖用户已有的其它 hook |

> ⚠️ 「客户端设置托管」读写的是**上游客户端自己**的 `settings.json`，与本工具自己的代理设置（`SettingsPage` / `TraeSettingsPage` 里的 `proxy*` / `traePath*`，落在 `~/.buddy-switch/**`）是**两个不同的文件**，只是名字相近。

## 使用

### 通用

1. **选作用域**：顶栏左上角切换平台 × 区域；侧栏随之跟随，选 `TraeWork` 即进 Trae
2. **看概览**：默认落在「概览」，先处理「待处理」里的条目——重新登录、今日未签到（可一键补齐）、即将过期的积分
3. **搜索与跳转**：`⌘K` / `Ctrl+K` 打开命令面板，跨作用域搜索账号、页面与动作；`⌘1`–`⌘4` 直接切作用域
4. **应用设置**：顶栏 ⚙ 打开右侧抽屉，可改主题、界面语言、开机静默启动与更新选项

### WorkBuddy

1. **添加账号**：账号页 →「OAuth 扫码添加」（device flow）、「导入本机账号」或「导入备份」
   - 客户端 5.6 起把登录态 token 加密存储。**「导入本机账号」会自动调用本机客户端解开这层加密**（需已安装 WorkBuddy 桌面端），导入后签到 / 积分 / 用量统计可直接使用。若本机没装客户端或客户端换了加密实现，账号**仍会导入成功**，但仅能用于「切换账号」；此时可对账号点「刷新 Token」重试解锁
2. **切换账号**：账号卡片 →「切换」，可勾选把当前账号的会话一并复制过去，并把长期记忆与连接器配置合并到目标账号（跨版本迁移时可选数据来源版本）
3. **切换程序位**：每张卡上的 `WB` / `IDE` / `CLI` / `VSC` / `JB` 直接切换该账号要写入的客户端。「切换 CodeBuddy」只更新**后续加载会话**使用的默认账号，当前运行会话不会切换——请由 ACP 重新加载会话，或重启 CodeBuddy CLI 后生效
4. **自动签到 / 自动旅行**：账号页顶部直接开关；设置 →「签到与积分」可调整参数、立即签到并查看日志
5. **自动轮换**：设置 →「签到与积分」→ CodeBuddy CLI 自动轮换（策略见下）
6. **看用量**：侧栏「用量」——趋势 / 模型 / 会话 / 积分四个视图共用一条参数条；点「刷新统计」才重新采集官方接口
7. **删除与恢复**：删除账号先进回收站（账号页工具栏 →「回收站」），30 天内可恢复
8. **备份与迁移**：设置 →「备份与恢复」统一导出 / 导入，默认不含凭据

### TraeWork

1. **添加账号**：账号页 →「OAuth 网页登录」（浏览器完成授权后回调）或「粘贴 JWT」；也可「导入本机账号」「导入备份」
2. **选程序位**：账号卡上的程序位按钮决定要操作的客户端（TraeWork / TraeCode / Trae）；区域已在顶栏作用域里定好
3. **签到**：账号页「签到并刷新积分」，或对单个账号执行；开启「跳过已签到」避免重复请求
4. **切换登录态**：账号卡片 →「切换」，写入所选客户端登录态并重启客户端；切换前自动保存快照
5. **快照回滚**：设置 →「账号与登录」里的登录态快照，可保存 / 恢复 / 删除
6. **看用量**：侧栏「用量」——趋势 / 模型 / 积分（Trae 无会话副本能力，不渲染「会话」视图）
7. **网关**：侧栏「网关」，创建 Key 后按页面上的接入指引配置到你的工具里。想让**一个 provider 用遍全部平台**（模型名带命名空间前缀）就到网关页的「统一入口」分区启用它（默认关，端口 `57892`）

### 应用级能力（四个作用域共用）

顶栏 ⚙「应用设置」抽屉里与作用域无关，切换作用域时保持可用：

1. **Skill 管理**：抽屉 →「更多入口」→ Skill 管理 → 选择区域，查看该区域 `skills/` 目录下的清单；用开关逐项**启用 / 停用**（同时写齐两个 frontmatter 字段），或用「从对侧区域搬迁」把条目补充到另一区域，也可从本地目录 / zip 导入
2. **客户端设置托管**：抽屉 →「更多入口」→ 客户端设置 → 选择要托管的客户端槽位（WorkBuddy 两版 + Trae 四个程序位），读取并编辑其 `settings.json` 指定键值；写入为**最小化字节级补丁**（缩进与行尾保持原样），可先查看跨槽位差异、也可随时备份 / 恢复。**写入前请先退出对应客户端进程**（运行中会被直接拒绝）

### 自动轮换策略（WorkBuddy）

自动轮换的目标是防止积分过期浪费：后台定时查询所有账号的积分到期情况，把 CodeBuddy CLI 后续会话的默认账号设为「最紧迫」的账号（最早到期且仍有剩余积分）。macOS/Linux 的 `apiKeyHelper` 与 Windows 的 settings env 都不会替换正在运行会话已经持有的 token；轮换结果需在 ACP 重新加载会话或重启 CLI 后生效。为避免默认账号频繁变化，每次检查按以下顺序决策：

1. **有效账号**：查询成功、未过期、有剩余积分的账号才可被选为目标
2. **紧迫度检查**：所有账号到期都还早（最紧迫的剩余超过 `min_urgency_hours`，默认 72 小时）→ 不切
3. **已是目标**：CLI 默认账号就是最紧迫账号 → 不切
4. **冷却期**：切换后 `cooldown_minutes`（默认 120）内不重复切
5. **活跃保护**：最近 `active_guard_minutes`（默认 30）内 CLI 会话有写入（正在对话）→ 不切
6. **价值过滤**：目标账号剩余积分低于 `min_remaining_credits` → 不值得切（默认 0 关闭；每次检查会把各账号剩余积分写入日志，可据此调整）
7. **防抖动**：目标比当前早到期但差异小于 `min_gap_hours`（默认 24）→ 不切

> **生效边界**：自动轮换只更新后续 restore/load 使用的默认账号，不会热切换当前会话。macOS/Linux 下一次 helper 执行会读取最新账号；Windows 会把最新 Token 写入 settings。正在运行的会话继续使用启动或加载时取得的账号；请由 ACP 重新加载会话，或重启 CodeBuddy CLI。

配置项：`check_interval_minutes`（检查间隔，默认 5）、`cooldown_minutes`、`min_urgency_hours`、`active_guard_minutes`、`min_remaining_credits`、`min_gap_hours`。可在设置页调整，或直接编辑 `~/.buddy-switch/auto_rotate_config.json`。

## 界面预览

### 概览

默认目的地。顶栏左侧是作用域选择器（平台 × 区域），下面是「待处理」清单——每一条都带行内动作；再往下是四个统计卡与近 14 天 Token 柱状图，右侧是其他作用域与能力清单。

<table>
  <thead>
    <tr>
      <th>浅色模式</th>
      <th>深色模式</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/overview-light.png" alt="概览页（浅色模式，演示数据）" /></td>
      <td><img src="docs/images/overview-dark.png" alt="概览页（深色模式，演示数据）" /></td>
    </tr>
  </tbody>
</table>

### WorkBuddy 账号

账号卡片集中展示登录状态、签到状态、积分余额与临期资源，卡片底部是 5 个程序位按钮；工具栏集搜索、排序、分组、卡片 / 表格视图切换、回收站、全选与「添加账号」于一行。

<table>
  <thead>
    <tr>
      <th>浅色模式</th>
      <th>深色模式</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/workbuddy-accounts-light.png" alt="WorkBuddy 账号页（浅色模式，演示数据）" /></td>
      <td><img src="docs/images/workbuddy-accounts-dark.png" alt="WorkBuddy 账号页（深色模式，演示数据）" /></td>
    </tr>
  </tbody>
</table>

### 用量

Token 与积分统计合并到一个工作台：一级四视图（趋势 / 模型 / 会话 / 积分），参数条只有一条——统计范围与「仅本作用域 / 合并全部作用域」。

**趋势视图**（统计卡 + 逐日堆叠柱状图 + 累计消耗曲线等）：

<table>
  <thead>
    <tr>
      <th>浅色模式</th>
      <th>深色模式</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/usage-trend-light.png" alt="用量页·趋势视图（浅色模式，演示数据）" /></td>
      <td><img src="docs/images/usage-trend-dark.png" alt="用量页·趋势视图（深色模式，演示数据）" /></td>
    </tr>
  </tbody>
</table>

**积分视图**（官方请求用量，含每日积分消耗、积分余额变化与积分到期分布）：

<table>
  <thead>
    <tr>
      <th>浅色模式</th>
      <th>深色模式</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/usage-credits-light.png" alt="用量页·积分视图（浅色模式，演示数据）" /></td>
      <td><img src="docs/images/usage-credits-dark.png" alt="用量页·积分视图（深色模式，演示数据）" /></td>
    </tr>
  </tbody>
</table>

### 会话（WorkBuddy）

同一份会话在各账号里的副本集中展示：谁落后、谁分叉、同步给谁；状态由内容指纹派生，同步是加法（只追加、不覆盖）。

<table>
  <thead>
    <tr>
      <th>浅色模式</th>
      <th>深色模式</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/sessions-light.png" alt="会话页（浅色模式，演示数据）" /></td>
      <td><img src="docs/images/sessions-dark.png" alt="会话页（深色模式，演示数据）" /></td>
    </tr>
  </tbody>
</table>

### 网关

七个分区（概览 / 密钥 / 模型 / 账号池 / 请求代理 / 请求日志 / 统一入口），概览页给出启用开关、监听地址与端口、接入地址与模型 / 账号池入口，下方是接入指引。

<table>
  <thead>
    <tr>
      <th>浅色模式</th>
      <th>深色模式</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/api-service-light.png" alt="网关页（浅色模式，演示数据）" /></td>
      <td><img src="docs/images/api-service-dark.png" alt="网关页（深色模式，演示数据）" /></td>
    </tr>
  </tbody>
</table>

### 设置

两个平台共用同一张九项子导航（通用 / 账号与登录 / 签到与积分 / 网关 / 计划任务 / 客户端 / 权限与诊断 / 日志 / 备份与恢复），分区由 URL 承载，不再是一屏长滚动。

<table>
  <thead>
    <tr>
      <th>浅色模式</th>
      <th>深色模式</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/settings-light.png" alt="设置页（浅色模式，演示数据）" /></td>
      <td><img src="docs/images/settings-dark.png" alt="设置页（深色模式，演示数据）" /></td>
    </tr>
  </tbody>
</table>

### TraeWork 账号

与 WorkBuddy 账号页**逐条同构**：同一套工具栏、同一套卡片骨架，差异只在数据与程序位（TraeWork / TraeCode / Trae）；侧栏不渲染「会话」。

<table>
  <thead>
    <tr>
      <th>浅色模式</th>
      <th>深色模式</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/trae-accounts-light.png" alt="TraeWork 账号页（浅色模式，演示数据）" /></td>
      <td><img src="docs/images/trae-accounts-dark.png" alt="TraeWork 账号页（深色模式，演示数据）" /></td>
    </tr>
  </tbody>
</table>

### TraeWork 用量

同一份用量工作台，Trae 侧只有三个视图（趋势 / 模型 / 积分）——缺数据源就不画空壳。

<table>
  <thead>
    <tr>
      <th>浅色模式</th>
      <th>深色模式</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/trae-usage-light.png" alt="TraeWork 用量页（浅色模式，演示数据）" /></td>
      <td><img src="docs/images/trae-usage-dark.png" alt="TraeWork 用量页（深色模式，演示数据）" /></td>
    </tr>
  </tbody>
</table>

> 以上截图取自演示运行（`VITE_DEMO_MODE=1`，请求层打桩），账号、积分与请求记录均为虚构数据。

## 数据与隐私

- 所有数据都保存在本机 `~/.buddy-switch/` 下，不上传到任何第三方服务
- 账号库：WorkBuddy 为 `accounts.json`（国内版）/ `accounts.global.json`（国际版）；Trae 为 `trae/checkin_accounts.json`（国内版）/ `trae/checkin_accounts.global.json`（国际版）
- 认证文件：WorkBuddy 为 `workbuddy-desktop.info` / `workbuddy-desktop-ai.info`
- 切换账号前会先备份原认证文件；Trae 切换登录态前会先保存快照；会话「统一到其他副本」覆盖前也会自动备份
- 删除账号**不直接销毁**：先移入回收站保留 30 天，期间可恢复；永久删除或清空才连带删除其凭据与快照
- 导出备份默认**不含凭据**（token 字段不会出现在文件里）；只有显式勾选并二次确认后才会写入明文 token，页面会同时警告该文件可被他人直接用于登录
- 网关 API Key 只保存前缀与哈希，明文仅在创建时返回一次；统一入口的 Key 同样只存 sha256 + 前 4 / 后 4 位（`gateway_unified_keys.json`），配置落在 `gateway_unified.json`
- 界面语言、主题等偏好只存在本机，不上传
- 仓库不提交本地数据；发布前会扫描 token 模式（`ghp_` / `npm_` / `gho_` 等）

## macOS 权限说明

切换账号需要写入 WorkBuddy 认证文件，macOS 要求授权「App 管理」（或「完全磁盘访问」）：

1. 首次切换报「无权限」时，点「打开系统设置」
2. 优先在 **App 管理** 里打开 BuddySwitch 开关；若没有，则去 **完全磁盘访问** 把 BuddySwitch 拖进带箭头的框
3. 授权后重启本应用生效；设置页「权限检测」可随时验证

> webui 模式：由启动服务的终端进程权限决定；若终端已授权完全磁盘访问则无需额外操作。

## 开发

环境要求、构建命令、发布流程与目录结构见 [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)。简要说明：

```bash
npm install
npm run tauri dev     # 桌面端开发模式
npm run build         # 构建前端（dist/）
npm run check:api     # 校验前端 API 契约与后端一致
```

仓库为 Rust workspace + Vite/React 前端：

```
crates/
  buddy-switch-core/     # 核心逻辑：账号、认证、OAuth、进程、切换、会话、签到、刷新、区域、Trae 模块、统一网关命名空间
  buddy-switch-gateway/  # OpenAI / Anthropic 兼容网关（**三套实现**：WorkBuddy / Trae / 统一入口，共用 host 与账号池内核）
  buddy-switch-server/   # HTTP server + CLI（axum API + rust-embed 内嵌前端）
src-tauri/               # 桌面宿主（Tauri command 薄包装 + 托盘 + 三个网关宿主）
src/                     # 前端：components / pages / lib（api.ts 双通道：Tauri invoke 或 HTTP fetch）
npm/                     # npm 包（**尚未发布**）：主包 @nextagentx/buddy-switch + 5 个平台分包
```

三个网关宿主各占一个端口（`57891` / `7864` / `57892`），路由的 `/v1/models`、`/v1/chat/completions` **逐字重叠** ⇒ 不能 merge 进同一个 Router（axum 同路径 merge 是**运行期 panic**）。这是「三个入口并存」而非「一个入口加前缀」的原因。

## 参与贡献

**欢迎一切形式的贡献，也欢迎 PR。** 提 Issue、补文档、修 Bug、加功能都算 —— 完整的贡献指引（能做什么、PR 自检清单、本仓库的几条代码约定）见 **[CONTRIBUTING.md](CONTRIBUTING.md)**（[English](CONTRIBUTING.en.md)）。

## 支持这个项目

如果 Buddy Switch 帮到了你，可以请作者喝杯饮料 ☕

<table>
  <thead>
    <tr>
      <th>微信支付</th>
      <th>支付宝</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/donate-wechat.png" alt="微信支付收款码" width="260" /></td>
      <td><img src="docs/images/donate-alipay.jpg" alt="支付宝收款码" width="260" /></td>
    </tr>
  </tbody>
</table>


## 致谢

本项目在实现过程中参考了以下开源项目，在此向作者致谢：

- [changexbc/workbuddy-switch](https://github.com/changexbc/workbuddy-switch) —— 借鉴了 WorkBuddy 账号数据格式与多账号切换的整体思路。
- [Sliverkiss/workbuddy2api](https://github.com/Sliverkiss/workbuddy2api) —— 借鉴了将 WorkBuddy 模型额度封装为 OpenAI 兼容接口、对外提供 API 服务的思路。

以上项目各自遵循其自身许可；本项目为独立实现，与上述项目之间不存在隶属、合作或背书关系。相关名称与商标归各自权利人所有。本项目拥有独立的代码库、发布通道与数据目录，不会替换、覆盖或改写其他项目的升级源或数据目录。

## 免责声明

使用本项目即表示你已完整阅读、理解并同意全部免责条款；如不同意，请立即停止使用并卸载。

完整条款见 **[docs/DISCLAIMER.md](docs/DISCLAIMER.md)**（[English](docs/DISCLAIMER.en.md)），共 6 条：非官方第三方工具、服务条款与合规自理、数据写入与备份、自动化行为风险、API 网关暴露风险、按「原样」提供。

## 许可

本项目采用 **[PolyForm Noncommercial License 1.0.0](./LICENSE)**：**个人非商业使用是允许的，商业使用不被授权**（商业用途须事先取得作者的书面授权）。

- 条款全文：[LICENSE](./LICENSE)
- 白话说明、允许与禁止的具体情形、常见问题、以及 2026-09-22 之前版本的 MIT 说明：[docs/LICENSING.md](docs/LICENSING.md)
- 如需商业授权，请通过本仓库 Issues 联系作者。

## Star 趋势

<a href="https://www.star-history.com/?repos=NextAgentX%2Ftrae-workbuddy-switch&type=date&legend=top-left">

 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=NextAgentX/trae-workbuddy-switch&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=NextAgentX/trae-workbuddy-switch&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=NextAgentX/trae-workbuddy-switch&type=date&legend=top-left" />
 </picture>
</a>
