import { useEffect, useMemo, useState } from "react";
import { Cpu, RefreshCw } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { CatalogSourceBar } from "@/components/gateway/catalog-source-bar";
import { ModelTable } from "@/components/gateway/model-table";
import { DemoAction } from "@/components/demo-action";
import * as api from "@/lib/api";
import { formatAbsoluteTime, type CatalogSourceView } from "@/lib/catalog-source";
import { useT } from "@/lib/i18n";
import type { LoomyGatewayCatalog } from "@/lib/loomy-gateway";
import { scopeOf } from "@/lib/scope";
import { servedCount } from "@/lib/types";
import type { GatewayModelRow } from "@/lib/types";
import { cn } from "@/lib/utils";
import { EMPTY_DISABLED_SET, useGatewayStore } from "@/stores/gateway";

/**
 * Loomy 模型清单 —— 上游清单 + **禁用的两个出口**（P11 同款）。
 *
 * ## 数据源与另三条产品线的关系
 *
 * 与 Qoder **完全同款**：清单只能由网关**向官方拉**（要账号凭据），来源三级是
 * 「实时 / 上次成功 / 暂无清单」，三者都**必须带原因**。
 *
 * ## 与 Qoder 侧的唯一差别：**没有上下文档位开关**
 *
 * Loomy 的清单条目只带一个 `context_length`（上游不声明「默认档 / 上限档」），
 * 因此不渲染 `ContextSourceCard` —— 摆一个改了没效果的开关就是假控件。
 *
 * ## 未过滤清单 + 逐条 `disabled`
 *
 * 与 Qoder 同款：行渲染的 `disabled` 取 **store**（与网关出口 ① 同源），
 * 计数 `served = total − 已禁用`。
 */
