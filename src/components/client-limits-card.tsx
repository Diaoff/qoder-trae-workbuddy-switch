import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, useLocation } from "react-router-dom";
import { Gauge, Loader2, RefreshCw, Sparkles } from "lucide-react";
import { toast } from "sonner";

import { clientLimits } from "@/lib/api";
import { formatDate } from "@/lib/format";
import { useT } from "@/lib/i18n";
import {
  limitConflict,
  probeCellOf,
  probeOnlyLimits,
  probeTargets,
  probeTone,
  tallyCells,
} from "@/lib/limit-link";
import { scopeFromLegacy } from "@/lib/scope";
import type { TranslationKey } from "@/locales/zh";
import type { ClientLimitEntry, ClientLimitsResponse, ModelProbeCell, Region } from "@/lib/types";
import { cn } from "@/lib/utils";
import { useGatewayStore, useProbeCells } from "@/stores/gateway";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";

/** 扫描范围：`7` / `30` / `all`（`all` = 全量）。 */
type WindowKey = "7" | "30" | "all";

const WINDOW_DAYS: Record<WindowKey, number> = { "7": 7, "30": 30, all: 0 };

const WINDOW_LABELS: Record<WindowKey, TranslationKey> = {
  "7": "wbAccounts.limits.window.7",
  "30": "wbAccounts.limits.window.30",
  all: "wbAccounts.limits.window.all",
};

/**
 * 上游**排队类**错误码（客户端 `capacityQueueBlock.*` 队列块）。
 *
 * ⚠️ 口径遵循 PRD **R7**：`6020` / `6022` 的**精确语义文案待拍板前不得臆造**，
 * 界面**只**在原始 code 旁标注 R7 明确允许的保守词「排队中」。
 * 取证见 PRD「★ 6020/6022 取证」（候选：`6020≈…title.waiting` / `6022≈…title.userLimit`，**属推断**）。
 * code 原样带出，便于后续口径拍板后回填精确文案（PRD S6）。
 */
const QUEUE_CODES = new Set([6020, 6022]);

/**
 * 状态 → 徽章变体。**三态**，不是布尔：
 * `unknown`（恢复时间解析不出）必须与 `recovered`（确定已过恢复时刻）区分开 ——
 * 把「不知道」渲染成「已恢复」是凭空下结论。
 */
const STATE_VARIANTS = {
  active: "warning",
  unknown: "outline",
  recovered: "secondary",
} as const;

const STATE_LABELS: Record<ClientLimitEntry["state"], TranslationKey> = {
  active: "wbAccounts.limits.active",
  unknown: "wbAccounts.limits.unknown",
  recovered: "wbAccounts.limits.recovered",
};

/** 语义色调 → Badge 变体（`probeTone` 是框架无关的那一半）。 */
const TONE_VARIANTS = {
  ok: "success",
  warn: "warning",
  bad: "destructive",
  muted: "secondary",
} as const;

/**
 * 客户端限流台账卡片：**账号 × 模型 × 恢复时刻**（**只读**观测）。
 *
 * 数据源是客户端自身落盘的三张只读表 join（见 core `client_limits.rs`）——
 * **不依赖网关、不代理客户端流量、不要求客户端配合**。后端已把「仍受限 → 未知 → 已恢复」
 * 排好序，本组件只负责渲染（PRD S4）。
 *
 * ## ★ 与网关「限流探测」的联动（2026-10-22）
 *
 * 台账是**日志反推**，探测是**真发一次请求**的实判 —— 两者回答同一个问题的两种证据。
 * 此前各自成篇：这里看不到实测结论，网关卡也只单向取台账里 `active` 的格当被动底色。
 * 现在两侧按 `(uid, model)` 对齐：
 *
 * 1. **同一份数据**：实测结论读 store 的 `probeCells`（跨作业累积，见 `lib/limit-link.ts`），
 *    与网关卡矩阵**同一个来源**。任一侧发起探测，另一侧下次渲染就读到它。
 * 2. **入口**：卡头「实测 N 格」= 台账里仍受限/未知的账号 × 模型（后端按账号外层展开笛卡尔积，
 *    故 N 是**真实格数**）；行尾「实测」= 只打这一格。两处都 **`adopt: false`** ——
 *    账号页是「查证」，不是「治理」，**绝不悄悄改池冷却**。
 * 3. **冲突只并列**：台账徽章照旧，实测结论作为第二结论并列显示，并在不一致时说明
 *    （`limitConflict`）。谁对不归 UI 裁定。
 *
 * ⚠️ **只读**：本卡不写台账、不改账号状态（PRD R8）。唯一的写入口是显式点了「实测」。
 */
