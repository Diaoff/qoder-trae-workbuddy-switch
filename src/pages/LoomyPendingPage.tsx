import { useLocation } from "react-router-dom";
import { PlugZap } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { platformOfPath } from "@/lib/platforms/registry";
import { scopeOf } from "@/lib/scope";
import { useScope } from "@/lib/use-scope";

/**
 * Loomy 侧**尚未接入**的目的地（会话 / 网关）—— **结构化空态**。
 *
 * 与 `QoderPendingPage` 同款纪律：复用别的平台的页面会把**别的平台的账号**渲染进来
 * （「静默错平台」），空态把「还没有这个页面的数据源」讲清楚并给唯一出口（账号页）。
 *
 * 网关目的地的文案指向**统一网关**（`loomy-cn:` 命名空间走 57892，无独立端口）。
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
          description="Loomy 一期提供账号管理 / 签到 / 积分；模型调用走统一网关（模型名写 loomy-cn:<模型id>，端口 57892），无独立端口。账号管理请去账号页。"
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
