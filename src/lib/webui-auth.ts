/**
 * webui 管理面**登录态**（浏览器专用，桌面端永不加载）。
 *
 * ## 为什么单开一个文件、自带 `fetch`，而不复用 `lib/api.ts`
 *
 * `lib/api.ts` 的 `httpCall` 走 `ROUTES` 表，而那张表受
 * `scripts/check-api-contract.cjs` 第 4 条约束：**每个命令都必须在
 * `src-tauri/src/lib.rs` 的 `invoke_handler` 登记**。
 * 登录对桌面端是无意义的概念（桌面端走 `invoke`，压根不经过 HTTP），
 * 强行塞进 `ROUTES` 会同时造成两个后果：
 *
 * 1. 门禁变红（或必须为登录开一个「不登记」的白名单例外 —— 例外会腐烂）；
 * 2. 桌面端 `invoke("login")` 报 `command not found`。
 *
 * ⇒ 鉴权三路由（`/api/auth/state|login|logout`）**不进 `ROUTES`**，
 * 在本文件里用裸 `fetch` 直连。它们与 `api.ts` 的 `httpCall` 只有一处交集：
 * 后者在收到 `401` 时把浏览器送到 `/login`（见 `api.ts` 里的 `onUnauthorized`）。
 *
 * ## 为什么不用 `localStorage` 存 token
 *
 * token 在 `HttpOnly` Cookie 里（服务端下发），JS **读不到也写不了** ——
 * 这正是选 Cookie 的全部收益。本文件因此**不碰任何存储**：
 * 登录态的唯一真相是「服务端还认不认这个 Cookie」，
 * 前端只通过 `GET /api/auth/state` 问它。
 */

/** 与 `lib/api.ts` 的 `API_BASE` **同一套推导**（生产同源、dev 指向 57890）。 */
const API_BASE =
  import.meta.env.DEV || typeof window === "undefined"
    ? "http://127.0.0.1:57890"
    : window.location.origin;

/** `GET /api/auth/state` 的响应。 */
export interface WebuiAuthState {
  /** 服务端是否设了密码。`false` ⇒ 本机免密形态，界面不该出现登录页。 */
  authRequired: boolean;
  /** 当前浏览器的 Cookie 是否被服务端认可。 */
  authed: boolean;
}

/**
 * 未登录时浏览器该去哪。
 *
 * 带 `?next=` 保留原始深链：`SameSite=Strict` 下从外部链接进来时首个请求不带
 * Cookie，必然先落登录页；没有 `next` 就等于把用户的书签悄悄改成了首页。
 */
export const LOGIN_PATH = "/login";

/** 供 `api.ts` 的 401 分支调用：整页跳登录页（**不用** `navigate`，见下）。 */
export function redirectToLogin(): void {
  if (typeof window === "undefined") return;
  const { pathname, search, hash } = window.location;
  if (pathname === LOGIN_PATH) return;
  const next = encodeURIComponent(`${pathname}${search}${hash}`);
  // 用 `location.assign` 而非 react-router 的 `navigate`：
  // `api.ts` 是**非组件模块**，拿不到 router 上下文；而且登录态失效时整页重载
  // 顺带丢掉所有内存态（避免用过期数据继续渲染）。这也是 App 里 `AuthGate`
  // 在挂载时复查 `/api/auth/state` 的原因。
  window.location.assign(`${LOGIN_PATH}?next=${next}`);
}

/** 从 `?next=` 里取回跳目标；**只接受站内路径**（防开放重定向）。 */
export function safeNextPath(raw: string | null): string {
  if (!raw) return "/";
  let decoded = raw;
  try {
    decoded = decodeURIComponent(raw);
  } catch {
    return "/";
  }
  // 必须以单个 `/` 开头：`//evil.com` 是协议相对 URL，`http://…` 是绝对 URL，
  // 两者都会被浏览器当成站外跳转。这一条是开放重定向的唯一闸门。
  if (!decoded.startsWith("/") || decoded.startsWith("//")) return "/";
  if (decoded === LOGIN_PATH || decoded.startsWith(`${LOGIN_PATH}?`)) return "/";
  return decoded;
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  let response: Response;
  try {
    response = await fetch(`${API_BASE}${path}`, {
      // 同源请求默认就会带 Cookie；显式写出来是为了让「必须带凭据」这件事
      // 在代码里可见 —— 漏掉它会让登录永远「成功但立刻又 401」。
      credentials: "same-origin",
      headers: { "Content-Type": "application/json" },
      ...init,
    });
  } catch {
    throw new Error("无法连接 Buddy Switch 服务，请确认进程仍在运行");
  }
  const data = (await response.json().catch(() => ({}))) as Record<string, unknown>;
  if (!response.ok) {
    const message =
      (typeof data.error === "string" && data.error) ||
      (typeof data.message === "string" && data.message) ||
      `请求失败 (${response.status})`;
    throw new Error(message);
  }
  return data as T;
}

/** 问服务端：要不要登录、当前是否已登录。 */
export async function fetchAuthState(): Promise<WebuiAuthState> {
  const data = await request<{ authRequired?: boolean; authed?: boolean }>("/api/auth/state");
  return {
    authRequired: data.authRequired === true,
    authed: data.authed === true,
  };
}

/** 登录。成功时服务端下发 `HttpOnly` Cookie，本函数**不接触** token。 */
export async function login(password: string): Promise<void> {
  await request("/api/auth/login", {
    method: "POST",
    body: JSON.stringify({ password }),
  });
}

/** 登出（清服务端会话 + 让浏览器丢弃 Cookie）。 */
export async function logout(): Promise<void> {
  await request("/api/auth/logout", { method: "POST" });
}

/** 设置 / 修改管理密码。已设密码时 `oldPassword` 必填。 */
export async function changePassword(input: {
  oldPassword?: string;
  newPassword: string;
}): Promise<void> {
  await request("/api/auth/password", {
    method: "POST",
    body: JSON.stringify(input),
  });
}
