"use client";

import { Gauge, KeyRound, Loader2, RefreshCw, Sparkles, type LucideIcon } from "lucide-react";

import { DropdownMenuItem } from "@/components/ui/dropdown-menu";
import { ACCOUNT_REFRESH_KINDS, type AccountRefreshKind } from "@/lib/account-refresh";
import { useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";

/**
 * 卡片「更多」菜单里三平台共用的两块积木：**分组小标题** 与 **四项刷新分组**。
 *
 * ## 为什么要抽出来
 *
 * WorkBuddy / Trae / Qoder 三张卡片的菜单结构一样、图标与文案也一样。各写一份的下场
 * 在本仓已经出现过多次（`gateway-snippets`、抽屉壳）：三处会立刻在文案、禁用逻辑、
 * 进行中态上漂移。所以「四项刷新」只在这里定义一次，卡片只负责把它摆进菜单。
 *
 * ## 为什么是四项，而不是一个「刷新」
 *
 * 见 `lib/account-refresh.ts` 的文件头：四类数据来自不同层（上游 / 本机日志 / 本机签到 /
 * 本机凭据），快慢与失败原因都不同；合成一个动作时慢的拖住快的，且**失败原因分不清**。
 *
 * 选中后菜单由 Radix 关闭（默认行为）—— 我们**不**为了显示转圈而把菜单钉住：
 * 用户点完就该回到列表看结果，转圈与「已更新」由 toast + 卡片上的状态反映。
 * 卡片会在菜单再次打开时用 `refreshingKinds` 把还在跑的那一项标出来。
 */
export function MenuGroupLabel({ children, className }: { children: React.ReactNode; className?: string }) {
  /* 不用 `DropdownMenuLabel`：本仓的 `ui/dropdown-menu` 没有导出它（只有 Menu / Item /
     Separator / Trigger / Content），为了一个 `<div>` 去改原语不划算。 */
  return (
    <div
      role="presentation"
      className={cn("px-2 pt-1 pb-0.5 text-[10.5px] font-normal tracking-wide text-muted-foreground", className)}
    >
      {children}
    </div>
  );
}

/* `as const` 是为了让 `labelKey` 保留**字面量类型** —— `t()` 只接受已知的键（编译期校验），
   写宽成 `string` 会直接报错（这正是防「拼错 key 静默回退原文」的闸门）。 */
const REFRESH_ITEMS = [
  { kind: "credits", icon: RefreshCw, labelKey: "shared.cardMenu.refresh.credits" },
  { kind: "tokens", icon: Gauge, labelKey: "shared.cardMenu.refresh.tokens" },
  { kind: "checkin", icon: Sparkles, labelKey: "shared.cardMenu.refresh.checkin" },
  { kind: "credential", icon: KeyRound, labelKey: "shared.cardMenu.refresh.credential" },
] as const satisfies ReadonlyArray<{ kind: AccountRefreshKind; icon: LucideIcon; labelKey: string }>;

export function RefreshMenuGroup({
  refreshingKinds,
  kinds = ACCOUNT_REFRESH_KINDS,
  onPick,
}: {
  /** 本账号正在进行中的刷新项。 */
  refreshingKinds?: readonly AccountRefreshKind[];
  /**
   * 本平台**真的能刷**的项（默认四项全给）。
   *
   * ⚠️ 这是「不许假装」的闸门：Trae / Qoder 没有「今日 Token」与「签到状态」的**只读重取**接口，
   * 摆一个点了只会重读缓存列表的菜单项，比不摆更坏 —— 用户会以为刷过了。
   */
  kinds?: readonly AccountRefreshKind[];
  onPick: (kind: AccountRefreshKind) => void;
}) {
  const t = useT();
  const items = REFRESH_ITEMS.filter((item) => kinds.includes(item.kind));
  if (items.length === 0) return null;
  return (
    <>
      <MenuGroupLabel>{t("shared.cardMenu.refreshGroup")}</MenuGroupLabel>
      {items.map(({ kind, icon: Icon, labelKey }) => {
        const busy = refreshingKinds?.includes(kind) ?? false;
        return (
          <DropdownMenuItem key={kind} disabled={busy} onSelect={() => onPick(kind)}>
            {busy ? <Loader2 className="animate-spin" /> : <Icon />}
            <span className="min-w-0 flex-1 truncate">{t(labelKey)}</span>
            {busy ? (
              <span className="ml-2 shrink-0 text-[10.5px] text-muted-foreground">
                {t("shared.cardMenu.busy")}
              </span>
            ) : null}
          </DropdownMenuItem>
        );
      })}
    </>
  );
}

/** 顺序对外暴露一份，供调用方做类型收窄（不必再 import 数组常量）。 */
export { ACCOUNT_REFRESH_KINDS };
