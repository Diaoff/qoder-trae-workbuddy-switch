/**
 * 「用量」· **Token 视图**（Trae 分区）：趋势 / 模型 —— 与 WorkBuddy **同一套骨架与图表**。
 *
 * ## 严格按原型实现（用户 2026-10-12 要求）
 *
 * 原型 `viewUsage`（`docs/redesign/demo-app.js:794-997`）是**平台无关**的
 * （同一段代码 + 不同数据），两个 Token 视图的块序逐条对齐：
 *
 * - **趋势**：4 统计卡 → `每日 Token · 按模型拆分`（堆叠柱 + 图例）
 *   → `累计消耗曲线` | `模型占比`（g2）→ `使用时段分布`（7×24 热力图）；
 * - **模型**：4 统计卡 → `模型 Token 占比`（环形）→ `模型用量排行`（横向条 + 迷你趋势线）。
 *
 * 改造前本页是 Trae 自建的一套（整年活动网格 / 每日折线 / 按模型柱 / 按模型明细 /
 * 按账号表 / 状态码卡）—— **原型里一个都没有**。现已全部替换：
 * 「按账号」与「状态码」两个维度在原型里是**积分页的两个环形**（见 `TraeCreditsPage`），
 * 不是趋势页的卡片。
 *
 * ## 与 WorkBuddy 的差异**只在数据**
 *
 * | 维度 | WorkBuddy | Trae（本页） |
 * |:--|:--|:--|
 * | 数据源 | 客户端会话文件（4 来源） | 本机网关请求日志（单源） |
 * | 「活跃账号」统计卡 | **无账号维度** ⇒ 用「模型数」替代（门禁已登记） | 有 `accounts` ⇒ 原型原样「活跃账号」 |
 * | 数据源提示条 | 无 | 「数据来源」Alert（**必须保留**，否则用户把「数字小」当 bug） |
 *
 * 图表**一律走共用原语**（`StackedDailyBar` / `CumulativeArea` / `ModelDonut` /
 * `ModelRanking` / `UsageHeatmap`），数据由 `lib/trae-usage-charts.ts` 映射成共用形状
 * ⇒ 契约（分段和=当日总量 / 7×24 / 扇区数=模型数）只有一份实现。
 *
 * ## 视图参数是**受控**的
 *
 * `range` / `combineAll` 由 `UsagePage` 传入（原型 `state.range` / `state.merge` 是页面级的）。
 * 本页**不持有**参数状态，也**不放**任何页内区域选择器 —— 区域归侧栏。
 */

import { useCallback, useMemo } from "react";
import {
  AlertTriangle,
  Gauge,
  Info,
  Loader2,
  MessagesSquare,
  RefreshCw,
  Server,
  Sparkles,
  TrendingUp,
  Users,
} from "lucide-react";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { CardHead } from "@/components/ui/card-head";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { scopeLabel } from "@/components/scope-chip";
import { ChartFrame } from "@/components/usage/chart-frame";
import { CumulativeArea } from "@/components/usage/cumulative-area";
import { compactTokens, exactTokens } from "@/components/usage/format";
import { ModelDonut } from "@/components/usage/model-donut";
import { ModelRanking } from "@/components/usage/model-ranking";
import { StackedDailyBar } from "@/components/usage/stacked-daily-bar";
import { StatTiles, type StatTile } from "@/components/usage/stat-tiles";
import { UsageHeatmap } from "@/components/usage/usage-heatmap";
import { useUsageWindowLabel, type UsageViewProps } from "@/components/usage/view-param-bar";
import * as api from "@/lib/api";
import { cumulativeCaption } from "@/lib/chart-caption";
import { useT } from "@/lib/i18n";
import { scopeOf } from "@/lib/scope";
import { toTokenStatsSource } from "@/lib/trae-usage-charts";
import type { TraeTokenScope, TraeTokenStatistics, TraeUnsupported } from "@/lib/trae-types";
import { useCachedResource } from "@/lib/use-cached-resource";
import { useRefreshAction } from "@/lib/use-refresh-action";
import { useScope } from "@/lib/use-scope";
import {
  dailyModelSeries,
  heatmapMatrix,
  modelSlices,
} from "@/lib/usage-charts";
import { usageWindowDays, usageWindowSpanDays, type UsageWindow } from "@/lib/usage-window";

/** 本页承载的视图（积分视图在 `TraeCreditsPage`）。与原型 `tabs` 的前两项同名。 */
export type TraeUsageView = "trend" | "models";

