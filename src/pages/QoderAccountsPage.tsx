/**
 * Qoder 账号页。
 *
 * ## 与原型的关系（`buddy-switch-prototype.html` 是唯一原型）
 *
 * 原型的账号页是**平台无关的工作台**：`viewAccounts()`（工具栏一行：搜索 / 排序 / 分组 /
 * 视图切换 / 全选 / 添加账号）+ `accountCard()`（勾选 → 整卡可点开详情 → 页脚更多菜单）
 * + `drawerAccount()`（详情抽屉：账号信息 / 备注编辑 / 操作区含删除）+ `batchBarHtml()`
 * （批量条）。本页**逐件复用**了共享壳 —— `AccountWorkbench`（工具栏 / 批量条 / 空态 /
 * 分桶 / 卡片栅格与表格切换）、`AccountDetailDrawer`、`AddAccountDrawer` —— 与
 * `AccountsPage`（WB）/ `TraeAccountsPage` 是**同一条交互链**，差异只有数据。
 *
 * ## ★ 备注编辑在**抽屉**里（不在卡上）
 *
 * 与原型 `drawerAccount(id, editing)` 同构：卡片点开抽屉 → 抽屉里「编辑备注」→
 * 就地输入 + 保存 / 取消。卡片**不再堆按钮**（原型 `acct-more` 的明说：
 * 「操作都在账号抽屉里 —— 点账号卡打开抽屉即可操作」）。
 *
 * ## 与后端能力的对齐（缺的就不渲染，绝不摆假控件）
 *
 * - **添加方式**只出「从本机导入 + 粘贴 PAT」两条（OAuth / 导入备份尚未落地 ⇒
 *   `AddAccountDrawer` 的 `ways` 收窄 + `extraOptions` 注入 PAT）。
 * - **批量条只出「删除」**（Qoder 没有签到 / 刷新 / 导出账号的命令；
 *   `AccountWorkbench` 的批量动作已按「缺哪个不渲染哪个」对齐批量条契约）。
 * - **删除必须确认**：Qoder 是**硬删除**、没有回收站 ⇒ 弹层文案**不许**抄
 *   「可在回收站恢复」（那是 WB / Trae 的话）。
 * - 状态槽**恒有一枚**（凭据可用 / 凭据未识别 / 无可用凭据，与另两平台的状态槽同义）；
 *   「归属异常」是正交的独立徽章（正常时不渲染）。
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useLocation, useSearchParams } from "react-router-dom";
import { AlertTriangle, KeyRound, Loader2, Plus, RefreshCw, ShieldAlert, UserRound } from "lucide-react";
import { toast } from "sonner";

import { AccountDetailDrawer } from "@/components/account-detail-drawer";
import type { AccountColumn } from "@/components/account-table";
import { AccountsTableAvatar } from "@/components/account-list-parts";
import { AccountWorkbench } from "@/components/account-workbench";
import { AddAccountDrawer } from "@/components/add-account-drawer";
import {
  QoderAccountCard,
  QoderForeignBadge,
  QoderStatusChips,
  qoderAccountName,
  qoderSourceLabel,
} from "@/components/qoder/qoder-account-card";
import { QoderLocalSourceRow } from "@/components/qoder/qoder-local-source-row";
import { ScopeChip, scopeLabel } from "@/components/scope-chip";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { EmptyState } from "@/components/ui/empty-state";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import * as api from "@/lib/api";
import type { QoderAccountView, QoderLocalSourcesView } from "@/lib/api";
import { accountSearchText } from "@/lib/account-search";
import {
  filterAccounts,
  groupAccounts,
  sortAccounts,
  type AccountComparator,
  type AccountGroupKey,
  type AccountListConfig,
  type AccountSortKey,
  type AccountViewMode,
} from "@/lib/account-list-view";
import { formatAbsoluteTime } from "@/lib/catalog-source";
import { displayText } from "@/lib/display-text";
import { useT } from "@/lib/i18n";
import { platformDescriptor, platformOfPath } from "@/lib/platforms/registry";
import { scopeOf, type Scope } from "@/lib/scope";
import { useScope } from "@/lib/use-scope";
import { useMediaQuery } from "@/lib/use-media-query";
import { useRefreshAction } from "@/lib/use-refresh-action";

/** ISO-8601 → 毫秒时间戳（解析不了给 `null` ⇒ 调用方**省掉那一行**，不显示 1970）。 */
const stampToMs = (iso: string | null | undefined): number | null => {
  if (!iso) return null;
  const ts = Date.parse(iso);
  return Number.isNaN(ts) ? null : ts;
};

