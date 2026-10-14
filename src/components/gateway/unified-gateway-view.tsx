/**
 * 统一网关（第三个产品线）**跨作用域**的管理页 —— 网关页子导航第 7 项「统一入口」。
 *
 * 设计依据：`docs/redesign/UNIFIED-GATEWAY-UI.md`（三张卡 / 状态矩阵 / 危险动作分档）。
 * 本组件落地**全部三张卡**：
 *   ① 统一网关（开关 / 监听 / 端口 / 命名空间启用集，按平台分组）
 *   ② 统一密钥（列表 + 创建 + 吊销 / 恢复 / 删除）
 *   ③ 命名空间与清单预览（逐段给出来源与原因 —— 非实时必有原因，一个总徽标糊过去是不允许的）
 *
 * ## ★ 它是**跨作用域**的
 *
 * 与「请求日志不做版本过滤」同性质：统一入口把两个产品线的模型合成一个入口，
 * 因此**上方的作用域选择器对它不生效**（卡 1 顶部有一行明确说明，否则用户切作用域没变化会当成 bug）。
 * 也因此这些命令**不进** `SCOPE_PARAMETERIZED`。
 *
 * ## 为什么状态放在组件内（`useState`）而不是 zustand
 *
 * 本页是**单点消费**（只有这一处渲染），且数据全部来自两条只读命令 + 三条写命令；
 * 引入一个 store 只会多一层需要同步的间接层。等出现第二个消费者时再提取 store
 * —— 与「命名空间镜像表」同一个判断标准。
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { Copy, KeyRound, Loader2, Plus, RotateCcw, Trash2 } from "lucide-react";
import { toast } from "sonner";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { EmptyState } from "@/components/ui/empty-state";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import * as api from "@/lib/api";
import { copyText } from "@/lib/clipboard";
import { formatDate } from "@/lib/format";
import { useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";

/* ─────────── 命名空间镜像表 ───────────
   与 `crates/buddy-switch-core/src/modules/gateway_namespace.rs` 的**表逐条对齐**。
   ⚠️ 只取**展示所需**的字段（分组用的平台名 + 客户端/程序位提示）；解析与合法性判定
   一律由后端负责 —— 前端**绝不**用它决定「这个前缀能不能用」（那是 fail-closed 的后端职责）。
   ⚠️ 派生层（前 4 条）由后端的 `Scope::all()` 生成；这里手写是**过渡**，等出现第二个消费者时
   提取为 `src/lib/gateway-namespaces.ts` 并加跨语言门禁（架构文档 §2.8 已登记）。 */
interface NamespaceMeta {
  id: string;
  /** 分组用（平台展示名）。 */
  platform: string;
  /** 提示用：客户端名 + 程序位。 */
  client: string;
  /** 是否属默认启用集（派生层）。 */
  defaultOn: boolean;
}

const NAMESPACE_META: readonly NamespaceMeta[] = [
  { id: "wb-cn", platform: "WorkBuddy", client: "WorkBuddy · 国内版", defaultOn: true },
  { id: "wb-global", platform: "WorkBuddy", client: "WorkBuddy · 国际版", defaultOn: true },
  { id: "trae-cn", platform: "Trae", client: "Trae CN（合并版）", defaultOn: true },
  { id: "trae-global", platform: "Trae", client: "Trae（合并版）", defaultOn: true },
  { id: "traework-cn", platform: "Trae", client: "TraeWork CN（老程序线）", defaultOn: false },
  { id: "traework-global", platform: "Trae", client: "TraeWork（老程序线）", defaultOn: false },
  { id: "traecode-cn", platform: "Trae", client: "TraeCode CN（遗留 IDE）", defaultOn: false },
  { id: "traecode-global", platform: "Trae", client: "TraeCode（遗留 IDE）", defaultOn: false },
];

/** 一条展示记录：镜像表里有则用它的说明，没有（后端新增了平台而前端还没跟上）则**原样显示 id** —— 不隐藏。 */
function metaOf(id: string): NamespaceMeta {
  return NAMESPACE_META.find((item) => item.id === id) ?? {
    id,
    platform: "其他",
    client: id,
    defaultOn: false,
  };
}

