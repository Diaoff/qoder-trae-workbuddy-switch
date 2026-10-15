/**
 * Qoder 账号卡片。
 *
 * 结构与 WorkBuddy / Trae 的账号卡**一一对应**（同样的 header / chips / body / footer
 * 四块、同样的「勾选 → 整卡可点开详情 → 页脚更多菜单」交互、同样的头像色板与几何）。
 *
 * ## 交互边界（原型 `accountCard()` 的纪律：**卡片不堆按钮**）
 *
 * 原型的「更多操作」在弹出层里明说：*「操作都在账号抽屉里 —— 点账号卡打开抽屉即可操作；
 * 刷新 Token / 手动签到 / 编辑备注 / 删除 —— 卡片不再堆按钮」*。
 * ⇒ 本卡片的**常规可点区只有三处**：勾选框 / 整卡（打开详情）/ 页脚「更多」菜单；
 * **备注编辑与删除都不在卡上**（它们住在 `account-detail-drawer.tsx` 里）。
 *
 * ## 与另两个平台卡的差异只有数据
 *
 * | 块 | WB / Trae | Qoder（本文件） |
 * |:--|:--|:--|
 * | header | 头像 + 昵称 + 身份 + 「当前登录」徽章 | 同形；**无**「当前登录」（程序位尚未落地） |
 * | chips | 使用中 N 工具 / 状态 / 临期 / 优先 | 状态槽（凭据可用 / 凭据未识别 / 无可用凭据）+ 归属异常 + 来源徽章 |
 * | body | 指标行（积分 / 今日 Token / 最后使用） | 凭据摘要 + 访问令牌掩码（积分与用量**没有数据源**，不摆空指标） |
 * | footer | 程序位行 + 更多菜单 | 仅更多菜单（程序位未落地 ⇒ 不渲染，落地后按原型补 `.progs` 行） |
 *
 * ## 三个刻意的取舍（都有出处）
 *
 * 1. **状态槽恒有一枚**（`凭据可用` / `凭据未识别` / `无可用凭据`）—— 与另两平台的
 *    状态槽同义（WB 的「已签到」/ Trae 的默认成功徽章都是恒有一枚）；「归属异常」
 *    是**正交**的独立徽章（正常时不渲染 —— 与 `travelChip` 的「默认态不渲染」同源）。
 * 2. **不渲染程序位胶囊**：Qoder 的两处官方凭据存储（桌面版 `auth.v1.dat` / CLI
 *    `~/.qoder/.auth/user`）对应的切换能力**尚未落地**（没有命令、没有写盘实现）——
 *    按「缺的就不渲染」纪律留空，绝不摆点不动的灰胶囊。
 * 3. **不渲染 `expiresAt`**：该字段单位未取证（见 `QoderAccountsView` 的说明），
 *    按毫秒渲染会把秒值显示成 1970 = 谎报。
 */

import { Ellipsis, Info, StickyNote, Trash2 } from "lucide-react";

import { AVATAR_TEXT_CLASS, accountInitials, avatarTone } from "@/components/account-card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { DemoAction } from "@/components/demo-action";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import type { QoderAccountView } from "@/lib/api";
import { demoModeEnabled } from "@/lib/demo-mode";
import { displayText } from "@/lib/display-text";
import { useT, type Translate } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import type { TranslationKey } from "@/locales/zh";

/* 与 `account-card.tsx` 同一处理：几何交给 `Badge` 基础样式（已逐值对齐原型 `.badge`），
   这里只抵消它的 1px 透明边框（原型 `.badge` 没有边框）。 */
const CHIP_CLASS = "border-0 px-[7px] font-medium";

/**
 * 来源 id → 词条键（id 是**后端数据**，见 `qoder::account::SOURCE_*`）。
 *
 * ⚠️ 与 `companion-row.tsx` 的 `SOURCE_LABEL_KEY` 同一手法：**表外的新取值原样显示**，
 * 不隐藏也不猜 —— 后端加了新来源时，界面宁可显示 `mystery-source` 也不要显示空白。
 */
const SOURCE_LABEL_KEY: Record<string, TranslationKey> = {
  pat: "qoder.page.accounts.origin.pat",
  "desktop-app": "qoder.page.accounts.origin.desktopApp",
  cli: "qoder.page.accounts.origin.cli",
  oauth: "qoder.page.accounts.origin.oauth",
};

