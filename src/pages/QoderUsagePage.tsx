/**
 * 「用量」· Qoder 分区（趋势）。
 *
 * ## ★★ 为什么这里的单位是 **credits**，不是 Token
 *
 * Qoder 与另两个平台的计费口径**根本不同**：Qoder CN 客户端按 **credits** 计费，
 * 而它三处本地存储里的 **token 计数恒为 0**（已取证）：
 *
 * | 来源 | 证据 |
 * |:--|:--|
 * | 会话 jsonl（`~/.qoder-cn/projects/**`） | `usage.input_tokens/output_tokens = 0`，同一对象里 `credits` **有真值** |
 * | 运行日志（`~/.qoder-cn/logs/**`） | `input_tokens=0 output_tokens=0`（全量统计只有 0） |
 * | 客户端 SQLite（`main.sqlite`） | `snapshot_json` 自己写着 `"tokenCountsAvailable":false` |
 *
 * ⇒ 画一张「逐日 Token」的图**只能得到一根贴地的 0 线**，那是**编造**。
 * 本页因此换成一个**诚实的单位**：credits（并如实标注来源与局限，见页面底部的说明条）。
 *
 * ## 与另两个平台的用量页的差异**只在数据与单位**
 *
 * 骨架（四统计卡 → 逐日柱 → 按模型拆分 → 来源说明）与 WB / Trae 同款；
 * 差异是 Qoder 没有「会话」维度（客户端会话记录里没有账号 uid，反查不到人）。
 *
 * ## 视图参数是**受控**的
 *
 * `viewWindow` 由 `UsagePage` 传入（参数条也在那边渲染）—— 本页**不持有**窗口状态，
 * 也**不放**任何页内选择器（与 `TraeTokenStatsPage` 同一条纪律）。
 *
 * ## 为什么「按模型拆分」没有复用 `ModelRanking`
 *
 * 那个共用原语的数字格式与 tooltip **写死了 Token 单位**（`compactTokens` +
 * `` `${…} Token` ``）。在 credits 视图里复用它会让图注与 tooltip **谎报单位** ——
 * 宁可这里写一段 20 行的自有列表，也不要一个说错单位的共用组件。
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { AlertTriangle, Coins, Hash, Layers, MessagesSquare, RefreshCw } from "lucide-react";
import { Bar, BarChart, CartesianGrid, XAxis, YAxis } from "recharts";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { ChartFrame } from "@/components/usage/chart-frame";
import { StatTiles, type StatTile } from "@/components/usage/stat-tiles";
import * as api from "@/lib/api";
import { creditTrendCaption } from "@/lib/chart-caption";
import { formatCredits } from "@/components/usage/format";
import { useT } from "@/lib/i18n";
import { platformOfPath } from "@/lib/platforms/registry";
import { scopeOf } from "@/lib/scope";
import { useLocation } from "react-router-dom";
import { useScope } from "@/lib/use-scope";
import { useRefreshAction } from "@/lib/use-refresh-action";
import { usageWindowSpanDays, type UsageWindow } from "@/lib/usage-window";

interface Props {
  /** 受控的取数窗口（参数条在 `UsagePage`）。 */
  viewWindow: UsageWindow;
}

