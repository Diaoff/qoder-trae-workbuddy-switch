/**
 * 文案域：**Trae 产品线 · 统计与 API 服务页**（中文，键的权威之一）。
 *
 * 归属文件：
 * `src/pages/TraeTokenStatsPage.tsx`、`src/pages/TraeCreditsPage.tsx`、
 * `src/pages/TraeApiServicePage.tsx`
 *
 * ⚠️ 命名空间用 `trae`（不是 `traeStats`）：与另两个 Trae 域共用前缀、靠二级段区分
 * （`trae.page.*` / `trae.comp.*` / `trae.gateway.*` / `trae.stats.*`），
 * 因为它们本就是同一产品线的文案。
 *
 * 键前缀：`trae.stats.`
 */
export const zh = {
  // =====================================================================
  // TraeTokenStatsPage.tsx —— 「用量 · Token」视图（Trae 分区）：趋势 / 模型
  //
  // ⚠️ 页头 / 子 Tab / 视图参数条由 `UsagePage` 渲染，故本域**不再有** title / subtitle /
  //    时间窗口 / 版本范围条词条（那些控件已删除或上移）。
  // ⚠️ 两个视图的**统计卡与图表标题**全部走 `wbStats.usage.trend.*` / `wbStats.usage.models.*`
  //    / `wbStats.usage.range.*` —— 那是**原型用量视图的公共标签**（原型平台无关），
  //    两平台共用同一批键才能逐字一致。
  // =====================================================================
  "trae.stats.token.loadFailed": "无法读取 Token 统计",
  "trae.stats.token.retry": "重试",
  "trae.stats.token.refresh": "刷新",

  // ---- 数据源边界提示 ----
  "trae.stats.token.sourceTitle": "数据来源",
  "trae.stats.token.sourceFallback": "只统计经过本机 Trae 网关的调用。",

  // ---- 空态 ----
  "trae.stats.token.emptyTitle": "当前窗口内没有任何网关调用",
  "trae.stats.token.emptyBody": "到「API 服务」页启用网关，再把客户端的 Base URL 指过来，调用就会记在这里。",

  // ---- 统计卡 ----
  // 标签走公共键；这里只有 aria 与 Trae 独有的第 4 枚（原型是「活跃账号」，
  // WB 无账号维度才用「模型数」替代 —— 见页面注释）。
  "trae.stats.token.summaryAria": "用量汇总统计",
  "trae.stats.token.summary.activeAccounts": "活跃账号",

  // ---- 平台不支持的维度 ----
  "trae.stats.token.unsupportedTitle": "平台不支持的维度",
  "trae.stats.token.unsupportedOnly": "（仅 {on}）",

  // ---- 日志解析失败（**诚实性提示**：数字不完整时必须说）----
  "trae.stats.token.parseErrors": "日志中有 {count} 条记录无法解析（缺少时间戳），已跳过。",

  // =====================================================================
  // TraeCreditsPage.tsx —— 「用量 · 积分」视图（Trae 分区）
  //
  // ⚠️ 统计卡与图表标题尽量走 `wbStats.usage.credits.*`（原型积分视图的公共标签）；
  //    本域只留 Trae 独有的部分（双向柱的标题、两个环形、流水说明）。
  // =====================================================================
  "trae.stats.credits.subtitle": "查看每个 Trae 账号的剩余积分、每日变化与历史趋势。",
  "trae.stats.credits.sync": "同步积分",
  "trae.stats.credits.syncing": "同步中",
  "trae.stats.credits.refreshDone": "积分数据已刷新",
  "trae.stats.credits.refreshFailed": "刷新失败",
  "trae.stats.credits.loadFailed": "无法读取 Trae 积分数据",
  "trae.stats.credits.overviewAria": "积分总览",

  // ---- 统计卡的三条副标题（原型：签到奖励 / 净 ±Y / 最早 D+N 天后）----
  "trae.stats.credits.inHint": "签到入账",
  "trae.stats.credits.netHint": "净 {net}",
  "trae.stats.credits.soonestHint": "最早 {days} 天后",
  "trae.stats.credits.noExpiryHint": "30 天内无到期",

  // ---- 每日积分收支（Trae **两侧都有** ⇒ 双向柱；WB 只有消耗侧）----
  "trae.stats.credits.dailyTitle": "每日积分收支",
  "trae.stats.credits.dailyNote": "上 = 签到入账　下 = 对话消耗",
  "trae.stats.credits.dailyAria": "每日积分收支双向柱状图（上为入账、下为消耗）",
  "trae.stats.credits.expiryEmpty": "未来 30 天内没有积分到期。",

  // ---- 原型积分页的两个环形（WB 侧无数据源 ⇒ 不渲染）----
  "trae.stats.credits.accountShareTitle": "账号 Token 占比",
  "trae.stats.credits.shareNote": "累计",
  "trae.stats.credits.accountShareAria": "各账号累计 Token 占比环形图",
  "trae.stats.credits.accountShareEmpty": "还没有账号产生用量。",
  "trae.stats.credits.statusTitle": "网关状态码分布",
  "trae.stats.credits.statusNote": "当前窗口",
  "trae.stats.credits.statusAria": "网关请求状态码分布环形图",
  "trae.stats.credits.statusEmpty": "本作用域没有网关请求。",

  // ---- 积分流水 ----
  "trae.stats.credits.flowNote": "按日期倒序",
  "trae.stats.credits.recordEarned": "积分入账",
  "trae.stats.credits.recordSpent": "积分消耗",
  "trae.stats.credits.flowEmptyTitle": "暂无积分流水",
  "trae.stats.credits.flowEmptyBody": "执行一次签到或「同步积分」后，这里会按天记录积分变动。",

  // ---- 空态 ----
  "trae.stats.credits.trendEmptyAccounts": "尚无账号数据。添加账号后这里会展示积分统计。",
  "trae.stats.credits.trendEmptyData": "暂无积分数据。执行一次签到或「同步积分」后即可看到每日变化。",

  // ---- 平台不支持的维度 ----
  "trae.stats.credits.unsupportedTitle": "平台不支持的维度",
  "trae.stats.credits.unsupportedOnly": "（仅 {on}）",

  // =====================================================================
  // TraeApiServicePage.tsx —— 「API 服务」页
  // =====================================================================
  "trae.stats.api.title": "API 服务",
  "trae.stats.api.subtitle": "把 Trae 的模型额度以 OpenAI 兼容接口提供给本机工具。",
  "trae.stats.api.loadFailed": "无法读取网关状态",
  "trae.stats.api.retry": "重试",
  "trae.stats.api.refresh": "刷新",

  // ---- 网关卡 ----
  "trae.stats.api.gateway.enable": "启用 API 网关",
  "trae.stats.api.gateway.desc": "开启后本机 AI 工具可通过下方地址调用 Trae 模型（上游 {upstream}）。",
  "trae.stats.api.gateway.bindAddr": "监听地址",
  "trae.stats.api.gateway.bindLoopback": "127.0.0.1（仅本机）",
  "trae.stats.api.gateway.bindLan": "0.0.0.0（局域网）",
  "trae.stats.api.gateway.port": "监听端口",
  "trae.stats.api.gateway.statusLabel": "状态：",
  "trae.stats.api.gateway.running": "运行中",
  "trae.stats.api.gateway.stopped": "已停止",
  "trae.stats.api.gateway.openDataDir": "打开数据目录",
  "trae.stats.api.gateway.noteLead": "默认端口 7864，与 WorkBuddy 网关（57891）错开——两者都占用 ",
  "trae.stats.api.gateway.noteTail": "。",
  "trae.stats.api.gateway.lanWarning": "局域网模式下，同网段任何设备拿到 Key 都能消耗你的 Trae 积分。",
  "trae.stats.api.gateway.lastError": "最近一次错误",

  // ---- 接入地址卡 ----
  "trae.stats.api.endpoint.title": "接入地址",
  "trae.stats.api.endpoint.baseUrl": "Base URL",
  "trae.stats.api.copy": "复制",
  "trae.stats.api.endpoint.copied": "Base URL 已复制",
  "trae.stats.api.endpoint.keyPrefix": "最近 Key 前缀",
  "trae.stats.api.endpoint.noKey": "尚无可用 Key",

  // ---- 操作结果提示 ----
  "trae.stats.api.toast.savedStarted": "网关已保存并启动",
  "trae.stats.api.toast.savedDisabled": "网关已保存（未启用监听）",
  "trae.stats.api.toast.saveFailed": "保存失败",
  "trae.stats.api.toast.portInvalid": "端口无效",
  "trae.stats.api.toast.portRange": "请输入 1–65535 之间的整数",
  "trae.stats.api.toast.logsCleared": "日志已清空",
  "trae.stats.api.toast.clearFailed": "清空失败",
  "trae.stats.api.toast.unsupported": "当前平台不支持",
  "trae.stats.api.toast.unsupportedDesc": "该能力仅在 Windows 提供。",
  "trae.stats.api.toast.dirOpened": "已打开 Trae 数据目录",
  "trae.stats.api.toast.dirOpenFailed": "打开数据目录失败",

  // ---- 局域网风险确认 ----
  "trae.stats.api.lanDialog.title": "允许局域网访问？",
  "trae.stats.api.lanDialog.desc": "监听 0.0.0.0 后，同网段（含公共 Wi-Fi）的任何设备只要拿到 API Key，就能消耗你的 Trae 积分。请只在可信网络下开启。",
  "trae.stats.api.lanDialog.cancel": "取消",
  "trae.stats.api.lanDialog.confirm": "我已确认，开启",
} as const;
