import { useEffect, useState, type ReactNode } from "react";
import { Navigate, useLocation } from "react-router-dom";

import { PageFallback } from "@/components/page-fallback";
import { demoModeEnabled } from "@/lib/demo-mode";
import { fetchAuthState, LOGIN_PATH } from "@/lib/webui-auth";

/**
 * webui 登录守卫。
 *
 * ## 它做三件事，顺序不能换
 *
 * 1. **问服务端**（`GET /api/auth/state`）—— 前端不猜「有没有密码」，
 *    唯一真相在服务端。猜错的两种后果都很难看：误判「有密码」会把本机用户
 *    挡在登录页外（而他压根没设过密码）；误判「没密码」会先渲染一屏 401。
 * 2. **未登录 → 去 `/login`**，并把当前深链塞进 `?next=`。
 * 3. **已登录 → 原样渲染**。
 *
 * ## 为什么只在挂载时问一次，而不是每个请求前问
 *
 * 逐请求询问会让每个页面多一次往返，而且仍然挡不住「页面渲染到一半会话过期」。
 * 那条路径由 `lib/api.ts` 的 `httpCall` 401 分支兜底（`redirectToLogin`），
 * 两者的分工是：**这里负责首屏，那里负责运行中**。
 *
 * ## 演示模式直接放行
 *
 * `demoModeEnabled` 时 `call()` 走本地假数据，根本不发 HTTP；
 * 此时去问 `/api/auth/state` 只会得到一个 `unreachable`，把静态演示站挡死。
 */
export function AuthGate({ children }: { children: ReactNode }) {
  const location = useLocation();
  const [state, setState] = useState<"checking" | "ok" | "login">(
    demoModeEnabled ? "ok" : "checking",
  );

  useEffect(() => {
    if (demoModeEnabled) return;
    let alive = true;
    // 已经在登录页就不必再问：登录页本身要渲染，问出「未登录」也只是原地不动，
    // 而多一次失败请求会让「服务没起」的报错在登录页上闪一下。
    if (location.pathname === LOGIN_PATH) {
      setState("ok");
      return;
    }
    void fetchAuthState()
      .then((next) => {
        if (!alive) return;
        setState(!next.authRequired || next.authed ? "ok" : "login");
      })
      .catch(() => {
        // 连不上服务：**不**把用户送到登录页（那里同样连不上，且会误导成
        // 「密码错了」）。放行让页面自己渲染 `unreachable` 错误 —— 它说的是
        // 真话：「服务没起」，而这句话比登录页有用。
        if (alive) setState("ok");
      });
    return () => {
      alive = false;
    };
    // 依赖只留 `pathname`：登录成功后回跳会改变它，从而重新复查一次登录态。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [location.pathname]);

  if (state === "checking") return <PageFallback />;
  if (state === "login") {
    const next = encodeURIComponent(`${location.pathname}${location.search}${location.hash}`);
    return <Navigate to={`${LOGIN_PATH}?next=${next}`} replace />;
  }
  return <>{children}</>;
}
