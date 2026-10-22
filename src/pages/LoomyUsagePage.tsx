/**
 * 「用量」· Loomy 分区（积分快照 + 逐日趋势）。
 *
 * ## 单位是「积分」（永久 + 每日两池）
 *
 * Loomy 的计费口径是**积分**（讯飞 points），两池**不可通约**：
 * 永久池（`balance`）不清零；每日池（`dailyBalance`）当天作废。
 * 合并成一个数字会让用户看不出「哪部分今天会作废」⇒ 图与卡都分两列。
 *
 * ## 趋势的数据源是**本地采样台账**，不是账号库快照
 *
 * 账号库里的 `credits` / `dailyCredits` 只是**最近一次**读数（覆盖写）。
 * 拿它画逐日趋势只能得到一根贴地的假线 —— 那正是一期「不画趋势」的理由。
 * 二期（9.3）加了排程 `loomy_points_sample`：每日两帧把两池读数记进
 * `loomy/credits_daily.cn.json`，本页读的就是那份台账（`loomy_credits_daily`，
 * **只读本地文件、不联网**）。
 *
 * 台账为空时**如实说明**（「还没有采样数据」+ 去哪开），不画一根 0 线冒充。
 *
 * ## 视图参数是**受控**的
 *
 * `viewWindow` 由 `UsagePage` 传入（参数条在那边渲染）—— 本页不持有窗口状态，
 * 也不放页内选择器（与 `QoderUsagePage` / `TraeTokenStatsPage` 同一条纪律）。
 */
import { useEffect, useMemo, useState } from "react";
import { useLocation } from "react-router-dom";
import { Bar, BarChart, CartesianGrid, Legend, XAxis, YAxis } from "recharts";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { ChartFrame } from "@/components/usage/chart-frame";
import { StatTiles, type StatTile } from "@/components/usage/stat-tiles";
import { ScopeChip } from "@/components/scope-chip";
import * as api from "@/lib/api";
import { formatCredits } from "@/components/usage/format";
import { platformOfPath } from "@/lib/platforms/registry";
import { scopeOf } from "@/lib/scope";
import { useScope } from "@/lib/use-scope";
import { usageWindowKeys, type UsageWindow } from "@/lib/usage-window";
import { Coins, RefreshCw } from "lucide-react";

interface Props {
  /** 受控的取数窗口（参数条在 `UsagePage`）。 */
  viewWindow?: UsageWindow;
}

