import { useState } from "react";
import { useLocation, useSearchParams } from "react-router-dom";

import { AppSettingsGroups } from "@/components/app-settings";
import { BackupSection } from "@/components/settings/backup-section";
import {
  parseSettingsSubnav,
  SettingsSubnav,
  type SettingsSubnavId,
} from "@/components/settings/settings-subnav";
import { ScopeChip } from "@/components/scope-chip";
import { platformOfPath } from "@/lib/platforms/registry";
import { scopeOf } from "@/lib/scope";
import { useScope } from "@/lib/use-scope";

/**
 * Loomy 设置页。
 *
 * 与 Qoder 设置页同款收敛：**应用级设置与备份是跨平台共享面**
 * （`AppSettingsGroups` / `BackupSection` 内部按 `Scope::all()` 自动覆盖 `loomy-cn`）。
 * Loomy 无排程任务 / 无本地客户端设置 ⇒ 子导航只留「通用 / 备份」两项。
 */
export default function LoomySettingsPage() {
  const location = useLocation();
  const [params] = useSearchParams();
  const product = platformOfPath(location.pathname);
  const [region] = useScope(product);
  const scope = scopeOf(product, region);
  const [section, setSection] = useState<SettingsSubnavId>(
    parseSettingsSubnav(params.get("section")),
  );

  return (
    <div className="flex flex-col gap-4 p-6">
      <header className="flex items-center gap-2">
        <h1 className="text-lg font-semibold">Loomy 设置</h1>
        <ScopeChip scope={scope} />
      </header>
      <SettingsSubnav
        items={[
          { id: "general", labelKey: "settings.subnav.general" },
          { id: "backup", labelKey: "settings.subnav.backup" },
        ]}
        active={section}
        onSelect={setSection}
      />
      {section === "general" && <AppSettingsGroups />}
      {section === "backup" && <BackupSection />}
    </div>
  );
}