export default function QoderUsagePage({ viewWindow }: Props) {
  const t = useT();
  const location = useLocation();
  const product = platformOfPath(location.pathname);
  const [region] = useScope(product);
  const scope = scopeOf(product, region);

  const days = usageWindowSpanDays(viewWindow);
  const [data, setData] = useState<api.QoderUsageView | null>(null);
  const [error, setError] = useState("");
  const { busy, run } = useRefreshAction();

  const load = useCallback(async () => {
    setError("");
    try {
      setData(await api.qoderUsage(scope, days));
    } catch (e) {
      setError(api.asError(e));
    }
  }, [scope, days]);

  useEffect(() => {
    void load();
  }, [load]);

  const totals = data?.totals;
  const tiles = useMemo<StatTile[]>(() => {
    if (!data || !totals) return [];
    return [
      {
        icon: Coins,
        label: t("qoder.page.usage.totals.credits"),
        value: formatCredits(totals.credits),
        hint: t("qoder.page.usage.unit.credits"),
      },
      {
        icon: Hash,
        label: t("qoder.page.usage.totals.requests"),
        value: formatCredits(totals.requests),
      },
      {
        icon: MessagesSquare,
        label: t("qoder.page.usage.totals.sessions"),
        value: formatCredits(totals.sessions),
      },
      {
        icon: Layers,
        label: t("qoder.page.usage.totals.models"),
        value: formatCredits(totals.models),
      },
    ];
  }, [data, totals, t]);

  /* 逐日柱的数据：`date` 截成 `MM-DD` 当横轴标签（完整 ISO 太长，会把轴挤满）。 */
  const bars = useMemo(
    () =>
      (data?.days ?? []).map((day) => ({
        label: day.date.slice(5),
        credits: day.credits,
      })),
    [data],
  );
  const peak = useMemo(
    () => bars.reduce((max, bar) => Math.max(max, bar.credits), 0),
    [bars],
  );

  if (error) {
    return (
      <Alert variant="destructive">
        <AlertTriangle />
        <AlertTitle>{t("qoder.page.usage.loadFailed")}</AlertTitle>
        <AlertDescription>
          <span>{error}</span>
          <div className="mt-2">
            <Button size="sm" variant="outline" onClick={() => void run(() => load())} disabled={busy}>
              {busy ? <RefreshCw className="animate-spin" /> : <RefreshCw />}
              {t("qoder.page.usage.recheck")}
            </Button>
          </div>
        </AlertDescription>
      </Alert>
    );
  }

  if (!data) return <Skeleton className="h-32" />;

  /* ★ 两种空态**绝不混用**：目录不存在（去装客户端 / 去对话）与
     「有目录但这段时间没有记录」（去换个窗口）是两种不同的用户动作。 */
  if (!data.source.exists) {
    return (
      <Card className="gap-0 py-0">
        <EmptyState
          icon={MessagesSquare}
          title={t("qoder.page.usage.missing.title")}
          description={t("qoder.page.usage.missing.description", {
            path: data.source.paths[0] ?? "~/.qoder-cn/projects",
          })}
        />
      </Card>
    );
  }

  if (data.days.length === 0) {
    return (
      <Card className="gap-0 py-0">
        <EmptyState
          icon={MessagesSquare}
          title={t("qoder.page.usage.empty.title")}
          description={t("qoder.page.usage.empty.description")}
        />
      </Card>
    );
  }

  return (
    <div className="flex min-w-0 flex-col gap-4">
      <StatTiles tiles={tiles} />

      <ChartFrame
        title={t("qoder.page.usage.trend")}
        caption={creditTrendCaption(t, totals?.credits ?? 0, peak)}
        ariaLabel={t("qoder.page.usage.trend")}
        exportable={false}
      >
        <BarChart data={bars} margin={{ top: 8, right: 8, bottom: 0, left: 0 }}>
          <CartesianGrid vertical={false} stroke="var(--border)" />
          <XAxis
            dataKey="label"
            tickLine={false}
            axisLine={false}
            tick={{ fontSize: 11, fill: "var(--muted-foreground)" }}
          />
          <YAxis
            tickLine={false}
            axisLine={false}
            width={48}
            tick={{ fontSize: 11, fill: "var(--muted-foreground)" }}
            tickFormatter={(value: number) => formatCredits(value)}
          />
          <Bar dataKey="credits" fill="var(--primary)" radius={[3, 3, 0, 0]} />
        </BarChart>
      </ChartFrame>

      {/* 按模型拆分：**自有列表**（不复用 `ModelRanking` —— 它的单位写死了 Token，见文件头）。 */}
      <ChartFrame
        title={t("qoder.page.usage.byModel")}
        ariaLabel={t("qoder.page.usage.byModel")}
        exportable={false}
      >
        <div className="space-y-2.5">
          {(data.byModel ?? []).map((row, index) => {
            const total = (data.byModel ?? []).reduce((sum, item) => sum + item.credits, 0);
            const ratio = total > 0 ? (row.credits / total) * 100 : 0;
            return (
              <div key={row.model} className="min-w-0">
                <div className="flex min-w-0 items-baseline gap-3 text-xs">
                  <span className="w-5 shrink-0 font-mono text-muted-foreground">
                    {String(index + 1).padStart(2, "0")}
                  </span>
                  <span className="min-w-0 flex-1 truncate font-medium" title={row.model}>
                    {row.model}
                  </span>
                  <span className="shrink-0 tabular-nums">
                    {t("qoder.page.usage.creditsValue", { value: formatCredits(row.credits) })}
                  </span>
                </div>
                <div className="mt-1 ml-8 h-1.5 overflow-hidden rounded-full bg-foreground/[0.06]">
                  <div className="h-full rounded-full bg-primary" style={{ width: `${ratio}%` }} />
                </div>
              </div>
            );
          })}
        </div>
      </ChartFrame>

      {/* ★ 来源说明**必须展示**：用户看到「数字比预期小」时，唯一能解释的就是这一条
          （它统计的是**本机客户端会话记录**，不含其它设备、也不含经网关的第三方调用）。 */}
      <Alert>
        <AlertDescription>{data.source.note}</AlertDescription>
      </Alert>
    </div>
  );
}
