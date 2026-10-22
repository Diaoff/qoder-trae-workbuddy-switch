/**
 * Loomy 账号卡片。
 *
 * 结构与 WorkBuddy / Trae / Qoder 的账号卡**一一对应**（同样的 header / chips / body /
 * footer 四块、同样的头像色板与几何、同样的「勾选 → 整卡可点开详情 → 页脚更多菜单」）。
 *
 * ## 为什么必须与别的分区同构（而不是自画一套）
 *
 * 改造前的 Loomy 卡是一个裸 `<div className="rounded-lg border p-4">` + `<dl>`，
 * 且动作用 `window.alert` / `window.confirm` —— 同一个应用里两种账号卡的观感与
 * 交互完全不同，用户会以为 Loomy 是「没做完的功能」。四张卡共用同一套
 * `account-card.tsx` 的原语（头像色板 / 首字母 / 几何），差异只允许是**数据**。
 *
 * ## 交互边界（原型 `accountCard()` 的纪律：**卡片不堆按钮**）
 *
 * 常规可点区只有三处：勾选框 / 整卡（打开详情）/ 页脚「更多」菜单。
 * 签到、刷新积分、编辑备注、删除**都进菜单**（与 Qoder 卡一致）。
 *
 * ## 三处刻意的取舍
 *
 * 1. **不渲染 `expiresAt`**：Loomy 的 `expires_at` 由本地按 `session_ttl_seconds`
 *    推算（服务端响应不带到期时间戳）。既然它是**本地估算**，摆一个精确到分钟的
 *    「14 天后过期」会给出它没有的确定性 —— 只在**已过期**时给一枚警示徽章。
 * 2. **不渲染程序位行**：Loomy 有官方客户端，但本仓没有它的程序位/切换能力
 *    （无命令、无写盘实现）⇒ 按「缺的就不渲染」纪律留空，绝不摆点不动的灰胶囊。
 * 3. **积分两池分列且恒显示**：`0` 在这里是**真实读数**（积分确实为 0），
 *    与「无数据源」不同 —— 后者是 Qoder 侧不渲染积分的理由，这里不适用。
 */

import { Ellipsis, FolderTree, Info, RefreshCw, StickyNote, Trash2 } from "lucide-react";

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
import { MenuGroupLabel, MenuHead, MenuNote } from "@/components/account-menu-parts";
import type { LoomyAccountView } from "@/lib/api";
import { demoModeEnabled } from "@/lib/demo-mode";
import { displayText } from "@/lib/display-text";
import { useT, type Translate } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import type { TranslationKey } from "@/locales/zh";

/** 与 `account-card.tsx` / `qoder-account-card.tsx` 同一处理：抵消 1px 透明边框。 */
const CHIP_CLASS = "border-0 px-[7px] font-medium";

/**
 * 来源 id → 词条键（id 是**后端数据**，见 `loomy::account::SOURCE_*`）。
 *
 * ⚠️ 与另三张卡同一手法：**表外的新取值原样显示**，不隐藏也不猜。
 */
const SOURCE_LABEL_KEY: Record<string, TranslationKey> = {
  import: "loomy.page.accounts.origin.import",
  "sms-login": "loomy.page.accounts.origin.smsLogin",
  "wechat-login": "loomy.page.accounts.origin.wechatLogin",
};

/** 来源的展示名（表外取值原样回落；空值给 `null` 让调用方省掉这一枚）。 */
export function loomySourceLabel(t: Translate, source: string | null | undefined): string | null {
  if (!source) return null;
  const key = SOURCE_LABEL_KEY[source];
  return key ? t(key) : source;
}

/**
 * 账号展示名（卡片 / 表格 / toast / 无障碍名**同一份回落链**）。
 *
 * 回落顺序与另三平台同一纪律：昵称 → 名字 → 手机号 → uid → `—`；
 * **绝不渲染对象**（脏值会让 React 卸整棵树 ⇒ 白屏，issue #2）。
 */
export function loomyAccountName(account: LoomyAccountView): string {
  return (
    displayText(account.nickname) ||
    displayText(account.name) ||
    displayText(account.phone) ||
    displayText(account.uid) ||
    "—"
  );
}

