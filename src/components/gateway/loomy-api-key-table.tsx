import { useCallback, useEffect, useRef, useState } from "react";
import { Check, Copy, KeyRound, Loader2, RotateCcw, Trash2 } from "lucide-react";
import { toast } from "sonner";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { DemoAction } from "@/components/demo-action";
import { KeysBar } from "@/components/gateway/keys-bar";
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
import { Skeleton } from "@/components/ui/skeleton";
import * as api from "@/lib/api";
import { copyText } from "@/lib/clipboard";
import { formatDate } from "@/lib/format";
import { useT } from "@/lib/i18n";
import { normalizeLoomyApiKeys, type LoomyApiKeyRecord } from "@/lib/loomy-gateway";
import { cn } from "@/lib/utils";

/**
 * Loomy 多 Key 列表 + 创建 / 吊销 / 恢复 / 删除（对齐 Qoder `qoder-api-key-table.tsx` 骨架）。
 *
 * ## 与 Qoder 版的唯一差别：**没有归属轴**
 *
 * Loomy 单作用域 `loomy-cn`（一本账号库、一套端点）⇒ Key 不存在「归属哪个区域」这回事。
 * 摆一个只有 `cn` 一个选项的区域选择器就是**假控件**（§3.15.1 同一判据）。
 * 创建对话框因此只有「名称」一项，表格也少一列。
 *
 * ## 三态动作（与另两条产品线**同形**）
 *
 * **吊销**（置 `revokedAt`，可逆）→ 已吊销的行**必须同时给「恢复」与「删除」两个出口**；
 * **恢复**（清 `revokedAt`，同一把 Key 明文未变 ⇒ 客户端无需改配置）；
 * **删除**（仅已吊销可删，物理删除）。只给删除等于逼用户为一次误吊销删掉重建。
 *
 * 明文只在创建时一次性返回（`create_loomy_api_key`），列表接口永远只给脱敏前缀 + 末 4 位。
 *
 * ## 2026-10-15 骨架对齐原型：卡头 → `KeysBar` 工具条 + 裸表
 *
 * 原型（`viewGateway()` 的 `keys` 分支）= **工具条 + 裸表 + 说明段**，没有卡头。
 * 本组件此前套了一整张 `<Card>`（卡头放「API Key」标题 + 创建按钮）：标题与左栏分区
 * 导航重复，已去掉；创建出口挪进 `KeysBar` 的 `children`（三条产品线**共用同一个构件**）。
 *
 * 原型表下的**说明段**（掩码口径 / 累计口径 / 吊销可逆）在表下方渲染，走**三条产品线
 * 共用**的键 `shared.gateway.keys.note`（各侧差异只在表里的「归属」列）；
 * 空态时不附说明段（原型如此：空态只给 `emptyBlock`）。
 */
