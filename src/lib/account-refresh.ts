"use client";

import { useCallback, useState } from "react";

/**
 * 账号「单项刷新」的共用契约（三平台卡片 + 详情抽屉都按这一套来）。
 *
 * ## 为什么是**四项**而不是一个「刷新全部」
 *
 * 四类数据来自**不同层**，耗时与失败原因都不一样：
 *
 * | 项 | 数据来源 | 特点 |
 * |:--|:--|:--|
 * | `credits` | 上游接口（查剩余积分 / 到期） | 最慢，可能被限流 |
 * | `tokens` | 本机客户端日志 + 网关日志 | 快，离线可读 |
 * | `checkin` | 本机签到记录 | 快 |
 * | `credential` | 本机凭据（JWT 有效期） | 最快，且**凭据失效时只有它能给出答案** |
 *
 * 合成一个动作时，慢的拖住快的，而且失败原因分不清 —— 凭据过期的账号其实只有
 * 「凭据状态」能刷出真值，另外三项必然失败。拆开之后用户能只刷要用的那项，
 * 也能从失败项里看出卡在哪一层（判据见 `docs/specs/PLAN.md` §3.3）。
 *
 * ## 为什么按「账号 + 项」记进行中，而不是一个全局布尔
 *
 * 列表里可能同时有多个账号在刷，同一个账号也可能同时刷两项（比如先刷凭据再刷积分）。
 * 用一个布尔会让后开始的把先开始的转圈吞掉；用一个 `Set<accountId>` 又会让
 * 「点了积分，四项一起转」。所以键是 `id:kind`。
 */
export type AccountRefreshKind = "credits" | "tokens" | "checkin" | "credential";

/** 固定顺序：菜单分组与指标块都从这里取，避免两处各写一份顺序。 */
export const ACCOUNT_REFRESH_KINDS: readonly AccountRefreshKind[] = [
  "credits",
  "tokens",
  "checkin",
  "credential",
] as const;

const keyOf = (accountId: string, kind: AccountRefreshKind) => `${accountId}:${kind}`;

/**
 * 四项各自的成功文案键。
 *
 * 写死成字面量是为了让 `t()` 的**编译期键校验**生效（拼错 key 会直接报错，而不是静默回退成 key 本身）；
 * 也避免三个页面各自写一份「已刷新」的措辞 —— 用户需要知道刷的是哪一层。
 */
export const REFRESH_DONE_LABEL = {
  credits: "shared.refreshPart.credits",
  tokens: "shared.refreshPart.tokens",
  checkin: "shared.refreshPart.checkin",
  credential: "shared.refreshPart.credential",
} as const;

/**
 * 进行中态的小 hook。
 *
 * 用法：
 * ```tsx
 * const refresh = useAccountRefreshBusy();
 * const onRefreshPart = (a, kind) => refresh.run(a.id, kind, () => api.xxx(...));
 * <AccountCard refreshingKinds={refresh.kindsOf(a.id)} onRefreshPart={onRefreshPart} />
 * ```
 *
 * ⚠️ `kindsOf` 每次返回**新数组**：调用方不要把它放进 `useMemo`/`useEffect` 依赖里
 * （会每次重算）。卡片只需要读它来决定哪一项显示转圈。
 */
export function useAccountRefreshBusy() {
  const [busy, setBusy] = useState<Map<string, Set<AccountRefreshKind>>>(() => new Map());

  const setOne = useCallback((accountId: string, kind: AccountRefreshKind, on: boolean) => {
    setBusy((prev) => {
      const current = prev.get(accountId);
      if (on) {
        if (current?.has(kind)) return prev;
        const next = new Map(prev);
        const set = new Set(current ?? []);
        set.add(kind);
        next.set(accountId, set);
        return next;
      }
      if (!current?.has(kind)) return prev;
      const next = new Map(prev);
      const set = new Set(current);
      set.delete(kind);
      if (set.size === 0) next.delete(accountId);
      else next.set(accountId, set);
      return next;
    });
  }, []);

  const kindsOf = useCallback(
    (accountId: string): AccountRefreshKind[] => {
      const set = busy.get(accountId);
      return set ? [...set] : [];
    },
    [busy],
  );

  const isBusy = useCallback(
    (accountId: string, kind: AccountRefreshKind): boolean => busy.get(accountId)?.has(kind) ?? false,
    [busy],
  );

  /**
   * 跑一次单项刷新：先标记进行中，**无论成功失败都撤掉**（`finally`）。
   * 失败的处理（toast / 重试）交给调用方 —— 这个 hook 只管「谁在转圈」。
   */
  const run = useCallback(
    async (accountId: string, kind: AccountRefreshKind, task: () => Promise<unknown>): Promise<void> => {
      setOne(accountId, kind, true);
      try {
        await task();
      } finally {
        setOne(accountId, kind, false);
      }
    },
    [setOne],
  );

  return { kindsOf, isBusy, run, keyOf };
}