export default function LoomyUsagePage({ viewWindow }: Props) {
  const location = useLocation();
  const product = platformOfPath(location.pathname);
  const [region] = useScope(product);
  const scope = scopeOf(product, region);
  const [accounts, setAccounts] = useState<api.LoomyAccountView[] | null>(null);
  const [daily, setDaily] = useState<api.LoomyCreditsDailyView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const reload = async () => {
    setError(null);
    setBusy(true);
    try {
      // 两个数据源是**同一个作用域**的两份本地文件，一起取（少一次往返）。
      const [view, dailyView] = await Promise.all([
        api.getLoomyAccounts("loomy-cn"),
        api.loomyCreditsDaily("loomy-cn"),
      ]);
      setAccounts(view.accounts);
      setDaily(dailyView);
    } catch (err) {
      setError(api.asError(err));
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    void reload();
  }, []);

  /** 窗口键集合（`today` / 档位 / 自定义三态统一口径）。 */
  const windowKeys = useMemo(
    () => (viewWindow ? new Set(usageWindowKeys(viewWindow)) : null),
    [viewWindow],
  );

  /**
   * 逐日合计：**跨账号求和**。
   *
   * 台账按 `uid` 分列（见 `credits_daily.rs` 模块头），这里求和只用于画总量趋势；
   * 账号级的明细在卡片的合计里已经表达（永久 / 每日两列），此页不再铺第二张表。
   */
  const bars = useMemo(() => {
    const days = (daily?.days ?? []).filter((day) => !windowKeys || windowKeys.has(day.date));
    return days.map((day) => ({
      label: day.date.slice(5),
      permanent: day.accounts.reduce((sum, row) => sum + (row.permanent || 0), 0),
      daily: day.accounts.reduce((sum, row) => sum + (row.daily || 0), 0),
    }));
  }, [daily, windowKeys]);

  const peak = useMemo(
    () => bars.reduce((max, bar) => Math.max(max, bar.permanent + bar.daily), 0),
    [bars],
  );

  const permanentTotal = accounts?.reduce((s, a) => s + (a.credits || 0), 0) ?? 0;
  const dailyTotal = accounts?.reduce((s, a) => s + (a.dailyCredits || 0), 0) ?? 0;

  const tiles: StatTile[] = [
    {
      icon: Coins,
      label: "永久积分合计",
      value: String(permanentTotal),
      hint: "不变动就不减少",
    },
    {
      icon: Coins,
      label: "每日池合计",
      value: String(dailyTotal),
      hint: "当天作废",
    },
    {
      icon: Coins,
      label: "账号数",
      value: String(accounts?.length ?? 0),
    },
  ];

  return (
    <div className="flex min-w-0 flex-col gap-4">
      <header className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <h2 className="text-base font-semibold">Loomy 积分</h2>
          <ScopeChip scope={scope} />
        </div>
        <Button variant="outline" size="sm" onClick={() => void reload()} disabled={busy}>
          <RefreshCw className={busy ? "animate-spin" : ""} />
        </Button>
      </header>

      {error && <p className="text-sm text-destructive">{error}</p>}

      {accounts === null && !error && (
        <div className="grid gap-3 sm:grid-cols-3">
          {Array.from({ length: 3 }).map((_, i) => (
            <Skeleton key={i} className="h-24" />
          ))}
        </div>
      )}

      {accounts !== null && accounts.length === 0 && (
        <Card className="p-0">
          <EmptyState
            icon={Coins}
            title="还没有 Loomy 账号"
            description="去账号页添加后即可看到积分快照与逐日趋势。"
            action={
              <Button asChild variant="outline" size="sm">
                <a href="#/s/loomy-cn/accounts">去账号页</a>
              </Button>
            }
          />
        </Card>
      )}

      {accounts !== null && accounts.length > 0 && (
        <>
          <StatTiles tiles={tiles} />

          {bars.length === 0 ? (
            /* 空态**必须带出口**（§1.6 四要素）：趋势靠采样台账，用户得知道去哪开。 */
            <Card className="gap-0 py-0">
              <EmptyState
                icon={Coins}
                title="还没有采样数据"
                description="「每日采样」默认开启（每天 09:00 / 21:00 各采一帧），也可以在设置页改时点或立即刷新一次积分来写入今天这一帧。"
                action={
                  <div className="flex gap-2">
                    <Button asChild variant="outline" size="sm">
                      <a href="#/s/loomy-cn/settings?section=schedule">去设置采样</a>
                    </Button>
                    <Button variant="outline" size="sm" disabled={busy} onClick={() => void reload()}>
                      <RefreshCw className={busy ? "animate-spin" : ""} />
                      重新读取
                    </Button>
                  </div>
                }
              />
            </Card>
          ) : (
            <ChartFrame
              title="每日积分"
              caption={`共 ${bars.length} 天 · 单日峰值 ${formatCredits(peak)}（两池合计）`}
              ariaLabel="Loomy 每日积分趋势（永久池与每日池分列）"
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
                <Legend
                  wrapperStyle={{ fontSize: 11 }}
                  formatter={(value: string) => (value === "permanent" ? "永久池" : "每日池")}
                />
                <Bar dataKey="permanent" fill="var(--primary)" radius={[3, 3, 0, 0]} />
                <Bar dataKey="daily" fill="var(--muted-foreground)" radius={[3, 3, 0, 0]} />
              </BarChart>
            </ChartFrame>
          )}

          <Alert>
            <Coins className="size-4" />
            <AlertTitle>数据来源说明</AlertTitle>
            <AlertDescription>
              上方数字是**账号库快照**（点账号卡「刷新积分」或排程采样时写入的最近一次读数）；
              趋势图来自本地采样台账（每日两帧，按账号分列后求和，保留 90 天）。
              两者都不含经网关的第三方调用，也不含其它设备上的用量。
            </AlertDescription>
          </Alert>
        </>
      )}
    </div>
  );
}