export function LoomyApiKeyTable({
  className,
  /** 数据变化（创建 / 吊销 / 恢复 / 删除）后的回调，供页面刷新网关状态里的 Key 前缀。 */
  onChanged,
}: {
  className?: string;
  onChanged?: () => void;
}) {
  const t = useT();
  const [keys, setKeys] = useState<LoomyApiKeyRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [createOpen, setCreateOpen] = useState(false);
  const [name, setName] = useState("");
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
  const [revokeTarget, setRevokeTarget] = useState<LoomyApiKeyRecord | null>(null);
  const [restoreTarget, setRestoreTarget] = useState<LoomyApiKeyRecord | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<LoomyApiKeyRecord | null>(null);
  const [busy, setBusy] = useState(false);

  /** 拉取列表。创建 / 吊销 / 恢复 / 删除后都重新拉，保证列表与后端一致。 */
  const load = useCallback(async () => {
    setLoading(true);
    try {
      const result = await api.listLoomyApiKeys();
      setKeys(normalizeLoomyApiKeys(result));
    } catch (e) {
      // 演示模式 / 后端不可用：保持空列表，不清空已有数据。
      toast.error(t("loomy.gateway.key.loadFailed"), { description: api.asError(e) });
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  function openCreate() {
    setName("");
    setCreateOpen(true);
  }

  async function onCreate() {
    const trimmed = name.trim();
    if (!trimmed) {
      toast.error(t("loomy.gateway.key.nameRequired"));
      return;
    }
    setCreating(true);
    try {
      // 单作用域平台：命令**不收 scope**（多一个恒为 `loomy-cn` 的参数就是假参数）。
      const result = await api.createLoomyApiKey(trimmed);
      const value = result.key;
      if (!value) {
        // 创建成功但没回明文 = 契约异常，必须显式提示而不是静默（明文不可复原）。
        toast.error(t("loomy.gateway.key.noPlaintext"));
        return;
      }
      setCreateOpen(false);
      setCopied(false);
      setPlaintext({ value, name: trimmed });
      await load();
      onChanged?.();
    } catch (e) {
      toast.error(t("loomy.gateway.key.createFailed"), { description: api.asError(e) });
    } finally {
      setCreating(false);
    }
  }

  /** 复制明文。**可反复调用**：成功后把按钮切成「已复制」，1.6s 后复位。 */
  async function onCopyPlaintext() {
    if (!plaintext) return;
    await copyText(plaintext.value, t("loomy.gateway.key.copied"));
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
      await api.revokeLoomyApiKey(revokeTarget.id);
      toast.success(t("loomy.gateway.key.revoked"), { description: revokeTarget.name });
      setRevokeTarget(null);
      await load();
      onChanged?.();
    } catch (e) {
      toast.error(t("loomy.gateway.key.revokeFailed"), { description: api.asError(e) });
    } finally {
      setBusy(false);
    }
  }

  async function confirmRestore() {
    if (!restoreTarget) return;
    setBusy(true);
    try {
      await api.restoreLoomyApiKey(restoreTarget.id);
      toast.success(t("loomy.gateway.key.restored"), { description: restoreTarget.name });
      setRestoreTarget(null);
      await load();
      onChanged?.();
    } catch (e) {
      toast.error(t("loomy.gateway.key.restoreFailed"), { description: api.asError(e) });
    } finally {
      setBusy(false);
    }
  }

  async function confirmDelete() {
    if (!deleteTarget) return;
    setBusy(true);
    try {
      await api.deleteLoomyApiKey(deleteTarget.id);
      toast.success(t("loomy.gateway.key.deleted"), { description: deleteTarget.name });
      setDeleteTarget(null);
      await load();
      onChanged?.();
    } catch (e) {
      toast.error(t("loomy.gateway.key.deleteFailed"), { description: api.asError(e) });
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className={cn(className)}>
      {/* 工具条与另两条产品线 / 统一网关**共用同一个构件**：计数 + 说明 + 创建出口。 */}
      <KeysBar total={keys.length} live={keys.filter((key) => !key.revoked).length}>
        <DemoAction>
          <Button size="sm" onClick={openCreate}>
            <KeyRound />
            {t("loomy.gateway.key.create")}
          </Button>
        </DemoAction>
      </KeysBar>

      {loading && keys.length === 0 ? (
        /* 区块**首次加载** ⇒ `<Skeleton>` 占位（§3.6）；文字型「加载中」+ `<Loader2>`
           是**按钮操作中**态的语言，两者不可混用。 */
        <div className="space-y-2 py-4" aria-busy="true">
          <Skeleton className="h-4 w-40" />
          <Skeleton className="h-4 w-64" />
          <Skeleton className="h-4 w-52" />
        </div>
      ) : keys.length === 0 ? (
        <EmptyState icon={KeyRound} title={t("loomy.gateway.key.empty")} className="py-4" />
      ) : (
        <div className="min-w-0 overflow-x-auto">
          <table className="w-full min-w-[640px] text-left text-sm">
            <thead>
              <tr className="text-xs text-muted-foreground">
                <th className="pb-2 pr-4 font-medium">{t("loomy.gateway.key.name")}</th>
                <th className="pb-2 pr-4 font-medium">{t("loomy.gateway.key.col.prefix")}</th>
                <th className="pb-2 pr-4 font-medium">{t("loomy.gateway.key.col.createdAt")}</th>
                <th className="pb-2 pr-4 font-medium">{t("loomy.gateway.key.col.lastUsed")}</th>
                <th className="pb-2 pr-4 font-medium">{t("loomy.gateway.key.col.status")}</th>
                <th className="pb-2 font-medium">{t("loomy.gateway.key.col.actions")}</th>
              </tr>
            </thead>
            <tbody>
              {keys.map((key) => {
                const revoked = key.revoked;
                return (
                  <tr key={key.id} className="border-t border-border/60">
                    <td className="py-2 pr-4 font-medium">{key.name}</td>
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
                      {key.lastUsedAt ? formatDate(key.lastUsedAt) : t("loomy.gateway.key.neverUsed")}
                    </td>
                    <td className="py-2 pr-4">
                      {revoked ? (
                        <Badge variant="secondary" className="rounded-md text-muted-foreground">
                          {t("loomy.gateway.key.statusRevoked")}
                        </Badge>
                      ) : (
                        <Badge variant="success" className="rounded-md">
                          {t("loomy.gateway.key.statusActive")}
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
                            {t("loomy.gateway.key.restore")}
                          </Button>
                          <Button
                            variant="ghost"
                            size="sm"
                            className="text-destructive hover:text-destructive"
                            onClick={() => setDeleteTarget(key)}
                          >
                            <Trash2 />
                            {t("loomy.gateway.key.delete")}
                          </Button>
                        </div>
                      ) : (
                        <Button variant="ghost" size="sm" onClick={() => setRevokeTarget(key)}>
                          {t("loomy.gateway.key.revoke")}
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

      {/* 说明段（原型 `<p class="tiny muted" style="margin:10px 2px 0">`）：掩码口径 /
          累计口径 / 吊销可逆。三条产品线共用一条键，差异只在表里的「归属」列。 */}
      {keys.length > 0 && (
        <p className="mt-2.5 text-[11.5px] leading-5 text-muted-foreground">
          {t("shared.gateway.keys.note")}
        </p>
      )}

      {/* 创建对话框 */}
      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("loomy.gateway.key.create")}</DialogTitle>
            <DialogDescription>{t("loomy.gateway.key.createDesc")}</DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="loomy-key-name">{t("loomy.gateway.key.name")}</Label>
              <Input
                id="loomy-key-name"
                value={name}
                onChange={(event) => setName(event.target.value)}
                placeholder={t("loomy.gateway.key.namePlaceholder")}
                spellCheck={false}
                autoComplete="off"
              />
            </div>
            {/* 刻意**没有**区域选择器：Loomy 单作用域，只有 `cn` 一个取值 ——
                摆一个单选项的下拉就是本仓明令禁止的假控件。 */}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setCreateOpen(false)} disabled={creating}>
              {t("loomy.gateway.key.cancel")}
            </Button>
            <Button onClick={() => void onCreate()} disabled={creating}>
              {creating && <Loader2 className="animate-spin" />}
              {t("loomy.gateway.key.createSubmit")}
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
            <DialogTitle>{t("loomy.gateway.key.createdTitle")}</DialogTitle>
            <DialogDescription>{t("loomy.gateway.key.createdDesc")}</DialogDescription>
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
                {copied ? t("shared.clipboard.copied") : t("loomy.gateway.key.copy")}
              </Button>
            </div>
          )}
          <DialogFooter>
            <Button onClick={closePlaintext}>{t("loomy.gateway.key.saved")}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 吊销确认 */}
      <Dialog open={revokeTarget !== null} onOpenChange={(open) => !open && setRevokeTarget(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("loomy.gateway.key.revokeTitle")}</DialogTitle>
            <DialogDescription>
              {t("loomy.gateway.key.revokeDesc", { name: revokeTarget?.name ?? "" })}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setRevokeTarget(null)} disabled={busy}>
              {t("loomy.gateway.key.cancel")}
            </Button>
            <Button variant="destructive" onClick={() => void confirmRevoke()} disabled={busy}>
              {t("loomy.gateway.key.revoke")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 恢复确认（可逆动作 ⇒ 主按钮非 destructive） */}
      <Dialog open={restoreTarget !== null} onOpenChange={(open) => !open && setRestoreTarget(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("loomy.gateway.key.restoreTitle")}</DialogTitle>
            <DialogDescription>
              {t("loomy.gateway.key.restoreDesc", { name: restoreTarget?.name ?? "" })}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setRestoreTarget(null)} disabled={busy}>
              {t("loomy.gateway.key.cancel")}
            </Button>
            <Button onClick={() => void confirmRestore()} disabled={busy}>
              {t("loomy.gateway.key.restore")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 删除确认 */}
      <Dialog open={deleteTarget !== null} onOpenChange={(open) => !open && setDeleteTarget(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("loomy.gateway.key.deleteTitle")}</DialogTitle>
            <DialogDescription>
              {t("loomy.gateway.key.deleteDesc", { name: deleteTarget?.name ?? "" })}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDeleteTarget(null)} disabled={busy}>
              {t("loomy.gateway.key.cancel")}
            </Button>
            <Button variant="destructive" onClick={() => void confirmDelete()} disabled={busy}>
              {t("loomy.gateway.key.delete")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