/**
 * 状态徽章（卡片 chips 行与其它分区**同一份口径**）。
 *
 * 优先级：**凭据问题 > 过期 > 正常**。凭据坏了的账号，「今天签没签到」是次要信息
 * —— 它根本调不动上游；把「今日未签」盖在「无可用凭据」上会让用户去点一个必然失败的按钮。
 *
 * ⚠️ `expiresAt` 是**本地推算值**（见文件头取舍 1）⇒ 只在**确实已过期**时报警，
 * 不做「N 天后过期」这种它给不出精度的倒计时。
 */
export function LoomyStatusChips({ account }: { account: LoomyAccountView }) {
  const t = useT();
  if (!account.hasCredential) {
    return (
      <Badge variant="destructive" className={CHIP_CLASS}>
        {t("loomy.page.accounts.chip.noCredential")}
      </Badge>
    );
  }
  if (account.expiresAt > 0 && account.expiresAt <= Date.now()) {
    return (
      <Badge variant="warning" className={CHIP_CLASS}>
        {t("loomy.page.accounts.chip.expired")}
      </Badge>
    );
  }
  if (account.lastCheckin) {
    return (
      <Badge variant="success" className={CHIP_CLASS}>
        {t("loomy.page.accounts.chip.checkedIn")}
      </Badge>
    );
  }
  return (
    <Badge variant="success" className={CHIP_CLASS}>
      {t("loomy.page.accounts.chip.ok")}
    </Badge>
  );
}

export interface LoomyAccountCardProps {
  account: LoomyAccountView;
  /** 是否被勾选（批量操作）。 */
  selected?: boolean;
  onToggleSelect?: (account: LoomyAccountView) => void;
  /** 打开详情（抽屉状态由页面持有）。 */
  onOpenDetail?: (account: LoomyAccountView) => void;
  /** 「更多 → 编辑备注」。 */
  onEditRemark?: (account: LoomyAccountView) => void;
  /** 「更多 → 设置分组」。 */
  onSetGroup?: (account: LoomyAccountView) => void;
  /** 「更多 → 删除账号」（页面弹确认：Loomy 无回收站）。 */
  onDelete?: (account: LoomyAccountView) => void;
  /** 「更多 → 签到」（页面调 `loomy_checkin_claim`）。 */
  onCheckin?: (account: LoomyAccountView) => void;
  /** 「更多 → 刷新积分」（页面调 `loomy_credits`）。 */
  onRefreshCredits?: (account: LoomyAccountView) => void;
  /** 本账号有动作在途（禁用菜单项并显示转圈）。 */
  busy?: boolean;
}

