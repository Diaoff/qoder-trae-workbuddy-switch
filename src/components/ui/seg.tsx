/**
 * 分段控件（原型 `.seg`，`demo.css:275-282`）。
 *
 * ## 为什么收成一个原语
 *
 * 用量页的**视图参数条**上有两个切换器 —— 「近 N 天」与「仅本作用域 / 合并全部作用域」。
 * 它们在原型里是**同一个 `.seg`**，几何必须逐值一致；各写一遍必然漂移
 * （改造前：范围条与合并条一个是 `p-1/rounded-lg`、另一个也是 `p-1/rounded-lg` 但
 * 按钮内边距不同，肉眼就是「两个不像一套的控件」）。
 *
 * 逐值（`demo.css:275-282`）：
 * - 容器 `padding: 2px` · `gap: 2px` · `border-radius: 10px`（= `rounded-md`，
 *   本主题 `--radius-md: calc(var(--radius) - 2px)` = 10px）· `background: var(--muted)`；
 * - 选项 `height: 26px` · `padding: 0 11px` · `border-radius: 8px`（= `rounded-sm`，
 *   `--radius-sm: calc(var(--radius) - 4px)` = 8px）· `font-size: 12px`；
 * - 选中 `font-weight: 550` + `box-shadow: var(--shadow-sm)`。
 *
 * ⚠️ **半径一律走语义类**（`rounded-md` / `rounded-sm`），不写 `rounded-[10px]` ——
 * 后者与主题令牌脱钩，改 `--radius` 时不会跟着走。
 *
 * ## 选中态底色 = `bg-card`（**照抄**原型 `var(--card)`）
 *
 * ⚠️ 本主题深色下 `--card` 与 `--muted` **同值**（`index.css:88` / `:98` 都是
 * `oklch(0.279 0.041 260.031)`）⇒ 选中项在深色下主要靠 `font-weight:550` + `shadow-sm`
 * 区分。这是**原型自带的取舍**，不是本组件的 bug：`Tabs`（`tabs.tsx:15`
 * `data-[state=active]:bg-card`）与账号页工具栏（`account-toolbar.tsx:217`
 * `data-[on=true]:bg-card`）两处已对齐的控件都是这个值。三处必须同值 ——
 * 否则同一个「用量」页里，子 Tab 条与参数条会呈现出两种选中态。
 *
 * ## 与 `Tabs` 的分工
 *
 * `Tabs` 管**内容面板切换**（有 `TabsContent`、有 roving tabindex 语义）；
 * 本原语只管**一个参数取哪一档**（`role="group"` + `aria-pressed`，不承载面板）。
 * 用法用量页的「趋势 / 模型 / 会话 / 积分」仍走 `Tabs`，参数条上的两枚走这里。
 */

import { cn } from "@/lib/utils";

export interface SegOption<T extends string | number> {
  value: T;
  label: string;
}

export interface SegProps<T extends string | number> {
  value: T;
  onChange: (next: T) => void;
  options: readonly SegOption<T>[];
  /** 无障碍分组名（读屏会先念它，再念每一档）。 */
  ariaLabel: string;
  disabled?: boolean;
  className?: string;
}

export function Seg<T extends string | number>({
  value,
  onChange,
  options,
  ariaLabel,
  disabled = false,
  className,
}: SegProps<T>) {
  return (
    <div
      className={cn(
        "inline-flex shrink-0 gap-0.5 rounded-md bg-muted p-0.5",
        disabled && "opacity-60",
        className,
      )}
      role="group"
      aria-label={ariaLabel}
    >
      {options.map((option) => (
        <button
          key={String(option.value)}
          type="button"
          className={cn(
            "h-[26px] cursor-pointer rounded-sm px-[11px] text-xs transition-colors",
            value === option.value
              ? "bg-card font-[550] text-foreground shadow-sm"
              : "text-muted-foreground hover:text-foreground",
            disabled && "cursor-not-allowed",
          )}
          onClick={() => onChange(option.value)}
          disabled={disabled}
          aria-pressed={value === option.value}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}
