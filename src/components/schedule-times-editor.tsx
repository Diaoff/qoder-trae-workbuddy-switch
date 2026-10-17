import { useState } from "react";

import { Plus, X } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useT } from "@/lib/i18n";

/** `HH:MM`（00:00–23:59）。与后端 `schedule::TimeOfDay` 的线上格式**逐字一致**。 */
const TIME_PATTERN = /^([01]\d|2[0-3]):[0-5]\d$/;

/** 时点表的排序键：自 00:00 起的分钟数（`"09:37"` → 577）。 */
function minutesOf(time: string): number {
  const [hour, minute] = time.split(":");
  return Number(hour) * 60 + Number(minute);
}

/**
 * 某一类定时任务的**时点列表编辑器**：以标签展示当前时点，可逐个删除或新增。
 *
 * ## 粒度是「时:分」（issue #27）
 *
 * 此前这里是 `<input type="number">` + 0-23 的整点校验，用户只能把签到定在整点。
 * 现在改用原生 `<input type="time">`（`step=60`）：浏览器自带时/分选择，值天然是
 * `"HH:MM"`，与后端 `TimeOfDay` 的序列化格式一字不差 —— 不需要在前端做格式转换，
 * 也就不会出现「前端存 `9:5`、后端认不出」这类两端格式漂移。
 *
 * ## 为什么抽成独立组件（而不是留在某个设置页里）
 *
 * 同一种控件现在有三个使用方：WorkBuddy 设置页的「定时任务」卡片，以及 Trae 设置页
 * 的「自动签到」「JWT 续期」两张卡片。**各写一遍必然漂移** —— 一边加了重复值提示、
 * 另一边没加，用户的感受就是「同一个功能在两页里行为不同」。校验规则（格式、去重、排序）
 * 属于规则而不是版式，必须只有一份实现。
 *
 * 组件本身**不认识任何任务**：`id` / `times` / `onChange` 全部由调用方给，
 * 因此「哪个字段是哪类任务的」这类知识仍留在各自的设置页里。
 */
export function TimesEditor({
  id,
  times,
  disabled,
  onChange,
}: {
  id: string;
  times: string[];
  disabled?: boolean;
  onChange: (times: string[]) => void;
}) {
  const t = useT();
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);

  function add() {
    const value = draft.trim();
    if (!TIME_PATTERN.test(value)) {
      setError(t("wbSettings.schedule.timeInvalid"));
      return;
    }
    if (times.includes(value)) {
      setError(t("wbSettings.schedule.timeDuplicate", { time: value }));
      return;
    }
    setError(null);
    setDraft("");
    onChange([...times, value].sort((a, b) => minutesOf(a) - minutesOf(b)));
  }

  return (
    <div className="flex w-full flex-col items-end gap-2 sm:w-auto">
      <div className="flex w-full flex-wrap items-center justify-end gap-1.5">
        {times.length === 0 ? (
          <span className="text-xs text-muted-foreground">{t("wbSettings.schedule.timeEmpty")}</span>
        ) : (
          times.map((time) => (
            <Badge key={time} variant="secondary" className="gap-1 pr-1 font-mono">
              {time}
              <button
                type="button"
                aria-label={t("wbSettings.schedule.timeRemoveAria", { time })}
                className="rounded-full p-0.5 text-muted-foreground transition-colors hover:bg-foreground/10 hover:text-foreground disabled:pointer-events-none disabled:opacity-50"
                disabled={disabled}
                onClick={() => onChange(times.filter((value) => value !== time))}
              >
                <X className="size-3" />
              </button>
            </Badge>
          ))
        )}
      </div>
      <div className="flex w-full items-center justify-end gap-2">
        {/* `type="time"` + `step=60`：秒被隐藏，值恒为 `HH:MM`（带秒时浏览器会给出
            `HH:MM:SS`，那是后端认不出的格式，故必须显式设 step）。 */}
        <Input
          id={id}
          className="w-full sm:w-32"
          type="time"
          step={60}
          value={draft}
          disabled={disabled}
          onChange={(e) => {
            setDraft(e.target.value);
            if (error) setError(null);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              add();
            }
          }}
        />
        <Button type="button" size="sm" variant="outline" onClick={add} disabled={disabled}>
          <Plus />
          {t("wbSettings.schedule.timeAdd")}
        </Button>
      </div>
      {error && <p className="text-xs text-destructive">{error}</p>}
    </div>
  );
}
