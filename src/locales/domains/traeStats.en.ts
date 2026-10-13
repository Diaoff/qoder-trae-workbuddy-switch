/**
 * 文案域：**Trae 产品线 · 统计与 API 服务页**（英文）。
 *
 * ⚠️ 不写类型注解：键名合法性由 `src/locales/en.ts` 单点校验。域文件保持零依赖。
 * 键必须与 `traeStats.zh.ts` **一一对应**（缺哪条就回落中文）。
 */
export const en = {
  // =====================================================================
  // TraeTokenStatsPage.tsx — Usage · Token views (Trae section): trend / models
  //
  // ⚠️ The page header / sub-tabs / view-parameter bar are rendered by `UsagePage`,
  //    so this domain no longer carries title / subtitle / time-window / scope-bar keys.
  // ⚠️ Both views' stat tiles and chart titles reuse `wbStats.usage.trend.*` /
  //    `wbStats.usage.models.*` / `wbStats.usage.range.*` (the prototype's
  //    platform-agnostic usage labels) so both platforms read identically.
  // =====================================================================
  "trae.stats.token.loadFailed": "Cannot read token statistics",
  "trae.stats.token.retry": "Retry",
  "trae.stats.token.refresh": "Refresh",

  // ---- Data-source boundary ----
  "trae.stats.token.sourceTitle": "Data source",
  "trae.stats.token.sourceFallback": "Only calls that went through the local Trae gateway are counted.",

  // ---- Empty state ----
  "trae.stats.token.emptyTitle": "No gateway calls in the current window",
  "trae.stats.token.emptyBody": "Enable the gateway on the API service page and point your client's Base URL at it; calls will then be recorded here.",

  // ---- Stat tiles (labels come from the shared keys) ----
  "trae.stats.token.summaryAria": "Usage summary",
  "trae.stats.token.summary.activeAccounts": "Active accounts",

  // ---- Dimensions the platform cannot provide ----
  "trae.stats.token.unsupportedTitle": "Dimensions not supported by the platform",
  "trae.stats.token.unsupportedOnly": " ({on} only)",

  // ---- Log parse failures (honesty notice: incomplete numbers must be stated) ----
  "trae.stats.token.parseErrors": "{count} records in the log could not be parsed (missing timestamp) and were skipped.",

  // =====================================================================
  // TraeCreditsPage.tsx — Usage · Credits view (Trae section)
  //
  // ⚠️ Stat tiles and chart titles reuse `wbStats.usage.credits.*` (the prototype's
  //    platform-agnostic credits labels); only Trae-specific strings live here.
  // =====================================================================
  "trae.stats.credits.subtitle": "Remaining credits, daily change and history for each Trae account.",
  "trae.stats.credits.sync": "Sync credits",
  "trae.stats.credits.syncing": "Syncing",
  "trae.stats.credits.refreshDone": "Credits refreshed",
  "trae.stats.credits.refreshFailed": "Failed to refresh",
  "trae.stats.credits.loadFailed": "Cannot read Trae credits",
  "trae.stats.credits.overviewAria": "Credits overview",

  // ---- Stat-tile captions (prototype: check-in reward / net ±Y / earliest in D+N days) ----
  "trae.stats.credits.inHint": "Check-in rewards",
  "trae.stats.credits.netHint": "Net {net}",
  "trae.stats.credits.soonestHint": "Earliest in {days} days",
  "trae.stats.credits.noExpiryHint": "Nothing expires within 30 days",

  // ---- Daily credits flow (Trae has both sides ⇒ two-way bars; WB only has spend) ----
  "trae.stats.credits.dailyTitle": "Daily credits flow",
  "trae.stats.credits.dailyNote": "Up = check-in rewards　Down = chat spend",
  "trae.stats.credits.dailyAria": "Two-way bar chart of daily credits flow (rewards up, spend down)",
  "trae.stats.credits.expiryEmpty": "Nothing expires within the next 30 days.",

  // ---- The prototype's two donuts (WB has no data source ⇒ not rendered) ----
  "trae.stats.credits.accountShareTitle": "Token share by account",
  "trae.stats.credits.shareNote": "Cumulative",
  "trae.stats.credits.accountShareAria": "Donut chart of cumulative token share per account",
  "trae.stats.credits.accountShareEmpty": "No account has produced usage yet.",
  "trae.stats.credits.statusTitle": "Gateway status codes",
  "trae.stats.credits.statusNote": "Current window",
  "trae.stats.credits.statusAria": "Donut chart of gateway response status codes",
  "trae.stats.credits.statusEmpty": "No gateway requests in this scope.",

  // ---- Credits ledger ----
  "trae.stats.credits.flowNote": "Newest first",
  "trae.stats.credits.recordEarned": "Credits earned",
  "trae.stats.credits.recordSpent": "Credits spent",
  "trae.stats.credits.flowEmptyTitle": "No credits ledger yet",
  "trae.stats.credits.flowEmptyBody": "Run a check-in or “Sync credits” once and the daily credits changes will be listed here.",

  // ---- Empty states ----
  "trae.stats.credits.trendEmptyAccounts": "No account data yet. Add an account and the credits statistics show up here.",
  "trae.stats.credits.trendEmptyData": "No credits data yet. Run a check-in or “Sync credits” once to see the daily change.",

  // ---- Dimensions the platform cannot provide ----
  "trae.stats.credits.unsupportedTitle": "Dimensions not supported by the platform",
  "trae.stats.credits.unsupportedOnly": " ({on} only)",

  // =====================================================================
  // TraeApiServicePage.tsx — API service page
  // =====================================================================
  "trae.stats.api.title": "API service",
  "trae.stats.api.subtitle": "Serve Trae's model quota to local tools through an OpenAI-compatible API.",
  "trae.stats.api.loadFailed": "Cannot read gateway status",
  "trae.stats.api.retry": "Retry",
  "trae.stats.api.refresh": "Refresh",

  // ---- Gateway card ----
  "trae.stats.api.gateway.enable": "Enable API gateway",
  "trae.stats.api.gateway.desc": "Once enabled, local AI tools can call Trae models through the address below (upstream {upstream}).",
  "trae.stats.api.gateway.bindAddr": "Listen address",
  "trae.stats.api.gateway.bindLoopback": "127.0.0.1 (this machine only)",
  "trae.stats.api.gateway.bindLan": "0.0.0.0 (LAN)",
  "trae.stats.api.gateway.port": "Listen port",
  "trae.stats.api.gateway.statusLabel": "Status:",
  "trae.stats.api.gateway.running": "Running",
  "trae.stats.api.gateway.stopped": "Stopped",
  "trae.stats.api.gateway.openDataDir": "Open data directory",
  "trae.stats.api.gateway.noteLead": "The default port is 7864, deliberately clear of the WorkBuddy gateway (57891) — both serve ",
  "trae.stats.api.gateway.noteTail": ".",
  "trae.stats.api.gateway.lanWarning": "In LAN mode, any device on the same network that gets hold of a key can spend your Trae credits.",
  "trae.stats.api.gateway.lastError": "Most recent error",

  // ---- Endpoint card ----
  "trae.stats.api.endpoint.title": "Endpoint",
  "trae.stats.api.endpoint.baseUrl": "Base URL",
  "trae.stats.api.copy": "Copy",
  "trae.stats.api.endpoint.copied": "Base URL copied",
  "trae.stats.api.endpoint.keyPrefix": "Latest key prefix",
  "trae.stats.api.endpoint.noKey": "No usable key yet",

  // ---- Action feedback ----
  "trae.stats.api.toast.savedStarted": "Gateway saved and started",
  "trae.stats.api.toast.savedDisabled": "Gateway saved (listening disabled)",
  "trae.stats.api.toast.saveFailed": "Failed to save",
  "trae.stats.api.toast.portInvalid": "Invalid port",
  "trae.stats.api.toast.portRange": "Enter a whole number between 1 and 65535",
  "trae.stats.api.toast.logsCleared": "Logs cleared",
  "trae.stats.api.toast.clearFailed": "Failed to clear",
  "trae.stats.api.toast.unsupported": "Not supported on this platform",
  "trae.stats.api.toast.unsupportedDesc": "This capability is only available on Windows.",
  "trae.stats.api.toast.dirOpened": "Trae data directory opened",
  "trae.stats.api.toast.dirOpenFailed": "Failed to open the data directory",

  // ---- LAN risk confirmation ----
  "trae.stats.api.lanDialog.title": "Allow LAN access?",
  "trae.stats.api.lanDialog.desc": "Once listening on 0.0.0.0, any device on the same network — public Wi-Fi included — can spend your Trae credits as long as it holds an API key. Only enable this on a network you trust.",
  "trae.stats.api.lanDialog.cancel": "Cancel",
  "trae.stats.api.lanDialog.confirm": "I understand, enable it",
};
