import { useState } from "react";
import { Eraser, Loader2, ScrollText } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { DemoAction } from "@/components/demo-action";
import * as api from "@/lib/api";
import { useT } from "@/lib/i18n";
import { regionDescriptor } from "@/lib/region";
import { cn } from "@/lib/utils";
import type { GatewayLogEntry } from "@/lib/types";
import { useGatewayStore } from "@/stores/gateway";

/** 单次渲染的日志行上限（与 Trae 侧 `trae-request-log.tsx` **同值**）。 */
const MAX_LOG_ROWS = 50;

function formatClock(ts: number): string {
  const date = new Date(ts);
  if (Number.isNaN(date.getTime())) return "—";
  return `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}:${String(date.getSeconds()).padStart(2, "0")}`;
}

function formatTokens(value: number | null | undefined): string {
  if (value == null) return "—";
  return new Intl.NumberFormat("zh-CN").format(value);
}

function statusTone(status: number): string {
  if (status >= 200 && status < 300) return "text-success";
  if (status === 429) return "text-warning";
  return "text-destructive";
}

/**
 * 最近 N 条请求日志（仅元数据）+ 清空（P1-1）。
 *
 * ## 2026-10-13 跨平台对齐：骨架统一到 **table 形态**
 *
 * 此前本组件是**自建形态**（无表头的 grid 行 + 一行灰字空态 + 延迟显示 `1.2s`），
 * 而 Trae 侧 `trae-request-log.tsx` 是 `<table>` + sticky 表头 + `EmptyState`。
 * 同一件事（看最近请求）在两个页面长得不一样 ⇒ 统一到 table 形态：
 *
 * - **列集** = 时间 / 版本 / 账号 / 模型 / 状态 / 耗时 / Token（Trae 无「版本」列 ——
 *   它的日志没有 region 维度，属**数据差异**，不是形态差异）；
 * - **空态**用全应用唯一的 `EmptyState`（§3.6），不再是「一行灰字」；
 * - **耗时统一用 `ms`**（原始单位，`1.2s` 会把 1,240ms 与 1,199ms 显示成同一个值）；
 * - **条数上限 50** 提到常量，与 Trae 侧同值。
 *
 * 保留 WB 特有的**版本列**：每条日志都带 `region`，用户需要知道这次调用走的哪条产品线。
 */
export function RequestLog({ className }: { className?: string }) {
  const t = useT();
  const logs = useGatewayStore((s) => s.logs);
  const clearLogs = useGatewayStore((s) => s.clearLogs);
  const loadLogs = useGatewayStore((s) => s.loadLogs);
  const [clearing, setClearing] = useState(false);

  async function onClear() {
    setClearing(true);
    try {
      await clearLogs();
      await loadLogs();
      toast.success(t("wbStats.gateway.cleared"));
    } catch (e) {
      toast.error(t("wbStats.gateway.clearFail"), { description: api.asError(e) });
    } finally {
      setClearing(false);
    }
  }

  /* store 的 `loadLogs` 已按 `ts` **降序**写入（新在前），这里只截取上限。 */
  const rows = logs.slice(0, MAX_LOG_ROWS);

  return (
    <Card className={cn("gap-0 py-0", className)}>
      <div className="flex items-center justify-between gap-3 border-b border-border/60 px-5 py-3">
        <span className="text-sm font-semibold">{t("wbStats.gateway.requestLog")}</span>
        <DemoAction>
          <Button variant="ghost" size="sm" onClick={() => void onClear()} disabled={clearing || logs.length === 0}>
            {clearing ? <Loader2 className="animate-spin" /> : <Eraser />}
            {t("wbStats.gateway.clear")}
          </Button>
        </DemoAction>
      </div>

      {rows.length === 0 ? (
        <EmptyState
          icon={ScrollText}
          title={t("wbStats.gateway.noRequestsLog")}
          className="px-5 py-6"
        />
      ) : (
        <div className="max-h-80 overflow-y-auto">
          <table className="w-full text-sm">
            <thead className="sticky top-0 bg-card text-xs text-muted-foreground">
              <tr className="border-b border-border/60">
                <th className="px-5 py-2 text-left font-medium">{t("wbStats.gateway.log.col.time")}</th>
                <th className="px-3 py-2 text-left font-medium">{t("wbStats.gateway.log.col.version")}</th>
                <th className="px-3 py-2 text-left font-medium">{t("wbStats.gateway.log.col.account")}</th>
                <th className="px-3 py-2 text-left font-medium">{t("wbStats.gateway.log.col.model")}</th>
                <th className="px-3 py-2 text-right font-medium">{t("wbStats.gateway.log.col.status")}</th>
                <th className="px-3 py-2 text-right font-medium">{t("wbStats.gateway.log.col.latency")}</th>
                <th className="px-5 py-2 text-right font-medium">{t("wbStats.gateway.log.col.tokens")}</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((entry, index) => (
                <LogRow key={`${entry.ts}-${index}`} entry={entry} />
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Card>
  );
}

function LogRow({ entry }: { entry: GatewayLogEntry }) {
  return (
    <tr className="border-b border-border/40 last:border-0">
      <td className="whitespace-nowrap px-5 py-2 font-mono text-xs">{formatClock(entry.ts)}</td>
      <td className="px-3 py-2 text-xs text-muted-foreground">
        {regionDescriptor(entry.region).versionLabel}
      </td>
      {/* 账号 / 模型：显示**原值** + `truncate`（完整值放 `title`）——
          与 Trae 侧同一做法（此前 Trae 只显示末 8 位，两边对不上）。 */}
      <td className="max-w-[10rem] truncate px-3 py-2 text-xs text-muted-foreground" title={entry.account ?? undefined}>
        {entry.account || "—"}
      </td>
      <td className="max-w-[10rem] truncate px-3 py-2 text-xs text-muted-foreground" title={entry.model ?? undefined}>
        {entry.model || "—"}
      </td>
      <td className={cn("px-3 py-2 text-right font-mono text-xs", statusTone(entry.status))}>
        {entry.status}
      </td>
      {/* 耗时用 `ms`（原始单位）—— `toFixed(1)s` 会把相邻两档显示成同一个值。 */}
      <td className="px-3 py-2 text-right font-mono text-xs text-muted-foreground">
        {entry.latencyMs ? `${entry.latencyMs}ms` : "—"}
      </td>
      <td className="px-5 py-2 text-right font-mono text-xs text-muted-foreground">
        {entry.promptTokens != null || entry.completionTokens != null
          ? formatTokens((entry.promptTokens ?? 0) + (entry.completionTokens ?? 0))
          : "—"}
      </td>
    </tr>
  );
}
