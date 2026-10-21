/**
 * 聚合页的 **WorkBuddy 程序位切换**（issue #50）。
 *
 * ## 为什么单独一层
 *
 * 账号页（`AccountsPage`）里四个程序位各有一段切换实现（CLI / IDE / VS Code /
 * JetBrains），且**都要先走统一的重启确认框**（IDE 系要重启、CLI 会话会中断）。
 * 聚合页要能点同一排胶囊 ⇒ 这套流程得有一份**共享实现**，否则两处会漂：
 * 最容易漂的是「确认框的默认勾选」与「取消勾选后不得说『并重启』」——
 * 那是假承诺（§3.5），漂了就会出现「说重启了其实没重启」。
 *
 * ## 四条程序的差别（**不是同一个命令**）
 *
 * | 程序位 | 命令 | 要重启？ | 特殊文案 |
 * |:--|:--|:--|:--|
 * | CLI | `switch_codebuddy_cli_account` | ❌（会话会中断） | 不渲染「自动重开」勾选 |
 * | IDE | `switch_codebuddy_cn_ide_account` | ✅ | — |
 * | VS Code | `switch_vscode_codebuddy_plugin_account` | ✅ | — |
 * | JetBrains | `switch_jetbrains_codebuddy_plugin_account` | ✅ | — |
 *
 * ⚠️ 「需重启」的三个**默认勾选「自动重开」**，且**必须**显式提示未保存文件风险。
 */

import { useCallback, useState } from "react";
import { toast } from "sonner";

import * as api from "@/lib/api";
import { useT } from "@/lib/i18n";
import { aggregateDisplayName, type AggregateAccount } from "@/lib/aggregate-accounts";

/** 四个程序位。 */
export type WbProgramSlot = "cli" | "ide" | "vscode" | "jetbrains";

export interface AggregateWbSwitcher {
  /** 请求切换（**先弹确认框**，不直接执行）。 */
  request: (slot: WbProgramSlot, account: AggregateAccount) => void;
  /** 正在切换中的账号 id（用于把胶囊标成「切换中」）。 */
  busyAccountId: string | null;
  /** 确认框的 props（页面渲染 `RestartConfirmDialog`）。 */
  confirmProps: {
    open: boolean;
    clientLabel: string;
    isCli: boolean;
    onConfirm: (restart: boolean) => void;
    onOpenChange: (open: boolean) => void;
  };
}

/**
 * 四个程序位的切换（聚合页用）。
 *
 * @param onSwitched 切换成功后的回调（页面据此重取程序位状态与账号列表）。
 */
export function useAggregateWbSwitcher(onSwitched?: () => void): AggregateWbSwitcher {
  const t = useT();
  const [pending, setPending] = useState<{ slot: WbProgramSlot; account: AggregateAccount } | null>(null);
  const [busyAccountId, setBusyAccountId] = useState<string | null>(null);

  const request = useCallback((slot: WbProgramSlot, account: AggregateAccount) => {
    setPending({ slot, account });
  }, []);

  const clientLabel = useCallback(
    (slot: WbProgramSlot) =>
      slot === "cli"
        ? t("wbAccounts.program.codebuddyCli")
        : slot === "ide"
          ? t("wbAccounts.program.codebuddyCnIde")
          : slot === "vscode"
            ? t("wbAccounts.program.vscodeCodebuddyPlugin")
            : t("wbAccounts.program.jetbrainsCodebuddyPlugin"),
    [t],
  );

  const onConfirm = useCallback(
    (restart: boolean) => {
      const target = pending;
      setPending(null);
      if (!target || target.account.platform !== "workbuddy") return;
      const id = target.account.data.id;
      setBusyAccountId(id);
      const label = aggregateDisplayName(target.account);
      const toastId = toast.loading(
        target.slot === "cli"
          ? t("wbAccounts.toast.switchCliLoading")
          : target.slot === "ide"
            ? t("wbAccounts.toast.switchIdeLoading")
            : t("aggregate.toast.switching"),
        { description: label },
      );
      void (async () => {
        try {
          if (target.slot === "cli") {
            await api.switchCodebuddyCliAccount(id);
          } else if (target.slot === "ide") {
            await api.switchCodebuddyCnIdeAccount(id, restart);
          } else if (target.slot === "vscode") {
            await api.switchVscodeCodebuddyPluginAccount(id, restart);
          } else {
            await api.switchJetbrainsCodebuddyPluginAccount(id, restart);
          }
          toast.success(t("wbAccounts.toast.configUpdated"), { id: toastId, description: label });
          onSwitched?.();
        } catch (error) {
          toast.error(api.asError(error), { id: toastId });
        } finally {
          setBusyAccountId(null);
        }
      })();
    },
    [onSwitched, pending, t],
  );

  return {
    request,
    busyAccountId,
    confirmProps: {
      open: pending !== null,
      clientLabel: clientLabel(pending?.slot ?? "ide"),
      isCli: pending?.slot === "cli",
      onConfirm,
      onOpenChange: (open) => {
        if (!open) setPending(null);
      },
    },
  };
}