/**
 * 「用量 · Token」视图（Trae 分区）。
 *
 * `range` / `combineAll` 受控；`view` 决定画趋势还是模型。
 */
export default function TraeTokenStatsPage({
  view = "trend",
  viewWindow,
  combineAll,
}: { view?: TraeUsageView } & UsageViewProps) {
  const t = useT();
  const [region] = useScope("trae");
  const scope = scopeOf("trae", region);
  /**
   * 范围筛选（`TraeTokenScope`）= **区域**档：`cn` / `global` / `unlabeled` / `all`。
   *
   * 「仅本作用域」= 当前区域；「合并全部作用域」= `all`（国内 ∪ 国际 ∪ 未标注旧日志）。
   * 与 WorkBuddy 的 `RegionFilter` 是**同一根轴的两个取值形态** ⇒ 共用同一枚控件。
   */
  const tokenScope: TraeTokenScope = combineAll ? "all" : region;
  /* ★ Trae 的统计接口按「最近 N 天」拉日志（`days <= 0` = 不限）⇒ 必须拉**覆盖整个窗口**
     的跨度，而不是窗口自己的天数：窗口可能是**锚在过去**的一段（如 9/1–9/5），
     拉 5 天只会拉到最后 5 天，那段历史**根本不在返回里**，图表会**静默全 0**。
     拉回来之后再按窗口的日历日切片 —— 多拉的日在这两步之间被丢掉。 */
  const spanDays = usageWindowSpanDays(viewWindow);

  const load = useCallback(
    () => api.getTraeTokenStatistics(spanDays, tokenScope),
    [spanDays, tokenScope],
  );

  const {
    data: stats,
    loading,
    error,
    refresh,
  } = useCachedResource<TraeTokenStatistics>(`trae:token-stats:${spanDays}:${tokenScope}`, load);

  /** 重试按钮的进行中态（`loading` 只在无值时 true，点重试那一刻它恒为 false）。 */
  const { busy, run } = useRefreshAction();

  /** 映射成共用形状后，下面全部走与 WorkBuddy 同一批适配函数（见 `lib/trae-usage-charts.ts`）。 */
  const source = useMemo(() => (stats ? toTokenStatsSource(stats) : null), [stats]);
  const columns = useMemo(
    () => (source ? dailyModelSeries(source, viewWindow) : []),
    [source, viewWindow],
  );
  const slices = useMemo(
    () => (source ? modelSlices(source, viewWindow) : []),
    [source, viewWindow],
  );
  const matrix = useMemo(() => (source ? heatmapMatrix(source.hours) : null), [source]);

  /* ⚠️ 加载 / 失败态不带页头与页容器：页头由上层 `UsagePage` 统一渲染。 */
  if (loading && !stats) {
    return (
      <div className="min-w-0 space-y-6" aria-busy="true">
        <Skeleton className="h-[74px] w-full" />
        <Skeleton className="h-64 w-full" />
        <Skeleton className="h-40 w-full" />
      </div>
    );
  }

  if (error && !stats) {
    return (
      <div className="min-w-0">
        <Alert variant="destructive">
          <AlertTriangle />
          <AlertTitle>{t("trae.stats.token.loadFailed")}</AlertTitle>
          <AlertDescription className="flex flex-col gap-3">
            <span>{error}</span>
            <div>
              <Button variant="outline" size="sm" disabled={busy} onClick={() => void run(() => refresh())}>
                {busy ? <Loader2 className="animate-spin" /> : <RefreshCw />}
                {t("trae.stats.token.retry")}
              </Button>
            </div>
          </AlertDescription>
        </Alert>
      </div>
    );
  }

  const summary = stats?.summary;
  const accounts = stats?.accounts ?? [];
  const unsupported = stats?.unsupported ?? [];
  const empty = (summary?.records ?? 0) === 0;

  return (
    <div className="min-w-0 space-y-6">
      {/* 数据源边界：不写清楚，用户会把「数字小」当成 bug。**必须保留**（提示条，非内容块）。 */}
      <Alert>
        <Info />
        <AlertTitle>{t("trae.stats.token.sourceTitle")}</AlertTitle>
        <AlertDescription>
          <span className="break-all">{stats?.note ?? t("trae.stats.token.sourceFallback")}</span>
          {stats?.logFile && (
            <span className="mt-1 block break-all font-mono text-xs text-muted-foreground">
              {stats.logFile}
            </span>
          )}
        </AlertDescription>
      </Alert>

      {empty ? (
        /* 空窗口：原型每个视图都有自己的空态块（四要素含出口）。 */
        <EmptyState
          icon={Server}
          title={t("trae.stats.token.emptyTitle")}
          description={t("trae.stats.token.emptyBody")}
          action={
            <Button size="sm" variant="outline" onClick={() => void refresh()}>
              {t("trae.stats.token.refresh")}
            </Button>
          }
        />
      ) : view === "models" ? (
        <ModelsView slices={slices} onRefresh={() => void refresh()} />
      ) : (
        <TrendView
          columns={columns}
          slices={slices}
          matrix={matrix}
          accounts={accounts.length}
          summaryTotal={summary?.total ?? 0}
          viewWindow={viewWindow}
          scopeName={combineAll ? t("shared.region.combine.all") : scopeLabel(t, scope)}
        />
      )}

      {/* ---- 平台做不到的维度（置灰卡；形状来自 handlers::unsupported_note）----
          不是原型的内容块，而是**诚实性要求**：Trae 没有「按项目 / 会话」维度，
          不说清楚用户会以为「统计漏了」。 */}
      {unsupported.length > 0 && <UnsupportedSection items={unsupported} />}

      {/* 日志解析失败：**诚实性提示** —— 数字不完整时必须说（不是原型的内容块，
          但与 `unsupported` 同类：不说明会让用户把「少算」当成 bug）。 */}
      {stats && stats.parseErrors > 0 && (
        <p className="flex items-center gap-1.5 px-1 text-xs text-warning">
          <AlertTriangle className="size-3.5" aria-hidden="true" />
          {t("trae.stats.token.parseErrors", { count: stats.parseErrors })}
        </p>
      )}
    </div>
  );
}

