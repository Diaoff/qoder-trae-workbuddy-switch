import { useMemo } from "react";
import { create } from "zustand";

import * as api from "@/lib/api";
import {
  DEFAULT_GATEWAY_CONFIG,
  normalizeGatewayConfig,
  normalizeGatewayStatus,
  poolOf,
} from "@/lib/gateway";
import type {
  AccountStrategy,
  AccountStrategyView,
  ApiKeyRecord,
  CatalogSnapshot,
  CreateApiKeyResult,
  GatewayConfig,
  GatewayLogEntry,
  GatewayStatus,
  ModelProbeJob,
  ModelProbeState,
  Region,
} from "@/lib/types";
import { SCOPES, type Scope } from "@/lib/scope";

/** 网关配置缺省值（与后端 GatewayConfig::default 对齐）。 */
export { DEFAULT_GATEWAY_CONFIG };

/**
 * 未拉取到的作用域回落的**稳定空集**（引用稳定 ⇒ 不触发无谓重渲染）。
 * 消费者：模型列表组件（`s.disabled[scope]` 可能尚未加载）。
 */
export const EMPTY_DISABLED_SET: ReadonlySet<string> = new Set();

function defaultStrategyView(region?: Region): AccountStrategyView {
  return {
    region,
    strategy: { kind: "current" },
    selected: null,
  };
}

function defaultStrategies(): Record<Region, AccountStrategyView> {
  return { cn: defaultStrategyView("cn"), global: defaultStrategyView("global") };
}

/**
 * 限流探测的轮询间隔（约 1s，与 `switch_progress` 的进度轮询同款）。
 *
 * 依据（方案 §3.4）：本仓**没有** EventSource / SSE 前端消费，长任务一律走
 * 「前端定时 `probeStatus()`」——既有先例 `switch-account-dialog.tsx:191`。
 */
const PROBE_POLL_MS = 1000;

/**
 * 探测轮询定时器（模块级，**唯一实例**）。
 *
 * ⚠️ 必须是模块级而不是组件级：`model-probe-card` 可能因切换分区 / 页面导航卸载，
 * 组件里的 timer 会随卸载被遗忘 —— 于是「切走再切回」会留下多个并发轮询。
 * 这里由 store 单点持有，卡片卸载时显式调 `stopProbePolling()` 清掉。
 */
let probeTimer: ReturnType<typeof setInterval> | null = null;

function stopProbeTimer(): void {
  if (probeTimer !== null) {
    clearInterval(probeTimer);
    probeTimer = null;
  }
}

/** 兼容后端可能缺字段：始终补齐两个 region。 */
function normalizeStrategies(input: Partial<Record<Region, AccountStrategyView>> | null | undefined): Record<Region, AccountStrategyView> {
  return {
    cn: input?.cn ? { ...input.cn, region: input.cn.region ?? "cn" } : defaultStrategyView("cn"),
    global: input?.global ? { ...input.global, region: input.global.region ?? "global" } : defaultStrategyView("global"),
  };
}