export function ClientLimitsCard({ region }: { region: Region }) {
  const t = useT();
  const location = useLocation();
  /** 作用域由 **region + 当前路由所属平台** 推出（与网关卡同一套转换，别自己拼 `-`）。 */
  const scope = useMemo(
    () => scopeFromLegacy(location.pathname, region),
    [location.pathname, region],
  );

  const [windowKey, setWindowKey] = useState<WindowKey>("7");
  const [data, setData] = useState<ClientLimitsResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);

  /* ── 探测数据（与网关卡同源） ── */
  const probe = useGatewayStore((s) => s.probe);
  const cells = useProbeCells(scope);
  const startProbe = useGatewayStore((s) => s.startProbe);
  const refreshProbeStatus = useGatewayStore((s) => s.refreshProbeStatus);
  const cancelProbe = useGatewayStore((s) => s.cancelProbe);
  /** 台账刷新后同步网关侧的「被动底色」——否则网关卡/模型表那一列会停在旧台账上。 */
  const loadProbePassive = useGatewayStore((s) => s.loadProbePassive);
  /** 拉**落盘台账**（跨重启的实测结论）：账号页是它的主要读者（网卡矩阵也读同一份）。 */
  const loadProbeLedger = useGatewayStore((s) => s.loadProbeLedger);
  /** 落盘台账的最近写入时刻（只用于汇总条文案）。 */
  const ledgerAt = useGatewayStore((s) => s.probeLedgerAt[scope]);

  // 竞态防护：切 region / 切窗口时旧请求可能后到，用序号丢弃过期响应。
  const requestSeq = useRef(0);

  const load = useCallback(async () => {
    const seq = ++requestSeq.current;
    setLoading(true);
    setError(null);
    try {
      const next = await clientLimits(region, WINDOW_DAYS[windowKey]);
      if (seq !== requestSeq.current) return;
      setData(next);
      /* 两侧共用一份台账口径：刷新完顺手让网关侧被动底色跟上（拉不到就静默，
         它是加分项，不该污染本卡错误条）。 */
      void loadProbePassive(region);
    } catch (cause) {
      if (seq !== requestSeq.current) return;
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      if (seq === requestSeq.current) setLoading(false);
    }
  }, [region, windowKey, loadProbePassive]);

  useEffect(() => {
    void load();
  }, [load]);

  /* 进入本卡就对齐一次实测快照：可能是网关页刚发起的探测（同一份数据）。
     store 在 `running` 时会自己续轮询（单实例），故这里只拉第一枪。 */
  useEffect(() => {
    void refreshProbeStatus(scope);
    // 同时把**落盘**的实测台账拉回来：重启后本卡仍要显示上次实测结论（同一份数据）。
    void loadProbeLedger(scope);
  }, [scope, refreshProbeStatus, loadProbeLedger]);

  const limits = useMemo(() => data?.limits ?? [], [data]);
  const running = probe?.phase === "running";
  const targets = useMemo(() => probeTargets(limits), [limits]);
  /** 实测查出限流、但台账没有记录的格（单列，不污染台账语义）。 */
  const probeOnly = useMemo(() => probeOnlyLimits(cells, limits), [cells, scope, limits]);
  /* 汇总按**合并索引**算（重启后作业是 idle，但台账仍有结论 —— 只按作业算会整段消失）。 */
  const tally = useMemo(() => tallyCells(cells), [cells]);

  /**
   * 起一轮实测：**恒 `adopt: false`**（账号页只查证）。
   *
   * ★ 传 `cells`（**显式格清单**）而不是 `uids` / `models`：后者是笛卡尔积，
   * 台账 3 条会被跑成 6 格。显式清单让「N 格」这个数字与实际请求数相等。
   */
  const runProbe = useCallback(
    async (pairs: { uid: string; model: string }[]) => {
      if (running || starting) return;
      if (pairs.length === 0) return;
      setStarting(true);
      try {
        await startProbe({ scope, cells: pairs, adopt: false });
      } catch (cause) {
        // 作业位单例：别的入口（含网关页）正在跑时后端会拒绝 —— 如实说，并顺手对齐一次快照。
        toast.error(t("wbAccounts.limits.probeStartFail"), { description: String(cause) });
        void refreshProbeStatus(scope);
      } finally {
        setStarting(false);
      }
    },
    [running, starting, startProbe, scope, refreshProbeStatus, t],
  );

  async function onProbeAll() {
    if (targets.cells === 0) return;
    await runProbe(targets.pairs);
  }

  async function onProbeRow(uid: string, model: string) {
    await runProbe([{ uid, model }]);
  }

  async function onCancel() {
    try {
      await cancelProbe(scope);
    } catch {
      /* 取消失败不打断：下一次 refreshProbeStatus 会对齐真实相位。 */
    }
  }

  return (
    <Card className="gap-0 py-0" data-act="client-limits-card">
      <div className="flex flex-wrap items-start justify-between gap-3 px-5 py-4">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <Gauge className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
            <h2 className="text-sm font-semibold text-foreground">{t("wbAccounts.limits.title")}</h2>
            {data ? (
              <span className="text-xs text-muted-foreground tabular-nums">
                {t("wbAccounts.limits.summary", {
                  total: data.counts.entries,
                  active: data.counts.active,
                })}
                {data.counts.unknown > 0
                  ? t("wbAccounts.limits.summaryUnknown", { unknown: data.counts.unknown })
                  : null}
                {/* 非限流错误（网络类）被排除时**报数**，不是静默丢弃。 */}
                {data.counts.skipped > 0 ? (
                  <span title={t("wbAccounts.limits.skippedHint")}>
                    {t("wbAccounts.limits.summarySkipped", { skipped: data.counts.skipped })}
                  </span>
                ) : null}
              </span>
            ) : null}
          </div>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">
            {t("wbAccounts.limits.subtitle")}
            {" "}
            <span className="text-muted-foreground/80">{t("wbAccounts.limits.probeShared")}</span>
            {" "}
            {/* 出口②：去网关看完整矩阵（矩阵天生要宽度，不搬进本卡）。 */}
            <Link
              to="/api-service?type=independent&sub=models"
              className="text-primary underline decoration-dashed underline-offset-2"
            >
              {t("wbAccounts.limits.openProbeMatrix")}
            </Link>
          </p>
          {/* 实测汇总（只报本作用域、本卡这一份数据）。 */}
          {tally && !running ? (
            <p className="mt-1 text-xs tabular-nums text-muted-foreground">
              {t("wbAccounts.limits.probeSummary", {
                cells: tally.cells,
                limited: tally.limited,
                ok: tally.ok,
                time: formatDate(ledgerAt ?? probe?.finished_at_ms ?? 0),
              })}
            </p>
          ) : null}
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <Tabs value={windowKey} onValueChange={(next) => setWindowKey(next as WindowKey)}>
            <TabsList aria-label={t("wbAccounts.limits.windowLabel")}>
              {(Object.keys(WINDOW_LABELS) as WindowKey[]).map((key) => (
                <TabsTrigger key={key} value={key}>
                  {t(WINDOW_LABELS[key])}
                </TabsTrigger>
              ))}
            </TabsList>
          </Tabs>

          {/* 入口①：卡头实测（进行中 ⇒ 进度 + 取消，不给「再点一次」的机会）。 */}
          {running && probe ? (
            <div
              className="flex items-center gap-2 rounded-md border border-border px-2 py-1"
              data-act="limits-probe-progress"
            >
              <Loader2 className="size-3.5 shrink-0 animate-spin text-primary" aria-hidden="true" />
              <span className="text-xs tabular-nums">
                {probe.done} / {probe.total}
              </span>
              <span className="max-w-40 truncate text-xs text-muted-foreground" title={probe.current}>
                {probe.current || "…"}
              </span>
              <Button size="sm" variant="ghost" className="h-6 px-2" onClick={() => void onCancel()}>
                {t("wbAccounts.limits.probeCancel")}
              </Button>
            </div>
          ) : (
            <Button
              size="sm"
              variant="outline"
              disabled={targets.cells === 0 || starting}
              onClick={() => void onProbeAll()}
              title={
                targets.cells === 0
                  ? t("wbAccounts.limits.probeAllEmpty")
                  : t("wbAccounts.limits.probeAllHint", {
                      accounts: targets.uids.length,
                      models: targets.models.length,
                      cells: targets.cells,
                    })
              }
            >
              {starting ? <Loader2 className="animate-spin" /> : <Sparkles />}
              {t("wbAccounts.limits.probeAll", { cells: targets.cells })}
            </Button>
          )}

          <Button size="sm" variant="outline" onClick={() => void load()} disabled={loading}>
            {loading ? <Loader2 className="animate-spin" /> : <RefreshCw />}
            {t("wbAccounts.limits.refresh")}
          </Button>
        </div>
      </div>

      <div className="border-t border-border px-5 py-4">
        {loading && !data ? (
          <div className="space-y-2">
            {[0, 1].map((index) => (
              <Skeleton key={index} className="h-16 w-full rounded-lg" />
            ))}
          </div>
        ) : error ? (
          <p className="text-sm text-destructive">
            {t("wbAccounts.limits.loadFailed")}：{error}
          </p>
        ) : limits.length === 0 ? (
          <EmptyState icon={Gauge} title={t("wbAccounts.limits.empty")} />
        ) : (
          <ul className="space-y-2">
            {limits.map((entry) => (
              <LimitRow
                key={limitRowKey(entry)}
                entry={entry}
                cell={probeCellOf(cells, entry.accountUid, entry.model)}
                running={running}
                starting={starting}
                onProbe={onProbeRow}
              />
            ))}
          </ul>
        )}

        {/* 实测独有：台账里没有记录、但实测查出限流的格。**单列**，不进台账。 */}
        {probeOnly.length > 0 ? (
          <Collapsible className="mt-3" defaultOpen>
            <CollapsibleTrigger
              className="group inline-flex items-center gap-1.5 rounded-md border border-border px-2 py-0.5 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
              data-act="limits-probe-only"
            >
              {t("wbAccounts.limits.probeOnlyTitle", { count: probeOnly.length })}
              <span className="text-[10px] transition-transform group-data-[state=open]:rotate-180">▾</span>
            </CollapsibleTrigger>
            <CollapsibleContent className="mt-2">
              <p className="mb-1.5 text-xs leading-5 text-muted-foreground">
                {t("wbAccounts.limits.probeOnlyHint")}
              </p>
              <ul className="space-y-1.5">
                {probeOnly.map((cell) => (
                  <li
                    key={`${cell.uid}:${cell.model}`}
                    className="flex flex-wrap items-center gap-2 rounded-lg border border-dashed border-border px-3 py-2"
                  >
                    <span className="text-sm font-medium">{cell.nickname || cell.uid}</span>
                    <Badge variant="secondary">{cell.model}</Badge>
                    <Badge variant={TONE_VARIANTS[probeTone(cell.state)]} className="shrink-0">
                      {t(`shared.gateway.probe.state.${cell.state}` as never)}
                    </Badge>
                    <span className="text-xs text-muted-foreground tabular-nums">
                      HTTP {cell.http_status}
                    </span>
                  </li>
                ))}
              </ul>
            </CollapsibleContent>
          </Collapsible>
        ) : null}
      </div>
    </Card>
  );
}

