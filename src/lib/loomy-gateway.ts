/**
 * Loomy 网关字段归一化（与 `@/lib/gateway` / `trae-gateway` / `qoder-gateway` 平行的第四套）。
 *
 * ## 为什么又是独立一份
 *
 * 与另三套的模块头同一条理由：四套网关的类型**没有共用字段**
 * （归属轴 / Key 库形状 / 清单来源都不同），混在一个文件里只会让
 * 「这个 helper 是给谁用的」变成需要读实现才能回答的问题。
 *
 * ## 序列化风格
 *
 * Rust 侧 `LoomyGatewayConfig` 与状态视图都是 **snake_case**（与另三条刻意一致），
 * 因此这里以 snake_case 为准，camelCase 只作防御性兜底。
 *
 * ## 与 Qoder 侧的**两处形状差异**
 *
 * 1. **没有归属轴**：Loomy 单作用域 `loomy-cn` ⇒ 状态视图里**没有 `region`**，
 *    Key 记录里也**没有 `region` 字段**（数据里多一列恒值就是假字段）。
 * 2. **没有 `maxContext` 开关**：Loomy 的清单条目只带一个 `context_length`
 *    （上游没有「默认档 / 上限档」两档），少一个开关就少一处可能说谎的控件。
 */

import type { GatewayPoolSummary } from "@/lib/types";
import type { TraeGatewayLogEntry } from "@/lib/trae-types";
import { normalizeTraeGatewayLogs } from "@/lib/trae-gateway";

/** 默认配置（与 Rust `LoomyGatewayConfig::default` 对齐；默认**关**，与另两条产品线同口径）。 */
export const DEFAULT_LOOMY_GATEWAY_CONFIG: LoomyGatewayConfig = {
  enabled: false,
  bindAddr: "127.0.0.1",
  port: 57896,
  allowNonLoopback: false,
  logKeep: 200,
  logBodies: false,
  maxBodyMb: 0,
  // 空串 = 由清单第一条兜底（不硬编码一个可能不存在的模型名）。
  defaultModel: "",
  maxRotate: 3,
  // 空串 = 不指定账号（自动择优 / 会话粘性）。
  preferredUid: "",
  stickyTtlMs: 30 * 60 * 1000,
};

/** 网关配置（前端形状）。 */
export interface LoomyGatewayConfig {
  enabled: boolean;
  bindAddr: string;
  port: number;
  allowNonLoopback: boolean;
  logKeep: number;
  logBodies: boolean;
  maxBodyMb: number;
  defaultModel: string;
  maxRotate: number;
  preferredUid: string;
  stickyTtlMs: number;
}

/** 配置的线上形状（snake_case；写回时用）。 */
export interface LoomyGatewayConfigRaw {
  enabled: boolean;
  bind_addr: string;
  port: number;
  allow_non_loopback: boolean;
  log_keep: number;
  log_bodies: boolean;
  max_body_mb: number;
  default_model: string;
  max_rotate: number;
  preferred_uid: string;
  sticky_ttl_ms: number;
}

/**
 * 池计数 —— 直接用共用形状 `GatewayPoolSummary`。
 *
 * ⚠️ **不扩展**：Loomy 池没有模型级冷却轴（上游 429 是账号级的），
 * 也没有积分建模（额度是独立只读视图）⇒ 共用形状已经把 Loomy 的真实状态说全。
 * 多带一格恒 0 的字段只会让卡片多一个永远不动的数字（假字段）。
 */
export type LoomyGatewayPoolSummary = GatewayPoolSummary;

/** 一条池内账号（与共用池卡 `GatewayPoolAccount` 同形）。 */
export interface LoomyGatewayPoolAccount {
  uid: string;
  name: string;
  status: "available" | "cooling" | "disabled";
  /** Loomy 池不建模积分 ⇒ 恒 `null`（卡片显示「—」）。 */
  credits: number | null;
  cooling: boolean;
  cooldownUntil: number | null;
  cooldownReason: string | null;
  /** `true` = **需人工干预**（无凭据 / 凭据失效），「恢复」出口据此渲染。 */
  disabled: boolean;
}