interface GatewayState {
  config: GatewayConfig;
  status: GatewayStatus | null;
  keys: ApiKeyRecord[];
  models: Record<Region, CatalogSnapshot | null>;
  /**
   * 每个**作用域**被用户**禁用**的模型 id 集合（P11）。
   *
   * 它是**用户偏好**（§2.7，不进 `capabilities`），与后端 `/v1/models` 过滤（出口 ①）、
   * 调用拒绝（出口 ②）**同源** —— 后端读的是同一份存储（`gateway_disabled_models.json`），
   * 故这里看到的禁用集合就是各出口真正在用的那份。
   *
   * ⚠️ 键是**作用域**（2026-10-15 从 region 升级）：三个平台互不相通的模型目录各自分家，
   * 同区域的 WB / Trae / Qoder 不再共用一把键。懒加载 ⇒ `Partial`（未拉取的作用域为 `undefined`）。
   *
   * ⚠️ 用 `Set` 是为了 O(1) 命中；更新时**必须换一个新 `Set`**（zustand 靠引用变化触发重渲染）。
   */
  disabled: Partial<Record<Scope, Set<string>>>;
  strategies: Record<Region, AccountStrategyView>;
  logs: GatewayLogEntry[];
  /**
   * 最近一次（或进行中）的**限流探测作业**（方案 §3.1）。
   *
   * `null` = 从未跑过 / 已被换作用域清掉（后端以 `{ phase: "idle" }` 表示）。
   * 与后端 `ProbeJob` 的 wire 形状**逐字段一致**（snake_case，见 `model_probe.rs`）。
   */
  probe: ModelProbeJob | null;
  /**
   * **被动底色（来源之一）**：客户端限流台账（`client_limits`）里「仍受限」的 `uid × 模型`。
   *
   * ⚠️ 这只是被动底色的**一半**；另一半是**池的模型级冷却**（`GatewayPoolAccount.modelCoolings`，
   * 随 `status` 到达）。两者在读取处由 {@link useProbePassive} **派生**成一份并集 ——
   * 消费方（探测卡 / 模型表列）**只读那份派生值**，别再各自拼一份。
   * 键是**账号 uid**（与池条目同键），内层键是**模型名**。
   */
  probePassive: Record<string, Record<string, ModelProbeState>>;
  loading: boolean;
  saving: boolean;
  error: string | null;
  lastLoadedAt: number;

  loadConfig: () => Promise<void>;
  saveConfig: (config: GatewayConfig) => Promise<void>;
  refreshStatus: () => Promise<void>;
  loadKeys: () => Promise<void>;
  createKey: (name: string, region: Region) => Promise<CreateApiKeyResult>;
  revokeKey: (id: string) => Promise<void>;
  /** ★ issue #76：更新某把 Key 的策略（完整策略，非补丁）。 */
  setKeyPolicy: (id: string, policy: api.KeyPolicy) => Promise<void>;
  /** 撤销吊销：清 `revokedAt`，Key 重新可用（后端拒绝「未吊销」的恢复）。 */
  restoreKey: (id: string) => Promise<void>;
  deleteKey: (id: string) => Promise<void>;
  loadModels: (region: Region) => Promise<void>;
  refreshModels: (region: Region) => Promise<void>;
  /** 读取某**作用域**的禁用模型集合（P11）。 */
  loadDisabled: (scope: Scope) => Promise<void>;
  /** 置位 / 清位某**作用域**的一个模型（禁用 / 启用）—— 后端幂等。 */
  setModelDisabled: (scope: Scope, modelId: string, disabled: boolean) => Promise<void>;
  loadStrategies: () => Promise<void>;
  saveStrategy: (region: Region, strategy: AccountStrategy) => Promise<void>;
  loadLogs: () => Promise<void>;
  clearLogs: () => Promise<void>;
  /** 启动一次限流探测（后端「作业 + 轮询」；第二次调用返回「已有探测进行中」）。 */
  startProbe: (args: {
    scope: Scope;
    uids?: string[];
    models?: string[];
    adopt: boolean;
  }) => Promise<{ started: boolean; total: number }>;
  /** 拉一次作业快照；`running` 时自动续轮询，`done` / `cancelled` 时清定时器。 */
  refreshProbeStatus: (scope: Scope) => Promise<void>;
  /** 取消剩余格（保留已探到的部分结果）。 */
  cancelProbe: (scope: Scope) => Promise<void>;
  /** 读取该 region 的**被动底色**（客户端台账；失败静默为空表）。 */
  loadProbePassive: (region: Region) => Promise<void>;
  /** 停掉轮询定时器（卡片卸载时调用，防泄漏）。 */
  stopProbePolling: () => void;
  /** 首次进入页面：并发拉取配置 / 状态 / Keys / 策略 / 日志 / 两版模型。 */
  loadAll: () => Promise<void>;
}