export function LoomyModelList({
  catalog,
  refreshing,
  onRefresh,
  className,
}: {
  catalog: LoomyGatewayCatalog;
  refreshing: boolean;
  onRefresh: () => void;
  className?: string;
}) {
  const t = useT();
  /** 正在切换禁用状态的行 id（驱动该行按钮的 `<Loader2>` + `disabled`）。 */
  const [pendingId, setPendingId] = useState<string | null>(null);

  /** 本作用域（禁用偏好按作用域键控；四平台互不相通）。 */
  const scope = scopeOf("loomy", "cn");
  const disabledSet = useGatewayStore((s) => s.disabled[scope]) ?? EMPTY_DISABLED_SET;
  const loadDisabled = useGatewayStore((s) => s.loadDisabled);
  const setModelDisabled = useGatewayStore((s) => s.setModelDisabled);

  /* 进入 / 切作用域时读一次该作用域的禁用偏好。 */
  useEffect(() => {
    void loadDisabled(scope);
  }, [loadDisabled, scope]);

  /* 行 = 上游全量清单 + 用户偏好（`disabled`）。 */
  const rows = useMemo<GatewayModelRow[]>(
    () =>
      catalog.rows.map((model) => ({
        id: model.id,
        name: model.name,
        scope,
        // Loomy 无「非可服务」概念：在清单里即可用（与另三条同一判据）。
        available: true,
        disabled: disabledSet.has(model.id),
        // 上游只有一个窗口值 ⇒ 直接用（没有两档可切）。
        context_window: model.maxInputTokens,
      })),
    [catalog, disabledSet, scope],
  );
  const { served, total } = servedCount(rows);

  const labels = {
    colModel: t("shared.gateway.models.colModel"),
    colContext: t("shared.gateway.models.colContext"),
    colUpstream: t("shared.gateway.models.colUpstream"),
    colExposed: t("shared.gateway.models.colExposed"),
    upstreamAvailable: t("shared.gateway.models.upstreamAvailable"),
    upstreamUnavailable: t("shared.gateway.models.upstreamUnavailable"),
    exposed: t("shared.gateway.models.exposed"),
    disabled: t("shared.gateway.models.disabled"),
    disable: t("shared.gateway.models.disable"),
    enable: t("shared.gateway.models.enable"),
    copyId: t("shared.gateway.models.copyId"),
    idCopied: t("shared.gateway.models.idCopied"),
  };

  /**
   * 来源视图（契约与 WB / Trae 共用，词汇不同）。
   *
   * ★ 原因**恒非空**（契约）：后端 `note` 优先，缺失时按来源档位兜底。
   */
  const view: CatalogSourceView = {
    kind: catalog.source,
    badgeLabel: t(
      catalog.source === "live"
        ? "loomy.gateway.models.sourceLive"
        : catalog.source === "cached"
          ? "loomy.gateway.models.sourceCached"
          : "loomy.gateway.models.sourceUnavailable",
    ),
    badgeVariant:
      catalog.source === "live" ? "success" : catalog.source === "cached" ? "secondary" : "warning",
    identifiedAt: catalog.fetchedAt,
    reason:
      catalog.source === "live"
        ? null
        : (catalog.note ??
          t(
            catalog.source === "cached"
              ? "loomy.gateway.models.reasonCached"
              : "loomy.gateway.models.reasonUnavailable",
          )),
    identifiedCount: catalog.rows.length,
  };

  /** 切换某行的禁用状态；失败不改内存集合（先落库再更新）⇒ 行不会「假装切了」。 */
  async function onToggleDisabled(row: GatewayModelRow, disabled: boolean) {
    setPendingId(row.id);
    try {
      await setModelDisabled(scope, row.id, disabled);
      toast.success(
        t(
          disabled
            ? "shared.gateway.models.disabledToast"
            : "shared.gateway.models.enabledToast",
          { name: row.name },
        ),
      );
    } catch (e) {
      toast.error(t("shared.gateway.models.toggleFail"), { description: api.asError(e) });
    } finally {
      setPendingId(null);
    }
  }

  return (
    <div className={cn(className)}>
      {/* 无卡头 —— 分区名由左栏导航承担（与 WB / Trae 侧同口径）。 */}
      <p className="mb-3 text-xs text-muted-foreground">{t("loomy.gateway.models.note")}</p>

      {/* 刻意**没有**上下文档位开关：Loomy 的清单只有一个窗口值（见文件头）。 */}
      <CatalogSourceBar
        view={view}
        scanning={refreshing}
        onRescan={onRefresh}
        rescanLabel={t("loomy.gateway.models.refresh")}
        scanningLabel={t("loomy.gateway.models.scanning")}
        identifiedAtTitle={view.identifiedAt ? formatAbsoluteTime(view.identifiedAt) : undefined}
      />

      {total === 0 ? (
        <EmptyState
          icon={Cpu}
          title={t("loomy.gateway.models.empty")}
          description={t("loomy.gateway.models.emptyDesc")}
          action={
            <DemoAction>
              <Button variant="outline" size="sm" onClick={() => onRefresh()} disabled={refreshing}>
                <RefreshCw />
                {t("loomy.gateway.models.refresh")}
              </Button>
            </DemoAction>
          }
          className="py-4"
        />
      ) : (
        <>
          {/* Qoder **不渲染「上游状态」列**（在清单里即可用，决策 #3）⇒ showUpstream={false}。 */}
          <ModelTable
            rows={rows}
            pendingId={pendingId}
            onToggleDisabled={(row, disabled) => void onToggleDisabled(row, disabled)}
            showUpstream={false}
            labels={labels}
          />
          {/* 计数 = available && !disabled，与出口 ① 同源。
              ★ 守卫是 `total > 0`（**有清单就显示**）—— `served === 0` 不是「无数据」，
              而是「**全部被禁用**」这一真实且正需被看见的状态。 */}
          {total > 0 && (
            <p className="mt-3 text-xs text-muted-foreground" data-act="gw-model-count">
              {t("shared.gateway.models.count", { served, total })}
            </p>
          )}
          {/* ★ 两个出口同时生效的说明：否则用户不知道禁用真的生效了（会以为是假开关）。 */}
          <p className="mt-1 text-xs text-muted-foreground" data-act="gw-two-exits">
            {t("shared.gateway.models.twoExits")}
          </p>
        </>
      )}
    </div>
  );
}
