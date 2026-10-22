import { useCallback, useEffect, useMemo, useState } from "react";
import { AlertTriangle, Copy, Loader2, Power, RefreshCw, ShieldAlert } from "lucide-react";
import { useSearchParams } from "react-router-dom";
import { toast } from "sonner";

import { DemoAction } from "@/components/demo-action";
import { ScopeChip, scopeLabel } from "@/components/scope-chip";
import {
  AUTO_OPTION,
  AccountPoolCard,
  LOOMY_POOL_TILES,
  RANDOM_OPTION,
} from "@/components/gateway/account-pool-card";
import { GatewayClientlessNotice } from "@/components/clientless-notice";
import { GatewayShell } from "@/components/gateway/gateway-shell";
import { UnifiedGatewayView } from "@/components/gateway/unified-gateway-view";
import {
  GW_INDEPENDENT_SECTIONS_NO_PROXY,
  parseGatewayDeepLink,
  unifiedSectionId,
  type GatewaySectionId,
  type GatewayType,
} from "@/components/gateway/gateway-subnav";
import { LoomyApiKeyTable } from "@/components/gateway/loomy-api-key-table";
import { LoomyModelList } from "@/components/gateway/loomy-model-list";
import { ModelProbeCard } from "@/components/gateway/model-probe-card";
import { TraeIntegrationGuide } from "@/components/gateway/trae-integration-guide";
import { TraeRequestLog } from "@/components/gateway/trae-request-log";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import * as api from "@/lib/api";
import { copyText } from "@/lib/clipboard";
import { useT } from "@/lib/i18n";
import {
  DEFAULT_LOOMY_GATEWAY_CONFIG,
  normalizeLoomyGatewayCatalog,
  normalizeLoomyGatewayConfig,
  normalizeLoomyGatewayLogs,
  normalizeLoomyGatewayStatus,
  toLoomyGatewayConfigRaw,
  type LoomyGatewayCatalog,
  type LoomyGatewayConfig,
  type LoomyGatewayLogEntry,
  type LoomyGatewayStatus,
} from "@/lib/loomy-gateway";
import { scopeOf } from "@/lib/scope";
import { useCachedResource } from "@/lib/use-cached-resource";
import { cn } from "@/lib/utils";

const LOOPBACK = "127.0.0.1";
const LAN = "0.0.0.0";

/**
 * Loomy 网关的子导航表：**去掉「请求代理」**。
 *
 * P12 的「C 路 base URL 重定向」只覆盖 WorkBuddy 网关端口，Loomy 侧没有对应后端
 * ⇒ 不渲染该项（缺能力不画空壳，§3.4 规则 1）。
 */
const LOOMY_GATEWAY_SUBNAV = GW_INDEPENDENT_SECTIONS_NO_PROXY;

/** 本页快照：配置 / 状态 / 模型清单 / 请求日志（一起取、一起被变更操作改写）。 */
interface ApiServiceSnapshot {
  config: LoomyGatewayConfig;
  status: LoomyGatewayStatus | null;
  catalog: LoomyGatewayCatalog;
  logs: LoomyGatewayLogEntry[];
}

/**
 * 「API 服务」页（Loomy 分区，独立端口 57896）。
 *
 * ## 与另三条网关页的关系：**结构同构、数据分家**
 *
 * 页头 / 子导航 / 卡片节奏与 WB / Trae / Qoder **刻意一致**（原型 `viewGateway`
 * 只有一套结构，按作用域换数据）。真正不同的只有：
 *
 * 1. **单作用域、无归属轴**：Key 没有「归属区域」列，创建对话框也没有区域选择器
 *    （Loomy 只有 `cn` 一个取值 —— 单选项的下拉就是假控件）。
 * 2. **没有上下文档位开关**：清单只有一个 `context_length`（见 `LoomyModelList`）。
 * 3. **与统一入口并存**：`loomy-cn:` 命名空间仍可走 57892；本页管的是 57896 这个独立监听，
 *    两者共用同一份池与日志（关掉任一个不影响另一个）。
 *
 * ⚠️ 本页**不渲染**「打开数据目录」：Loomy 没有对应能力（通用命令对 Loomy 作用域
 * 一律可诊断拒绝）—— 缺能力不画空壳。
 */