/** 状态视图。 */
export interface LoomyGatewayStatus {
  enabled: boolean;
  running: boolean;
  addr: string | null;
  baseUrl: string;
  bindAddr: string;
  port: number;
  allowNonLoopback: boolean;
  version: string;
  totalRequests: number;
  lastError: string | null;
  apiKeyPrefix: string;
  pool: LoomyGatewayPoolSummary;
  accounts: LoomyGatewayPoolAccount[];
  diagnose: string[];
  /** 上游基址（管理面要写出请求实际打到哪）。 */
  upstream: string;
}

/** 清单来源三级（与后端 `CatalogView::source` 逐字对齐）。 */
export type LoomyCatalogSource = "live" | "cached" | "unavailable";

/** 一条上游模型（管理面全量清单，**未过滤**禁用的）。 */
export interface LoomyGatewayModelRow {
  id: string;
  /** 展示名（后端 `display_name`，缺失时回落 id）。 */
  name: string;
  /** Loomy 上游不声明「是否推理模型」⇒ 恒 `false`（不编造）。 */
  isReasoning: boolean;
  /** 上下文窗口（上游只有一个值；无两档）。 */
  maxInputTokens: number | null;
  contextDefault: number | null;
  contextMax: number | null;
  /** 后端现算的一份禁用标记（渲染以 store 为准；这里用于交叉校验与计数）。 */
  disabled: boolean;
}

/** 模型清单视图（含来源与原因）。 */
export interface LoomyGatewayCatalog {
  source: LoomyCatalogSource;
  /** 上次成功拉取时刻（毫秒）；`unavailable` 时 `null`。 */
  fetchedAt: number | null;
  /** 拉取失败 / 无账号时的可读原因；`live` 时 `null`。 */
  note: string | null;
  rows: LoomyGatewayModelRow[];
}

/** 一条 API Key（列表脱敏返回；**永不含明文与哈希**）。 */
export interface LoomyApiKeyRecord {
  id: string;
  name: string;
  /** 前缀（脱敏展示用），如 `sk-loomy-a1b2`。 */
  prefix: string;
  suffix: string;
  createdAt: number;
  revokedAt: number | null;
  revoked: boolean;
  lastUsedAt: number | null;
}

/**
 * 请求日志条目 —— 形状与 Trae / WB / Qoder **逐字相同**（字段名是四平台刻意对齐的，
 * 见 Rust 侧四处 `record_request` 的注释）⇒ 直接复用 Trae 侧的类型与归一化。
 *
 * ⚠️ **不带 `region`**：Loomy 单作用域，一个监听只服务一本账号库 ——
 * 加一个恒为 `"cn"` 的字段就是假字段（与 Qoder 刻意相反：那边真的有两个区域）。
 */
export type LoomyGatewayLogEntry = TraeGatewayLogEntry;

