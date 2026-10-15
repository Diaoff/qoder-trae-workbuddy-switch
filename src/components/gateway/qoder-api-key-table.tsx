import { useCallback, useEffect, useRef, useState } from "react";
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
import { Skeleton } from "@/components/ui/skeleton";
import * as api from "@/lib/api";
import { copyText } from "@/lib/clipboard";
import { formatDate } from "@/lib/format";
import { useT } from "@/lib/i18n";
import { normalizeQoderApiKeys, type QoderApiKeyRecord } from "@/lib/qoder-gateway";
import { REGIONS, regionDescriptor } from "@/lib/region";
import { scopeOf } from "@/lib/scope";
import type { Region } from "@/lib/types";
import { cn } from "@/lib/utils";

/**
 * Qoder 多 Key 列表 + 创建 / 吊销 / 恢复 / 删除（对齐 Trae `trae-api-key-table.tsx` 骨架）。
 *
 * ## 与 Trae 版的唯一差别：归属轴是**区域**而不是程序位
 *
 * Qoder 没有程序位轴（一个区域一本账号库、一套端点），网关**按 Key 的归属区域选池**
 * ⇒ Key 的归属就是区域，共两个取值（国内版 / 国际版）。这不是「少了一个维度」，
 * 而是「这个平台的维度就是区域」：把程序位选择器搬过来会是**假控件**。
 *
 * ## 三态动作（与另两条产品线**同形**）
 *
 * **吊销**（置 `revokedAt`，可逆）→ 已吊销的行**必须同时给「恢复」与「删除」两个出口**；
 * **恢复**（清 `revokedAt`，同一把 Key 明文未变 ⇒ 客户端无需改配置）；
 * **删除**（仅已吊销可删，物理删除）。只给删除等于逼用户为一次误吊销删掉重建。
 *
 * 明文只在创建时一次性返回（`create_qoder_api_key`），列表接口永远只给脱敏前缀 + 末 4 位。
 */
