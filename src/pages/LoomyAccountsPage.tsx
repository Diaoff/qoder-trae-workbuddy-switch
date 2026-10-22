/**
 * Loomy 账号页（单作用域 `loomy-cn`）。
 *
 * ## 登录方式（诚实口径）
 *
 * - **微信扫码**（纯 HTTP 长轮询，无本地监听，容器形态同样可用）、
 *   **短信登录**（两步：发码 → 验证码登录）与**粘贴凭据导入**；
 * - 账号卡：与另三个分区**同一套** `LoomyAccountCard`（头像 / chips / 指标 / 更多菜单）；
 * - 签到 / 刷新积分 / 编辑备注 / 设置分组 / 删除**都在卡的「更多」菜单里**
 *   （原型纪律：卡片不堆按钮）。
 *
 * ⚠️ 一期**不做**的（不画空壳）：批量签到、回收站、本机导入（本仓不读 Loomy 客户端目录）、
 * 切换登录态（无程序位能力）。缺的能力一律不渲染。
 *
 * ## 为什么用 toast + Dialog 而不是 `window.alert` / `window.confirm`
 *
 * 改造前本页用 `window.alert` 报签到结果、`window.confirm` 问删除 —— 那是**原生弹窗**：
 * 与应用的视觉完全脱节、在 Tauri 里还会阻塞 webview、且**不可 i18n、不可撤销**。
 * 同应用里另三个分区都是 toast / 应用内弹层，这里对齐。
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { Gift, Loader2, MessageSquare, Plus, QrCode, RefreshCw, Users } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { ScopeChip } from "@/components/scope-chip";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { LoomyAccountCard, loomyAccountName } from "@/components/loomy/loomy-account-card";
import { LoomyWechatLoginDialog } from "@/components/loomy/loomy-wechat-login-dialog";
import * as api from "@/lib/api";
import type { LoomyAccountView } from "@/lib/api";
import { useT } from "@/lib/i18n";

type LoomyScope = "loomy-cn";

export default function LoomyAccountsPage() {
  return (
    <div className="p-6">
      <LoomyAccountsInner scope="loomy-cn" />
    </div>
  );
}

/** 备注 / 分组共用的就地编辑弹层（两者交互一致 ⇒ 一份实现）。 */
type TextEditState = {
  account: LoomyAccountView;
  field: "remark" | "group";
  value: string;
} | null;