export const useGatewayStore = create<GatewayState>((set, get) => ({
  config: DEFAULT_GATEWAY_CONFIG,
  status: null,
  keys: [],
  models: { cn: null, global: null },
  // 默认全开（D27）：各作用域的 `disabled` 懒加载，未拉取即视为空集，不替用户做决定。
  disabled: {},
  strategies: defaultStrategies(),
  logs: [],
  // 探测未跑过（后端 `{ phase: "idle" }` 归一为 `null`）；被动底色懒加载，默认空表。
  probe: null,
  probePassive: {},
  loading: false,
  saving: false,
  error: null,
  lastLoadedAt: 0,

  async loadConfig() {
    try {
      const config = await api.getGatewayConfig();
      set({ config: normalizeGatewayConfig(config) });
    } catch (e) {
      set({ error: api.asError(e) });
    }
  },

  async saveConfig(config) {
    set({ saving: true });
    try {
      const saved = await api.saveGatewayConfig(config);
      set({ config: normalizeGatewayConfig(saved), error: null });
      await get().refreshStatus();
    } finally {
      set({ saving: false });
    }
  },

  async refreshStatus() {
    try {
      set({ status: normalizeGatewayStatus(await api.gatewayStatus()) });
    } catch (e) {
      set({ status: null, error: api.asError(e) });
    }
  },

  async loadKeys() {
    try {
      const { keys } = await api.listApiKeys();
      set({ keys: keys ?? [] });
    } catch (e) {
      set({ error: api.asError(e) });
    }
  },

  async createKey(name, region) {
    const result = await api.createApiKey(name, region);
    if (result.record) {
      set((s) => ({ keys: [result.record!, ...s.keys.filter((key) => key.id !== result.record!.id)] }));
    } else {
      await get().loadKeys();
    }
    // ★ 「接入即识别」（P10 §1.6 / T03）：创建 Key 后**顺手拉一次**该 region 快照。
    // 后端在 create_api_key 成功后 fire-and-forget 探测（可能此刻尚未完成），故这里只是
    // **best-effort**：拿到的是「当前」快照（探测已写 live 就是实时，否则仍是缓存 / 内置）。
    // 真正的定论发生在下次 `loadModels`（进入模型页 / 手动「重新识别」）—— 与 §3.7 时序一致。
    // 失败静默：`loadModels` 内部已 catch（绝不 reject），不会污染「Key 已创建」的成功语义。
    await get().loadModels(region);
    return result;
  },

  async revokeKey(id) {
    await api.revokeApiKey(id);
    await get().loadKeys();
  },

  async restoreKey(id) {
    await api.restoreApiKey(id);
    await get().loadKeys();
  },

  async deleteKey(id) {
    await api.deleteApiKey(id);
    await get().loadKeys();
  },

  async setKeyPolicy(id, policy) {
    await api.setKeyPolicy(id, policy);
    // 保存后**重新拉列表**：策略写进台账，列表要反映新值（含掩码视图的七件套）。
    await get().loadKeys();
  },

  async loadModels(region) {
    try {
      const snapshot = await api.getGatewayModels(region);
      set((s) => ({ models: { ...s.models, [region]: snapshot } }));
    } catch (e) {
      set({ error: api.asError(e) });
    }
  },

  async refreshModels(region) {
    const snapshot = await api.refreshGatewayModels(region);
    set((s) => ({ models: { ...s.models, [region]: snapshot } }));
  },

  async loadDisabled(scope) {
    try {
      const { ids } = await api.getDisabledModels(scope);
      set((s) => ({ disabled: { ...s.disabled, [scope]: new Set(ids ?? []) } }));
    } catch (e) {
      set({ error: api.asError(e) });
    }
  },

  async setModelDisabled(scope, modelId, disabled) {
    // 先落库（后端幂等），成功后再更新内存 —— 失败时集合保持不变，行不会「假装切了」。
    await api.setModelDisabled(scope, modelId, disabled);
    set((s) => {
      const next = new Set(s.disabled[scope]);
      if (disabled) next.add(modelId);
      else next.delete(modelId);
      return { disabled: { ...s.disabled, [scope]: next } };
    });
  },

  async loadStrategies() {
    try {
      const map = await api.getAccountStrategy();
      set({ strategies: normalizeStrategies(map) });
    } catch (e) {
      set({ error: api.asError(e) });
    }
  },

  async saveStrategy(region, strategy) {
    await api.saveAccountStrategy(region, strategy);
    await get().loadStrategies();
  },

  async loadLogs() {
    try {
      const { logs } = await api.getGatewayLogs();
      set({ logs: [...(logs ?? [])].sort((a, b) => b.ts - a.ts) });
    } catch (e) {
      set({ error: api.asError(e) });
    }
  },

  async clearLogs() {
    await api.clearGatewayLogs();
    set({ logs: [] });
  },

  async startProbe(args) {
    const result = await api.probeStart(args);
    // 启动后立刻拉一次快照（`running` 会在 `refreshProbeStatus` 里起轮询）。
    await get().refreshProbeStatus(args.scope);
    return result;
  },

  async refreshProbeStatus(scope) {
    try {
      const job = await api.probeStatus(scope);
      // 后端用 `{ phase: "idle" }` 表示「从未跑过」⇒ 归一成 `null`（其余档字段齐全）。
      const next =
        job && (job as ModelProbeJob).phase && (job as ModelProbeJob).phase !== "idle"
          ? (job as ModelProbeJob)
          : null;
      set({ probe: next });
      if (next && next.phase === "running") {
        // `running` ⇒ 续轮询（`setInterval` 单实例；已在跑时先清再起，杜绝并发）。
        stopProbeTimer();
        probeTimer = setInterval(() => {
          void get().refreshProbeStatus(scope);
        }, PROBE_POLL_MS);
      } else {
        // `done` / `cancelled`（或 idle）⇒ 必须清掉定时器，否则会一直空转。
        stopProbeTimer();
      }
    } catch (e) {
      stopProbeTimer();
      set({ error: api.asError(e) });
    }
  },

  async cancelProbe(scope) {
    try {
      await api.probeCancel(scope);
    } finally {
      // 取消后立即对齐一次：后端会把 phase 置 `cancelled` 并保留部分结果。
      await get().refreshProbeStatus(scope);
    }
  },

  async loadProbePassive(region) {
    try {
      const res = await api.clientLimits(region);
      const map: Record<string, Record<string, ModelProbeState>> = {};
      for (const entry of res.limits ?? []) {
        // 只认**仍受限**（`active`）的条目：`unknown` 推不出「仍受限」，`recovered` 已恢复。
        if (entry.state !== "active") continue;
        const uid = entry.accountUid;
        const model = entry.model;
        if (!uid || !model) continue;
        (map[uid] ??= {})[model] = "rate_limited_model";
      }
      set({ probePassive: map });
    } catch {
      // 被动底色是**加分项**：拉不到就当空表，绝不让它污染主流程的错误条。
      set({ probePassive: {} });
    }
  },

  stopProbePolling() {
    stopProbeTimer();
  },

  async loadAll() {
    set({ loading: true, error: null });
    try {
      await Promise.all([
        get().loadConfig(),
        get().refreshStatus(),
        get().loadKeys(),
        get().loadStrategies(),
        get().loadLogs(),
        get().loadModels("cn"),
        get().loadModels("global"),
        /* 禁用偏好按**作用域**键控 ⇒ 六个作用域全拉（每个只是一次小 GET），
           任一平台的模型页进入时都已有数据，不必各自等待。 */
        ...SCOPES.map((scope) => get().loadDisabled(scope)),
      ]);
      set({ lastLoadedAt: Date.now() });
    } finally {
      set({ loading: false });
    }
  },
}));