/* ────────────────────────────── 趋势（原型四块） ────────────────────────────── */

function TrendView({
  columns,
  slices,
  matrix,
  accounts,
  summaryTotal,
  viewWindow,
  scopeName,
}: {
  columns: ReturnType<typeof dailyModelSeries>;
  slices: ReturnType<typeof modelSlices>;
  matrix: ReturnType<typeof heatmapMatrix> | null;
  accounts: number;
  summaryTotal: number;
  viewWindow: UsageWindow;
  scopeName: string;
}) {
  const t = useT();
  const windowLabel = useUsageWindowLabel();

  const total = columns.reduce((sum, column) => sum + column.total, 0);
  /* 日均的分母是**窗口天数**（不是「有数据的天数」）—— 与 WB 的 A1-3 同一口径。 */
  const average = Math.round(total / usageWindowDays(viewWindow));
  const peak = columns.reduce(
    (best, column) => (column.total > best.total ? column : best),
    columns[0] ?? { day: "", label: "", total: 0, segments: [] },
  );
  const cumulative = useMemo(() => {
    let running = 0;
    return columns.map((column) => {
      running += column.total;
      return { label: column.label, value: running };
    });
  }, [columns]);
  const peakDelta = columns.reduce((max, column) => Math.max(max, column.total), 0);
  const heatmapHasData = matrix?.cells.some((row) => row.some((value) => value > 0)) ?? false;

  /* 四枚统计卡 = 原型 `statTile` 的区间总 Token / 日均 / 峰值 / **活跃账号**。
     ⚠️ 原型第 4 枚是「活跃账号」—— WB 侧**没有账号维度**，用「模型数」替代（已登记）；
     Trae 有 `accounts` ⇒ **按原型原样**用「活跃账号」。这是数据差异，不是做法差异。 */
  const tiles: StatTile[] = [
    {
      icon: MessagesSquare,
      label: t("wbStats.usage.trend.total"),
      value: compactTokens(summaryTotal),
      hint: windowLabel(viewWindow),
    },
    {
      icon: TrendingUp,
      label: t("wbStats.usage.trend.avg"),
      value: compactTokens(average),
      hint: t("wbStats.usage.trend.avgHint"),
    },
    {
      icon: Gauge,
      label: t("wbStats.usage.trend.peak"),
      value: compactTokens(peak.total),
      hint: peak.label || undefined,
    },
    {
      icon: Users,
      label: t("trae.stats.token.summary.activeAccounts"),
      value: exactTokens(accounts),
      hint: scopeName,
    },
  ];

  return (
    <>
      <StatTiles tiles={tiles} ariaLabel={t("trae.stats.token.summaryAria")} />

      {columns.length > 0 ? (
        <ChartFrame
          title={t("wbStats.usage.trend.stackedTitle")}
          caption={t("wbStats.usage.trend.stackedDesc")}
          ariaLabel={t("wbStats.usage.trend.stackedAria")}
        >
          <StackedDailyBar columns={columns} />
        </ChartFrame>
      ) : null}

      <div className="grid min-w-0 gap-6 lg:grid-cols-2">
        {cumulative.length > 0 ? (
          <ChartFrame
            title={t("wbStats.usage.trend.cumulativeTitle")}
            caption={cumulativeCaption(t, cumulative[cumulative.length - 1].value, peakDelta)}
            ariaLabel={t("wbStats.usage.trend.cumulativeTitle")}
          >
            <CumulativeArea points={cumulative} />
          </ChartFrame>
        ) : null}
        {slices.length > 0 ? (
          <ChartFrame
            title={t("wbStats.usage.trend.donutTitle")}
            caption={`${exactTokens(slices.length)} · ${compactTokens(total)}`}
            ariaLabel={t("wbStats.usage.trend.donutTitle")}
          >
            <ModelDonut slices={slices} size={150} />
          </ChartFrame>
        ) : null}
      </div>

      {heatmapHasData && matrix ? (
        <ChartFrame
          title={t("wbStats.usage.trend.heatmapTitle")}
          caption={t("wbStats.usage.trend.heatmapDesc")}
          ariaLabel={t("wbStats.usage.trend.heatmapAria")}
          exportable={false}
        >
          <UsageHeatmap matrix={matrix} />
        </ChartFrame>
      ) : null}
    </>
  );
}

