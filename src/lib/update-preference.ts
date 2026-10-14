/**
 * 「自动检查更新」偏好 —— 顶栏 ⚙「应用设置」抽屉里那一行。
 *
 * ## 为什么存 `localStorage` 而不是 store 里的 `app_prefs.json`
 *
 * 判据是**谁需要读它**：这一项只有渲染层读（`App.tsx` 的 `AppFooter` 轮询），
 * 宿主（Rust）一辈子不看它。留在 `localStorage` 就是同步可得、零 IPC；
 * 搬进后端反而要给每个纯前端开关都加一对命令 + 四处接线。
 * 对照：`close_to_tray` 必须后端存，因为「点关闭」发生在 Rust 侧（见 `app_prefs.rs`）。
 *
 * ## 为什么是一个外部 store（而不是 `AppFooter` 自己 `useState`）
 *
 * 这个偏好有**两个消费者**：抽屉里的开关（勾选态）与 `AppFooter` 的轮询（决定开不开）。
 * 各自 `useState` 会出现「设置里关了、轮询还在跑」—— 与 `shortcut-preference.ts`
 * 同一种病（那边是「关掉后全局键不失效」）。因此收敛成一个模块级 store +
 * `useSyncExternalStore`（与 `lib/i18n.ts` / `lib/theme.ts` / `lib/shortcut-preference.ts` 同构）。
 *
 * ## 默认「开」
 *
 * 缺失键 ⇒ `true`。这是**改这个开关之前就有的行为**（`AppFooter` 原本挂载即查一次、
 * 之后每 30 分钟一次），默认值必须与它一致，否则老用户升级后升级提示会**静默消失**。
 *
 * ## 读取 / 写入是纯函数
 *
 * 接收 `storage` 入参（可传桩），因此可被门禁直接断言 —— 前端没有单测框架。
 */

import { useCallback, useSyncExternalStore } from "react";

/** `localStorage` 键。`v1` 前缀：将来语义若变可换键而不破坏旧值（同 `scope.ts` 惯例）。 */
export const AUTO_CHECK_UPDATE_KEY = "buddy-switch:v1:auto-check-update";

/** 检查间隔：原型这一行的说明就是「每 30 分钟检查一次」（`buddy-switch-prototype.html:4459`）。 */
export const AUTO_CHECK_UPDATE_INTERVAL_MS = 30 * 60 * 1000;

/**
 * 读取偏好。**缺失 ⇒ `true`**（默认开，与改开关前的行为一致）；
 * 存储不可用或抛异常也 ⇒ `true`。
 *
 * 静默吞掉 `localStorage` 异常：隐私模式 / 存储被禁时不该让应用崩掉，
 * 只是「记不住这个偏好」而已。
 */
export function readAutoCheckUpdate(storage: Pick<Storage, "getItem"> | undefined): boolean {
  try {
    const raw = storage?.getItem(AUTO_CHECK_UPDATE_KEY);
    if (raw === "0" || raw === "false") return false;
    return true;
  } catch {
    return true;
  }
}

/** 写入偏好。**写失败不抛**（记不住不影响功能）。 */
export function writeAutoCheckUpdate(
  storage: Pick<Storage, "setItem"> | undefined,
  enabled: boolean,
): void {
  try {
    storage?.setItem(AUTO_CHECK_UPDATE_KEY, enabled ? "1" : "0");
  } catch {
    /* 忽略：存储不可用时只是记不住 */
  }
}

/** 取可用的 `localStorage`（不可用返回 `undefined`）。 */
function safeStorage(): Storage | undefined {
  try {
    return typeof window === "undefined" ? undefined : window.localStorage;
  } catch {
    return undefined;
  }
}

/** 模块内缓存 —— 保证 `useSyncExternalStore` 的快照在两次渲染间稳定。 */
let current: boolean | null = null;
const listeners = new Set<() => void>();

function snapshot(): boolean {
  if (current === null) current = readAutoCheckUpdate(safeStorage());
  return current;
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** 设置偏好：持久化 + 更新缓存 + 通知订阅者（开关与轮询一起重渲染）。 */
export function setAutoCheckUpdate(enabled: boolean): void {
  writeAutoCheckUpdate(safeStorage(), enabled);
  current = enabled;
  for (const listener of listeners) listener();
}

/**
 * 订阅偏好。返回 `[enabled, setEnabled]`。
 *
 * 服务端快照（`getServerSnapshot`）固定 `true`：本应用是纯客户端，此分支不会被用到，
 * 但给出确定值可避免水合不一致（与 `useShortcutsEnabled` 同）。
 */
export function useAutoCheckUpdate(): [boolean, (value: boolean) => void] {
  const enabled = useSyncExternalStore(subscribe, snapshot, () => true);
  const set = useCallback((value: boolean) => setAutoCheckUpdate(value), []);
  return [enabled, set];
}