/** 平台展示顺序 = 镜像表里的出现顺序（新增平台时自动排到末尾，不会插队）。 */
function platformOrder(ids: readonly string[]): string[] {
  const seen: string[] = [];
  for (const id of ids) {
    const platform = metaOf(id).platform;
    if (!seen.includes(platform)) seen.push(platform);
  }
  return seen;
}

/**
 * 从密钥列表响应里取出**数组**。
 *
 * ## 为什么这里要容忍两种形状（实测事故）
 *
 * `list_unified_api_keys` 曾在桌面宿主返回**裸数组**、webui 宿主返回 `{ keys: [...] }`。
 * 前端按 `{ keys }` 取值时，拿到的不是 `undefined` 而是 **`Array.prototype.keys` 这个函数**
 * （数组的 `.keys` 是原型方法！）⇒ `setKeys(函数)` 被 React 当成 updater 调用
 * ⇒ `TypeError: Cannot convert undefined or null to object`，**整页白屏**。
 *
 * 形状现在由后端的 `unified::admin::keys_list_payload` **一处构造**保证一致；
 * 这里再做一次归一，是因为**形状漂移不该让整页白屏** —— 与「展示字段出口归一」
 * （`display_str` / `displayText`）同一条原则：脏值只该降级，不该炸树。
 */
function keysOf(payload: unknown): api.UnifiedApiKeyView[] {
  if (Array.isArray(payload)) return payload as api.UnifiedApiKeyView[];
  const inner = (payload as { keys?: unknown } | null | undefined)?.keys;
  return Array.isArray(inner) ? (inner as api.UnifiedApiKeyView[]) : [];
}

