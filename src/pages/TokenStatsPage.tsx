/**
 * 「用量」工作台 · **Token 三视图**（趋势 / 模型 / 会话）—— P3 重做。
 *
 * ## 为什么重写
 *
 * 改造前本页是**旧实现**：4 组视图参数散在页内（来源切片 / 范围 / 区域 / 合并）、
 * 图表张数与图注口径都不是原型的四视图（COVERAGE-REPORT §2.12，12 条 ⚠️）。
 * 本版改为「**装配数据 → 渲染四视图组件**」：视图参数收敛为**一条** `ViewParamBar`
 * （范围 + 合并），图表逻辑全部下沉到 `components/usage/*` 与纯函数 `lib/usage-charts.ts`。
 *
 * ## 数据装配（关键决策）
 *
 * 原型「一个作用域 = 一份用量」⇒ 本页把当前范围下的**全部来源**
 * （workbuddy / codebuddy-cli / codebuddy-ide / workbuddy-ai）用 `mergeSources` 聚合成**一份**。
 * 合并作用域（`region === "all"`）时后端同时下发两版来源，一并求和 —— 因此
 * **图表数据（模型 / 热力图）真的跟着变多**（契约 12），不是只换 days。
 *
 * ## 网关逐日图（2026-10-15 新增，独立小图）
 *
 * 主堆叠柱的数据源是 `token_stats`（扫客户端日志）；**网关用量走另一条源**：
 * `get_gateway_token_trend`（读 `token_daily` 的 90 天落库历史）。它渲染成趋势视图里的
 * **独立小图**而不是并进主柱，两条理由（详见 `TrendView` 内注释）：
 * ① 主柱的「末柱 = 概览统计块 = 账号卡今日合计」是**跨视图一致性契约**，并进去会静默改口径；
 * ② 客户端与网关是**两条独立记录**（配置重叠时同一请求两处各记一次），求和会把重复计数藏起来。
 * 小图图注**恒披露覆盖率** —— 历史只覆盖「App 运行过」的日子，未记录日**不是 0**。
 *
 * ## 平台
 *
 * 本页只服务 WorkBuddy（`useScope("workbuddy")`）。Trae 侧的 Token 统计数据源形状不同
 * （`TraeTokenStatistics`），由 `TraeTokenStatsPage` 承担 —— 见 UsagePage 的平台映射。
 *
 * ## 契约（详见 `docs/specs/P3-ARCH.md`）
 *
 * - ★ **视图参数是受控的**（`range` / `combineAll` / `onRangeChange` 由 `UsagePage` 传入）：
 *   本页**不持有**范围与合并态 —— 原型 `viewUsage` 的 `state.range` / `state.merge`
 *   是页面级的，切子 Tab 不重置（见 `UsagePage` 文件头）。参数条也由 `UsagePage` 渲染一次。
 * - 空数组**不渲染**该图；整视图空 ⇒ 区块级 `EmptyState`（含出口）；
 * - 区分「真的没有」与「被视图参数筛没」；
 * - 所有展示字段过 `displayText`（白屏防线）；
 * - ★ **四个视图都必须消费 `viewWindow`**（趋势 / 模型 / 积分接口不变，会话是
 *   2026-10-15 补的口径）：会话此前吃全时数据、切时间纹丝不动（用户报障），
 *   现在按 `sessions[].daily`（后端逐日拆分）在 `sessionPoints` 里按窗口求和；
 * - ★ **会话视图只渲染前 `SESSION_RENDER_LIMIT` 个**（**有意偏离** P3 契约 8
 *   「排行条数 = 气泡数 = 会话数」—— 会话数可达 1.5 万，全量渲染要等数秒；
 *   截断由渲染层显式说明「共 M 个」，统计 tile 仍按全量算。理由见该常量注释）。
 */