/**
 * 被动底色的**来源**（§5.3 角标「真实流量」/「客户端日志」—— 两者必须能区分）。
 *
 * - `flow` = 池的**模型级冷却**（`accounts[].modelCoolings`）—— 经网关**真实流量**打出来的；
 * - `ledger` = **客户端限流台账**（`client_limits`）—— 只是「客户端日志里出现过」。
 */
export type ProbePassiveSource = "flow" | "ledger";

/**
 * 被动底色的**一格**：状态 + **来源**。
 *
 * ⚠️ 两个来源语义不同（一个是网关实测、一个只是客户端日志推断），此前被并成一档只发一个
 * 状态串 ⇒ 用户分不出。这里把来源一并带出，消费方（探测卡矩阵 / 模型表列）**读同一份**。
 */
export interface ProbePassiveInfo {
  /** 被动状态（池里只有模型级冷却一档；台账也记这一档）。 */
  state: ModelProbeState;
  /** 来源 —— **两来源都有时以 `flow`（真实流量）优先**。 */
  source: ProbePassiveSource;
  /** 两来源**同时**有记录（`source` 仍以 flow 优先，文案据此说明二者皆有）。 */
  both: boolean;
}

/**
 * 被动底色的**唯一合并口径**（纯函数，可测）：客户端限流台账 ∪ 池的模型级冷却。
 *
 * - 客户端台账（`ledger` = `probePassive`，来自 `client_limits`）：键 `uid → 模型名 → 状态`，
 *   来源记 `ledger`（客户端日志）；
 * - 池冷却（`accounts[].modelCoolings`，camelCase `{模型名: 解冻ms}`）：后端只发未过期的，
 *   这里**再判一次** `until_ms > now`（防御 + 便于单测），一律记 `rate_limited_model`
 *  （池里只有模型级冷却这一档），来源记 `flow`（真实流量）。
 *
 * ⚠️ **两来源同一格时 `flow` 优先**（`source: "flow"` + `both: true`）—— 真实流量是更强的证据；
 * `both` 让文案能说明「客户端日志里也有」。
 *
 * ⚠️ 这是**派生**，不落 state —— 存成独立 state 会与两个来源脱节（换来源不跟进）。
 * 读取处走 {@link useProbePassive}（两来源任一变化即重算）。
 */
