/**
 * 密钥用量的**纯聚合逻辑**（issue #76 / 规范 U4）。
 *
 * 单独成文件而不是留在 `key-ranking.tsx` 里：它是**纯函数**，应当能被脚本直接跑
 * （`scripts/check-key-usage.mjs`）。留在 `.tsx` 里就得为了跑断言而引入 React 运行时。
 *
 * ## 两条判据（都有测试守着）
 *
 * 1. `keyId == null`（确实没有 Key）与 `keyId` **键不存在**（升级前的老条目）
 *    **分开统计** —— 合并会让历史数据凭空多出一批「匿名调用」；
 * 2. 分组键带 `keyScope` —— 同一个 id 字符串在两套台账里是两把不同的 Key。
 */

/**
 * 聚合只依赖这几个字段（**结构最小接口**）。
 *
 * 为什么不直接用 `GatewayLogEntry`：WB / Trae / Qoder 是**三套日志类型**
 * （`promptTokens` 有的可空有的不可空、Qoder 还多 `credits`），但它们都能
 * 提供这五项。绑死某一个具体类型就得在三个页面各写一遍适配器。
 */
export interface KeyUsageLogEntry {
  ts: number;
  promptTokens?: number | null;
  completionTokens?: number | null;
  credit?: number | null;
  /** `null` = 无 Key；`undefined` = 升级前老条目（两种「没有」不合并）。 */
  keyId?: string | null;
  /**
   * ★ 这个 `keyId` 属于**哪套台账**（`"wb"` / `"trae"` / `"qoder"` / `"unified"`）。
   *
   * 统一网关把请求转发给产品线中继时，产品线日志记下的是**网关那把 Key 的 id** ——
   * 与产品线自己的 Key id 混在同一个字段。靠这个标签区分，否则网关的 Key 会被
   * 当成一把「已删除的本地 Key」（显示成短 id），用户永远对不上。
   */
  keyScope?: string | null;
}

/** 一行排行（按密钥聚合后）。 */
export interface KeyUsageSlice {
  /** 密钥 id；`null` = 确实没有 Key 的调用（不含老条目）。 */
  keyId: string | null;
  /** 展示名（Key 名；取不到时用短 id；`null` 时是「无密钥调用」）。 */
  name: string;
  tokens: number;
  requests: number;
  credit: number;
}

/** 台账标签（跨台账的 Key 在「按密钥」里要标清来自哪里）。 */
export const SCOPE_LABELS: Record<string, string> = {
  wb: "WorkBuddy",
  trae: "Trae",
  qoder: "Qoder",
  unified: "统一网关",
};

/** 老条目（升级前写入、没有 `keyId` 键）单独一行展示的标识。 */
export const LEGACY_KEY_LABEL = "（升级前的历史条目）";

/**
 * 把日志条目按 `keyId` 聚合成排行（**纯函数**，不依赖 React / 网络）。
 *
 * @param entries 已在时间窗口内过滤过的日志条目
 * @param keyNames `id → 展示名`（来自密钥列表；缺失的 id 用短 id 兜底）
 */
export function aggregateByKey(
  entries: readonly KeyUsageLogEntry[],
  keyNames: ReadonlyMap<string, string>,
  /** 本页所属的台账（`"wb"` / `"trae"` / `"qoder"`）；跨台账的 Key 单独命名。 */
  selfScope: string,
): KeyUsageSlice[] {
  const buckets = new Map<string, KeyUsageSlice>();

  const bump = (slice: KeyUsageSlice, entry: KeyUsageLogEntry) => {
    slice.tokens += (entry.promptTokens ?? 0) + (entry.completionTokens ?? 0);
    slice.requests += 1;
    slice.credit += entry.credit ?? 0;
  };

  for (const entry of entries) {
    // ★ `in` 而不是 `??`：要区分「键不存在」（老条目）与「值为 null」（确实无 Key）。
    const hasKey = Object.prototype.hasOwnProperty.call(entry, "keyId");
    if (!hasKey) {
      const slice = buckets.get("__legacy__") ?? {
        keyId: null,
        name: LEGACY_KEY_LABEL,
        tokens: 0,
        requests: 0,
        credit: 0,
      };
      bump(slice, entry);
      buckets.set("__legacy__", slice);
      continue;
    }
    const id = entry.keyId ?? null;
    const scope = entry.keyScope ?? null;
    /* ★ 分组键带 scope：同一个 id 字符串在**两套台账**里是两把不同的 Key
       （网关的 Key 与本产品线的 Key 各自独立生成）。不带上 scope 会把它们并成一行。 */
    const bucketKey = id === null ? "__none__" : `${scope ?? selfScope}:${id}`;
    const slice = buckets.get(bucketKey) ?? {
      keyId: id,
      name:
        id === null
          ? "无密钥调用"
          : scope && scope !== selfScope
            ? /* 跨台账：**明确标出**「这是网关（或另一条产品线）的 Key」，
                 而不是拿短 id 冒充本地 Key。 */
              `${SCOPE_LABELS[scope] ?? scope} 的密钥（${shortId(id)}）`
            : (keyNames.get(id) ?? shortId(id)),
      tokens: 0,
      requests: 0,
      credit: 0,
    };
    bump(slice, entry);
    buckets.set(bucketKey, slice);
  }

  // 按 token 降序（token 为 0 的按请求数排 —— 业务代理只有请求数没有 token）。
  return [...buckets.values()].sort(
    (a, b) => b.tokens - a.tokens || b.requests - a.requests,
  );
}

/** 取不到 Key 名时的兜底：前 8 位 + 省略号（够辨认，不占宽度）。 */
export function shortId(id: string): string {
  return id.length > 8 ? `${id.slice(0, 8)}…` : id;
}