function asString(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function asNumber(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

function asBoolean(value: unknown): boolean | undefined {
  return typeof value === "boolean" ? value : undefined;
}

/** 取 camelCase 优先、snake_case 兜底的字段。 */
function pick<T>(record: Record<string, unknown>, camel: string, snake: string): T | undefined {
  return (record[camel] ?? record[snake]) as T | undefined;
}

/** 归一化 `get_loomy_gateway_config` 的返回。 */
export function normalizeLoomyGatewayConfig(raw: unknown): LoomyGatewayConfig {
  const record = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const read = (camel: string, snake: string) => record[camel] ?? record[snake];
  return {
    enabled: asBoolean(record.enabled) ?? DEFAULT_LOOMY_GATEWAY_CONFIG.enabled,
    bindAddr: asString(read("bindAddr", "bind_addr")) ?? DEFAULT_LOOMY_GATEWAY_CONFIG.bindAddr,
    port: asNumber(record.port) ?? DEFAULT_LOOMY_GATEWAY_CONFIG.port,
    allowNonLoopback:
      asBoolean(read("allowNonLoopback", "allow_non_loopback")) ??
      DEFAULT_LOOMY_GATEWAY_CONFIG.allowNonLoopback,
    logKeep: asNumber(read("logKeep", "log_keep")) ?? DEFAULT_LOOMY_GATEWAY_CONFIG.logKeep,
    logBodies: asBoolean(read("logBodies", "log_bodies")) ?? DEFAULT_LOOMY_GATEWAY_CONFIG.logBodies,
    maxBodyMb: asNumber(read("maxBodyMb", "max_body_mb")) ?? DEFAULT_LOOMY_GATEWAY_CONFIG.maxBodyMb,
    // 空串是**合法值**（= 由清单第一条兜底）⇒ `asString` 对空串返回 undefined 时
    // 必须回落到空串，而不是某个「默认模型」（那会让用户以为配置里写了它）。
    defaultModel:
      asString(read("defaultModel", "default_model")) ?? DEFAULT_LOOMY_GATEWAY_CONFIG.defaultModel,
    maxRotate: asNumber(read("maxRotate", "max_rotate")) ?? DEFAULT_LOOMY_GATEWAY_CONFIG.maxRotate,
    preferredUid:
      asString(read("preferredUid", "preferred_uid")) ??
      DEFAULT_LOOMY_GATEWAY_CONFIG.preferredUid,
    stickyTtlMs:
      asNumber(read("stickyTtlMs", "sticky_ttl_ms")) ?? DEFAULT_LOOMY_GATEWAY_CONFIG.stickyTtlMs,
  };
}

/**
 * 前端形状 → 后端 snake_case。
 *
 * 后端同时接受两种写法（`#[serde(alias)]`），但**写回时统一用 snake_case**：
 * 磁盘上的配置文件应该只有一种风格。
 */
export function toLoomyGatewayConfigRaw(config: LoomyGatewayConfig): LoomyGatewayConfigRaw {
  return {
    enabled: config.enabled,
    bind_addr: config.bindAddr,
    port: config.port,
    allow_non_loopback: config.allowNonLoopback,
    log_keep: config.logKeep,
    log_bodies: config.logBodies,
    max_body_mb: config.maxBodyMb,
    default_model: config.defaultModel,
    max_rotate: config.maxRotate,
    preferred_uid: config.preferredUid,
    sticky_ttl_ms: config.stickyTtlMs,
  };
}

/** 归一化 `loomy_gateway_status` 的返回。非法值 ⇒ `null`（调用方回退到「未启动」空态）。 */
export function normalizeLoomyGatewayStatus(raw: unknown): LoomyGatewayStatus | null {
  if (!raw || typeof raw !== "object") return null;
  const record = raw as Record<string, unknown>;
  const pool = (record.pool && typeof record.pool === "object" ? record.pool : {}) as Record<
    string,
    unknown
  >;
  const accounts = Array.isArray(record.accounts)
    ? record.accounts
        .map((item): LoomyGatewayPoolAccount | null => {
          if (!item || typeof item !== "object") return null;
          const entry = item as Record<string, unknown>;
          const uid = asString(entry.uid);
          if (!uid) return null;
          const status = asString(entry.status) ?? "available";
          return {
            uid,
            name: asString(entry.name) ?? uid,
            status:
              status === "cooling" || status === "disabled"
                ? (status as LoomyGatewayPoolAccount["status"])
                : "available",
            credits: asNumber(entry.credits) ?? null,
            cooling: asBoolean(entry.cooling) ?? false,
            cooldownUntil: asNumber(pick(entry, "cooldownUntil", "cooldown_until")) ?? null,
            cooldownReason: asString(pick(entry, "cooldownReason", "cooldown_reason")) ?? null,
            disabled: asBoolean(entry.disabled) ?? false,
          };
        })
        .filter((item): item is LoomyGatewayPoolAccount => item !== null)
    : [];
  const diagnose = Array.isArray(record.diagnose)
    ? record.diagnose.filter((item): item is string => typeof item === "string")
    : [];
  return {
    enabled: asBoolean(record.enabled) ?? false,
    running: asBoolean(record.running) ?? false,
    addr: asString(record.addr) ?? null,
    baseUrl: asString(pick(record, "baseUrl", "base_url")) ?? "",
    bindAddr: asString(pick(record, "bindAddr", "bind_addr")) ?? "127.0.0.1",
    port: asNumber(record.port) ?? DEFAULT_LOOMY_GATEWAY_CONFIG.port,
    allowNonLoopback: asBoolean(pick(record, "allowNonLoopback", "allow_non_loopback")) ?? false,
    version: asString(record.version) ?? "",
    totalRequests: asNumber(pick(record, "totalRequests", "total_requests")) ?? 0,
    lastError: asString(pick(record, "lastError", "last_error")) ?? null,
    apiKeyPrefix: asString(pick(record, "apiKeyPrefix", "api_key_prefix")) ?? "",
    pool: {
      total: asNumber(pool.total) ?? 0,
      available: asNumber(pool.available) ?? 0,
      cooling: asNumber(pool.cooling) ?? 0,
      disabled: asNumber(pool.disabled) ?? 0,
      // Loomy 池不建模积分 ⇒ 后端恒给 0；这里照实归一（不造数），卡片也不渲染这两格。
      expired: asNumber(pool.expired) ?? 0,
      zeroCredits: asNumber(pick(pool, "zeroCredits", "zero_credits")) ?? 0,
      totalCredits: asNumber(pick(pool, "totalCredits", "total_credits")) ?? 0,
    },
    accounts,
    diagnose,
    upstream: asString(record.upstream) ?? "",
  };
}

/** 归一化 `get_loomy_gateway_models` 的返回（清单 + 来源 + 原因）。 */
export function normalizeLoomyGatewayCatalog(raw: unknown): LoomyGatewayCatalog {
  const record = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const source = asString(record.source);
  const rows = Array.isArray(record.data)
    ? record.data
        .map((item): LoomyGatewayModelRow | null => {
          if (!item || typeof item !== "object") return null;
          const entry = item as Record<string, unknown>;
          const id = asString(entry.id);
          if (!id) return null;
          return {
            id,
            name: asString(pick(entry, "displayName", "display_name")) ?? id,
            isReasoning: asBoolean(pick(entry, "isReasoning", "is_reasoning")) ?? false,
            // Loomy 清单条目只有一个 `context_length`（无两档）⇒ 三格同源，
            // `maxInputTokens` 是上游那个值，另两格照实复制（前端按开关取档时不出空）。
            maxInputTokens:
              asNumber(pick(entry, "contextLength", "context_length")) ?? null,
            contextDefault:
              asNumber(pick(entry, "contextLength", "context_length")) ?? null,
            contextMax: asNumber(pick(entry, "contextLength", "context_length")) ?? null,
            disabled: asBoolean(entry.disabled) ?? false,
          };
        })
        .filter((item): item is LoomyGatewayModelRow => item !== null)
    : [];
  return {
    source:
      source === "live" || source === "cached" || source === "unavailable"
        ? (source as LoomyCatalogSource)
        : "unavailable",
    fetchedAt: asNumber(pick(record, "fetchedAt", "fetched_at")) ?? null,
    note: asString(record.note) ?? null,
    rows,
  };
}

/** 归一化 `list_loomy_api_keys` 的返回。 */
export function normalizeLoomyApiKeys(raw: unknown): LoomyApiKeyRecord[] {
  const record = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const list = Array.isArray(record.keys) ? record.keys : Array.isArray(raw) ? raw : [];
  return (list as unknown[])
    .map((item): LoomyApiKeyRecord | null => {
      if (!item || typeof item !== "object") return null;
      const entry = item as Record<string, unknown>;
      const id = asString(entry.id);
      if (!id) return null;
      return {
        id,
        name: asString(entry.name) ?? id,
        prefix: asString(entry.prefix) ?? "",
        suffix: asString(entry.suffix) ?? "",
        createdAt: asNumber(pick(entry, "createdAt", "created_at")) ?? 0,
        revokedAt: asNumber(pick(entry, "revokedAt", "revoked_at")) ?? null,
        revoked: asBoolean(entry.revoked) ?? false,
        lastUsedAt: asNumber(pick(entry, "lastUsedAt", "last_used_at")) ?? null,
      };
    })
    .filter((item): item is LoomyApiKeyRecord => item !== null);
}

/** 归一化 `get_loomy_gateway_logs` 的返回（复用 Trae 侧同一实现；无附加字段）。 */
export function normalizeLoomyGatewayLogs(raw: unknown): LoomyGatewayLogEntry[] {
  return normalizeTraeGatewayLogs(raw);
}