export default function QoderAccountsPage() {
  const t = useT();
  const location = useLocation();
  const product = platformOfPath(location.pathname);
  const [region] = useScope(product);
  const scope = scopeOf(product, region);

  const [accounts, setAccounts] = useState<QoderAccountView[] | null>(null);
  const [sources, setSources] = useState<QoderLocalSourcesView | null>(null);
  const [error, setError] = useState("");
  const [importing, setImporting] = useState(false);

  /* ── 视图参数（页内 state，不进 URL、不进全局 store —— `account-list-view.ts` 的纪律） ── */
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<AccountSortKey>("name");
  const [group, setGroup] = useState<AccountGroupKey>("none");
  const [view, setView] = useState<AccountViewMode>("card");
  const tableEnabled = useMediaQuery("(min-width: 900px)");
  const searchRef = useRef<HTMLInputElement>(null);

  /* ── 选择（批量） ── */
  const [selected, setSelected] = useState<Set<string>>(new Set());

  /* ── 抽屉 / 弹层 ── */
  const [addDrawerOpen, setAddDrawerOpen] = useState(false);
  const [patOpen, setPatOpen] = useState(false);
  const [detail, setDetail] = useState<QoderAccountView | null>(null);
  const [detailAutoEditRemark, setDetailAutoEditRemark] = useState(false);
  const [pendingDelete, setPendingDelete] = useState<QoderAccountView | null>(null);
  const [batchDeleteOpen, setBatchDeleteOpen] = useState(false);
  const [deleteBusy, setDeleteBusy] = useState(false);

  /** 失败态「重新检测」按钮的进行中态（操作进行中 ⇒ 按钮内 Loader2）。 */
  const { busy, run } = useRefreshAction();

  const reload = useCallback(async () => {
    setError("");
    try {
      const [accountsView, localSources] = await Promise.all([
        api.getQoderAccounts(scope),
        api.getQoderLocalSources(scope),
      ]);
      setAccounts(accountsView.accounts ?? []);
      setSources(localSources);
    } catch (e) {
      setError(api.asError(e));
      setAccounts((prev) => prev ?? []);
    }
  }, [scope]);

  useEffect(() => {
    void reload();
  }, [reload]);

  /** `/` 键聚焦搜索框（与另两平台同一条规则）；在输入框里打字时不劫持。 */
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.key !== "/" || event.metaKey || event.ctrlKey || event.altKey) return;
      const target = event.target as HTMLElement | null;
      const tag = target?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || target?.isContentEditable) return;
      event.preventDefault();
      searchRef.current?.focus();
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  /**
   * P6：命令面板跳过来的 **`?q=` 一次性种子** —— 读一次播种页内搜索 state，
   * 随后 `replace` 清参（视图参数留页内）。`seededRef` 保证只读一次。
   */
  const [searchParams, setSearchParams] = useSearchParams();
  const seedAppliedRef = useRef(false);
  useEffect(() => {
    if (seedAppliedRef.current) return;
    seedAppliedRef.current = true;
    const seed = searchParams.get("q");
    if (!seed) return;
    setQuery(seed);
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        next.delete("q");
        return next;
      },
      { replace: true },
    );
  }, [searchParams, setSearchParams]);

  /* ── 取数后的派生数据 ── */

  const rows = accounts ?? [];
  const attentionCount = rows.filter(
    (a) => !a.hasCredential || a.credential.unrecognized || a.domainMismatch,
  ).length;
  const foreignAccounts = rows.filter((a) => a.domainMismatch);

  /**
   * 列表配置 —— 差异**全部以数据表达**（共享组件里不写平台判断）。
   *
   * 排序键只提供有数据支撑的两个：`名称` 与 `最近更新`（`updatedAt`，记录最后一次
   * 被导入 / 改备注的时刻）。**不提供**「积分优先级」（没有积分数据源 —— 摆一个
   * 按不存在字段排的选项就是假选项）。
   */
  const listConfig = useMemo<AccountListConfig<QoderAccountView>>(() => {
    const nameComparator: AccountComparator<QoderAccountView> = (left, right) =>
      qoderAccountName(left).localeCompare(qoderAccountName(right), undefined, {
        numeric: true,
        sensitivity: "base",
      });
    const recentComparator: AccountComparator<QoderAccountView> = (left, right) =>
      (stampToMs(right.updatedAt) ?? -Infinity) - (stampToMs(left.updatedAt) ?? -Infinity);
    return {
      searchText: (account) =>
        accountSearchText({
          name: displayText(account.name),
          uid: displayText(account.uid),
          remark: displayText(account.remark),
        }),
      comparators: { name: nameComparator, recent: recentComparator },
      bucketKey: (account, key) => {
        if (key === "status") {
          if (!account.hasCredential) return t("qoder.page.accounts.toolbar.statusBucket.noCredential");
          if (account.credential.unrecognized) return t("qoder.page.accounts.toolbar.statusBucket.unrecognized");
          return t("qoder.page.accounts.toolbar.statusBucket.normal");
        }
        return null;
      },
      availableSorts: ["name", "recent"],
      availableGroups: ["none", "status"],
    };
  }, [t]);

  const sortOptions = useMemo(
    () =>
      (
        [
          ["name", t("qoder.page.accounts.toolbar.sort.name")],
          ["recent", t("qoder.page.accounts.toolbar.sort.recent")],
        ] as const
      ).map(([value, label]) => ({ value, label })),
    [t],
  );
  const groupOptions = useMemo(
    () =>
      (
        [
          ["none", t("qoder.page.accounts.toolbar.group.none")],
          ["status", t("qoder.page.accounts.toolbar.group.status")],
        ] as const
      ).map(([value, label]) => ({ value, label })),
    [t],
  );

  const filtered = useMemo(() => filterAccounts(rows, listConfig, query), [rows, listConfig, query]);
  const sorted = useMemo(() => sortAccounts(filtered, sort, listConfig), [filtered, sort, listConfig]);
  const buckets = useMemo(() => groupAccounts(sorted, group, listConfig), [sorted, group, listConfig]);

  /* ── 选择（照 Trae：全选只作用于**当前可见**的行） ── */

  const allVisibleSelected = sorted.length > 0 && sorted.every((a) => selected.has(a.uid));
  const someVisibleSelected = !allVisibleSelected && sorted.some((a) => selected.has(a.uid));
  function toggleSelect(account: QoderAccountView) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(account.uid)) next.delete(account.uid);
      else next.add(account.uid);
      return next;
    });
  }
  function toggleVisibleSelection() {
    setSelected((prev) => {
      const next = new Set(prev);
      for (const account of sorted) {
        if (allVisibleSelected) next.delete(account.uid);
        else next.add(account.uid);
      }
      return next;
    });
  }
  function bucketAllSelected(items: QoderAccountView[]): boolean {
    return items.length > 0 && items.every((a) => selected.has(a.uid));
  }
  function bucketSomeSelected(items: QoderAccountView[]): boolean {
    return !bucketAllSelected(items) && items.some((a) => selected.has(a.uid));
  }
  function toggleBucketSelection(items: QoderAccountView[], checked: boolean) {
    setSelected((prev) => {
      const next = new Set(prev);
      for (const account of items) {
        if (checked) next.add(account.uid);
        else next.delete(account.uid);
      }
      return next;
    });
  }

  /* ── 动作 ── */

  /** 探测 + 导入（一个动作走完：结果由 toast 汇报；来源四态在首屏空态卡里可查）。 */
  async function onImportLocal() {
    if (importing) return;
    setImporting(true);
    try {
      const outcome = await api.qoderImportLocal(scope);
      const imported = outcome.imported ?? 0;
      const updated = outcome.updated ?? 0;
      if (imported + updated === 0) {
        /* 没导到东西时把**逐来源的问题说明**带上 —— 只说「没有可导入的」等于什么都不说。 */
        const detail = (outcome.problems ?? [])
          .map((problem) => `${problem.label}：${problem.detail ?? problem.state}`)
          .join("；");
        if (detail) toast.message(t("qoder.page.accounts.import.none"), { description: detail });
        else toast.message(t("qoder.page.accounts.import.none"));
      } else {
        toast.success(
          t("qoder.page.accounts.import.result", {
            imported: String(imported),
            updated: String(updated),
          }),
        );
      }
      await reload();
    } catch (e) {
      toast.error(api.asError(e));
    } finally {
      setImporting(false);
    }
  }

  /**
   * 保存备注（抽屉的就地编辑器调用）。
   *
   * ⚠️ 后端在 uid 不存在时返回 `{ ok: false }`（**不是**抛错）—— 必须如实报错，
   * 否则用户会以为「改成功了」，而库里根本没这条账号。
   * 成功后**同步抽屉里的快照**：抽屉的展示值来自 `remark` prop，只调 `reload()`
   * 不会让它重渲（那是「保存成功但界面还是旧值」的经典缺陷）。
   */
  async function saveRemark(account: QoderAccountView, remark: string): Promise<boolean> {
    const value = remark.trim();
    try {
      const result = await api.qoderSetAccountRemark(scope, account.uid, value === "" ? null : value);
      if (!result.ok) {
        toast.error(t("qoder.page.accounts.remark.missing"));
        return false;
      }
      setDetail((prev) => (prev && prev.uid === account.uid ? { ...prev, remark: value === "" ? null : value } : prev));
      await reload();
      toast.success(t("qoder.page.accounts.remark.saved"));
      return true;
    } catch (e) {
      toast.error(api.asError(e));
      return false;
    }
  }

  function openDetail(account: QoderAccountView) {
    setDetailAutoEditRemark(false);
    setDetail(account);
  }

  /** 「更多 → 编辑备注」：打开抽屉并**直接进入编辑态**（原型 `drawerAccount(id, true)`）。 */
  function openRemark(account: QoderAccountView) {
    setDetailAutoEditRemark(true);
    setDetail(account);
  }

  /** 单账号删除（**硬删除**：弹层确认是唯一的机会窗口）。 */
  async function confirmSingleDelete() {
    if (!pendingDelete || deleteBusy) return;
    setDeleteBusy(true);
    try {
      await api.qoderDeleteAccount(scope, pendingDelete.uid);
      toast.success(t("qoder.page.accounts.removed", { name: qoderAccountName(pendingDelete) }));
      setPendingDelete(null);
      await reload();
    } catch (e) {
      toast.error(api.asError(e));
    } finally {
      setDeleteBusy(false);
    }
  }

  /**
   * 批量删除：**顺序逐条**调单删命令（后端只有单删），并如实汇报成败 ——
   * 批量操作必然可能部分失败，不能让「批量删除」看起来永远 100% 成功。
   */
  async function confirmBatchDelete() {
    if (deleteBusy) return;
    const targets = rows.filter((a) => selected.has(a.uid));
    if (targets.length === 0) return;
    setDeleteBusy(true);
    let ok = 0;
    let fail = 0;
    for (const account of targets) {
      try {
        await api.qoderDeleteAccount(scope, account.uid);
        ok += 1;
      } catch {
        fail += 1;
      }
    }
    setDeleteBusy(false);
    setBatchDeleteOpen(false);
    if (fail === 0) toast.success(t("qoder.page.accounts.delete.done", { n: String(ok) }));
    else toast.error(t("qoder.page.accounts.delete.partial", { ok: String(ok), fail: String(fail) }));
    setSelected(new Set());
    await reload();
  }

  /* ── 抽屉与表格的装配 ── */

  const detailRows = useMemo(() => {
    if (!detail) return [];
    const rowsOut = [
      { label: t("qoder.page.accounts.drawer.uid"), value: displayText(detail.uid) ?? "—", mono: true },
      { label: t("qoder.page.accounts.drawer.domain"), value: displayText(detail.domain) ?? "—" },
      { label: t("qoder.page.accounts.drawer.source"), value: qoderSourceLabel(t, detail.source) ?? "—" },
    ];
    const addedAt = stampToMs(detail.addedAt);
    if (addedAt !== null) {
      rowsOut.push({ label: t("qoder.page.accounts.drawer.addedAt"), value: formatAbsoluteTime(addedAt), mono: false });
    }
    const updatedAt = stampToMs(detail.updatedAt);
    if (updatedAt !== null) {
      rowsOut.push({ label: t("qoder.page.accounts.drawer.updatedAt"), value: formatAbsoluteTime(updatedAt), mono: false });
    }
    rowsOut.push({ label: t("qoder.page.accounts.drawer.credential"), value: displayText(detail.credentialSummary) ?? "—", mono: false });
    if (detail.accessTokenMasked) {
      rowsOut.push({ label: t("qoder.page.accounts.drawer.accessToken"), value: detail.accessTokenMasked, mono: true });
    }
    return rowsOut;
  }, [detail, t]);

  const columns: AccountColumn<QoderAccountView>[] = [
    {
      key: "account",
      header: t("qoder.page.accounts.table.account"),
      render: (account) => (
        /* 与 WB / Trae 表格同一结构：头像 + 名称 + 身份行。 */
        <div className="flex min-w-0 items-center gap-2.5">
          <AccountsTableAvatar name={qoderAccountName(account)} />
          <div className="min-w-0">
            <div className="truncate font-medium" title={qoderAccountName(account)}>
              {qoderAccountName(account)}
            </div>
            <div className="truncate font-mono text-xs text-muted-foreground">
              UID · {displayText(account.uid) ?? "—"}
            </div>
          </div>
        </div>
      ),
    },
    {
      key: "status",
      header: t("qoder.page.accounts.table.status"),
      render: (account) => (
        <div className="flex flex-wrap items-center gap-1">
          <QoderStatusChips account={account} />
          <QoderForeignBadge account={account} />
        </div>
      ),
    },
    {
      key: "source",
      header: t("qoder.page.accounts.table.source"),
      render: (account) => (
        <span className="text-muted-foreground">{qoderSourceLabel(t, account.source) ?? "—"}</span>
      ),
    },
    {
      key: "remark",
      header: t("qoder.page.accounts.table.remark"),
      render: (account) => (
        <span className="text-muted-foreground">{displayText(account.remark) ?? "—"}</span>
      ),
    },
  ];

  /* ── 首屏空态（「真没有账号」由它承担：出口 = 添加账号抽屉） ── */

  const emptyState =
    accounts !== null && accounts.length === 0 ? (
      <QoderEmptyCard
        error={error}
        sources={sources}
        onAdd={() => setAddDrawerOpen(true)}
        onReload={reload}
      />
    ) : null;

  /**
   * 「从本机导入」是否可用：任一来源不是 `missing` 即可点（`unreadable` 也要放行 ——
   * 点下去会给出可读的原因；禁用它等于把「读不出来」变成「点不动」）。
   * 探测未返回时**放行**：点了最坏也是一条诚实的报错，比一个闪烁的禁用态好。
   */
  const localInstalled = sources === null ? true : sources.sources.some((s) => s.state !== "missing");

  const productLabel = t(platformDescriptor(product).labelKey);

  return (
    <div className="mx-auto w-full max-w-[1240px] px-7 pt-[26px] pb-[60px]">
      <header className="flex items-start gap-4">
        <div className="min-w-0">
          <h1 className="text-[28px] font-[640] tracking-[-0.02em]">{t("qoder.page.accounts.title")}</h1>
          <p className="mt-1.5 text-[12.5px] text-muted-foreground">
            {attentionCount > 0
              ? t("pageHead.accounts.pending", {
                  scope: scopeLabel(t, scope),
                  n: rows.length,
                  m: attentionCount,
                })
              : t("pageHead.accounts", { scope: scopeLabel(t, scope), n: rows.length })}
          </p>
        </div>
        <div className="grow" />
        <ScopeChip scope={scope} />
      </header>

      {/* ★ 账号工作台（**三平台共用容器**）—— 与 `AccountsPage` / `TraeAccountsPage`
          是**同一个组件**；差异只在数据（排序/分组选项、卡片渲染、列定义、空态）。 */}
      <AccountWorkbench
        query={query}
        onQueryChange={setQuery}
        searchInputRef={searchRef}
        sort={sort}
        sortOptions={sortOptions}
        onSortChange={(next) => setSort(next as AccountSortKey)}
        group={group}
        groupOptions={groupOptions}
        onGroupChange={(next) => setGroup(next as AccountGroupKey)}
        view={view}
        onViewChange={setView}
        tableViewEnabled={tableEnabled}
        selection={{
          allSelected: allVisibleSelected,
          someSelected: someVisibleSelected,
          onToggleAll: toggleVisibleSelection,
          label: allVisibleSelected ? t("shared.toolbar.deselectAll") : t("shared.toolbar.selectAll"),
        }}
        /* ★ 主操作 = 原型工具栏最右侧的 `+ 添加账号`（打开添加抽屉，方式卡由页面注入）。 */
        addAction={{
          label: t("shared.addDrawer.title"),
          onClick: () => setAddDrawerOpen(true),
        }}
        loading={accounts === null}
        emptyState={emptyState}
        notices={
          <>
            {error && rows.length > 0 ? (
              <Alert variant="destructive" className="mb-4">
                <AlertTriangle />
                <AlertTitle>{t("qoder.page.accounts.loadFailed")}</AlertTitle>
                <AlertDescription>
                  <span>{error}</span>
                  {/* ★ 失败态**必须给一条出路**：没有它，用户唯一能做的就是重启应用。 */}
                  <div className="mt-2">
                    <Button size="sm" variant="outline" onClick={() => void run(() => reload())} disabled={busy}>
                      {busy ? <Loader2 className="animate-spin" /> : <RefreshCw />}
                      {t("qoder.page.accounts.recheck")}
                    </Button>
                  </div>
                </AlertDescription>
              </Alert>
            ) : null}
            {/* 归属不符的账号**逐条列出**（不静默丢弃）—— 它们确实在这个库里，
                只是 domain 属于另一个区域；删除或迁走是用户要做的决定。 */}
            {foreignAccounts.length > 0 ? (
              <Card className="mb-4 gap-0 border-warning/40 py-0">
                <div className="flex items-start gap-2.5 px-4 py-3.5">
                  <ShieldAlert className="mt-0.5 size-4 shrink-0 text-[var(--warning)]" />
                  <div className="min-w-0 text-[12.5px]">
                    {t("qoder.page.accounts.domainMismatch")}：
                    {foreignAccounts.map((a) => qoderAccountName(a)).join("、")}
                  </div>
                </div>
              </Card>
            ) : null}
          </>
        }
        checkinResult={null}
        batch={{
          selectedCount: selected.size,
          busy: deleteBusy,
          /* 只有删除是已实现动作（签到 / 刷新 Token / 导出账号的命令尚未落地）——
             其余按钮由批量条按「缺哪个不渲染哪个」自动消失。 */
          onDelete: () => setBatchDeleteOpen(true),
          onClear: () => setSelected(new Set()),
        }}
        listAriaLabel={t("qoder.page.accounts.sectionAria", { count: rows.length })}
        totalCount={rows.length}
        visibleCount={sorted.length}
        emptyNone={{
          title: t("shared.list.empty.none"),
          description: t("shared.list.empty.noneDesc"),
        }}
        emptyFiltered={{
          title: t("shared.list.empty.filtered"),
          description: t("shared.list.empty.filteredDesc"),
          action: (
            <Button
              variant="outline"
              onClick={() => {
                setQuery("");
                setGroup("none");
              }}
            >
              {t("shared.list.empty.clearFilters")}
            </Button>
          ),
        }}
        buckets={buckets}
        columns={columns}
        rowKey={(account) => displayText(account.uid) ?? ""}
        selectedIds={selected}
        onToggleSelect={toggleSelect}
        onToggleBucketAll={toggleBucketSelection}
        bucketAllSelected={bucketAllSelected}
        bucketSomeSelected={bucketSomeSelected}
        onOpenRow={openDetail}
        /* Qoder 没有「当前登录」概念（程序位未落地）⇒ 恒 false，不亮竖条。 */
        isRowCurrent={() => false}
        rowSelectLabel={(account) => t("shared.table.selectRowAria", { name: qoderAccountName(account) })}
        rowOpenLabel={(account) => t("shared.table.openRowAria", { name: qoderAccountName(account) })}
        renderCard={(account) => (
          <QoderAccountCard
            key={account.uid}
            account={account}
            selected={selected.has(account.uid)}
            onToggleSelect={toggleSelect}
            onOpenDetail={openDetail}
            onEditRemark={openRemark}
            onDelete={setPendingDelete}
          />
        )}
      />

      {/* 产品名走注册表（唯一来源）—— 本页在 Qoder 分区下渲染，但产品名不写死。 */}
      <p className="mt-6 text-[11.5px] text-muted-foreground">{productLabel}</p>

      {/* ── 添加账号抽屉（原型 `drawerAdd`：方式卡 + 平台专属注入） ── */}
      <AddAccountDrawer
        open={addDrawerOpen}
        onOpenChange={setAddDrawerOpen}
        scopeLabel={scopeLabel(t, scope)}
        installed={localInstalled}
        /* 只出**已落地**的两条方式：从本机导入（基础卡）+ 粘贴 PAT（extraOptions）。
           OAuth / 导入备份尚未落地 ⇒ 不渲染对应卡（免得点下去必然失败）。 */
        ways={["local"]}
        onPick={(way) => {
          if (way === "local") void onImportLocal();
        }}
        extraOptions={[
          {
            key: "paste-pat",
            icon: KeyRound,
            title: t("qoder.page.accounts.pat.title"),
            desc: t("qoder.page.accounts.pat.description"),
            onClick: () => setPatOpen(true),
          },
        ]}
        labels={{
          title: t("shared.addDrawer.title"),
          scopeHintPrefix: t("shared.addDrawer.scopePrefix"),
          scopeHintSuffix: t("shared.addDrawer.scopeSuffix"),
          oauthTitle: t("shared.addDrawer.oauthTitle"),
          oauthDesc: t("shared.addDrawer.oauthDesc"),
          localTitle: t("shared.addDrawer.localTitle"),
          localDesc: t("shared.addDrawer.localDesc"),
          backupTitle: t("shared.addDrawer.backupTitle"),
          backupDesc: t("shared.addDrawer.backupDesc"),
          /* ⚠️ 不抄 shared 那句「可先用 OAuth 登录」—— Qoder 的 OAuth 尚未落地，
             照抄会把用户引向一个不存在的能力。 */
          notInstalled: t("qoder.page.accounts.addDrawer.notInstalled"),
          moreTools: t("shared.addDrawer.moreTools"),
        }}
      />

      {/* ── 粘贴 PAT 弹层（「从本机导入」读不出来时的兜底路径） ── */}
      <PastePatDialog
        open={patOpen}
        onOpenChange={setPatOpen}
        scope={scope}
        onImported={() => void reload()}
      />

      {/* ── 账号详情抽屉（备注编辑在这里；操作区只有删除） ── */}
      <AccountDetailDrawer
        open={detail !== null}
        onOpenChange={(open) => {
          if (!open) setDetail(null);
        }}
        name={detail ? qoderAccountName(detail) : ""}
        subtitle={
          detail
            ? t("qoder.page.accounts.drawer.subtitle", {
                name: qoderAccountName(detail),
                uid: displayText(detail.uid) ?? "—",
              })
            : ""
        }
        avatarSeed={detail ? qoderAccountName(detail) : ""}
        sections={detail ? [{ key: "account", title: t("qoder.page.accounts.drawer.info"), rows: detailRows }] : []}
        remark={detail?.remark ?? ""}
        onSaveRemark={detail ? (remark) => saveRemark(detail, remark) : undefined}
        autoEditRemark={detailAutoEditRemark}
        /* 与 WB / Trae 抽屉**同一取值**（备注是否可编辑不受演示模式影响）。 */
        featuresDisabled={false}
        actions={
          detail
            ? {
                onDelete: () => {
                  const target = detail;
                  setDetail(null);
                  setPendingDelete(target);
                },
              }
            : null
        }
      />

      {/* ── 单账号删除确认（硬删除 ⇒ 必须先确认；不提供「回收站恢复」的说法） ── */}
      <Dialog open={pendingDelete !== null} onOpenChange={(open) => !open && setPendingDelete(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("qoder.page.accounts.delete.title")}</DialogTitle>
            <DialogDescription>
              {pendingDelete
                ? t("qoder.page.accounts.delete.desc", { name: qoderAccountName(pendingDelete) })
                : ""}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setPendingDelete(null)} disabled={deleteBusy}>
              {t("qoder.page.accounts.delete.cancel")}
            </Button>
            <Button variant="destructive" onClick={() => void confirmSingleDelete()} disabled={deleteBusy}>
              {deleteBusy ? <Loader2 className="size-4 animate-spin" /> : null}
              {t("qoder.page.accounts.delete.confirm")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* ── 批量删除确认 ── */}
      <Dialog open={batchDeleteOpen} onOpenChange={setBatchDeleteOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("qoder.page.accounts.delete.batchTitle")}</DialogTitle>
            <DialogDescription>
              {t("qoder.page.accounts.delete.batchDesc", { n: String(selected.size) })}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setBatchDeleteOpen(false)} disabled={deleteBusy}>
              {t("qoder.page.accounts.delete.cancel")}
            </Button>
            <Button variant="destructive" onClick={() => void confirmBatchDelete()} disabled={deleteBusy}>
              {deleteBusy ? <Loader2 className="size-4 animate-spin" /> : null}
              {t("qoder.page.accounts.delete.confirm")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

/** 展示名走卡片模块的**唯一回落链**（页面、卡片、抽屉、toast 同一份）。 */

/**
 * 首屏空态卡（「这个区域还没有账号」）。
 *
 * 照原型 `emptyBlock`（标题 / 说明 / 出口按钮），出口 = 「添加账号」抽屉；并附
 * **本机来源四态**（`missing / unreadable / empty / ready` 对应四种不同的用户动作：
 * 去装 / 去客户端重新登录 / 换个来源 / 直接导入 —— 压成一个「未检测到」就会给错引导）。
 * 失败信息也在这里兜底展示（工作台在首屏空态时会整块替换，含 notices 槽）。
 */
function QoderEmptyCard({
  error,
  sources,
  onAdd,
  onReload,
}: {
  error: string;
  sources: QoderLocalSourcesView | null;
  onAdd: () => void;
  onReload: () => Promise<void>;
}) {
  const t = useT();
  const { busy, run } = useRefreshAction();
  return (
    <Card className="gap-0 py-0">
      {error ? (
        <div className="flex flex-wrap items-center gap-2.5 border-b px-4 py-3.5">
          <ShieldAlert className="size-4 shrink-0 text-destructive" />
          <div className="min-w-0 grow text-[12.5px]">{error}</div>
          <Button size="sm" variant="outline" onClick={() => void run(() => onReload())} disabled={busy}>
            {busy ? <Loader2 className="animate-spin" /> : <RefreshCw />}
            {t("qoder.page.accounts.recheck")}
          </Button>
        </div>
      ) : null}
      <EmptyState
        icon={UserRound}
        title={t("qoder.page.accounts.empty.title")}
        description={t("qoder.page.accounts.empty.description")}
        action={
          <Button onClick={onAdd}>
            <Plus className="size-4" />
            {t("qoder.page.accounts.empty.action")}
          </Button>
        }
      />
      {sources ? (
        <div className="border-t">
          <div className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5 px-4 pb-1 pt-3">
            <span className="text-[12.5px] font-[550]">{t("qoder.page.accounts.empty.sources")}</span>
            <span className="text-[11px] text-muted-foreground">
              {t("qoder.page.accounts.import.description")}
            </span>
          </div>
          {sources.sources.map((source) => (
            <QoderLocalSourceRow key={source.kind} source={source} />
          ))}
        </div>
      ) : null}
    </Card>
  );
}

/** 粘贴 PAT 弹层：前缀校验（`pt-`）在客户端先拦一道，导入走 `qoder_import_pat`。 */
function PastePatDialog({
  open,
  onOpenChange,
  scope,
  onImported,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  scope: Scope;
  onImported: () => void;
}) {
  const t = useT();
  const [pat, setPat] = useState("");
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);

  async function submit() {
    const value = pat.trim();
    if (!value.startsWith("pt-")) {
      setError(t("qoder.page.accounts.pat.invalid"));
      return;
    }
    setError("");
    setSaving(true);
    try {
      await api.qoderImportPat(scope, value);
      toast.success(t("qoder.page.accounts.import.result", { imported: "1", updated: "0" }));
      setPat("");
      onOpenChange(false);
      onImported();
    } catch (e) {
      setError(api.asError(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t("qoder.page.accounts.pat.title")}</DialogTitle>
          <DialogDescription>{t("qoder.page.accounts.pat.description")}</DialogDescription>
        </DialogHeader>
        <div className="space-y-2">
          <Label htmlFor="qoder-pat">PAT</Label>
          <Input
            id="qoder-pat"
            value={pat}
            onChange={(event) => {
              setPat(event.target.value);
              setError("");
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !saving) void submit();
            }}
            placeholder={t("qoder.page.accounts.pat.placeholder")}
            className="font-mono text-[12.5px]"
            spellCheck={false}
          />
          {error ? <div className="text-[11.5px] text-destructive">{error}</div> : null}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)} disabled={saving}>
            {t("qoder.page.accounts.pat.cancel")}
          </Button>
          <Button onClick={() => void submit()} disabled={saving || !pat.trim()}>
            {saving ? <Loader2 className="size-4 animate-spin" /> : null}
            {t("qoder.page.accounts.pat.action")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
