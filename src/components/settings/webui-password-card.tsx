import { useCallback, useEffect, useState } from "react";
import { Loader2, LogOut, Save } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { SettingsFieldRow, SettingsGroup } from "@/components/settings-primitives";
import { isWebui } from "@/lib/api";
import { useT } from "@/lib/i18n";
import { changePassword, fetchAuthState, logout, redirectToLogin } from "@/lib/webui-auth";

/**
 * 「WebUI 密码」卡。
 *
 * ## 为什么只在浏览器形态渲染
 *
 * 桌面端（Tauri）走 `invoke`，压根没有 HTTP 管理面，也就不存在「访问密码」。
 * 在桌面端渲染这张卡会让用户以为它保护了什么 —— 它什么也没保护。
 *
 * ## 为什么**不**放在「权限与诊断」之外
 *
 * 设置子导航被门禁钉死为 9 项（`SETTINGS_SUBNAV` 的长度在三处被断言），
 * 加第 10 项会同时破坏作用域注册表与快捷键序号。因此密码卡作为
 * 「权限与诊断」分区里的一张卡存在 —— 它本来也属于「谁能进来」这一类。
 */
export function WebuiPasswordCard() {
  const t = useT();
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [confirm, setConfirm] = useState("");
  const [saving, setSaving] = useState(false);
  const [signingOut, setSigningOut] = useState(false);

  const load = useCallback(async () => {
    try {
      const state = await fetchAuthState();
      setEnabled(state.authRequired);
    } catch {
      // 读不到就按「未知」处理（`null`），不猜成「未启用」——
      // 猜错会让用户以为密码没了，然后去重设一个其实已存在的密码。
      setEnabled(null);
    }
  }, []);

  useEffect(() => {
    if (!isWebui()) return;
    void load();
  }, [load]);

  if (!isWebui()) return null;

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    if (saving) return;
    if (next.length < 8) {
      toast.error(t("shared.auth.passwordTooShort"));
      return;
    }
    if (next !== confirm) {
      toast.error(t("shared.auth.passwordMismatch"));
      return;
    }
    if (enabled && !current) {
      toast.error(t("shared.auth.currentPasswordRequired"));
      return;
    }
    setSaving(true);
    try {
      await changePassword({ oldPassword: enabled ? current : undefined, newPassword: next });
      // 服务端会撤销**全部**会话（含当前这条），所以这里必须回登录页，
      // 否则界面看起来「还在」，下一次请求才 401 —— 那更难理解。
      toast.success(t("shared.auth.passwordSaved"));
      setCurrent("");
      setNext("");
      setConfirm("");
      // 延迟一拍再跳：`redirectToLogin` 是整页导航（`location.assign`），
      // 立刻执行会让上面那条 toast 只闪一帧 —— 而「其他设备要重新登录」
      // 正是最容易被忽略、事后最困惑的一条信息。
      window.setTimeout(redirectToLogin, 1200);
    } catch (reason) {
      toast.error(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setSaving(false);
    }
  }

  async function signOut() {
    if (signingOut) return;
    setSigningOut(true);
    try {
      await logout();
      toast.success(t("shared.auth.logoutDone"));
      redirectToLogin();
    } catch (reason) {
      toast.error(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setSigningOut(false);
    }
  }

  return (
    <SettingsGroup id="settings-webui-password" title={t("shared.auth.passwordTitle")}>
      <CardContent className="space-y-0 p-0">
        <p className="border-b border-border/60 bg-muted/25 px-4 py-3 text-xs leading-5 text-muted-foreground">
          {t("shared.auth.passwordDesc")}
          {enabled === false ? ` ${t("shared.auth.notEnabled")}` : null}
        </p>

        <form onSubmit={submit}>
          {enabled ? (
            <SettingsFieldRow
              label={t("shared.auth.currentPassword")}
              htmlFor="webui-current-password"
              stacked
            >
              <Input
                id="webui-current-password"
                type="password"
                autoComplete="current-password"
                className="max-w-[320px]"
                value={current}
                disabled={saving}
                onChange={(event) => setCurrent(event.target.value)}
              />
            </SettingsFieldRow>
          ) : null}

          <SettingsFieldRow
            label={t("shared.auth.newPassword")}
            htmlFor="webui-new-password"
            stacked
          >
            <Input
              id="webui-new-password"
              type="password"
              autoComplete="new-password"
              className="max-w-[320px]"
              value={next}
              disabled={saving}
              onChange={(event) => setNext(event.target.value)}
            />
          </SettingsFieldRow>

          <SettingsFieldRow
            label={t("shared.auth.confirmPassword")}
            htmlFor="webui-confirm-password"
            stacked
          >
            <Input
              id="webui-confirm-password"
              type="password"
              autoComplete="new-password"
              className="max-w-[320px]"
              value={confirm}
              disabled={saving}
              onChange={(event) => setConfirm(event.target.value)}
            />
          </SettingsFieldRow>

          <SettingsFieldRow
            label={t("shared.auth.logout")}
            description={t("shared.auth.logoutDesc")}
            operational
          >
            <div className="flex items-center gap-2">
              <Button type="button" size="sm" variant="outline" onClick={() => void signOut()} disabled={signingOut}>
                {signingOut ? <Loader2 className="animate-spin" /> : <LogOut />}
                {t("shared.auth.logout")}
              </Button>
              <Button type="submit" size="sm" disabled={saving || !next || !confirm}>
                {saving ? <Loader2 className="animate-spin" /> : <Save />}
                {t("shared.auth.savePassword")}
              </Button>
            </div>
          </SettingsFieldRow>
        </form>
      </CardContent>
    </SettingsGroup>
  );
}
