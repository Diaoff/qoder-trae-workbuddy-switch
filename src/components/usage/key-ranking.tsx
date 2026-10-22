/**
 * 密钥 · **用量排行**（issue #76 / 规范 U4）。
 *
 * 「按密钥看用量」是 issue #76 的原始诉求之一：`key` 分给别人用之后，
 * 要能回答「**哪把 key 用掉了我多少额度**」。
 *
 * ## 数据源与判据
 *
 * 聚合的原料是**网关请求日志**（`get_gateway_logs`）的 `keyId` 字段 ——
 * 它在 `8f1a1c4` 才加进日志条目，所以：
 *
 * - `keyId == null` ⇒ 确实没有 Key 的调用（本地直连 / 管理面探测）；
 * - `keyId == undefined` ⇒ **升级前写入的老条目**（那时还没有这个键）。
 *
 * ★ 两者**分开统计**，不合并：把 `undefined` 并进「无 Key」会让历史数据凭空多出
 * 一批「匿名调用」，而这个数字一旦出现在排行里就会被当成真的。
 *
 * ## 为什么不用 `token_daily`
 *
 * 那个账本只有 client / gateway 两桶，gateway 桶的维度是模型 / 账号 / 端点 ——
 * **没有 key 维度**（规范 U4 的原话）。补 key 维度需要改账本 schema 与全部读写路径，
 * 而「按密钥看排行」是一次性的聚合查询，直接扫日志更直接、也不会让账本多一个
 * 只在某个页面用得上的维度。
 */

import { useEffect, useMemo, useState } from "react";

import {
  aggregateByKey,
  type KeyUsageLogEntry,
  type KeyUsageSlice,
} from "@/lib/key-usage";
import { dateKey, usageWindowKeys, type UsageWindow } from "@/lib/usage-window";

import { compactTokens, sharePercent } from "./format";
import { seriesColor } from "./palette";

export function KeyRanking({ slices }: { slices: KeyUsageSlice[] }) {
  if (slices.length === 0) return null;
  const total = slices.reduce((sum, slice) => sum + slice.tokens, 0);
  // token 全为 0（例如只有业务代理请求）⇒ 占比按**请求数**算，否则会全是「—」。
  const totalRequests = slices.reduce((sum, slice) => sum + slice.requests, 0);
  const useTokens = total > 0;

  return (
    <div className="space-y-3">
      {slices.map((slice, index) => {
        const value = useTokens ? slice.tokens : slice.requests;
        const denominator = useTokens ? total : totalRequests;
        const share = sharePercent(value, denominator);
        return (
          <div key={slice.keyId ?? slice.name} className="min-w-0">
            <div className="flex min-w-0 items-center gap-3 text-xs">
              <span className="w-5 shrink-0 font-mono text-muted-foreground">
                {String(index + 1).padStart(2, "0")}
              </span>
              <span
                className="min-w-0 flex-1 truncate font-medium"
                title={slice.keyId ?? slice.name}
              >
                {slice.name}
              </span>
              <span className="shrink-0 tabular-nums" title={`${slice.tokens} Token`}>
                {compactTokens(slice.tokens)}
              </span>
              <span className="w-14 shrink-0 text-right text-muted-foreground tabular-nums">
                {slice.requests} 次
              </span>
              <span className="w-12 shrink-0 text-right text-muted-foreground tabular-nums">
                {share}
              </span>
            </div>
            <div className="ml-8 mt-1.5 h-1.5 overflow-hidden rounded-full bg-muted">
              <div
                className="h-full rounded-full"
                style={{
                  width: share === "—" ? "0%" : `${(value / denominator) * 100}%`,
                  backgroundColor: seriesColor(index),
                }}
              />
            </div>
          </div>
        );
      })}
    </div>
  );
}


/** 视图取数：返回日志条目与 `id → Key 名` 映射。 */
export type KeyUsageLoader = () => Promise<{
  entries: readonly KeyUsageLogEntry[];
  keyNames: ReadonlyMap<string, string>;
}>;

/**
 * 「按密钥看用量」视图（**数据源注入**，WB / Trae 共用）。
 *
 * 为什么要注入而不是写死 `api.getGatewayLogs`：三条产品线的日志是**三套 API**
 * （`get_gateway_logs` / `get_trae_gateway_logs` / Qoder 的），但聚合与呈现逻辑
 * 完全一样 —— 写死一份就得复制三份，三份必然漂移。
 *
 * @param loader 取数函数（必须已含鉴权与错误处理语义）
 * @param window 时间窗口（用 `usageWindowKeys` + `dateKey` 过滤，与另三个视图同口径）
 */
export function KeysView({
  loader,
  window,
  selfScope,
  emptyTitle,
  emptyHint,
  title,
  caption,
}: {
  loader: KeyUsageLoader;
  window: UsageWindow;
  /** 本页所属台账（跨台账的 Key 会标出归属）。 */
  selfScope: string;
  emptyTitle: string;
  emptyHint: string;
  title: string;
  caption: string;
}) {
  const [entries, setEntries] = useState<readonly KeyUsageLogEntry[] | null>(null);
  const [keyNames, setKeyNames] = useState<ReadonlyMap<string, string>>(new Map());
  const [error, setError] = useState<string | null>(null);
  const [reload, setReload] = useState(0);

  useEffect(() => {
    let cancelled = false;
    setError(null);
    void (async () => {
      try {
        const result = await loader();
        if (cancelled) return;
        setEntries(result.entries);
        setKeyNames(result.keyNames);
      } catch (e) {
        if (!cancelled) setError(String(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [loader, reload]);

  const slices = useMemo(() => {
    if (!entries) return [];
    /* 窗口过滤复用**同一个**日期键算法 —— 自己写 `toISOString().slice(0,10)`
       会按 UTC 切日，与另三个视图在时区边界上差一天。 */
    const inWindow = new Set(usageWindowKeys(window));
    const filtered = entries.filter((entry) => inWindow.has(dateKey(new Date(entry.ts))));
    return aggregateByKey(filtered, keyNames, selfScope);
  }, [entries, keyNames, window, selfScope]);

  const reloadAction = (
    <button
      type="button"
      className="rounded-md border border-border px-2.5 py-1 text-xs hover:bg-muted"
      onClick={() => setReload((value) => value + 1)}
    >
      重新读取日志
    </button>
  );

  if (error) {
    return (
      <div className="rounded-xl border border-border p-6 text-center">
        <p className="text-sm font-medium">{emptyTitle}</p>
        <p className="mt-1 text-xs text-muted-foreground">{error}</p>
        <div className="mt-3">{reloadAction}</div>
      </div>
    );
  }
  if (!entries) {
    return <div className="h-[180px] animate-pulse rounded-xl bg-muted" />;
  }
  if (slices.length === 0) {
    return (
      <div className="rounded-xl border border-border p-6 text-center">
        <p className="text-sm font-medium">{emptyTitle}</p>
        <p className="mt-1 text-xs text-muted-foreground">{emptyHint}</p>
        <div className="mt-3">{reloadAction}</div>
      </div>
    );
  }
  return (
    <div className="space-y-3">
      <div className="text-xs font-medium">{title}</div>
      <KeyRanking slices={slices} />
      <p className="text-[11.5px] leading-5 text-muted-foreground">{caption}</p>
    </div>
  );
}