export default function LoomyApiServicePage() {
  const t = useT();
  // 单作用域：作用域**恒为** `loomy-cn`（URL 里的 region 只是路由外壳，无第二个取值）。
  const scope = scopeOf("loomy", "cn");
  const [saving, setSaving] = useState(false);
  const [clearing, setClearing] = useState(false);
  const [riskOpen, setRiskOpen] = useState(false);
  /** 模型清单「重新识别」的进行中状态（只驱动按钮转圈，不参与快照）。 */
  const [refreshingModels, setRefreshingModels] = useState(false);
  /** 正在排除 / 取消排除的 uid。 */
  const [excludingUid, setExcludingUid] = useState<string | null>(null);

  const [searchParams, setSearchParams] = useSearchParams();
  const [{ type, section }, setNav] = useState(() => parseGatewayDeepLink(searchParams));

  function writeNav(next: GatewayType, sub: GatewaySectionId) {
    setNav({ type: next, section: sub });
    const params = new URLSearchParams(searchParams);
    params.set("type", next);
    params.set("sub", sub);
    setSearchParams(params, { replace: true });
  }

  function goType(next: GatewayType) {
    writeNav(next, GW_INDEPENDENT_SECTIONS_NO_PROXY[0].id);
  }

  function goSub(id: GatewaySectionId) {
    writeNav(type, id);
  }

  /** 统一网关分区内的跳转（裸分区名 → 带 `u-` 前缀；约定只在 `unifiedSectionId` 里写一次）。 */
  function goUnified(next: string) {
    goSub(unifiedSectionId(next));
  }

  const loadSnapshot = useCallback(async (): Promise<ApiServiceSnapshot> => {
    const [configRaw, statusRaw, catalogRaw, logsRaw] = await Promise.all([
      api.getLoomyGatewayConfig(),
      api.loomyGatewayStatus(),
      api.getLoomyGatewayModels(),
      api.getLoomyGatewayLogs(),
    ]);
    return {
      config: normalizeLoomyGatewayConfig(configRaw),
      status: normalizeLoomyGatewayStatus(statusRaw),
      catalog: normalizeLoomyGatewayCatalog(catalogRaw),
      logs: normalizeLoomyGatewayLogs(logsRaw),
    };
  }, []);

  const {
    data: snapshot,
    loading,
    error,
    refresh: loadAll,
    patch,
  } = useCachedResource<ApiServiceSnapshot>("loomy:api-service", loadSnapshot);

  const config = snapshot?.config ?? DEFAULT_LOOMY_GATEWAY_CONFIG;
  const status = snapshot?.status ?? null;
  const catalog = snapshot?.catalog ?? {
    source: "unavailable" as const,
    fetchedAt: null,
    note: null,
    rows: [],
  };
  const logs = snapshot?.logs ?? [];

  const patchSnapshot = useCallback(
    (next: Partial<ApiServiceSnapshot>) => {
      patch((current) => ({ ...current, ...next }));
    },
    [patch],
  );

  /** 端口草稿值：与 `config.port` 同步（保存成功 / 加载完成时变）。 */
  const [portDraft, setPortDraft] = useState(() => String(config.port));
  useEffect(() => {
    setPortDraft(String(config.port));
  }, [config.port]);

  async function persist(next: Partial<LoomyGatewayConfig>) {
    const merged = { ...config, ...next };
    const previous = config;
    patchSnapshot({ config: merged });
    setSaving(true);
    try {
      await api.saveLoomyGatewayConfig(toLoomyGatewayConfigRaw(merged));
      const statusRaw = await api.loomyGatewayStatus();
      patchSnapshot({ status: normalizeLoomyGatewayStatus(statusRaw) });
      toast.success(
        merged.enabled
          ? t("loomy.gateway.savedStarted")
          : t("loomy.gateway.savedDisabled"),
      );
    } catch (e) {
      patchSnapshot({ config: previous });
      toast.error(t("loomy.gateway.saveFailed"), { description: api.asError(e) });
    } finally {
      setSaving(false);
    }
  }

  function onBindAddrChange(next: string) {
    if (next === config.bindAddr) return;
    if (next === LOOPBACK) {
      void persist({ bindAddr: LOOPBACK, allowNonLoopback: false });
      return;
    }
    setRiskOpen(true);
  }

  function confirmLan() {
    setRiskOpen(false);
    void persist({ bindAddr: LAN, allowNonLoopback: true });
  }

  function commitPort() {
    const parsed = Number.parseInt(portDraft, 10);
    if (!Number.isFinite(parsed) || parsed < 1 || parsed > 65535) {
      toast.error(t("loomy.gateway.portInvalid"), {
        description: t("loomy.gateway.portRange"),
      });
      setPortDraft(String(config.port));
      return;
    }
    if (parsed === config.port) return;
    void persist({ port: parsed });
  }

  async function onClearLogs() {
    setClearing(true);
    try {
      await api.clearLoomyGatewayLogs();
      patchSnapshot({ logs: [] });
      toast.success(t("loomy.gateway.logsCleared"));
    } catch (e) {
      toast.error(t("loomy.gateway.logsClearFailed"), { description: api.asError(e) });
    } finally {
      setClearing(false);
    }
  }

  /** 「重新识别」模型清单 —— **强制重拉**（绕过后端 10 分钟缓存）。 */
  async function onRefreshModels() {
    setRefreshingModels(true);
    try {
      const catalogRaw = await api.getLoomyGatewayModels(true);
      patchSnapshot({ catalog: normalizeLoomyGatewayCatalog(catalogRaw) });
    } catch (e) {
      toast.error(t("loomy.gateway.models.refreshFailed"), { description: api.asError(e) });
    } finally {
      setRefreshingModels(false);
    }
  }

  /**
   * 排除 / 取消排除。
   *
   * 与「恢复」不同：排除是记**用户意图**（硬约束：选号 / 兜底 / 偏好全跳过），
   * 不是修故障。Loomy 池的 `disabled` 派生自「无凭据 / 凭据失效」，
   * 唯一复原路径是重新登录（Loomy 无续期端点）—— 因此这里**不提供**「恢复」插槽，
   * 摆一个刷不动的按钮就是假控件。
   */
  async function onExclude(uid: string, excluded: boolean) {
    setExcludingUid(uid);
    try {
      await api.setLoomyGatewayPoolExclusion(uid, excluded);
      toast.success(
        excluded
          ? t("shared.gateway.pool.excludeDone")
          : t("shared.gateway.pool.unexcludeDone"),
      );
      await loadAll();
    } catch (e) {
      toast.error(t("shared.gateway.pool.excludeFail"), { description: api.asError(e) });
    } finally {
      setExcludingUid(null);
    }
  }

  const running = status?.running ?? false;
  const baseUrl = useMemo(() => {
    const host = config.bindAddr === LAN ? LOOPBACK : config.bindAddr;
    return `http://${host}:${config.port}/v1`;
  }, [config.bindAddr, config.port]);

  const keyPrefix = status?.apiKeyPrefix || "";
  /* 与另三个分区**同源口径**：
     - 「可用模型」= 网关对外清单条数（全量 − 已禁用），不能直接用 `rows.length`；
     - 「账号池」= 池条目总数（缺数据时回落到账号数）。 */
  /**
   * 限流探测卡（与另四条网关页**同一个组件**）要的**账号轴 / 模型轴**。
   *
   * ⚠️ Loomy 的池 / 清单不在 WB 网关 store 里，故这里**显式注入**自己的轴 ——
   * 「列集合 / 账号轴由作用域决定」由本页保证，组件本身**不按平台分叉**。
   */
  const probeAccounts = useMemo(
    () => (status?.accounts ?? []).map((account) => ({ uid: account.uid, name: account.name })),
    [status],
  );
  const probeModels = useMemo(
    () => catalog.rows.map((row) => ({ id: row.id, name: row.name })),
    [catalog],
  );

  const disabledCount = catalog.rows.filter((row) => row.disabled).length;
  const modelCount = Math.max(0, catalog.rows.length - disabledCount);
  const poolCount = status?.pool?.total ?? status?.accounts?.length ?? 0;

  const header = (
    <header className="mb-5 flex items-start gap-4">
      <div className="min-w-0">
        <h1 className="text-[28px] font-[640] tracking-[-0.02em]">{t("loomy.gateway.title")}</h1>
        <p className="mt-1.5 text-[12.5px] text-muted-foreground">
          {t("pageHead.gateway", { scope: scopeLabel(t, scope) })}
        </p>
      </div>
      <div className="grow" />
      <ScopeChip scope={scope} />
    </header>
  );

  if (loading && !status) {
    return (
      <div className="mx-auto w-full max-w-[1240px] px-7 pt-[26px] pb-[60px]">
        {header}
        <div className="space-y-4" aria-busy="true">
          <Skeleton className="h-[34px] w-[186px]" />
          <Skeleton className="h-32 w-full" />
          <Skeleton className="h-64 w-full" />
        </div>
      </div>
    );
  }

  return (
    <div className="mx-auto w-full max-w-[1240px] px-7 pt-[26px] pb-[60px]">
      {header}

      {error && (
        <Alert variant="destructive" className="mb-5">
          <AlertTriangle />
          <AlertTitle>{t("loomy.gateway.loadFailed")}</AlertTitle>
          <AlertDescription className="flex flex-col gap-3">
            <span>{error}</span>
            <div>
              <Button variant="outline" size="sm" onClick={() => void loadAll()}>
                <RefreshCw />
                {t("loomy.gateway.retry")}
              </Button>
            </div>
          </AlertDescription>
        </Alert>
      )}

      <GatewayClientlessNotice scope={scope} />
      <GatewayShell
        type={type}
        section={section}
        sections={LOOMY_GATEWAY_SUBNAV}
        onTypeChange={goType}
        onSelect={goSub}
      >
        {/* ---- 概览：服务状态 + 接入地址 + 接入指引 ---- */}
        {section === "overview" && (
          <>
            <Card className="mb-6 gap-0 py-0">
              <div className="flex items-center justify-between gap-3 border-b border-border/60 px-5 py-4">
                <div className="min-w-0">
                  <div className="flex items-center gap-2 text-sm font-medium">
                    <Power className="size-4 text-muted-foreground" />
                    {t("loomy.gateway.enable")}
                  </div>
                  <p className="mt-1 text-xs text-muted-foreground">
                    {t("loomy.gateway.enableDesc", {
                      upstream: status?.upstream || "loomyad.xunfei.cn",
                    })}
                  </p>
                </div>
                <div className="flex shrink-0 items-center gap-2">
                  {saving && <Loader2 className="size-3.5 animate-spin text-muted-foreground" />}
                  <DemoAction>
                    <Switch
                      checked={config.enabled}
                      disabled={saving}
                      onCheckedChange={(checked) => void persist({ enabled: checked })}
                      aria-label={t("loomy.gateway.enable")}
                    />
                  </DemoAction>
                  <DemoAction>
                    <Button variant="outline" size="sm" onClick={() => void loadAll()}>
                      <RefreshCw />
                      {t("loomy.gateway.refresh")}
                    </Button>
                  </DemoAction>
                </div>
              </div>

              <div className="flex flex-wrap items-center gap-x-6 gap-y-3 px-5 py-4">
                <div className="flex items-center gap-2">
                  <span className="text-xs text-muted-foreground">{t("loomy.gateway.bindAddr")}</span>
                  <Select value={config.bindAddr} onValueChange={onBindAddrChange} disabled={saving}>
                    <SelectTrigger size="sm" className="w-52" aria-label={t("loomy.gateway.bindAddr")}>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value={LOOPBACK}>{t("loomy.gateway.bindLoopback")}</SelectItem>
                      <SelectItem value={LAN}>{t("loomy.gateway.bindLan")}</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <div className="flex items-center gap-2">
                  <span className="text-xs text-muted-foreground">{t("loomy.gateway.port")}</span>
                  <DemoAction>
                    <Input
                      className="h-8 w-28"
                      inputMode="numeric"
                      value={portDraft}
                      onChange={(event) => setPortDraft(event.target.value.replace(/[^\d]/g, ""))}
                      onBlur={commitPort}
                      onKeyDown={(event) => {
                        if (event.key === "Enter") commitPort();
                      }}
                      aria-label={t("loomy.gateway.port")}
                    />
                  </DemoAction>
                </div>
                <span className="flex items-center gap-1.5 text-xs">
                  {t("loomy.gateway.statusLabel")}
                  <span
                    className={cn(
                      "inline-flex items-center gap-1.5 font-medium",
                      running ? "text-success" : "text-muted-foreground",
                    )}
                  >
                    <span
                      className={cn(
                        "size-2 rounded-full",
                        running ? "bg-success" : "bg-muted-foreground/50",
                      )}
                    />
                    {running ? t("loomy.gateway.running") : t("loomy.gateway.stopped")}
                  </span>
                </span>
              </div>

              <div className="flex flex-wrap items-center gap-2 border-t border-border/60 px-5 py-3 text-xs text-muted-foreground">
                {t("loomy.gateway.noteLead")}
                <code className="font-mono">/v1/chat/completions</code>
                {t("loomy.gateway.noteTail")}
                {config.bindAddr === LAN && t("loomy.gateway.lanWarning")}
              </div>

              {status?.lastError && (
                <div className="border-t border-border/60 px-5 py-3">
                  <p className="text-xs text-muted-foreground">{t("loomy.gateway.lastError")}</p>
                  <p className="mt-1 break-words text-xs text-destructive">{status.lastError}</p>
                </div>
              )}
            </Card>

            <Card className="mb-6 gap-0 py-0">
              <div className="border-b border-border/60 px-5 py-3">
                <span className="text-sm font-semibold">{t("loomy.gateway.endpoint.title")}</span>
              </div>
              <div className="px-5 py-4">
                <div className="flex flex-wrap items-center gap-3">
                  <span className="text-xs text-muted-foreground">
                    {t("loomy.gateway.endpoint.baseUrl")}
                  </span>
                  <code className="min-w-0 break-all rounded-md border border-border bg-muted/40 px-2 py-1 font-mono text-xs">
                    {baseUrl}
                  </code>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => void copyText(baseUrl, t("loomy.gateway.endpoint.copied"))}
                  >
                    <Copy />
                    {t("loomy.gateway.copy")}
                  </Button>
                </div>
                <div className="mt-1.5 flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
                  <span>{t("loomy.gateway.endpoint.keyPrefix")}</span>
                  <code className="font-mono">
                    {keyPrefix ? `${keyPrefix}…` : t("loomy.gateway.endpoint.noKey")}
                  </code>
                </div>
              </div>

              <div className="divide-y divide-border/60 border-t border-border/60">
                <div className="flex items-center justify-between gap-3 px-5 py-3">
                  <div className="min-w-0">
                    <div className="text-sm">{t("loomy.gateway.overview.models")}</div>
                    <p className="mt-0.5 text-xs text-muted-foreground">
                      {t("loomy.gateway.overview.modelsCount", { count: modelCount })}
                    </p>
                  </div>
                  <Button variant="outline" size="sm" onClick={() => goSub("models")}>
                    {t("loomy.gateway.overview.view")}
                  </Button>
                </div>
                <div className="flex items-center justify-between gap-3 px-5 py-3">
                  <div className="min-w-0">
                    <div className="text-sm">{t("loomy.gateway.overview.pool")}</div>
                    <p className="mt-0.5 text-xs text-muted-foreground">
                      {t("loomy.gateway.overview.poolCount", { count: poolCount })}
                    </p>
                  </div>
                  <Button variant="outline" size="sm" onClick={() => goSub("pool")}>
                    {t("loomy.gateway.overview.view")}
                  </Button>
                </div>
              </div>
            </Card>

            <TraeIntegrationGuide
              baseUrl={baseUrl}
              model={config.defaultModel}
              keyPrefix={keyPrefix}
              noKeyLabel={t("loomy.gateway.guide.noKey")}
              streamNote={t("loomy.gateway.guide.streamNote")}
            />
          </>
        )}

        {/* ---- 密钥（无归属轴） ---- */}
        {section === "keys" && <LoomyApiKeyTable onChanged={() => void loadAll()} />}

        {/* ---- 模型清单（向上游实时拉取 + 禁用 / 启用） ---- */}
        {section === "models" && (
          <>
            {/* 限流探测卡**在表之上**（与另四页同一组件、同一位置）。
                ★ `adoptSupported={false}`：Loomy 池**没有「模型级冷却」轴**（上游 429 是
                账号级的），把「某模型被限」回填成账号级冷却会误伤同账号其它模型
                （方案 §4.5.4 的 T1）⇒ 开关禁用并说明原因。后端 `ProbeTarget::Loomy::adopt`
                也是显式空操作（双保险）。 */}
            <ModelProbeCard
              region="cn"
              accounts={probeAccounts}
              models={probeModels}
              adoptSupported={false}
              className="mb-6"
            />
            <LoomyModelList
              catalog={catalog}
              refreshing={refreshingModels}
              onRefresh={() => void onRefreshModels()}
            />
          </>
        )}

        {/* ---- 账号池（与另三条共用同一张卡；tiles / 插槽走数据） ---- */}
        {section === "pool" && (
          <AccountPoolCard
            pool={status?.pool ?? null}
            accounts={status?.accounts ?? []}
            diagnose={status?.diagnose ?? []}
            preferredUid={config.preferredUid}
            onPreferredUidChange={(uid) => void persist({ preferredUid: uid })}
            saving={saving}
            onExclude={(uid, excluded) => void onExclude(uid, excluded)}
            excludingUid={excludingUid}
            totalRequests={status?.totalRequests ?? null}
            tiles={LOOMY_POOL_TILES}
            autoOptions={[
              { value: AUTO_OPTION, label: t("shared.gateway.pool.preferredAuto") },
              { value: RANDOM_OPTION, label: t("shared.gateway.pool.strategyRandom") },
            ]}
          />
        )}

        {/* ---- 请求日志 ---- */}
        {section === "logs" && (
          <TraeRequestLog logs={logs} clearing={clearing} onClear={() => void onClearLogs()} />
        )}

        {/* ---- 统一网关（与另三页**同一个组件**；它跨全部作用域，只读汇总） ---- */}
        {section === "u-overview" && <UnifiedGatewayView section="overview" onGoSection={goUnified} />}
        {section === "u-namespaces" && <UnifiedGatewayView section="namespaces" onGoSection={goUnified} />}
        {section === "u-keys" && <UnifiedGatewayView section="keys" onGoSection={goUnified} />}
        {section === "u-pool" && <UnifiedGatewayView section="pool" onGoSection={goUnified} />}
      </GatewayShell>

      {/* ---- 局域网风险确认 ---- */}
      <Dialog open={riskOpen} onOpenChange={setRiskOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <ShieldAlert className="size-4 text-warning" />
              {t("loomy.gateway.lanDialog.title")}
            </DialogTitle>
            <DialogDescription>{t("loomy.gateway.lanDialog.desc")}</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setRiskOpen(false)}>
              {t("loomy.gateway.lanDialog.cancel")}
            </Button>
            <Button variant="destructive" onClick={confirmLan}>
              {t("loomy.gateway.lanDialog.confirm")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