function LoomyAccountsInner({ scope }: { scope: LoomyScope }) {
  const t = useT();
  const [accounts, setAccounts] = useState<LoomyAccountView[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busyUid, setBusyUid] = useState<string | null>(null);
  /** 勾选（批量动作尚未落地，但勾选态与另三张卡同构，先保留视觉一致）。 */
  const [selected, setSelected] = useState<Set<string>>(() => new Set());

  // 登录对话框状态（微信扫码 + 短信两条路）
  const [wechatOpen, setWechatOpen] = useState(false);
  const [addOpen, setAddOpen] = useState(false);
  const [phone, setPhone] = useState("");
  const [code, setCode] = useState("");
  const [msgid, setMsgid] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const [verifying, setVerifying] = useState(false);
  const [loginError, setLoginError] = useState<string | null>(null);

  // 导入对话框状态
  const [importOpen, setImportOpen] = useState(false);
  const [importText, setImportText] = useState("");
  const [importing, setImporting] = useState(false);
  const [importError, setImportError] = useState<string | null>(null);

  /** 备注 / 分组就地编辑。 */
  const [textEdit, setTextEdit] = useState<TextEditState>(null);
  const [textSaving, setTextSaving] = useState(false);
  /** 删除确认（Loomy 无回收站 ⇒ 文案必须说清不可恢复）。 */
  const [pendingDelete, setPendingDelete] = useState<LoomyAccountView | null>(null);
  /** 批量签到进行中（驱动按钮转圈 + 防连点）。 */
  const [batchBusy, setBatchBusy] = useState(false);

  const reload = useCallback(async () => {
    setError(null);
    try {
      const view = await api.getLoomyAccounts(scope);
      setAccounts(view.accounts);
      // 勾选集合与账号库对齐：删掉的账号不该留在 selection 里。
      setSelected((previous) => {
        const alive = new Set(view.accounts.map((account) => account.uid));
        const next = new Set([...previous].filter((uid) => alive.has(uid)));
        return next.size === previous.size ? previous : next;
      });
    } catch (err) {
      setError(api.asError(err));
    }
  }, [scope]);

  useEffect(() => {
    void reload();
  }, [reload]);

  /** 本作用域已有的分组名（去重、非空）——给分组编辑弹层做候选。 */
  const groups = useMemo(() => {
    const seen = new Set<string>();
    for (const account of accounts ?? []) {
      const group = account.group?.trim();
      if (group) seen.add(group);
    }
    return [...seen].sort();
  }, [accounts]);

  const toggleSelect = (account: LoomyAccountView) => {
    setSelected((previous) => {
      const next = new Set(previous);
      if (next.has(account.uid)) next.delete(account.uid);
      else next.add(account.uid);
      return next;
    });
  };

  const sendCode = async () => {
    setLoginError(null);
    setSending(true);
    try {
      const result = await api.loomySmsSend(scope, phone);
      setMsgid(result.msgid);
    } catch (err) {
      setLoginError(api.asError(err));
    } finally {
      setSending(false);
    }
  };

  const verify = async () => {
    if (!msgid) return;
    setLoginError(null);
    setVerifying(true);
    try {
      await api.loomySmsVerify(scope, phone, code, msgid);
      setAddOpen(false);
      setMsgid(null);
      setCode("");
      setPhone("");
      toast.success(t("loomy.page.accounts.login.ok"));
      await reload();
    } catch (err) {
      setLoginError(api.asError(err));
    } finally {
      setVerifying(false);
    }
  };

  const doImport = async () => {
    setImportError(null);
    setImporting(true);
    try {
      await api.loomyImport(scope, importText);
      setImportOpen(false);
      setImportText("");
      toast.success(t("loomy.page.accounts.import.ok"));
      await reload();
    } catch (err) {
      setImportError(api.asError(err));
    } finally {
      setImporting(false);
    }
  };

  const doCheckin = async (account: LoomyAccountView) => {
    setBusyUid(account.uid);
    try {
      const outcome = await api.loomyCheckinClaim(scope, account.uid);
      // 上游的可读说明（含「今天已初始化」这类语义）原样呈现 —— 它是**事实**，
      // 不该被我们压成一句笼统的成功提示。
      toast.success(t("loomy.page.accounts.checkin.done"), { description: outcome.detail });
      await reload();
    } catch (err) {
      toast.error(t("loomy.page.accounts.checkin.failed"), { description: api.asError(err) });
    } finally {
      setBusyUid(null);
    }
  };

  /**
   * 批量签到：**串行**走上游（后端一次调用即完成整轮循环）。
   *
   * ★ 与 WorkBuddy 侧 `AccountsPage.batchCheckin` 同一条产品纪律：
   *   部分失败时把失败项留在 toast 的 action 里 ⇒ 用户可「只重试失败项」，
   *   不必把已成功的一起重跑（原型 P1-2：部分失败态不能只给一句 toast 就完事）。
   *
   * ⚠️ 不用前端 for 循环逐个调单账号端点：那会把「串行 + 部分失败不中断」这条
   *   逻辑复制一份到前端，且每个账号一次 IPC 往返。后端 `run_cycle_for` 已经是
   *   那个循环的唯一实现。
   */
  const doBatchCheckin = async () => {
    if (batchBusy) return;
    setBatchBusy(true);
    try {
      const result = await api.loomyCheckinAll(scope);
      await reload();

      const summary =
        result.failed > 0
          ? t("loomy.page.accounts.batch.summary", {
              ok: result.ok,
              already: result.already,
              failed: result.failed,
            })
          : t("loomy.page.accounts.batch.summaryNoFailed", {
              ok: result.ok,
              already: result.already,
            });
      const description =
        result.skipped > 0
          ? `${summary} · ${t("loomy.page.accounts.batch.skippedNote", { n: result.skipped })}`
          : summary;

      if (result.failed > 0) {
        toast.warning(t("loomy.page.accounts.batch.partial"), {
          description,
          action: {
            label: t("loomy.page.accounts.batch.retryFailed", { n: result.failed }),
            // 重试 = 再跑一轮整库。后端已对「今天签过」的账号幂等命中（alreadyProcessed），
            // 所以重复跑不会重复发放额度 —— 这比在前端筛 uid 再调单账号端点更简单，
            // 也不会漏掉「账号库刚变化」的情况。
            onClick: () => void doBatchCheckin(),
          },
        });
      } else {
        toast.success(t("loomy.page.accounts.batch.done"), { description });
      }
    } catch (err) {
      toast.error(t("loomy.page.accounts.batch.failed"), { description: api.asError(err) });
    } finally {
      setBatchBusy(false);
    }
  };

  const doRefreshCredits = async (account: LoomyAccountView) => {
    setBusyUid(account.uid);
    try {
      await api.loomyCredits(scope, account.uid);
      toast.success(t("loomy.page.accounts.credits.refreshed"));
      await reload();
    } catch (err) {
      toast.error(t("loomy.page.accounts.credits.refreshFailed"), {
        description: api.asError(err),
      });
    } finally {
      setBusyUid(null);
    }
  };

  const confirmDelete = async () => {
    if (!pendingDelete) return;
    const target = pendingDelete;
    setBusyUid(target.uid);
    try {
      await api.loomyDeleteAccount(scope, target.uid);
      setPendingDelete(null);
      toast.success(t("loomy.page.accounts.delete.done", { name: loomyAccountName(target) }));
      await reload();
    } catch (err) {
      toast.error(t("loomy.page.accounts.delete.failed"), { description: api.asError(err) });
    } finally {
      setBusyUid(null);
    }
  };

  const saveText = async () => {
    if (!textEdit) return;
    const { account, field, value } = textEdit;
    setTextSaving(true);
    try {
      const trimmed = value.trim();
      const next = trimmed.length > 0 ? trimmed : null;
      if (field === "remark") await api.loomySetAccountRemark(scope, account.uid, next);
      else await api.loomySetAccountGroup(scope, account.uid, next);
      setTextEdit(null);
      toast.success(
        field === "remark"
          ? t("loomy.page.accounts.remark.saved")
          : t("loomy.page.accounts.group.saved"),
      );
      await reload();
    } catch (err) {
      toast.error(
        field === "remark"
          ? t("loomy.page.accounts.remark.failed")
          : t("loomy.page.accounts.group.failed"),
        { description: api.asError(err) },
      );
    } finally {
      setTextSaving(false);
    }
  };

  return (
    <div className="flex flex-col gap-4">
      <header className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <h1 className="text-lg font-semibold">{t("loomy.page.accounts.title")}</h1>
          <ScopeChip scope={scope} />
        </div>
        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" onClick={() => void reload()} disabled={batchBusy}>
            <RefreshCw className="size-4" />
          </Button>
          {/* 批量签到：只在该平台声明了 checkin 能力、且有账号时才摆出来。
              没有账号时摆一个「全部签到」按钮点了无事发生 = 假控件。 */}
          {accounts !== null && accounts.length > 0 && (
            <Button
              variant="outline"
              size="sm"
              onClick={() => void doBatchCheckin()}
              disabled={batchBusy}
            >
              {batchBusy ? (
                <Loader2 className="size-4 animate-spin" />
              ) : (
                <Gift className="size-4" />
              )}
              {batchBusy
                ? t("loomy.page.accounts.batch.busy")
                : t("loomy.page.accounts.batch.checkin")}
            </Button>
          )}
          <Button variant="outline" size="sm" onClick={() => setImportOpen(true)}>
            <Plus className="size-4" />
            {t("loomy.page.accounts.add.import")}
          </Button>
          <Button variant="outline" size="sm" onClick={() => setWechatOpen(true)}>
            <QrCode className="size-4" />
            {t("loomy.page.accounts.add.wechat")}
          </Button>
          <Button size="sm" onClick={() => setAddOpen(true)}>
            <MessageSquare className="size-4" />
            {t("loomy.page.accounts.add.sms")}
          </Button>
        </div>
      </header>

      {error && <p className="text-sm text-destructive">{error}</p>}

      {accounts === null && !error && (
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {Array.from({ length: 3 }).map((_, i) => (
            <Skeleton key={i} className="h-[132px]" />
          ))}
        </div>
      )}

      {/* 空态**必须带出口**（§1.6 四要素）：没有它的账号页就是死路。 */}
      {accounts !== null && accounts.length === 0 && (
        <Card className="gap-0 py-0">
          <EmptyState
            icon={Users}
            title={t("loomy.page.accounts.empty.title")}
            description={t("loomy.page.accounts.empty.description")}
            action={
              <div className="flex flex-wrap gap-2">
                <Button size="sm" onClick={() => setWechatOpen(true)}>
                  <QrCode className="size-4" />
                  {t("loomy.page.accounts.add.wechat")}
                </Button>
                <Button variant="outline" size="sm" onClick={() => setAddOpen(true)}>
                  <MessageSquare className="size-4" />
                  {t("loomy.page.accounts.add.sms")}
                </Button>
                <Button variant="outline" size="sm" onClick={() => setImportOpen(true)}>
                  <Plus className="size-4" />
                  {t("loomy.page.accounts.add.import")}
                </Button>
              </div>
            }
          />
        </Card>
      )}

      {accounts !== null && accounts.length > 0 && (
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {accounts.map((account) => (
            <LoomyAccountCard
              key={account.uid}
              account={account}
              selected={selected.has(account.uid)}
              onToggleSelect={toggleSelect}
              busy={busyUid === account.uid}
              onCheckin={(target) => void doCheckin(target)}
              onRefreshCredits={(target) => void doRefreshCredits(target)}
              onEditRemark={(target) =>
                setTextEdit({ account: target, field: "remark", value: target.remark ?? "" })
              }
              onSetGroup={(target) =>
                setTextEdit({ account: target, field: "group", value: target.group ?? "" })
              }
              onDelete={(target) => setPendingDelete(target)}
            />
          ))}
        </div>
      )}

      {/* 微信扫码登录（二期） */}
      <LoomyWechatLoginDialog
        scope={scope}
        open={wechatOpen}
        onOpenChange={setWechatOpen}
        onLoggedIn={() => void reload()}
      />

      {/* 短信登录 */}
      <Dialog open={addOpen} onOpenChange={setAddOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t("loomy.page.accounts.sms.title")}</DialogTitle>
            <DialogDescription>{t("loomy.page.accounts.sms.description")}</DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-3">
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="loomy-sms-phone">{t("loomy.page.accounts.sms.phone")}</Label>
              <Input
                id="loomy-sms-phone"
                placeholder={t("loomy.page.accounts.sms.phonePlaceholder")}
                value={phone}
                inputMode="numeric"
                maxLength={11}
                onChange={(e) => setPhone(e.target.value.replace(/\D/g, ""))}
              />
            </div>
            {msgid ? (
              <div className="flex flex-col gap-1.5">
                <Label htmlFor="loomy-sms-code">{t("loomy.page.accounts.sms.code")}</Label>
                <Input
                  id="loomy-sms-code"
                  placeholder={t("loomy.page.accounts.sms.codePlaceholder")}
                  value={code}
                  inputMode="numeric"
                  onChange={(e) => setCode(e.target.value)}
                />
              </div>
            ) : null}
            {loginError && <p className="text-sm text-destructive">{loginError}</p>}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setAddOpen(false)} disabled={verifying}>
              {t("loomy.page.accounts.cancel")}
            </Button>
            {msgid ? (
              <Button onClick={() => void verify()} disabled={verifying || code.length === 0}>
                {verifying ? <Loader2 className="size-4 animate-spin" /> : null}
                {t("loomy.page.accounts.sms.submit")}
              </Button>
            ) : (
              <Button onClick={() => void sendCode()} disabled={sending || phone.length !== 11}>
                {sending ? <Loader2 className="size-4 animate-spin" /> : null}
                {t("loomy.page.accounts.sms.send")}
              </Button>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 导入凭据 */}
      <Dialog open={importOpen} onOpenChange={setImportOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t("loomy.page.accounts.import.title")}</DialogTitle>
            <DialogDescription>{t("loomy.page.accounts.import.description")}</DialogDescription>
          </DialogHeader>
          <textarea
            className="min-h-24 rounded-md border border-border bg-transparent p-2 text-sm"
            placeholder='{"access_token": "...", "userid": "...", "phone": "..."}'
            value={importText}
            onChange={(e) => setImportText(e.target.value)}
          />
          {importError && <p className="text-sm text-destructive">{importError}</p>}
          <DialogFooter>
            <Button variant="outline" onClick={() => setImportOpen(false)} disabled={importing}>
              {t("loomy.page.accounts.cancel")}
            </Button>
            <Button
              onClick={() => void doImport()}
              disabled={importing || importText.trim().length === 0}
            >
              {importing ? <Loader2 className="size-4 animate-spin" /> : null}
              {t("loomy.page.accounts.import.submit")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 备注 / 分组就地编辑（两个字段交互一致 ⇒ 一个弹层） */}
      <Dialog open={textEdit !== null} onOpenChange={(open) => !open && setTextEdit(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>
              {textEdit?.field === "remark"
                ? t("loomy.page.accounts.remark.title")
                : t("loomy.page.accounts.group.title")}
            </DialogTitle>
            <DialogDescription>
              {textEdit?.field === "remark"
                ? t("loomy.page.accounts.remark.description")
                : t("loomy.page.accounts.group.description")}
            </DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-2">
            <Input
              value={textEdit?.value ?? ""}
              placeholder={
                textEdit?.field === "remark"
                  ? t("loomy.page.accounts.remark.placeholder")
                  : t("loomy.page.accounts.group.placeholder")
              }
              onChange={(event) =>
                setTextEdit((previous) =>
                  previous ? { ...previous, value: event.target.value } : previous,
                )
              }
              onKeyDown={(event) => {
                if (event.key === "Enter") void saveText();
              }}
              spellCheck={false}
              autoComplete="off"
            />
            {/* 分组候选：让用户看见「已经有哪些组」，避免同一个组写成三种拼法。 */}
            {textEdit?.field === "group" && groups.length > 0 ? (
              <div className="flex flex-wrap gap-1.5">
                {groups.map((group) => (
                  <button
                    key={group}
                    type="button"
                    onClick={() =>
                      setTextEdit((previous) => (previous ? { ...previous, value: group } : previous))
                    }
                    className="rounded-md border border-border px-1.5 py-[2px] text-[11px] text-muted-foreground transition-colors hover:border-border-strong hover:text-foreground"
                  >
                    {group}
                  </button>
                ))}
              </div>
            ) : null}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setTextEdit(null)} disabled={textSaving}>
              {t("loomy.page.accounts.cancel")}
            </Button>
            <Button onClick={() => void saveText()} disabled={textSaving}>
              {textSaving ? <Loader2 className="size-4 animate-spin" /> : null}
              {t("loomy.page.accounts.save")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 删除确认。⚠️ Loomy **没有回收站** ⇒ 必须说清「不可恢复」，
          不能照抄 Qoder 的「可在回收站恢复」（那是谎报）。 */}
      <Dialog open={pendingDelete !== null} onOpenChange={(open) => !open && setPendingDelete(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("loomy.page.accounts.delete.title")}</DialogTitle>
            <DialogDescription>
              {t("loomy.page.accounts.delete.description", {
                name: pendingDelete ? loomyAccountName(pendingDelete) : "",
              })}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setPendingDelete(null)}
              disabled={busyUid !== null}
            >
              {t("loomy.page.accounts.cancel")}
            </Button>
            <Button
              variant="destructive"
              onClick={() => void confirmDelete()}
              disabled={busyUid !== null}
            >
              {busyUid !== null ? (
                <Loader2 className="size-4 animate-spin" />
              ) : (
                <Gift className="size-4" />
              )}
              {t("loomy.page.accounts.delete.confirm")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
