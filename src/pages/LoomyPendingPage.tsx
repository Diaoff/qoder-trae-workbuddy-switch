import { useLocation } from "react-router-dom";
import { PlugZap } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { platformOfPath } from "@/lib/platforms/registry";
import { scopeOf } from "@/lib/scope";
import { useScope } from "@/lib/use-scope";

/**
 * Loomy 侧**尚未接入**的目的地（当前只有「会话」）—— **结构化空态**。
 *
 * 与 `QoderPendingPage` 同款纪律：复用别的平台的页面会把**别的平台的账号**渲染进来
 * （「静默错平台」），空态把「还没有这个页面的数据源」讲清楚并给唯一出口（账号页）。
 *
 * ⚠️ 二期 9.2 起「网关」不再是未接入项 —— 它有了真实的
 * `LoomyApiServicePage`（独立端口 57896 + `sk-loomy-` Key 库）。
 * 本页只剩会话：Loomy 没有「会话副本」这个概念（无本地客户端），数据源不存在。
 */
export default function LoomyPendingPage() {
  const location = useLocation();
  const product = platformOfPath(location.pathname);
  const [region] = useScope(product);
  const scope = scopeOf(product, region);

  return (
    <div className="p-6">
      <Card className="p-0">
        <EmptyState
          icon={PlugZap}
          title="该分区尚未接入"
          description="Loomy 没有本地客户端，也就没有「会话副本」这类数据源 —— 会话页不会凭空造数据。账号管理 / 签到 / 积分 / 网关请用各自的目的地。"
          action={
            <Button variant="outline" size="sm" asChild>
              <a href={`#/s/${scope}/accounts`}>去账号页</a>
            </Button>
          }
        />
      </Card>
    </div>
  );
}
