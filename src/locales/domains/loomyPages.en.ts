/**
 * 文案域：**Loomy 产品线 · 页面**（英文）。键集合必须与 `loomyPages.zh.ts` 一致。
 */
export const en = {
  "loomy.page.accounts.title": "Loomy accounts",
  "loomy.page.accounts.cancel": "Cancel",
  "loomy.page.accounts.save": "Save",
  "loomy.page.accounts.empty.title": "No Loomy accounts yet",
  "loomy.page.accounts.empty.description":
    "Three ways to add one: WeChat QR (no client needed — works in containers), SMS code login, or paste an existing credential.",
  "loomy.page.accounts.add.wechat": "WeChat QR",
  "loomy.page.accounts.add.sms": "SMS login",
  "loomy.page.accounts.add.import": "Import credential",

  "loomy.page.accounts.card.manageAria": "More actions for {name}",
  "loomy.page.accounts.card.moreActions": "More actions",
  "loomy.page.accounts.card.viewDetail": "View details",
  "loomy.page.accounts.card.checkin": "Check in",
  "loomy.page.accounts.card.editRemark": "Edit note",
  "loomy.page.accounts.card.setGroup": "Set group",
  "loomy.page.accounts.card.deleteAccount": "Delete account",
  "loomy.page.accounts.card.selectAria": "Select {name}",

  "loomy.page.accounts.chip.ok": "Credential usable",
  "loomy.page.accounts.chip.noCredential": "No usable credential",
  "loomy.page.accounts.chip.expired": "Session expired",
  "loomy.page.accounts.chip.checkedIn": "Checked in today",

  "loomy.page.accounts.metric.permanent": "Permanent",
  "loomy.page.accounts.metric.daily": "Daily pool (expires today)",
  "loomy.page.accounts.metric.lastCheckin": "Last check-in: {at}",

  "loomy.page.accounts.origin.import": "Credential import",
  "loomy.page.accounts.origin.smsLogin": "SMS login",
  "loomy.page.accounts.origin.wechatLogin": "WeChat QR",

  "loomy.page.accounts.login.ok": "Signed in",
  "loomy.page.accounts.import.ok": "Credential imported",
  "loomy.page.accounts.checkin.done": "Check-in processed",
  "loomy.page.accounts.checkin.failed": "Check-in failed",
  "loomy.page.accounts.credits.refreshed": "Credits refreshed",
  "loomy.page.accounts.credits.refreshFailed": "Failed to refresh credits",
  "loomy.page.accounts.remark.saved": "Note saved",
  "loomy.page.accounts.remark.failed": "Failed to save note",
  "loomy.page.accounts.group.saved": "Group saved",
  "loomy.page.accounts.group.failed": "Failed to save group",
  "loomy.page.accounts.delete.done": "Deleted {name}",
  "loomy.page.accounts.delete.failed": "Delete failed",

  "loomy.page.accounts.remark.title": "Edit note",
  "loomy.page.accounts.remark.description":
    "The note is stored locally and never synced upstream. Leave it empty to clear.",
  "loomy.page.accounts.remark.placeholder": "e.g. primary account",
  "loomy.page.accounts.group.title": "Set group",
  "loomy.page.accounts.group.description":
    "Free-text group, stored locally. Leave it empty for ungrouped.",
  "loomy.page.accounts.group.placeholder": "Empty = ungrouped",

  "loomy.page.accounts.delete.title": "Delete this account?",
  "loomy.page.accounts.delete.description":
    "\"{name}\" will be removed from the local account store. ⚠️ Loomy has no recycle bin — afterwards you can only sign in or import the credential again.",
  "loomy.page.accounts.delete.confirm": "Delete",

  "loomy.page.accounts.sms.title": "Loomy SMS login",
  "loomy.page.accounts.sms.description":
    "Enter your phone number to receive a code. Sessions last 14 days and must be renewed by signing in again (Loomy has no refresh endpoint).",
  "loomy.page.accounts.sms.phone": "Phone number",
  "loomy.page.accounts.sms.phonePlaceholder": "11-digit phone number",
  "loomy.page.accounts.sms.code": "Verification code",
  "loomy.page.accounts.sms.codePlaceholder": "SMS code",
  "loomy.page.accounts.sms.send": "Send code",
  "loomy.page.accounts.sms.submit": "Sign in",

  "loomy.page.accounts.import.title": "Import Loomy credential",
  "loomy.page.accounts.import.description":
    "Paste the credential JSON from a Loomy login (access_token / userid / phone). It must contain a 32-hex iFlytek session or a valid userid.",
  "loomy.page.accounts.import.submit": "Import",
  "loomy.page.settings.title": "Settings",
  "loomy.page.settings.schedule.sampleGroup": "Daily credit sampling",
  "loomy.page.settings.schedule.sampleEnable": "Enable daily sampling",
  "loomy.page.settings.schedule.sampleDesc":
    "Reads both credit pools for every account at the times below and records them in the local ledger (read-only — it never checks in or spends credits). Turning it off stops the usage page trend from updating.",
  "loomy.page.settings.schedule.sampleTimesLabel": "Sampling times",
  "loomy.page.settings.schedule.sampleTimesDesc":
    "One sample per time per day; repeated samples on the same day keep the last reading (the trend is one cell per day).",
  "loomy.page.settings.schedule.saveFailed": "Failed to save schedule",
  "loomy.page.settings.checkinGroup": "Automatic check-in",
  "loomy.page.settings.checkinUnavailable":
    "Loomy automatic check-in is not available: the scheduler only registers daily credit sampling, there is no Loomy check-in task. Trigger it manually from an account card's More menu (check-in resets the daily allowance — it does not add credits, and it is idempotent).",
} as const;
