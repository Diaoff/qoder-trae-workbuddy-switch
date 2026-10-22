import { useState } from "react";
import { useLocation, useSearchParams } from "react-router-dom";

import { AppSettingsGroups } from "@/components/app-settings";
import { TimesEditor } from "@/components/schedule-times-editor";
import { BackupSection } from "@/components/settings/backup-section";
import {
  parseSettingsSubnav,
  SettingsSubnav,
  SETTINGS_SUBNAV,
  type SettingsSubnavId,
} from "@/components/settings/settings-subnav";
import { SettingsFieldRow, SettingsGroup } from "@/components/settings-primitives";
import { ScopeChip } from "@/components/scope-chip";
import { CardContent } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import * as api from "@/lib/api";
import { platformOfPath } from "@/lib/platforms/registry";
import {
  loadScheduleConfig,
  saveSchedulePatch,
  SCHEDULE_CONFIG_KEY,
} from "@/lib/schedule-config";
import { scopeOf } from "@/lib/scope";
import type { ScheduleConfig } from "@/lib/types";
import { useCachedResource } from "@/lib/use-cached-resource";
import { useScope } from "@/lib/use-scope";
import { toast } from "sonner";

/**
 * Loomy 设置页。
 *
 * 与 Qoder 设置页同款收敛：**应用级设置与备份是跨平台共享面**
 * （`AppSettingsGroups` / `BackupSection` 内部按 `Scope::all()` 自动覆盖 `loomy-cn`）。
 *
 * ## 为什么这里只有「计划任务」一项是 Loomy 专属的
 *
 * 二期 9.3 给 Loomy 加了一类排程 —— **积分逐日采样**（`loomy_points_sample`）。
 * 它必须在这里有开关与时间表，否则「后台在采样、用户却看不到也关不掉」就是
 * 本仓反复消灭的**假控件**的反面（真动作没有控件）。
 *
 * ⚠️ 但它**不是** Loomy 独有的排程页：`schedule_config.json` 是**全局单份**，
 * 三类产品各占自己的任务字段。本页只呈现 Loomy 那一条，其余字段改由各自设置页负责。
 */
const LOOMY_SECTION_IDS: readonly SettingsSubnavId[] = ["general", "schedule", "backup"];

/** 收窄后的子导航项（顺序取自共享表，不另排）。 */
const LOOMY_SETTINGS_SECTIONS = SETTINGS_SUBNAV.filter((item) =>
  LOOMY_SECTION_IDS.includes(item.id),
);

export default function LoomySettingsPage() {
  const location = useLocation();
  const [params] = useSearchParams();
  const product = platformOfPath(location.pathname);
  const [region] = useScope(product);
  const scope = scopeOf(product, region);

  /**
   * 排程是**全局单份**（`schedule_config.json`，按任务分家而不是按产品 / 区域），
   * 与 WB / Trae / Qoder 设置页读写同一份文件、共用 `SCHEDULE_CONFIG_KEY`。
   */
  const { data: schedule, patch: patchSchedule } = useCachedResource<ScheduleConfig>(
    SCHEDULE_CONFIG_KEY,
    loadScheduleConfig,
  );
  const [scheduleSaving, setScheduleSaving] = useState(false);

  /**
   * 保存排程：乐观落本地 → **整份**提交 → 回读权威值；失败回滚并提示。
   *
   * 必须带当前整份配置（见 `saveSchedulePatch`）：后端按整份解析，只发一个字段
   * 会让其余字段被默认值覆盖 —— 那是静默的配置丢失。
   */
  async function saveSchedule(next: Partial<ScheduleConfig>) {
    if (!schedule || scheduleSaving) return;
    const previous = schedule;
    setScheduleSaving(true);
    patchSchedule((prev) => ({ ...prev, ...next }));
    try {
      const saved = await saveSchedulePatch(previous, next);
      patchSchedule(() => saved);
    } catch (error) {
      patchSchedule(() => previous);
      toast.error("保存计划任务失败", { description: api.asError(error) });
    } finally {
      setScheduleSaving(false);
    }
  }

  const [section, setSection] = useState<SettingsSubnavId>(() => {
    const parsed = parseSettingsSubnav(params.get("section"));
    return LOOMY_SECTION_IDS.includes(parsed) ? parsed : "general";
  });

  return (
    <div className="flex flex-col gap-4 p-6">
      <header className="flex items-center gap-2">
        <h1 className="text-lg font-semibold">Loomy 设置</h1>
        <ScopeChip scope={scope} />
      </header>
      <SettingsSubnav items={LOOMY_SETTINGS_SECTIONS} active={section} onSelect={setSection} />

      {section === "general" && <AppSettingsGroups />}

      {/* 积分逐日采样：趋势图的数据来源（用量页「每日积分」那张图）。
          与签到分开摆 —— 采样是**只读 GET**，不是替用户去领东西。 */}
      {section === "schedule" && (
        <SettingsGroup id="loomy-settings-sample" title="积分逐日采样">
          <CardContent className="space-y-0 p-0">
            <SettingsFieldRow
              label="启用每日采样"
              description="按下面的时点读取各账号的两池积分并记入本地台账（只读请求，不会签到或消耗积分）。关掉后用量页的趋势不再更新。"
            >
              <Switch
                checked={schedule?.loomy_points_sample_enabled ?? false}
                disabled={!schedule || scheduleSaving}
                onCheckedChange={(checked) =>
                  void saveSchedule({ loomy_points_sample_enabled: checked })
                }
                aria-label="启用每日采样"
              />
            </SettingsFieldRow>
            <SettingsFieldRow
              label="采样时点"
              description="每天每个时点采一帧；同日重复采样保留最后一次读数（趋势按日一格）。"
              htmlFor="loomy-sample-time"
            >
              <TimesEditor
                id="loomy-sample-time"
                times={schedule?.loomy_points_sample_times ?? []}
                jitter={schedule?.loomy_points_sample_jitter_minutes ?? 0}
                disabled={!schedule || scheduleSaving}
                onChange={(times) => void saveSchedule({ loomy_points_sample_times: times })}
                onJitterChange={(minutes) =>
                  void saveSchedule({ loomy_points_sample_jitter_minutes: minutes })
                }
              />
            </SettingsFieldRow>
          </CardContent>
        </SettingsGroup>
      )}

      {section === "backup" && <BackupSection />}
    </div>
  );
}
