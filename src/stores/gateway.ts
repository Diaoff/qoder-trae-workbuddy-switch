import { create } from "zustand";

import * as api from "@/lib/api";
import {
  DEFAULT_GATEWAY_CONFIG,
  normalizeGatewayConfig,
  normalizeGatewayStatus,
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