export function LoomyAccountCard({
  account,
  selected = false,
  onToggleSelect,
  onOpenDetail,
  onEditRemark,
  onSetGroup,
  onDelete,
  onCheckin,
  onRefreshCredits,
  busy = false,
}: LoomyAccountCardProps) {
  const t = useT();
  const name = loomyAccountName(account);
  const uid = displayText(account.uid);
  /* 身份行：手机号优先（用户认得出），否则 userid，再否则 uid。 */
  const identity =
    displayText(account.phone) || displayText(account.userid) || (uid ? `UID · ${uid}` : "—");
  const sourceLabel = loomySourceLabel(t, account.source);
  const canAct = account.hasCredential && !busy;

  const menu = demoModeEnabled ? (
    <DemoAction>
      <Button
        variant="ghost"
        size="sm"
        className="!px-[10px] text-muted-foreground hover:bg-accent hover:text-foreground"
        aria-label={t("loomy.page.accounts.card.manageAria", { name })}
        title={t("loomy.page.accounts.card.moreActions")}
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
          aria-label={t("loomy.page.accounts.card.manageAria", { name })}
          title={t("loomy.page.accounts.card.moreActions")}
        >
          <Ellipsis className="size-4" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-56">
        <MenuHead>{displayText(account.name) || uid || name}</MenuHead>
        <DropdownMenuItem disabled={!onOpenDetail} onSelect={() => onOpenDetail?.(account)}>
          <Info />
          {t("loomy.page.accounts.card.viewDetail")}
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <MenuGroupLabel>{t("shared.cardMenu.refreshGroup")}</MenuGroupLabel>
        {/* 签到与刷新积分都是**上游写/读动作**，凭据不可用时一律禁用（点了必然失败）。 */}
        <DropdownMenuItem disabled={!canAct || !onCheckin} onSelect={() => onCheckin?.(account)}>
          <RefreshCw />
          {t("loomy.page.accounts.card.checkin")}
        </DropdownMenuItem>
        <DropdownMenuItem
          disabled={!canAct || !onRefreshCredits}
          onSelect={() => onRefreshCredits?.(account)}
        >
          <RefreshCw />
          {t("shared.cardMenu.refresh.credits")}
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <MenuGroupLabel>{t("shared.cardMenu.accountGroup")}</MenuGroupLabel>
        <DropdownMenuItem disabled={!onEditRemark} onSelect={() => onEditRemark?.(account)}>
          <StickyNote />
          {t("loomy.page.accounts.card.editRemark")}
        </DropdownMenuItem>
        <DropdownMenuItem disabled={!onSetGroup} onSelect={() => onSetGroup?.(account)}>
          <FolderTree />
          {t("loomy.page.accounts.card.setGroup")}
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem
          className="text-destructive focus:bg-destructive/5 focus:text-destructive"
          onSelect={() => onDelete?.(account)}
        >
          <Trash2 />
          {t("loomy.page.accounts.card.deleteAccount")}
        </DropdownMenuItem>
        <MenuNote>{t("shared.cardMenu.note")}</MenuNote>
      </DropdownMenuContent>
    </DropdownMenu>
  );

  return (
    <article
      tabIndex={0}
      role="article"
      onClick={(event) => {
        if (!onOpenDetail) return;
        const target = event.target;
        if (
          target instanceof Element &&
          target.closest(
            "button, a, input, [role='checkbox'], [role='switch'], [data-no-account-open]",
          )
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
      {/* header：勾选 + 头像 32 + 昵称/身份 —— 与另三张卡逐块对应。 */}
      <header className="flex items-center gap-2.5">
        <div className="relative z-10 flex w-full min-w-0 items-center gap-2.5">
          <Checkbox
            checked={selected}
            onCheckedChange={() => onToggleSelect?.(account)}
            disabled={!onToggleSelect}
            aria-label={t("loomy.page.accounts.card.selectAria", { name })}
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
          {busy && (
            <RefreshCw className="size-3.5 shrink-0 animate-spin text-muted-foreground" />
          )}
        </div>
      </header>

      {/* chips：状态槽（恒一枚） + 来源徽章 + 分组（有才渲染）。 */}
      <div className="flex min-w-0 flex-wrap items-center gap-[5px]">
        <LoomyStatusChips account={account} />
        {sourceLabel ? (
          <Badge variant="secondary" className={cn(CHIP_CLASS, "text-muted-foreground")}>
            {sourceLabel}
          </Badge>
        ) : null}
        {displayText(account.group) ? (
          <Badge variant="secondary" className={cn(CHIP_CLASS, "text-muted-foreground")}>
            {displayText(account.group)}
          </Badge>
        ) : null}
      </div>

      {/* body：两池积分分列（`0` 是真实读数，恒显示）+ 令牌掩码（**只给掩码**）。 */}
      <section className="flex min-w-0 flex-1 flex-col gap-1">
        <div className="flex min-w-0 flex-wrap items-baseline gap-x-3 gap-y-0.5 text-[11.5px]">
          <span className="text-muted-foreground">
            {t("loomy.page.accounts.metric.permanent")}{" "}
            <span className="font-medium tabular-nums text-foreground">{account.credits}</span>
          </span>
          <span className="text-muted-foreground">
            {t("loomy.page.accounts.metric.daily")}{" "}
            <span className="font-medium tabular-nums text-foreground">
              {account.dailyCredits}
            </span>
          </span>
        </div>
        {account.accessTokenMasked ? (
          <div
            className="truncate font-mono text-[11px] text-muted-foreground"
            title={account.accessTokenMasked}
          >
            {account.accessTokenMasked}
          </div>
        ) : null}
        {account.lastCheckin ? (
          <div className="truncate text-[11px] text-muted-foreground">
            {t("loomy.page.accounts.metric.lastCheckin", { at: account.lastCheckin })}
          </div>
        ) : null}
      </section>

      {/* footer：**不渲染程序位行**（见文件头取舍 2）⇒ 只有更多菜单右对齐。 */}
      <footer className="flex items-center gap-1.5 border-t border-border pt-2.5">
        <div className="grow" />
        {menu}
      </footer>
    </article>
  );
}