/** 来源的展示名（表外取值原样回落；空值给 `null` 让调用方决定省不省这一行）。 */
export function qoderSourceLabel(t: Translate, source: string | null | undefined): string | null {
  if (!source) return null;
  const key = SOURCE_LABEL_KEY[source];
  return key ? t(key) : source;
}

/**
 * 账号展示名（卡片 / 表格 / 抽屉 / toast / 无障碍名**同一份回落链**，避免各区各拼一遍）。
 *
 * 回落顺序与另两平台同一纪律：昵称 → 名字 → uid → `—`；**绝不渲染对象**
 * （脏值会让 React 卸整棵树 ⇒ 白屏，issue #2）。
 */
export function qoderAccountName(account: QoderAccountView): string {
  return displayText(account.nickname) || displayText(account.name) || displayText(account.uid) || "—";
}

/**
 * 账号的**状态徽章**（卡片 chips 行与表格「状态」列**同一份实现**，避免两处口径漂移）。
 *
 * 与原型 `.acct-chips` 的第 2 槽同义（WB「已签到」/ Trae 的默认成功徽章）：**恒有一枚** ——
 * 问题态是警示色，正常态是成功色「凭据可用」。`正常` 不渲染的写法会让 chips 行
 * 在两种视图间跳变，也与另两个平台不同形。
 */
export function QoderStatusChips({ account }: { account: QoderAccountView }) {
  const t = useT();
  if (!account.hasCredential) {
    return (
      <Badge variant="destructive" className={CHIP_CLASS}>
        {t("qoder.page.accounts.chip.noCredential")}
      </Badge>
    );
  }
  if (account.credential.unrecognized) {
    return (
      <Badge variant="warning" className={CHIP_CLASS}>
        {t("qoder.page.accounts.chip.unrecognized")}
      </Badge>
    );
  }
  return (
    <Badge variant="success" className={CHIP_CLASS}>
      {t("qoder.page.accounts.chip.ok")}
    </Badge>
  );
}

/** 「归属异常」徽章（与状态槽**正交**的独立一枚；正常时不渲染）。 */
export function QoderForeignBadge({ account }: { account: QoderAccountView }) {
  const t = useT();
  if (!account.domainMismatch) return null;
  return (
    <Badge variant="warning" className={CHIP_CLASS} title={t("qoder.page.accounts.chip.foreignTip")}>
      {t("qoder.page.accounts.chip.foreign")}
    </Badge>
  );
}

export interface QoderAccountCardProps {
  account: QoderAccountView;
  /** 是否被勾选（批量操作）。 */
  selected?: boolean;
  /** 切换勾选。 */
  onToggleSelect?: (account: QoderAccountView) => void;
  /** 打开详情抽屉（抽屉状态由页面持有）。 */
  onOpenDetail?: (account: QoderAccountView) => void;
  /** 「更多 → 编辑备注」：打开抽屉并进入编辑态。 */
  onEditRemark?: (account: QoderAccountView) => void;
  /** 「更多 → 删除账号」：页面弹确认（Qoder 侧是**硬删除**，没有回收站，必须先确认）。 */
  onDelete?: (account: QoderAccountView) => void;
}

