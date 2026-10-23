import { useEffect, useRef, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { KeyRound, Loader2, ShieldCheck } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useT } from "@/lib/i18n";
import { login, safeNextPath } from "@/lib/webui-auth";

/**
 * webui 登录页。
 *
 * ## 为什么挂在 `Layout` **之外**
 *
 * `Layout` 会立刻发起一串管理面请求（状态、账号、网关…），而它们此刻必然 401。
 * 挂在里面等于每次登录都先刷一屏错误提示。这里只渲染一张表单。
 *
 * ## `?next=` 回跳
 *
 * `SameSite=Strict` 下从站外链接进来时首个请求不带 Cookie，必然先落这里。
 * 没有 `next` 就等于把用户的书签悄悄改成了首页；`safeNextPath` 负责挡开放重定向。
 */
export default function LoginPage() {
  const t = useT();
  const navigate = useNavigate();
  const [params] = useSearchParams();
  const [password, setPassword] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  // 自动聚焦：这是整页唯一的输入项，用户不需要先找它。
  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!password || pending) return;
    setPending(true);
    setError(null);
    try {
      await login(password);
      // 用 `replace`：登录页不该留在历史里（否则后退会回到「已登录状态下」的登录页）。
      navigate(safeNextPath(params.get("next")), { replace: true });
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
      setPassword("");
      inputRef.current?.focus();
    } finally {
      setPending(false);
    }
  };

  return (
    <div className="bg-background flex min-h-screen items-center justify-center px-6">
      <Card className="w-full max-w-[380px]">
        <CardContent className="flex flex-col gap-5">
          <div className="flex flex-col items-center gap-2 text-center">
            <div className="bg-muted text-foreground flex size-10 items-center justify-center rounded-full">
              <ShieldCheck className="size-5" />
            </div>
            <h1 className="text-[15px] font-semibold">{t("shared.auth.loginTitle")}</h1>
            <p className="text-muted-foreground text-[11.5px]">{t("shared.auth.loginDesc")}</p>
          </div>

          <form className="flex flex-col gap-3" onSubmit={submit}>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="webui-password" className="text-[12px]">
                {t("shared.auth.password")}
              </Label>
              <Input
                id="webui-password"
                ref={inputRef}
                type="password"
                autoComplete="current-password"
                value={password}
                disabled={pending}
                onChange={(event) => setPassword(event.target.value)}
                aria-invalid={error ? true : undefined}
                aria-describedby={error ? "webui-password-error" : undefined}
              />
            </div>

            {error ? (
              <p id="webui-password-error" className="text-destructive text-[11.5px]" role="alert">
                {error}
              </p>
            ) : null}

            <Button type="submit" disabled={!password || pending} className="w-full">
              {pending ? (
                <Loader2 className="size-4 animate-spin" />
              ) : (
                <KeyRound className="size-4" />
              )}
              {t("shared.auth.loginAction")}
            </Button>
          </form>

          <p className="text-muted-foreground text-center text-[11px]">
            {t("shared.auth.loginHint")}
          </p>
        </CardContent>
      </Card>
    </div>
  );
}
