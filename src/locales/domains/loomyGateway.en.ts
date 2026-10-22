/**
 * 文案域：**Loomy 产品线 · API 网关**（英文）。
 *
 * 键集合必须与 `loomyGateway.zh.ts` 逐字一致（门禁按 zh 推类型）。
 */
export const en = {
  "loomy.gateway.title": "Gateway",

  "loomy.gateway.enable": "Enable local gateway",
  "loomy.gateway.enableDesc":
    "Serve OpenAI / Anthropic compatible endpoints on 127.0.0.1 · upstream {upstream}",
  "loomy.gateway.bindAddr": "Bind address",
  "loomy.gateway.bindLoopback": "Local only (127.0.0.1)",
  "loomy.gateway.bindLan": "LAN (0.0.0.0)",
  "loomy.gateway.port": "Port",
  "loomy.gateway.statusLabel": "Status",
  "loomy.gateway.running": "Running",
  "loomy.gateway.stopped": "Stopped",
  "loomy.gateway.noteLead": "All gateways share",
  "loomy.gateway.noteTail": "— ports must differ (this page is Loomy, default 57896).",
  "loomy.gateway.lanWarning": "LAN access is allowed — make sure the network is trusted.",
  "loomy.gateway.lastError": "Last failure",
  "loomy.gateway.refresh": "Refresh",
  "loomy.gateway.savedStarted": "Saved and listener started",
  "loomy.gateway.savedDisabled": "Saved: listener stopped",
  "loomy.gateway.saveFailed": "Save failed",
  "loomy.gateway.portInvalid": "Invalid port",
  "loomy.gateway.portRange": "Port must be between 1 and 65535",
  "loomy.gateway.loadFailed": "Failed to load gateway info",
  "loomy.gateway.retry": "Retry",

  "loomy.gateway.endpoint.title": "Endpoint",
  "loomy.gateway.endpoint.baseUrl": "Base URL",
  "loomy.gateway.endpoint.keyPrefix": "API Key",
  "loomy.gateway.endpoint.noKey": "No key yet (create one under Keys)",
  "loomy.gateway.endpoint.copied": "Base URL copied",
  "loomy.gateway.copy": "Copy",

  "loomy.gateway.overview.models": "Served models",
  "loomy.gateway.overview.modelsCount": "{count}",
  "loomy.gateway.overview.pool": "Account pool",
  "loomy.gateway.overview.poolCount": "{count} entries",
  "loomy.gateway.overview.view": "View",

  "loomy.gateway.lanDialog.title": "Allow LAN access?",
  "loomy.gateway.lanDialog.desc":
    "Binding 0.0.0.0 lets any device on the same network call this gateway with your key (spending your Loomy credits). Enable only on a trusted network.",
  "loomy.gateway.lanDialog.cancel": "Cancel",
  "loomy.gateway.lanDialog.confirm": "Allow",

  "loomy.gateway.guide.noKey": "sk-loomy-… (create a key above first)",
  "loomy.gateway.guide.streamNote":
    "Loomy upstream supports streaming; with `stream: false` this gateway aggregates locally and returns once, so the first byte takes longer.",

  "loomy.gateway.logsCleared": "Logs cleared",
  "loomy.gateway.logsClearFailed": "Failed to clear logs",

  "loomy.gateway.models.note":
    "The list comes straight from Loomy (requires account credentials) and is cached for 10 minutes; disabled models never appear in /v1/models.",
  "loomy.gateway.models.refresh": "Refresh list",
  "loomy.gateway.models.refreshFailed": "Refresh failed",
  "loomy.gateway.models.scanning": "Refreshing…",
  "loomy.gateway.models.sourceLive": "Live",
  "loomy.gateway.models.sourceCached": "Last success",
  "loomy.gateway.models.sourceUnavailable": "No list yet",
  "loomy.gateway.models.reasonCached": "This fetch failed; showing the last successful result",
  "loomy.gateway.models.reasonUnavailable":
    "The list has never been fetched successfully: check that an account works, then refresh",
  "loomy.gateway.models.empty": "No models to show yet",
  "loomy.gateway.models.emptyDesc":
    "The list requires account credentials — import an account under Accounts, then refresh.",

  "loomy.gateway.key.create": "Create key",
  "loomy.gateway.key.createDesc": "Loomy has a single scope: this key maps to the one local account store.",
  "loomy.gateway.key.name": "Name",
  "loomy.gateway.key.namePlaceholder": "e.g. CI automation",
  "loomy.gateway.key.nameRequired": "Name is required",
  "loomy.gateway.key.cancel": "Cancel",
  "loomy.gateway.key.createSubmit": "Create",
  "loomy.gateway.key.createdTitle": "Key created",
  "loomy.gateway.key.createdDesc": "The plaintext is shown only once — copy it now (only the mask is kept).",
  "loomy.gateway.key.copy": "Copy",
  "loomy.gateway.key.saved": "I saved it",
  "loomy.gateway.key.copied": "Copied",
  "loomy.gateway.key.noPlaintext": "Created but no plaintext returned — contract violation, please retry",
  "loomy.gateway.key.createFailed": "Create failed",
  "loomy.gateway.key.loadFailed": "Failed to load keys",
  "loomy.gateway.key.empty": "No keys yet",
  "loomy.gateway.key.col.prefix": "Key",
  "loomy.gateway.key.col.createdAt": "Created",
  "loomy.gateway.key.col.lastUsed": "Last used",
  "loomy.gateway.key.col.status": "Status",
  "loomy.gateway.key.col.actions": "Actions",
  "loomy.gateway.key.statusActive": "Active",
  "loomy.gateway.key.statusRevoked": "Revoked",
  "loomy.gateway.key.neverUsed": "Never",
  "loomy.gateway.key.revoke": "Revoke",
  "loomy.gateway.key.revokeTitle": "Revoke this key?",
  "loomy.gateway.key.revokeDesc":
    "\"{name}\" stops working immediately (reversible): revoked rows stay in the table and can be restored at any time.",
  "loomy.gateway.key.revoked": "Revoked",
  "loomy.gateway.key.revokeFailed": "Revoke failed",
  "loomy.gateway.key.restore": "Restore",
  "loomy.gateway.key.restoreTitle": "Restore this key?",
  "loomy.gateway.key.restoreDesc":
    "\"{name}\" becomes usable again — it is the same key (plaintext unchanged), so clients need no reconfiguration.",
  "loomy.gateway.key.restored": "Restored",
  "loomy.gateway.key.restoreFailed": "Restore failed",
  "loomy.gateway.key.delete": "Delete",
  "loomy.gateway.key.deleteTitle": "Delete this key?",
  "loomy.gateway.key.deleteDesc":
    "The record for \"{name}\" is deleted permanently. Clients using it get 401 immediately.",
  "loomy.gateway.key.deleted": "Deleted",
  "loomy.gateway.key.deleteFailed": "Delete failed",
} as const;
