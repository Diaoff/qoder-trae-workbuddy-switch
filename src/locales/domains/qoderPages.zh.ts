/**
 * 文案域：**Qoder 产品线 · 页面**（中文，键的权威之一）。
 *
 * 归属文件：
 * `src/pages/QoderAccountsPage.tsx`、`src/pages/QoderPendingPage.tsx`、
 * `src/pages/QoderUsagePending.tsx`
 *
 * ⚠️ 命名空间用 `qoder`（与 `trae` 同款：页面与组件共用一个前缀、靠二级段区分），
 * 键前缀：`qoder.page.`
 */
export const zh = {
  "qoder.page.accounts.title": "账号",
  // ⚠️ 页头副标题走 `shared` 域的 `pageHead.accounts` / `pageHead.accounts.pending`
  //    （「{scope} · {n} 个账号 · {m} 个需处理」）—— 与 WB / Trae 同一公式，不另立门户。

  "qoder.page.accounts.sectionAria": "Qoder 账号列表（{count} 个账号）",
  "qoder.page.accounts.loadFailed": "账号列表加载失败",
  "qoder.page.accounts.recheck": "重新检测",

  "qoder.page.accounts.empty.title": "这个区域还没有 Qoder 账号",
  "qoder.page.accounts.empty.description":
    "可以从本机已登录的 Qoder 客户端导入，或粘贴一个 PAT。",
  "qoder.page.accounts.empty.action": "添加账号",
  "qoder.page.accounts.empty.sources": "本机来源",

  "qoder.page.accounts.import.description":
    "只读探测桌面版与 CLI 的登录态。不会修改客户端里的任何文件。",
  "qoder.page.accounts.import.result": "新增 {imported} 个，更新 {updated} 个",
  "qoder.page.accounts.import.none": "两处来源都没有可导入的账号",
  "qoder.page.accounts.addDrawer.notInstalled":
    "两处本机来源都未检测到。可以先用官方客户端登录后导入，或直接粘贴 PAT。",

  "qoder.page.accounts.source.desktop": "Qoder 桌面版",
  "qoder.page.accounts.source.cli": "Qoder CLI",
  "qoder.page.accounts.source.missing": "未检测到",
  "qoder.page.accounts.source.unreadable": "读不出来",
  "qoder.page.accounts.source.empty": "里面没有账号",
  "qoder.page.accounts.source.ready": "可导入",

  "qoder.page.accounts.pat.title": "粘贴 PAT",
  "qoder.page.accounts.pat.description":
    "个人访问令牌（以 pt- 开头）。适合没有装客户端、或客户端登录态读不出来的情况。",
  "qoder.page.accounts.pat.placeholder": "pt-…",
  "qoder.page.accounts.pat.action": "导入",
  "qoder.page.accounts.pat.invalid": "前缀不对：PAT 应以 pt- 开头",
  "qoder.page.accounts.pat.cancel": "取消",

  // ---- 卡片 / 表格 / 工具栏（原型 `accountCard` / `tableView` / `viewAccounts` 的槽位） ----
  "qoder.page.accounts.chip.ok": "凭据可用",
  "qoder.page.accounts.chip.noCredential": "无可用凭据",
  "qoder.page.accounts.chip.unrecognized": "凭据未识别",
  "qoder.page.accounts.chip.foreign": "归属异常",
  "qoder.page.accounts.chip.foreignTip": "这条账号的归属域名不属于当前区域（可能被错误导入）",

  "qoder.page.accounts.card.manageAria": "{name} 的更多操作",
  "qoder.page.accounts.card.moreActions": "更多操作",
  "qoder.page.accounts.card.viewDetail": "查看详情",
  "qoder.page.accounts.card.editRemark": "编辑备注",
  "qoder.page.accounts.card.deleteAccount": "删除账号",
  "qoder.page.accounts.card.selectAria": "选择 {name}",

  "qoder.page.accounts.toolbar.sort.name": "名称",
  "qoder.page.accounts.toolbar.sort.recent": "最近更新",
  "qoder.page.accounts.toolbar.group.none": "不分组",
  "qoder.page.accounts.toolbar.group.status": "按状态",
  "qoder.page.accounts.toolbar.statusBucket.normal": "正常",
  "qoder.page.accounts.toolbar.statusBucket.noCredential": "无可用凭据",
  "qoder.page.accounts.toolbar.statusBucket.unrecognized": "凭据未识别",

  "qoder.page.accounts.table.account": "账号",
  "qoder.page.accounts.table.status": "状态",
  "qoder.page.accounts.table.source": "来源",
  "qoder.page.accounts.table.remark": "备注",

  // ---- 详情抽屉（原型 `drawerAccount`：账号信息 + 备注**在抽屉里**编辑） ----
  "qoder.page.accounts.drawer.subtitle": "{name} · UID {uid}",
  "qoder.page.accounts.drawer.info": "账号信息",
  "qoder.page.accounts.drawer.uid": "UID",
  "qoder.page.accounts.drawer.domain": "域名",
  "qoder.page.accounts.drawer.source": "来源",
  "qoder.page.accounts.drawer.addedAt": "添加时间",
  "qoder.page.accounts.drawer.updatedAt": "最近更新",
  "qoder.page.accounts.drawer.credential": "凭据",
  "qoder.page.accounts.drawer.accessToken": "访问令牌",

  // ---- 删除（Qoder 是**硬删除**、没有回收站 ⇒ 文案不许抄「可在回收站恢复」） ----
  "qoder.page.accounts.removed": "已删除 {name}",
  "qoder.page.accounts.domainMismatch": "归属不属于当前区域",
  "qoder.page.accounts.delete.title": "删除账号",
  "qoder.page.accounts.delete.desc": "确定删除「{name}」？Qoder 侧是永久删除，没有回收站。",
  "qoder.page.accounts.delete.batchTitle": "批量删除账号",
  "qoder.page.accounts.delete.batchDesc": "确定删除选中的 {n} 个账号？Qoder 侧是永久删除，没有回收站。",
  "qoder.page.accounts.delete.confirm": "删除",
  "qoder.page.accounts.delete.cancel": "取消",
  "qoder.page.accounts.delete.done": "已删除 {n} 个账号",
  "qoder.page.accounts.delete.partial": "已删除 {ok} 个，{fail} 个失败",

  "qoder.page.accounts.remark.saved": "备注已保存",
  "qoder.page.accounts.remark.missing": "这条账号已不在库里 —— 备注未保存",

  // 来源 id 是**后端数据**（`qoder::account::SOURCE_*`）；表外取值原样显示。
  "qoder.page.accounts.origin.pat": "PAT 导入",
  "qoder.page.accounts.origin.desktopApp": "桌面版导入",
  "qoder.page.accounts.origin.cli": "CLI 导入",
  "qoder.page.accounts.origin.oauth": "设备码登录",

  "qoder.page.pending.title": "这个页面尚未接入 Qoder",
  "qoder.page.pending.description":
    "Qoder 平台的账号管理、概览与设置已经可用；用量 / 网关依赖网关请求日志与用量台账，尚未落地。",
  "qoder.page.pending.action": "去账号页",

  "qoder.page.usage.pending.title": "Qoder 用量尚未接入",
  "qoder.page.usage.pending.description":
    "逐日 Token 与额度都来自网关请求日志与本地额度缓存，两者尚未落地 ⇒ 这里不画空壳。",
  "qoder.page.usage.pending.action": "去账号页",

  // ---- 概览 ----
  "qoder.page.overview.title": "概览",
  "qoder.page.overview.subtitle": "{scope} · 账号与本机客户端；用量与网关尚未落地",
  "qoder.page.overview.error": "读取失败：{error}",
  "qoder.page.overview.kpi.aria": "Qoder 概览统计",
  "qoder.page.overview.kpi.accounts": "账号",
  "qoder.page.overview.kpi.accountsHint": "全部归属本区域",
  "qoder.page.overview.kpi.accountsForeign": "其中 {n} 条归属不属于本区域",
  "qoder.page.overview.kpi.accountsTitle": "本区域账号库里的条数",
  "qoder.page.overview.kpi.clients": "本机客户端",
  "qoder.page.overview.kpi.clientsReady": "有可导入的登录态",
  "qoder.page.overview.kpi.clientsNone": "两处都没检测到",
  "qoder.page.overview.kpi.clientsHint": "桌面版 / CLI 的探测结果",
  "qoder.page.overview.kpi.clientsTitle": "两处官方凭据存储里「读到了可用账号」的来源数",
  "qoder.page.overview.kpi.domain": "区域域名",
  "qoder.page.overview.kpi.domainHint": "账号库与请求都按它分家",
  "qoder.page.overview.kpi.domainTitle": "该区域的官方域名 —— 双区域是两本互不相通的库",
  "qoder.page.overview.kpi.usage": "今日用量",
  "qoder.page.overview.kpi.usageHint": "网关未接入 ⇒ 没有数据源",
  "qoder.page.overview.kpi.usageTitle":
    "逐日 Token 与额度来自网关请求日志与本地额度缓存，两者尚未落地；未知用「—」而不是 0",
  "qoder.page.overview.clients.title": "本机凭据来源",
  "qoder.page.overview.clients.description": "只读探测，不落库、不改客户端里的任何文件。",
  "qoder.page.overview.clients.action": "去账号页",
  "qoder.page.overview.accounts.title": "账号",
  "qoder.page.overview.accounts.description": "按区域分家的两本账号库之一。",
  "qoder.page.overview.accounts.action": "去账号页",
  "qoder.page.overview.accounts.more": "另有 {n} 个",
  "qoder.page.overview.accounts.empty": "这个区域还没有账号 —— 去账号页从本机导入或粘贴 PAT。",
  "qoder.page.overview.pending.title": "用量与网关尚未落地",
  "qoder.page.overview.pending.description":
    "逐日 Token、额度与请求日志都来自网关；网关未接入前这里不摆空壳 KPI。",
  "qoder.page.overview.pending.action": "去账号页",

  // ---- 设置 ----
  "qoder.page.settings.title": "设置",
  "qoder.page.settings.section.client": "客户端检测",
  "qoder.page.settings.client.description": "本机两处官方凭据存储的探测结果（只读，不落库）。",
  "qoder.page.settings.client.error": "探测失败：{error}",
  "qoder.page.settings.filtered":
    "这里只列已有内容的分区；客户端路径 / 签到 / 网关 / 计划任务等设置项会随对应功能落地后出现（不预先摆空壳）。",
} as const;
