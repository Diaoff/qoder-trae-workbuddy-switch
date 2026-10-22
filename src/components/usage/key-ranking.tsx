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

import { dateKey, usageWindowKeys, type UsageWindow } from "@/lib/usage-window";

import { compactTokens, sharePercent } from "./format";
import { seriesColor } from "./palette";

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
    const bucketKey = id ?? "__none__";
    const slice = buckets.get(bucketKey) ?? {
      keyId: id,
      name: id ? (keyNames.get(id) ?? shortId(id)) : "无密钥调用",
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
function shortId(id: string): string {
  return id.length > 8 ? `${id.slice(0, 8)}…` : id;
}

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
  emptyTitle,
  emptyHint,
  title,
  caption,
}: {
  loader: KeyUsageLoader;
  window: UsageWindow;
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
    return aggregateByKey(filtered, keyNames);
  }, [entries, keyNames, window]);

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