import { useEffect, useMemo, useState } from "react";
import { CircleAlert, Coins, Gauge, Loader2, MessagesSquare, RefreshCw, Server, Sparkles, TrendingUp, Users } from "lucide-react";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { ChartFrame } from "@/components/usage/chart-frame";
import { CumulativeArea } from "@/components/usage/cumulative-area";
import { compactTokens, exactTokens } from "@/components/usage/format";
import { ModelDonut } from "@/components/usage/model-donut";
import { ModelRanking } from "@/components/usage/model-ranking";
import { SessionRanking } from "@/components/usage/session-ranking";
import { SessionScatter } from "@/components/usage/session-scatter";
import { StackedDailyBar } from "@/components/usage/stacked-daily-bar";
import { StatTiles, type StatTile } from "@/components/usage/stat-tiles";
import { UsageHeatmap } from "@/components/usage/usage-heatmap";
import { useUsageWindowLabel, type UsageViewProps } from "@/components/usage/view-param-bar";
import * as api from "@/lib/api";
import { cumulativeCaption, gatewayTrendCaption, scatterAxisCaption } from "@/lib/chart-caption";
import { useT } from "@/lib/i18n";
import type { GatewayTokenTrend, Region, RegionFilter, TokenStatistics, TokenStatsSource } from "@/lib/types";
import { useRefreshAction } from "@/lib/use-refresh-action";
import { useScope } from "@/lib/use-scope";
import {
  SESSION_RENDER_LIMIT,
  calendarWindowKeys,
  dailyModelSeries,
  gatewayCoverage,
  gatewayDailyColumns,
  heatmapMatrix,
  mergeGatewayTrends,
  mergeSources,
  modelSlices,
  sessionPoints,
} from "@/lib/usage-charts";
import { usageWindowDays, type UsageWindow } from "@/lib/usage-window";

/** 本页承载的视图（积分视图在 `CreditStatsPage`）。 */
export type TokenUsageView = "trend" | "models" | "sessions";

/** 会话级缓存：切走再切回不重取（与 `CreditStatsPage` 同一约定）。 */
const statisticsCache: Partial<Record<RegionFilter, TokenStatistics>> = {};

/**
 * 网关逐日趋势的**分 region 缓存**（与 `statisticsCache` 同约定）。
 *
 * 值可以是 `null`（该 region 没有落库历史 / 读取失败）—— 缓存 `null` 与「没取过」
 * 用 `has` 区分，避免每次切回都重发一次注定失败的请求。
 */
const gatewayTrendCache = new Map<RegionFilter, GatewayTokenTrend | null>();

/** 视图为空时的统一空态（四要素：icon + title + description + 出口）。 */
function ViewEmpty({
  icon,
  title,
  description,
  actionLabel,
  onAction,
}: {
  icon: typeof MessagesSquare;
  title: string;
  description: string;
  actionLabel: string;
  onAction: () => void;
}) {
  return (
    <EmptyState
      icon={icon}
      title={title}
      description={description}
      action={
        <Button size="sm" variant="outline" onClick={onAction}>
          {actionLabel}
        </Button>
      }
    />
  );
}

/* ────────────────────────────── 趋势 ────────────────────────────── */

