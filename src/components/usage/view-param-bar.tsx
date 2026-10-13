/**
 * 用量 · **视图参数条**（原型 `.toolbar`，`demo.css:256-263`）—— **整页唯一一条**。
 *
 * ## 为什么在 `UsagePage` 上而不是每个视图里
 *
 * 原型 `viewUsage` 的结构是 **`页头 → seg（子 Tab）→ params → body`**
 * （`demo.js:987-996`）：**一条**参数条管**全部**子 Tab，`state.range` / `state.merge`
 * 也是页面级的 —— 切「趋势 → 积分」时范围**不重置**。
 *
 * 改造前每个视图页各持一份 `range` / `combineAll` 并各渲染一条参数条 ⇒ 两个后果：
 * ① 同一件事有两份实现（必然漂移）；② 切子 Tab 范围被悄悄重置回 30 天，
 * 用户以为「切了个 Tab 数据变了」。
 *
 * ## 为什么收敛成一条（PLAN §3.9）
 *
 * 改造前同一页有 **4 组**切换器（来源切片 / 时间范围 / 区域 / 合并），
 * 用户分不清哪一组决定「数据从哪来」、哪一组只是「怎么看」。
 * 收敛后：**范围（7/14/30）在本条**，**区域归侧栏 `ScopeSwitch`**，
 * **合并态在本条右端**（`RegionBar`）。
 *
 * ## 平台差异只在**数据**，不在控件
 *
 * 两个平台都渲染这一条、控件集逐枚相同：
 * - WorkBuddy：范围 → 后端统计窗口；合并态 → `RegionFilter` 的 `all`；
 * - Trae：范围 → 网关日志统计窗口；合并态 → `TraeTokenScope` 的 `all`
 *   （日志按区域分档，`all` = 国内 ∪ 国际 ∪ 未标注）。
 */

import { RegionBar } from "@/components/region-bar";
import { Seg } from "@/components/ui/seg";
import { useT } from "@/lib/i18n";
import type { UsageRange } from "@/lib/usage-charts";
import { cn } from "@/lib/utils";
import type { TranslationKey } from "@/locales/zh";

const RANGES: readonly UsageRange[] = [7, 14, 30];

const RANGE_LABEL: Record<UsageRange, TranslationKey> = {
  7: "wbStats.usage.range.7",
  14: "wbStats.usage.range.14",
  30: "wbStats.usage.range.30",
};

export interface ViewParamBarProps {
  range: UsageRange;
  onRangeChange: (range: UsageRange) => void;
  /** 合并态（仅本作用域 / 合并全部作用域）—— 两平台同一枚控件。 */
  combineAll: boolean;
  onCombineAllChange: (next: boolean) => void;
  className?: string;
}

/**
 * 四个视图页共用的**受控视图参数** —— 由 `UsagePage` 统一持有并传下去。
 *
 * ⚠️ 这是「用量」页的**视图参数契约**：谁要读范围 / 合并态就从 props 拿，
 * **不许**在视图页里再 `useState` 一份（那就又变成四条参数条）。
 * `onRangeChange` 是留给视图内部的**出口**（例：趋势视图「被视图参数筛没」时
 * 给一枚「看近 30 天」按钮）—— 它改的仍是**页面级**那一个 `range`。
 */
export interface UsageViewProps {
  range: UsageRange;
  combineAll: boolean;
  onRangeChange: (range: UsageRange) => void;
}

export function ViewParamBar({
  range,
  onRangeChange,
  combineAll,
  onCombineAllChange,
  className,
}: ViewParamBarProps) {
  const t = useT();
  return (
    <div
      className={cn(
        "mb-3.5 flex min-w-0 flex-wrap items-center gap-2 rounded-lg border bg-card px-3 py-[10px]",
        className,
      )}
    >
      <span className="text-[11px] text-muted-foreground">{t("wbStats.usage.param.title")}</span>
      <Seg
        value={range}
        onChange={onRangeChange}
        ariaLabel={t("wbStats.usage.param.aria")}
        options={RANGES.map((value) => ({ value, label: t(RANGE_LABEL[value]) }))}
      />
      <div className="grow" />
      <RegionBar value={combineAll} onChange={onCombineAllChange} />
    </div>
  );
}
