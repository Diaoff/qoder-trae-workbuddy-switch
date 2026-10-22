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
import { ScopeChip, scopeLabel } from "@/components/scope-chip";
import { CardContent } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import * as api from "@/lib/api";
import { useT } from "@/lib/i18n";
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
 * ## 版式与另三个设置页**同构**（2026-10-22 修正）
 *
 * 此前本页是个简易版：`<div className="p-6">` + 18px 标题 + 单列堆叠，没有两栏。
 * 而 WB / Trae / Qoder 三页都是：28px 标题 + `pageHead.settings` 副标题 + ScopeChip
 * 页头，外加 `data-split-layout` 两栏（左子导航 / 右主区）。同一个应用里两种设置页
 * 版式，用户会以为 Loomy 这块「没做完」。
 *
 * ⇒ 现在四页共用同一套骨架：`data-split-layout` + `parseSettingsSubnav` 的 URL 承载
 * + 主区 16px 间距 + 子导航取自共享表 `SETTINGS_SUBNAV`（只按内容收窄 `items`，
 * 不复制表 —— 复制即漂移的种子）。
 *
 * ## 内容上两项是 Loomy 专属的（两条排程任务）
 *
 * 1. **自动签到**（`loomy_checkin`）：**默认关**（写动作，替用户去上游领东西 ⇒ opt-in，
 *    判据同另三条 `*_checkin`）。没有这个开关时它只是个永不执行的配置项。
 * 2. **积分逐日采样**（二期 9.3 的 `loomy_points_sample`）：**默认开**（只读 GET）。
 *    真动作必须有控件，否则「后台在采样、用户看不到也关不掉」。
 *
 * `schedule_config.json` 是**全局单份**，各产品线各占自己的任务字段；本页只呈现
 * Loomy 那一条，其余字段由各自设置页负责。
 */
const LOOMY_SECTION_IDS: readonly SettingsSubnavId[] = [
  "general",
  "schedule",
  "backup",
];

/** 收窄后的子导航项（顺序取自共享表，不另排）。 */
const LOOMY_SETTINGS_SECTIONS = SETTINGS_SUBNAV.filter((item) =>
  LOOMY_SECTION_IDS.includes(item.id),
);

export default function LoomySettingsPage() {
  const t = useT();
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
      toast.error(t("loomy.page.settings.schedule.saveFailed"), {
        description: api.asError(error),
      });
    } finally {
      setScheduleSaving(false);
    }
  }

  const [section, setSection] = useState<SettingsSubnavId>(() => {
    const parsed = parseSettingsSubnav(params.get("section"));
    return LOOMY_SECTION_IDS.includes(parsed) ? parsed : "general";
  });

  return (
    <div className="mx-auto min-w-0 w-full max-w-[1240px] px-7 pt-[26px] pb-[60px]">
      <header className="mb-5 flex items-start gap-4">
        <div className="min-w-0">
          <h1 className="text-[28px] font-[640] tracking-[-0.02em]">
            {t("loomy.page.settings.title")}
          </h1>
          <p className="mt-1.5 text-[12.5px] text-muted-foreground">
            {t("pageHead.settings", { scope: scopeLabel(t, scope) })}
          </p>
        </div>
        <div className="grow" />
        <ScopeChip scope={scope} />
      </header>

      {/* 两栏容器：方向 / 断点整组在 `index.css` 的 `[data-split-layout]` 里
          （与 WB / Trae / Qoder 三个设置页**同一套骨架**）。 */}
      <div data-split-layout className="min-w-0">
        <SettingsSubnav items={LOOMY_SETTINGS_SECTIONS} active={section} onSelect={setSection} />

        {/* 主区 `.split-main`：分区之间 16px。 */}
        <div className="min-w-0 flex-1 space-y-4">
          {section === "general" && <AppSettingsGroups />}

          {section === "schedule" && (
            <>
              {/* 自动签到：**写**动作（触发每日额度重置）⇒ 默认关，与另三条签到同判据。
                  与采样分开摆：一个是替你领东西，一个只是读一眼。 */}
              <SettingsGroup
                id="loomy-settings-checkin"
                title={t("loomy.page.settings.schedule.checkinGroup")}
              >
                <CardContent className="space-y-0 p-0">
                  <SettingsFieldRow
                    label={t("loomy.page.settings.schedule.checkinEnable")}
                    description={t("loomy.page.settings.schedule.checkinDesc")}
                  >
                    <Switch
                      checked={schedule?.loomy_checkin_enabled ?? false}
                      disabled={!schedule || scheduleSaving}
                      onCheckedChange={(checked) =>
                        void saveSchedule({ loomy_checkin_enabled: checked })
                      }
                      aria-label={t("loomy.page.settings.schedule.checkinEnable")}
                    />
                  </SettingsFieldRow>
                  <SettingsFieldRow
                    label={t("loomy.page.settings.schedule.checkinTimesLabel")}
                    description={t("loomy.page.settings.schedule.checkinTimesDesc")}
                    htmlFor="loomy-checkin-time"
                  >
                    <TimesEditor
                      id="loomy-checkin-time"
                      times={schedule?.loomy_checkin_times ?? []}
                      jitter={schedule?.loomy_checkin_jitter_minutes ?? 0}
                      disabled={!schedule || scheduleSaving}
                      onChange={(times) => void saveSchedule({ loomy_checkin_times: times })}
                      onJitterChange={(minutes) =>
                        void saveSchedule({ loomy_checkin_jitter_minutes: minutes })
                      }
                    />
                  </SettingsFieldRow>
                </CardContent>
              </SettingsGroup>

              {/* 积分逐日采样：趋势图的数据来源（用量页「每日积分」那张图）。
                  与签到分开摆 —— 采样是**只读 GET**，不是替用户去领东西。 */}
              <SettingsGroup
                id="loomy-settings-sample"
                title={t("loomy.page.settings.schedule.sampleGroup")}
              >
                <CardContent className="space-y-0 p-0">
                  <SettingsFieldRow
                    label={t("loomy.page.settings.schedule.sampleEnable")}
                    description={t("loomy.page.settings.schedule.sampleDesc")}
                  >
                    <Switch
                      checked={schedule?.loomy_points_sample_enabled ?? false}
                      disabled={!schedule || scheduleSaving}
                      onCheckedChange={(checked) =>
                        void saveSchedule({ loomy_points_sample_enabled: checked })
                      }
                      aria-label={t("loomy.page.settings.schedule.sampleEnable")}
                    />
                  </SettingsFieldRow>
                  <SettingsFieldRow
                    label={t("loomy.page.settings.schedule.sampleTimesLabel")}
                    description={t("loomy.page.settings.schedule.sampleTimesDesc")}
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
            </>
          )}

          {section === "backup" && <BackupSection />}
        </div>
      </div>
    </div>
  );
}
