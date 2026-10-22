/**
 * 文案域：**Loomy 产品线 · 页面**（中文，键的权威之一）。
 *
 * 归属文件：`src/pages/LoomyAccountsPage.tsx`、`src/components/loomy/loomy-account-card.tsx`。
 *
 * ⚠️ 命名空间用 `loomy.page.`（与 `qoder.page.` / `trae.page.` 同款）。
 * 可共用的文案（弹层通用按钮、卡片菜单分组名）走 `shared.*`，本域只放 Loomy 侧特有的。
 */
export const zh = {
  // ---- 账号页页头 / 空态 ----
  "loomy.page.accounts.title": "Loomy 账号",
  "loomy.page.accounts.cancel": "取消",
  "loomy.page.accounts.save": "保存",
  "loomy.page.accounts.empty.title": "还没有 Loomy 账号",
  "loomy.page.accounts.empty.description":
    "三条添加方式任选：微信扫码（无需客户端，容器里也能用）、短信验证码登录、或粘贴已有凭据。",
  "loomy.page.accounts.add.wechat": "微信扫码",
  "loomy.page.accounts.add.sms": "短信登录",
  "loomy.page.accounts.add.import": "导入凭据",

  // ---- 卡片 ----
  "loomy.page.accounts.card.manageAria": "{name} 的更多操作",
  "loomy.page.accounts.card.moreActions": "更多操作",
  "loomy.page.accounts.card.viewDetail": "查看详情",
  "loomy.page.accounts.card.checkin": "签到",
  "loomy.page.accounts.card.editRemark": "编辑备注",
  "loomy.page.accounts.card.setGroup": "设置分组",
  "loomy.page.accounts.card.deleteAccount": "删除账号",
  "loomy.page.accounts.card.selectAria": "选择 {name}",

  // ---- 状态徽章 ----
  "loomy.page.accounts.chip.ok": "凭据可用",
  "loomy.page.accounts.chip.noCredential": "无可用凭据",
  "loomy.page.accounts.chip.expired": "登录已过期",
  "loomy.page.accounts.chip.checkedIn": "今日已签",

  // ---- 指标 ----
  "loomy.page.accounts.metric.permanent": "永久",
  "loomy.page.accounts.metric.daily": "每日池（当天作废）",
  "loomy.page.accounts.metric.lastCheckin": "上次签到：{at}",

  // ---- 来源 ----
  "loomy.page.accounts.origin.import": "凭据导入",
  "loomy.page.accounts.origin.smsLogin": "短信登录",
  "loomy.page.accounts.origin.wechatLogin": "微信扫码",

  // ---- 动作回执（全部走 toast，不再用 window.alert）----
  "loomy.page.accounts.login.ok": "登录成功",
  "loomy.page.accounts.import.ok": "凭据已导入",
  "loomy.page.accounts.checkin.done": "签到已处理",
  "loomy.page.accounts.checkin.failed": "签到失败",
  "loomy.page.accounts.credits.refreshed": "积分已刷新",
  "loomy.page.accounts.credits.refreshFailed": "刷新积分失败",
  "loomy.page.accounts.remark.saved": "备注已保存",
  "loomy.page.accounts.remark.failed": "备注保存失败",
  "loomy.page.accounts.group.saved": "分组已保存",
  "loomy.page.accounts.group.failed": "分组保存失败",
  "loomy.page.accounts.delete.done": "已删除 {name}",
  "loomy.page.accounts.delete.failed": "删除失败",

  // ---- 备注 / 分组弹层 ----
  "loomy.page.accounts.remark.title": "编辑备注",
  "loomy.page.accounts.remark.description": "备注只存在本机，不会同步到上游。留空表示清除。",
  "loomy.page.accounts.remark.placeholder": "例如：主力号",
  "loomy.page.accounts.group.title": "设置分组",
  "loomy.page.accounts.group.description": "自由文本分组，只存在本机。留空表示未分组。",
  "loomy.page.accounts.group.placeholder": "留空表示未分组",

  // ---- 删除确认 ----
  "loomy.page.accounts.delete.title": "删除这个账号？",
  "loomy.page.accounts.delete.description":
    "「{name}」会从本机账号库移除。⚠️ Loomy 没有回收站，删除后只能重新登录或重新导入凭据。",
  "loomy.page.accounts.delete.confirm": "删除",

  // ---- 短信登录 ----
  "loomy.page.accounts.sms.title": "Loomy 短信登录",
  "loomy.page.accounts.sms.description":
    "输入手机号获取验证码。session 有效期 14 天，过期需重新登录（Loomy 无续期端点）。",
  "loomy.page.accounts.sms.phone": "手机号",
  "loomy.page.accounts.sms.phonePlaceholder": "11 位手机号",
  "loomy.page.accounts.sms.code": "验证码",
  "loomy.page.accounts.sms.codePlaceholder": "短信验证码",
  "loomy.page.accounts.sms.send": "发送验证码",
  "loomy.page.accounts.sms.submit": "登录",

  // ---- 导入凭据 ----
  "loomy.page.accounts.import.title": "导入 Loomy 凭据",
  "loomy.page.accounts.import.description":
    "粘贴 Loomy 登录后的凭据 JSON（access_token / userid / phone）。需包含 32 位十六进制的讯飞 session 或有效 userid。",
  "loomy.page.accounts.import.submit": "导入",
  // ---- 设置页 ----
  "loomy.page.settings.title": "设置",
  "loomy.page.settings.schedule.sampleGroup": "积分逐日采样",
  "loomy.page.settings.schedule.sampleEnable": "启用每日采样",
  "loomy.page.settings.schedule.sampleDesc":
    "按下面的时点读取各账号的两池积分并记入本地台账（只读请求，不会签到、也不消耗积分）。关掉后用量页的趋势不再更新。",
  "loomy.page.settings.schedule.sampleTimesLabel": "采样时点",
  "loomy.page.settings.schedule.sampleTimesDesc":
    "每天每个时点采一帧；同日重复采样保留最后一次读数（趋势按日一格）。",
  "loomy.page.settings.schedule.saveFailed": "保存计划任务失败",
  "loomy.page.settings.checkinGroup": "自动签到",
  "loomy.page.settings.checkinUnavailable":
    "Loomy 的自动签到尚未提供：本仓排程里只登记了「积分逐日采样」，没有 Loomy 签到任务。需要签到时请在账号卡的「更多」菜单里手动触发（签到 = 触发每日赠送额度重置，不是加分，幂等）。",
} as const;
