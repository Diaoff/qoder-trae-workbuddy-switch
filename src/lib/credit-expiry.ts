/**
 * 「临期积分」的**唯一汇总口径** —— 账号卡片上那枚 `N 积分 M 天内过期` 徽章用它。
 *
 * ## 为什么需要这个模块（issue #44）
 *
 * 徽章原先在两张卡片里各写一遍，且两处的 `amount` 与 `days` **不是同一批积分**：
 *
 * - `amount` = 窗口内**全部**临期剩余积分之和；
 * - `days`   = 窗口内**最早**那一刻距今天数（向上取整）。
 *
 * 于是只要窗口里有两批不同到期日，徽章就是假话：100 明天到期 + 200 后天到期，
 * 会显示「300 积分 1 天后过期」—— 那 200 积分根本不在 1 天内到期。
 *
 * ## 现在的口径
 *
 * - `total`  窗口内临期剩余积分之和（**不**只算最早一批，总量不丢）；
 * - `days`   窗口内**最晚**到期时刻距今天数（向上取整）⇒「{total} 积分 {days} 天内过期」
 *   对窗口内每一笔都成立，不再有半句假话；
 * - `batches` 按到期时刻**升序**的明细，供 tooltip 逐批展开（最急的那批排第一）。
 *
 * ## 窗口由**后端**决定，前端不再自设
 *
 * 是否临期只认后端下发的 `expiringSoon` 标记（`credits.rs::EXPIRING_SOON_DAYS` = 7 天、
 * `trae/credits.rs::EXPIRING_SOON_SECS` = 7 天）。此前 WorkBuddy 卡片在前端另设 3 天窗口，
 * 与统计页 / 总览页（都读后端的 `expiringSoonRemaining`）口径不一致 —— 同一件事两个数字。
 *
 * ⚠️ 不可把 `expiringSoon`（布尔）当额度求和，也不可在缺该字段时自行推窗口：
 * 缺字段一律当「未知」跳过，宁可少一枚徽章，不可编一个到期日。
 */

/** 一天的毫秒数。 */
const DAY_MS = 86_400_000;

/** 一批到期：同一时刻到期的剩余积分之和。 */
export interface ExpiringBatch {
  /** 到期时刻（epoch 毫秒）。 */
  expireAt: number;
  /** 该批此刻的剩余积分之和。 */
  amount: number;
}

export interface ExpiringSummary {
  /** 窗口内全部临期剩余积分之和。 */
  total: number;
  /** 窗口内**最晚**到期时刻距今天数（向上取整，≥1）。 */
  days: number;
  /** 按到期时刻升序的逐批明细。 */
  batches: ExpiringBatch[];
}

/** 汇总入参：WorkBuddy 的 `CreditResource` 与 Trae 的 `TraeCreditPackage` 都满足它。 */
export interface ExpiringItem {
  remaining: number;
  expireAt: number | null;
  /** 后端标记：已到期。 */
  expired?: boolean;
  /** 后端标记：临期（7 天窗口）。`undefined` 表示数据源没这个字段 ⇒ 视为未知、跳过。 */
  expiringSoon?: boolean;
}

/**
 * 汇总临期积分。没有可渲染的临期批次时返回 `null`（调用方据此**整条不渲染**，
 * 绝不写一个猜出来的「N 天」标签）。
 *
 * @param items 积分包 / 积分资源列表（可为 `null`）
 * @param options.now 判定基准时刻（毫秒），默认 `Date.now()`
 *   —— 一次渲染里多处汇总应传同一个值，否则同一张卡会出现两个「今天」。
 * @param options.unit 入参 `expireAt` 的单位。**两个平台不同，必须显式给**：
 *   WorkBuddy 的 `CreditResource.expireAt` 是**毫秒**（后端 `parse_timestamp_ms`），
 *   Trae 的 `TraeCreditPackage.expireAt` 是**秒**（后端直接透传上游 `expire_time`）。
 */
export function summarizeExpiring(
  items: readonly ExpiringItem[] | null | undefined,
  options: { now?: number; unit?: "ms" | "s" } = {},
): ExpiringSummary | null {
  if (!items || items.length === 0) return null;
  const now = options.now ?? Date.now();
  const toMs = options.unit === "s" ? (value: number) => value * 1000 : (value: number) => value;

  /* 同一到期时刻合并成一批：上游可能把一次发放拆成多个包，逐包列会让 tooltip 变成流水账。 */
  const merged = new Map<number, number>();
  for (const item of items) {
    if (item.expired) continue;
    // `expiringSoon` 必须显式为 `true`：缺字段 = 未知，不当「临期」也不当「不临期」去推测窗口。
    if (item.expiringSoon !== true) continue;
    if (!Number.isFinite(item.remaining) || item.remaining <= 0) continue;
    if (typeof item.expireAt !== "number" || !Number.isFinite(item.expireAt)) continue;
    const expireAt = toMs(item.expireAt);
    if (!Number.isFinite(expireAt) || expireAt <= now) continue;
    merged.set(expireAt, (merged.get(expireAt) ?? 0) + item.remaining);
  }
  if (merged.size === 0) return null;

  const batches = [...merged.entries()]
    .map(([expireAt, amount]) => ({ expireAt, amount }))
    .sort((left, right) => left.expireAt - right.expireAt);
  const total = batches.reduce((sum, batch) => sum + batch.amount, 0);
  if (total <= 0) return null;

  const latest = batches[batches.length - 1].expireAt;
  return { total, days: Math.max(1, Math.ceil((latest - now) / DAY_MS)), batches };
}