export function unionProbePassive(
  accounts: readonly { uid: string; modelCoolings?: Record<string, number> }[],
  ledger: Record<string, Record<string, ModelProbeState>>,
): Record<string, Record<string, ProbePassiveInfo>> {
  const out: Record<string, Record<string, ProbePassiveInfo>> = {};
  for (const uid of Object.keys(ledger)) {
    const models: Record<string, ProbePassiveInfo> = {};
    for (const [model, state] of Object.entries(ledger[uid])) {
      models[model] = { state, source: "ledger", both: false };
    }
    out[uid] = models;
  }
  const now = Date.now();
  for (const account of accounts) {
    for (const [model, untilMs] of Object.entries(account.modelCoolings ?? {})) {
      if (typeof untilMs !== "number" || untilMs <= now) continue;
      const bucket = (out[account.uid] ??= {});
      bucket[model] = {
        state: "rate_limited_model",
        source: "flow",
        // 台账里也有同一格 ⇒ 两来源都有（状态仍以真实流量为准）。
        both: bucket[model] !== undefined,
      };
    }
  }
  return out;
}

/**
 * **派生**的被动底色（卡片矩阵与模型表「限流（探测）」列**共用同一份**）。
 *
 * 订阅两个来源（`probePassive` 台账 + `status` 池冷却），任一变化即 `useMemo` 重算 ⇒
 * 二者**必然同时**跟着变，不会两处各拼一份、也不会与来源脱节。
 *
 * @param region 只取该区域池条目的冷却（Trae / Qoder 无 `modelCoolings` ⇒ 缺省空）。
 */
export function useProbePassive(region: Region): Record<string, Record<string, ProbePassiveInfo>> {
  const ledger = useGatewayStore((s) => s.probePassive);
  const status = useGatewayStore((s) => s.status);
  return useMemo(
    () => unionProbePassive(poolOf(status, region).accounts, ledger),
    [ledger, status, region],
  );
}
