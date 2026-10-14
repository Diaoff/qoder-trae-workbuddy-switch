/**
 * 文案域：**应用外壳**（英文）。
 *
 * ⚠️ 这里**不写类型注解**：键名合法性由组合入口 `src/locales/en.ts` 单点校验
 * （它把整体约束为 `Partial<Record<TranslationKey, string>>`）。域文件保持零依赖，
 * 既避免循环导入，也让「拼错键名」只报一处。
 *
 * 译文约定（全仓一致）：
 * - 术语与客户端一致：account / check-in / credits / token / gateway / profile。
 * - **产品名不译**（WorkBuddy / TraeWork / Buddy Switch）。
 * - 中文的顿号、书名号按英文习惯改为逗号与引号 —— 照搬标点是最常见的「机翻味」。
 * - 语态用祈使/陈述，不堆砌 Please；长度尽量控制在中文的两倍内，避免挤破 220px 侧栏。
 */
export const en = {
  // ---- App shell / sidebar ----
  "app.name": "Buddy Switch",
  "app.demoBadge": "Demo",
  "product.workbuddy": "WorkBuddy",
  "product.trae": "TraeWork",
  "product.switchAria": "Switch product",
  "product.navAria": "{product} navigation",

  "nav.overview": "Overview",
  /* Short labels, mirroring the prototype's `DESTINATIONS` (buddy-switch-prototype.html:1139-1146). */
  "nav.accounts": "Accounts",
  "nav.usage": "Usage",
  "nav.tokenStats": "Token usage",
  "nav.credits": "Credits",
  "nav.apiService": "Gateway",
  "nav.settings": "Settings",

  "sidebar.version": "Version",
  // Brand block tagline (prototype `.brand-text i`, `buddy-switch-prototype.html:163`).
  "sidebar.brand.tagline": "Multi-platform account manager",
  "sidebar.running": "{product} running",
  "sidebar.notRunning": "{product} not running",
  "sidebar.update": "Update",

  // ---- Top app bar (redesign L1 scope + global actions) ----
  "topbar.scope.aria": "Switch scope",
  "topbar.scope.menuLabel": "Scopes (platform × region)",
  "topbar.scope.sub.loggedIn": "Signed in as {name} · {n} accounts",
  "topbar.scope.sub.loggedOut": "Not signed in · {n} accounts",
  "topbar.scope.sub.missing": "Not detected · {n} accounts",
  // Appended when a scope has accounts needing action (prototype `scope-menu`).
  "topbar.scope.sub.attention": " · {n} need action",
  // Footer note of the scope menu (prototype `scope-menu`): what a scope switch actually affects.
  "topbar.scope.hint": "The scope decides which set of accounts you manage. Switching updates the sidebar, content and stats.",
  "topbar.run.running": "Running",
  "topbar.run.stopped": "Not running",
  "topbar.run.missing": "Not detected",
  "topbar.search": "Search accounts, pages, or actions",
  "topbar.refresh": "Refresh current scope",
  "topbar.theme": "Switch light/dark theme",
  "topbar.appMenu": "App settings",

  // ---- Page-head scope subtitles (redesign §3.2) ----
  // Read-only: the only scope switcher lives in the top bar (region is part of L1 scope).
  "pageHead.overview.signedIn": "{scope} · Signed in as {name}",
  "pageHead.overview.signedOut": "{scope} · Not signed in",
  "pageHead.accounts": "{scope} · {n} accounts",
  "pageHead.accounts.pending": "{scope} · {n} accounts · {m} need action",
  "pageHead.usage": "{scope} · Token and credit stats in one workbench",
  "pageHead.gateway": "{scope} · Expose the account pool as OpenAI / Anthropic compatible APIs",
  "pageHead.settings": "{scope} · Grouped by topic instead of one long scroll",
  "pageHead.sessions": "{scope} · One session with a copy in each account — who lags, who diverged, sync to whom",

  // ---- Platform capability labels (overview "Capabilities" card; from `Capability` in registry) ----
  "capability.checkin": "Auto check-in",
  "capability.travel": "Auto travel",
  "capability.credits": "Credits",
  "capability.oauth": "OAuth sign-in",
  "capability.gateway": "API gateway",
  "capability.autoRotate": "Auto rotate",
  "capability.sessionCopy": "Session copy",

  // ---- Command palette (⌘K, P6 §3.7) ----
  "palette.entry": "Search or jump",
  "palette.title": "Search or jump",
  "palette.description": "Search accounts, pages, and actions across scopes; press Enter to go.",
  "palette.aria": "Command palette",
  "palette.placeholder": "Search accounts, pages, or actions…",
  "palette.loading": "Loading accounts…",
  "palette.results": "{count} results",
  "palette.empty.title": "No matches",
  "palette.empty.description": "Try a different keyword, or clear the search to see everything.",
  "palette.empty.clear": "Clear search",
  "palette.group.page": "Pages",
  "palette.group.scope": "Scopes",
  "palette.group.account": "Accounts",
  "palette.group.action": "Actions",
  "palette.scope.current": "Current",
  "palette.account.inScope": "in {scope}",
  "palette.action.refresh": "Refresh current scope",
  "palette.action.addAccount": "Add account",
  "palette.action.openDataDir": "Open data folder",

  // ---- Keyboard shortcuts settings (P6 §3.1) ----
  "shortcuts.settings.title": "Keyboard shortcuts",
  "shortcuts.settings.label": "Enable scope shortcuts",
  "shortcuts.settings.description": "Use {mod} + 1…{n} to switch directly to a scope.",
  "shortcuts.settings.degraded": "More than {limit} scopes are available, so number shortcuts are disabled.",

  // ---- First-run hint (P6 §1.5) ----
  "firstRun.title": "Quick start",
  "firstRun.openPalette": "Press {shortcut} to open the command palette and search across scopes.",
  "firstRun.scopeShortcut": "Press {shortcut} to switch directly to a scope.",
  "firstRun.listSearch": "Press / on the accounts page to focus search.",
  "firstRun.dismiss": "Got it",

  // ---- Undo toast (P6 T04 §1.4) ----
  // Deletion is a soft delete (the account moves to the recycle bin) => "can undo".
  // Never "permanently deleted" (that is the `recycleBin.purge` semantics), and never
  // imply that a timeout means permanent deletion — the account stays in the recycle bin.
  "undo.deletedTitle": "Deleted — can undo",
  "undo.batchTitle": "{n} accounts deleted — can undo",
  "undo.action": "Undo",
  "undo.undone": "Restored",
  "undo.failedTitle": "Undo failed",

  // ---- App-level settings (settings page "General" + top-bar gear drawer; one implementation, two shells) ----
  "appSettings.drawer.more": "More",

  "appSettings.appearance.title": "Appearance",
  "appSettings.appearance.theme.label": "Theme",
  "appSettings.appearance.theme.description":
    "Use light, dark, or follow the system appearance",
  "appSettings.appearance.theme.aria": "Theme",
  "appSettings.theme.system": "System",
  "appSettings.theme.light": "Light",
  "appSettings.theme.dark": "Dark",

  "appSettings.language.title": "Language",
  "appSettings.language.label": "Interface language",
  "appSettings.language.description":
    "Applies immediately; the preference is stored on this device only.",
  "appSettings.language.zh": "简体中文",
  "appSettings.language.en": "English",

  "appSettings.startup.title": "Startup & tray",
  "appSettings.startup.silent.label": "Start minimized to tray at login",
  "appSettings.startup.silent.description":
    "Reflects the system login item directly; reopen from the tray later",
  "appSettings.startup.enabled": "Launch at login enabled",
  "appSettings.startup.disabled": "Launch at login disabled",
  "appSettings.tray.closeToTray.label": "Keep running in the tray when the window is closed",
  "appSettings.tray.closeToTray.description":
    "Closing the main window hides it to the tray and keeps running; turn this off to quit the app on close",
  "appSettings.tray.saveMismatch":
    "Setting not applied: the read-back value differs from the requested one. Try again.",

  "appSettings.update.title": "Updates",
  "appSettings.update.auto.label": "Check for updates automatically",
  "appSettings.update.auto.description":
    "Checks every 30 minutes; you can still check manually below when this is off",
  "appSettings.update.current": "Current version: ",
  "appSettings.update.source": "Public update source",
  "appSettings.update.openRelease": "Open GitHub Releases",
  "appSettings.update.openReleaseTitle": "Open GitHub Releases",
  "appSettings.update.proxy.label": "Update proxy",
  "appSettings.update.proxy.description":
    "Used only for GitHub update checks and installer downloads; leave empty to disable an explicit proxy.",
  "appSettings.update.proxy.placeholder": "e.g. http://127.0.0.1:7897",
  "appSettings.update.proxy.save": "Save proxy",
  "appSettings.update.proxy.saved": "Update proxy saved",
  "appSettings.update.proxy.cleared": "Update proxy disabled",
  "appSettings.update.proxy.invalid":
    "Invalid proxy address. Enter an HTTP/HTTPS URL, e.g. http://127.0.0.1:7897",
  "appSettings.update.check": "Check for updates",
  "appSettings.update.checkFailed": "Update check failed",
  "appSettings.update.foundTitle": "New version available",
  "appSettings.update.doneTitle": "Update check complete",
  "appSettings.update.foundDetail": "Version v{latest} is available (current v{current})",
  "appSettings.update.upToDate": "You are on the latest version v{current}",
  "appSettings.update.installNow": "Update now",

  // ---- Settings sub-nav (P7 §3.9: 9 items, the 9th is "Backup & restore") ----
  "settings.subnav.aria": "Settings sections",
  "settings.subnav.general": "General",
  "settings.subnav.account": "Accounts & sign-in",
  "settings.subnav.checkin": "Check-in & credits",
  "settings.subnav.gateway": "Gateway",
  "settings.subnav.schedule": "Scheduled tasks",
  "settings.subnav.client": "Client",
  "settings.subnav.diagnostics": "Permissions & diagnostics",
  "settings.subnav.logs": "Logs",
  "settings.subnav.backup": "Backup & restore",

  // ---- Gateway workspace subnav (PLAN §3.9: 6 items, shared by both product lines) ----
  "gateway.subnav.aria": "Gateway sections",
  "gateway.subnav.overview": "Overview",
  "gateway.subnav.keys": "Keys",
  "gateway.subnav.models": "Models",
  "gateway.subnav.pool": "Account pool",
  "gateway.subnav.proxy": "Request proxy",
  "gateway.subnav.logs": "Request logs",

  // ---- 统一网关（第三个产品线 · 跨作用域）----
  "gateway.subnav.unified": "Unified entry",
  "unifiedGateway.title": "Unified gateway",
  "unifiedGateway.running": "Running",
  "unifiedGateway.stopped": "Stopped",
  "unifiedGateway.crossScope": "This entry spans all scopes; the scope selector above does not affect it.",
  "unifiedGateway.summary": "{namespaces} namespaces · {keys} active keys · listening on {addr}",
  "unifiedGateway.enable": "Enable the unified gateway",
  "unifiedGateway.enableHint": "One namespaced entry — every platform and version through a single provider",
  "unifiedGateway.bindAddr": "Bind address",
  "unifiedGateway.bindLan": "Allow LAN",
  "unifiedGateway.bindLoopback": "Loopback only",
  "unifiedGateway.port": "Port",
  "unifiedGateway.portHint": "Restart required; the two existing gateways (57891 / 7864) are left untouched",
  "unifiedGateway.portInvalid": "Invalid port: enter an integer between 1 and 65535",
  "unifiedGateway.nsTitle": "Enabled namespaces",
  "unifiedGateway.nsHint": "Disabled ones are not parsed or listed, and calls return 400. Keep at least one enabled.",
  "unifiedGateway.showMore": "Show more program lines",
  "unifiedGateway.showLess": "Hide extra program lines",
  "unifiedGateway.keepOne": "Keep at least one namespace enabled",
  "unifiedGateway.nsOff": "Disabled on this entry — calls return 400",
  "unifiedGateway.keysTitle": "Unified keys",
  "unifiedGateway.createKey": "Create key",
  "unifiedGateway.noKeys": "No unified keys yet",
  "unifiedGateway.noKeysHint": "One key can authorize several namespaces at once (e.g. WorkBuddy CN plus Trae CN).",
  "unifiedGateway.colName": "Name",
  "unifiedGateway.colNamespaces": "Authorized namespaces",
  "unifiedGateway.colMasked": "Key",
  "unifiedGateway.colCreated": "Created",
  "unifiedGateway.colLastUsed": "Last used",
  "unifiedGateway.colStatus": "Status",
  "unifiedGateway.colActions": "Actions",
  "unifiedGateway.neverUsed": "Never used",
  "unifiedGateway.revoked": "Revoked",
  "unifiedGateway.active": "Active",
  "unifiedGateway.restore": "Restore",
  "unifiedGateway.delete": "Delete",
  "unifiedGateway.revoke": "Revoke",
  "unifiedGateway.loadFailed": "Failed to load the unified gateway config",
  "unifiedGateway.saveFailed": "Save failed",
  "unifiedGateway.created": "Key created",
  "unifiedGateway.createdHint": "The plaintext is shown only once (the store keeps sha256 only). Copy it now — it cannot be retrieved later.",
  "unifiedGateway.copy": "Copy",
  "unifiedGateway.savedClose": "I've saved it, close",
  "unifiedGateway.keyName": "Name",
  "unifiedGateway.keyNamePlaceholder": "e.g. My main key",
  "unifiedGateway.authorize": "Authorized namespaces (at least one)",
  "unifiedGateway.authorizeRequired": "Select at least one. Disabled namespaces may be authorized too — they take effect once enabled.",
  "unifiedGateway.bizTitle": "Business-forwarding namespace",
  "unifiedGateway.bizNone": "No business forwarding",
  "unifiedGateway.bizHint": "Business requests carry no model name, so the namespace can only come from the key; Trae namespaces are invalid.",
  "unifiedGateway.cancel": "Cancel",
  "unifiedGateway.create": "Create",
  "unifiedGateway.unnamed": "Untitled",
  "unifiedGateway.previewTitle": "Namespaces and model preview",
  "unifiedGateway.previewTotal": "{count} models in total",
  "unifiedGateway.previewEmpty": "No namespace is enabled — pick one under \"Enabled namespaces\" above.",
  "unifiedGateway.previewCount": "{count} models",
  "unifiedGateway.showModels": "Show models",
  "unifiedGateway.hideModels": "Hide",
  "unifiedGateway.previewNoModels": "No catalog for this segment yet",
  "unifiedGateway.source.live": "Live",
  "unifiedGateway.source.cached": "Cached",
  "unifiedGateway.source.builtin": "Built-in",

  // ---- Settings "Logs" section ----
  // ⚠️ Corrected after measuring on 2026-10-14: WorkBuddy DOES keep its own runtime logs
  //    (`~/.workbuddy/logs/`: per-day folders plus `automation.log`). The earlier wording
  //    pointed users at the gateway page, which shows THIS tool's request log — a different thing.
  "settings.logs.emptyTitle": "No runtime log in this section",
  "settings.logs.emptyBody":
    "This section only holds this tool's settings and backup entries; WorkBuddy's own runtime logs live in a logs subdirectory of its data dir (not surfaced here yet).",
  "settings.logs.emptyAction": "Open gateway settings",

  // ---- Shared fallbacks ----
  "common.unknownError": "Unknown error",

  // ---- Top-level error boundary (see components/error-boundary.tsx) ----
  "app.error.title": "Something went wrong",
  "app.error.description":
    "A rendering error was caught. The UI stopped here to avoid showing a garbled screen. Copy the details below and report them; reloading usually gets you back in.",
  "app.error.reload": "Reload",
  "app.error.copy": "Copy error details",
  "app.error.copied": "Copied",
};
