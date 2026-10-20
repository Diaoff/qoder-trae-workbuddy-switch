import { Badge } from "@/components/ui/badge";
import { useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";

/**
 * 「上下文上限」只读说明卡（WorkBuddy / Qoder 的模型页）。
 *
 * ## 为什么这两个平台没有开关
 *
 * Max Mode 是 **Trae 上游**的请求级布尔字段（`is_max_mode`）：同一个模型有 `dev` 与 `max`
 * 两个窗口，才需要「开 / 关」。WorkBuddy 的目录只有一个 `context_window`、Qoder 的模型
 * 元数据只有一个 `max_input_tokens` —— 它们**本来就是模型上限**，没有第二个模式可切。
 *
 * 因此这里只说明「数字从哪来、怎么用」，不放一个点了没反应的假开关：
 * `/v1/models` 已按目录声明 `context_length`，客户端直接用即可。
 */
export function ContextSourceCard({ className }: { className?: string }) {
  const t = useT();
  return (
    <div
      className={cn(
        "mb-3 rounded-lg border border-border/60 bg-muted/20 px-4 py-3",
        className,
      )}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="text-sm font-medium">{t("shared.gateway.models.contextTitle")}</div>
          <p className="mt-1 text-xs leading-relaxed text-muted-foreground">
            {t("shared.gateway.models.contextFixedDesc")}
          </p>
        </div>
        <Badge variant="secondary" className="shrink-0 rounded-md">
          {t("shared.gateway.models.contextFromCatalog")}
        </Badge>
      </div>
    </div>
  );
}
