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
 * ## 三个视图（趋势 / 模型 / 会话）共用一份数据，只是切法不同
 *
 * 数据全部来自同一个 `qoder_usage`（逐日 + byModel + sessions + source）：
 * - **趋势**：逐日 credits 柱 + 四个统计卡；
 * - **模型**：每日模型构成（堆叠柱）+ 模型排行（占比）；
 * - **会话**：按 credits 排序的会话列表（标题取首个 user 消息，拿不到就回落短 id）。
 *
 * ★ Qoder **没有会话复制能力**，所以会话视图是**只读排行**，不提供「打开 / 复制」出口 ——
 *   摆一个点不动的按钮比不摆更糟。
 *
 * ⚠️ 与 WB / Trae 的差异只在**单位与数据源**：这里是 credits（客户端按 credits 计费），
 *   且会话没有「账号」归属（客户端记录里没有 uid）。
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
import { KeysView } from "@/components/usage/key-ranking";
import { normalizeQoderGatewayLogs, type QoderApiKeyRecord } from "@/lib/qoder-gateway";
import { ChartFrame } from "@/components/usage/chart-frame";
import { StatTiles, type StatTile } from "@/components/usage/stat-tiles";
import { seriesColor } from "@/components/usage/palette";
import type { UsageViewProps } from "@/components/usage/view-param-bar";
import * as api from "@/lib/api";
import { creditTrendCaption } from "@/lib/chart-caption";
import { formatCredits } from "@/components/usage/format";
import { useT } from "@/lib/i18n";
import { platformOfPath } from "@/lib/platforms/registry";
import { scopeOf } from "@/lib/scope";
import { useLocation } from "react-router-dom";
import { useScope } from "@/lib/use-scope";
import { useRefreshAction } from "@/lib/use-refresh-action";
import { usageWindowKeys, usageWindowSpanDays } from "@/lib/usage-window";

/**
 * 模型堆叠柱最多几个独立系列（其余并进「其他」）。
 *
 * 判据是**色板**：`palette` 只有 8 色，超过就会重复取色、图例失去意义。
 */
const MODEL_SERIES_MAX = 6;

/** 「其他」系列的 dataKey（双下划线前缀，避免与真实模型名撞）。 */
const OTHER_SERIES = "__other__";

/** 会话列表最多显示多少行（与后端 `SESSION_ROWS_LIMIT` 同值；仅用于文案）。 */
const SESSION_ROWS = 200;

/** 三个子视图（`UsagePage` 的 `PLATFORM_TABS.qoder` 与之对应）。 */
export type QoderUsageView = "trend" | "models" | "sessions" | "keys";

/**
 * 「按密钥」取数（Qoder：网关日志 + 密钥名）。模块级 ⇒ 引用稳定。
 *
 * ⚠️ Qoder 的**用量账本**按 credits 计（客户端会话），但「按密钥」看的是**网关请求日志**
 * 的 `keyId` —— 与 WB / Trae 同一份 `KeysView`，只是数据源不同。
 */
async function loadQoderKeyUsage() {
  const [raw, keys] = await Promise.all([
    api.getQoderGatewayLogs(),
    api.listQoderApiKeys().catch(() => ({ keys: [] as QoderApiKeyRecord[] })),
  ]);
  /* `list_qoder_api_keys` 的返回类型是 `unknown`（命令面参数化后未收窄）⇒ 就地断言。
     取不到名字时 `aggregateByKey` 会用短 id 兜底，不会显示「未知」。 */
  const rows = ((keys as { keys?: QoderApiKeyRecord[] }).keys ?? []) as QoderApiKeyRecord[];
  return {
    entries: normalizeQoderGatewayLogs(raw),
    keyNames: new Map(rows.map((key) => [key.id, key.name] as const)),
  };
}

interface Props extends UsageViewProps {
  /** 当前子视图；缺省 `trend`（老调用点不传也照常工作）。 */
  view?: QoderUsageView;
}