export function QoderAccountCard({
  account,
  selected = false,
  onToggleSelect,
  onOpenDetail,
  onEditRemark,
  onDelete,
}: QoderAccountCardProps) {
  const t = useT();
  const name = qoderAccountName(account);
  const uid = displayText(account.uid);
  const identity = uid ? `UID · ${uid}` : "—";
  const sourceLabel = qoderSourceLabel(t, account.source);
  const credentialSummary = displayText(account.credentialSummary);

  const menu = demoModeEnabled ? (
    <DemoAction>
      <Button
        variant="ghost"
        size="sm"
        className="!px-[10px] text-muted-foreground hover:bg-accent hover:text-foreground"
        aria-label={t("qoder.page.accounts.card.manageAria", { name })}
        title={t("qoder.page.accounts.card.moreActions")}
      >
        <Ellipsis className="size-4" />
      </Button>
    </DemoAction>
  ) : (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          className="!px-[10px] text-muted-foreground hover:bg-accent hover:text-foreground"
          aria-label={t("qoder.page.accounts.card.manageAria", { name })}
          title={t("qoder.page.accounts.card.moreActions")}
        >
          <Ellipsis className="size-4" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-40">
        <DropdownMenuItem disabled={!onOpenDetail} onSelect={() => onOpenDetail?.(account)}>
          <Info />
          {t("qoder.page.accounts.card.viewDetail")}
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem disabled={!onEditRemark} onSelect={() => onEditRemark?.(account)}>
          <StickyNote />
          {t("qoder.page.accounts.card.editRemark")}
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        {/* 删除是**危险动作**且无回收站兜底（Qoder 侧硬删除）⇒ 落到页面统一弹确认。 */}
        <DropdownMenuItem
          className="text-destructive focus:bg-destructive/5 focus:text-destructive"
          onSelect={() => onDelete?.(account)}
        >
          <Trash2 />
          {t("qoder.page.accounts.card.deleteAccount")}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );

  return (
    <article
      /* 键盘可达：整卡可 Tab 聚焦，Enter / Space 打开详情（与 WB / Trae 卡同一交互）。 */
      tabIndex={0}
      role="article"
      onClick={(event) => {
        if (!onOpenDetail) return;
        const target = event.target;
        if (
          target instanceof Element &&
          target.closest("button, a, input, [role='checkbox'], [role='switch'], [data-no-account-open]")
        ) {
          return;
        }
        onOpenDetail(account);
      }}
      onKeyDown={(event) => {
        if (!onOpenDetail || event.target !== event.currentTarget) return;
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onOpenDetail(account);
        }
      }}
      className={cn(
        "relative flex min-w-0 flex-col gap-2.5 overflow-hidden rounded-lg border border-border bg-card px-3.5 py-[13px] shadow-card transition-[border-color,box-shadow] hover:border-border-strong hover:shadow-card-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
        selected && "border-ring bg-primary/[0.07]",
      )}
    >
      {/* header：勾选 + 头像 32 + 昵称/身份 —— 原型 `.acct-top` 的四件套（无「当前登录」）。 */}
      <header className="flex items-center gap-2.5">
        <div className="relative z-10 flex w-full min-w-0 items-center gap-2.5">
          <Checkbox
            checked={selected}
            onCheckedChange={() => onToggleSelect?.(account)}
            disabled={!onToggleSelect}
            aria-label={t("qoder.page.accounts.card.selectAria", { name })}
            className="size-[17px]"
          />
          <div
            className={cn(
              "flex size-8 shrink-0 items-center justify-center rounded-[10px] text-[12.5px] font-[640]",
              AVATAR_TEXT_CLASS,
              avatarTone(name),
            )}
          >
            {accountInitials(name)}
          </div>
          <div className="min-w-0 flex-1">
            <h3 className="truncate text-[13px] font-semibold" title={name}>
              {name}
            </h3>
            <p className="truncate text-[11px] text-muted-foreground" title={identity}>
              {identity}
            </p>
          </div>
        </div>
      </header>

      {/* chips：状态槽（恒一枚） + 归属异常（正交） + 来源徽章。 */}
      <div className="flex min-w-0 flex-wrap items-center gap-[5px]">
        <QoderStatusChips account={account} />
        <QoderForeignBadge account={account} />
        {sourceLabel ? (
          <Badge variant="secondary" className={cn(CHIP_CLASS, "text-muted-foreground")}>
            {sourceLabel}
          </Badge>
        ) : null}
      </div>

      {/* body：凭据摘要 + 访问令牌掩码（**只给掩码**，绝不出明文）。 */}
      <section className="flex min-w-0 flex-1 flex-col gap-1">
        {credentialSummary ? (
          <div className="truncate text-[11.5px] text-muted-foreground" title={credentialSummary}>
            {credentialSummary}
          </div>
        ) : null}
        {account.accessTokenMasked ? (
          <div className="truncate font-mono text-[11px] text-muted-foreground" title={account.accessTokenMasked}>
            {account.accessTokenMasked}
          </div>
        ) : null}
      </section>

      {/* footer：原型 `.acct-actions`（程序位行 + 更多菜单）—— 程序位未落地 ⇒ 仅菜单。 */}
      <footer className="flex items-center gap-1.5 border-t border-border pt-2.5">
        <div className="grow" />
        {menu}
      </footer>
    </article>
  );
}
