/**
 * 「用量」· Loomy 分区（积分快照）。
 *
 * ## 单位是「积分」（永久 + 每日两池）
 *
 * Loomy 的计费口径是**积分**（讯飞 points）。一期数据源是**账号库快照**
 * （最近一次「刷新积分」时记录的两池读数），不是逐日台账 —— 画「逐日趋势」
 * 只能得到一根贴地的假线 ⇒ **不画**（§3.5 不假承诺）。
 *
 * 展示当前两池快照 + 来源说明。逐日趋势等网关请求日志 / 定期采样落地后再补。
 */
import { useEffect, useState } from "react";
import { Coins, RefreshCw } from "lucide-react";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { StatTiles, type StatTile } from "@/components/usage/stat-tiles";
import * as api from "@/lib/api";
import { useLocation } from "react-router-dom";
import { ScopeChip } from "@/components/scope-chip";
import { platformOfPath } from "@/lib/platforms/registry";
import { scopeOf } from "@/lib/scope";
import { useScope } from "@/lib/use-scope";

interface Props {
  /** 受控的取数窗口（参数条在 `UsagePage`）。Loomy 一期无逐日数据，暂未消费。 */
  viewWindow?: unknown;
}

export default function LoomyUsagePage(_props: Props) {
  const location = useLocation();
  const product = platformOfPath(location.pathname);
  const [region] = useScope(product);
  const scope = scopeOf(product, region);
  const [accounts, setAccounts] = useState<api.LoomyAccountView[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const reload = async () => {
    setError(null);
    try {
      const view = await api.getLoomyAccounts("loomy-cn");
      setAccounts(view.accounts);
    } catch (err) {
      setError(api.asError(err));
    }
  };

  useEffect(() => {
    void reload();
  }, []);

  const tiles: StatTile[] = [
    {
      icon: Coins,
      label: "永久积分合计",
      value: String(accounts?.reduce((s, a) => s + (a.credits || 0), 0) ?? 0),
    },
    {
      icon: Coins,
      label: "每日池合计（当天作废）",
      value: String(accounts?.reduce((s, a) => s + (a.dailyCredits || 0), 0) ?? 0),
    },
    {
      icon: Coins,
      label: "账号数",
      value: String(accounts?.length ?? 0),
    },
  ];

  return (
    <div className="flex flex-col gap-4">
      <header className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <h2 className="text-base font-semibold">Loomy 积分</h2>
          <ScopeChip scope={scope} />
        </div>
        <Button
          variant="outline"
          size="sm"
          onClick={() => void reload()}
        >
          <RefreshCw className="size-4" />
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
            description="去账号页添加后即可看到积分快照。"
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
          <Alert>
            <Coins className="size-4" />
            <AlertTitle>数据来源说明</AlertTitle>
            <AlertDescription>
              这里展示的是**账号库快照**（每次点账号卡「刷新积分」时写入的读数），
              不是逐日台账 —— Loomy 的用量无本地记录，逐日趋势等定期采样落地后再补。
            </AlertDescription>
          </Alert>
        </>
      )}
    </div>
  );
}
