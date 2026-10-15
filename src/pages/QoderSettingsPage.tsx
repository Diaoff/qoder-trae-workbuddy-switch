import { useEffect, useState } from "react";
import { useLocation, useSearchParams } from "react-router-dom";
import { MonitorSmartphone, ShieldAlert } from "lucide-react";

import { AppSettingsGroups } from "@/components/app-settings";
import { QoderLocalSourceRow } from "@/components/qoder/qoder-local-source-row";
import { BackupSection } from "@/components/settings/backup-section";
import {
  parseSettingsSubnav,
  SettingsSubnav,
  SETTINGS_SUBNAV,
  type SettingsSubnavId,
} from "@/components/settings/settings-subnav";
import { ScopeChip, scopeLabel } from "@/components/scope-chip";
import { Card } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import * as api from "@/lib/api";
import type { QoderLocalSourcesView } from "@/lib/api";
import { useT } from "@/lib/i18n";
import { platformOfPath } from "@/lib/platforms/registry";
import { scopeOf } from "@/lib/scope";
import { useScope } from "@/lib/use-scope";

/**
 * Qoder 设置页。
 *
 * ## 与另两个设置页的关系
 *
 * 子导航**共用同一张表**（`SETTINGS_SUBNAV`，PLAN §3.9 / D12），版式同构：
 * `data-split-layout` + `parseSettingsSubnav` 的 URL 承载 + 主区 16px 间距。
 *
 * ## ★ 为什么只列「已有内容」的分区
 *
 * 表里 9 项是按 WB / Trae 的内容收敛出来的；Qoder 侧目前**只有三处有真内容**：
 * 通用（应用级设置）、客户端检测（本机凭据四态探测）、备份与恢复。
 * 其余六项（账号 / 签到 / 网关 / 计划任务 / 权限与诊断 / 日志）在 Qoder 侧**没有设置项**
 * —— 摆上去就是 §1.7 说的**假控件**（点了只有一句「尚未接入」，比不摆更糟）。
 * 因此用 `SettingsSubnav` 的 `items` 收窄，而不是复制一份表（复制即漂移的种子）。
 * 对应功能落地后，把它的 id 加进 {@link QODER_SETTINGS_SECTIONS} 即可。
 */
const QODER_SECTION_IDS: readonly SettingsSubnavId[] = ["general", "client", "backup"];

/** 收窄后的子导航项（顺序取自共享表，不另排）。 */
const QODER_SETTINGS_SECTIONS = SETTINGS_SUBNAV.filter((item) => QODER_SECTION_IDS.includes(item.id));

export default function QoderSettingsPage() {
  const t = useT();
  const location = useLocation();
  const product = platformOfPath(location.pathname);
  const [region] = useScope(product);
  const scope = scopeOf(product, region);

  const [searchParams, setSearchParams] = useSearchParams();
  /**
   * ★ 初始分区从 URL 读，且**按本页的分区集校验**：`?section=gateway` 这类
   * （表里有、本页没有）必须 fail-closed 回 `general` —— 否则会渲染出一个空的主区，
   * 用户看到「点了没反应」。
   */
  const [section, setSection] = useState<SettingsSubnavId>(() => {
    const parsed = parseSettingsSubnav(searchParams.get("section"));
    return QODER_SECTION_IDS.includes(parsed) ? parsed : "general";
  });

  const [sources, setSources] = useState<QoderLocalSourcesView | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    let cancelled = false;
    setError("");
    api
      .getQoderLocalSources(scope)
      .then((view) => {
        if (!cancelled) setSources(view);
      })
      .catch((err) => {
        if (!cancelled) setError(api.asError(err));
      });
    return () => {
      cancelled = true;
    };
  }, [scope]);

  /** 切换分区：**同时写 URL**（与另两个设置页同构 —— 位置真值在 URL）。 */
  function goSection(id: SettingsSubnavId) {
    setSection(id);
    const next = new URLSearchParams(searchParams);
    next.set("section", id);
    setSearchParams(next, { replace: true });
  }

  return (
    <div className="mx-auto min-w-0 w-full max-w-[1240px] px-7 pt-[26px] pb-[60px]">
      <header className="mb-5 flex items-start gap-4">
        <div className="min-w-0">
          <h1 className="text-[28px] font-[640] tracking-[-0.02em]">{t("qoder.page.settings.title")}</h1>
          <p className="mt-1.5 text-[12.5px] text-muted-foreground">
            {t("pageHead.settings", { scope: scopeLabel(t, scope) })}
          </p>
        </div>
        <div className="grow" />
        <ScopeChip scope={scope} />
      </header>

      {/* 两栏容器：方向 / 断点整组在 `index.css` 的 `[data-split-layout]` 里。 */}
      <div data-split-layout className="min-w-0">
        <SettingsSubnav items={QODER_SETTINGS_SECTIONS} active={section} onSelect={goSection} />

        <div className="min-w-0 flex-1 space-y-4">
          {section === "general" && <AppSettingsGroups />}

          {section === "client" && (
            <Card className="gap-0 py-0">
              <div className="flex flex-wrap items-start gap-3 px-4 py-3.5">
                <MonitorSmartphone className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
                <div className="min-w-0 grow">
                  <div className="text-[13px] font-[550]">{t("qoder.page.settings.section.client")}</div>
                  <div className="mt-0.5 text-[11.5px] text-muted-foreground">
                    {t("qoder.page.settings.client.description")}
                  </div>
                </div>
              </div>
              {error ? (
                <div className="flex items-start gap-2.5 border-t px-4 py-3 text-[12px] text-destructive">
                  <ShieldAlert className="mt-0.5 size-4 shrink-0" />
                  <div className="min-w-0">{t("qoder.page.settings.client.error", { error })}</div>
                </div>
              ) : sources ? (
                <div className="border-t">
                  {sources.sources.map((source) => (
                    <QoderLocalSourceRow key={source.kind} source={source} />
                  ))}
                </div>
              ) : (
                <div className="space-y-2 border-t px-4 py-3">
                  <Skeleton className="h-3 w-40" />
                  <Skeleton className="h-3 w-64" />
                </div>
              )}
            </Card>
          )}

          {section === "backup" && <BackupSection />}

          {/* 收窄子导航的**理由**写在界面上 —— 用户看不到「少了几项」却不知道原因。 */}
          <p className="text-[11.5px] text-muted-foreground">{t("qoder.page.settings.filtered")}</p>
        </div>
      </div>
    </div>
  );
}