/* ────────────────────────────── 模型（原型三块） ────────────────────────────── */

function ModelsView({
  slices,
  onRefresh,
}: {
  slices: ReturnType<typeof modelSlices>;
  onRefresh: () => void;
}) {
  const t = useT();
  const total = slices.reduce((sum, slice) => sum + slice.value, 0);
  const top = slices[0];

  if (slices.length === 0) {
    /* 原型 `emptyBlock(ICON.usage, "暂无模型用量", …)` —— 出口由 §3.6 补上。 */
    return (
      <EmptyState
        icon={Server}
        title={t("wbStats.usage.empty.models")}
        description={t("wbStats.usage.empty.modelsHint")}
        action={
          <Button size="sm" variant="outline" onClick={onRefresh}>
            {t("wbStats.usage.empty.refresh")}
          </Button>
        }
      />
    );
  }

  const tiles: StatTile[] = [
    { icon: Server, label: t("wbStats.usage.models.count"), value: exactTokens(slices.length) },
    {
      icon: Sparkles,
      label: t("wbStats.usage.models.top"),
      value: top?.name ?? "—",
      hint: top ? `${((top.value / total) * 100).toFixed(0)}%` : undefined,
    },
    { icon: MessagesSquare, label: t("wbStats.usage.models.total"), value: compactTokens(total) },
    { icon: Gauge, label: t("wbStats.usage.models.avg"), value: compactTokens(Math.round(total / slices.length)) },
  ];

  return (
    <>
      <StatTiles tiles={tiles} ariaLabel={t("wbStats.usage.view.models")} />
      <ChartFrame
        title={t("wbStats.usage.models.donutTitle")}
        caption={`${exactTokens(slices.length)} · ${compactTokens(total)}`}
        ariaLabel={t("wbStats.usage.models.donutTitle")}
      >
        <ModelDonut slices={slices} />
      </ChartFrame>
      <ChartFrame
        title={t("wbStats.usage.models.rankTitle")}
        caption={t("wbStats.usage.models.rankDesc")}
        ariaLabel={t("wbStats.usage.models.rankTitle")}
        exportable={false}
      >
        <ModelRanking slices={slices} />
      </ChartFrame>
    </>
  );
}

/* ────────────────────────────── 平台做不到的维度 ────────────────────────────── */

function UnsupportedSection({ items }: { items: TraeUnsupported[] }) {
  const t = useT();
  return (
    <Card className="min-w-0 gap-0 overflow-hidden rounded-lg border-dashed py-0 shadow-none">
      <CardHead title={t("trae.stats.token.unsupportedTitle")} />
      <div className="divide-y divide-border/60">
        {items.map((item) => (
          <div key={item.capability} className="flex flex-wrap items-baseline gap-x-3 gap-y-1 px-4 py-3 opacity-70">
            <span className="text-sm font-medium text-muted-foreground">{item.label}</span>
            <span className="text-xs text-muted-foreground">
              {t("trae.stats.token.unsupportedOnly", { on: item.supportedOn })}
            </span>
            <span className="w-full text-xs leading-5 text-muted-foreground">{item.reason}</span>
          </div>
        ))}
      </div>
    </Card>
  );
}