export function QoderApiKeyTable({
  className,
  /** 创建时的默认归属（由页面传入当前作用域的区域；缺省 `cn` = 国内版）。 */
  defaultRegion = "cn",
  /** 数据变化（创建 / 吊销 / 恢复 / 删除）后的回调，供页面刷新网关状态里的 Key 前缀。 */
  onChanged,
}: {
  className?: string;
  defaultRegion?: Region;
  onChanged?: () => void;
}) {
  const t = useT();
  const [keys, setKeys] = useState<QoderApiKeyRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [createOpen, setCreateOpen] = useState(false);
  const [name, setName] = useState("");
  const [region, setRegion] = useState<Region>(defaultRegion);
  const [creating, setCreating] = useState(false);
  const [plaintext, setPlaintext] = useState<{ value: string; name: string } | null>(null);
  /**
   * 「刚复制成功」态（驱动 `Copy` → `Check`）。
   *
   * 与另两条产品线**同一规则**：只影响图标与文案，**不影响可点性** —— 明文只显示这一次，
   * 用户可能要反复复制，按钮永远保持可点。定时器存 ref：连点两次时旧定时器不会提前复位。
   */
  const [copied, setCopied] = useState(false);
  const copyTimer = useRef<number | null>(null);
  const [revokeTarget, setRevokeTarget] = useState<QoderApiKeyRecord | null>(null);
  const [restoreTarget, setRestoreTarget] = useState<QoderApiKeyRecord | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<QoderApiKeyRecord | null>(null);
  const [busy, setBusy] = useState(false);

  /** 拉取列表。创建 / 吊销 / 恢复 / 删除后都重新拉，保证列表与后端一致。 */
  const load = useCallback(async () => {
    setLoading(true);
    try {
      const result = await api.listQoderApiKeys();
      setKeys(normalizeQoderApiKeys(result));
    } catch (e) {
      // 演示模式 / 后端不可用：保持空列表，不清空已有数据。
      toast.error(t("qoder.gateway.key.loadFailed"), { description: api.asError(e) });
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  function openCreate() {
    setName("");
    setRegion(defaultRegion);
    setCreateOpen(true);
  }

  async function onCreate() {
    const trimmed = name.trim();
    if (!trimmed) {
      toast.error(t("qoder.gateway.key.nameRequired"));
      return;
    }
    setCreating(true);
    try {
      // Key 的归属是**区域**，但命令面统一收 `scope`（与另两条产品线同一纪律：
      // 作用域是唯一的区域载体）。这里把当前区域升成 `qoder-*` 作用域。
      const result = await api.createQoderApiKey(scopeOf("qoder", region), trimmed);
      const value = result.key;
      if (!value) {
        // 创建成功但没回明文 = 契约异常，必须显式提示而不是静默（明文不可复原）。
        toast.error(t("qoder.gateway.key.noPlaintext"));
        return;
      }
      setCreateOpen(false);
      setCopied(false);
      setPlaintext({ value, name: trimmed });
      await load();
      onChanged?.();
    } catch (e) {
      toast.error(t("qoder.gateway.key.createFailed"), { description: api.asError(e) });
    } finally {
      setCreating(false);
    }
  }

  /** 复制明文。**可反复调用**：成功后把按钮切成「已复制」，1.6s 后复位。 */
  async function onCopyPlaintext() {
    if (!plaintext) return;
    await copyText(plaintext.value, t("qoder.gateway.key.copied"));
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
      await api.revokeQoderApiKey(revokeTarget.id);
      toast.success(t("qoder.gateway.key.revoked"), { description: revokeTarget.name });
      setRevokeTarget(null);
      await load();
      onChanged?.();
    } catch (e) {
      toast.error(t("qoder.gateway.key.revokeFailed"), { description: api.asError(e) });
    } finally {
      setBusy(false);
    }
  }

  async function confirmRestore() {
    if (!restoreTarget) return;
    setBusy(true);
    try {
      await api.restoreQoderApiKey(restoreTarget.id);
      toast.success(t("qoder.gateway.key.restored"), { description: restoreTarget.name });
      setRestoreTarget(null);
      await load();
      onChanged?.();
    } catch (e) {
      toast.error(t("qoder.gateway.key.restoreFailed"), { description: api.asError(e) });
    } finally {
      setBusy(false);
    }
  }

  async function confirmDelete() {
    if (!deleteTarget) return;
    setBusy(true);
    try {
      await api.deleteQoderApiKey(deleteTarget.id);
      toast.success(t("qoder.gateway.key.deleted"), { description: deleteTarget.name });
      setDeleteTarget(null);
      await load();
      onChanged?.();
    } catch (e) {
      toast.error(t("qoder.gateway.key.deleteFailed"), { description: api.asError(e) });
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
            {t("qoder.gateway.key.create")}
          </Button>
        </DemoAction>
      </div>

      <div className="px-5 py-3">
        {loading && keys.length === 0 ? (
          /* 区块**首次加载** ⇒ `<Skeleton>` 占位（§3.6）；文字型「加载中」+ `<Loader2>`
             是**按钮操作中**态的语言，两者不可混用。 */
          <div className="space-y-2 py-4" aria-busy="true">
            <Skeleton className="h-4 w-40" />
            <Skeleton className="h-4 w-64" />
            <Skeleton className="h-4 w-52" />
          </div>
        ) : keys.length === 0 ? (
          <EmptyState icon={KeyRound} title={t("qoder.gateway.key.empty")} className="py-4" />
        ) : (
          <div className="min-w-0 overflow-x-auto">
            <table className="w-full min-w-[640px] text-left text-sm">
              <thead>
                <tr className="text-xs text-muted-foreground">
                  <th className="pb-2 pr-4 font-medium">{t("qoder.gateway.key.name")}</th>
                  <th className="pb-2 pr-4 font-medium">{t("qoder.gateway.key.col.region")}</th>
                  <th className="pb-2 pr-4 font-medium">{t("qoder.gateway.key.col.prefix")}</th>
                  <th className="pb-2 pr-4 font-medium">{t("qoder.gateway.key.col.createdAt")}</th>
                  <th className="pb-2 pr-4 font-medium">{t("qoder.gateway.key.col.lastUsed")}</th>
                  <th className="pb-2 pr-4 font-medium">{t("qoder.gateway.key.col.status")}</th>
                  <th className="pb-2 font-medium">{t("qoder.gateway.key.col.actions")}</th>
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
                          {regionDescriptor(keyRegion(key)).versionLabel}
                        </Badge>
                      </td>
                      {/* 掩码补尾部 4 位（`prefix••••••••suffix`）—— 用户靠它**确认是哪一把**
                          （同时建两把时前缀可能只差一两位）。无 `suffix` ⇒ 只显示前缀 + 省略号
                          （与另两条产品线同一规则）。 */}
                      <td className="py-2 pr-4 font-mono text-xs text-muted-foreground">
                        {key.suffix
                          ? `${key.prefix}••••••••${key.suffix}`
                          : `${key.prefix}…`}
                      </td>
                      <td className="py-2 pr-4 text-xs text-muted-foreground">
                        {formatDate(key.createdAt)}
                      </td>
                      <td className="py-2 pr-4 text-xs text-muted-foreground">
                        {key.lastUsedAt ? formatDate(key.lastUsedAt) : t("qoder.gateway.key.neverUsed")}
                      </td>
                      <td className="py-2 pr-4">
                        {revoked ? (
                          <Badge variant="secondary" className="rounded-md text-muted-foreground">
                            {t("qoder.gateway.key.statusRevoked")}
                          </Badge>
                        ) : (
                          <Badge variant="success" className="rounded-md">
                            {t("qoder.gateway.key.statusActive")}
                          </Badge>
                        )}
                      </td>
                      <td className="py-2">
                        {/* ★ 已吊销 ⇒ **两个出口都在**：恢复（可逆）+ 删除（不可逆）。
                            与另两条产品线同形同序（恢复在前、删除在后）。 */}
                        {revoked ? (
                          <div className="flex items-center gap-1">
                            <Button variant="ghost" size="sm" onClick={() => setRestoreTarget(key)}>
                              <RotateCcw />
                              {t("qoder.gateway.key.restore")}
                            </Button>
                            <Button
                              variant="ghost"
                              size="sm"
                              className="text-destructive hover:text-destructive"
                              onClick={() => setDeleteTarget(key)}
                            >
                              <Trash2 />
                              {t("qoder.gateway.key.delete")}
                            </Button>
                          </div>
                        ) : (
                          <Button variant="ghost" size="sm" onClick={() => setRevokeTarget(key)}>
                            {t("qoder.gateway.key.revoke")}
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
            <DialogTitle>{t("qoder.gateway.key.create")}</DialogTitle>
            <DialogDescription>{t("qoder.gateway.key.createDesc")}</DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="qoder-key-name">{t("qoder.gateway.key.name")}</Label>
              <Input
                id="qoder-key-name"
                value={name}
                onChange={(event) => setName(event.target.value)}
                placeholder={t("qoder.gateway.key.namePlaceholder")}
                spellCheck={false}
                autoComplete="off"
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="qoder-key-region">{t("qoder.gateway.key.region")}</Label>
              <Select value={region} onValueChange={(value) => setRegion(value as Region)}>
                <SelectTrigger
                  id="qoder-key-region"
                  className="w-full"
                  aria-label={t("qoder.gateway.key.region")}
                >
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {REGIONS.map((item) => (
                    <SelectItem key={item} value={item}>
                      {regionDescriptor(item).versionLabel}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <p className="text-xs text-muted-foreground">{t("qoder.gateway.key.regionHint")}</p>
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setCreateOpen(false)} disabled={creating}>
              {t("qoder.gateway.key.cancel")}
            </Button>
            <Button onClick={() => void onCreate()} disabled={creating}>
              {creating && <Loader2 className="animate-spin" />}
              {t("qoder.gateway.key.createSubmit")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 一次性明文展示。
          ★ 与另两条产品线**同规则**：遮罩点击与 ESC 一律拦截（明文不可复原，误关即永久丢失），
            复制按钮不禁用（允许反复复制）。 */}
      <Dialog open={plaintext !== null} onOpenChange={(open) => !open && closePlaintext()}>
        <DialogContent
          className="sm:max-w-md"
          onInteractOutside={(event) => event.preventDefault()}
          onEscapeKeyDown={(event) => event.preventDefault()}
        >
          <DialogHeader>
            <DialogTitle>{t("qoder.gateway.key.createdTitle")}</DialogTitle>
            <DialogDescription>{t("qoder.gateway.key.createdDesc")}</DialogDescription>
          </DialogHeader>
          {plaintext && (
            <div className="flex items-center gap-2 rounded-lg border border-border bg-muted/40 px-3 py-2.5">
              {/* `select-all`：点一下即全选，手动 Ctrl+C 也能用（剪贴板 API 被拒时的兜底）。 */}
              <code className="min-w-0 flex-1 select-all break-all font-mono text-xs">
                {plaintext.value}
              </code>
              <Button
                variant="outline"
                size="sm"
                data-act="gw-key-copy"
                onClick={() => void onCopyPlaintext()}
              >
                {copied ? <Check /> : <Copy />}
                {copied ? t("shared.clipboard.copied") : t("qoder.gateway.key.copy")}
              </Button>
            </div>
          )}
          <DialogFooter>
            <Button onClick={closePlaintext}>{t("qoder.gateway.key.saved")}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 吊销确认 */}
      <Dialog open={revokeTarget !== null} onOpenChange={(open) => !open && setRevokeTarget(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("qoder.gateway.key.revokeTitle")}</DialogTitle>
            <DialogDescription>
              {t("qoder.gateway.key.revokeDesc", { name: revokeTarget?.name ?? "" })}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setRevokeTarget(null)} disabled={busy}>
              {t("qoder.gateway.key.cancel")}
            </Button>
            <Button variant="destructive" onClick={() => void confirmRevoke()} disabled={busy}>
              {t("qoder.gateway.key.revoke")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 恢复确认（可逆动作 ⇒ 主按钮非 destructive） */}
      <Dialog open={restoreTarget !== null} onOpenChange={(open) => !open && setRestoreTarget(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("qoder.gateway.key.restoreTitle")}</DialogTitle>
            <DialogDescription>
              {t("qoder.gateway.key.restoreDesc", { name: restoreTarget?.name ?? "" })}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setRestoreTarget(null)} disabled={busy}>
              {t("qoder.gateway.key.cancel")}
            </Button>
            <Button onClick={() => void confirmRestore()} disabled={busy}>
              {t("qoder.gateway.key.restore")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 删除确认 */}
      <Dialog open={deleteTarget !== null} onOpenChange={(open) => !open && setDeleteTarget(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("qoder.gateway.key.deleteTitle")}</DialogTitle>
            <DialogDescription>
              {t("qoder.gateway.key.deleteDesc", { name: deleteTarget?.name ?? "" })}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDeleteTarget(null)} disabled={busy}>
              {t("qoder.gateway.key.cancel")}
            </Button>
            <Button variant="destructive" onClick={() => void confirmDelete()} disabled={busy}>
              {t("qoder.gateway.key.delete")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Card>
  );
}

/**
 * Key 记录里的区域串 → `Region`。
 *
 * 后端只可能给 `"cn"` / `"global"`（`QoderRegion` 的两个取值）；出现别的值说明磁盘被
 * 人工改过 —— 此时**回落 `cn` 并在界面上如实显示**（而不是抛错炸掉整张 Key 表：
 * 一张表里一个坏字段不该让整页不可用）。
 */
function keyRegion(record: QoderApiKeyRecord): Region {
  return record.region === "global" ? "global" : "cn";
}
