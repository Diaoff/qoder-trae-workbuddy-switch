import { useEffect, useState } from "react";
import { useLocation } from "react-router-dom";
import { Coins, Users } from "lucide-react";

import { ScopeChip } from "@/components/scope-chip";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import * as api from "@/lib/api";
import { platformOfPath } from "@/lib/platforms/registry";
import { scopeOf } from "@/lib/scope";
import { useScope } from "@/lib/use-scope";

/**
 * Loomy 概览页（单作用域）。
 *
 * 四格都**读得到**（§3.5 不假承诺）：账号数 / 可用凭据 / 积分快照合计 / 网关指引。
 * 数据全部来自本地账号库快照（不联网）。
 */
export default function LoomyOverviewPage() {
  const location = useLocation();
  const product = platformOfPath(location.pathname);
  const [region] = useScope(product);
  const scope = scopeOf(product, region);
  const [accounts, setAccounts] = useState<api.LoomyAccountView[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    api
      .getLoomyAccounts("loomy-cn")
      .then((view) => {
        if (!cancelled) setAccounts(view.accounts);
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(api.asError(err));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const usable = accounts?.filter((a) => a.hasCredential).length ?? 0;
  const totalCredits = accounts?.reduce((sum, a) => sum + (a.credits || 0), 0) ?? 0;
  const dailyCredits = accounts?.reduce((sum, a) => sum + (a.dailyCredits || 0), 0) ?? 0;

  return (
    <div className="flex flex-col gap-4 p-6">
      <header className="flex items-center gap-2">
        <h1 className="text-lg font-semibold">Loomy 概览</h1>
        <ScopeChip scope={scope} />
      </header>

      {error && <p className="text-sm text-destructive">{error}</p>}
      {accounts === null && !error && (
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          {Array.from({ length: 4 }).map((_, i) => (
            <Skeleton key={i} className="h-24" />
          ))}
        </div>
      )}
      {accounts !== null && accounts.length === 0 && (
        <Card className="p-0">
          <EmptyState
            icon={Users}
            title="还没有 Loomy 账号"
            description="去账号页用「短信登录」或「导入凭据」添加。"
            action={
              <Button asChild variant="outline" size="sm">
                <a href="#/s/loomy-cn/accounts">去账号页</a>
              </Button>
            }
          />
        </Card>
      )}
      {accounts !== null && accounts.length > 0 && (
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          <StatTile label="账号总数" value={String(accounts.length)} icon={<Users className="size-4" />} />
          <StatTile label="可用凭据" value={String(usable)} icon={<Users className="size-4" />} />
          <StatTile label="永久积分合计" value={String(totalCredits)} icon={<Coins className="size-4" />} />
          <StatTile
            label="每日池合计（当天作废）"
            value={String(dailyCredits)}
            icon={<Coins className="size-4" />}
          />
        </div>
      )}
    </div>
  );
}

function StatTile({ label, value, icon }: { label: string; value: string; icon: React.ReactNode }) {
  return (
    <Card className="p-4">
      <div className="flex items-center gap-2 text-xs text-muted-foreground">
        {icon}
        {label}
      </div>
      <p className="mt-1 text-2xl font-semibold">{value}</p>
    </Card>
  );
}
