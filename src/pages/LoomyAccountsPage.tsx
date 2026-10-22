/**
 * Loomy 账号页（单作用域 `loomy-cn`）。
 *
 * ## 登录方式（诚实口径）
 *
 * - **微信扫码**（二期：纯 HTTP 长轮询，无本地监听，容器形态同样可用）、
 *   **短信登录**（两步：发码 → 验证码登录）与**粘贴凭据导入**；
 * - 账号卡片：积分快照（永久 / 每日两池）、备注 / 分组、删除；
 * - 签到 / 积分查询走**逐账号**动作（卡内按钮）。
 *
 * ⚠️ 一期**不做**的（不画空壳）：批量签到、回收站、本机导入（Loomy 无本地客户端）、
 * 切换登录态（无客户端可切）。缺的能力一律不渲染。
 */
import { useCallback, useEffect, useState } from "react";
import { Gift, Loader2, MessageSquare, Plus, QrCode, RefreshCw, Trash2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
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
import { LoomyWechatLoginDialog } from "@/components/loomy/loomy-wechat-login-dialog";
import * as api from "@/lib/api";
import type { LoomyAccountView } from "@/lib/api";

type LoomyScope = "loomy-cn";

export default function LoomyAccountsPage() {
  return (
    <div className="p-6">
      <LoomyAccountsInner scope="loomy-cn" />
    </div>
  );
}

function LoomyAccountsInner({ scope }: { scope: LoomyScope }) {
  const [accounts, setAccounts] = useState<LoomyAccountView[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busyUid, setBusyUid] = useState<string | null>(null);

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

  const reload = useCallback(async () => {
    setError(null);
    try {
      const view = await api.getLoomyAccounts(scope);
      setAccounts(view.accounts);
    } catch (err) {
      setError(api.asError(err));
    }
  }, [scope]);

  useEffect(() => {
    void reload();
  }, [reload]);

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
      await reload();
    } catch (err) {
      setImportError(api.asError(err));
    } finally {
      setImporting(false);
    }
  };

  const doCheckin = async (uid: string) => {
    setBusyUid(uid);
    try {
      const outcome = await api.loomyCheckinClaim(scope, uid);
      window.alert(outcome.detail);
      await reload();
    } catch (err) {
      window.alert(api.asError(err));
    } finally {
      setBusyUid(null);
    }
  };

  const doCredits = async (uid: string) => {
    setBusyUid(uid);
    try {
      await api.loomyCredits(scope, uid);
      await reload();
    } catch (err) {
      window.alert(api.asError(err));
    } finally {
      setBusyUid(null);
    }
  };

  const doDelete = async (uid: string) => {
    if (!window.confirm(`确定删除账号 ${uid}？（Loomy 无回收站，删除后需重新导入/登录）`)) return;
    setBusyUid(uid);
    try {
      await api.loomyDeleteAccount(scope, uid);
      await reload();
    } catch (err) {
      window.alert(api.asError(err));
    } finally {
      setBusyUid(null);
    }
  };

  return (
    <div className="flex flex-col gap-4">
      <header className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <h1 className="text-lg font-semibold">Loomy 账号</h1>
          <ScopeChip scope={scope} />
        </div>
        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" onClick={() => void reload()}>
            <RefreshCw className="size-4" />
          </Button>
          <Button variant="outline" size="sm" onClick={() => setImportOpen(true)}>
            <Plus className="size-4" />
            导入凭据
          </Button>
          <Button variant="outline" size="sm" onClick={() => setWechatOpen(true)}>
            <QrCode className="size-4" />
            微信扫码
          </Button>
          <Button size="sm" onClick={() => setAddOpen(true)}>
            <MessageSquare className="size-4" />
            短信登录
          </Button>
        </div>
      </header>

      {accounts === null && !error && (
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {Array.from({ length: 3 }).map((_, i) => (
            <Skeleton key={i} className="h-32" />
          ))}
        </div>
      )}
      {error && <p className="text-sm text-destructive">{error}</p>}
      {accounts !== null && accounts.length === 0 && (
        <p className="text-sm text-muted-foreground">
          还没有 Loomy 账号：用「微信扫码」「短信登录」或「导入凭据」添加。
        </p>
      )}
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {accounts?.map((account) => (
          <div key={account.uid} className="rounded-lg border p-4">
            <div className="flex items-start justify-between">
              <div>
                <p className="font-medium">{account.name}</p>
                <p className="text-xs text-muted-foreground">
                  {account.phone || account.userid || account.uid}
                </p>
              </div>
              <Button
                variant="ghost"
                size="sm"
                disabled={busyUid === account.uid}
                onClick={() => void doDelete(account.uid)}
              >
                <Trash2 className="size-4 text-destructive" />
              </Button>
            </div>
            <dl className="mt-3 grid grid-cols-2 gap-2 text-sm">
              <div>
                <dt className="text-xs text-muted-foreground">永久积分</dt>
                <dd>{account.credits}</dd>
              </div>
              <div>
                <dt className="text-xs text-muted-foreground">每日池（当天作废）</dt>
                <dd>{account.dailyCredits}</dd>
              </div>
            </dl>
            {account.lastCheckin && (
              <p className="mt-2 text-xs text-muted-foreground">上次签到：{account.lastCheckin}</p>
            )}
            <div className="mt-3 flex gap-2">
              <Button
                variant="outline"
                size="sm"
                disabled={busyUid === account.uid || !account.hasCredential}
                onClick={() => void doCheckin(account.uid)}
              >
                {busyUid === account.uid ? (
                  <Loader2 className="size-4 animate-spin" />
                ) : (
                  <Gift className="size-4" />
                )}
                签到
              </Button>
              <Button
                variant="outline"
                size="sm"
                disabled={busyUid === account.uid || !account.hasCredential}
                onClick={() => void doCredits(account.uid)}
              >
                刷新积分
              </Button>
            </div>
          </div>
        ))}
      </div>

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
            <DialogTitle>Loomy 短信登录</DialogTitle>
            <DialogDescription>
              输入手机号获取验证码；session 有效期 14 天，过期需重新登录（Loomy 无续期端点）。
            </DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-3">
            <Input
              placeholder="11 位手机号"
              value={phone}
              inputMode="numeric"
              maxLength={11}
              onChange={(e) => setPhone(e.target.value.replace(/\D/g, ""))}
            />
            {msgid ? (
              <Input
                placeholder="短信验证码"
                value={code}
                inputMode="numeric"
                onChange={(e) => setCode(e.target.value)}
              />
            ) : null}
            {loginError && <p className="text-sm text-destructive">{loginError}</p>}
          </div>
          <DialogFooter>
            {msgid ? (
              <Button onClick={() => void verify()} disabled={verifying || code.length === 0}>
                {verifying ? <Loader2 className="size-4 animate-spin" /> : null}
                登录
              </Button>
            ) : (
              <Button
                onClick={() => void sendCode()}
                disabled={sending || phone.length !== 11}
              >
                {sending ? <Loader2 className="size-4 animate-spin" /> : null}
                发送验证码
              </Button>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 导入凭据 */}
      <Dialog open={importOpen} onOpenChange={setImportOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>导入 Loomy 凭据</DialogTitle>
            <DialogDescription>
              粘贴 Loomy 登录后的凭据 JSON（`access_token` / `userid` / `phone`）。
              需包含 32 位十六进制的讯飞 session 或有效的 userid。
            </DialogDescription>
          </DialogHeader>
          <textarea
            className="min-h-24 rounded-md border bg-transparent p-2 text-sm"
            placeholder='{"access_token": "...", "userid": "...", "phone": "..."}'
            value={importText}
            onChange={(e) => setImportText(e.target.value)}
          />
          {importError && <p className="text-sm text-destructive">{importError}</p>}
          <DialogFooter>
            <Button
              onClick={() => void doImport()}
              disabled={importing || importText.trim().length === 0}
            >
              {importing ? <Loader2 className="size-4 animate-spin" /> : null}
              导入
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