export default function QoderUsagePage({ viewWindow, view = "trend" }: Props) {
  const t = useT();
  const location = useLocation();
  const product = platformOfPath(location.pathname);
  const [region] = useScope(product);
  const scope = scopeOf(product, region);

  /**
   * 取数跨度：后端 `days` 的语义是「**含今天**的回看窗口」（`usage::usage_view_for`），
   * 而 `custom` 的 `to` 可以落在过去 ⇒ 单靠 `days` 会多取一段。
   * ⚠️ 前端因此**再按窗口键切一刀**（下面 `windowDays`），否则图与统计会**多说一段数据**
   *   —— 参数条回执写着 A~B，图上却是 A~今天。
   */
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

  /** 窗口键集合（`today` / 档位 / 自定义三态统一口径）。 */
  const windowKeys = useMemo(() => new Set(usageWindowKeys(viewWindow)), [viewWindow]);
  /** 真正落在窗口内的逐日数据（★ 与 `data.days` 同源，切的是**窗口**不是**数据**）。 */
  const windowDays = useMemo(
    () => (data?.days ?? []).filter((day) => windowKeys.has(day.date)),
    [data, windowKeys],
  );
  /** 窗口内按模型汇总（逐日 `byModel` 跨天相加；与后端 `totals.credits` 同源同窗）。 */
  const windowByModel = useMemo(() => {
    const merged = new Map<string, number>();
    for (const day of windowDays) {
      for (const row of day.byModel ?? []) {
        merged.set(row.model, (merged.get(row.model) ?? 0) + row.credits);
      }
    }
    return [...merged.entries()]
      .map(([model, credits]) => ({ model, credits }))
      .sort((left, right) => right.credits - left.credits);
  }, [windowDays]);
  /**
   * 窗口内合计。⚠️ `sessions` **刻意不给数**：
   * 后端的 `sessions` 是「窗口内**去重**的会话 id 个数」，而逐日数据里没有会话列表 ——
   * 自定义区间下前端**算不出**这个去重值，与其猜一个（按天相加会把跨天会话重复计数）
   * 不如显示 `—`（同「概览」页未知即 `—` 的纪律）。
   */
  const windowTotals = useMemo(
    () => ({
      credits: windowDays.reduce((sum, day) => sum + day.credits, 0),
      requests: windowDays.reduce((sum, day) => sum + day.requests, 0),
      models: windowByModel.length,
      /** 后端窗口与前端窗口**完全一致**时（today / 档位）才可直接引用去重会话数。 */
      sessionsExact: !data || windowDays.length === data.days.length,
      sessions: data?.totals.sessions ?? 0,
    }),
    [windowDays, windowByModel, data],
  );

  /** 窗口内的会话行（与 `windowDays` 同一套窗口口径：**区间相交**即算在窗内）。 */
  const windowSessions = useMemo(() => {
    const keys = [...windowKeys].sort();
    if (keys.length === 0) return [];
    const min = keys[0];
    const max = keys[keys.length - 1];
    return (data?.sessions ?? []).filter(
      (row) => row.lastDate >= min && row.firstDate <= max,
    );
  }, [data, windowKeys]);

  /** 窗口内的 credits 合计（会话 / 模型两个视图共用，与 trend 同一个口径）。 */
  const totalCredits = windowTotals.credits;

  /**
   * 三个视图各自的统计卡。
   *
   * ⚠️ 不在 trend 里塞「模型 / 会话」的数字：统计卡是**该视图的摘要**，
   * 混着放会让「切到模型视图后数字没变」看起来像没刷新。
   */
  const tiles = useMemo<StatTile[]>(() => {
    if (!data) return [];
    const creditsTile: StatTile = {
      icon: Coins,
      label: t("qoder.page.usage.totals.credits"),
      value: formatCredits(totalCredits),
      hint: t("qoder.page.usage.unit.credits"),
    };
    if (view === "models") {
      const top = windowByModel[0];
      const share = top && totalCredits > 0 ? Math.round((top.credits / totalCredits) * 100) : 0;
      return [
        {
          icon: Layers,
          label: t("qoder.page.usage.models.count"),
          value: formatCredits(windowByModel.length),
        },
        creditsTile,
        {
          icon: Coins,
          label: t("qoder.page.usage.models.top"),
          value: top ? top.model : "—",
          hint: top ? t("qoder.page.usage.creditsValue", { value: formatCredits(top.credits) }) : undefined,
        },
        {
          icon: Hash,
          label: t("qoder.page.usage.models.topShare"),
          /* 合计为 0 时算不出占比 ⇒ `—`（0/0 给一个 0% 是把未知说成已知）。 */
          value: totalCredits > 0 ? `${share}%` : "—",
        },
      ];
    }
    if (view === "sessions") {
      const top = windowSessions[0];
      const avg = windowSessions.length > 0 ? totalCredits / windowSessions.length : 0;
      return [
        {
          icon: MessagesSquare,
          label: t("qoder.page.usage.sessions.count"),
          value: formatCredits(windowSessions.length),
        },
        creditsTile,
        {
          icon: Coins,
          label: t("qoder.page.usage.sessions.top"),
          value: top ? sessionLabel(top) : "—",
          hint: top ? t("qoder.page.usage.creditsValue", { value: formatCredits(top.credits) }) : undefined,
        },
        {
          icon: Hash,
          label: t("qoder.page.usage.sessions.avg"),
          value: windowSessions.length > 0 ? formatCredits(avg) : "—",
        },
      ];
    }
    return [
      creditsTile,
      {
        icon: Hash,
        label: t("qoder.page.usage.totals.requests"),
        value: formatCredits(windowTotals.requests),
      },
      {
        icon: MessagesSquare,
        label: t("qoder.page.usage.totals.sessions"),
        /* 自定义区间下算不出「去重会话数」⇒ 显示 `—`，不拿「按天相加」冒充。 */
        value: windowTotals.sessionsExact ? formatCredits(windowTotals.sessions) : "—",
        hint: windowTotals.sessionsExact
          ? undefined
          : t("qoder.page.usage.totals.sessionsCustomHint"),
      },
      {
        icon: Layers,
        label: t("qoder.page.usage.totals.models"),
        value: formatCredits(windowTotals.models),
      },
    ];
  }, [data, view, totalCredits, windowTotals, windowByModel, windowSessions, t]);

  /* 逐日柱的数据：`date` 截成 `MM-DD` 当横轴标签（完整 ISO 太长，会把轴挤满）。 */
  const bars = useMemo(
    () =>
      windowDays.map((day) => ({
        label: day.date.slice(5),
        credits: day.credits,
      })),
    [windowDays],
  );
  const peak = useMemo(
    () => bars.reduce((max, bar) => Math.max(max, bar.credits), 0),
    [bars],
  );

  /**
   * 模型视图的堆叠柱：**前 6 个模型**各自一色，其余并进「其他」。
   *
   * 为什么截到 6：颜色只有 8 个（`palette`），再多就会开始重复色，图例失去意义。
   * 6 + 其他 = 最多 7 个系列，仍在色板内。
   */
  const modelStack = useMemo(() => {
    const top = windowByModel.slice(0, MODEL_SERIES_MAX);
    const topNames = new Set(top.map((row) => row.model));
    const series = [
      ...top.map((row) => row.model),
      ...(windowByModel.length > MODEL_SERIES_MAX ? [OTHER_SERIES] : []),
    ];
    const rows = windowDays.map((day) => {
      const bucket: Record<string, number | string> = { label: day.date.slice(5) };
      let other = 0;
      for (const row of day.byModel ?? []) {
        if (topNames.has(row.model)) {
          bucket[row.model] = (bucket[row.model] as number | undefined ?? 0) + row.credits;
        } else {
          other += row.credits;
        }
      }
      if (other > 0) bucket[OTHER_SERIES] = other;
      return bucket;
    });
    return { series, rows, top };
  }, [windowDays, windowByModel]);

  /* ── 会话视图的标签 / 日期区间 ── */
  function sessionLabel(row: api.QoderUsageSession): string {
    return row.title || t("qoder.page.usage.sessions.untitled", { id: row.id.slice(0, 8) });
  }
  function sessionRange(row: api.QoderUsageSession): string {
    return row.firstDate === row.lastDate
      ? row.firstDate
      : t("qoder.page.usage.sessions.range", { from: row.firstDate, to: row.lastDate });
  }

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
     「有目录但这段时间没有记录」（去换个窗口）是两种不同的用户动作。
     ⚠️ 两处都**必须带出口**（§1.6 四要素：icon + title + description + action）——
     没有出口等于把用户丢在原地，2026-10-18 接回入口时补上。 */
  if (!data.source.exists) {
    return (
      <Card className="gap-0 py-0">
        <EmptyState
          icon={MessagesSquare}
          title={t("qoder.page.usage.missing.title")}
          description={t("qoder.page.usage.missing.description", {
            path: data.source.paths[0] ?? "~/.qoder-cn/projects",
          })}
          action={
            <Button size="sm" variant="outline" disabled={busy} onClick={() => void run(() => load())}>
              {busy ? <RefreshCw className="animate-spin" /> : <RefreshCw />}
              {t("qoder.page.usage.recheck")}
            </Button>
          }
        />
      </Card>
    );
  }

  if (windowDays.length === 0) {
    return (
      <Card className="gap-0 py-0">
        <EmptyState
          icon={MessagesSquare}
          title={t("qoder.page.usage.empty.title")}
          description={t("qoder.page.usage.empty.description")}
          action={
            <Button size="sm" variant="outline" disabled={busy} onClick={() => void run(() => load())}>
              {busy ? <RefreshCw className="animate-spin" /> : <RefreshCw />}
              {t("qoder.page.usage.recheck")}
            </Button>
          }
        />
      </Card>
    );
  }

  /* 模型视图 / 会话视图各自的「窗内有数据但该维度为空」：如实说，不退化成空白。 */
  const modelsEmpty = view === "models" && windowByModel.length === 0;
  const sessionsEmpty = view === "sessions" && windowSessions.length === 0;

  return (
    <div className="flex min-w-0 flex-col gap-4">
      <StatTiles tiles={tiles} />

      {modelsEmpty || sessionsEmpty ? (
        <Card className="gap-0 py-0">
          <EmptyState
            icon={Layers}
            title={
              modelsEmpty
                ? t("qoder.page.usage.models.empty")
                : t("qoder.page.usage.sessions.empty")
            }
            description={t("qoder.page.usage.empty.description")}
          />
        </Card>
      ) : view === "models" ? (
        <>
          <ChartFrame
            title={t("qoder.page.usage.models.stacked")}
            ariaLabel={t("qoder.page.usage.models.stacked")}
            exportable={false}
          >
            <BarChart data={modelStack.rows} margin={{ top: 8, right: 8, bottom: 0, left: 0 }}>
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
              {modelStack.series.map((name, index) => (
                <Bar
                  key={name}
                  dataKey={name}
                  stackId="models"
                  fill={seriesColor(index)}
                  name={name === OTHER_SERIES ? t("qoder.page.usage.models.other") : name}
                  radius={index === modelStack.series.length - 1 ? [3, 3, 0, 0] : undefined}
                />
              ))}
            </BarChart>
          </ChartFrame>

          {/* 排行：**自有列表**（不复用 `ModelRanking` —— 它的单位写死了 Token，见文件头）。 */}
          <ChartFrame
            title={t("qoder.page.usage.models.ranking")}
            ariaLabel={t("qoder.page.usage.models.ranking")}
            exportable={false}
          >
            <div className="space-y-2.5">
              {windowByModel.map((row, index) => {
                const ratio = totalCredits > 0 ? (row.credits / totalCredits) * 100 : 0;
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
        </>
      ) : view === "keys" ? (
        <KeysView
          window={viewWindow}
          loader={loadQoderKeyUsage}
          title={t("wbStats.usage.keys.title")}
          caption={t("wbStats.usage.keys.caption")}
          emptyTitle={t("wbStats.usage.keys.empty")}
          emptyHint={t("wbStats.usage.keys.emptyHint")}
        />
      ) : view === "sessions" ? (
        <ChartFrame
          title={t("qoder.page.usage.sessions.list")}
          caption={t("qoder.page.usage.sessions.capped", { n: String(SESSION_ROWS) })}
          ariaLabel={t("qoder.page.usage.sessions.list")}
          exportable={false}
        >
          <div className="divide-y divide-border/60">
            {windowSessions.map((row) => (
              <div
                key={row.id}
                className="flex items-start justify-between gap-3 py-2 first:pt-0 last:pb-0"
              >
                <div className="min-w-0">
                  <div className="truncate text-[12.5px] font-medium" title={sessionLabel(row)}>
                    {sessionLabel(row)}
                  </div>
                  <div className="mt-0.5 flex flex-wrap items-center gap-x-2 gap-y-1 text-[11px] text-muted-foreground">
                    <span className="tabular-nums">{sessionRange(row)}</span>
                    <span>·</span>
                    <span>
                      {t("qoder.page.usage.sessions.requests", { n: formatCredits(row.requests) })}
                    </span>
                    {row.models.slice(0, 2).map((model) => (
                      <span key={model} className="badge">
                        {model}
                      </span>
                    ))}
                  </div>
                </div>
                <span className="shrink-0 text-[12px] tabular-nums">
                  {t("qoder.page.usage.creditsValue", { value: formatCredits(row.credits) })}
                </span>
              </div>
            ))}
          </div>
        </ChartFrame>
      ) : (
        <ChartFrame
          title={t("qoder.page.usage.trend")}
          caption={creditTrendCaption(t, totalCredits, peak)}
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
      )}

      {/* ★ 来源说明**必须展示**：用户看到「数字比预期小」时，唯一能解释的就是这一条
          （它统计的是**本机客户端会话记录**，不含其它设备、也不含经网关的第三方调用）。 */}
      <Alert>
        <AlertDescription>
          {data.source.note}
          {view === "sessions" ? ` ${t("qoder.page.usage.sessions.note")}` : ""}
        </AlertDescription>
      </Alert>
    </div>
  );
}
