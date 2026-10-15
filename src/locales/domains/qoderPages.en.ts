/** 文案域：Qoder 产品线 · 页面（英文）。键与 `qoderPages.zh.ts` 一一对应。 */
export const en = {
  "qoder.page.accounts.title": "Accounts",
  // The page-head subtitle uses the shared `pageHead.accounts` formula
  // ("{scope} · {n} accounts · {m} need attention") — same as WB / Trae.

  "qoder.page.accounts.sectionAria": "Qoder account list ({count} accounts)",
  "qoder.page.accounts.loadFailed": "Failed to load the account list",
  "qoder.page.accounts.recheck": "Check again",

  "qoder.page.accounts.empty.title": "No Qoder account in this realm yet",
  "qoder.page.accounts.empty.description":
    "Import from a signed-in Qoder client on this machine, or paste a PAT.",
  "qoder.page.accounts.empty.action": "Add account",
  "qoder.page.accounts.empty.sources": "Sources on this machine",

  "qoder.page.accounts.import.description":
    "Read-only probe of the desktop app and CLI sessions. No client file is modified.",
  "qoder.page.accounts.import.result": "{imported} added, {updated} updated",
  "qoder.page.accounts.import.none": "Neither source has an importable account",
  "qoder.page.accounts.addDrawer.notInstalled":
    "Neither source was detected. Sign in with the official client first, or paste a PAT directly.",

  "qoder.page.accounts.source.desktop": "Qoder desktop",
  "qoder.page.accounts.source.cli": "Qoder CLI",
  "qoder.page.accounts.source.missing": "Not found",
  "qoder.page.accounts.source.unreadable": "Unreadable",
  "qoder.page.accounts.source.empty": "No account inside",
  "qoder.page.accounts.source.ready": "Importable",

  "qoder.page.accounts.pat.title": "Paste a PAT",
  "qoder.page.accounts.pat.description":
    "Personal access token (starts with pt-). For machines without a client, or when the client session cannot be read.",
  "qoder.page.accounts.pat.placeholder": "pt-…",
  "qoder.page.accounts.pat.action": "Import",
  "qoder.page.accounts.pat.invalid": "Wrong prefix: a PAT starts with pt-",
  "qoder.page.accounts.pat.cancel": "Cancel",

  // ---- Card / table / toolbar (slots of `accountCard` / `tableView` / `viewAccounts`) ----
  "qoder.page.accounts.chip.ok": "Credential usable",
  "qoder.page.accounts.chip.noCredential": "No usable credential",
  "qoder.page.accounts.chip.unrecognized": "Credential unrecognized",
  "qoder.page.accounts.chip.foreign": "Wrong realm",
  "qoder.page.accounts.chip.foreignTip":
    "This account's domain does not belong to the current realm (possibly imported by mistake)",

  "qoder.page.accounts.card.manageAria": "More actions for {name}",
  "qoder.page.accounts.card.moreActions": "More actions",
  "qoder.page.accounts.card.viewDetail": "View details",
  "qoder.page.accounts.card.editRemark": "Edit remark",
  "qoder.page.accounts.card.deleteAccount": "Delete account",
  "qoder.page.accounts.card.selectAria": "Select {name}",

  "qoder.page.accounts.toolbar.sort.name": "Name",
  "qoder.page.accounts.toolbar.sort.recent": "Recently updated",
  "qoder.page.accounts.toolbar.group.none": "No grouping",
  "qoder.page.accounts.toolbar.group.status": "By status",
  "qoder.page.accounts.toolbar.statusBucket.normal": "Normal",
  "qoder.page.accounts.toolbar.statusBucket.noCredential": "No usable credential",
  "qoder.page.accounts.toolbar.statusBucket.unrecognized": "Credential unrecognized",

  "qoder.page.accounts.table.account": "Account",
  "qoder.page.accounts.table.status": "Status",
  "qoder.page.accounts.table.source": "Source",
  "qoder.page.accounts.table.remark": "Remark",

  // ---- Detail drawer (`drawerAccount`: account info + remark edited **inside the drawer**) ----
  "qoder.page.accounts.drawer.subtitle": "{name} · UID {uid}",
  "qoder.page.accounts.drawer.info": "Account info",
  "qoder.page.accounts.drawer.uid": "UID",
  "qoder.page.accounts.drawer.domain": "Domain",
  "qoder.page.accounts.drawer.source": "Source",
  "qoder.page.accounts.drawer.addedAt": "Added at",
  "qoder.page.accounts.drawer.updatedAt": "Last updated",
  "qoder.page.accounts.drawer.credential": "Credential",
  "qoder.page.accounts.drawer.accessToken": "Access token",

  // ---- Deletion (Qoder deletes **permanently**, no recycle bin ⇒ never claim "restorable") ----
  "qoder.page.accounts.removed": "Deleted {name}",
  "qoder.page.accounts.domainMismatch": "Belongs to a different realm",
  "qoder.page.accounts.delete.title": "Delete account",
  "qoder.page.accounts.delete.desc": "Delete “{name}”? Deletion on Qoder is permanent — there is no recycle bin.",
  "qoder.page.accounts.delete.batchTitle": "Delete selected accounts",
  "qoder.page.accounts.delete.batchDesc": "Delete the {n} selected accounts? Deletion on Qoder is permanent — there is no recycle bin.",
  "qoder.page.accounts.delete.confirm": "Delete",
  "qoder.page.accounts.delete.cancel": "Cancel",
  "qoder.page.accounts.delete.done": "Deleted {n} accounts",
  "qoder.page.accounts.delete.partial": "Deleted {ok}, {fail} failed",

  "qoder.page.accounts.remark.saved": "Remark saved",
  "qoder.page.accounts.remark.missing": "That account is no longer in the store — remark not saved",

  // Source ids are backend data (`qoder::account::SOURCE_*`); unknown values render as-is.
  "qoder.page.accounts.origin.pat": "Imported via PAT",
  "qoder.page.accounts.origin.desktopApp": "Imported from desktop app",
  "qoder.page.accounts.origin.cli": "Imported from CLI",
  "qoder.page.accounts.origin.oauth": "Device-code sign-in",

  "qoder.page.pending.title": "This page is not wired for Qoder yet",
  "qoder.page.pending.description":
    "Account management, overview and settings are available for Qoder; usage / gateway depend on gateway request logs and the usage ledger, which are not landed yet.",
  "qoder.page.pending.action": "Go to accounts",

  "qoder.page.usage.pending.title": "Qoder usage is not wired yet",
  "qoder.page.usage.pending.description":
    "Daily tokens and credits both come from gateway request logs and a local credit cache; neither exists yet, so this page renders no empty shell.",
  "qoder.page.usage.pending.action": "Go to accounts",

  // ---- Overview ----
  "qoder.page.overview.title": "Overview",
  "qoder.page.overview.subtitle": "{scope} · accounts and local clients; usage and gateway not landed yet",
  "qoder.page.overview.error": "Failed to load: {error}",
  "qoder.page.overview.kpi.aria": "Qoder overview statistics",
  "qoder.page.overview.kpi.accounts": "Accounts",
  "qoder.page.overview.kpi.accountsHint": "All belong to this realm",
  "qoder.page.overview.kpi.accountsForeign": "{n} of them belong to another realm",
  "qoder.page.overview.kpi.accountsTitle": "Number of entries in this realm's account store",
  "qoder.page.overview.kpi.clients": "Local clients",
  "qoder.page.overview.kpi.clientsReady": "Has an importable session",
  "qoder.page.overview.kpi.clientsNone": "Neither location detected",
  "qoder.page.overview.kpi.clientsHint": "Probe result for desktop / CLI",
  "qoder.page.overview.kpi.clientsTitle":
    "How many of the two official credential stores hold a usable account",
  "qoder.page.overview.kpi.domain": "Realm domain",
  "qoder.page.overview.kpi.domainHint": "Account store and requests are split by it",
  "qoder.page.overview.kpi.domainTitle":
    "The official domain of this realm — the two realms are separate stores",
  "qoder.page.overview.kpi.usage": "Usage today",
  "qoder.page.overview.kpi.usageHint": "Gateway not wired ⇒ no data source",
  "qoder.page.overview.kpi.usageTitle":
    "Daily tokens and credits come from gateway request logs and a local credit cache, neither of which is landed; unknown shows “—” rather than 0",
  "qoder.page.overview.clients.title": "Local credential sources",
  "qoder.page.overview.clients.description": "Read-only probe; nothing is persisted and no client file is modified.",
  "qoder.page.overview.clients.action": "Go to accounts",
  "qoder.page.overview.accounts.title": "Accounts",
  "qoder.page.overview.accounts.description": "One of the two realm-split account stores.",
  "qoder.page.overview.accounts.action": "Go to accounts",
  "qoder.page.overview.accounts.more": "{n} more",
  "qoder.page.overview.accounts.empty":
    "No account in this realm yet — open the accounts page to import from this machine or paste a PAT.",
  "qoder.page.overview.pending.title": "Usage and gateway are not landed yet",
  "qoder.page.overview.pending.description":
    "Daily tokens, credits and request logs all come from the gateway; until it is wired this page shows no placeholder KPIs.",
  "qoder.page.overview.pending.action": "Go to accounts",

  // ---- Settings ----
  "qoder.page.settings.title": "Settings",
  "qoder.page.settings.section.client": "Client detection",
  "qoder.page.settings.client.description": "Probe result for the two official credential stores (read-only).",
  "qoder.page.settings.client.error": "Probe failed: {error}",
  "qoder.page.settings.filtered":
    "Only sections with real content are listed; client path / check-in / gateway / scheduled-task settings will appear once those features land (no empty shells are shown up front).",
} as const;
