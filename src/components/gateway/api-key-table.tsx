import { useRef, useState } from "react";
import { Check, Copy, KeyRound, Loader2, RotateCcw, Trash2 } from "lucide-react";
import { toast } from "sonner";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { DemoAction } from "@/components/demo-action";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import * as api from "@/lib/api";
import { copyText } from "@/lib/clipboard";
import { formatDate } from "@/lib/format";
import { useT } from "@/lib/i18n";
import { REGIONS, regionDescriptor } from "@/lib/region";
import { cn } from "@/lib/utils";
import type { ApiKeyRecord, Region } from "@/lib/types";
import { useGatewayStore } from "@/stores/gateway";

/* P3：本地 `formatDate` 已移到 `@/lib/format` —— 它与 Trae 侧各写一份且**已经漂移**：
   本版把时间戳 `0` 渲染成 `01-01 08:00` 这种**假时间**，共享版会返回 `—`。 */

/** API Key 列表 + 创建对话框 + 一次性明文展示 + 吊销 / 删除（P0-3）。 */
export function ApiKeyTable({ className }: { className?: string }) {
  const t = useT();
  const keys = useGatewayStore((s) => s.keys);
  const createKey = useGatewayStore((s) => s.createKey);
  const revokeKey = useGatewayStore((s) => s.revokeKey);
  const restoreKey = useGatewayStore((s) => s.restoreKey);
  const deleteKey = useGatewayStore((s) => s.deleteKey);

  const [createOpen, setCreateOpen] = useState(false);
  const [name, setName] = useState("");
  const [region, setRegion] = useState<Region>("cn");
  const [creating, setCreating] = useState(false);
  const [plaintext, setPlaintext] = useState<{ value: string; name: string } | null>(null);
  /**
   * 「刚复制成功」态（驱动复制按钮 `Copy` → `Check`）。
   *
   * 它**只影响图标与文案，不影响可点性** —— 明文只显示这一次，用户可能需要反复复制
   * （粘到多个客户端、或先粘进密码管理器），所以按钮**永远保持可点**。
   * 用 ref 存定时器：连点两次时旧定时器不会把新一次的「已复制」提前复位。
   */
  const [copied, setCopied] = useState(false);
  const copyTimer = useRef<number | null>(null);
  const [revokeTarget, setRevokeTarget] = useState<ApiKeyRecord | null>(null);
  /**
   * ★ 恢复目标：与 `revokeTarget` **互逆**。已吊销的行必须同时给出「恢复」与「删除」
   * 两条出口 —— 吊销是可逆动作（后端只置 `revoked_at`、明文未变），
   * 若只留「删除」，用户想撤销一次误吊销就只能删掉重建、再把所有客户端配置改一遍。
   */
  const [restoreTarget, setRestoreTarget] = useState<ApiKeyRecord | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<ApiKeyRecord | null>(null);
  const [busy, setBusy] = useState(false);

  function openCreate() {
    setName("");
    setRegion("cn");
    setCreateOpen(true);
  }

  async function onCreate() {
    const trimmed = name.trim();
    if (!trimmed) {
      toast.error(t("wbStats.gateway.nameRequired"));
      return;
    }
    setCreating(true);
    try {
      const result = await createKey(trimmed, region);
      const value = result.key;
      if (!value) {
        toast.error(t("wbStats.gateway.createdNoPlaintext"));
        return;
      }
      setCreateOpen(false);
      setCopied(false);
      setPlaintext({ value, name: trimmed });
    } catch (e) {
      toast.error(t("wbStats.gateway.createFail"), { description: api.asError(e) });
    } finally {
      setCreating(false);
    }
  }

  /** 复制明文。**可反复调用**：成功后把按钮切成「已复制」，1.6s 后复位。 */
  async function onCopyPlaintext() {
    if (!plaintext) return;
    await copyText(plaintext.value, t("wbStats.gateway.keyCopied"));
    setCopied(true);
    if (copyTimer.current !== null) window.clearTimeout(copyTimer.current);
    copyTimer.current = window.setTimeout(() => setCopied(false), 1600);
  }

  /** 关闭明文对话框。**唯一的关闭路径**（遮罩 / ESC 已被拦截，见下方 DialogContent）。 */
  function closePlaintext() {
    if (copyTimer.current !== null) window.clearTimeout(copyTimer.current);
    copyTimer.current = null;
    setCopied(false);
    setPlaintext(null);
  }

  async function confirmRevoke() {
    if (!revokeTarget) return;
    setBusy(true);
    try {
      await revokeKey(revokeTarget.id);
      toast.success(t("wbStats.gateway.revokedToast"), { description: revokeTarget.name });
      setRevokeTarget(null);
    } catch (e) {
      toast.error(t("wbStats.gateway.revokeFail"), { description: api.asError(e) });
    } finally {
      setBusy(false);
    }
  }

  async function confirmRestore() {
    if (!restoreTarget) return;
    setBusy(true);
    try {
      await restoreKey(restoreTarget.id);
      toast.success(t("wbStats.gateway.restoredToast"), { description: restoreTarget.name });
      setRestoreTarget(null);
    } catch (e) {
      toast.error(t("wbStats.gateway.restoreFail"), { description: api.asError(e) });
    } finally {
      setBusy(false);
    }
  }

  async function confirmDelete() {
    if (!deleteTarget) return;
    setBusy(true);
    try {
      await deleteKey(deleteTarget.id);
      toast.success(t("wbStats.gateway.deletedToast"), { description: deleteTarget.name });
      setDeleteTarget(null);
    } catch (e) {
      toast.error(t("wbStats.gateway.deleteFail"), { description: api.asError(e) });
    } finally {
      setBusy(false);
    }
  }

  return (
    <Card className={cn("gap-0 py-0", className)}>
      <div className="flex items-center justify-between gap-3 border-b border-border/60 px-5 py-3">
        <span className="text-sm font-semibold">API Key</span>
        <DemoAction>
          <Button size="sm" onClick={openCreate}>
            <KeyRound />
            {t("wbStats.gateway.createKey")}
          </Button>
        </DemoAction>
      </div>

      <div className="px-5 py-3">
        {keys.length === 0 ? (
          /* ★ 空态用全应用唯一的 `EmptyState`（§3.6）—— 与 Trae 侧 `trae-api-key-table.tsx`
             同一形态。原来这里是「一行灰字」，两平台的 Key 表空态长得不一样。 */
          <EmptyState icon={KeyRound} title={t("wbStats.gateway.noKeys")} className="py-4" />
        ) : (
          <div className="min-w-0 overflow-x-auto">
            <table className="w-full min-w-[560px] text-left text-sm">
              <thead>
                <tr className="text-xs text-muted-foreground">
                  <th className="pb-2 pr-4 font-medium">{t("wbStats.table.name")}</th>
                  <th className="pb-2 pr-4 font-medium">{t("wbStats.table.version")}</th>
                  <th className="pb-2 pr-4 font-medium">{t("wbStats.table.prefix")}</th>
                  <th className="pb-2 pr-4 font-medium">{t("wbStats.table.createdAt")}</th>
                  <th className="pb-2 pr-4 font-medium">{t("wbStats.table.lastUsed")}</th>
                  <th className="pb-2 pr-4 font-medium">{t("wbStats.table.status")}</th>
                  <th className="pb-2 font-medium">{t("wbStats.table.actions")}</th>
                </tr>
              </thead>
              <tbody>
                {keys.map((key) => {
                  const revoked = key.revoked;
                  return (
                    <tr key={key.id} className="border-t border-border/60">
                      <td className="py-2 pr-4 font-medium">{key.name}</td>
                      <td className="py-2 pr-4">
                        <Badge variant="secondary" className="rounded-md">
                          {regionDescriptor(key.region).versionLabel}
                        </Badge>
                      </td>
                      {/* ★ E3：掩码补上尾部 4 位（`prefix••••••••suffix`）——
                      用户靠它**确认是哪一把**。旧台账无 suffix ⇒ 只显示前缀 + 省略号。 */}
                      <td className="py-2 pr-4 font-mono text-xs text-muted-foreground">
                        {key.suffix
                          ? `${key.prefix}••••••••${key.suffix}`
                          : `${key.prefix}…`}
                      </td>
                      <td className="py-2 pr-4 text-xs text-muted-foreground">{formatDate(key.createdAt)}</td>
                      {/* ★ 与 Trae 侧同列同序：`lastUsedAt` 字段后端一直有（`ApiKeyRecord`），
                          此前 WB 表**没有渲染**它 —— 用户看不出哪把 Key 还在被用。 */}
                      <td className="py-2 pr-4 text-xs text-muted-foreground">
                        {key.lastUsedAt ? formatDate(key.lastUsedAt) : t("wbStats.table.neverUsed")}
                      </td>
                      <td className="py-2 pr-4">
                        {revoked ? (
                          <Badge variant="secondary" className="rounded-md text-muted-foreground">
                            {t("wbStats.gateway.revoked")}
                          </Badge>
                        ) : (
                          <Badge variant="success" className="rounded-md">
                            {t("wbStats.gateway.enabled")}
                          </Badge>
                        )}
                      </td>
                      <td className="py-2">
                        {/* ★ 已吊销 ⇒ **两个出口都在**：恢复（可逆，回到「启用」）+ 删除（不可逆，清记录）。
                            「吊销」与「启用」是同一把 Key 的两种状态，恢复不需要重建 Key、
                            也不需要改动任何客户端的配置。 */}
                        {revoked ? (
                          <div className="flex items-center gap-1">
                            <Button variant="ghost" size="sm" onClick={() => setRestoreTarget(key)}>
                              <RotateCcw />
                              {t("wbStats.gateway.restore")}
                            </Button>
                            <Button variant="ghost" size="sm" className="text-destructive hover:text-destructive" onClick={() => setDeleteTarget(key)}>
                              <Trash2 />
                              {t("wbStats.gateway.delete")}
                            </Button>
                          </div>
                        ) : (
                          <Button variant="ghost" size="sm" onClick={() => setRevokeTarget(key)}>
                            {t("wbStats.gateway.revoke")}
                          </Button>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* 创建对话框 */}
      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("wbStats.gateway.createTitle")}</DialogTitle>
            <DialogDescription>{t("wbStats.gateway.createDesc")}</DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="key-name">{t("wbStats.gateway.nameLabel")}</Label>
              <Input
                id="key-name"
                value={name}
                onChange={(event) => setName(event.target.value)}
                placeholder={t("wbStats.gateway.namePlaceholder")}
                spellCheck={false}
                autoComplete="off"
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="key-region">{t("wbStats.gateway.regionLabel")}</Label>
              <Select value={region} onValueChange={(value) => setRegion(value as Region)}>
                <SelectTrigger id="key-region" className="w-full" aria-label={t("wbStats.gateway.regionAria")}>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {REGIONS.map((r) => (
                    <SelectItem key={r} value={r}>
                      {regionDescriptor(r).gatewayLabel}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setCreateOpen(false)} disabled={creating}>
              {t("wbStats.gateway.cancel")}
            </Button>
            <Button onClick={() => void onCreate()} disabled={creating}>
              {creating && <Loader2 className="animate-spin" />}
              {t("wbStats.gateway.create")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 一次性明文展示。
          ★ 2026-10-13：遮罩点击与 ESC **一律拦截** —— 明文不可复原（库里只有 sha256），
            误关一次就得吊销重建、并把所有客户端配置改一遍。关闭只剩下面那个显式按钮。
            复制按钮**不禁用**：用户可能要反复复制（多个客户端 / 先存进密码管理器）。 */}
      <Dialog open={plaintext !== null} onOpenChange={(open) => !open && closePlaintext()}>
        <DialogContent
          className="sm:max-w-md"
          onInteractOutside={(event) => event.preventDefault()}
          onEscapeKeyDown={(event) => event.preventDefault()}
        >
          <DialogHeader>
            <DialogTitle>{t("wbStats.gateway.createdTitle")}</DialogTitle>
            <DialogDescription>{t("wbStats.gateway.createdDesc")}</DialogDescription>
          </DialogHeader>
          {plaintext && (
            <div className="flex items-center gap-2 rounded-lg border border-border bg-muted/40 px-3 py-2.5">
              {/* `select-all`：点一下即全选，手动 Ctrl+C 也能用（剪贴板 API 被拒时的兜底）。 */}
              <code className="min-w-0 flex-1 select-all break-all font-mono text-xs">{plaintext.value}</code>
              <Button
                variant="outline"
                size="sm"
                data-act="gw-key-copy"
                onClick={() => void onCopyPlaintext()}
              >
                {copied ? <Check /> : <Copy />}
                {copied ? t("shared.clipboard.copied") : t("wbStats.gateway.copy")}
              </Button>
            </div>
          )}
          <DialogFooter>
            <Button onClick={closePlaintext}>{t("wbStats.gateway.savedClose")}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 吊销确认 */}
      <Dialog open={revokeTarget !== null} onOpenChange={(open) => !open && setRevokeTarget(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("wbStats.gateway.revokeTitle")}</DialogTitle>
            <DialogDescription>
              {t("wbStats.gateway.revokeDesc", { name: revokeTarget?.name ?? "" })}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setRevokeTarget(null)} disabled={busy}>
              {t("wbStats.gateway.cancel")}
            </Button>
            <Button variant="destructive" onClick={() => void confirmRevoke()} disabled={busy}>
              {t("wbStats.gateway.revoke")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 恢复确认（可逆动作 ⇒ 主按钮非 destructive） */}
      <Dialog open={restoreTarget !== null} onOpenChange={(open) => !open && setRestoreTarget(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("wbStats.gateway.restoreTitle")}</DialogTitle>
            <DialogDescription>
              {t("wbStats.gateway.restoreDesc", { name: restoreTarget?.name ?? "" })}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setRestoreTarget(null)} disabled={busy}>
              {t("wbStats.gateway.cancel")}
            </Button>
            <Button onClick={() => void confirmRestore()} disabled={busy}>
              {t("wbStats.gateway.restore")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 删除确认 */}
      <Dialog open={deleteTarget !== null} onOpenChange={(open) => !open && setDeleteTarget(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("wbStats.gateway.deleteTitle")}</DialogTitle>
            <DialogDescription>{t("wbStats.gateway.deleteDesc", { name: deleteTarget?.name ?? "" })}</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDeleteTarget(null)} disabled={busy}>
              {t("wbStats.gateway.cancel")}
            </Button>
            <Button variant="destructive" onClick={() => void confirmDelete()} disabled={busy}>
              {t("wbStats.gateway.delete")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Card>
  );
}
