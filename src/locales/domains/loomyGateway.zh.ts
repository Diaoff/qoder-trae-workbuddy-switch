/**
 * 文案域：**Loomy 产品线 · API 网关**（中文，键的权威之一）。
 *
 * 归属文件：`src/pages/LoomyApiServicePage.tsx`、
 * `src/components/gateway/loomy-api-key-table.tsx`、`src/components/gateway/loomy-model-list.tsx`
 *
 * ⚠️ 命名空间用 `loomy.gateway.`（与 `qoder.gateway.` / `trae.gateway.` 同款）。
 * **可共用的文案一律不在这里**：账号池卡 / 模型表 / 来源条 / 日志卡的词条走 `shared.*`
 * （四平台同形），本域只放 Loomy 侧特有的那批。
 */
export const zh = {
  // ---- 页头 / 服务卡 ----
  "loomy.gateway.title": "网关",
  // ⚠️ 页头副标题走 `shared` 域的 `pageHead.gateway` —— 与另三条产品线同一公式。

  "loomy.gateway.enable": "启用本地网关",
  "loomy.gateway.enableDesc": "在 127.0.0.1 上提供 OpenAI / Anthropic 兼容接口·上游为 {upstream}",
  "loomy.gateway.bindAddr": "监听地址",
  "loomy.gateway.bindLoopback": "仅本机（127.0.0.1）",
  "loomy.gateway.bindLan": "局域网（0.0.0.0）",
  "loomy.gateway.port": "端口",
  "loomy.gateway.statusLabel": "状态",
  "loomy.gateway.running": "运行中",
  "loomy.gateway.stopped": "已停用",
  "loomy.gateway.noteLead": "各条网关都占用",
  "loomy.gateway.noteTail": "—— 端口必须错开（本页是 Loomy，默认 57896）。",
  "loomy.gateway.lanWarning": "当前已允许局域网访问 —— 请确认所处网络可信。",
  "loomy.gateway.lastError": "最近一次失败",
  "loomy.gateway.refresh": "刷新",
  "loomy.gateway.savedStarted": "已保存并启动监听",
  "loomy.gateway.savedDisabled": "已保存：监听已停止",
  "loomy.gateway.saveFailed": "保存失败",
  "loomy.gateway.portInvalid": "端口无效",
  "loomy.gateway.portRange": "端口需在 1–65535 之间",
  "loomy.gateway.loadFailed": "加载网关信息失败",
  "loomy.gateway.retry": "重试",

  // ---- 接入地址卡 ----
  "loomy.gateway.endpoint.title": "接入地址",
  "loomy.gateway.endpoint.baseUrl": "Base URL",
  "loomy.gateway.endpoint.keyPrefix": "API Key",
  "loomy.gateway.endpoint.noKey": "还没有 Key（去「密钥」分区创建）",
  "loomy.gateway.endpoint.copied": "地址已复制",
  "loomy.gateway.copy": "复制",

  // ---- 概览两行 ----
  "loomy.gateway.overview.models": "可用模型",
  "loomy.gateway.overview.modelsCount": "{count} 个",
  "loomy.gateway.overview.pool": "账号池",
  "loomy.gateway.overview.poolCount": "{count} 个条目",
  "loomy.gateway.overview.view": "查看",

  // ---- 局域网风险确认 ----
  "loomy.gateway.lanDialog.title": "允许局域网访问？",
  "loomy.gateway.lanDialog.desc":
    "监听 0.0.0.0 后，同一网络内的任何设备都可以用你的 Key 调用本网关（消耗你的 Loomy 额度）。仅在你确认网络可信时开启。",
  "loomy.gateway.lanDialog.cancel": "取消",
  "loomy.gateway.lanDialog.confirm": "确认开启",

  // ---- 接入指引（Loomy 侧两处平台化文案；其余词条复用 `trae.gateway.guide.*`）----
  "loomy.gateway.guide.noKey": "sk-loomy-…（请先在上方创建 Key）",
  "loomy.gateway.guide.streamNote":
    "Loomy 上游支持流式；请求 `stream: false` 时由本网关在本地聚合后一次性返回，首字节延迟较长。",

  // ---- 请求日志（日志卡与另三条共用；只有这两条 toast 是 Loomy 侧自己的）----
  "loomy.gateway.logsCleared": "日志已清空",
  "loomy.gateway.logsClearFailed": "清空日志失败",

  // ---- 模型清单（loomy-model-list.tsx）----
  "loomy.gateway.models.note":
    "清单直接来自 Loomy 官方（拉取需要账号凭据），10 分钟内不重复请求；「禁用」的模型不会出现在 /v1/models。",
  "loomy.gateway.models.refresh": "重新识别",
  "loomy.gateway.models.refreshFailed": "重新识别失败",
  "loomy.gateway.models.scanning": "识别中…",
  "loomy.gateway.models.sourceLive": "实时",
  "loomy.gateway.models.sourceCached": "上次成功",
  "loomy.gateway.models.sourceUnavailable": "暂无清单",
  "loomy.gateway.models.reasonCached": "本次拉取失败，展示的是上次成功的结果",
  "loomy.gateway.models.reasonUnavailable":
    "还没成功拉到过清单：请确认账号可调用，然后重新识别",
  "loomy.gateway.models.empty": "还没有可展示的模型",
  "loomy.gateway.models.emptyDesc":
    "模型清单需要账号凭据向官方拉取 —— 先在「账号」分区导入一个账号，再点「重新识别」。",

  // ---- 密钥表（loomy-api-key-table.tsx）----
  "loomy.gateway.key.create": "创建密钥",
  "loomy.gateway.key.createDesc": "Loomy 单作用域：这把 Key 直接对应本机唯一的账号库。",
  "loomy.gateway.key.name": "名称",
  "loomy.gateway.key.namePlaceholder": "例如：CI 自动化",
  "loomy.gateway.key.nameRequired": "请填写名称",
  "loomy.gateway.key.cancel": "取消",
  "loomy.gateway.key.createSubmit": "创建",
  "loomy.gateway.key.createdTitle": "密钥已创建",
  "loomy.gateway.key.createdDesc": "明文只显示这一次，请立即复制保存（之后只能看到掩码）。",
  "loomy.gateway.key.copy": "复制",
  "loomy.gateway.key.saved": "我已保存",
  "loomy.gateway.key.copied": "已复制",
  "loomy.gateway.key.noPlaintext": "创建成功但没有返回明文 —— 属于契约异常，请重试",
  "loomy.gateway.key.createFailed": "创建失败",
  "loomy.gateway.key.loadFailed": "密钥列表加载失败",
  "loomy.gateway.key.empty": "还没有密钥",
  "loomy.gateway.key.col.prefix": "密钥",
  "loomy.gateway.key.col.createdAt": "创建于",
  "loomy.gateway.key.col.lastUsed": "最后使用",
  "loomy.gateway.key.col.status": "状态",
  "loomy.gateway.key.col.actions": "操作",
  "loomy.gateway.key.statusActive": "启用",
  "loomy.gateway.key.statusRevoked": "已吊销",
  "loomy.gateway.key.neverUsed": "从未使用",
  "loomy.gateway.key.revoke": "吊销",
  "loomy.gateway.key.revokeTitle": "吊销这把密钥？",
  "loomy.gateway.key.revokeDesc":
    "「{name}」将立即无法调用（可逆）：已吊销的行仍留在表里，随时可以「恢复」。",
  "loomy.gateway.key.revoked": "已吊销",
  "loomy.gateway.key.revokeFailed": "吊销失败",
  "loomy.gateway.key.restore": "恢复",
  "loomy.gateway.key.restoreTitle": "恢复这把密钥？",
  "loomy.gateway.key.restoreDesc":
    "「{name}」将重新可用 —— 库中存的是同一把 Key，明文未变，客户端无需改配置。",
  "loomy.gateway.key.restored": "已恢复",
  "loomy.gateway.key.restoreFailed": "恢复失败",
  "loomy.gateway.key.delete": "删除",
  "loomy.gateway.key.deleteTitle": "删除这把密钥？",
  "loomy.gateway.key.deleteDesc": "「{name}」的记录将被物理删除（不可逆）。使用它的客户端会立即 401。",
  "loomy.gateway.key.deleted": "已删除",
  "loomy.gateway.key.deleteFailed": "删除失败",
} as const;