/**
 * React key：必须把**错误码**也编进去。
 *
 * 后端按 `(uid, model, code, reset_at)` 分桶 ⇒ 同一账号同模型下**不同错误码**的条目
 * `resetAt` 可能同为 `null`，只用 `uid:model:resetAt` 会撞出重复 key（列表渲染错位）。
 */
function limitRowKey(entry: ClientLimitEntry): string {
  const code = entry.code ?? entry.codeText ?? "?";
  return `${entry.accountUid ?? "?"}:${entry.model ?? "?"}:${code}:${entry.resetAt ?? "?"}`;
}

function LimitRow({
  entry,
  cell,
  running,
  starting,
  onProbe,
}: {
  entry: ClientLimitEntry;
  /** 该格的**实测**结论（跨作业累积）；`null` = 没实测过。 */
  cell: ModelProbeCell | null;
  running: boolean;
  starting: boolean;
  onProbe: (uid: string, model: string) => Promise<void>;
}) {
  const t = useT();
  const account = entry.account ?? t("wbAccounts.limits.accountUnknown");
  const model = entry.model ?? t("wbAccounts.limits.modelUnknown");
  // ⚠️ `resetAt` 是**配额窗口**的重置时刻，不是解锁时刻 —— 有实测实证时优先显示实证，
  // 否则「恢复于 明天 09:04」和「已恢复」并排会自相矛盾。
  const reset =
    entry.state === "recovered" && entry.evidenceAt !== null
      ? t("wbAccounts.limits.recoveredByEvidence", { time: formatDate(entry.evidenceAt) })
      : entry.resetAt !== null
        ? t("wbAccounts.limits.resetAt", { time: formatDate(entry.resetAt) })
        : t("wbAccounts.limits.resetUnknown");
  // 错误码：数值码优先，字符串码（实测 `ECONNRESET`）兜底 —— 都不丢。
  const code = entry.code ?? entry.codeText;
  /** 关联不到账号/模型 ⇒ 不能实测（不是 bug，是台账的 join 失败兜底）。 */
  const probeable = Boolean(entry.accountUid && entry.model);
  const conflict = limitConflict(entry, cell);
  const verdict = cell ? t(`shared.gateway.probe.state.${cell.state}` as never) : null;
  const probeTitle = cell
    ? t("wbAccounts.limits.probeVerdictHint", {
        state: verdict ?? "",
        http: cell.http_status,
        ms: cell.latency_ms,
        reset: cell.reset_at_ms ? ` · ${t("wbAccounts.limits.resetAt", { time: formatDate(cell.reset_at_ms) })}` : "",
      })
    : t("wbAccounts.limits.probeRowHint");

  return (
    <li
      className={cn(
        "flex items-start justify-between gap-4 rounded-lg border border-border px-3.5 py-3",
        entry.state === "active" && "border-l-2 border-l-warning bg-warning/[0.04]",
        entry.state === "unknown" && "border-l-2 border-l-muted-foreground/50",
        entry.state === "recovered" && "opacity-60",
      )}
    >
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <span className="truncate text-sm font-medium" title={entry.accountUid ?? undefined}>
            {account}
          </span>
          <Badge variant="secondary" className="shrink-0">
            {model}
          </Badge>
          {/* 实测结论：**并列**，不顶替台账徽章。 */}
          {cell ? (
            <Badge
              variant={TONE_VARIANTS[probeTone(cell.state)]}
              className="shrink-0"
              title={probeTitle}
            >
              {t("wbAccounts.limits.probeVerdict", { state: verdict ?? "" })}
            </Badge>
          ) : null}
          <Badge variant={STATE_VARIANTS[entry.state]} className="shrink-0">
            {t(STATE_LABELS[entry.state])}
          </Badge>
        </div>
        <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
          <span
            className={entry.state === "active" ? "font-medium text-warning" : undefined}
            title={t("wbAccounts.limits.resetHint")}
          >
            {reset}
          </span>
          <span>{t("wbAccounts.limits.occurredAt", { time: formatDate(entry.occurredAt) })}</span>
          {/* 合并了多条原始记录时如实标出条数，避免「条数」变成黑箱。 */}
          {entry.occurrences > 1 ? (
            <span
              className="tabular-nums"
              title={t("wbAccounts.limits.occurrencesHint", { count: entry.occurrences })}
            >
              {t("wbAccounts.limits.occurrences", { count: entry.occurrences })}
            </span>
          ) : null}
          {code !== null ? (
            <span className="inline-flex items-center gap-1">
              {t("wbAccounts.limits.code", { code })}
              {/* R7：排队类 code 只标注保守词「排队中」，不臆造精确语义文案。 */}
              {entry.code !== null && QUEUE_CODES.has(entry.code) ? (
                <Badge
                  variant="outline"
                  className="h-4 px-1.5 text-[10px] leading-none"
                  title={t("wbAccounts.limits.queuedHint")}
                >
                  {t("wbAccounts.limits.queued")}
                </Badge>
              ) : null}
            </span>
          ) : null}
          {/* 会话 ID 只给短码 + 全量 title，便于定位又不把长 ID 铺满一行。 */}
          <span className="font-mono" title={t("wbAccounts.limits.session", { id: entry.sessionId })}>
            {entry.sessionId.slice(0, 8)}
          </span>
        </div>
        {/* 冲突说明：只陈述事实，不裁定谁对（台账是历史、实测是此刻）。 */}
        {conflict === "recovered_by_probe" ? (
          <p className="mt-1 text-xs text-success" data-act="limits-conflict">
            {t("wbAccounts.limits.conflictRecovered")}
          </p>
        ) : conflict === "limited_after_recovery" ? (
          <p className="mt-1 text-xs text-warning" data-act="limits-conflict">
            {t("wbAccounts.limits.conflictLimited")}
          </p>
        ) : cell && cell.state === "no_credits" ? (
          <p className="mt-1 text-xs text-muted-foreground">{t("wbAccounts.limits.probeNotCredit")}</p>
        ) : null}
      </div>

      <div className="flex shrink-0 flex-col items-end gap-1.5">
        {/* 入口②：只打这一格（恒 1 格）。 */}
        <Button
          size="sm"
          variant="ghost"
          className="h-7 px-2 text-xs"
          disabled={!probeable || running || starting}
          title={probeable ? t("wbAccounts.limits.probeRowHint") : t("wbAccounts.limits.probeRowDisabled")}
          onClick={() => {
            if (!entry.accountUid || !entry.model) return;
            void onProbe(entry.accountUid, entry.model);
          }}
          data-act="limits-probe-row"
        >
          {running ? t("wbAccounts.limits.probing") : t("wbAccounts.limits.probeRow")}
        </Button>
      </div>
    </li>
  );
}