/** 统一网关工作台（跨作用域）。 */
export function UnifiedGatewayView({ className }: { className?: string }) {
  const t = useT();
  const [snapshot, setSnapshot] = useState<api.UnifiedGatewaySnapshot | null>(null);
  const [keys, setKeys] = useState<api.UnifiedApiKeyView[]>([]);
  /** 首屏加载：**只在从来没有数据时**用骨架 —— 保存后重读不该把用户正看着的配置换成骨架。 */
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [showMore, setShowMore] = useState(false);
  /** 端口输入框的**本地草稿**（提交前不写库；回车 / 失焦提交）。 */
  const [portDraft, setPortDraft] = useState("");
  const [createOpen, setCreateOpen] = useState(false);
  const [plaintext, setPlaintext] = useState<{ value: string; name: string } | null>(null);
  const [models, setModels] = useState<api.UnifiedModelsPreview | null>(null);
  /** 展开查看清单的命名空间（受控展开；不用 details/summary —— 本仓 AGENTS.md 禁止）。 */
  const [expandedNs, setExpandedNs] = useState<string[]>([]);

  const load = useCallback(async () => {
    try {
      const [next, list, preview] = await Promise.all([
        api.getUnifiedGatewayConfig(),
        api.listUnifiedApiKeys(),
        api.getUnifiedGatewayModels(),
      ]);
      setSnapshot(next);
      setKeys(keysOf(list));
      setModels(preview);
      setPortDraft(String(next.config.port));
    } catch (error) {
      toast.error(t("unifiedGateway.loadFailed"), { description: String(error) });
    } finally {
      setLoading(false);
    }
  }, [t]);

  useEffect(() => {
    void load();
  }, [load]);

  /** 保存配置：**先落库再更新内存态** —— 否则行会「假装切了」（状态矩阵里的既有约定）。 */
  const save = useCallback(
    async (patch: Partial<api.UnifiedGatewayConfigView>) => {
      if (!snapshot) return;
      const next = { ...snapshot.config, ...patch };
      setSaving(true);
      try {
        const result = await api.saveUnifiedGatewayConfig(next);
        setSnapshot({ ...snapshot, config: result.config, running: result.running, addr: result.addr });
        setPortDraft(String(result.config.port));
      } catch (error) {
        toast.error(t("unifiedGateway.saveFailed"), { description: String(error) });
        // 失败 ⇒ 回滚显示值（不留下「看起来改了」的假象）
        setPortDraft(String(snapshot.config.port));
      } finally {
        setSaving(false);
      }
    },
    [snapshot, t],
  );

  const enabledIds = snapshot?.namespaces ?? [];
  const allIds = snapshot?.allNamespaces ?? NAMESPACE_META.map((item) => item.id);
  const visibleIds = useMemo(
    () => (showMore ? allIds : allIds.filter((id) => metaOf(id).defaultOn)),
    [allIds, showMore],
  );
  const platforms = useMemo(() => platformOrder(visibleIds), [visibleIds]);
  const activeKeyCount = keys.filter((key) => !key.revoked).length;

  /** 勾选 / 取消一个命名空间。★ 守卫：**至少保留一个**（取消最后一条时给 toast 并还原）。 */
  function toggleNamespace(id: string, next: boolean) {
    if (!snapshot) return;
    if (!next && enabledIds.length <= 1) {
      toast.error(t("unifiedGateway.keepOne"));
      return;
    }
    const nextIds = next
      ? allIds.filter((item) => item === id || enabledIds.includes(item))
      : enabledIds.filter((item) => item !== id);
    void save({ namespaces: nextIds });
  }

  function commitPort() {
    if (!snapshot) return;
    const parsed = Number.parseInt(portDraft, 10);
    if (!Number.isFinite(parsed) || parsed < 1 || parsed > 65535) {
      toast.error(t("unifiedGateway.portInvalid"));
      setPortDraft(String(snapshot.config.port));
      return;
    }
    if (parsed === snapshot.config.port) return;
    void save({ port: parsed });
  }

  async function onCreateKey(name: string, namespaces: string[], bizNamespace: string | null) {
    try {
      const result = await api.createUnifiedApiKey(name, namespaces, bizNamespace);
      setCreateOpen(false);
      // 明文只在创建时返回一次 ⇒ 立刻展示，且**只给显式关闭出口**。
      // ★ 后端没返回明文时**不弹空对话框**（那会让用户以为「创建了但看不到」，实际是异常）
      if (result.key) {
        setPlaintext({ value: result.key, name });
        toast.success(t("unifiedGateway.created"));
      } else {
        toast.error(t("unifiedGateway.saveFailed"));
      }
      await load();
    } catch (error) {
      toast.error(t("unifiedGateway.saveFailed"), { description: String(error) });
    }
  }

  async function runKeyAction(action: "revoke" | "restore" | "delete", id: string) {
    try {
      if (action === "revoke") await api.revokeUnifiedApiKey(id);
      else if (action === "restore") await api.restoreUnifiedApiKey(id);
      else await api.deleteUnifiedApiKey(id);
      await load();
    } catch (error) {
      toast.error(t("unifiedGateway.saveFailed"), { description: String(error) });
    }
  }

  if (loading && !snapshot) {
    return (
      <div className={cn("space-y-4", className)}>
        <div className="h-[120px] animate-pulse rounded-xl bg-muted" />
        <div className="h-[240px] animate-pulse rounded-xl bg-muted" />
      </div>
    );
  }

  return (
    <div className={cn("space-y-4", className)}>
      {/* ── 卡 1 · 统一网关 ── */}
      <Card className="gap-0 py-0">
        <div className="flex items-center justify-between gap-3 border-b border-border/60 px-5 py-3">
          <span className="text-sm font-semibold">{t("unifiedGateway.title")}</span>
          <Badge variant={snapshot?.running ? "success" : "secondary"} className="rounded-md">
            {snapshot?.running ? t("unifiedGateway.running") : t("unifiedGateway.stopped")}
          </Badge>
        </div>

        {/* 跨作用域说明：不写这行，用户切作用域没变化会当成 bug */}
        <div className="border-b border-border/60 bg-muted/40 px-5 py-2.5 text-xs text-muted-foreground">
          {t("unifiedGateway.crossScope")}
        </div>

        {/* 摘要条：一眼看全（与两个既有网关卡同构） */}
        <div className="flex flex-wrap items-center gap-x-4 gap-y-1 border-b border-border/60 px-5 py-2.5 text-xs text-muted-foreground">
          <span>
            {t("unifiedGateway.summary", {
              namespaces: enabledIds.length,
              keys: activeKeyCount,
              addr: `${snapshot?.config.bind_addr ?? "127.0.0.1"}:${snapshot?.config.port ?? ""}`,
            })}
          </span>
        </div>

        <div className="flex items-center gap-3 border-b border-border/60 px-5 py-3">
          <div className="min-w-0 flex-1">
            <div className="text-xs font-medium">{t("unifiedGateway.enable")}</div>
            <div className="mt-0.5 text-[11.5px] text-muted-foreground">
              {t("unifiedGateway.enableHint")}
            </div>
          </div>
          <Switch
            checked={Boolean(snapshot?.config.enabled)}
            disabled={saving}
            onCheckedChange={(next) => void save({ enabled: next })}
            aria-label={t("unifiedGateway.enable")}
          />
        </div>

        <div className="flex items-center gap-3 border-b border-border/60 px-5 py-3">
          <div className="min-w-0 flex-1">
            <div className="text-xs font-medium">{t("unifiedGateway.bindAddr")}</div>
            <div className="mt-0.5 font-mono text-[11.5px] text-muted-foreground">
              {snapshot?.config.bind_addr}
            </div>
          </div>
          <Button
            variant="outline"
            size="sm"
            disabled={saving}
            onClick={() =>
              void save({
                bind_addr: snapshot?.config.bind_addr === "0.0.0.0" ? "127.0.0.1" : "0.0.0.0",
                // 开非回环必须同时置真，否则后端会拒绝（安全默认）
                allow_non_loopback: snapshot?.config.bind_addr !== "0.0.0.0",
              })
            }
          >
            {snapshot?.config.bind_addr === "0.0.0.0"
              ? t("unifiedGateway.bindLoopback")
              : t("unifiedGateway.bindLan")}
          </Button>
        </div>

        <div className="flex items-center gap-3 border-b border-border/60 px-5 py-3">
          <div className="min-w-0 flex-1">
            <div className="text-xs font-medium">{t("unifiedGateway.port")}</div>
            <div className="mt-0.5 text-[11.5px] text-muted-foreground">
              {t("unifiedGateway.portHint")}
            </div>
          </div>
          <Input
            className="w-[86px] font-mono text-xs"
            value={portDraft}
            disabled={saving}
            inputMode="numeric"
            onChange={(event) => setPortDraft(event.target.value.replace(/[^\d]/g, ""))}
            onBlur={commitPort}
            onKeyDown={(event) => {
              if (event.key === "Enter") commitPort();
            }}
            aria-label={t("unifiedGateway.port")}
          />
        </div>

        {/* 命名空间启用集：**按平台分组遍历渲染** —— 新增平台/命名空间时前端零改动 */}
        <div className="px-5 py-3">
          <div className="text-xs font-medium">{t("unifiedGateway.nsTitle")}</div>
          <div className="mt-0.5 text-[11.5px] text-muted-foreground">
            {t("unifiedGateway.nsHint")}
          </div>
          <div className="mt-3 space-y-3">
            {platforms.map((platform) => (
              <div key={platform}>
                <div className="mb-1.5 text-[11px] font-semibold text-muted-foreground">
                  {platform}
                </div>
                <div className="grid grid-cols-[repeat(auto-fill,minmax(190px,1fr))] gap-x-4 gap-y-1.5">
                  {visibleIds
                    .filter((id) => metaOf(id).platform === platform)
                    .map((id) => {
                      const meta = metaOf(id);
                      const checked = enabledIds.includes(id);
                      return (
                        <label
                          key={id}
                          className="flex cursor-pointer items-center gap-2 text-xs"
                          title={meta.client}
                        >
                          <Checkbox
                            checked={checked}
                            disabled={saving}
                            onCheckedChange={(next) => toggleNamespace(id, next === true)}
                          />
                          <code className="font-mono text-[11.5px]">{id}</code>
                          {/* 停用的命名空间在密钥表里被标红 —— 这里不给额外徽标，避免两处重复提示 */}
                        </label>
                      );
                    })}
                </div>
              </div>
            ))}
          </div>
          <Button
            variant="ghost"
            size="sm"
            className="mt-2"
            onClick={() => setShowMore((value) => !value)}
          >
            {showMore ? t("unifiedGateway.showLess") : t("unifiedGateway.showMore")}
          </Button>
        </div>
      </Card>

      {/* ── 卡 2 · 统一密钥 ── */}
      <Card className="gap-0 py-0">
        <div className="flex items-center justify-between gap-3 border-b border-border/60 px-5 py-3">
          <span className="text-sm font-semibold">{t("unifiedGateway.keysTitle")}</span>
          <Button size="sm" onClick={() => setCreateOpen(true)}>
            <Plus />
            {t("unifiedGateway.createKey")}
          </Button>
        </div>

        <div className="px-5 py-3">
          {keys.length === 0 ? (
            <EmptyState
              icon={KeyRound}
              title={t("unifiedGateway.noKeys")}
              description={t("unifiedGateway.noKeysHint")}
              className="py-4"
            />
          ) : (
            <div className="min-w-0 overflow-x-auto">
              <table className="w-full min-w-[720px] text-left text-sm">
                <thead>
                  <tr className="text-xs text-muted-foreground">
                    <th className="pb-2 pr-4 font-medium">{t("unifiedGateway.colName")}</th>
                    <th className="pb-2 pr-4 font-medium">{t("unifiedGateway.colNamespaces")}</th>
                    <th className="pb-2 pr-4 font-medium">{t("unifiedGateway.colMasked")}</th>
                    <th className="pb-2 pr-4 font-medium">{t("unifiedGateway.colCreated")}</th>
                    <th className="pb-2 pr-4 font-medium">{t("unifiedGateway.colLastUsed")}</th>
                    <th className="pb-2 pr-4 font-medium">{t("unifiedGateway.colStatus")}</th>
                    <th className="pb-2 font-medium">{t("unifiedGateway.colActions")}</th>
                  </tr>
                </thead>
                <tbody>
                  {keys.map((key) => (
                    <tr
                      key={key.id}
                      className={cn("border-t border-border/60", key.revoked && "opacity-60")}
                    >
                      <td className="py-2 pr-4 font-medium">{key.name}</td>
                      <td className="py-2 pr-4">
                        {/* 授权集里的命名空间**逐条标状态**：已停用的标红并说明（跨卡联动的落点） */}
                        <span className="flex flex-wrap gap-1">
                          {key.namespaces.map((id) => {
                            const off = !enabledIds.includes(id);
                            return (
                              <Badge
                                key={id}
                                variant={off ? "destructive" : "secondary"}
                                className="rounded-md whitespace-nowrap"
                                title={off ? t("unifiedGateway.nsOff") : undefined}
                              >
                                {id}
                              </Badge>
                            );
                          })}
                          {key.bizNamespace && (
                            <Badge variant="outline" className="rounded-md whitespace-nowrap">
                              /biz → {key.bizNamespace}
                            </Badge>
                          )}
                        </span>
                      </td>
                      <td className="py-2 pr-4 font-mono text-xs text-muted-foreground">
                        {key.suffix ? `${key.prefix}••••••••${key.suffix}` : `${key.prefix}…`}
                      </td>
                      <td className="py-2 pr-4 text-xs text-muted-foreground">
                        {formatDate(key.createdAt)}
                      </td>
                      <td className="py-2 pr-4 text-xs text-muted-foreground">
                        {key.lastUsedAt ? formatDate(key.lastUsedAt) : t("unifiedGateway.neverUsed")}
                      </td>
                      <td className="py-2 pr-4">
                        <Badge
                          variant={key.revoked ? "secondary" : "success"}
                          className="rounded-md"
                        >
                          {key.revoked ? t("unifiedGateway.revoked") : t("unifiedGateway.active")}
                        </Badge>
                      </td>
                      <td className="py-2">
                        {/* ★ 已吊销 ⇒ **两个出口都在**：恢复（可逆，同一把 Key）+ 删除（不可逆）。
                            只留删除 = 逼用户为一次误吊销删掉重建、再改所有客户端配置。恢复在前。 */}
                        {key.revoked ? (
                          <span className="flex items-center gap-1">
                            <Button
                              variant="ghost"
                              size="sm"
                              onClick={() => void runKeyAction("restore", key.id)}
                            >
                              <RotateCcw />
                              {t("unifiedGateway.restore")}
                            </Button>
                            <Button
                              variant="ghost"
                              size="sm"
                              className="text-destructive hover:text-destructive"
                              onClick={() => void runKeyAction("delete", key.id)}
                            >
                              <Trash2 />
                              {t("unifiedGateway.delete")}
                            </Button>
                          </span>
                        ) : (
                          <Button
                            variant="ghost"
                            size="sm"
                            onClick={() => void runKeyAction("revoke", key.id)}
                          >
                            {t("unifiedGateway.revoke")}
                          </Button>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      </Card>

      {/* ── 卡 3 · 命名空间与清单预览 ──
          逐段给出**来源与原因**（非实时必有原因）—— 一个总徽标糊过去是不允许的：
          用户要能看出哪段是实时、哪段是兜底、为什么。 */}
      <Card className="gap-0 py-0">
        <div className="flex items-center justify-between gap-3 border-b border-border/60 px-5 py-3">
          <span className="text-sm font-semibold">{t("unifiedGateway.previewTitle")}</span>
          <span className="text-xs text-muted-foreground">
            {t("unifiedGateway.previewTotal", { count: models?.data.length ?? 0 })}
          </span>
        </div>
        <div className="divide-y divide-border/60">
          {enabledIds.length === 0 && (
            <div className="px-5 py-4 text-xs text-muted-foreground">
              {t("unifiedGateway.previewEmpty")}
            </div>
          )}
          {enabledIds.map((id) => {
            const meta = metaOf(id);
            const catalog = models?.namespaces[id];
            const expanded = expandedNs.includes(id);
            const rows = (models?.data ?? []).filter((row) => row["namespace"] === id);
            return (
              <div key={id} className="px-5 py-3">
                <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                  <code className="font-mono text-xs font-medium">{id}</code>
                  <span className="text-[11.5px] text-muted-foreground" title={meta.client}>
                    {meta.client}
                  </span>
                  {catalog && (
                    <Badge
                      variant={
                        catalog.source === "live"
                          ? "success"
                          : catalog.source === "cached"
                            ? "secondary"
                            : "destructive"
                      }
                      className="rounded-md"
                    >
                      {t(`unifiedGateway.source.${catalog.source}` as "unifiedGateway.source.live")}
                    </Badge>
                  )}
                  <span className="text-[11.5px] text-muted-foreground">
                    {t("unifiedGateway.previewCount", { count: rows.length })}
                  </span>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() =>
                      setExpandedNs((list) =>
                        list.includes(id) ? list.filter((item) => item !== id) : [...list, id],
                      )
                    }
                  >
                    {expanded ? t("unifiedGateway.hideModels") : t("unifiedGateway.showModels")}
                  </Button>
                </div>
                {/* ★ 非实时必有原因：把接口给的原因**原样透出**，不在这里另编一套解释 */}
                {catalog?.note && (
                  <div className="mt-1 text-[11.5px] text-muted-foreground">{catalog.note}</div>
                )}
                {expanded && (
                  <div className="mt-2 flex flex-wrap gap-1.5">
                    {rows.length === 0 ? (
                      <span className="text-[11.5px] text-muted-foreground">
                        {t("unifiedGateway.previewNoModels")}
                      </span>
                    ) : (
                      rows.map((row) => (
                        <code
                          key={String(row["id"])}
                          className="rounded-md border border-border bg-muted/40 px-1.5 py-0.5 font-mono text-[11px]"
                        >
                          {String(row["id"])}
                        </code>
                      ))
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </Card>

      <CreateKeyDialog
        open={createOpen}
        onOpenChange={setCreateOpen}
        namespaces={allIds}
        onConfirm={onCreateKey}
      />

      {/* 一次性明文：**拦截遮罩与 ESC** —— 明文不可复原（库里只有 sha256），
          误关一次就得吊销重建 + 改所有客户端配置。关闭只剩显式按钮。 */}
      <Dialog open={plaintext !== null} onOpenChange={() => undefined}>
        <DialogContent
          className="sm:max-w-md"
          onInteractOutside={(event) => event.preventDefault()}
          onEscapeKeyDown={(event) => event.preventDefault()}
        >
          <DialogHeader>
            <DialogTitle>{t("unifiedGateway.created")}</DialogTitle>
            <DialogDescription>{t("unifiedGateway.createdHint")}</DialogDescription>
          </DialogHeader>
          {plaintext && (
            <div className="flex items-center gap-2 rounded-lg border border-border bg-muted/40 px-3 py-2.5">
              <code className="min-w-0 flex-1 select-all break-all font-mono text-xs">
                {plaintext.value}
              </code>
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  // 复制按钮**不禁用**：用户可能要粘到多个客户端 / 先存进密码管理器
                  void copyText(plaintext.value);
                  toast.success(t("shared.clipboard.copied"));
                }}
              >
                <Copy />
                {t("unifiedGateway.copy")}
              </Button>
            </div>
          )}
          <DialogFooter>
            <Button onClick={() => setPlaintext(null)}>{t("unifiedGateway.savedClose")}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

/** 创建统一密钥（名称 + 命名空间多选 + 业务转发单选）。 */
function CreateKeyDialog({
  open,
  onOpenChange,
  namespaces,
  onConfirm,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  namespaces: readonly string[];
  onConfirm: (name: string, namespaces: string[], bizNamespace: string | null) => Promise<void>;
}) {
  const t = useT();
  const [name, setName] = useState("");
  const [selected, setSelected] = useState<string[]>([]);
  const [biz, setBiz] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // 每次打开都重置：上一次的残留选择会让「新建」意外继承旧授权
  useEffect(() => {
    if (!open) return;
    setName("");
    setSelected([]);
    setBiz(null);
  }, [open]);

  /** 业务转发候选 = **已勾选里属于 WorkBuddy 的那些**（Trae 业务域不可配）。 */
  const bizOptions = selected.filter((id) => metaOf(id).platform === "WorkBuddy");

  function toggleNamespace(id: string, next: boolean) {
    const updated = next ? [...selected, id] : selected.filter((item) => item !== id);
    setSelected(updated);
    // 业务转发必须落在仍被授权的 WB 命名空间里 ⇒ 失效就清空
    if (biz && !updated.includes(biz)) setBiz(null);
  }

  async function submit() {
    if (selected.length === 0) return;
    setBusy(true);
    try {
      await onConfirm(name.trim() || t("unifiedGateway.unnamed"), selected, biz);
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t("unifiedGateway.createKey")}</DialogTitle>
          <DialogDescription>{t("unifiedGateway.noKeysHint")}</DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="unified-key-name">{t("unifiedGateway.keyName")}</Label>
            <Input
              id="unified-key-name"
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder={t("unifiedGateway.keyNamePlaceholder")}
              spellCheck={false}
              autoComplete="off"
            />
          </div>

          <div className="space-y-2">
            <Label>{t("unifiedGateway.authorize")}</Label>
            <div className="space-y-3">
              {platformOrder(namespaces).map((platform) => (
                <div key={platform}>
                  <div className="mb-1.5 text-[11px] font-semibold text-muted-foreground">
                    {platform}
                  </div>
                  <div className="grid grid-cols-[repeat(auto-fill,minmax(190px,1fr))] gap-x-4 gap-y-1.5">
                    {namespaces
                      .filter((id) => metaOf(id).platform === platform)
                      .map((id) => (
                        <label
                          key={id}
                          className="flex cursor-pointer items-center gap-2 text-xs"
                          title={metaOf(id).client}
                        >
                          <Checkbox
                            checked={selected.includes(id)}
                            onCheckedChange={(next) => toggleNamespace(id, next === true)}
                          />
                          <code className="font-mono text-[11.5px]">{id}</code>
                        </label>
                      ))}
                  </div>
                </div>
              ))}
            </div>
            {selected.length === 0 && (
              <p className="text-[11.5px] text-muted-foreground">
                {t("unifiedGateway.authorizeRequired")}
              </p>
            )}
          </div>

          <div className="space-y-2">
            <Label htmlFor="unified-key-biz">{t("unifiedGateway.bizTitle")}</Label>
            <Select
              value={biz ?? "__none__"}
              onValueChange={(value) => setBiz(value === "__none__" ? null : value)}
            >
              <SelectTrigger id="unified-key-biz" className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="__none__">{t("unifiedGateway.bizNone")}</SelectItem>
                {bizOptions.map((id) => (
                  <SelectItem key={id} value={id}>
                    {id}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <p className="text-[11.5px] text-muted-foreground">{t("unifiedGateway.bizHint")}</p>
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)} disabled={busy}>
            {t("unifiedGateway.cancel")}
          </Button>
          <Button onClick={() => void submit()} disabled={busy || selected.length === 0}>
            {busy && <Loader2 className="animate-spin" />}
            {t("unifiedGateway.create")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
