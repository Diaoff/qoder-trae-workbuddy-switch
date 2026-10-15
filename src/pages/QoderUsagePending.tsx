import { useLocation, useNavigate } from "react-router-dom";
import { ChartNoAxesColumn } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { useT } from "@/lib/i18n";
import { platformOfPath } from "@/lib/platforms/registry";
import { scopeOf } from "@/lib/scope";
import { useScope } from "@/lib/use-scope";

/**
 * Qoder 的用量子页（Token / 积分）—— **尚未接入**的结构化空态。
 *
 * ## 为什么单独一个文件（而不是复用 `QoderPendingPage`）
 *
 * 它由 `UsagePage` 的 `USAGE_PAGES.qoder` 以**懒加载**方式渲染（与四个平台的
 * 其它子页同款），而 `UsagePage` 已经渲染了页头与视图参数条
 * ⇒ 本页**不得**再渲染标题或 `max-w-[1240px]` 容器（「每个目的地只有一处页头」）。
 * `QoderPendingPage` 是**整页**（自带页头），两者用途不同，故不合并。
 *
 * ⚠️ 与整页空态同款：**不加载数据、不渲染计数**（编一个「0」等于臆造）。
 */
export default function QoderUsagePending() {
  const t = useT();
  const navigate = useNavigate();
  const location = useLocation();
  const product = platformOfPath(location.pathname);
  const [region] = useScope(product);
  const scope = scopeOf(product, region);

  return (
    <Card className="gap-0 py-0">
      <EmptyState
        icon={ChartNoAxesColumn}
        title={t("qoder.page.usage.pending.title")}
        description={t("qoder.page.usage.pending.description")}
        action={
          <Button variant="outline" size="sm" onClick={() => navigate(`/s/${scope}/accounts`)}>
            {t("qoder.page.usage.pending.action")}
          </Button>
        }
      />
    </Card>
  );
}