function TrendView({
  source,
  viewWindow,
  onViewWindowChange,
  gatewayTrend,
}: {
  source: TokenStatsSource;
  viewWindow: UsageWindow;
  onViewWindowChange: (next: UsageWindow) => void;
  gatewayTrend: GatewayTokenTrend | null;
}) {
  const t = useT();
  const windowLabel = useUsageWindowLabel();
  const columns = useMemo(() => dailyModelSeries(source, viewWindow), [source, viewWindow]);
  const matrix = useMemo(() => heatmapMatrix(source.hours), [source]);
  const slices = useMemo(() => modelSlices(source, viewWindow), [source, viewWindow]);
  /**
   * 网关逐日图（**独立小图**，刻意不并进上面的主堆叠柱）：
   *
   * ① 主柱的「末柱 = 概览统计块 = 账号卡今日合计」是跨视图一致性契约，并进去会静默改口径；
   * ② 客户端与网关是**两条独立记录**（配置重叠时同一请求两处各记一次），求和会把重复
   *    计数藏起来 —— 分开画 + 图注披露覆盖率，比一个「加起来正好」的数字诚实。
   */
  const gatewayColumns = useMemo(
    () => gatewayDailyColumns(gatewayTrend, viewWindow),
    [gatewayTrend, viewWindow],
  );
  const gatewayCoverageInfo = useMemo(
    () => gatewayCoverage(gatewayTrend, viewWindow),
    [gatewayTrend, viewWindow],
  );
  /** 窗口内**一天有网关用量**才出图（全 0 / 全未记录 ⇒ 没有可画的值）。 */
  const gatewayHasBars = gatewayColumns.some((column) => column.segments.length > 0);
  const heatmapHasData = useMemo(
    () => matrix.cells.some((row) => row.some((value) => value > 0)),
    [matrix],
  );

  const rangeRecords = useMemo(
    () => {
      // ★ A1：窗口按**日历日**取（与 `dailyModelSeries` 同源），不再 `slice(-range)`
      //   —— 稀疏数组下那会取到窗口外的历史天。
      const inWindow = new Set(calendarWindowKeys(viewWindow));
      return source.daily.reduce(
        (sum, point) => (inWindow.has(point.key) ? sum + point.records : sum),
        0,
      );
    },
    [source.daily, viewWindow],
  );
  /** 「被视图参数筛没」：全时段有记录，但当前范围窗口内为 0。 */
  const filtered = source.summary.records > 0 && rangeRecords === 0;

  const total = columns.reduce((sum, column) => sum + column.total, 0);
  /* ★ A1-3：日均的分母是**窗口天数**，不是「有数据的天数」。
     旧实现除以 `columns.length`（补 0 前 = 有数据的天数）⇒ 实测 1500 Token / 5 天
     显示「日均 300」，而真实 30 天日均应为 50 —— 与 `wbStats.zh.ts` 里
     「按自然日平均」的说明**互相矛盾**。
     ★ 换成「今天」时窗口就是 1 天 ⇒ 日均 == 当日总量，这是**对的**：
     一天的日均就是它自己。 */
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

  /**
   * 窗口内网关用量（与下面那张网关小图**同源同窗**，两处必须对齐）。
   *
   * ⚠️ **必须在 `filtered` 早退之前**（issue #45）：hook 放在条件 `return` 之后，
   * 「今天无请求 ⇒ filtered=true」时这一轮少调一个 hook、恢复时又多调一个 ⇒
   * React #300（rendered more hooks than during the previous render），整个
   * Token 统计页被错误边界接住。**任何新增 hook 都放这个位置以上。**
   */
  const gatewayTotal = useMemo(
    () => gatewayColumns.reduce((sum, column) => sum + column.total, 0),
    [gatewayColumns],
  );

  if (filtered) {
    return (
      <ViewEmpty
        icon={MessagesSquare}
        title={t("wbStats.usage.empty.filtered")}
        description={t("wbStats.usage.empty.filteredHint")}
        actionLabel={t("wbStats.usage.empty.viewAll")}
        onAction={() => onViewWindowChange({ kind: "preset", days: 30 })}
      />
    );
  }

  const tiles: StatTile[] = [
    { icon: MessagesSquare, label: t("wbStats.usage.trend.total"), value: compactTokens(total), hint: windowLabel(viewWindow), title: t("wbStats.usage.trend.totalTitle") },
    { icon: TrendingUp, label: t("wbStats.usage.trend.avg"), value: compactTokens(average), hint: t("wbStats.usage.trend.avgHint") },
    { icon: Gauge, label: t("wbStats.usage.trend.peak"), value: compactTokens(peak.total), hint: peak.label || undefined },
    /* 网关量单独成 tile 且**不与上面求和**：客户端与网关是两条独立记录（配置重叠时
       同一请求两处各记一次），求和的数字好看但藏起重复计数。 */
    { icon: Server, label: t("wbStats.usage.trend.gatewayTokens"), value: compactTokens(gatewayTotal), hint: t("wbStats.usage.trend.gatewayTokensHint"), title: t("wbStats.usage.trend.gatewayTokensTitle") },
    { icon: Coins, label: t("wbStats.usage.trend.models"), value: exactTokens(slices.length) },
  ];

  return (
    <div className="min-w-0 space-y-6">
      <StatTiles tiles={tiles} ariaLabel={t("wbStats.usage.view.trend")} />
      {columns.length > 0 ? (
        <ChartFrame
          title={t("wbStats.usage.trend.stackedTitle")}
          caption={t("wbStats.usage.trend.stackedDesc")}
          ariaLabel={t("wbStats.usage.trend.stackedAria")}
        >
          <StackedDailyBar columns={columns} />
        </ChartFrame>
      ) : null}
      {gatewayHasBars ? (
        <ChartFrame
          title={t("wbStats.usage.trend.gatewayTitle")}
          caption={gatewayTrendCaption(
            t,
            gatewayCoverageInfo.recorded,
            gatewayCoverageInfo.total,
            gatewayCoverageInfo.truncated,
          )}
          ariaLabel={t("wbStats.usage.trend.gatewayAria")}
        >
          <StackedDailyBar columns={gatewayColumns} />
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
      {heatmapHasData ? (
        <ChartFrame
          title={t("wbStats.usage.trend.heatmapTitle")}
          caption={t("wbStats.usage.trend.heatmapDesc")}
          ariaLabel={t("wbStats.usage.trend.heatmapAria")}
          exportable={false}
        >
          <UsageHeatmap matrix={matrix} />
        </ChartFrame>
      ) : null}
    </div>
  );
}

/* ────────────────────────────── 模型 ────────────────────────────── */

function ModelsView({
  source,
  viewWindow,
  onRefresh,
}: {
  source: TokenStatsSource;
  viewWindow: UsageWindow;
  onRefresh: () => void | Promise<void>;
}) {
  const t = useT();
  const { busy, run } = useRefreshAction();
  const slices = useMemo(() => modelSlices(source, viewWindow), [source, viewWindow]);
  const total = slices.reduce((sum, slice) => sum + slice.value, 0);
  const top = slices[0];

  if (slices.length === 0) {
    /* 区块级空态（§1.6）——**四要素含出口**：缺出口等于把用户丢在原地。 */
    return (
      <EmptyState
        icon={Server}
        title={t("wbStats.usage.empty.models")}
        description={t("wbStats.usage.empty.modelsHint")}
        action={
          <Button
            size="sm"
            variant="outline"
            disabled={busy}
            onClick={() => void run(() => onRefresh())}
          >
            {busy ? <Loader2 className="animate-spin" /> : <RefreshCw />}
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
    <div className="min-w-0 space-y-6">
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
    </div>
  );
}

/* ────────────────────────────── 会话 ────────────────────────────── */

function SessionsView({
  source,
  viewWindow,
  onViewWindowChange,
  onRefresh,
}: {
  source: TokenStatsSource;
  viewWindow: UsageWindow;
  onViewWindowChange: (next: UsageWindow) => void;
  onRefresh: () => void | Promise<void>;
}) {
  const t = useT();
  const windowLabel = useUsageWindowLabel();
  const { busy, run } = useRefreshAction();
  /**
   * **统计口径**：窗口内的全量点 —— tile 的「会话数 / 总 Token / 均值 / 最高消耗」按它算。
   *
   * ★ 2026-10-15：改为**窗口口径**（`sessionPoints(source, viewWindow)`）——
   * 用户报障「切换时间没有变化」之前，这里吃的是全时数据，切窗口时四个 tile 与两张图
   * 都不动。素材是后端下发的 `sessions[].daily`（逐日拆分），与趋势 / 模型视图同构。
   */
  const points = useMemo(() => sessionPoints(source, viewWindow), [source, viewWindow]);
  /**
   * **渲染口径**：只画前 `SESSION_RENDER_LIMIT` 个。
   *
   * ⚠️ 这是**有意偏离**原型契约 8（「排行条数 = 气泡数 = 会话数」）：会话数可达 1.5 万
   * （一个 jsonl 一条），全量渲染 = 2×N 个 DOM 节点 + N 个散点 ⇒ 切页要等数秒。
   * 取舍与「为什么不虚拟滚动」见 `SESSION_RENDER_LIMIT` 的注释。
   *
   * 两个图**共用同一个切片**，所以契约 8 里「排行与散点同源、同一批会话」仍成立。
   */
  const visible = useMemo(() => points.slice(0, SESSION_RENDER_LIMIT), [points]);
  const total = points.reduce((sum, point) => sum + point.value, 0);
  /** 截断说明。**只在真的截断时出现** —— 没截断还写「共 N 个」是句废话。 */
  const capNote =
    points.length > visible.length
      ? t("wbStats.usage.sessions.shownOf", { shown: visible.length, total: points.length })
      : null;

  if (points.length === 0) {
    /* 两种空态必须分开（同趋势视图的「筛没 / 真没有」判据）：
       - 全时有会话、当前窗口为 0 ⇒ 「筛没」：给「看近 30 天」这个**出口**；
       - 全时也没有 ⇒ 「还没有产生会话」，只留刷新。
       把两者合成一个「暂无会话记录」会**谎报**——用户明明有会话，只是不在这个窗口里。 */
    const hasAnySession = source.sessions.some(
      (session) => session.total > 0 || session.records > 0,
    );
    if (hasAnySession) {
      return (
        <ViewEmpty
          icon={Users}
          title={t("wbStats.usage.empty.filtered")}
          description={t("wbStats.usage.empty.filteredHint")}
          actionLabel={t("wbStats.usage.empty.viewAll")}
          onAction={() => onViewWindowChange({ kind: "preset", days: 30 })}
        />
      );
    }
    /* 区块级空态（§1.6）——**四要素含出口**。 */
    return (
      <EmptyState
        icon={Users}
        title={t("wbStats.usage.empty.sessions")}
        description={t("wbStats.usage.empty.sessionsHint")}
        action={
          <Button
            size="sm"
            variant="outline"
            disabled={busy}
            onClick={() => void run(() => onRefresh())}
          >
            {busy ? <Loader2 className="animate-spin" /> : <RefreshCw />}
            {t("wbStats.usage.empty.refresh")}
          </Button>
        }
      />
    );
  }

  const tiles: StatTile[] = [
    /* 「会话数」的 hint 用**当前窗口**标签（原型同款语义：区间内活跃）——
       没有它，用户看不出这个数字随参数条变化。 */
    { icon: Users, label: t("wbStats.usage.sessions.count"), value: exactTokens(points.length), hint: windowLabel(viewWindow) },
    { icon: MessagesSquare, label: t("wbStats.usage.sessions.total"), value: compactTokens(total), hint: windowLabel(viewWindow) },
    { icon: Gauge, label: t("wbStats.usage.sessions.avg"), value: compactTokens(Math.round(total / points.length)) },
    { icon: TrendingUp, label: t("wbStats.usage.sessions.max"), value: compactTokens(points[0].value), hint: points[0].title },
  ];

  return (
    <div className="min-w-0 space-y-6">
      <StatTiles tiles={tiles} ariaLabel={t("wbStats.usage.view.sessions")} />
      <div className="grid min-w-0 gap-6 lg:grid-cols-2">
        <ChartFrame
          title={t("wbStats.usage.sessions.rankTitle")}
          caption={capNote ?? `${exactTokens(points.length)} · ${compactTokens(total)}`}
          ariaLabel={t("wbStats.usage.sessions.rankTitle")}
          exportable={false}
        >
          <SessionRanking points={visible} />
        </ChartFrame>
        <ChartFrame
          title={t("wbStats.usage.sessions.scatterTitle")}
          /* 轴语义（记录数）**必须留**（「图不能撒谎」红线，门禁断言）；截断说明接在后面。 */
          caption={`${scatterAxisCaption(t)} · ${t("wbStats.usage.sessions.scatterDesc")}${capNote ? ` · ${capNote}` : ""}`}
          ariaLabel={t("wbStats.usage.sessions.scatterAria")}
        >
          <SessionScatter points={visible} />
        </ChartFrame>
      </div>
    </div>
  );
}

/* ────────────────────────────── 页面 ────────────────────────────── */

export default function TokenStatsPage({
  view = "trend",
  viewWindow,
  combineAll,
  onViewWindowChange,
}: { view?: TokenUsageView } & UsageViewProps) {
  const t = useT();
  /* P1：`cn` / `global` 统一走 `useScope`（URL 优先 + localStorage 记忆）。
     ⚠️ 只取值、不取 setter —— 切区域的动作在**侧栏**（`ScopeSwitch`），页内不再有区域选择器。 */
  const [scopeRegion] = useScope("workbuddy");
  const region: RegionFilter = combineAll ? "all" : scopeRegion;
  const [stats, setStats] = useState<TokenStatistics | null>(() => statisticsCache[region] ?? null);
  const [loading, setLoading] = useState(() => !statisticsCache[region]);
  const [error, setError] = useState<string | null>(null);
  const [reload, setReload] = useState(0);
  /* 三处「重试 / 刷新」出口（错误 Alert、两种空态）共用同一个 reload 触发 ⇒ 共用一个实例。 */
  const { busy: reloadBusy, run: runReload } = useRefreshAction();
  const [gatewayTrend, setGatewayTrend] = useState<GatewayTokenTrend | null>(
    () => gatewayTrendCache.get(region) ?? null,
  );

  // region 是页面级数据源：切换即重新取数；旧结果保留到新结果返回，避免闪白。
  useEffect(() => {
    let disposed = false;
    setLoading(true);
    setError(null);
    api
      .getTokenStatistics(undefined, region)
      .then((result) => {
        if (disposed) return;
        statisticsCache[region] = result;
        setStats(result);
      })
      .catch((cause) => {
        if (!disposed) setError(api.asError(cause));
      })
      .finally(() => {
        if (!disposed) setLoading(false);
      });
    /* 网关逐日趋势与统计**并行**取（互不依赖）：单 region 命令，`all` 时前端分区取再合并
       （与 `mergeSources` 同哲学）。单区失败只让该区缺席（`.catch(() => null)`）——
       整条失败 ⇒  trend 为 null ⇒ 网关小图不渲染（渐进增强，不弹错噪屏）。
       ⚠️ 老服务端没有这条路由时会 404：同样按「不出图」处理，不影响主统计。 */
    const gatewayRegions: Region[] = region === "all" ? ["cn", "global"] : [region];
    Promise.all(
      gatewayRegions.map((gatewayRegion) =>
        api.getGatewayTokenTrend(gatewayRegion).catch(() => null),
      ),
    )
      .then((results) => {
        if (disposed) return;
        const merged = mergeGatewayTrends(results);
        gatewayTrendCache.set(region, merged);
        setGatewayTrend(merged);
      })
      .catch(() => {
        if (!disposed) setGatewayTrend(null);
      });
    return () => {
      disposed = true;
    };
  }, [region, reload]);

  const source = useMemo(() => (stats ? mergeSources(stats.sources) : null), [stats]);
  const hasSource = (stats?.sources.length ?? 0) > 0;
  /* 网关用量（`gatewayTrend`，扫 `gateway_logs`）是客户端（扫 `projects/*.jsonl`）
     之外的另一条记录源，按窗口取。 */
  const gatewayInWindow = useMemo(() => {
    const keys = new Set(calendarWindowKeys(viewWindow));
    return (gatewayTrend?.days ?? []).reduce(
      (sum, day) => (keys.has(day.key) ? sum + day.total : sum),
      0,
    );
  }, [gatewayTrend, viewWindow]);
  /**
   * 有来源但**两个数据源都翻不出用量** ⇒ 整页空态（区分于「被视图参数筛没」，后者在视图内处理）。
   *
   * ⚠️ 两侧口径**故意不对称**，勿强行统一：
   * - 网关侧按窗口判：`gatewayTrend` 含 90 天落库历史，不筛窗会把窗外历史算成
   *   「有数据」，而当前窗口其实为空。
   * - 客户端侧按全量判：`getTokenStatistics` 未按窗口传参（`days` 缺省 ⇒ 后端
   *   不加 cutoff），窗口内被筛没的由各视图自行展示 ⇒ 这里只需「有没有任何来源
   *   记录」来决定是否渲染页面，避免空态把可能有值的视图整片吞掉。 */
  const clientHasRecords = (source?.summary.records ?? 0) > 0;
  const trulyEmpty = Boolean(source) && !clientHasRecords && gatewayInWindow === 0;

  return (
    /* ⚠️ 页头 / 子 Tab / **视图参数条**都由**上层** `UsagePage` 统一渲染
       （原型 `viewUsage` 只有一个「用量」页头、一条参数条）⇒ 本页只渲染视图内容。
       `view` 决定画趋势 / 模型 / 会话哪一个。 */
    <div className="min-w-0">
      {error && (
        <Alert variant="destructive" className="mb-5">
          <CircleAlert />
          <AlertTitle>{t("wbStats.token.loadFailed")}</AlertTitle>
          <AlertDescription className="flex flex-wrap items-center gap-3">
            <span>{error}</span>
            <Button
              size="sm"
              variant="outline"
              disabled={reloadBusy}
              onClick={() => void runReload(() => setReload((value) => value + 1))}
            >
              {reloadBusy ? <Loader2 className="animate-spin" /> : <RefreshCw />}
              {t("wbStats.token.retry")}
            </Button>
          </AlertDescription>
        </Alert>
      )}

      {loading && !stats ? (
        <div className="space-y-4" aria-busy="true">
          <Skeleton className="h-24 w-full" />
          <Skeleton className="h-64 w-full" />
          <Skeleton className="h-40 w-full" />
        </div>
      ) : !hasSource || !source ? (
        <EmptyState
          icon={CircleAlert}
          title={t("wbStats.usage.empty.noSource")}
          description={t("wbStats.usage.empty.noSourceHint")}
          action={
            <Button
              size="sm"
              variant="outline"
              disabled={reloadBusy}
              onClick={() => void runReload(() => setReload((value) => value + 1))}
            >
              {reloadBusy ? <Loader2 className="animate-spin" /> : <RefreshCw />}
              {t("wbStats.usage.empty.refresh")}
            </Button>
          }
        />
      ) : trulyEmpty ? (
        <EmptyState
          icon={MessagesSquare}
          title={t("wbStats.usage.empty.noData")}
          description={t("wbStats.usage.empty.noDataHint")}
          action={
            <Button
              size="sm"
              variant="outline"
              disabled={reloadBusy}
              onClick={() => void runReload(() => setReload((value) => value + 1))}
            >
              {reloadBusy ? <Loader2 className="animate-spin" /> : <RefreshCw />}
              {t("wbStats.usage.empty.refresh")}
            </Button>
          }
        />
      ) : view === "models" ? (
        <ModelsView source={source} viewWindow={viewWindow} onRefresh={() => setReload((value) => value + 1)} />
      ) : view === "sessions" ? (
        <SessionsView
          source={source}
          viewWindow={viewWindow}
          onViewWindowChange={onViewWindowChange}
          onRefresh={() => setReload((value) => value + 1)}
        />
      ) : (
        <TrendView
          source={source}
          viewWindow={viewWindow}
          onViewWindowChange={onViewWindowChange}
          gatewayTrend={gatewayTrend}
        />
      )}
    </div>
  );
}
