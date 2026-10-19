/* ==========================================================================
   Buddy Switch 原型 — 状态、渲染与交互
   刻意用「一个 state 对象 + 数据驱动的 render」结构，对照真实实现里的
   useScope() + 工作台组件，方便你判断这套模型落到 React 后的形态。
   ========================================================================== */

/* ─────────── 状态 ─────────── */
const state = {
  scope: "wb-cn",
  dest: "overview",
  theme: "dark",
  /* 账号工作台 */
  q: "", sort: "priority", group: "none", view: "card",
  selected: new Set(),
  /* 卡片「更多」菜单：打开中的账号 id（`null` = 关着）。 */
  acctMenu: null,
  /* 正在刷新的**单项数据**：key = `${id}:${kind}`，kind ∈ credits / tokens / checkin / credential。
     用集合而不是一个布尔：四类数据各自独立，同时刷两项时互不吞掉对方的进行中态。 */
  refreshing: new Set(),
  /* 刚刷完、还在显示「已更新」的项（900ms 后自动清掉，避免永久挂着一个完成态）。 */
  refreshDone: new Set(),
  /* 正在「新建分组」的账号 id（下拉里选「＋ 新建分组…」后就地变成输入框）。 */
  groupNew: null,
  /* 用量工作台 */
  usageTab: "trend", range: 30, rangeMode: "preset", customFrom: "", customTo: "", rangeOpen: false, merge: false,
  /* 子导航 */
  gwSub: "overview",
  gwApi: "openai",      // 快速接入的协议 Tab：openai / anthropic
  gwScanning: false,    // 模型清单「识别中」（§3.17）
  /* Trae 上下文策略（Max Mode）：always = 支持表内模型始终注入 is_max_mode；off = 回到上游默认窗口。
     Max Mode 是**请求级开关**，不是模型 —— 所以清单里一个模型只有一条记录，没有 `-max` 影子 id。 */
  maxMode: "always",
  gwTool: "codex",      // 接入说明里选中的客户端（§3.18.3）
  setSub: "general",
  /* 会话工作台（v2.3）：客户端 Tab + 状态筛选 + 排序 */
  sessionClient: "workbuddy",
  sessionFilter: "all",
  sessionSort: "recent",
  /* ★ P14 S4a · 插件侧「复制会话」抽屉的状态。
     `extVariant` 只是**原型的演示变体**（ok / empty / missing 三种真实返回形状），
     真实实现里它由 `list_vscode_sessions` 的返回决定（`dataRoot` 是否为 null）。 */
  extVariant: "ok",
  extTarget: null,
  extPicked: new Set(),
  /* 账号池「显式指定」：每个作用域最多一个（排他）。{ [scope]: accountName } */
  pins: {},
  /* 抽屉 */
  drawer: null,        // { kind, id, step, ... }
  /* 命令面板 */
  palette: { open: false, q: "", cursor: 0 },
  loading: false,
  /* 错误态：键为场景（gateway / usage），为 true 时对应页面渲染行内错误态。
     用可寻址状态驱动，才能用 ?err=gateway 深链复现。 */
  errors: {},
  /* 桌面悬浮窗是否显示（§3.16 / D21）：入口在侧栏左下角 + 托盘。 */
  companionOpen: true,
  /* 请求代理开关（§3.19 / P12）：**默认关**（D29 拍板）。 */
  proxyOn: false,
  /* 最近一次批量签到的结果：{ ok, failIds }。null 表示没有可展示的汇总。 */
  checkinResult: null,
  /* 随机抖动（issue #27 的「另一支」）：**默认关** —— 打开才改变触发时刻，
     默认值不动任何既有行为。幅度三档见 `JITTER_STEPS`。 */
  jitter: { on: false, minutes: 15 },
  /* 全局备份（§3.14）：parts 懒初始化为「全部作用域 × 全部内容」。
     creds 默认 false —— 安全默认，与 §2.5「默认拒绝」同源。 */
  backup: {
    parts: {},
    creds: false,
    conflict: "keep-local",
    preview: false,
    result: null,
  },
  /* 升级前说明弹窗（版本内容）：stage = available / downloading / ready / error。
     `seenVersion` 记住关掉时看过的版本，供「自动检查不重复打扰」判断。 */
  update: { open: false, stage: "available", progress: 0, olderOpen: false, seenVersion: null },
};

/* ─────────── 工具 ─────────── */
const $ = (s) => document.querySelector(s);
const esc = (s) => String(s == null ? "" : s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
/* 邮箱脱敏（§3.3 / D22 已拍板）：本地段只留首字符。
   这是**展示层的隐私默认**，不是安全边界 —— 真实值仍在本地库，
   防的是截图 / 投屏 / 求助时泄露完整邮箱（本项目的用户经常贴截图报障）。
   ⚠️ 搜索用**原值**，展示用**脱敏值**：两者不能混。 */
const maskEmail = (email) => {
  const s = String(email || "");
  const at = s.indexOf("@");
  if (at <= 0) return s;
  const local = s.slice(0, at);
  return local.slice(0, 1) + "*".repeat(Math.max(3, local.length - 1)) + s.slice(at);
};

const fmt = (n) => Number(n || 0).toLocaleString("zh-CN");
/* 资源包数量：最多两位小数、去掉多余的 0（1000 → 1,000；30.86 → 30.86；6 → 6）。 */
const fmtQty = (n) => Number(Number(n || 0).toFixed(2)).toLocaleString("zh-CN", { maximumFractionDigits: 2 });
/* 到期时刻：MM-DD HH:mm —— 资源包最多几十天到期，再带年份是噪声。 */
function fmtMonthDay(ts) {
  const d = new Date(ts);
  if (!Number.isFinite(ts) || Number.isNaN(d.getTime())) return "";
  const p = (n) => String(n).padStart(2, "0");
  return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}
const fmtTok = (n) => (n >= 1e6 ? (n / 1e6).toFixed(1) + "M" : n >= 1e3 ? (n / 1e3).toFixed(1) + "K" : String(n || 0));
const curScope = () => SCOPES[state.scope];
const curPlatform = () => PLATFORMS[curScope().platform];
/* 展示名的拼装**只有一处实现**（`demo-data.js` 的 `scopeDisplayName`）——
   这里只是取个短名字，方便页面代码继续读作 `scopeLabel(...)`。 */
const scopeLabel = (id) => scopeDisplayName(id);
const currentAccount = (id) => {
  const s = SCOPES[id];
  const p = PLATFORMS[s.platform];
  return s.accounts.find((a) => a.programs[p.programs[0].id]) || null;
};
const expiredCount = (s) => s.accounts.filter((a) => a.expired || a.needsRelogin).length;

function avatarTone(name) {
  /* 亮度压到 L ≤ 0.5，压在上面的白字（oklch .99）才能过 AA 4.5:1。
     色相保持不变，只降亮度。 */
  const tones = ["oklch(0.5 0.13 163)", "oklch(0.48 0.14 265)", "oklch(0.5 0.13 300)", "oklch(0.5 0.14 60)", "oklch(0.48 0.13 20)", "oklch(0.5 0.12 220)"];
  let h = 0;
  for (const ch of name) h = (h * 31 + ch.charCodeAt(0)) % 997;
  return tones[h % tones.length];
}
const initials = (name) => (name || "?").replace(/[^a-zA-Z0-9\u4e00-\u9fa5]/g, "").slice(0, 2).toUpperCase();

/* ─────────── toast ─────────── */
let toastSeq = 0;
/* note：写给评审看的「原型说明」，与面向用户的产品文案视觉分离（.proto-note）。 */
function toast(kind, title, desc, action, note) {
  const id = "t" + ++toastSeq;
  const ico = kind === "err" ? ICON.warn : ICON.check;
  const node = document.createElement("div");
  node.className = "toast " + (kind === "err" ? "err" : "ok");
  node.id = id;
  node.innerHTML = `<span class="ico">${ico}</span><span class="t"><b>${esc(title)}</b>${desc ? `<span>${esc(desc)}</span>` : ""}${note ? `<span class="proto-note">${esc(note)}</span>` : ""}</span>` +
    (action ? `<button class="btn xs" data-act="toast-action|${id}">${esc(action.label)}</button>` : "");
  $("#toasts").appendChild(node);
  if (action) pendingActions[id] = action.run;
  setTimeout(() => { node.style.transition = "opacity .2s, transform .2s"; node.style.opacity = "0"; node.style.transform = "translateY(6px)"; setTimeout(() => node.remove(), 220); }, action ? 6000 : 2800);
}
const pendingActions = {};

/* ─────────── 骨架渲染 ─────────── */
function renderTopbar() {
  const s = curScope(), p = curPlatform();
  $("#scope-mark").innerHTML = p.mark;
  $("#scope-mark").style.background = `color-mix(in oklab, ${p.accent} 20%, transparent)`;
  $("#scope-mark").style.color = p.accent;
  $("#scope-name").textContent = scopeLabel(state.scope);
  const cur = currentAccount(state.scope);
  $("#scope-sub").textContent = (cur ? cur.name + (cur.email ? " · " : "") : "") + `${s.accounts.length} 个账号`;
  const pill = $("#run-pill");
  pill.className = "run-pill" + (s.running ? "" : " off");
  pill.innerHTML = `<i class="dot"></i><span>${s.running ? "运行中" : s.installed ? "未运行" : "未检测到"}</span>`;
  $("#foot-dot").style.color = s.running ? "var(--success)" : "var(--muted-fg)";
}

function renderNav() {
  const s = curScope(), p = curPlatform();
  const badges = {
    accounts: { n: s.accounts.length, warn: expiredCount(s) > 0 },
    gateway: { n: s.gatewayKeys, warn: false },
    /* 待同步会话数：含「落后」与「分叉」，让用户知道有事要处理 */
    sessions: sessionPendingCount() ? { n: sessionPendingCount(), warn: true } : null,
  };
  /* 目的地按**平台能力**过滤（§2.7 的平台固有维度）：缺能力就不渲染入口。
     ⚠️ 只是「不渲染入口」—— **URL 直达仍然可用**，由页面自己给空态（§3.2 规则 2）。 */
  const dests = DESTINATIONS.filter((d) => !d.cap || p.capabilities.includes(d.cap));
  $("#nav").innerHTML = dests.map((d) => {
    const b = badges[d.id];
    return `<button class="nav-item${state.dest === d.id ? " active" : ""}" data-act="nav|${d.id}">
      ${d.icon}<span class="nav-label">${d.label}</span>
      ${b && b.n ? `<span class="nav-badge${b.warn ? " warn" : ""}">${b.n}</span>` : ""}
    </button>`;
  }).join("");
}

function renderContent() {
  if (state.loading) { $("#content").innerHTML = skeleton(); return; }
  const map = { overview: viewOverview, accounts: viewAccounts, usage: viewUsage, sessions: viewSessions, gateway: viewGateway, settings: viewSettings };
  $("#content").innerHTML = map[state.dest]();
  $("#content").scrollTop = 0;
  /* D1：账号页渲染后立刻再同步一次列表区。工具栏里的「全选」按钮与列表区
     必须出自同一个 allSel 计算——否则视图里硬编码的初始值会造成
     「首屏按钮语义错误、要点两次才生效」。以后改列表逻辑不会再漏同步按钮。 */
  if (state.dest === "accounts") renderAccountList();
}

function skeleton() {
  return `<div class="page">
    <div class="sk" style="height:26px;width:220px;margin-bottom:10px"></div>
    <div class="sk" style="height:14px;width:340px;margin-bottom:24px"></div>
    <div class="grid g4" style="margin-bottom:20px">
      ${Array.from({ length: 4 }, () => `<div class="card stat"><div class="sk" style="height:12px;width:60%"></div><div class="sk" style="height:24px;width:40%;margin-top:10px"></div></div>`).join("")}
    </div>
    <div class="grid g3">
      ${Array.from({ length: 6 }, () => `<div class="card card-pad"><div class="sk" style="height:34px;width:34px;border-radius:10px"></div><div class="sk" style="height:14px;width:70%;margin-top:12px"></div><div class="sk" style="height:12px;width:50%;margin-top:8px"></div><div class="sk" style="height:28px;width:100%;margin-top:14px"></div></div>`).join("")}
    </div>
  </div>`;
}

function renderAll() { renderTopbar(); renderNav(); renderContent(); renderCompanion(); }

/* ══════════════════════ 桌面悬浮窗（§3.16 / D21 已拍板） ══════════════════════
   独立浮层，**只读** —— 不承载任何写操作（§3.16 明确「不做的」）。
   ⚠️ 它是**独立窗口** ⇒ 键盘可达性必须**单独验证**，不能假定主窗口的焦点管理适用。 */
const COMPANION_STATE = {
  running: { label: "运行中", cls: "run" },
  waiting: { label: "待确认", cls: "wait" },
  done: { label: "已完成", cls: "done" },
  failed: { label: "失败", cls: "fail" },
};

function renderCompanion() {
  const slot = $("#companion-slot");
  if (!slot) return;
  if (!state.companionOpen) { slot.innerHTML = ""; return; }
  const rows = COMPANION_SOURCES.map((c) => {
    const m = COMPANION_STATE[c.state] || COMPANION_STATE.done;
    /* 标题默认模糊（防截图 / 投屏泄露），悬停才看清 —— 与 §3.3 的邮箱脱敏同一原则。 */
    return `<button class="cp-row" data-act="cp-jump|${c.id}" title="${esc(c.title)}">
      <i class="cp-dot ${m.cls}"></i>
      <span class="cp-text"><b>${esc(c.source)}</b><span class="cp-title">${esc(c.title)}</span></span>
      <span class="cp-hint">${esc(c.hint)}</span>
    </button>`;
  }).join("");
  const waiting = COMPANION_SOURCES.filter((c) => c.state === "waiting").length;
  slot.innerHTML = `<aside class="companion" role="complementary" aria-label="会话状态悬浮窗（只读）">
    <header class="cp-head">
      <i class="cp-live"></i><b>会话状态</b>
      ${waiting ? `<span class="badge warn">${waiting} 待确认</span>` : ""}
      <span class="grow"></span>
      <button class="cp-x" data-act="cp-close" title="隐藏悬浮窗" aria-label="隐藏悬浮窗">×</button>
    </header>
    <div class="cp-body">${rows}</div>
    <footer class="cp-foot">只读 · 点击回到原会话 · 标题悬停显示</footer>
  </aside>`;
}

/* ══════════════════════ 概览 ══════════════════════ */
function viewOverview() {
  const s = curScope(), p = curPlatform();
  const cur = currentAccount(state.scope);
  /* 概览永远看「当前作用域」，不受用量页的合并开关影响。 */
  const u = USAGE[state.scope];
  const today = u.days[u.days.length - 1];
  const prev = u.days[u.days.length - 2] || today;
  /* 「对比昨日」用 days 的最后两天真实计算，不再硬编码 +12%。 */
  const delta = prev.value ? Math.round(((today.value - prev.value) / prev.value) * 100) : 0;
  const deltaTone = delta >= 0 ? "up" : "down";
  const deltaText = `对比昨日 ${delta >= 0 ? "+" : ""}${delta}%`;

  /* 待处理项：这才是「概览」存在的意义——回答「有什么要我处理」 */
  const todos = [];
  const expired = s.accounts.filter((a) => a.expired);
  const relogin = s.accounts.filter((a) => a.needsRelogin);
  const notChecked = s.accounts.filter((a) => !a.checkedIn && !a.expired);
  const expiring = s.accounts.filter((a) => a.expiring > 0 && a.expireInDays <= 5);
  if (relogin.length) todos.push({ tone: "bad", title: `${relogin.length} 个账号需要重新登录`, desc: relogin.map((a) => a.name).join("、"), act: "去处理", dest: "accounts" });
  if (expired.length) todos.push({ tone: "bad", title: `${expired.length} 个账号凭据已过期`, desc: expired.map((a) => a.name).join("、"), act: "去处理", dest: "accounts" });
  if (notChecked.length) todos.push({ tone: "warn", title: `${notChecked.length} 个账号今日未签到`, desc: notChecked.map((a) => a.name).join("、"), act: "一键签到", run: "batch-checkin" });
  if (expiring.length) todos.push({ tone: "warn", title: `${expiring.length} 个账号的积分将在 5 天内过期`, desc: `共 ${fmt(expiring.reduce((x, a) => x + a.expiring, 0))} 积分`, act: "查看", dest: "accounts" });
  if (!s.running) todos.push({ tone: "info", title: "客户端当前未运行", desc: "账号切换需要客户端在运行状态", act: "去设置", dest: "settings" });
  if (!s.accounts.length) todos.push({ tone: "info", title: "这个作用域还没有账号", desc: "先添加一个账号才能使用", act: "添加账号", run: "add-account" });

  const otherScopes = Object.keys(SCOPES).filter((id) => id !== state.scope);

  return `<div class="page">
    <div class="page-head">
      <div>
        <h1>概览</h1>
        <p>${scopeLabel(state.scope)} · 客户端版本 ${s.version} · ${cur ? `当前登录 <b>${esc(cur.name)}</b>` : "<b>未登录</b>"}</p>
      </div>
      <div class="grow"></div>
      <button class="btn" data-act="refresh">${ICON.refresh}刷新</button>
    </div>

    ${checkinResultHtml()}

    <!-- 待处理优先：概览先回答「有什么要我处理」，统计块退到其后 -->
    <section class="card" style="margin-bottom:20px">
      <div class="card-head">
        <h3>待处理</h3>
        <div class="grow"></div>
        <span class="sub">${todos.length} 项</span>
      </div>
      ${todos.length ? todos.map((t) => `
        <div class="srow">
          <span class="badge ${t.tone}">${ICON.warn}</span>
          <div class="lab"><b>${esc(t.title)}</b><span>${esc(t.desc)}</span></div>
          ${t.run ? `<button class="btn sm" data-act="${t.run}">${esc(t.act)}</button>`
                  : `<button class="btn sm" data-act="nav|${t.dest}">${esc(t.act)}</button>`}
        </div>`).join("")
      : `<div class="empty" style="padding:40px 20px">
           <div class="ico">${ICON.check}</div>
           <b>一切正常</b><p>没有需要你处理的事项。</p>
         </div>`}
    </section>

    <div class="grid g4" style="margin-bottom:20px">
      ${statTile("账号总数", String(s.accounts.length), `${expiredCount(s)} 个需处理`, expiredCount(s) ? "down" : "")}
      ${statTile("今日 Token", fmtTok(today.value), deltaText, deltaTone)}
      ${statTile("今日积分消耗", String(s.todayCredits), `剩余合计 ${fmt(s.accounts.reduce((x, a) => x + a.credits, 0))}`)}
      ${statTile("网关请求（今日）", fmt(s.gatewayRequests), `${s.gatewayKeys} 个密钥 · 累计见网关页`)}
    </div>

    <div class="split">
      <div class="split-main">
        <section class="card">
          <div class="card-head">
            <h3>近 14 天 Token 用量</h3>
            <div class="grow"></div>
            <button class="btn ghost sm" data-act="nav|usage">查看详情 ${ICON.arrowRight}</button>
          </div>
          <div class="card-pad">${barChart(u.days.slice(-14), 120)}</div>
        </section>
      </div>

      <div style="width:290px;flex-shrink:0;display:flex;flex-direction:column;gap:16px" class="ov-side">
        <section class="card">
          <div class="card-head"><h3>其他作用域</h3></div>
          <div style="padding:8px">
            ${otherScopes.map((id) => {
              const os = SCOPES[id], op = PLATFORMS[os.platform];
              const oc = currentAccount(id);
              return `<button class="scope-item" data-act="scope|${id}" style="width:100%">
                <span class="scope-mark" style="background:color-mix(in oklab, ${op.accent} 18%, transparent);color:${op.accent}">${op.mark}</span>
                <span class="t"><b>${scopeLabel(id)}</b><span>${oc ? "已登录 " + esc(oc.name) : os.installed ? "未登录" : "未检测到"} · ${os.accounts.length} 个账号</span></span>
                <span class="run-pill${os.running ? "" : " off"}" style="height:18px;padding:0 7px"><i class="dot"></i></span>
              </button>`;
            }).join("")}
          </div>
        </section>

        <section class="card">
          <div class="card-head"><h3>能力</h3></div>
          <div class="card-pad row wrap" style="gap:6px">
            ${p.capabilities.map((c) => `<span class="badge info">${capLabel(c)}</span>`).join("")}
          </div>
        </section>
      </div>
    </div>
  </div>`;
}

function statTile(k, v, d, tone) {
  return `<div class="card stat"><div class="k">${k}</div><div class="v">${v}</div>${d ? `<div class="d ${tone || ""}">${d}</div>` : ""}</div>`;
}
function capLabel(c) {
  return { checkin: "自动签到", travel: "自动旅行", credits: "积分统计", oauth: "OAuth 登录", snapshot: "登录态快照", group: "账号分组", gateway: "API 网关", autoRotate: "自动轮换" }[c] || c;
}
/** 坐标轴短标签：`10/1` 取 `1`；没有斜杠的（如 `D+3`）原样返回。 */
const shortLabel = (l) => (String(l).includes("/") ? String(l).split("/")[1] : String(l));

function barChart(days, h, step) {
  const max = Math.max(1, ...days.map((d) => d.value));
  const st = step || 1;   // 项数多时每隔 st 个才标一次，避免标签糊成一片
  return `<div class="bars" style="height:${h}px">${days.map((d, i) => `
    <div class="bar"><div class="tip">${d.label} · ${fmt(d.value)}</div>
      <i style="height:${Math.max(2, (d.value / max) * (h - 22))}px"></i>
      <em>${i % st ? "" : shortLabel(d.label)}</em></div>`).join("")}</div>`;
}

/* ══════════════════════ 图表原语 ══════════════════════
   全部纯内联 SVG / CSS：零依赖、零外链，离线可开。
   约定（三条，改图时必须守住）：
     1. 每张图都带 `role="img"` + 中文 `aria-label`；
     2. 每个数据点都给 `<title>`/`title`，让看不了图的人拿到同样的数字；
     3. 不在图里写死数字——一律从传入的数据算，避免「图与表打架」。 */
const SERIES_VAR = (k) => `var(--s${(k % 8) + 1})`;

/** 通用图例。 */
function legend(items) {
  return `<div class="legend">${items.map((it, k) =>
    `<span><i style="background:${SERIES_VAR(k)}"></i>${esc(it.name)}</span>`).join("")}</div>`;
}

/** 堆叠柱：各分段之和**精确等于**当日总量（由数据层保证，见 demo-data.js 的 buildUsageExtras）。 */
function stackedBar(days, series, h, label) {
  const max = Math.max(1, ...days.map((d) => d.value));
  const step = days.length > 14 ? 5 : 1;   // 天数多时隔几个才标一次日期，避免糊成一片
  return `<div class="chart cbar" style="height:${h}px" role="img" aria-label="${esc(label)}">
    ${days.map((d, i) => {
      let acc = 0;
      const segs = series.map((s, k) => {
        const v = s.values[i] || 0;
        const pct = (v / max) * 100, bottom = (acc / max) * 100;
        acc += v;
        return `<i style="height:${pct.toFixed(3)}%;bottom:${bottom.toFixed(3)}%;background:${SERIES_VAR(k)}"></i>`;
      }).join("");
      const tip = `${d.label} · 合计 ${fmt(d.value)} ｜ ` + series.map((s) => `${s.name} ${fmt(s.values[i] || 0)}`).join(" / ");
      return `<span class="ccol" title="${esc(tip)}">${segs}<em class="${i % step ? "hide" : ""}">${shortLabel(d.label)}</em></span>`;
    }).join("")}</div>`;
}

/** 累计/余额面积图。
 *  ★ `anchorZero:false` 是给「高位小幅波动」的序列（如积分余额）用的：
 *    若一律从 0 起画，6406→7910 的波动会被压成一条直线，**视觉上等于在撒谎说「没变」**。
 *    改从最小值附近取基线，并在图注里写明基线，避免读者误以为轴从 0 开始。 */
let _areaSeq = 0;
function areaChart(points, h, label, opts) {
  const o = opts || {};
  const vals = points.map((p) => p.value);
  const max = Math.max(1, ...vals);
  const min = Math.min(...vals);
  const lo = o.anchorZero === false ? Math.max(0, min - (max - min) * 0.35) : 0;
  const span = max - lo || 1;
  const n = points.length;
  const xy = points.map((p, i) => [n === 1 ? 50 : (i / (n - 1)) * 100, 100 - ((p.value - lo) / span) * 96]);
  const line = xy.map((p, i) => `${i ? "L" : "M"}${p[0].toFixed(2)} ${p[1].toFixed(2)}`).join(" ");
  const id = "ag" + (++_areaSeq);   // 同页多个面积图时 id 不能撞
  const cap = o.caption || (o.anchorZero === false
    ? `${fmt(vals[0])} → ${fmt(vals[n - 1])}　（基线 ${fmt(Math.round(lo))}，非 0）`
    : `累计至 ${fmt(vals[n - 1])} · 单日峰值 ${fmt(Math.max(...points.map((p, i) => (i ? p.value - points[i - 1].value : p.value))))}`);
  return `<div class="chart carea" style="height:${h}px" role="img" aria-label="${esc(label)}">
    <svg viewBox="0 0 100 100" preserveAspectRatio="none">
      <defs><linearGradient id="${id}" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0%" stop-color="var(--primary)" stop-opacity=".38"/>
        <stop offset="100%" stop-color="var(--primary)" stop-opacity="0"/>
      </linearGradient></defs>
      <path d="${line} L100 100 L0 100 Z" fill="url(#${id})"/>
      <path d="${line}" fill="none" stroke="var(--primary)" stroke-width="1.6" vector-effect="non-scaling-stroke"/>
    </svg>
    <figcaption>${cap}</figcaption>
  </div>`;
}

/** 环形图 + 图例。 */
function donut(items, size, label, unit) {
  const total = items.reduce((s, i) => s + i.value, 0) || 1;
  const R = 42, C = 2 * Math.PI * R;
  let acc = 0;
  const arcs = items.map((it, k) => {
    const frac = it.value / total, dash = frac * C, off = acc * C;
    acc += frac;
    return `<circle r="${R}" cx="50" cy="50" fill="none" stroke="${SERIES_VAR(k)}" stroke-width="13"
      stroke-dasharray="${dash.toFixed(3)} ${(C - dash).toFixed(3)}" stroke-dashoffset="${(-off).toFixed(3)}"
      transform="rotate(-90 50 50)"><title>${esc(it.name)} · ${fmt(it.value)}${unit || ""}（${Math.round(frac * 100)}%）</title></circle>`;
  }).join("");
  return `<div class="cdonut-wrap">
    <div class="cdonut" style="width:${size}px;height:${size}px" role="img" aria-label="${esc(label)}">
      <svg viewBox="0 0 100 100">${arcs}
        <text x="50" y="48" text-anchor="middle" class="ct-total">${fmtTok(total)}</text>
        <text x="50" y="60" text-anchor="middle" class="ct-sub">合计</text></svg>
    </div>
    <div class="grow" style="min-width:180px">${legend(items)}</div>
  </div>`;
}

/** 时段热力图：行 = 星期，列 = 小时。取**相对强度**，刻意不声称是绝对 Token。 */
function heatmap(matrix, rowLabels, label) {
  const max = Math.max(1, ...matrix.flat());
  return `<div class="chart cheat" role="img" aria-label="${esc(label)}">
    <div class="cheat-rows">${matrix.map((row, ri) =>
      `<div class="cheat-row"><span class="rl">${esc(rowLabels[ri])}</span><span class="cells">${
        row.map((v, ci) => `<i style="opacity:${(0.08 + (v / max) * 0.92).toFixed(2)}"
          title="${esc(rowLabels[ri])} ${String(ci).padStart(2, "0")}:00 · 相对强度 ${Math.round((v / max) * 100)}%"></i>`).join("")
      }</span></div>`).join("")}</div>
    <div class="cheat-x">${[0, 6, 12, 18, 23].map((hh) => `<span style="left:${(hh / 23) * 100}%">${hh}:00</span>`).join("")}</div>
  </div>`;
}

/** 双向柱：上=入账、下=出账，零线居中。 */
function divergingBars(items, h, label, inName, outName) {
  const max = Math.max(1, ...items.flatMap((i) => [i.inAmt, i.outAmt]));
  const step = items.length > 14 ? 5 : 1;
  return `<div class="chart cdiv" style="height:${h}px" role="img" aria-label="${esc(label)}">
    <span class="zero"></span>
    ${items.map((it, i) => `<span class="dcol" title="${esc(it.label)} ｜ ${inName} +${fmt(it.inAmt)} ｜ ${outName} -${fmt(it.outAmt)}">
      <i class="up" style="height:${((it.inAmt / max) * 48).toFixed(3)}%"></i>
      <i class="dn" style="height:${((it.outAmt / max) * 48).toFixed(3)}%"></i>
      <em class="${i % step ? "hide" : ""}">${shortLabel(it.label)}</em></span>`).join("")}</div>`;
}

/** 气泡散点：x = 消息数，y = Token，气泡大小随 Token。
 *  ★ 坐标要按**气泡半径**留边（半径最大 26px，绘图区高约 180px ⇒ 竖向占 14%），
 *    否则最大的那个气泡会溢出绘图区边框。 */
function scatterChart(points, h, label, xName, yName) {
  const maxX = Math.max(1, ...points.map((p) => p.x));
  const maxY = Math.max(1, ...points.map((p) => p.y));
  return `<div class="chart cscatter" style="height:${h}px" role="img" aria-label="${esc(label)}">
    <span class="ax y">${esc(yName)} ↑</span>
    <span class="ax x">${esc(xName)} →</span>
    <span class="plot">${points.map((p) => {
      const r = Math.round(9 + (p.y / maxY) * 17);
      return `<i class="bub" style="left:${((p.x / maxX) * 84 + 8).toFixed(2)}%;bottom:${((p.y / maxY) * 70 + 15).toFixed(2)}%;width:${r}px;height:${r}px"
        title="${esc(p.label)} · ${p.x} ${xName} · ${fmt(p.y)} ${yName}"></i>`;
    }).join("")}</span></div>`;
}

/** 迷你趋势线（列表/表格内联用）。 */
function sparkline(values, w, h) {
  if (!values || values.length < 2) return "";
  const max = Math.max(...values), min = Math.min(...values), span = max - min || 1;
  const pts = values.map((v, i) => `${((i / (values.length - 1)) * 100).toFixed(1)},${(100 - ((v - min) / span) * 100).toFixed(1)}`).join(" ");
  return `<svg class="cspark" viewBox="0 0 100 100" preserveAspectRatio="none" style="width:${w || 62}px;height:${h || 18}px" aria-hidden="true">
    <polyline points="${pts}" fill="none" stroke="var(--primary)" stroke-width="1.8" vector-effect="non-scaling-stroke"/></svg>`;
}

/** 横向排行条（可带迷你趋势线）。 */
function hbarList(items, opts) {
  const o = opts || {};
  const max = Math.max(1, ...items.map((i) => i.value));
  return items.map((it, k) => `<div class="hbar">
    <span class="nm" title="${esc(it.name)}">${esc(it.name)}</span>
    ${o.spark ? sparkline(it.spark, o.sparkW, o.sparkH) : ""}
    <span class="tr"><i style="width:${((it.value / max) * 100).toFixed(1)}%;background:${o.color || SERIES_VAR(k)}"></i></span>
    <span class="vv">${o.fmt ? o.fmt(it) : fmtTok(it.value)}</span></div>`).join("");
}

/* 统一错误态：行内 Alert（role=alert，--destructive 描边）+ 明确的恢复动作。
   全站五态（加载 / 空 / 错误 / 部分失败 / 正常）里，错误态此前只有 toast。 */
function errorState(o) {
  o = o || {};
  return `<div class="errstate" role="alert">
    <span class="ico">${ICON.warn}</span>
    <div class="lab"><b>${esc(o.title || "出错了")}</b>${o.desc ? `<span>${esc(o.desc)}</span>` : ""}</div>
    <div class="grow"></div>
    ${o.action ? `<button class="btn sm" data-act="${esc(o.action.act)}">${ICON.refresh}${esc(o.action.label || "重试")}</button>` : ""}
  </div>`;
}

/* 积分页顶部：一个可点入口，用于复现「刷新失败 → 错误态 → 重试」的闭环。 */
function creditsHeadBar() {
  return `<div class="row" style="margin-bottom:12px">
    <span class="tiny muted">积分余额来自上游，可手动刷新</span>
    <div class="grow"></div>
    <button class="btn sm" data-act="usage-refresh-credits">${ICON.refresh}刷新积分</button>
  </div>`;
}

/* 批量签到的结果汇总条：成功 N / 失败 M + 失败账号名 + 重试入口。
   批量操作必然部分失败，不能让「一键签到」看起来永远 100% 成功。 */
function checkinResultHtml() {
  const r = state.checkinResult;
  if (!r) return "";
  const fails = (r.failIds || []).map((id) => {
    const a = curScope().accounts.find((x) => x.id === id);
    return a ? a.name : id;
  });
  const ok = r.ok || 0;
  return `<div class="result-bar${fails.length ? " has-fail" : " all-ok"}" role="status" style="margin-bottom:16px">
    <span class="badge ${fails.length ? "warn" : "ok"}">${fails.length ? ICON.warn : ICON.check}成功 ${ok} · 失败 ${fails.length}</span>
    ${fails.length ? `<span class="tiny muted">失败账号：${fails.map(esc).join("、")}</span>` : `<span class="tiny muted">全部账号已签到</span>`}
    <div class="grow"></div>
    ${fails.length ? `<button class="btn sm" data-act="retry-checkin">${ICON.refresh}重试失败项</button>` : ""}
    <button class="btn ghost sm" data-act="dismiss-result">知道了</button>
  </div>`;
}

/* ══════════════════════ 账号 ══════════════════════ */
function filteredAccounts() {
  const s = curScope();
  let list = s.accounts.slice();
  const q = state.q.trim().toLowerCase();
  if (q) list = list.filter((a) => [a.name, a.email, a.remark, a.group].some((x) => (x || "").toLowerCase().includes(q)));
  const sorters = {
    priority: (a, b) => (b.priority - a.priority) || (b.credits - a.credits),
    name: (a, b) => a.name.localeCompare(b.name),
    /* 没有可用资源包时 `expireInDays` 为 null ⇒ 排到最后，别参与减法（NaN 会让排序乱掉）。 */
    expiry: (a, b) => (a.expireInDays == null ? Infinity : a.expireInDays) - (b.expireInDays == null ? Infinity : b.expireInDays),
    recent: (a, b) => b.lastUsedTs - a.lastUsedTs,
  };
  list.sort(sorters[state.sort] || sorters.priority);
  return list;
}

function groupAccounts(list) {
  const p = curPlatform();
  if (state.group === "none") return [{ key: null, items: list }];
  const buckets = new Map();
  const push = (k, a) => { if (!buckets.has(k)) buckets.set(k, []); buckets.get(k).push(a); };
  if (state.group === "program") {
    list.forEach((a) => {
      const act = p.programs.filter((pg) => a.programs[pg.id]).map((pg) => pg.label);
      push(act.length ? act.join(" + ") : "未绑定程序位", a);
    });
  } else if (state.group === "status") {
    list.forEach((a) => push(a.expired ? "凭据已过期" : a.needsRelogin ? "需重新登录" : !a.checkedIn ? "今日未签到" : "正常", a));
  } else {
    list.forEach((a) => push(a.group || "默认", a));
  }
  return [...buckets.entries()].map(([key, items]) => ({ key, items }));
}

function accountStatus(a) {
  if (a.expired) return { tone: "bad", text: "凭据已过期" };
  if (a.needsRelogin) return { tone: "bad", text: "需重新登录" };
  if (!a.checkedIn) return { tone: "warn", text: "今日未签到" };
  return null;
}

/**
 * 账号工作台分两层渲染：
 *   工具栏（静态，含搜索框）+ 列表区（`#acct-list` / `#batch-slot`，动态）。
 * 搜索/排序/分组/勾选只重绘列表区——**工具栏不重绘**。
 * 这不是优化，是必须：重绘输入框会打断中文输入法组词、并丢失光标位置。
 */
function viewAccounts() {
  const s = curScope(), p = curPlatform();

  const toolbar = `<div class="toolbar">
    <label class="field search">${ICON.search}
      <input id="acct-q" type="text" placeholder="搜索账号名、邮箱、备注…（按 / 聚焦）" value="${esc(state.q)}" autocomplete="off" />
    </label>
    <label class="field">排序
      <select data-input="sort">
        ${[["priority", "积分优先级"], ["name", "名称"], ["expiry", "到期时间"], ["recent", "最近使用"]]
          .map(([v, l]) => `<option value="${v}"${state.sort === v ? " selected" : ""}>${l}</option>`).join("")}
      </select>
    </label>
    <label class="field">分组
      <select data-input="group">
        ${[["none", "不分组"], ["program", "按程序位"], ["status", "按状态"], ["group", "按自定义组"]]
          .map(([v, l]) => `<option value="${v}"${state.group === v ? " selected" : ""}>${l}</option>`).join("")}
      </select>
    </label>
    <div class="seg">
      <button class="${state.view === "card" ? "on" : ""}" data-act="view|card" title="卡片视图">${ICON.grid}</button>
      <button class="${state.view === "table" ? "on" : ""}" data-act="view|table" title="表格视图">${ICON.list}</button>
    </div>
    <div class="grow"></div>
    <button class="btn ghost sm" id="select-all-btn" data-act="select-all|0">全选</button>
    <button class="btn primary" data-act="add-account">${ICON.plus}添加账号</button>
  </div>`;

  return `<div class="page">
    <div class="page-head">
      <div>
        <h1>账号</h1>
        <p>${scopeLabel(state.scope)} · ${s.accounts.length} 个账号${expiredCount(s) ? ` · <b style="color:var(--warning)">${expiredCount(s)} 个需处理</b>` : ""}</p>
      </div>
      <div class="grow"></div>
      <span class="scope-chip">${p.mark}${scopeLabel(state.scope)}</span>
    </div>
    ${toolbar}
    <div id="batch-slot">${checkinResultHtml()}${batchBarHtml()}</div>
    <div id="acct-list">${accountListHtml()}</div>
  </div>`;
}

function batchBarHtml() {
  if (!state.selected.size) return "";
  return `<div class="batch-bar">
    <span class="badge info">已选 ${state.selected.size} 个</span>
    <div class="grow"></div>
    <button class="btn sm" data-act="batch-refresh">${ICON.refresh}批量刷新 Token</button>
    <button class="btn sm" data-act="batch-checkin">${ICON.spark}批量签到</button>
    <button class="btn sm" data-act="batch-export">${ICON.download}导出</button>
    <button class="btn sm danger" data-act="batch-delete">${ICON.trash}删除</button>
    <button class="btn ghost sm" data-act="batch-clear">取消</button>
  </div>`;
}

function accountListHtml() {
  const s = curScope();
  const list = filteredAccounts();
  if (!s.accounts.length) {
    return emptyBlock(ICON.accounts, "这个作用域还没有账号", `添加第一个账号，开始管理 ${scopeLabel(state.scope)} 的凭据与积分。`,
      `<button class="btn primary" data-act="add-account">${ICON.plus}添加账号</button>`);
  }
  if (!list.length) {
    return emptyBlock(ICON.search, "没有匹配的账号", `没有找到与「${esc(state.q)}」相关的账号。`,
      `<button class="btn" data-act="clear-q">清除搜索</button>`);
  }
  return groupAccounts(list).map((g) => `
    ${g.key ? `<div class="row" style="margin:18px 0 10px"><span class="badge">${esc(g.key)}</span><span class="tiny muted">${g.items.length} 个</span></div>` : ""}
    ${state.view === "card"
      ? `<div class="grid g3">${g.items.map(accountCard).join("")}</div>`
      : tableView(g.items)}
  `).join("");
}

/** 只重绘列表区 + 批量条 + 全选按钮文案，不动工具栏。 */
function renderAccountList() {
  const list = filteredAccounts();
  const allSel = list.length > 0 && list.every((a) => state.selected.has(a.id));
  const btn = $("#select-all-btn");
  if (btn) { btn.textContent = allSel ? "取消全选" : "全选"; btn.dataset.act = "select-all|" + (allSel ? "0" : "1"); }
  const slot = $("#batch-slot");
  const box = $("#acct-list");
  if (slot) slot.innerHTML = checkinResultHtml() + batchBarHtml();
  if (box) box.innerHTML = accountListHtml();
}

function accountCard(a) {
  const p = curPlatform();
  const sel = state.selected.has(a.id);
  const menuOpen = state.acctMenu === a.id;
  /* 一个账号可绑 0..N 个程序位 ⇒ 「当前登录」= 绑定了任一程序位（不只是第一个）。 */
  const activeN = p.programs.filter((pg) => a.programs[pg.id]).length;
  const isCur = activeN > 0;
  const st = accountStatus(a);
  return `<div class="acct${isCur ? " is-current" : ""}${sel ? " is-selected" : ""}" data-act="acct-open|${a.id}" tabindex="0" role="button" aria-label="打开 ${esc(a.name)} 的详情">
    <div class="acct-top">
      <button class="check${sel ? " on" : ""}" data-act="acct-select|${a.id}" aria-label="选择">${sel ? ICON.check : ""}</button>
      <span class="avatar" style="background:${avatarTone(a.name)}">${initials(a.name)}</span>
      <span class="acct-id"><b>${esc(a.name)}</b><span>${esc(maskEmail(a.email) || a.remark || "—")}</span></span>
      ${isCur ? `<span class="badge info">当前登录</span>` : ""}
    </div>
    <div class="acct-chips">
      ${activeN >= 2 ? `<span class="badge info">${ICON.check}使用中 ${activeN} 个工具</span>` : ""}
      ${st ? `<span class="badge ${st.tone}">${st.text}</span>` : `<span class="badge ok">${ICON.check}已签到</span>`}
      ${a.expiring ? `<span class="badge warn">${fmt(a.expiring)} 积分 ${a.expireInDays} 天后过期</span>` : ""}
      ${a.priority ? `<span class="badge info">优先</span>` : ""}
    </div>
    <div class="acct-metrics">
      <span>积分 <b>${fmt(a.credits)}</b>/${fmt(a.creditsTotal)}</span>
      <span>今日 <b>${fmtTok(a.todayTokens)}</b></span>
      <span class="muted">${esc(a.lastUsed)}</span>
    </div>
    <div class="acct-actions">
      <div class="progs">
        ${p.programs.map((pg) => {
          const on = Boolean(a.programs[pg.id]);
          /* 未检测到的程序位**不禁用**：禁用按钮既不给 tooltip 也不给解释，
             用户只会看到「点不动」。改为可点 + 明确报错，见 §3.3。
             ⚠️ `planned`（本仓尚未支持）与 `installed: false`（本机没装）**必须分开措辞**：
             前者引导用户去装客户端是没用的，只能说明「计划新增」。 */
          const title = pg.planned
            ? pg.label + "：本应用尚未支持，计划新增（不是「你没装」）"
            : pg.installed
              ? (on ? a.name + " 已是 " + pg.label + " 的当前账号" : "把 " + pg.label + " 切到 " + a.name)
              : pg.label + " 未检测到客户端，点击查看如何接入";
          return `<button class="prog${on ? " on" : ""}${pg.installed ? "" : " off"}${pg.planned ? " planned" : ""}"
            title="${title}"
            data-act="acct-prog|${a.id}|${pg.id}">${on ? ICON.check : ""}${esc(pg.short)}</button>`;
        }).join("")}
      </div>
      <button class="btn ghost sm${menuOpen ? " is-open" : ""}" data-act="acct-more|${a.id}"
        title="更多操作：单项刷新 / 详情 / 删除" aria-haspopup="menu" aria-expanded="${menuOpen}">${ICON.more}</button>
    </div>
  </div>`;
}

function tableView(list) {
  const p = curPlatform();
  return `<div class="tbl-wrap"><table class="tbl">
    <thead><tr>
      <th style="width:34px"></th><th>账号</th><th>状态</th><th>积分</th><th>今日 Token</th><th>到期</th><th>程序位</th>
    </tr></thead>
    <tbody>${list.map((a) => {
      const sel = state.selected.has(a.id);
      const st = accountStatus(a);
      const activeN = p.programs.filter((pg) => a.programs[pg.id]).length;
      const isCur = activeN > 0;
      return `<tr class="${isCur ? "is-current" : ""}" data-act="acct-open|${a.id}" tabindex="0" role="row" aria-label="打开 ${esc(a.name)} 的详情">
        <td><button class="check${sel ? " on" : ""}" data-act="acct-select|${a.id}">${sel ? ICON.check : ""}</button></td>
        <td><div class="row"><span class="avatar" style="width:26px;height:26px;border-radius:8px;font-size:11px;background:${avatarTone(a.name)}">${initials(a.name)}</span>
          <span><b>${esc(a.name)}</b>${isCur ? ` <span class="badge info">当前</span>` : ""}${activeN >= 2 ? ` <span class="badge info">使用中 ${activeN} 个工具</span>` : ""}<br><span class="tiny muted">${esc(maskEmail(a.email) || "")}</span></span></div></td>
        <td>${st ? `<span class="badge ${st.tone}">${st.text}</span>` : `<span class="badge ok">正常</span>`}</td>
        <td class="num">${fmt(a.credits)}</td>
        <td class="num">${fmtTok(a.todayTokens)}</td>
        <td class="num">${a.expireInDays == null ? `<span class="muted tiny">—</span>` : `${a.expireInDays} 天`}</td>
        <td>${p.programs.map((pg) => a.programs[pg.id] ? `<span class="badge info" style="margin-right:3px">${esc(pg.label)}</span>` : "").join("") || `<span class="muted tiny">未绑定</span>`}</td>
      </tr>`;
    }).join("")}</tbody>
  </table></div>`;
}

function emptyBlock(icon, title, desc, action) {
  return `<div class="card"><div class="empty">
    <div class="ico">${icon}</div><b>${esc(title)}</b><p>${desc}</p>${action || ""}
  </div></div>`;
}

/* ══════════════════════ 用量 ══════════════════════ */
/* 用量数据源：merge 关闭时只看当前作用域；打开时跨全部作用域真实聚合。
   注意：只有 viewUsage 用它；概览永远看当前作用域。 */
function usageFor() {
  if (!state.merge) return USAGE[state.scope];
  const ids = Object.keys(SCOPES);
  const days = USAGE[ids[0]].days.map((d, i) => ({
    label: d.label,
    value: ids.reduce((sum, id) => sum + (USAGE[id].days[i] ? USAGE[id].days[i].value : 0), 0),
  }));
  const modelMap = new Map();
  ids.forEach((id) => USAGE[id].models.forEach((m) => modelMap.set(m.name, (modelMap.get(m.name) || 0) + m.tokens)));
  const totalTok = [...modelMap.values()].reduce((a, b) => a + b, 0) || 1;
  const models = [...modelMap.entries()]
    .map(([name, tokens]) => ({ name, tokens, pct: Math.round((tokens / totalTok) * 100) }))
    .sort((a, b) => b.tokens - a.tokens);
  const sessions = ids.flatMap((id) => USAGE[id].sessions).sort((a, b) => b.tokens - a.tokens).slice(0, 5);
  /* 按真实时间戳 ts 排序，而不是中文字符串 when（「昨天/今天」按字典序是乱的）。 */
  const credits = ids.flatMap((id) => USAGE[id].credits).sort((a, b) => (b.ts || 0) - (a.ts || 0));
  return { days, models, sessions, credits, ext: mergeExt(ids) };
}

/** 合并态下的派生数据。**必须一起聚合**——否则「合并」只影响 days，
    而堆叠柱/环形/热力图仍显示单作用域，是典型的「切了开关只有一处变」的谎报。 */
function mergeExt(ids) {
  const list = ids.map((id) => USAGE[id].ext);
  const n = list[0].modelDays[0].values.length;
  const sum = (a) => a.reduce((s, v) => s + v, 0);
  const byName = new Map();
  list.forEach((x) => x.modelDays.forEach((m) => {
    if (!byName.has(m.name)) byName.set(m.name, new Array(n).fill(0));
    const arr = byName.get(m.name);
    m.values.forEach((v, i) => { arr[i] += v; });
  }));
  const modelDays = [...byName.entries()]
    .map(([name, values]) => ({ name, values }))
    .sort((a, b) => sum(b.values) - sum(a.values));
  const hourly = Array.from({ length: 7 }, (_, r) =>
    Array.from({ length: 24 }, (_, c) => list.reduce((s, x) => s + x.hourly[r][c], 0)));
  const creditDaily = list[0].creditDaily.map((d, i) => ({
    label: d.label,
    inAmt: list.reduce((s, x) => s + x.creditDaily[i].inAmt, 0),
    outAmt: list.reduce((s, x) => s + x.creditDaily[i].outAmt, 0),
  }));
  const expiry = list[0].expiry.map((d, i) => ({ day: d.day, value: list.reduce((s, x) => s + x.expiry[i].value, 0) }));
  const accountShare = list.flatMap((x) => x.accountShare).sort((a, b) => b.value - a.value).slice(0, 8);
  const sm = new Map();
  list.forEach((x) => x.statusCodes.forEach((s) => sm.set(s.name, (sm.get(s.name) || 0) + s.value)));
  const statusCodes = [...sm.entries()].map(([name, value]) => ({ name, value }));
  const scatter = list.flatMap((x) => x.scatter);
  const curTotal = list.reduce((s, x) => s + x.curTotal, 0);
  /* 余额曲线：以合并后的 curTotal 为终值回推，保证末值 = 合计。 */
  const net = creditDaily.map((c) => c.inAmt - c.outAmt);
  let bal = curTotal - sum(net);
  const balance = net.map((v, i) => { bal += v; return { label: creditDaily[i].label, value: Math.max(0, bal) }; });
  return { modelDays, hourly, creditDaily, balance, expiry, accountShare, statusCodes, scatter, curTotal };
}

const WEEKDAYS = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"];

/* ── 用量视图参数：区间 = 今天 / 近 N 天 / 自定义时间 ──
   数据窗口是「最近 30 天」，标签形如 M/D（见 demo-data.js 的 series）。
   自定义区间**按标签对齐到真实那一天**，不退化成「最后 N 天」——
   否则用户选了 9/20–9/22 却看到 9/28–9/30 的柱子，是能被一眼看穿的谎报。 */
const DAY_LABELS = USAGE[Object.keys(USAGE)[0]].days.map((d) => d.label);
const dayIndexOf = (label) => DAY_LABELS.indexOf(label);
function isoOfLabel(label) {
  const i = dayIndexOf(label);
  if (i < 0) return "";
  const d = new Date();
  d.setHours(0, 0, 0, 0);
  d.setDate(d.getDate() - (DAY_LABELS.length - 1 - i));
  const p = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}
const mdOfIso = (iso) => {
  if (!iso) return "";
  const d = new Date(`${iso}T00:00:00`);
  return `${d.getMonth() + 1}/${d.getDate()}`;
};
/** 当前视图参数 → 取 days 的哪一段（闭区间下标）。 */
function rangeWindow(len) {
  if (state.rangeMode === "today") return { from: len - 1, to: len - 1 };
  if (state.rangeMode === "custom") {
    let i = dayIndexOf(mdOfIso(state.customFrom));
    let j = dayIndexOf(mdOfIso(state.customTo));
    if (i < 0) i = 0;
    if (j < 0) j = len - 1;
    if (j < i) { const t = i; i = j; j = t; }
    return { from: i, to: j };
  }
  return { from: Math.max(0, len - state.range), to: len - 1 };
}
function rangeLabel() {
  if (state.rangeMode === "today") return "今天";
  if (state.rangeMode === "custom") {
    const f = mdOfIso(state.customFrom), t = mdOfIso(state.customTo);
    return f && t ? `${f} – ${t}` : `近 ${state.range} 天`;
  }
  return `近 ${state.range} 天`;
}
/** 自定义时间弹层：原生 date 输入，范围锁在数据窗口（最近 30 天）内。 */
function rangePopover() {
  const first = DAY_LABELS[0], last = DAY_LABELS[DAY_LABELS.length - 1];
  const min = isoOfLabel(first), max = isoOfLabel(last);
  return `<div class="range-pop" role="dialog" aria-label="自定义时间区间">
    <div class="grp">自定义时间区间</div>
    <div class="rp-row">
      <label class="rp-field">起<input type="date" id="range-from" value="${state.customFrom || min}" min="${min}" max="${max}"></label>
      <span class="rp-sep">–</span>
      <label class="rp-field">止<input type="date" id="range-to" value="${state.customTo || max}" min="${min}" max="${max}"></label>
    </div>
    <div class="rp-hint tiny muted">可选范围 ${first} – ${last}（最近 30 天）</div>
    <div class="rp-actions">
      <button class="btn ghost sm" data-act="range-custom-cancel">取消</button>
      <button class="btn sm" data-act="range-custom-apply">应用</button>
    </div>
  </div>`;
}

function viewUsage() {
  const u = usageFor();
  const e = u.ext;
  const win = rangeWindow(u.days.length);
  const days = u.days.slice(win.from, win.to + 1);
  const tab = state.usageTab;
  const tabs = [["trend", "趋势"], ["models", "模型"], ["sessions", "会话"], ["credits", "积分"]];
  const scopeName = state.merge ? "全部作用域" : scopeLabel(state.scope);
  const rangeName = rangeLabel();

  const params = `<div class="toolbar">
    <span class="tiny muted">视图参数</span>
    <div class="seg">
      <button class="${state.rangeMode === "today" ? "on" : ""}" data-act="range|today">今天</button>
      ${[7, 14, 30].map((n) => `<button class="${state.rangeMode === "preset" && state.range === n ? "on" : ""}" data-act="range|${n}">近 ${n} 天</button>`).join("")}
      <button class="${state.rangeMode === "custom" ? "on" : ""}" data-act="range-custom-open" aria-haspopup="dialog" aria-expanded="${state.rangeOpen}">自定义时间</button>
    </div>
    <div class="grow"></div>
    <div class="seg">
      <button class="${!state.merge ? "on" : ""}" data-act="merge|0">仅本作用域</button>
      <button class="${state.merge ? "on" : ""}" data-act="merge|1">合并全部作用域</button>
    </div>
    ${state.rangeOpen ? rangePopover() : ""}
  </div>`;

  let body = "";

  if (tab === "trend") {
    const total = days.reduce((x, d) => x + d.value, 0);
    const avg = Math.round(total / days.length);
    const peak = days.reduce((m, d) => (d.value > m.value ? d : m), days[0]);
    let run = 0;
    const cum = days.map((d) => ({ label: d.label, value: (run += d.value) }));
    const modelSlices = e.modelDays.map((m) => ({ name: m.name, values: m.values.slice(win.from, win.to + 1) }));
    const modelTotals = modelSlices.map((m) => ({ name: m.name, value: m.values.reduce((s, v) => s + v, 0) }));

    body = `
    <div class="grid g4" style="margin-bottom:16px">
      ${statTile("区间总 Token", fmtTok(total), rangeName)}
      ${statTile("日均", fmtTok(avg), "按自然日平均")}
      ${statTile("峰值", fmtTok(peak.value), peak.label)}
      ${statTile("活跃账号", String(e.accountShare.length), scopeName)}
    </div>

    <section class="card" style="margin-bottom:16px">
      <div class="card-head"><h3>每日 Token · 按模型拆分</h3><div class="grow"></div><span class="sub">${scopeName} · 悬停看当日各模型明细</span></div>
      <div class="card-pad">
        ${stackedBar(days, modelSlices, 210, `按模型拆分的每日 Token 堆叠柱状图，${scopeName}，${rangeName}`)}
        <div style="margin-top:14px">${legend(modelTotals)}</div>
      </div>
    </section>

    <div class="grid g2" style="margin-bottom:16px">
      <section class="card">
        <div class="card-head"><h3>累计消耗曲线</h3><div class="grow"></div><span class="sub">区间内累计</span></div>
        <div class="card-pad">${areaChart(cum, 170, `累计 Token 曲线，${scopeName}，${rangeName}`, { caption: `区间累计 ${fmt(cum[cum.length - 1].value)}　（单日峰值 ${fmt(Math.max(...days.map((d, i) => (i ? d.value - days[i - 1].value : d.value))))}）` })}</div>
      </section>
      <section class="card">
        <div class="card-head"><h3>模型占比</h3></div>
        <div class="card-pad">${donut(modelTotals, 150, "各模型 Token 占比环形图", " Token")}</div>
      </section>
    </div>

    <section class="card">
      <div class="card-head"><h3>使用时段分布</h3><div class="grow"></div><span class="sub">越亮 = 该时段越忙（相对强度，非绝对量）</span></div>
      <div class="card-pad">${heatmap(e.hourly, WEEKDAYS, "一周七天 × 24 小时的使用强度热力图")}</div>
    </section>`;
  } else if (tab === "models") {
    const items = e.modelDays
      .map((m) => ({ name: m.name, values: m.values.slice(win.from, win.to + 1) }))
      .map((m) => ({ name: m.name, spark: m.values, value: m.values.reduce((s, v) => s + v, 0) }))
      .sort((a, b) => b.value - a.value);
    const totalM = items.reduce((s, i) => s + i.value, 0) || 1;
    body = items.length
      ? `<div class="grid g4" style="margin-bottom:16px">
          ${statTile("模型数", String(items.length), "区间内被调用过")}
          ${statTile("头部模型", items[0].name, `${Math.round((items[0].value / totalM) * 100)}% 占比`)}
          ${statTile("模型总 Token", fmtTok(totalM), rangeName)}
          ${statTile("单模型均值", fmtTok(Math.round(totalM / items.length)), "按模型数平均")}
        </div>

        <section class="card" style="margin-bottom:16px">
          <div class="card-head"><h3>模型 Token 占比</h3><div class="grow"></div><span class="sub">${scopeName}</span></div>
          <div class="card-pad">${donut(items.map((i) => ({ name: i.name, value: i.value })), 170, "各模型 Token 占比环形图", " Token")}</div>
        </section>

        <section class="card">
          <div class="card-head"><h3>模型用量排行</h3><div class="grow"></div><span class="sub">条形 = 占比　迷你线 = 区间内走势</span></div>
          <div class="card-pad">${hbarList(items, {
            spark: true, sparkW: 74, sparkH: 20,
            fmt: (it) => `${fmtTok(it.value)} · ${Math.round((it.value / totalM) * 100)}%`,
          })}</div>
        </section>`
      : emptyBlock(ICON.usage, "暂无模型用量", "该作用域还没有产生调用记录。");
  } else if (tab === "sessions") {
    const top = [...u.sessions].sort((a, b) => b.tokens - a.tokens);
    const pts = top.map((s) => ({ x: s.msgs, y: s.tokens, label: s.title }));
    const totalS = top.reduce((s, x) => s + x.tokens, 0) || 1;
    body = top.length
      ? `<div class="grid g4" style="margin-bottom:16px">
          ${statTile("会话数", String(top.length), "区间内活跃")}
          ${statTile("会话总 Token", fmtTok(totalS), rangeName)}
          ${statTile("单会话均值", fmtTok(Math.round(totalS / top.length)), "按 Token")}
          ${statTile("最高消耗", fmtTok(top[0].tokens), top[0].title)}
        </div>

        <div class="grid g2" style="margin-bottom:16px">
          <section class="card">
            <div class="card-head"><h3>会话 Token 排行</h3></div>
            <div class="card-pad">${hbarList(top.map((s) => ({ name: s.title, value: s.tokens })), {
              fmt: (it) => `${fmtTok(it.value)} · ${Math.round((it.value / totalS) * 100)}%`,
            })}</div>
          </section>
          <section class="card">
            <div class="card-head"><h3>消息数 × Token</h3><div class="grow"></div><span class="sub">气泡越大 = 单次消耗越高</span></div>
            <div class="card-pad">${scatterChart(pts, 200, "会话的消息数与 Token 散点图", "消息数", "Token")}</div>
          </section>
        </div>

        <div class="tbl-wrap"><table class="tbl">
          <thead><tr><th>会话标题</th><th>账号</th><th>消息数</th><th>Token</th><th>占比</th><th>最后活动</th></tr></thead>
          <tbody>${top.map((x) => `<tr>
            <td><b>${esc(x.title)}</b></td><td>${esc(x.account)}</td>
            <td class="num">${x.msgs}</td><td class="num">${fmt(x.tokens)}</td>
            <td class="num muted">${Math.round((x.tokens / totalS) * 100)}%</td>
            <td class="muted nowrap">${esc(x.when)}</td>
          </tr>`).join("")}</tbody></table></div>`
      : emptyBlock(ICON.accounts, "暂无会话记录", "该作用域还没有产生会话。");
  } else {
    if (state.errors.usage) {
      /* 积分刷新失败：行内错误态 + 重试，而不是只弹一个 toast 就没了 */
      body = errorState({
        title: "积分刷新失败",
        desc: "无法从上游读取积分余额，下面显示的可能不是最新数据。",
        action: { label: "重试刷新", act: "retry|usage" },
      });
    } else {
      /* 合并态下这些数字也要跟着变，不能只有趋势图变。 */
      const statScopes = state.merge ? Object.keys(SCOPES) : [state.scope];
      const allAcc = statScopes.flatMap((id) => SCOPES[id].accounts);
      const daily = e.creditDaily.slice(win.from, win.to + 1);
      const bal = e.balance.slice(win.from, win.to + 1);
      const totalIn = daily.reduce((s, d) => s + d.inAmt, 0);
      const totalOut = daily.reduce((s, d) => s + d.outAmt, 0);
      const netAmt = totalIn - totalOut;
      const exp = e.expiry.filter((x) => x.value > 0);
      const expiring5 = allAcc.reduce((x, a) => x + a.expiring, 0);

      body = `${creditsHeadBar()}
      <div class="grid g4" style="margin-bottom:16px">
        ${statTile("可用积分合计", fmt(e.curTotal), `${allAcc.length} 个账号`)}
        ${statTile("区间入账", "+" + fmt(totalIn), "签到奖励")}
        ${statTile("区间消耗", "-" + fmt(totalOut), `净 ${netAmt >= 0 ? "+" : ""}${fmt(netAmt)}`)}
        ${statTile("即将过期", fmt(expiring5), exp.length ? `最早 ${exp[0].day} 天后` : "5 天内无到期")}
      </div>

      <section class="card" style="margin-bottom:16px">
        <div class="card-head"><h3>每日积分收支</h3><div class="grow"></div><span class="sub">上 = 签到入账　下 = 对话消耗</span></div>
        <div class="card-pad">${divergingBars(daily, 180, `每日积分收支双向柱状图，${scopeName}，${rangeName}`, "入账", "消耗")}</div>
      </section>

      <div class="grid g2" style="margin-bottom:16px">
        <section class="card">
          <div class="card-head"><h3>积分余额变化</h3><div class="grow"></div><span class="sub">末值 = 当前合计</span></div>
          <div class="card-pad">${areaChart(bal, 170, `积分余额变化曲线，${scopeName}，${rangeName}`, { anchorZero: false })}</div>
        </section>
        <section class="card">
          <div class="card-head"><h3>积分到期分布</h3><div class="grow"></div><span class="sub">未来 30 天</span></div>
          <div class="card-pad">${exp.length
            ? barChart(e.expiry.map((x) => ({ label: "D+" + x.day, value: x.value })), 170, 5)
            : `<p class="tiny muted" style="margin:8px 0">未来 30 天内没有积分到期。</p>`}</div>
        </section>
      </div>

      <div class="grid g2" style="margin-bottom:16px">
        <section class="card">
          <div class="card-head"><h3>账号 Token 占比</h3><div class="grow"></div><span class="sub">累计</span></div>
          <div class="card-pad">${e.accountShare.length
            ? donut(e.accountShare.slice(0, 6), 150, "各账号累计 Token 占比环形图", " Token")
            : `<p class="tiny muted" style="margin:8px 0">还没有账号产生用量。</p>`}</div>
        </section>
        <section class="card">
          <div class="card-head"><h3>网关状态码分布</h3><div class="grow"></div><span class="sub">今日请求</span></div>
          <div class="card-pad">${e.statusCodes.length
            ? donut(e.statusCodes, 150, "网关请求状态码分布环形图", " 次")
            : `<p class="tiny muted" style="margin:8px 0">本作用域没有网关请求。</p>`}</div>
        </section>
      </div>

      ${u.credits.length ? `<div class="tbl-wrap"><table class="tbl">
      <thead><tr><th>时间</th><th>账号</th><th>变动</th><th>说明</th></tr></thead>
      <tbody>${u.credits.map((c) => `<tr>
        <td class="muted nowrap">${esc(c.when)}</td><td>${esc(c.account)}</td>
        <td class="num" style="color:${c.delta > 0 ? "var(--success)" : "var(--warning)"}">${c.delta > 0 ? "+" : ""}${c.delta}</td>
        <td>${esc(c.note)}</td></tr>`).join("")}</tbody></table></div>`
        : emptyBlock(ICON.spark, "暂无积分流水", "该作用域还没有积分变动记录。")}`;
    }
  }

  return `<div class="page">
    <div class="page-head">
      <div><h1>用量</h1><p>${scopeLabel(state.scope)} · Token 与积分统计合并到一个工作台</p></div>
      <div class="grow"></div>
      <button class="btn" data-act="refresh">${ICON.refresh}刷新</button>
    </div>
    <div class="seg" style="margin-bottom:16px">${tabs.map(([v, l]) => `<button class="${tab === v ? "on" : ""}" data-act="usage-tab|${v}">${l}</button>`).join("")}</div>
    ${params}
    ${body}
  </div>`;
}

/* 模型清单来源条（§3.17）：三级来源必须显式标注，非「实时」时给出原因 + 重试。
   绝不把「内置」伪装成「实时」—— 用户照着清单调用会扑空，是典型的静默错误。 */
const CATALOG_META = {
  live: { label: "实时", cls: "ok", hint: "本次从上游成功识别" },
  cached: { label: "已保存", cls: "warn", hint: "本次未拉到，使用上次成功的结果" },
  builtin: { label: "内置", cls: "bad", hint: "从未成功识别过，使用内置兜底清单" },
};

function catalogBar(cat) {
  const c = cat || { source: "builtin", fetchedAt: null, note: null };
  const m = CATALOG_META[c.source] || CATALOG_META.builtin;
  const scanning = state.gwScanning;
  return `<div class="cat-bar">
    <span class="badge ${m.cls}">${m.label}</span>
    <span class="tiny muted">${c.fetchedAt ? `上次识别：${esc(c.fetchedAt)}` : "尚未识别过"}</span>
    ${c.note ? `<span class="tiny cat-note">${esc(c.note)}</span>` : `<span class="tiny muted">${m.hint}</span>`}
    <div class="grow"></div>
    <span class="tiny muted">接入后自动识别，无需手填模型名</span>
    <button class="btn sm" data-act="gw-scan"${scanning ? " disabled" : ""}>${scanning ? "识别中…" : "重新识别"}</button>
  </div>`;
}

/* 接入说明（§3.18.3）：**协议 Tab × 工具 Tab**。
   只写能确认的客户端；未确认的一律给「通用参数 + 以其官方文档为准」——
   **不编造配置片段**（编错的配置比没有说明更糟，用户会以为网关坏了）。 */
const ACCESS_TOOLS = {
  openai: [["codex", "Codex"], ["opencode", "opencode"], ["generic", "其它 OpenAI 兼容工具"]],
  anthropic: [["claude-code", "Claude Code"], ["opencode", "opencode"], ["generic", "其它 Anthropic 兼容工具"]],
};

function accessSnippet(api, tool, g) {
  const base = g.baseUrl;
  if (tool === "claude-code") {
    return `# Claude Code（Anthropic 协议）
export ANTHROPIC_BASE_URL=${base}
export ANTHROPIC_AUTH_TOKEN=<你的 Key>
# 模型名不用填：会从 ${base}/v1/models 自动发现`;
  }
  if (tool === "codex") {
    return `# Codex（OpenAI 协议）—— 自定义 model_provider
base_url = "${base}/v1"
env_key  = "BUDDY_SWITCH_KEY"    # 环境变量名；值填你的 Key
# 其余字段以 Codex 官方文档为准`;
  }
  if (tool === "opencode") {
    return `# opencode —— provider 配置
baseURL = "${base}"
apiKey  = "<你的 Key>"
# 字段名以 opencode 官方文档为准`;
  }
  return api === "anthropic"
    ? `# 其它 Anthropic 兼容工具
Base URL : ${base}
API Key  : <你的 Key>        （认证头：x-api-key）
模型名    : 从 GET ${base}/v1/models 自动发现`
    : `# 其它 OpenAI 兼容工具
Base URL : ${base}
API Key  : <你的 Key>        （认证头：Authorization: Bearer）
模型名    : 从 GET ${base}/v1/models 自动发现`;
}

function quickAccessHtml(g) {
  const api = state.gwApi || "openai";
  const tools = ACCESS_TOOLS[api];
  const tool = tools.some(([id]) => id === state.gwTool) ? state.gwTool : tools[0][0];
  const snippet = accessSnippet(api, tool, g);
  /* 示例里用的模型从**已暴露**的模型里取 —— 别拿一个被禁用的模型当例子。 */
  const model = (g.models.find((m) => m.available && !m.disabled) || {}).name || "claude-sonnet-4.5";
  const curl = api === "anthropic"
    ? `curl ${g.baseUrl}/v1/messages \\
  -H "x-api-key: $BUDDY_SWITCH_KEY" \\
  -H "anthropic-version: 2023-06-01" \\
  -d '{"model":"${model}","max_tokens":256,"messages":[{"role":"user","content":"hi"}]}'`
    : `curl ${g.baseUrl}/v1/chat/completions \\
  -H "Authorization: Bearer $BUDDY_SWITCH_KEY" \\
  -d '{"model":"${model}","messages":[{"role":"user","content":"hi"}]}'`;
  return `<section class="card">
    <div class="card-head"><h3>接入说明</h3><div class="grow"></div>
      <div class="seg">
        <button class="${api === "openai" ? "on" : ""}" data-act="gw-api|openai">OpenAI 兼容</button>
        <button class="${api === "anthropic" ? "on" : ""}" data-act="gw-api|anthropic">Anthropic 兼容</button>
      </div>
    </div>
    <div class="card-pad">
      <div class="seg tool-seg">${tools.map(([id, label]) => `<button class="${tool === id ? "on" : ""}" data-act="gw-tool|${id}">${label}</button>`).join("")}</div>
      <pre class="mono" style="margin:12px 0 0;padding:12px;background:var(--card-2);border-radius:10px;overflow-x:auto">${esc(snippet)}</pre>
      <p class="tiny muted" style="margin:10px 2px 0">
        协议端点：<span class="mono">POST /v1/chat/completions</span>（OpenAI） ·
        <span class="mono">POST /v1/messages</span>（Anthropic）—— 两者<b>都已支持</b>，含流式。
      </p>
      <p class="tiny muted" style="margin:6px 2px 0">
        客户端会自动从 <span class="mono">${g.baseUrl}/v1/models</span> 读取模型清单，<b>不需要手填模型名</b>；
        若你的工具不支持自动发现，到「模型」页复制即可。
      </p>
      <p class="tiny muted" style="margin:12px 2px 6px">原始请求（调试用）：</p>
      <pre class="mono" style="margin:0;padding:12px;background:var(--card-2);border-radius:10px;overflow-x:auto">${esc(curl)}</pre>
    </div>
  </section>`;
}

/* ══════════════════════ 网关 ══════════════════════ */
/* ══════════════════════ 请求代理（§3.19 / P12）══════════════════════
   代理客户端自身的业务请求，由网关按请求选号 ⇒ **无缝切号**（不写文件、不重启客户端）。
   ⚠️ 两条已拍板的硬约束（D29）：**默认关** + **绝不劫持**。 */
function proxyPanelHtml() {
  if (!state.proxyOn) {
    return `<section class="card">
      <div class="card-head"><h3>请求代理</h3><div class="grow"></div><span class="badge">已关闭</span></div>
      <div class="card-pad">
        <p class="tiny muted" style="margin:0 0 4px">
          让客户端把请求交给本机网关，由网关<b>按请求选号</b> ⇒ 切换账号<b>无需重启客户端</b>。
          本仓现在的切换要改登录态文件 + 重开客户端，这正是它变「重」的根因。
        </p>
        <div class="srow">
          <div class="lab"><b>启用请求代理</b><span>默认关闭；开启后需在客户端配置代理或 base URL</span></div>
          <button class="switch" data-act="proxy-toggle" role="switch" aria-checked="false" aria-label="启用请求代理"></button>
        </div>
        <div class="note-warn">
          <b>开启后，被代理域名的流量会经过本机网关</b>（含账号身份）。
          网关<b>不落盘请求正文</b>，日志只记元数据；且只代理白名单域名。
        </div>
        <div class="note-ok">
          <b>绝不劫持</b>：不改 hosts、不做 DNS 劫持、不做透明代理 ——
          只接受客户端<b>主动</b>把请求交给网关。关掉即恢复，无残留。
        </div>
      </div>
    </section>`;
  }

  const rows = PROXY.domains.map((d) => `<tr>
    <td class="mono"><b>${esc(d.host)}</b></td><td>${esc(d.note)}</td><td><span class="badge ok">代理</span></td>
  </tr>`).join("");
  return `<section class="card">
    <div class="card-head"><h3>请求代理</h3><div class="grow"></div><span class="badge ok">运行中</span></div>
    <div class="card-pad">
      <div class="srow">
        <div class="lab"><b>启用请求代理</b><span>拦截方式：${esc(PROXY.modeLabel)} —— ${esc(PROXY.modeNote)}</span></div>
        <button class="switch on" data-act="proxy-toggle" role="switch" aria-checked="true" aria-label="关闭请求代理"></button>
      </div>
      <div class="kpi-row">
        <div><b>${PROXY.todayRequests}</b><span>今日代理请求</span></div>
        <div><b>${PROXY.switches}</b><span>今日换号</span></div>
        <div><b>0</b><span>落盘正文</span></div>
      </div>
      <h4 class="sub-h">代理范围（白名单）</h4>
      <div class="tbl-wrap"><table class="tbl">
        <thead><tr><th>域名</th><th>归属</th><th>处理</th></tr></thead>
        <tbody>${rows}
        <tr><td class="muted">其它全部域名</td><td class="muted">—</td><td><span class="badge">直连</span></td></tr>
        </tbody></table></div>
      <div class="note-ok"><b>绝不劫持</b>：不改 hosts、不做 DNS 劫持、不做透明代理。</div>
      <div class="row-end"><button class="btn ghost sm" data-act="proxy-restore">一键还原</button></div>
    </div>
  </section>`;
}

/* 当前作用域的网关配置。禁用/启用模型直接改这份数据（原型重启即恢复）。 */
function curGateway() { return GATEWAY[state.scope]; }

/* ─────────── Trae Max Mode（1M 上下文）───────────
   上游没有「1M 模型」：Max Mode 是请求体里的布尔字段 `is_max_mode`。
   网关的做法是**策略层**：对支持表内的模型始终注入该字段，`/v1/models` 里这些模型
   直接声明 1M —— 于是「glm-5.3」就是 1M，不需要 `-max` 这种影子模型名。
   支持表与 Rust 侧 `payload::MAX_MODE_MODELS` 逐条对齐。 */
const MAX_MODE_MODELS = [
  "doubao-seed-evolving", "glm-5.3", "glm-5.2",
  "deepseek-v4-pro", "deepseek-v4-pro-official",
  "deepseek-v4-flash", "deepseek-v4-flash-official",
  "kimi-k3", "minimax-m3", "qwen3.8-max", "qwen-3.7-plus",
];
const supportsMaxMode = (name) => MAX_MODE_MODELS.includes(String(name ?? "").trim().toLowerCase());
const maxModeOn = () => state.maxMode !== "off";

/* 一行模型的上下文口径：支持且已开启 ⇒ 1M；否则上游默认（演示数据统一按 200k 展示）。 */
function contextCell(model) {
  if (supportsMaxMode(model.name) && maxModeOn()) {
    return `<span class="badge ok">1M</span>`;
  }
  return `<span class="tiny muted">上游默认</span>`;
}

/* Max Mode 策略条（只在 Trae 平台出现）：两个状态，切换即时反映到下方清单。 */
function maxModePolicyCard() {
  const on = maxModeOn();
  return `<section class="card">
    <div style="display:flex;align-items:center;gap:18px;padding:14px 16px">
      <div style="flex:1;min-width:0">
        <b style="display:block;font-size:12.5px;font-weight:550">上下文上限（Max Mode）</b>
        <div class="tiny muted" style="margin-top:3px;line-height:1.55">
          对支持 Max Mode 的模型<b>始终</b>注入 <span class="mono">is_max_mode</span>，清单直接声明 1M；
          关闭后回到上游默认窗口。Max Mode 是<b>请求级开关</b>，不是模型 —— 一个模型只有一条记录，不会有
          <span class="mono">glm-5.3-max</span> 这类影子 id。
        </div>
      </div>
      <div class="seg" role="group" aria-label="Trae 上下文上限" style="flex-shrink:0">
        <button class="${on ? "on" : ""}" data-act="max-mode|always">始终 1M</button>
        <button class="${on ? "" : "on"}" data-act="max-mode|off">关闭</button>
      </div>
    </div>
  </section>`;
}

function viewGateway() {
  const g = GATEWAY[state.scope], p = curPlatform();
  const subs = [["overview", "概览"], ["keys", "密钥"], ["models", "模型"], ["pool", "账号池"], ["proxy", "请求代理"], ["logs", "请求日志"]];

  let main = "";
  if (state.gwSub === "overview") {
    /* 上游凭据失效 → 行内错误态 + 重试（用「校验」按钮可复现）。 */
    const errBlock = state.errors.gateway ? errorState({
      title: "上游账号凭据已失效",
      desc: "账号池中的上游凭据无法解密，网关将无法转发请求。",
      action: { label: "重试校验", act: "retry|gateway" },
    }) : "";
    main = `${errBlock}<section class="card">
      <div class="card-head"><h3>服务状态</h3><div class="grow"></div>
        <span class="badge ${g.enabled ? "ok" : ""}">${g.enabled ? "运行中" : "已停用"}</span></div>
      <div class="srow"><div class="lab"><b>启用本地网关</b><span>在 127.0.0.1 上提供 OpenAI / Anthropic 兼容接口</span></div>
        <button class="switch${g.enabled ? " on" : ""}" data-act="toggle|gw-enabled"></button></div>
      <div class="srow"><div class="lab"><b>接入地址</b><span class="mono">${g.baseUrl}</span></div>
        <button class="btn sm" data-act="copy|${g.baseUrl}">${ICON.copy}复制</button></div>
      <div class="srow"><div class="lab"><b>上游账号凭据</b><span>账号池读取上游凭据的有效性</span></div>
        <button class="btn sm" data-act="gw-verify">${ICON.refresh}校验</button></div>
      <div class="srow"><div class="lab"><b>可用模型</b><span>${g.models.filter((m) => m.available).length} 个</span></div>
        <button class="btn sm" data-act="gw-sub|models">查看</button></div>
      <div class="srow"><div class="lab"><b>账号池</b><span>${g.pool.length} 个条目</span></div>
        <button class="btn sm" data-act="gw-sub|pool">查看</button></div>
    </section>
    ${quickAccessHtml(g)}`;
  } else if (state.gwSub === "keys") {
    /* ★ 密钥三态动作：吊销（置标记）· 恢复（清标记 ⇒ **同一把 Key 重新可用**）· 删除（不可逆）。
       已吊销的行必须**同时**给出「恢复」与「删除」—— 只留删除，用户撤销一次误吊销就要
       删掉重建、再把所有客户端配置改一遍。真实组件 `gateway/api-key-table.tsx` 同形。 */
    main = g.keys.length ? `<div class="tbl-wrap"><table class="tbl">
      <thead><tr><th>名称</th><th>密钥</th><th>归属</th><th>累计请求</th><th>最后使用</th><th>状态</th><th style="width:132px"></th></tr></thead>
      <tbody>${g.keys.map((k) => `<tr${k.revoked ? ' class="row-off"' : ""}>
        <td><b>${esc(k.name)}</b><br><span class="tiny muted">创建于 ${k.created}</span></td>
        <td class="mono">${esc(k.masked)}</td>
        <td><span class="badge">${esc(k.scope)}</span></td>
        <td class="num">${fmt(k.requests)}</td>
        <td class="muted nowrap">${esc(k.lastUsed)}</td>
        <td>${k.revoked ? `<span class="badge">已吊销</span>` : `<span class="badge ok">启用</span>`}</td>
        <td>${k.revoked
            ? `<button class="btn ghost xs" data-act="key-restore|${k.id}">恢复</button>
               <button class="btn ghost xs danger" data-act="key-delete|${k.id}">删除</button>`
            : `<button class="btn ghost xs" data-act="copy|${esc(k.masked)}">${ICON.copy}</button>
               <button class="btn ghost xs" data-act="revoke|${k.id}">${ICON.trash}</button>`}</td>
      </tr>`).join("")}</tbody></table></div>
      <p class="tiny muted" style="margin:10px 2px 0">密钥明文只在创建时展示一次；此处永远只显示掩码。请求数为该密钥创建以来的<b>累计值</b>，与概览的「今日」口径不同。<b>吊销可逆</b>：已吊销的密钥仍留在表里，可「恢复」回启用态（库中存的是同一把 Key，明文未变，客户端无需改配置），也可「删除」清掉记录。</p>`
      : emptyBlock(ICON.key, "还没有密钥", "创建一个密钥后即可通过本地网关调用模型。",
          `<button class="btn primary" data-act="new-key">${ICON.plus}创建密钥</button>`);
  } else if (state.gwSub === "models") {
    const served = g.models.filter((m) => m.available && !m.disabled).length;
    const trae = curScope().platform === "trae";
    main = `${trae ? maxModePolicyCard() : ""}${catalogBar(g.catalog)}
    <div class="tbl-wrap"><table class="tbl">
      <thead><tr><th>模型</th><th>协议</th><th>归属</th>${trae ? "<th>上下文</th>" : ""}<th>上游状态</th><th style="width:124px">网关暴露</th></tr></thead>
      <tbody>${g.models.map((m) => `<tr${m.disabled ? ' class="row-off"' : ""}>
        <td class="mono"><b>${esc(m.name)}</b></td><td>${esc(m.provider)}</td><td>${esc(m.scope)}</td>
        ${trae ? `<td>${contextCell(m)}</td>` : ""}
        <td>${m.available ? `<span class="badge ok">可用</span>` : `<span class="badge warn">不可用</span>`}</td>
        <td>${m.disabled
          ? `<span class="badge bad">已禁用</span> <button class="btn xs" data-act="model-enable|${esc(m.name)}|${esc(m.scope)}">启用</button>`
          : `<span class="badge ok">已暴露</span> <button class="btn xs" data-act="model-disable|${esc(m.name)}|${esc(m.scope)}">禁用</button>`}</td>
      </tr>`).join("")}</tbody></table></div>
      <p class="tiny muted" style="margin:10px 2px 0">
        禁用的模型<b>不会出现在 <span class="mono">/v1/models</span></b>，直接调用也会被拒绝并说明原因
        —— 两个出口同时生效，否则禁用就是假开关。当前暴露 <b>${served}</b> / ${g.models.length} 个。
      </p>`;
  } else if (state.gwSub === "pool") {
    /* 「显式指定」是选号优先级的第一级；指定是排他的，被指定的行置顶。 */
    const pinned = state.pins[state.scope];
    const rows = g.pool.slice().sort((x, y) => (x.account === pinned ? -1 : y.account === pinned ? 1 : 0));
    main = `<div class="tbl-wrap"><table class="tbl">
      <thead><tr><th>账号</th><th>积分</th><th>权重</th><th>状态</th><th>粘性</th><th style="width:96px">选号</th></tr></thead>
      <tbody>${rows.map((x) => `<tr>
        <td><b>${esc(x.account)}</b>${x.account === pinned ? ` <span class="badge info">已指定</span>` : ""}</td>
        <td class="num">${fmt(x.credits)}</td><td class="num">${x.weight}</td>
        <td>${x.state === "可用" ? `<span class="badge ok">${x.state}</span>` : x.state === "已排除" ? `<span class="badge bad">${x.state}</span>` : `<span class="badge warn">${x.state}</span>`}</td>
        <td>${x.sticky ? `<span class="badge info">粘性</span>` : `<span class="muted tiny">—</span>`}</td>
        <td>${x.state === "已排除"
          ? `<button class="btn xs" disabled title="已排除的账号不能作为上游">指定</button>`
          : x.account === pinned
            ? `<button class="btn xs" data-act="pin|">取消指定</button>`
            : `<button class="btn xs" data-act="pin|${esc(x.account)}">指定</button>`}</td>
      </tr>`).join("")}</tbody></table></div>
      <p class="tiny muted" style="margin:10px 2px 0">选号优先级：<b>显式指定</b> &gt; 粘性会话 &gt; 自动择优。指定是排他的——同一时刻只有一个账号被指定。</p>`;
  } else if (state.gwSub === "proxy") {
    main = proxyPanelHtml();
  } else {
    main = `<div class="tbl-wrap"><table class="tbl">
      <thead><tr><th>时间</th><th>模型</th><th>上游账号</th><th>状态</th><th>耗时</th><th>Token</th></tr></thead>
      <tbody>${g.logs.map((l) => `<tr>
        <td class="mono muted nowrap">${l.time}</td><td class="mono">${esc(l.model)}</td><td>${esc(l.account)}</td>
        <td>${l.status === 200 ? `<span class="badge ok">200</span>` : l.status === 429 ? `<span class="badge warn">429</span>` : `<span class="badge bad">${l.status}</span>`}</td>
        <td class="num">${l.ms} ms</td><td class="num">${l.tokens ? fmt(l.tokens) : "—"}</td>
      </tr>`).join("")}</tbody></table></div>`;
  }

  return `<div class="page">
    <div class="page-head">
      <div><h1>网关</h1><p>${scopeLabel(state.scope)} · 把账号池暴露为 OpenAI / Anthropic 兼容接口</p></div>
      <div class="grow"></div>
      <span class="scope-chip">${p.mark}${scopeLabel(state.scope)}</span>
    </div>
    <div class="split">
      <nav class="subnav">${subs.map(([v, l]) => `<button class="${state.gwSub === v ? "on" : ""}" data-act="gw-sub|${v}">${l}</button>`).join("")}</nav>
      <div class="split-main">${main}</div>
    </div>
  </div>`;
}

/* ══════════════════════ 会话工作台（PLAN §3.15）══════════════════════ */

/* 待同步会话组数（落后 + 分叉）—— 侧栏徽章用。 */
function sessionPendingCount() {
  return SESSION_GROUPS.filter((g) => sessionGroupStatus(g) !== "latest").length;
}

const SESSION_STATUS_META = {
  behind: { label: "待同步", cls: "warn" },
  diverge: { label: "有分歧", cls: "bad" },
  latest: { label: "已一致", cls: "ok" },
};

/* 状态筛选选项**按需出现**（§3.4 规则 1）：没有任何组处于该状态的选项不渲染。 */
function sessionFilterOptions(clientGroups) {
  const present = new Set(clientGroups.map((g) => sessionGroupStatus(g)));
  return [["all", "全部"], ["behind", "待同步"], ["diverge", "有分歧"], ["latest", "已一致"]]
    .filter(([v]) => v === "all" || present.has(v));
}

function viewSessions() {
  /* ★ 能力检查（§3.15 能力边界）：会话能力是**平台固有**的，当前只有 WorkBuddy 具备。
     Trae 经 URL 直达时给**结构化空态**并说明原因，
     **不能列出 WorkBuddy 的会话** —— 那会让用户以为"会话已经复制过来了"。 */
  if (!curPlatform().capabilities.includes("sessionCopy")) {
    const p = curPlatform();
    return `<div class="page">
      <div class="page-head">
        <div><h1>关联会话</h1><p>跨账号会话副本的关联与同步</p></div>
        <div class="grow"></div>
        <span class="scope-chip">${p.mark}${scopeLabel(state.scope)}</span>
      </div>
      ${emptyBlock(ICON.sessions, `${p.label} 暂不支持会话管理`,
        "会话复制依赖该平台自身的会话存储与云端归属机制，当前只有 WorkBuddy 具备该能力。",
        `<button class="btn sm" data-act="nav|accounts">去账号页</button>`)}
    </div>`;
  }

  const client = state.sessionClient;
  const groups = SESSION_GROUPS.filter((g) => g.client === client);
  const opts = sessionFilterOptions(groups);
  const filtered = groups.filter((g) => state.sessionFilter === "all" || sessionGroupStatus(g) === state.sessionFilter);
  const sorted = state.sessionSort === "oldest" ? [...filtered].reverse() : filtered;
  const active = SESSION_CLIENTS.find((c) => c.id === client);

  /* ★ 客户端 Tab 只在**多于 1 项**时渲染（§3.15.1）：
     只有单个选项的「切换器」是假控件 —— 本仓当前正是这种情况
     （会话复制只有 WorkBuddy 桌面）。退化成静态标题，功能一点不少。 */
  const tabs = SESSION_CLIENTS.length > 1
    ? `<div class="cli-tabs">${SESSION_CLIENTS.map((c) => {
        const n = SESSION_GROUPS.filter((g) => g.client === c.id).length;
        return `<button class="cli-tab${client === c.id ? " on" : ""}" data-act="sess-client|${c.id}">
          <span class="cli-mark">${c.mark}</span>
          <span class="cli-text"><b>${esc(c.label)}</b><i>${n} 组</i></span>
        </button>`;
      }).join("")}</div>`
    : "";

  const cards = sorted.map((g) => {
    const st = SESSION_STATUS_META[sessionGroupStatus(g)];
    return `<div class="sg-card" data-act="sess-open|${g.key}" tabindex="0" role="button" aria-label="打开会话组 ${esc(g.title)}">
      <div class="sg-head">
        <span class="badge ${st.cls}">${st.label}</span>
        <span class="tiny muted">${g.members.length} 个副本</span>
      </div>
      <h4>${esc(g.title)}</h4>
      <div class="tiny muted">${esc(g.updated)}</div>
    </div>`;
  }).join("");

  /* 空态二分（§3.4 规则 3）：真的没有 vs 被筛没了 —— 文案与出口都不同。 */
  const empty = groups.length === 0
    ? emptyBlock(ICON.sessions, "这个客户端还没有关联会话",
        "在账号页切换账号时勾选「复制会话」，复制过的会话会自动在这里建立关联。",
        `<button class="btn sm" data-act="nav|accounts">去账号页</button>`)
    : emptyBlock(ICON.search, "没有符合条件的会话",
        "当前筛选条件下没有会话组。",
        `<button class="btn sm" data-act="sess-filter|all">清除筛选</button>`);

  return `<div class="page">
    <div class="page-head">
      <div><h1>关联会话</h1><p>同一份会话在各账号里的副本 —— 谁落后、谁分叉、同步给谁</p></div>
      <div class="grow"></div>
      <span class="scope-chip">${curPlatform().mark}${scopeLabel(state.scope)}</span>
    </div>

    ${tabs}

    <div class="toolbar">
      <h2 class="tb-title">${esc(active ? active.label : "")} 会话 <span class="badge">${groups.length}</span></h2>
      <div class="grow"></div>
      <div class="seg sess-filter-seg">${opts.map(([v, l]) => `<button class="${state.sessionFilter === v ? "on" : ""}" data-act="sess-filter|${v}">${l}</button>`).join("")}</div>
      <div class="seg sess-sort-seg">
        <button class="${state.sessionSort === "recent" ? "on" : ""}" data-act="sess-sort|recent">最近更新</button>
        <button class="${state.sessionSort === "oldest" ? "on" : ""}" data-act="sess-sort|oldest">最早更新</button>
      </div>
    </div>

    ${sorted.length ? `<div class="sg-grid">${cards}</div>` : empty}
  </div>`;
}

/* 会话关联图（§3.15.2）：共同旧版 → 各账号副本分支。
   每个副本显示**相对共同旧版的差异**，而不是只给时间戳（时间新 ≠ 内容新）。 */
function sessionGraphHtml(g) {
  const st = sessionGroupStatus(g);
  const branch = (m) => {
    const cls = m.state === "diverge" ? "bad" : m.state === "behind" ? "warn" : "ok";
    return `<div class="sgm">
      <div class="sgm-acct">${esc(m.account)}</div>
      <div class="sgm-delta"><span class="badge ${cls}">${esc(m.delta)}</span></div>
    </div>`;
  };
  return `
    <div class="sg-graph">
      <div class="sg-base"><span class="tiny muted">共同旧版</span><b>${esc(g.base)}</b></div>
      <div class="sg-branches">${g.members.map(branch).join("")}</div>
    </div>
    ${st === "diverge" ? `<div class="bk-warn" role="alert">检测到内容分歧：各副本都有各自的更新。统一前必须先选择保留哪一份。</div>` : ""}
    <div class="tiny muted" style="margin:8px 0">同步是<b>加法</b>：只把来源的新增内容追加到目标，不会删除目标已有的内容。</div>
    <div class="bk-actions">
      ${st === "diverge"
        ? `<button class="btn sm" data-act="sess-unify|${g.key}">选择保留哪一份…</button>`
        : `<button class="btn primary sm" data-act="sess-sync|${g.key}">同步到全部副本</button>`}
      <button class="btn ghost sm" data-act="sess-unlink|${g.key}">解除关联</button>
    </div>`;
}

/* ★★ P14 S4a · 插件侧「复制会话」抽屉（VS Code 插件 / CodeBuddy IDE）。
 *
 * ## 为什么**不**复用桌面版的迁移向导
 *
 * 两者模型根本不同：桌面版是跨账号**副本组**（有账本、可双向同步、有「谁落后 / 谁分叉」）；
 * 插件侧是**一次性单向复制**，副本**恒取新 id** ⇒ 构不成可同步的副本组。
 * 所以这里**没有**「同步 / 统一」两个动作 —— 它们只在副本组上才有意义。
 *
 * ## 三条诚实规则（每条都有断言）
 *
 * 1. **未检测到客户端**（`dataRoot === null`）与「这个账号没有会话」**文案与出口都不同**
 *    —— 压成一句「没有会话」就是把「你没装 / 没登录」说成「你确实没有会话」；
 * 2. `skipped > 0` 要**如实标注**有几个工作区索引读不出来，不假装「全都列在这儿了」；
 * 3. 目标账号**不含源账号自己**（自我复制无意义，后端也会拒）。
 *
 * ⚠️ 复制是**加法**：恒取新 id ⇒ 目标已有内容**一个字都不会被动**。
 * 这一点必须写在界面上 —— 用户最怕的正是「复制会不会覆盖我目标账号的会话」。
 */
function extSessionCopyHtml(accountId) {
  const s = curScope();
  const view = EXT_SESSIONS[state.extVariant];
  const source = s.accounts.find((a) => a.id === accountId);

  /* 规则 1：未检测到客户端 ⇒ **独立空态**（不是「没有会话」）。 */
  if (!view.dataRoot) {
    return `<div class="dsec" data-ext-state="missing">
      ${emptyBlock(ICON.sessions, "未检测到 VS Code 插件的数据目录",
        "本机没找到 CodeBuddyExtension 的数据目录。这与「这个账号没有会话」是**两件事** —— 请先在 VS Code 里登录一次插件。")}
    </div>`;
  }
  /* 规则 1（另一半）：装了客户端但账号没会话 ⇒ 另一段文案，并给出**数据目录**作为可核对的事实。 */
  if (!view.sessions.length) {
    return `<div class="dsec" data-ext-state="empty">
      ${emptyBlock(ICON.sessions, "这个账号在插件里还没有会话",
        `数据目录已找到：${esc(view.dataRoot)}`)}
    </div>`;
  }

  /* 目标候选 = **本区域里除源账号以外的全部账号**（不写死列表：
     写死会让「多了一个账号但选不到」这种缺陷看不见）。跨区域**不**给候选 ——
     插件侧没有跨版本数据通道。 */
  const targets = s.accounts.filter((x) => x.id !== accountId);
  const target = targets.find((a) => a.id === state.extTarget) || targets[0] || null;
  const picked = view.sessions.filter((x) => state.extPicked.has(x.id));

  const rows = view.sessions.map((x) => {
    const on = state.extPicked.has(x.id);
    return `<label class="ext-row" data-ext-session="${esc(x.id)}">
      <input type="checkbox" data-act="ext-sess|pick|${esc(x.id)}" ${on ? "checked" : ""} />
      <span class="grow"><b>${esc(x.title)}</b>
        <i class="tiny muted">${esc(x.workspaceHash)} · ${esc(x.updated)}${x.hasHistory ? "" : " · 无正文"}</i></span>
    </label>`;
  }).join("");

  const targetBtns = targets.map((a) => `<button class="opt${target && target.id === a.id ? " on" : ""}"
    data-act="ext-sess|target|${a.id}">${esc(a.name)}</button>`).join("");

  return `<div class="dsec" data-ext-state="ok">
    <div class="kv"><span class="k">来源账号</span><span class="v">${esc(source ? source.name : accountId)}</span></div>
    <div class="kv"><span class="k">数据目录</span><span class="v tiny muted" style="word-break:break-all">${esc(view.dataRoot)}</span></div>
    ${view.skipped > 0
      ? `<p class="tiny" style="color:var(--warning);margin:4px 0" data-ext-skipped="${view.skipped}">
          有 ${view.skipped} 个工作区索引读不出来，这些工作区里的会话**没有列在这里**。</p>`
      : ""}
  </div>

  <div class="dsec">
    <h4>选择要复制的会话 <span class="badge">${view.sessions.length}</span></h4>
    <div class="row" style="gap:6px;margin-bottom:6px">
      <button class="btn ghost sm" data-act="ext-sess|all">${picked.length === view.sessions.length ? "全不选" : "全选"}</button>
    </div>
    <div class="ext-list">${rows}</div>
  </div>

  <div class="dsec">
    <h4>复制到</h4>
    <div class="row wrap">${targetBtns || `<span class="tiny muted">本区域没有其它账号可作为目标。</span>`}</div>
    <p class="tiny muted" style="margin:6px 0 0">副本会**新建**会话（新 id）—— 目标账号原有的会话**一个字都不会被动**。</p>
    <button class="btn primary" data-act="ext-sess|copy" ${picked.length && target ? "" : "disabled"}>
      ${ICON.arrowRight}复制 ${picked.length} 个会话${target ? ` 到 ${esc(target.name)}` : ""}
    </button>
    ${picked.length ? "" : `<p class="tiny muted" style="margin:4px 0 0">先勾选要复制的会话。</p>`}
  </div>`;
}

function drawerExtSessionCopy(accountId) {
  const view = EXT_SESSIONS[state.extVariant];
  state.drawer = { kind: "ext-sessions", id: accountId };
  /* 默认**全选**：复制是加法（新 id、不覆盖）⇒ 默认带走全部是安全且省事的。 */
  state.extPicked = new Set(view.sessions.map((x) => x.id));
  const a = curScope().accounts.find((x) => x.id === accountId);
  openDrawer(extSessionCopyHtml(accountId), `插件会话 · ${a ? a.name : accountId}`);
}

/** 只重画抽屉**内容**（不重跑 `drawerExtSessionCopy` —— 那会把勾选重置成全选）。 */
function renderExtDrawer() {
  if (!state.drawer || state.drawer.kind !== "ext-sessions") return;
  $("#drawer-body").innerHTML = extSessionCopyHtml(state.drawer.id);
}

function drawerSession(key) {
  const g = SESSION_GROUPS.find((x) => x.key === key);
  if (!g) return;
  openDrawer(sessionGraphHtml(g), `会话 · ${g.title}`);
}

/* ══════════════════════ 设置 ══════════════════════ */
function viewSettings() {
  const p = curPlatform();
  const group = SETTINGS.find((s) => s.id === state.setSub);
  return `<div class="page">
    <div class="page-head">
      <div><h1>设置</h1><p>${scopeLabel(state.scope)} · 按主题分区，不再是一屏长滚动</p></div>
      <div class="grow"></div>
      <span class="scope-chip">${p.mark}${scopeLabel(state.scope)}</span>
    </div>
    <div class="split">
      <nav class="subnav">${SETTINGS.map((s) => `<button class="${state.setSub === s.id ? "on" : ""}" data-act="set-sub|${s.id}">${s.label}</button>`).join("")}</nav>
      <div class="split-main">
        ${group.kind === "backup" ? viewBackup() : group.groups.map((g) => {
          const rows = g.rows.filter((r) => !r.cap || p.capabilities.includes(r.cap));
          if (!rows.length) return "";
          return `<section class="card">
            <div class="card-head"><h3>${esc(g.title)}</h3></div>
            ${rows.map((r) => settingsRow(r)).join("")}
            ${g.foot === "jitter" ? jitterPreviewHtml() : ""}
          </section>`;
        }).join("")}
      </div>
    </div>
  </div>`;
}

function settingsRow(r) {
  /* 抖动行自带「开关 + 幅度分段器」，形态与普通 switch 行不同，故整体交给渲染函数。 */
  if (r.type === "jitter") return jitterRowHtml();
  const label = `<div class="lab"><b>${esc(r.label)}</b><span>${esc(r.desc || "")}</span></div>`;
  let ctl = "";
  if (r.type === "switch") ctl = `<button class="switch${r.value ? " on" : ""}" data-act="toggle|${esc(r.label)}"></button>`;
  else if (r.type === "select") ctl = `<select class="inp" data-act="noop">${r.options.map((o) => `<option${o === r.value ? " selected" : ""}>${esc(o)}</option>`).join("")}</select>`;
  else if (r.type === "number" || r.type === "time") ctl = `<input class="inp" type="${r.type === "time" ? "time" : "text"}" value="${esc(r.value)}"${r.bind ? ` data-bind="${esc(r.bind)}"` : ""} />`;
  else if (r.type === "button") ctl = `<button class="btn sm danger" data-act="setting-action|${esc(r.label)}">${esc(r.value)}</button>`;
  else ctl = `<span class="mono muted">${esc(r.value)}</span>`;
  return `<div class="srow">${label}${ctl}</div>`;
}

/* ══════════════════════ 随机抖动（签到执行时刻）══════════════════════
 *
 * 需求（issue #27 的另一支）：给「执行时刻」加一个开关，打开后每次触发在该时刻
 * 前后随机偏移一点，避免每天都精确落在同一分钟。
 *
 * 原型要回答的问题不是「能不能抖」，而是**抖了之后用户看得懂吗**：
 *   · 开/关的差异必须一眼可见（预览直接给出未来三次的真实时刻与偏移量）；
 *   · 偏移必须**可复现** —— 同一 (日期, 幅度) 永远得到同一时刻，否则用户没法解释
 *     「今天为什么这个点签的」，测试也没法断言。故这里用确定性派生，不用 Math.random。
 */

/** 幅度三档（分钟）。只给三档：原型验证的是「开关是否被理解」，不是参数空间。 */
const JITTER_STEPS = [5, 15, 30];

/** 「执行时刻」的当前值（预览基准点）。取自 SETTINGS，避免与上面那一行漂移。 */
function checkinBaseTime() {
  const group = SETTINGS.find((s) => s.id === "checkin");
  const row = group && group.groups.flatMap((g) => g.rows).find((r) => r.type === "time");
  return (row && row.value) || "08:00";
}

/**
 * 抖动偏移（分钟）：由 (日期, 幅度) 派生的**确定性**伪随机，落在 [-max, +max]。
 *
 * 用 FNV-1a 而不是 `Math.random()`：真实实现同样要按 (任务, 日期) 派生偏移
 * （随机源会让「同一份配置两次运行结果不同」，排程不可复现、测试无法断言）。
 * 原型的数字因此与将来后端算出的形状一致 —— 只是这里的哈希写在前端便于演示。
 */
function jitterOffset(dateStr, max) {
  let h = 2166136261;
  for (let i = 0; i < dateStr.length; i += 1) {
    h ^= dateStr.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return ((h >>> 0) % (max * 2 + 1)) - max;
}

/** `"08:00"` + n 分钟 → `"07:52"`；跨零点回绕，绝不产出 `24:xx`。 */
function shiftTime(hhmm, minutes) {
  const [h, m] = hhmm.split(":").map(Number);
  const total = (((h * 60 + m + minutes) % 1440) + 1440) % 1440;
  return `${String(Math.floor(total / 60)).padStart(2, "0")}:${String(total % 60).padStart(2, "0")}`;
}

/** 演示用的三天：**固定日期**，保证截图与评审看到的是同一组数字。 */
const JITTER_DAYS = [
  { iso: "2026-10-17", label: "今天" },
  { iso: "2026-10-18", label: "明天" },
  { iso: "2026-10-19", label: "后天" },
];

/** 设置行：开关 + （开启时）幅度分段器。 */
function jitterRowHtml() {
  const j = state.jitter;
  const seg = j.on
    ? `<div class="seg" role="group" aria-label="抖动幅度">${JITTER_STEPS.map(
        (m) =>
          `<button class="${j.minutes === m ? "on" : ""}" data-act="jitter-range|${m}" aria-pressed="${j.minutes === m}">±${m} 分</button>`,
      ).join("")}</div>`
    : "";
  return `<div class="srow">
    <div class="lab">
      <b>随机抖动 <i class="badge info">新增</i></b>
      <span>${
        j.on
          ? `每次触发在 <b>${esc(checkinBaseTime())}</b> 前后 <b>±${j.minutes} 分钟</b>内偏移（每天不同，同一天恒定）`
          : "关闭时严格按「执行时刻」准点触发（当前行为，默认）"
      }</span>
    </div>
    ${seg}
    <button class="switch${j.on ? " on" : ""}" role="switch" aria-checked="${j.on}"
      aria-label="随机抖动" data-act="jitter-toggle"></button>
  </div>`;
}

/** 预览：未来三次触发的真实时刻 + 偏移量。开关关掉时三格都是准点，差异一眼可见。 */
function jitterPreviewHtml() {
  const j = state.jitter;
  const base = checkinBaseTime();
  const cells = JITTER_DAYS.map(({ iso, label }) => {
    const off = j.on ? jitterOffset(iso, j.minutes) : 0;
    const delta = off === 0 ? "准点" : `${off > 0 ? "+" : "−"}${Math.abs(off)} 分`;
    return `<span class="jcell">
      <i>${label}</i>
      <b class="mono">${shiftTime(base, off)}</b>
      <em class="${off === 0 ? "" : "hit"}">${delta}</em>
    </span>`;
  }).join("");
  return `<div class="jitter-preview">
    <span class="tiny muted">${
      j.on ? `未来三次触发（基准 ${base}，±${j.minutes} 分钟）` : `未来三次触发（基准 ${base}，抖动已关闭）`
    }</span>
    <div class="jcells">${cells}</div>
    <p class="tiny muted jnote">偏移由 (日期, 幅度) 派生：同一组合永远得到同一时刻 —— 刷新、重启都不会漂移，便于排查「今天为什么是这个点签的」。</p>
  </div>`;
}

/* ══════════════════════ 全局备份与恢复（PLAN §3.14）══════════════════════ */

/* parts 懒初始化 = 默认「全部作用域 × 全部内容」，与 §3.14 的默认全选一致。 */
function backupParts() {
  if (!Object.keys(state.backup.parts).length) {
    for (const s of BACKUP_SECTIONS) state.backup.parts[s.id] = { accounts: true, config: true, gateway: true };
  }
  return state.backup.parts;
}

/* 预估体积与条目数：**全部由已勾选的分区推导**，不写死（§3.10 第 1 条）。 */
function backupEstimate() {
  const parts = backupParts();
  let bytes = BACKUP_BYTES.base, accounts = 0, scopes = 0, configs = 0, keys = 0;
  for (const s of BACKUP_SECTIONS) {
    const p = parts[s.id];
    if (p.accounts || p.config || p.gateway) scopes += 1;
    if (p.accounts) { accounts += s.accounts; bytes += s.accounts * BACKUP_BYTES.account; }
    if (p.config) { configs += s.configs; bytes += s.configs * BACKUP_BYTES.config; }
    if (p.gateway) { keys += s.gatewayKeys; bytes += s.gatewayKeys * BACKUP_BYTES.gatewayKey; }
  }
  if (state.backup.creds) bytes += accounts * BACKUP_BYTES.credential;
  return { bytes, accounts, scopes, configs, keys };
}

function viewBackup() {
  const b = state.backup;
  const parts = backupParts();
  const est = backupEstimate();

  const rows = BACKUP_SECTIONS.map((s) => {
    const p = parts[s.id];
    const all = p.accounts && p.config && p.gateway;
    const none = !p.accounts && !p.config && !p.gateway;
    const tri = all ? "on" : none ? "" : "mixed";
    const cell = (key, label, count) =>
      `<span class="bk-part">
        <button class="check${p[key] ? " on" : ""}" data-act="bk-part|${s.id}|${key}"
          role="checkbox" aria-checked="${p[key] ? "true" : "false"}"
          aria-label="${esc(s.label)} 的${label}">${p[key] ? ICON.check : ""}</button>
        <span class="bk-lab">${label}<i class="tiny muted"> ${count}</i></span>
      </span>`;
    return `<div class="bk-row">
      <button class="check${tri === "on" ? " on" : ""}${tri === "mixed" ? " mixed" : ""}"
        data-act="bk-scope|${s.id}"
        role="checkbox" aria-checked="${tri === "mixed" ? "mixed" : tri === "on" ? "true" : "false"}"
        aria-label="全选 ${esc(s.label)}">${tri === "on" ? ICON.check : tri === "mixed" ? "–" : ""}</button>
      <span class="bk-scope">${esc(s.label)}</span>
      <span class="bk-parts">
        ${cell("accounts", "账号", s.accounts)}
        ${cell("config", "配置", s.configs)}
        ${cell("gateway", "网关", s.gatewayKeys)}
      </span>
    </div>`;
  }).join("");

  return `
    <section class="card">
      <div class="card-head"><h3>导出备份</h3></div>
      <p class="tiny muted" style="margin:0 0 10px">一次覆盖全部作用域。默认不含凭据 —— 文件里不会出现任何 token 字段。</p>
      <div class="bk-list bk-export-list">${rows}</div>

      <div class="srow">
        <div class="lab"><b>包含登录凭据（token）</b><span>开启后文件可被他人直接用来登录你的账号</span></div>
        <button class="switch${b.creds ? " on" : ""}" data-act="bk-creds" role="switch"
          aria-checked="${b.creds ? "true" : "false"}" aria-label="包含登录凭据"></button>
      </div>
      ${b.creds ? `<div class="bk-warn" role="alert">⚠️ 文件将包含明文凭据，请只保存在可信位置，不要通过聊天工具发送。</div>` : ""}

      <div class="bk-est">
        预计 <b>~${Math.max(1, Math.round(est.bytes / 1024))} KB</b> ·
        <b>${est.accounts}</b> 个账号 · <b>${est.scopes}</b> 个作用域 ·
        <b>${est.configs}</b> 项配置 · <b>${est.keys}</b> 个网关密钥 ·
        ${b.creds ? "<b>含凭据</b>" : "不含凭据"}
      </div>
      <div class="bk-actions">
        <button class="btn primary" data-act="bk-export"${est.scopes ? "" : " disabled"}>
          ${ICON.download}${b.creds ? "导出（含凭据）…" : "导出为文件…"}
        </button>
      </div>
    </section>

    <section class="card">
      <div class="card-head"><h3>导入备份</h3></div>
      ${b.result ? backupResultHtml(b.result) : b.preview ? backupPreviewHtml() : `
        <p class="tiny muted" style="margin:0 0 10px">选择文件后<b>先预览</b>，确认新增 / 覆盖 / 跳过数量再落盘。导入前会自动快照当前状态。</p>
        <button class="btn" data-act="bk-import">${ICON.download}选择备份文件…</button>`}
    </section>`;
}

function backupPreviewHtml() {
  const f = BACKUP_FILE_SAMPLE;
  const rows = Object.keys(f.scopes).map((id) => {
    const s = f.scopes[id];
    return `<div class="bk-row">
      <span class="bk-scope">${esc(scopeLabel(id))}</span>
      <span class="tiny">新增 <b>${s.added}</b> · 覆盖 <b>${s.replaced}</b> · 跳过 <b>${s.skipped}</b> · 冲突 <b>${s.conflict}</b></span>
    </div>`;
  }).join("");
  const cfgs = Object.values(f.scopes).flatMap((s) => s.configs);
  const opts = [["keep-local", "保留本地"], ["overwrite", "覆盖"], ["keep-both", "保留两者"], ["ask", "逐条询问"]];
  return `
    <div class="bk-pv-head">
      将导入 <b>buddy-switch-backup</b>
      <span class="tiny muted">formatVersion ${f.formatVersion} · ${f.exportedAt.slice(0, 10)} · ${f.includesCredentials ? "含凭据" : "不含凭据"}</span>
    </div>
    <div class="bk-list bk-pv-list">${rows}</div>
    ${cfgs.length ? `<div class="tiny muted" style="margin:6px 0">配置：${cfgs.length} 项将被覆盖（${cfgs.join(" / ")}）</div>` : ""}
    ${!f.includesCredentials ? `<div class="bk-warn" role="alert">该文件不含凭据 ⇒ 导入后 8 个账号需重新登录。</div>` : ""}
    <div class="srow">
      <div class="lab"><b>冲突处理</b><span>同一账号（按 uid 匹配）已存在时怎么办</span></div>
      <div class="seg">${opts.map(([v, l]) => `<button class="${state.backup.conflict === v ? "on" : ""}" data-act="bk-conflict|${v}">${l}</button>`).join("")}</div>
    </div>
    <div class="bk-actions">
      <button class="btn ghost" data-act="bk-cancel">取消</button>
      <button class="btn primary" data-act="bk-run">开始导入</button>
    </div>`;
}

function backupResultHtml(r) {
  return `<div class="bk-res">
    <div class="bk-est">
      <span class="badge ok">成功</span>
      新增 <b>${r.added}</b> · 覆盖 <b>${r.replaced}</b> · 跳过 <b>${r.skipped}</b>
      ${r.fail ? ` · <span class="badge bad">失败 ${r.fail}</span>` : ""}
    </div>
    ${r.fail ? `<div class="bk-warn" role="alert">${r.fail} 个账号导入失败（凭据文件被客户端占用）。
      <button class="btn sm" data-act="bk-retry">重试失败项</button></div>` : ""}
    <div class="tiny muted" style="margin:6px 0">已自动快照导入前状态，可在「从自动备份恢复」中回退。</div>
    <div class="bk-actions"><button class="btn ghost sm" data-act="bk-reset">完成</button></div>
  </div>`;
}

/* ══════════════════════ 抽屉 ══════════════════════ */
/**
 * 打开右侧抽屉。
 *
 * @param foot 固定底栏的 HTML（只有**账号详情**用；向导 / 扫描这类流程不给底栏，
 *   它们的「上一步 / 下一步」在正文里，底栏再放一份就成了两套按钮）。
 */
function openDrawer(html, title, foot) {
  clearInterval(permTimer); permTimer = null;
  $("#drawer-title").textContent = title;
  $("#drawer-body").innerHTML = html;
  const footEl = $("#drawer-foot");
  if (footEl) {
    footEl.innerHTML = foot || "";
    footEl.classList.toggle("show", Boolean(foot));
  }
  $("#drawer").classList.add("open");
  $("#drawer").setAttribute("aria-hidden", "false");
  $("#drawer-overlay").classList.add("open");
}
function closeDrawer() {
  clearInterval(permTimer); permTimer = null;
  $("#drawer").classList.remove("open");
  $("#drawer").setAttribute("aria-hidden", "true");
  $("#drawer-overlay").classList.remove("open");
  state.drawer = null;
}

/* 积分 / 资源包明细 —— 对齐「积分统计」页与设计参考图：
   每行 = 包名 + 到期时刻 + 剩余/总量 + 已用 + 进度条；按到期升序，已用完的不占位置。

   三条容易做错的：
   ① 进度条画的是**剩余 / 总量**，不是已用 —— 用户关心「这个包还能用多少」，
      画反了就变成「条越长越穷」；已用只做右侧对照数字。
   ② 颜色按**到期紧迫度**（7 天内橙、否则绿），不是按用了多少：
      同一批发放的包，没动过的也可能下周就过期，那才是要提醒的事。
   ③ 到期显示到分钟（`expireAt`）—— 只写「3 天后过期」时，用户没法判断赶不赶得上；
      数据里没有 `expireAt` 的旧包才退回天数。 */
function packsHtml(a) {
  const now = Date.now();
  const packs = (a.packs || [])
    .filter((p) => p.remaining > 0)
    .sort((x, y) => (x.expireAt || Infinity) - (y.expireAt || Infinity) || x.expireInDays - y.expireInDays);
  if (!packs.length) return `<div class="tiny muted">暂无可用资源包</div>`;
  const sumLeft = packs.reduce((sum, p) => sum + p.remaining, 0);
  const sumAll = packs.reduce((sum, p) => sum + p.total, 0);
  const nearest = packs[0];
  const nearestText = nearest.expireAt ? fmtMonthDay(nearest.expireAt) : `${nearest.expireInDays} 天后`;
  return `<div class="pk-sum">共 ${packs.length} 个包 · 剩余 <b>${fmtQty(sumLeft)}</b> / ${fmtQty(sumAll)} · 最近 ${nearestText} 到期</div>
    <div class="pk-list">
      ${packs.map((p) => {
        const pct = p.total > 0 ? Math.max(0, Math.min(100, (p.remaining / p.total) * 100)) : 0;
        const soon = p.expireAt ? p.expireAt - now <= 7 * 86400000 : p.expireInDays <= 7;
        const expText = p.expireAt ? `到期 ${fmtMonthDay(p.expireAt)}` : `${p.expireInDays} 天后过期`;
        return `<div class="pk">
          <div class="pk-top">
            <span class="pk-name">${esc(p.name)}</span>
            <span class="pk-exp${soon ? " warn" : ""}">${expText}</span>
          </div>
          <div class="pk-num">
            <span class="pk-left">${fmtQty(p.remaining)}<em> / ${fmtQty(p.total)}</em></span>
            <span class="pk-used">已用 ${fmtQty(p.total - p.remaining)}</span>
          </div>
          <div class="pk-bar"><i class="${soon ? "warn" : ""}" style="width:${pct.toFixed(1)}%"></i></div>
        </div>`;
      }).join("")}
    </div>`;
}
/* 签到历史：近 7 天。积分数额按作用域取（Trae +50 / WorkBuddy +100），不硬编码。 */
function checkinHistoryHtml(a) {
  const reward = curScope().checkinReward || 100;
  const hist = a.checkinHistory || [];
  if (!hist.length) return `<div class="tiny muted">暂无签到记录</div>`;
  return `<div class="tbl-wrap"><table class="tbl">
    <thead><tr><th>日期</th><th>结果</th><th>获得积分</th></tr></thead>
    <tbody>${hist.map((h) => `<tr>
      <td class="nowrap">${esc(h.date)}</td>
      <td>${h.ok ? `<span class="badge ok">成功</span>` : `<span class="badge bad">失败</span>`}</td>
      <td class="num">${h.ok ? "+" + reward : "—"}</td>
    </tr>`).join("")}</tbody></table></div>`;
}

/* 登录态快照：只有声明了 snapshot 能力的平台才渲染（能力开关驱动，非平台硬编码）。 */
function snapshotsHtml(a) {
  const list = a.snapshots || [];
  return `<div class="dsec">
    <h4>登录态快照</h4>
    ${list.length ? `<div class="tbl-wrap"><table class="tbl">
      <thead><tr><th>时间</th><th>大小</th><th style="width:70px"></th></tr></thead>
      <tbody>${list.map((s, i) => `<tr>
        <td class="nowrap">${esc(s.ts)}</td><td class="num">${esc(s.size)}</td>
        <td><button class="btn xs" data-act="snap-restore|${a.id}|${i}">恢复</button></td>
      </tr>`).join("")}</tbody></table></div>`
      : `<div class="empty" style="padding:22px 12px"><div class="ico">${ICON.clock}</div><b>还没有快照</b><p>保存当前登录态，之后可一键恢复。</p></div>`}
    <button class="btn sm" data-act="snap-save|${a.id}">${ICON.plus}保存当前登录态</button>
  </div>`;
}

function nowStamp() {
  const d = new Date();
  const pad = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/* ─────────── 账号详情抽屉（v2：指标条 + Tab + 固定底栏）───────────
   重设计的核心是**分工**，评审请看这一条：
     · 卡片页脚     → 勾选 / 切换程序位（点一下就切，最高频）
     · 卡片「更多」 → **所有单步动作**：4 项刷新、手动签到、导出、删除
     · 账号详情     → **只看 + 需要上下文的深操作**：指标 / 资源包 / 签到历史 / 凭据 / 快照，
                      动作只剩「带迁移的切换」「续期 JWT」「权限诊断」「复制插件会话」
   判据：**能一步做完的进菜单；要看数据才敢决定的留在抽屉。** 同一个动作不许两处都有 ——
   上一版「刷新」在菜单、「刷新 Token」在抽屉、「手动签到」两边都能点，就是乱在重复上。 */
const ACCOUNT_TABS = [
  { id: "overview", label: "概览" },
  { id: "credits", label: "积分" },
  { id: "checkin", label: "签到" },
  { id: "credential", label: "凭据" },
];

/* 分组：真实的组是**用户自建**的。下面几档只是演示数据里已经在用的名字，
   真正的来源是 `curScope().groups`（新建分组会写进去）+ 当前值兜底。 */
const ACCOUNT_GROUPS = ["常用", "测试", "闲置", "团队", "默认"];

/** 当前作用域里可选的分组名：内置 ∪ 用户新建过的 ∪ 当前值。 */
function scopeGroups() {
  const own = curScope().groups || [];
  return [...new Set([...ACCOUNT_GROUPS, ...own])];
}

/**
 * 分组选择器（就地改，改完即存）。
 *
 * 为什么用原生 `<select>` 而不是自建弹层：分组是**单值枚举**，原生 select 自带键盘、
 * 触屏与无障碍语义；自建弹层只会得到一个「看起来像下拉但其实不会用 Tab 操作」的控件。
 * 真实实现里这里是 shadcn 的 `Select`（同样是 Radix 的单值选择器）。
 */
function groupSelect(a) {
  const cur = a.group || "默认";
  const base = scopeGroups();
  const opts = base.includes(cur) ? base : [cur, ...base];
  /* 新建态：下拉原地变成输入框 —— 不弹第二个弹层，避免「弹层里开弹层」的焦点跳走。 */
  if (state.groupNew === a.id) {
    return `<span class="group-new">
      <input id="group-new-input" class="inp sel-inline" style="width:118px" maxlength="20"
        placeholder="新分组名" aria-label="新分组名" autocomplete="off" />
      <button class="btn xs primary" data-act="group-create|${a.id}">创建</button>
      <button class="btn ghost xs" data-act="group-create-cancel|${a.id}">取消</button>
    </span>`;
  }
  return `<select class="inp sel-inline" data-account-group="${a.id}" aria-label="分组">
    ${opts.map((g) => `<option value="${esc(g)}"${g === cur ? " selected" : ""}>${esc(g)}</option>`).join("")}
    <option value="__new__">＋ 新建分组…</option>
  </select>`;
}

/**
 * 指标块 + **就地刷新**。
 *
 * ★ 为什么抽屉里也留刷新入口：抽屉就是「看数据」的地方，看完发现数字旧了却要关掉抽屉、
 * 点卡片「⋯」、再点刷新、再打开抽屉 —— 这个来回正是上一版被吐槽的那种别扭。
 * 四个块各对应一个数据源，`data-act` 与卡片菜单里的四项**完全一致**（`acct-refresh|<id>|<kind>`），
 * 所以状态、反馈、失败口径全都复用同一套，不是第二套实现。
 */
function drawerStatTile(a, kind, label, valueHtml, subHtml, tone) {
  const key = `${a.id}:${kind}`;
  const busy = state.refreshing.has(key);
  const done = state.refreshDone.has(key);
  return `<div class="stat-tile${busy ? " busy" : ""}">
      <div class="lab">${label}<button class="tile-rf" data-act="acct-refresh|${a.id}|${kind}"
        title="刷新${label}" aria-label="刷新${label}"${busy ? " disabled" : ""}
        >${busy ? '<span class="spinner"></span>' : done ? ICON.check : ICON.refresh}</button></div>
      <div class="val">${valueHtml}</div>
      <div class="sub${tone ? " " + tone : ""}">${subHtml}</div>
    </div>`;
}

/** 四个关键指标：一眼看清「这个账号现在什么状态」，不用往下滚。 */
function drawerStats(a) {
  const hist = a.checkinHistory || [];
  const okDays = hist.filter((h) => h.ok).length;
  const credTone = a.expired ? "bad" : a.needsRelogin ? "bad" : "";
  const credText = a.expired ? "已过期" : a.needsRelogin ? "需重登" : "有效";
  const credSub = a.expired ? "先重新登录" : a.needsRelogin ? "会话已失效" : "可正常使用";
  return `<div class="acct-stats">
    ${drawerStatTile(a, "credits", "积分",
      `${fmt(a.credits)}<em>/${fmt(a.creditsTotal)}</em>`,
      a.expiring ? `${fmt(a.expiring)} 积分 ${a.expireInDays} 天内到期` : "无临期积分",
      a.expiring ? "warn" : "")}
    ${drawerStatTile(a, "tokens", "今日 Token", fmtTok(a.todayTokens), `累计 ${fmtTok(a.totalTokens)}`)}
    ${drawerStatTile(a, "checkin", "今日签到", a.checkedIn ? "已签到" : "未签到", `近 7 天 ${okDays}/${hist.length || 0}`)}
    ${drawerStatTile(a, "credential", "凭据", credText, credSub, credTone)}
  </div>`;
}

function drawerTabsNav(a, active, tabCounts) {
  const items = ACCOUNT_TABS.concat(curPlatform().capabilities.includes("snapshot") ? [{ id: "snapshot", label: "快照" }] : []);
  return `<div class="drawer-tabs" role="tablist">
    ${items.map((t) => `<button role="tab" aria-selected="${t.id === active}" class="${t.id === active ? "on" : ""}"
      data-act="drawer-tab|${a.id}|${t.id}">${t.label}${tabCounts && tabCounts[t.id] ? `<span class="n">${tabCounts[t.id]}</span>` : ""}</button>`).join("")}
  </div>`;
}

/** 概览：身份类信息 + 资源包 + 插件会话（备注在这一页就地编辑，因为改之前必须看见原文）。 */
function drawerOverview(a, editing) {
  const p = curPlatform();
  const ext = p.programs.find((pg) => (pg.id === "vscode" || pg.id === "ide") && a.programs[pg.id]);
  const loginOn = p.programs.filter((pg) => a.programs[pg.id]).map((pg) => pg.label);
  return `<div class="dsec">
      <h4>账号信息</h4>
      <div class="kv"><span class="k">分组</span><span class="v">${groupSelect(a)}</span></div>
      ${editing
        ? `<div class="kv"><span class="k">备注</span><span class="v"><input id="remark-input" class="inp" style="width:100%" value="${esc(a.remark || "")}" placeholder="输入备注…" /></span></div>
           <div class="row" style="gap:6px">
             <button class="btn sm primary" data-act="act|remark-save|${a.id}">保存</button>
             <button class="btn ghost sm" data-act="act|remark-cancel|${a.id}">取消</button>
           </div>`
        : `<div class="kv"><span class="k">备注</span><span class="v">${esc(a.remark || "—")}
             <button class="btn ghost xs" style="margin-left:6px" data-act="act|remark|${a.id}">${ICON.wand}编辑</button></span></div>`}
      <div class="kv"><span class="k">最后使用</span><span class="v">${esc(a.lastUsed)}</span></div>
      <div class="kv"><span class="k">登录程序位</span><span class="v">${loginOn.length ? esc(loginOn.join(" · ")) : `<span class="muted">未绑定</span>`}</span></div>
    </div>
    ${ext ? `<div class="dsec">
      <h4>插件会话</h4>
      <button class="btn sm" data-act="ext-sess|open|${a.id}">${ICON.sessions}复制插件会话（${esc(ext.label)}）</button>
      <div class="tiny muted">把该账号在插件侧的历史会话<b>单向复制</b>到当前工作区，不会回写。</div>
    </div>` : ""}`;
}

function drawerCheckin(a) {
  return `<div class="dsec">
      <h4>近 7 天签到</h4>
      ${checkinHistoryHtml(a)}
      <div class="tiny muted">手动签到在卡片右上角的「更多」里 —— 这里只负责看历史，不放第二个签到按钮。</div>
    </div>`;
}

/** 凭据：这里才有写操作（续期会改写客户端凭据），所以必须先把当前状态摆在上面。 */
function drawerCredential(a) {
  const state = a.expired ? { tone: "bad", text: "已过期" } : a.needsRelogin ? { tone: "bad", text: "需重新登录" } : { tone: "ok", text: "有效" };
  return `<div class="dsec">
      <h4>凭据状态</h4>
      <div class="kv"><span class="k">状态</span><span class="v"><span class="badge ${state.tone}">${state.text}</span></span></div>
      <div class="kv"><span class="k">来源</span><span class="v">本机客户端登录态</span></div>
      <div class="row wrap" style="margin-top:2px">
        <button class="btn sm" data-act="act|refresh-token|${a.id}">${ICON.refresh}续期 JWT</button>
        <button class="btn sm" data-act="perm-guide|${a.id}">${ICON.shield}切换权限诊断</button>
      </div>
      <div class="tiny muted">「续期 JWT」会<b>写入</b>客户端凭据文件；只想看有效期，用卡片「更多 → 刷新凭据状态」（只读）。</div>
      ${(a.expired || a.needsRelogin)
        ? `<div class="tiny" style="color:var(--destructive)">凭据已失效：积分 / Token / 签到三项在修好它之前都读不到，先重新登录。</div>`
        : ""}
    </div>`;
}

function drawerCredits(a) {
  return `<div class="dsec">
      <h4>积分 / 资源包</h4>
      ${packsHtml(a)}
      <div class="tiny muted">「刷新积分」在卡片「更多」里，也可以直接点上面「积分」指标块右上角的 ⟳。</div>
    </div>`;
}

function accountDrawerBody(a, tab, editing) {
  if (tab === "credits") return drawerCredits(a);
  if (tab === "checkin") return drawerCheckin(a);
  if (tab === "credential") return drawerCredential(a);
  if (tab === "snapshot") return snapshotsHtml(a);
  return drawerOverview(a, editing);
}

/** 抽屉底栏：**全抽屉唯一的动作出口** —— 带迁移的切换（正文里的程序位按钮已上移到卡片）。 */
function accountDrawerFoot(a) {
  return `<button class="btn primary" data-act="wizard|${a.id}">${ICON.arrowRight}用向导切换（可选迁移内容）</button>`;
}

function drawerAccount(id, editing) {
  const s = curScope(), p = curPlatform();
  const a = s.accounts.find((x) => x.id === id);
  if (!a) return;
  /* 保持当前 Tab：点「续期 JWT」这类动作后会重绘抽屉，跳回「概览」会让用户以为点错了。 */
  /* 换账号就别留在「新建分组」输入态（那只属于上一个账号）。 */
  if (state.groupNew && state.groupNew !== id) state.groupNew = null;
  const keepTab = state.drawer && state.drawer.kind === "account" && state.drawer.id === id ? state.drawer.tab : "overview";
  const tab = editing ? "overview" : (keepTab || "overview");
  state.drawer = { kind: "account", id, tab, editingRemark: Boolean(editing) };
  const isCur = p.programs.some((pg) => a.programs[pg.id]);
  openDrawer(`
    <div class="acct-hd">
      <span class="avatar" style="width:44px;height:44px;border-radius:13px;font-size:16px;background:${avatarTone(a.name)}">${initials(a.name)}</span>
      <span class="who"><b>${esc(a.name)}</b><span>${esc(maskEmail(a.email) || "—")} · UID ${esc(a.id)}</span></span>
      ${isCur ? `<span class="badge info">当前登录</span>` : ""}
    </div>
    ${drawerStats(a)}
    ${drawerTabsNav(a, tab)}
    ${accountDrawerBody(a, tab, Boolean(editing))}
  `, "账号详情", accountDrawerFoot(a));
  if (editing) setTimeout(() => { const i = $("#remark-input"); if (i) { i.focus(); if (i.select) i.select(); } }, 40);
}

/* 切换向导：三步。第二步的会话迁移从「布尔」升级为可展开的三层会话树
   （类型 → 目录 → 会话）。来源账号可显式指定，含跨作用域。
   opts: { program, source:{scope,id}, pickCross, sessions, memory, connectors,
           treeSel:Set<"ti/di/si">, openTree:Set<"t0"/"t0/d1"> } */
function sourceAccount(opts) {
  const s = SCOPES[opts.source.scope];
  return s ? s.accounts.find((a) => a.id === opts.source.id) || null : null;
}
function defaultTreeSel(acc) {
  const sel = new Set();
  if (acc && acc.sessionTree && acc.sessionTree[0]) {
    acc.sessionTree[0].dirs.forEach((d, di) => d.sessions.forEach((_, si) => sel.add(`0/${di}/${si}`)));
  }
  return sel;
}
function treeStats(tree, sel) {
  let total = 0, selected = 0, dirsSelected = 0;
  tree.forEach((t, ti) => t.dirs.forEach((d, di) => {
    let has = false;
    d.sessions.forEach((s, si) => { total++; if (sel.has(`${ti}/${di}/${si}`)) { selected++; has = true; } });
    if (has) dirsSelected++;
  }));
  return { total, selected, dirsSelected };
}
function allTreeOpen(acc) {
  const s = new Set();
  if (acc && acc.sessionTree) acc.sessionTree.forEach((t, ti) => {
    s.add(`t${ti}`);
    t.dirs.forEach((_, di) => s.add(`t${ti}/d${di}`));
  });
  return s;
}
function wizardDefaultOpts(a) {
  const p = curPlatform();
  const src = currentAccount(state.scope);
  const source = { scope: state.scope, id: src ? src.id : a.id };
  const srcAcc = SCOPES[source.scope].accounts.find((x) => x.id === source.id) || null;
  return {
    program: p.programs.find((pg) => pg.installed).id,
    source, pickCross: false, sessions: true, memory: false, connectors: false,
    openTree: allTreeOpen(srcAcc),
    treeSel: defaultTreeSel(srcAcc),
  };
}
function treeTypeNode(t, ti, opts) {
  const sel = opts.treeSel;
  let total = 0, selN = 0;
  t.dirs.forEach((d, di) => d.sessions.forEach((_, si) => { total++; if (sel.has(`${ti}/${di}/${si}`)) selN++; }));
  const allOn = total > 0 && selN === total;
  const open = opts.openTree.has(`t${ti}`);
  return `<div class="tree-node">
    <div class="tree-row">
      <button class="check${allOn ? " on" : ""}${selN && !allOn ? " part" : ""}" data-act="wiz-tree|type|${ti}" aria-label="选择类型">${allOn ? ICON.check : selN ? ICON.minus : ""}</button>
      <button class="tree-label" data-act="wiz-collapse|t${ti}">
        <span class="caret">${open ? "▾" : "▸"}</span><b>${esc(t.type)}</b><span class="tiny muted">${selN}/${total}</span>
      </button>
    </div>
    ${open ? t.dirs.map((d, di) => treeDirNode(d, ti, di, opts)).join("") : ""}
  </div>`;
}
function treeDirNode(d, ti, di, opts) {
  const sel = opts.treeSel;
  const keys = d.sessions.map((_, si) => `${ti}/${di}/${si}`);
  const selN = keys.filter((k) => sel.has(k)).length;
  const allOn = keys.length > 0 && selN === keys.length;
  const open = opts.openTree.has(`t${ti}/d${di}`);
  return `<div class="tree-node tree-dir">
    <div class="tree-row">
      <button class="check${allOn ? " on" : ""}${selN && !allOn ? " part" : ""}" data-act="wiz-tree|dir|${ti}/${di}" aria-label="选择目录">${allOn ? ICON.check : selN ? ICON.minus : ""}</button>
      <button class="tree-label" data-act="wiz-collapse|t${ti}/d${di}">
        <span class="caret">${open ? "▾" : "▸"}</span><span>${esc(d.name)}</span><span class="tiny muted">${selN}/${keys.length}</span>
      </button>
    </div>
    ${open ? d.sessions.map((s, si) => {
      const on = sel.has(`${ti}/${di}/${si}`);
      return `<button class="tree-session${on ? " on" : ""}" data-act="wiz-tree|sess|${ti}/${di}/${si}">
        <span class="check${on ? " on" : ""}">${on ? ICON.check : ""}</span>
        <span class="t">${esc(s.title)}</span><span class="tiny muted">${s.msgs} 条</span>
      </button>`;
    }).join("") : ""}
  </div>`;
}

function drawerWizard(id, step, opts) {
  const a = curScope().accounts.find((x) => x.id === id);
  if (!a) return;
  const p = curPlatform();
  step = step || 1;
  opts = opts || wizardDefaultOpts(a);
  state.drawer = { kind: "wizard", id, step, opts };

  const stepBar = `<div class="steps">
    ${[1, 2, 3].map((n, i) => `${i ? '<span class="line"></span>' : ""}
      <span class="s${step >= n ? " on" : ""}"><i>${n}</i>${["目标程序位", "迁移内容", "确认"][n - 1]}</span>`).join("")}
  </div>`;

  let body = "";
  if (step === 1) {
    const srcScopes = opts.pickCross ? Object.keys(SCOPES) : [state.scope];
    const accRow = (acc, sid) => {
      const on = opts.source.scope === sid && opts.source.id === acc.id;
      return `<button class="opt${on ? " on" : ""}" data-act="wiz-src|${sid}|${acc.id}">
        <span class="avatar" style="width:28px;height:28px;border-radius:8px;font-size:11px;background:${avatarTone(acc.name)}">${initials(acc.name)}</span>
        <span class="t"><b>${esc(acc.name)}</b><span>${esc(maskEmail(acc.email) || acc.remark || "")}</span></span>
        ${on ? `<span class="badge info">来源</span>` : ""}
      </button>`;
    };
    body = `
      <div class="dsec"><h4>把哪个客户端切到 ${esc(a.name)}？</h4>
        ${p.programs.map((pg) => `<button class="opt${opts.program === pg.id ? " on" : ""}" ${pg.installed ? "" : "disabled"} data-act="wiz-prog|${pg.id}">
          <span class="ico">${p.mark}</span>
          <span class="t"><b>${esc(pg.label)}</b><span>${pg.installed ? "已检测到客户端" : "未检测到，需先在设置中接入"}</span></span>
          ${opts.program === pg.id ? `<span class="badge info">已选</span>` : ""}
        </button>`).join("")}
      </div>
      <div class="dsec">
        <h4>从哪里迁移内容？</h4>
        <p class="tiny muted" style="margin:0">默认是当前登录账号；可换成作用域内任一账号，或从其他作用域选择。</p>
        ${srcScopes.map((sid) => `${srcScopes.length > 1 ? `<div class="tiny muted" style="margin-top:6px">${esc(scopeLabel(sid))}</div>` : ""}
          ${SCOPES[sid].accounts.length ? SCOPES[sid].accounts.map((acc) => accRow(acc, sid)).join("")
            : `<div class="tiny muted">该作用域暂无账号</div>`}`).join("")}
        <button class="btn ghost sm" data-act="wiz-cross">${opts.pickCross ? "仅看当前作用域" : "从其他作用域选择…"}</button>
      </div>`;
  } else if (step === 2) {
    const srcAcc = sourceAccount(opts);
    const tree = srcAcc ? srcAcc.sessionTree : [];
    const stats = treeStats(tree, opts.treeSel);
    const items = [["sessions", "复制会话", "把来源账号的会话按下方选择带过去"], ["memory", "迁移记忆", "同步记忆库内容"], ["connectors", "迁移连接器", "同步 MCP / 连接器配置"]];
    body = `<div class="dsec"><h4>要一并迁移哪些内容？</h4>
      ${items.map(([k, l, d]) => `<button class="opt${opts[k] ? " on" : ""}" data-act="wiz-opt|${k}">
        <span class="ico">${opts[k] ? ICON.check : ICON.arrowRight}</span>
        <span class="t"><b>${l}</b><span>${d}</span></span>
        <span class="badge ${opts[k] ? "info" : ""}">${opts[k] ? "迁移" : "跳过"}</span>
      </button>`).join("")}
      ${!opts.sessions ? `<p class="tiny muted" style="margin:2px 0 0">不迁移会话时，目标账号保持自己的既有会话。</p>` : ""}
    </div>
    ${opts.sessions ? `<div class="dsec">
      <div class="row"><h4 style="margin:0">选择要复制的会话</h4><div class="grow"></div><span class="tiny muted">已选 ${stats.selected} / ${stats.total}</span></div>
      <div class="row" style="gap:6px">
        <button class="btn xs" data-act="wiz-tree-all|1">全选</button>
        <button class="btn xs" data-act="wiz-tree-all|0">全不选</button>
        <div class="grow"></div>
        <span class="tiny muted">来源：${esc(srcAcc ? srcAcc.name : "—")}</span>
      </div>
      <div class="tree">${tree.map((t, ti) => treeTypeNode(t, ti, opts)).join("")}</div>
    </div>` : ""}`;
  } else {
    const pg = p.programs.find((x) => x.id === opts.program);
    const srcAcc = sourceAccount(opts);
    const stats = treeStats(srcAcc ? srcAcc.sessionTree : [], opts.treeSel);
    const mig = [];
    if (opts.sessions) mig.push(stats.selected ? `会话（已选 ${stats.selected} 个会话 / ${stats.dirsSelected} 个目录）` : "会话（未选择任何会话）");
    if (opts.memory) mig.push("记忆");
    if (opts.connectors) mig.push("连接器");
    body = `<div class="dsec"><h4>确认</h4>
      <div class="kv"><span class="k">目标账号</span><span class="v"><b>${esc(a.name)}</b> ${esc(maskEmail(a.email) || "")}</span></div>
      <div class="kv"><span class="k">来源账号</span><span class="v">${srcAcc ? `${esc(srcAcc.name)} · ${esc(scopeLabel(opts.source.scope))}` : "—"}</span></div>
      <div class="kv"><span class="k">程序位</span><span class="v">${esc(pg.label)}</span></div>
      <div class="kv"><span class="k">迁移内容</span><span class="v">${mig.join("、") || "不迁移"}</span></div>
      <div class="kv"><span class="k">影响</span><span class="v">客户端将被重启，未保存的编辑器内容可能丢失</span></div>
    </div>
    <div class="dsec"><button class="btn primary" data-act="wiz-run">${ICON.check}执行切换</button></div>`;
  }

  openDrawer(`${stepBar}
    ${body}
    <div class="row" style="margin-top:auto;padding-top:12px;border-top:1px solid var(--border)">
      <button class="btn ghost sm" data-act="wizard|${id}">返回详情</button>
      <div class="grow"></div>
      ${step > 1 ? `<button class="btn sm" data-act="wiz-back">上一步</button>` : ""}
      ${step < 3 ? `<button class="btn primary sm" data-act="wiz-next">下一步</button>` : ""}
    </div>`, `切换账号 · ${a.name}`);
}

function drawerAdd() {
  state.drawer = { kind: "add" };
  const s = curScope();
  openDrawer(`
    <p class="tiny muted" style="margin:0">当前作用域：<b>${scopeLabel(state.scope)}</b>。选择一种方式添加账号。</p>
    <div class="dsec">
      <button class="opt" data-act="add-way|oauth">
        <span class="ico">${ICON.login}</span>
        <span class="t"><b>OAuth 登录</b><span>打开官方授权页，用浏览器登录后自动导入</span></span>
        ${ICON.arrowRight}
      </button>
      <button class="opt" data-act="add-way|local">
        <span class="ico">${ICON.upload}</span>
        <span class="t"><b>从本机导入</b><span>读取当前客户端的登录态，把正在用的账号加进来</span></span>
        ${ICON.arrowRight}
      </button>
      <button class="opt" data-act="add-way|backup">
        <span class="ico">${ICON.download}</span>
        <span class="t"><b>导入备份文件</b><span>从导出的 JSON 备份批量恢复账号</span></span>
        ${ICON.arrowRight}
      </button>
    </div>
    ${!s.installed ? `<div class="card card-pad row" style="border-color:color-mix(in oklab, var(--warning) 40%, transparent)">
      <span class="badge warn">${ICON.warn}</span>
      <span class="tiny">当前作用域未检测到客户端，「从本机导入」不可用。可先用 OAuth 登录。</span></div>` : ""}
  `, "添加账号");
}

/* ── 权限错误升级引导 ──
   真实应用最高频的故障：写入凭据需要管理员权限。
   三步引导 + 每 2 秒一次的自检轮询（演示 3 轮后自愈成功）。 */
let permTimer = null;
function permStatusHtml() {
  const d = state.drawer || {};
  if (d.permDone) return `<div class="perm-check ok"><span class="badge ok">${ICON.check}已检测到管理员权限</span><span class="tiny muted">现在可以重试切换</span></div>`;
  return `<div class="perm-check"><span class="spinner" aria-hidden="true"></span><span>自动检测中… 第 ${d.permRound || 0}/3 次自检</span></div>`;
}
function startPermCheck() {
  clearInterval(permTimer);
  state.drawer.permRound = 0; state.drawer.permDone = false;
  const paint = () => { const box = $("#perm-status"); if (box) box.innerHTML = permStatusHtml(); };
  paint();
  permTimer = setInterval(() => {
    const d = state.drawer;
    if (!d || d.kind !== "perm") { clearInterval(permTimer); permTimer = null; return; }
    d.permRound = (d.permRound || 0) + 1;
    if (d.permRound >= 3) {
      clearInterval(permTimer); permTimer = null;
      d.permDone = true;
      paint();
      const btn = $("#perm-retry"); if (btn) btn.disabled = false;
      toast("ok", "已检测到管理员权限", "第 3 次自检通过，可以重试切换了");
    } else { paint(); }
  }, 2000);
}
function drawerPermGuide(id) {
  const a = curScope().accounts.find((x) => x.id === id);
  if (!a) return;
  state.drawer = { kind: "perm", id, permRound: 0, permDone: false };
  const steps = [
    ["关闭客户端", `完全退出正在使用 ${a.name} 的客户端（含系统托盘）`],
    ["以管理员身份重启", "右键客户端图标 →「以管理员身份运行」"],
    ["回到本应用重试", "权限就绪后，点下方按钮重试切换"],
  ];
  openDrawer(`
    <div class="dsec">
      <div class="row" style="gap:10px">
        <span class="badge bad">${ICON.warn}权限不足</span>
        <span class="tiny muted">切换到 ${esc(a.name)} 时被系统拒绝</span>
      </div>
      <p class="tiny muted" style="margin:0">写入账号凭据需要管理员权限。按以下三步操作，应用会自动检测权限是否就绪。</p>
    </div>
    <div class="dsec">
      ${steps.map((s, i) => `<div class="guide-step"><i>${i + 1}</i><div class="t"><b>${s[0]}</b><span>${s[1]}</span></div></div>`).join("")}
    </div>
    <div class="dsec" id="perm-status">${permStatusHtml()}</div>
    <div class="dsec">
      <button class="btn primary" id="perm-retry" data-act="perm-retry|${id}" disabled>${ICON.check}重试切换</button>
      <p class="tiny muted" style="margin:0">权限未就绪前按钮保持禁用，避免无效点击。</p>
    </div>
  `, "权限诊断");
  startPermCheck();
}

/* ── 不可逆操作的确认 Dialog ── */
let pendingConfirm = null;
function openConfirm(o) {
  $("#confirm-title").textContent = o.title;
  $("#confirm-body").innerHTML = o.body;
  const ok = $("#confirm-ok");
  ok.textContent = o.okLabel || "确认";
  ok.className = "btn " + (o.danger === false ? "primary" : "danger");
  pendingConfirm = o.onOk || null;
  $("#confirm-overlay").classList.add("open");
  $("#confirm-overlay").setAttribute("aria-hidden", "false");
}
function closeConfirm() {
  $("#confirm-overlay").classList.remove("open");
  $("#confirm-overlay").setAttribute("aria-hidden", "true");
  pendingConfirm = null;
}

/* ══════════════════════ 应用更新：升级前先讲清「这一版改了什么」══════════════════════
 *
 * 用户原话：「每次升级都不知道这个版本的变化内容是什么」。
 * 旧实现（侧栏 ↑ → 抽屉里三行 bullet → 点了立刻下载）有两处硬伤：
 *   ① 说明与动作塞在同一个抽屉里，说明沦为背景 —— 用户根本来不及读；
 *   ② 一进「升级」就开始下载：**不可逆动作发生在读懂之前**。
 *
 * 现在拆成「先说明，后决定」，四个阶段全部在同一枚弹窗里：
 *   available（这一版改了什么 + 立即升级）→ downloading（进度，期间不可关闭）
 *   → ready（已下载，等重启）；失败走 error（说清失败原因 + 重试 / 打开下载页）。
 *
 * 两个刻意的取舍：
 *   - **破坏性变更置顶**（`.upd-alert`）：混在长列表里必然被划走，端口 / 数据格式这类
 *     一旦漏看就是「升级完直接用不了」。
 *   - **进度只刷节点、不重画弹窗**：整块 innerHTML 重写会把正文滚动位置打回顶部，
 *     正在读说明的人被弹回第一行（本仓已出过同类事故：抽屉重画导致旧节点脱离文档）。
 */
let updTimer = null;

function updBusy() { return state.update.stage === "downloading"; }

/* 「10 月 17 日 18:33」：同一年省略年份，减少噪声；解析失败就不显示，不留一个假日期。 */
function fmtUpdDate(iso) {
  const d = new Date(iso);
  if (!iso || Number.isNaN(d.getTime())) return "";
  const pad = (n) => String(n).padStart(2, "0");
  const md = `${d.getMonth() + 1} 月 ${d.getDate()} 日 ${pad(d.getHours())}:${pad(d.getMinutes())}`;
  return d.getFullYear() === new Date().getFullYear() ? md : `${d.getFullYear()} 年 ${md}`;
}

function updHeadHtml() {
  /* 查不到更新信息时**不摆**发布日期 / 包大小：那是上一次检查的旧值，
     摆在失败态里等于告诉用户「这次也查到了」。 */
  const meta = [`当前 v${esc(APP_UPDATE.current)}`];
  if (state.update.stage === "error") {
    meta.push("无法获取更新信息");
  } else {
    const when = fmtUpdDate(APP_UPDATE.publishedAt);
    if (when) meta.push(`发布于 ${esc(when)}`);
    if (APP_UPDATE.size) meta.push(esc(APP_UPDATE.size));
  }
  const titles = {
    available: `发现新版本 <b>v${esc(APP_UPDATE.latest)}</b>`,
    downloading: `正在升级到 <b>v${esc(APP_UPDATE.latest)}</b>`,
    ready: `v${esc(APP_UPDATE.latest)} 已就绪`,
    error: "无法完成更新检查",
  };
  const mark = state.update.stage === "error" ? ICON.warn : ICON.download;
  return `
    <header class="upd-head">
      <span class="upd-mark${state.update.stage === "error" ? " bad" : ""}">${mark}</span>
      <div class="upd-head-t">
        <h3 id="upd-title">${titles[state.update.stage] || titles.available}</h3>
        <p>${meta.join(" · ")}</p>
      </div>
      <button class="icon-btn" data-act="upd-close" title="${updBusy() ? "下载中不能关闭" : "关闭"}"
        aria-label="关闭"${updBusy() ? " disabled" : ""}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg>
      </button>
    </header>`;
}

/* 分组渲染：组标题 + 条目。**不解析 markdown** —— 版本说明来自外部发布资产，
   这里只吃 { title, items[] } 纯文本，`esc()` 之后才进 innerHTML。 */
function updBreakingHtml() {
  /* 装完之后再喊「升级前必读」是自相矛盾的（已经升了）⇒ 只在说明 / 下载两态出现。
     失败态不渲染更新内容（见 renderUpdateDialog）：没有可信数据就不摆数据。 */
  if (state.update.stage === "ready") return "";
  const items = APP_UPDATE.breaking || [];
  if (!items.length) return "";
  return `<div class="upd-alert" role="alert" data-upd-breaking>
    <div class="upd-alert-h">${ICON.warn}<b>升级前必读</b><span class="upd-count">${items.length} 项会改变既有行为</span></div>
    <ul class="upd-alert-list">
      ${items.map((x) => `<li><b>${esc(x.title)}</b><span>${esc(x.desc)}</span></li>`).join("")}
    </ul>
  </div>`;
}

function updNotesHtml() {
  const secs = APP_UPDATE.sections || [];
  if (!secs.length) {
    /* 拿不到说明就**如实说**，不要留一块空标题假装「本版没有更新」。 */
    return `<div class="upd-sec" data-upd-notes-empty>
      <h4>本版更新</h4>
      <p class="upd-empty">这次发布没有附带说明。可以打开完整发布说明查看提交记录。</p>
    </div>`;
  }
  const total = secs.reduce((n, s) => n + s.items.length, 0);
  return `<div class="upd-sec" data-upd-notes>
    <h4>本版更新 <span class="upd-count">${secs.length} 组 · ${total} 条</span></h4>
    ${secs.map((s) => `<div class="upd-grp">
      <b>${esc(s.title)}</b>
      <ul class="upd-list">${s.items.map((i) => `<li>${esc(i)}</li>`).join("")}</ul>
    </div>`).join("")}
  </div>`;
}

function updOlderHtml() {
  const older = APP_UPDATE.older || [];
  if (!older.length) return "";
  if (!state.update.olderOpen) {
    return `<button class="upd-more" data-act="upd-older" aria-expanded="false">
      ${ICON.clock}<span>更早的 ${older.length} 个版本</span><span class="upd-caret">▾</span></button>`;
  }
  return `<div class="upd-sec" data-upd-older>
    <button class="upd-more" data-act="upd-older" aria-expanded="true">
      ${ICON.clock}<span>收起更早版本</span><span class="upd-caret">▴</span></button>
    ${older.map((v) => `<div class="upd-grp is-older">
      <div class="upd-grp-h"><b>v${esc(v.version)}</b><span class="upd-count">${esc(fmtUpdDate(v.publishedAt))}</span></div>
      <ul class="upd-list">${v.items.map((i) => `<li>${esc(i)}</li>`).join("")}</ul>
    </div>`).join("")}
  </div>`;
}

/* 阶段块：进度 / 就绪 / 失败。三种都在正文顶部 —— 眼睛刚落在标题下方就能看到结果。 */
function updStageHtml() {
  if (state.update.stage === "downloading") {
    const p = Math.max(0, Math.min(100, Math.round(state.update.progress)));
    return `<div class="upd-progress" data-upd-progress>
      <div class="upd-pbar"><i style="width:${p}%"></i></div>
      <div class="upd-pmeta"><span>正在下载并安装 · 期间不要退出应用</span><span data-upd-pct>${p}%</span></div>
    </div>`;
  }
  if (state.update.stage === "ready") {
    return `<div class="upd-alert ok" role="status" data-upd-ready>
      <div class="upd-alert-h">${ICON.check}<b>下载完成，重启后生效</b></div>
      <p class="upd-alert-p">重启只需几秒；账号库、网关设置与请求日志都会保留。</p>
    </div>`;
  }
  if (state.update.stage === "error") {
    return `<div class="upd-alert bad" role="alert" data-upd-error>
      <div class="upd-alert-h">${ICON.warn}<b>检查更新失败</b></div>
      <p class="upd-alert-p">无法连接更新源（GitHub Releases 不可达）。若本机需要代理，可在「应用设置 → 更新代理」里填写后重试；也可以直接打开下载页手动安装。</p>
    </div>`;
  }
  return "";
}

function updFootHtml() {
  const st = state.update.stage;
  if (st === "downloading") {
    return `<span class="upd-foot-hint">${ICON.clock}下载完成后会提示你重启</span><span class="upd-spacer"></span>
      <button class="btn" disabled><i class="spinner"></i>下载中</button>`;
  }
  if (st === "ready") {
    return `<span class="upd-foot-hint">${ICON.check}v${esc(APP_UPDATE.latest)} 已下载</span><span class="upd-spacer"></span>
      <button class="btn" data-act="upd-later">稍后重启</button>
      <button class="btn primary" data-act="upd-restart">${ICON.refresh}立即重启</button>`;
  }
  if (st === "error") {
    return `<button class="btn ghost sm" data-act="upd-release">${ICON.external}打开下载页</button><span class="upd-spacer"></span>
      <button class="btn" data-act="upd-later">关闭</button>
      <button class="btn primary" data-act="upd-retry">${ICON.refresh}重试</button>`;
  }
  return `<button class="btn ghost sm" data-act="upd-release">${ICON.external}查看完整发布说明</button><span class="upd-spacer"></span>
    <button class="btn" data-act="upd-later">稍后升级</button>
    <button class="btn primary" data-act="upd-start">${ICON.download}立即升级</button>`;
}

function renderUpdateDialog() {
  const box = $("#update-overlay");
  if (!box) return;
  /* 失败态只有「为什么失败 + 怎么办」：查不到就没有可信的版本内容，
     把旧说明摆在旁边会让人以为「这些也查到了」。 */
  const body = state.update.stage === "error"
    ? updStageHtml()
    : `${updStageHtml()}${updBreakingHtml()}${updNotesHtml()}${updOlderHtml()}`;
  box.innerHTML = `<div class="upd" role="dialog" aria-modal="true" aria-labelledby="upd-title" data-upd-stage="${state.update.stage}">
    ${updHeadHtml()}
    <div class="upd-body">${body}</div>
    <footer class="upd-foot">${updFootHtml()}</footer>
  </div>`;
}

function openUpdateDialog(stage) {
  state.update.stage = stage || "available";
  if (state.update.stage === "available") state.update.progress = 0;
  state.update.open = true;
  $("#update-overlay").classList.add("open");
  $("#update-overlay").setAttribute("aria-hidden", "false");
  renderUpdateDialog();
  /* 打开即把焦点落到主操作上：键盘用户不用先 Tab 一圈才知道「升级」在哪。
     失败态落到「重试」，与「主操作 = 最可能想做的事」同一条判据。 */
  const main = $("#update-overlay").querySelector('[data-act="upd-start"], [data-act="upd-retry"], [data-act="upd-restart"]');
  if (main) main.focus();
}

function closeUpdateDialog() {
  stopUpdateTimer();
  state.update.open = false;
  /* 记住「这版看过了」：自动检查每 30 分钟一轮，不能每次都把说明弹回用户脸上。 */
  state.update.seenVersion = APP_UPDATE.latest;
  $("#update-overlay").classList.remove("open");
  $("#update-overlay").setAttribute("aria-hidden", "true");
  $("#update-overlay").innerHTML = "";
}

function stopUpdateTimer() {
  if (updTimer) { clearInterval(updTimer); updTimer = null; }
}

/* 进度只改两个节点（宽度 + 百分比）：整块重画会把正文滚动位置打回顶部。 */
function paintUpdateProgress() {
  const p = Math.max(0, Math.min(100, Math.round(state.update.progress)));
  const bar = $("#update-overlay .upd-pbar > i");
  const pct = $("#update-overlay [data-upd-pct]");
  if (bar) bar.style.width = p + "%";
  if (pct) pct.textContent = p + "%";
}

function startUpdateDownload() {
  if (updBusy()) return;
  state.update.stage = "downloading";
  state.update.progress = 0;
  renderUpdateDialog();
  stopUpdateTimer();
  /* 原型用定时器模拟 tauri updater 的 chunk 事件；真实实现按 received/total 计算。 */
  updTimer = setInterval(() => {
    state.update.progress = Math.min(100, state.update.progress + 9);
    paintUpdateProgress();
    if (state.update.progress < 100) return;
    stopUpdateTimer();
    state.update.stage = "ready";
    renderUpdateDialog();
    toast("ok", `v${APP_UPDATE.latest} 已下载`, "重启应用后生效");
    const btn = $("#update-overlay").querySelector('[data-act="upd-restart"]');
    if (btn) btn.focus();
  }, 200);
}

/* ── 应用更新入口（数据驱动，见 demo-data.js 的 APP_UPDATE）── */
function checkUpdate() {
  /* `?err=update` 深链：与 gateway / usage 的错误态同一套约定，便于直接复现失败分支。 */
  if (state.errors.update) { openUpdateDialog("error"); return; }
  if (!APP_UPDATE.hasUpdate) {
    toast("ok", `已是最新版本 v${APP_UPDATE.current}`, "上次检查：刚刚");
    return;
  }
  openUpdateDialog("available");
}

/* ══════════════════════ 命令面板 ══════════════════════ */
function paletteItems(q) {
  const items = [];
  DESTINATIONS.forEach((d) => items.push({ grp: "页面", title: d.label, sub: `${scopeLabel(state.scope)} · 跳转`, icon: d.icon, run: () => { state.dest = d.id; state.selected.clear(); renderAll(); } }));
  Object.keys(SCOPES).forEach((id) => {
    const c = currentAccount(id);
    items.push({ grp: "作用域", title: scopeLabel(id), sub: `${c ? "已登录 " + c.name : "未登录"} · ${SCOPES[id].accounts.length} 个账号`, icon: PLATFORMS[SCOPES[id].platform].mark, run: () => switchScope(id) });
  });
  Object.keys(SCOPES).forEach((sid) => {
    SCOPES[sid].accounts.forEach((a) => items.push({
      grp: "账号", title: a.name, sub: `${scopeLabel(sid)} · ${maskEmail(a.email) || a.remark || ""}`, icon: ICON.accounts,
      run: () => { if (sid !== state.scope) switchScope(sid, true); state.dest = "accounts"; state.q = a.name; renderAll(); setTimeout(() => drawerAccount(a.id), 60); },
    }));
  });
  items.push({ grp: "动作", title: "刷新当前作用域", sub: scopeLabel(state.scope), icon: ICON.refresh, run: () => doRefresh() });
  items.push({ grp: "动作", title: "添加账号", sub: scopeLabel(state.scope), icon: ICON.plus, run: () => drawerAdd() });
  items.push({ grp: "动作", title: "一键签到全部账号", sub: `${curScope().accounts.filter((a) => !a.checkedIn).length} 个未签到`, icon: ICON.spark, run: () => batchCheckin() });
  items.push({ grp: "动作", title: "切换明暗主题", sub: "立即生效", icon: ICON.wand, run: () => toggleTheme() });
  items.push({ grp: "动作", title: "打开数据目录", sub: "~/.buddy-switch/", icon: ICON.shield, run: () => toast("ok", "已在文件管理器中打开", "~/.buddy-switch/") });

  const k = q.trim().toLowerCase();
  if (!k) return items;
  return items.filter((i) => (i.title + " " + (i.sub || "") + " " + i.grp).toLowerCase().includes(k));
}

function renderPalette() {
  const items = paletteItems(state.palette.q);
  if (state.palette.cursor >= items.length) state.palette.cursor = Math.max(0, items.length - 1);
  let html = "", lastGrp = "";
  items.forEach((it, i) => {
    if (it.grp !== lastGrp) { html += `<div class="palette-grp">${it.grp}</div>`; lastGrp = it.grp; }
    html += `<button class="palette-item${i === state.palette.cursor ? " cursor" : ""}" data-act="pal-run|${i}">
      <span class="ico">${it.icon}</span><span class="t"><b>${esc(it.title)}</b><span>${esc(it.sub || "")}</span></span></button>`;
  });
  $("#palette-list").innerHTML = html || `<div class="empty" style="padding:36px 16px"><div class="ico">${ICON.search}</div><b>没有匹配项</b><p>换个关键词试试。</p></div>`;
  window.__palItems = items;
}

function openPalette() {
  state.palette.open = true; state.palette.q = ""; state.palette.cursor = 0;
  $("#palette-overlay").classList.add("open");
  $("#palette-overlay").setAttribute("aria-hidden", "false");
  $("#palette-input").value = "";
  renderPalette();
  setTimeout(() => $("#palette-input").focus(), 30);
}
function closePalette() {
  state.palette.open = false;
  $("#palette-overlay").classList.remove("open");
  $("#palette-overlay").setAttribute("aria-hidden", "true");
}

/* ══════════════════════ 作用域菜单 ══════════════════════ */
function renderScopeMenu() {
  const ids = Object.keys(SCOPES);
  $("#scope-menu").innerHTML = `<div class="grp">作用域（平台 × 区域）</div>` + ids.map((id, i) => {
    const s = SCOPES[id], p = PLATFORMS[s.platform], c = currentAccount(id);
    return `<button class="scope-item${id === state.scope ? " on" : ""}" data-act="scope|${id}">
      <span class="scope-mark" style="background:color-mix(in oklab, ${p.accent} 18%, transparent);color:${p.accent}">${p.mark}</span>
      <span class="t"><b>${scopeLabel(id)}</b>
        <span>${c ? "已登录 " + esc(c.name) : s.installed ? "未登录" : "未检测到"} · ${s.accounts.length} 个账号${expiredCount(s) ? ` · ${expiredCount(s)} 个需处理` : ""}</span></span>
      <span class="run-pill${s.running ? "" : " off"}" style="height:18px;padding:0 7px"><i class="dot"></i></span>
      <span class="key">⌘${i + 1}</span>
    </button>`;
  }).join("") + `<div class="hint-row">作用域决定「我现在管的是哪一套账号」。切换后，侧栏、内容区、统计口径全部跟随。</div>`;
}

/* ══════════════════════ 行为 ══════════════════════ */
function switchScope(id, silent) {
  if (id === state.scope) { closeScopeMenu(); return; }
  closeScopeMenu();
  state.scope = id;
  state.selected.clear();
  state.q = "";
  /* 菜单跟着账号走：不清掉的话，切到另一个作用域时同 id 的账号会被误挂上菜单。 */
  closeAcctMenu();
  state.checkinResult = null;
  state.loading = true;
  renderAll();
  setTimeout(() => {
    state.loading = false;
    renderAll();
    if (!silent) toast("ok", "已切换到 " + scopeLabel(id), `${curScope().accounts.length} 个账号 · ${curScope().running ? "客户端运行中" : "客户端未运行"}`);
  }, 260);
}
function openScopeMenu() {
  renderScopeMenu();
  const r = $("#scope-trigger").getBoundingClientRect();
  const m = $("#scope-menu");
  m.style.left = r.left + "px";
  m.style.top = r.bottom + 6 + "px";
  m.classList.add("open"); m.setAttribute("aria-hidden", "false");
  $("#scope-trigger").setAttribute("aria-expanded", "true");
}
function closeScopeMenu() {
  $("#scope-menu").classList.remove("open");
  $("#scope-menu").setAttribute("aria-hidden", "true");
  $("#scope-trigger").setAttribute("aria-expanded", "false");
}
function toggleTheme() {
  state.theme = state.theme === "dark" ? "light" : "dark";
  document.documentElement.classList.toggle("dark", state.theme === "dark");
  toast("ok", state.theme === "dark" ? "已切换到深色主题" : "已切换到浅色主题");
}
function doRefresh() {
  /* 顶栏 ⟳ 的进行中反馈：图标旋转 + 禁用（点击必有动效，不再「点了没反应」）。 */
  const btn = $("#refresh-btn");
  if (btn) { btn.classList.add("spinning"); btn.disabled = true; }
  state.loading = true; renderContent();
  setTimeout(() => {
    state.loading = false; renderContent();
    if (btn) { btn.classList.remove("spinning"); btn.disabled = false; }
    toast("ok", "已刷新", scopeLabel(state.scope) + " 的账号与积分状态已更新");
  }, 500);
}
/* ── 卡片「更多」菜单：四类数据**各自**刷新（原型 · 待确认）──────────────
   为什么拆成四个而不是一个「刷新全部」：
     · 积分要走**上游**接口（慢、可能限流）；
     · 今日 Token 读**本机**客户端会话日志 + 网关日志（快，离线也能读）；
     · 签到状态读本机签到记录（快）；
     · 凭据状态读本机 JWT（最快）。
   合成一个动作时，慢的那项把另外三项一起拖住，而且**失败原因分不清**——
   凭据过期的账号其实只有「凭据状态」能刷出真值，另外三项必然失败。
   拆开之后，用户能只刷手上要用的那一项，也能从失败项里看出到底卡在哪一层。 */
const REFRESH_KINDS = [
  { kind: "credits", label: "刷新积分", icon: "refresh", ms: 650, hint: "向上游查询剩余积分与到期时间（最慢，可能被限流）" },
  { kind: "tokens", label: "刷新今日 Token", icon: "usage", ms: 420, hint: "重读本机客户端会话日志 + 网关日志（本地，快）" },
  { kind: "checkin", label: "刷新签到状态", icon: "spark", ms: 260, hint: "重读今天这个账号的签到结果（本地）" },
  { kind: "credential", label: "刷新凭据状态", icon: "key", ms: 180, hint: "重读本机登录凭据的有效期（凭据过期时只有这项还能用）" },
];

/** 某一项刷新的结果。**必须如实**：凭据失效时积分 / Token / 签到读不到，就是失败。 */
function refreshOutcome(a, kind) {
  const dead = a.expired ? "凭据已过期" : a.needsRelogin ? "需要重新登录" : null;
  /* 凭据状态**永远能刷出答案**：它读的是本机凭据，过期与否本身就是要读的结果 ——
     这也是「拆成四项」最直接的好处：账号挂了，至少还能知道挂在哪一步。 */
  if (kind === "credential") {
    return {
      ok: true,
      desc: a.expired ? "凭据已过期 —— 需要重新登录" : a.needsRelogin ? "会话已失效 —— 需要重新登录" : "凭据有效，客户端可正常使用",
    };
  }
  if (dead) {
    const what = kind === "credits" ? "积分" : kind === "tokens" ? "今日 Token" : "签到状态";
    return { ok: false, desc: `${dead}，读不到${what} —— 先在客户端重新登录` };
  }
  if (kind === "credits") {
    return {
      ok: true,
      desc: `积分 ${fmt(a.credits)}/${fmt(a.creditsTotal)}${a.expiring ? ` · ${fmt(a.expiring)} 积分 ${a.expireInDays} 天内到期` : ""}`,
    };
  }
  if (kind === "tokens") return { ok: true, desc: `今日 ${fmtTok(a.todayTokens)} · 累计 ${fmtTok(a.totalTokens)}` };
  return { ok: true, desc: a.checkedIn ? "今日已签到" : "今日未签到" };
}

function acctMenuInner(a) {
  const keyOf = (kind) => `${a.id}:${kind}`;
  return `<div class="am-head">${esc(a.name)}</div>
    <div class="am-grp">刷新</div>
    ${REFRESH_KINDS.map((k) => {
      const busy = state.refreshing.has(keyOf(k.kind));
      const done = state.refreshDone.has(keyOf(k.kind));
      return `<button class="am-item${done ? " done" : ""}" role="menuitem"
        data-act="acct-refresh|${a.id}|${k.kind}" title="${esc(k.hint)}"${busy ? " disabled" : ""}>
        <span class="am-ico">${busy ? '<span class="spinner"></span>' : done ? ICON.check : ICON[k.icon]}</span>
        <span class="am-lab">${k.label}</span>
        <span class="am-state">${busy ? "刷新中…" : done ? "已更新" : ""}</span>
      </button>`;
    }).join("")}
    <div class="am-sep"></div>
    <div class="am-grp">账号</div>
    ${a.checkedIn
      ? `<button class="am-item" disabled title="今天已经签过，无需重复"><span class="am-ico">${ICON.check}</span><span class="am-lab">今日已签到</span></button>`
      : (a.expired || a.needsRelogin)
        ? `<button class="am-item" disabled title="凭据已失效，先重新登录才能签到"><span class="am-ico">${ICON.spark}</span><span class="am-lab">手动签到</span><span class="am-state">需先登录</span></button>`
        : `<button class="am-item" data-act="act|checkin|${a.id}"><span class="am-ico">${ICON.spark}</span><span class="am-lab">手动签到</span></button>`}
    <button class="am-item" data-act="act|export|${a.id}"><span class="am-ico">${ICON.download}</span><span class="am-lab">导出账号</span></button>
    <div class="am-sep"></div>
    <button class="am-item danger" data-act="act|delete|${a.id}"><span class="am-ico">${ICON.trash}</span><span class="am-lab">删除账号</span></button>
    <div class="am-note">备注与登录态在账号详情里（点卡片打开）</div>`;
}

/* 菜单挂在 **body** 上（`position: fixed`），不塞进卡片：
   卡片高度只有 ~170px，四项刷新的菜单比它还高 —— 塞进去必然溢出卡片；
   而 `#content` 是滚动容器，`overflow:auto` 会把溢出的菜单裁掉。
   真实实现里这就是 Radix `DropdownMenu` 的行为（Portal + 视口定位），原型照它来。 */
function renderAcctMenu() {
  const a = state.acctMenu ? curScope().accounts.find((x) => x.id === state.acctMenu) : null;
  let node = document.getElementById("acct-menu");
  if (!a) { if (node) node.remove(); return; }
  if (!node) {
    node = document.createElement("div");
    node.id = "acct-menu";
    node.className = "acct-menu";
    node.dataset.act = "noop";      /* 点菜单空白处不冒泡成「打开账号抽屉」 */
    node.setAttribute("role", "menu");
    document.body.appendChild(node);
  }
  node.setAttribute("aria-label", `刷新 ${a.name} 的数据`);
  node.innerHTML = acctMenuInner(a);
}

/**
 * 打开 / 收起「更多」。位置在**打开时算一次**：
 * 之后菜单内容会随「刷新中 → 已更新」重绘，重绘不能让它跳位。
 */
function toggleAcctMenu(id, btn) {
  if (state.acctMenu === id) { closeAcctMenu(); return; }
  state.acctMenu = id;
  renderAcctMenu();
  const node = document.getElementById("acct-menu");
  if (node && btn) {
    const r = btn.getBoundingClientRect();
    const w = node.offsetWidth || 250;
    const h = node.offsetHeight || 210;
    const left = Math.max(12, Math.min(r.right - w, window.innerWidth - w - 12));
    /* 默认从按钮下面展开；下方放不下就翻到上面（与桌面下拉菜单同一套翻转规则）。 */
    let top = r.bottom + 6;
    if (top + h > window.innerHeight - 12) top = Math.max(12, r.top - h - 6);
    node.style.left = `${Math.round(left)}px`;
    node.style.top = `${Math.round(top)}px`;
  }
  renderAccountList();
}

function closeAcctMenu() {
  if (!state.acctMenu) return;
  state.acctMenu = null;
  renderAcctMenu();     /* 传不了 id ⇒ 内部直接移除节点 */
  renderAccountList();
}

/**
 * 新建分组并把账号归入其中。
 *
 * 分组是**自由文本**（后端 `groupName`），所以「新建」= 记到本作用域的分组清单里
 * （下拉下次能选到）+ 立刻归入。空名 / 同名不新建，直接归入已有那个。
 */
function createGroup(id) {
  const a = curScope().accounts.find((x) => x.id === id);
  const input = $("#group-new-input");
  if (!a || !input) return;
  const name = input.value.trim();
  if (!name) {
    toast("err", "分组名不能为空", "输入一个名字，或点「取消」回到原来的分组");
    input.focus();
    return;
  }
  const exists = scopeGroups().includes(name);
  if (!exists) curScope().groups = [...(curScope().groups || []), name];
  state.groupNew = null;
  a.group = name;
  renderAll();
  syncOpenDrawer(id);
  toast("ok", exists ? `${a.name} 已归入「${name}」` : `已新建分组「${name}」`,
    exists ? "该分组已存在，直接归入" : "分组下拉里下次可以直接选它");
}

/** 改分组：写回账号 + 重绘（抽屉保持当前 Tab，列表上的分组统计同步）。 */
function setAccountGroup(id, group) {
  const a = curScope().accounts.find((x) => x.id === id);
  if (!a || a.group === group) return;
  a.group = group;
  renderAll();
  syncOpenDrawer(id);
  toast("ok", `${a.name} 已归入「${group}」`, "分组只影响本机的整理方式");
}

/** 抽屉正开着这个账号时重绘它（保持当前 Tab）；没开着就什么都不做。 */
function syncOpenDrawer(id) {
  if (state.drawer && state.drawer.kind === "account" && state.drawer.id === id) drawerAccount(id);
}

/**
 * 刷新某个账号的**某一项**数据。
 *
 * 菜单**不关**：刷完一项还想刷另一项是很常见的（比如先看凭据再决定要不要刷积分），
 * 关掉菜单等于让用户重新点开一次；而且进行中/已更新这两个状态只有在菜单里才看得见。
 */
function refreshAccountPart(id, kind) {
  const a = curScope().accounts.find((x) => x.id === id);
  const conf = REFRESH_KINDS.find((k) => k.kind === kind);
  if (!a || !conf) return;
  const key = `${id}:${kind}`;
  if (state.refreshing.has(key)) return;
  state.refreshing.add(key);
  state.refreshDone.delete(key);
  renderAccountList();
  renderAcctMenu();
  /* 抽屉开着同一个账号时也要重绘：指标块上的 ⟳ 就在抽屉里，不重绘就没有「进行中」。 */
  syncOpenDrawer(id);
  setTimeout(() => {
    state.refreshing.delete(key);
    const outcome = refreshOutcome(a, kind);
    if (outcome.ok) {
      state.refreshDone.add(key);
      /* 「已更新」只停留 900ms：它是**刚刚发生了什么**的反馈，不是常驻状态。 */
      setTimeout(() => { state.refreshDone.delete(key); renderAccountList(); renderAcctMenu(); }, 900);
      toast("ok", `${a.name} · ${conf.label}完成`, outcome.desc);
    } else {
      toast("err", `${a.name} · ${conf.label}失败`, outcome.desc,
        { label: "打开账号", run: () => drawerAccount(id) });
    }
    renderAccountList();
    renderAcctMenu();
    syncOpenDrawer(id);
  }, conf.ms);
}
/* 批量签到：目标 = 已勾选账号（若有），否则整个作用域。
   批量操作必然可能部分失败——凭据过期 / 需重新登录是结构性失败，
   再补 1 个瞬时失败，凑成「8 个里 3 个失败」。失败项保留 checkedIn=false，
   用汇总条呈现，可「重试失败项」。 */
function checkinTargets() {
  const s = curScope();
  return state.selected.size ? s.accounts.filter((a) => state.selected.has(a.id)) : s.accounts.slice();
}
function runBatchCheckin(targets) {
  const structural = targets.filter((a) => a.expired || a.needsRelogin);
  const others = targets.filter((a) => !(a.expired || a.needsRelogin));
  const wantFail = Math.min(3, targets.length);
  const failIds = new Set(structural.map((a) => a.id));
  const pad = [...others.filter((a) => !a.checkedIn), ...others.filter((a) => a.checkedIn)];
  for (const a of pad) { if (failIds.size >= wantFail) break; failIds.add(a.id); }
  targets.forEach((a) => { a.checkedIn = !failIds.has(a.id); });
  const fails = targets.filter((a) => failIds.has(a.id));
  state.checkinResult = { ok: targets.length - fails.length, failIds: fails.map((a) => a.id) };
  return fails.length;
}
function batchCheckin() {
  const targets = checkinTargets();
  if (!targets.length) { toast("ok", "没有可签到的账号"); return; }
  const fails = runBatchCheckin(targets);
  renderAll();
  if (fails) toast("err", `签到完成：成功 ${targets.length - fails} · 失败 ${fails}`, "失败项保留未签到状态，可在汇总条重试");
  else toast("ok", `已为 ${targets.length} 个账号签到`, "积分将在稍后到账");
}

/* ══════════════════════ 事件 ══════════════════════ */
document.addEventListener("click", (e) => {
  const trigger = e.target.closest("#scope-trigger");
  if (trigger) { $("#scope-menu").classList.contains("open") ? closeScopeMenu() : openScopeMenu(); return; }
  if (!e.target.closest("#scope-menu")) closeScopeMenu();
  /* 卡片「更多」菜单：点菜单外的地方收起（点菜单本身、或点另一张卡的「更多」不算外面 ——
     后者由 `acct-more` 分支接管，直接换到那张卡的菜单）。 */
  if (state.acctMenu && !e.target.closest(".acct-menu") && !e.target.closest('[data-act^="acct-more|"]')) closeAcctMenu();

  const node = e.target.closest("[data-act]");
  /* 自定义时间弹层：点弹层外部收起（触发器与弹层内部除外），与本次动作同帧生效。 */
  const willCloseRange = state.rangeOpen && !e.target.closest(".range-pop") && !(node && node.dataset.act === "range-custom-open");
  if (willCloseRange) state.rangeOpen = false;
  if (!node) { if (willCloseRange) renderContent(); return; }
  const [act, a1, a2] = node.dataset.act.split("|");
  const stop = () => { e.preventDefault(); e.stopPropagation(); };

  switch (act) {
    case "nav": stop(); state.dest = a1; state.selected.clear(); closeAcctMenu(); renderAll(); break;
    case "scope": stop(); switchScope(a1); break;
    case "view": stop(); state.view = a1; renderAccountList(); break;
    case "range": stop();
      if (a1 === "today") { state.rangeMode = "today"; state.range = 1; }
      else { state.rangeMode = "preset"; state.range = Number(a1); }
      state.rangeOpen = false;
      renderContent(); break;
    /* 自定义时间：先开弹层选日期，再「应用」——不选就不改当前区间。 */
    case "range-custom-open": stop(); state.rangeOpen = !state.rangeOpen; renderContent(); break;
    case "range-custom-cancel": stop(); state.rangeOpen = false; renderContent(); break;
    case "range-custom-apply": {
      stop();
      const fromEl = $("#range-from"), toEl = $("#range-to");
      const from = fromEl ? fromEl.value : "", to = toEl ? toEl.value : "";
      if (!from || !to) { toast("err", "请选择起止日期"); break; }
      state.customFrom = from <= to ? from : to;
      state.customTo = from <= to ? to : from;
      state.rangeMode = "custom";
      const i = dayIndexOf(mdOfIso(state.customFrom)), j = dayIndexOf(mdOfIso(state.customTo));
      state.range = i >= 0 && j >= 0 ? Math.abs(j - i) + 1 : state.range;
      state.rangeOpen = false;
      renderContent();
      break;
    }
    case "merge": stop(); state.merge = a1 === "1"; renderContent(); break;
    case "usage-tab": stop(); state.usageTab = a1; renderContent(); break;
    case "gw-sub": stop(); state.gwSub = a1; renderContent(); break;
    /* Trae 上下文策略（Max Mode）：切换即时反映到清单的「上下文」列。
       演示「一个模型一条记录」—— 不存在 -max 影子 id。 */
    case "max-mode": {
      stop();
      state.maxMode = a1 === "off" ? "off" : "always";
      toast(state.maxMode === "off" ? "info" : "ok",
        state.maxMode === "off" ? "已回到上游默认窗口" : "已开启 1M 上下文",
        state.maxMode === "off"
          ? "支持 Max Mode 的模型不再注入 is_max_mode，清单声明回到上游默认"
          : "支持 Max Mode 的模型始终注入 is_max_mode，清单直接声明 1M");
      renderContent();
      break;
    }
    case "gw-api": stop(); state.gwApi = a1; state.gwTool = ACCESS_TOOLS[a1][0][0]; renderContent(); break;
    case "gw-tool": stop(); state.gwTool = a1; renderContent(); break;
    /* 禁用 / 启用模型（§3.18）：禁用是**用户偏好**，与「上游可用性」正交。
       禁用后两个出口同时生效：/v1/models 不列出 + 直接调用被拒绝并说明原因。 */
    case "model-disable": {
      stop();
      const m = curGateway().models.find((x) => x.name === a1 && x.scope === a2);
      if (m) { m.disabled = true; toast("ok", `已禁用 ${a1}`, "它不再出现在 /v1/models，直接调用会被拒绝并说明原因"); }
      renderContent();
      break;
    }
    case "model-enable": {
      stop();
      const m = curGateway().models.find((x) => x.name === a1 && x.scope === a2);
      if (m) { m.disabled = false; toast("ok", `已启用 ${a1}`, "已重新出现在 /v1/models"); }
      renderContent();
      break;
    }
    /* 重新识别模型清单（§3.17）：先进入「识别中」，再出结果 —— 演示非阻塞的探测。 */
    case "gw-scan": {
      stop();
      if (state.gwScanning) break;
      state.gwScanning = true;
      renderContent();
      setTimeout(() => {
        state.gwScanning = false;
        toast("ok", "已识别 6 个模型", "来源：实时 · 刚刚");
        renderContent();
      }, 900);
      break;
    }
    case "pin": stop();
      if (a1) {
        /* 已排除的账号不能成为上游（UI 已禁用按钮，这里再兜一层）。 */
        const row = (GATEWAY[state.scope].pool || []).find((x) => x.account === a1);
        if (row && row.state === "已排除") { toast("err", `${a1} 已排除，不能指定`, "请先恢复该账号的积分与权重"); break; }
        state.pins[state.scope] = a1; toast("ok", `已指定 ${a1}`, "该账号将优先承接网关请求");
      }
      else { state.pins[state.scope] = null; toast("ok", "已取消指定", "恢复按权重自动择优"); }
      renderContent(); break;
    case "set-sub": stop(); state.setSub = a1; renderContent(); break;
    /* ── 会话工作台（§3.15） ── */
    case "sess-client": stop(); state.sessionClient = a1; state.sessionFilter = "all"; renderContent(); break;
    case "sess-filter": stop(); state.sessionFilter = a1; renderContent(); break;
    case "sess-sort": stop(); state.sessionSort = a1; renderContent(); break;
    case "sess-open": stop(); drawerSession(a1); break;
    case "sess-sync": stop(); closeDrawer(); toast("ok", "已同步 1 个会话组", "新增内容已追加到落后的副本（加法，不删除原有内容）"); break;
    case "sess-unify": stop(); closeDrawer(); toast("ok", "已统一到其他副本", "保留了你选择的那一份"); break;
    case "sess-unlink": stop(); closeDrawer(); toast("ok", "已解除关联", "各副本从此独立，不再互相同步"); break;
    /* ── ★ P14 S4a · 插件侧「复制会话」（**不是**副本组，没有同步/统一） ── */
    case "ext-sess": {
      stop();
      if (a1 === "open") { drawerExtSessionCopy(a2); break; }
      if (a1 === "all") {
        const view = EXT_SESSIONS[state.extVariant];
        state.extPicked = state.extPicked.size === view.sessions.length
          ? new Set()
          : new Set(view.sessions.map((x) => x.id));
        renderExtDrawer();
        break;
      }
      if (a1 === "pick") {
        if (state.extPicked.has(a2)) state.extPicked.delete(a2);
        else state.extPicked.add(a2);
        renderExtDrawer();
        break;
      }
      if (a1 === "target") { state.extTarget = a2; renderExtDrawer(); break; }
      if (a1 === "copy") {
        const n = state.extPicked.size;
        closeDrawer();
        toast("ok", `已复制 ${n} 个会话`,
          "副本是**新会话**（新 id）—— 目标账号原有的会话一个字都没动");
        break;
      }
      break;
    }
    /* ── 全局备份与恢复（§3.14） ── */
    case "bk-part": {
      stop();
      const p = backupParts()[a1];
      if (p) { p[a2] = !p[a2]; renderContent(); }
      break;
    }
    case "bk-scope": {
      stop();
      const p = backupParts()[a1];
      if (p) {
        const all = p.accounts && p.config && p.gateway;
        p.accounts = p.config = p.gateway = !all;
        renderContent();
      }
      break;
    }
    case "bk-creds": stop(); state.backup.creds = !state.backup.creds; renderContent(); break;
    case "bk-export": {
      stop();
      const est = backupEstimate();
      if (!est.scopes) { toast("err", "没有可导出的内容", "至少勾选一个作用域"); break; }
      toast("ok", "已导出备份",
        `${est.accounts} 个账号 · ${est.scopes} 个作用域 · ${state.backup.creds ? "含凭据" : "不含凭据"}`,
        null, "buddy-switch-backup-2026-10-01.json");
      break;
    }
    case "bk-import": stop(); state.backup.preview = true; state.backup.result = null; renderContent(); break;
    case "bk-conflict": stop(); state.backup.conflict = a1; renderContent(); break;
    case "bk-cancel": stop(); state.backup.preview = false; renderContent(); break;
    case "bk-run": {
      stop();
      const agg = Object.values(BACKUP_FILE_SAMPLE.scopes).reduce(
        (t, s) => ({ added: t.added + s.added, replaced: t.replaced + s.replaced, skipped: t.skipped + s.skipped }),
        { added: 0, replaced: 0, skipped: 0 });
      state.backup.preview = false;
      state.backup.result = { ...agg, fail: 1 };   // 演示「部分失败态」（§3.6）
      renderContent();
      break;
    }
    case "bk-retry": stop(); toast("ok", "已重试 1 项", "导入完成"); break;
    case "bk-reset": stop(); state.backup.result = null; renderContent(); break;
    case "clear-q": stop();
      state.q = "";
      { const i = $("#acct-q"); if (i) i.value = ""; }
      renderAccountList(); break;
    case "refresh": stop(); doRefresh(); break;
    /* 请求代理（§3.19 / P12）：默认关；开启/关闭都只改本地开关，**不碰系统解析**。 */
    case "proxy-toggle": {
      stop();
      state.proxyOn = !state.proxyOn;
      if (state.proxyOn) {
        toast("ok", "请求代理已开启", `拦截方式 ${PROXY.modeLabel}；被代理域名的流量会经过本机网关`);
      } else {
        toast("ok", "请求代理已关闭", "客户端配置已还原，无残留");
      }
      renderContent();
      break;
    }
    case "proxy-restore": {
      stop();
      state.proxyOn = false;
      toast("ok", "已一键还原", "代理设置已撤、CA 未安装、未改 hosts —— 系统无残留");
      renderContent();
      break;
    }
    /* 两个入口（侧栏版本行 / 应用设置抽屉）都收敛到 checkUpdate()。先收抽屉：
       否则弹窗关掉后底下还压着一层抽屉，用户会以为「关错了东西」。 */
    case "check-update": stop(); closeDrawer(); checkUpdate(); break;
    /* 桌面悬浮窗（§3.16 / D21）：**只读** —— 点击只做"回到原会话"，没有任何写操作。 */
    case "cp-toggle": stop(); state.companionOpen = !state.companionOpen; renderCompanion(); break;
    case "cp-close": stop(); state.companionOpen = false; renderCompanion(); break;
    case "cp-jump": {
      stop();
      const c = COMPANION_SOURCES.find((x) => x.id === a1);
      if (c) toast("ok", `已回到 ${c.source} 的会话`, c.title);
      break;
    }
    case "setting-action": {
      stop();
      /* 设置页的危险按钮：重置设备标识不可逆 → 先确认；导出只给反馈。 */
      if (a1 === "重置设备标识") {
        openConfirm({
          title: "重置设备标识？",
          body: `将重新生成设备 ID，可能影响 <b>${esc(scopeLabel(state.scope))}</b> 下所有账号的登录态。<br><span class="tiny muted">此操作不可撤销。</span>`,
          okLabel: "重置",
          onOk: () => toast("ok", "设备标识已重置", "受影响账号可能需要重新登录"),
        });
      } else {
        toast("ok", `已开始${a1}`, "诊断包将保存到下载目录（不含凭据）");
      }
      break;
    }
    case "select-all": stop();
      if (a1 === "1") filteredAccounts().forEach((a) => state.selected.add(a.id));
      else state.selected.clear();
      renderAccountList(); break;
    case "acct-select": stop();
      state.selected.has(a1) ? state.selected.delete(a1) : state.selected.add(a1);
      renderAccountList(); break;
    case "acct-open": stop(); drawerAccount(a1); break;
    /* 抽屉 Tab：只切内容区，不动抽屉本身（保留滚动位置与底部动作）。 */
    case "group-create": stop(); createGroup(a1); break;
    case "group-create-cancel": {
      stop();
      state.groupNew = null;
      drawerAccount(a1);
      break;
    }
    case "drawer-tab": {
      stop();
      if (state.drawer && state.drawer.kind === "account") state.drawer.tab = a2;
      drawerAccount(a1);
      break;
    }
    /* 单项刷新：a1 = 账号 id，a2 = 数据种类（credits / tokens / checkin / credential）。 */
    case "acct-refresh": stop(); refreshAccountPart(a1, a2); break;
    case "acct-more": stop(); toggleAcctMenu(a1, node); break;
    case "acct-prog": {
      stop();
      const acc = curScope().accounts.find((x) => x.id === a1);
      const pg = curPlatform().programs.find((x) => x.id === a2);
      /* ⚠️ `planned`（本仓尚未支持）**必须排在 `installed` 之前**判断 ——
         否则会走「未检测到客户端 → 去设置接入」那条引导，而那条路**装好了也没用**。 */
      if (pg.planned) {
        toast("err", `${pg.label}：本应用尚未支持`, "这是计划新增的客户端，装好插件也还不能切换",
          null, "与「本机没装」区分：后者引导去装，前者只能等实现（§3.3 B 表）");
        break;
      }
      if (!pg.installed) {
        /* 真能跳：toast 给一个可点按钮，点了就进「设置 → 权限与诊断」。 */
        toast("err", `${pg.label} 未检测到客户端`, "需要先在「设置 → 权限与诊断」完成接入",
          { label: "去设置", run: () => { state.dest = "settings"; state.setSub = "diagnostics"; state.selected.clear(); renderAll(); } },
          "未检测到的程序位不置灰：可点 + 明确给出下一步，而不是一个点不动的灰按钮");
        break;
      }
      const wasOn = Boolean(acc.programs[a2]);
      if (wasOn) { toast("ok", `${pg.label} 已是 ${acc.name}`, "无需重复切换"); break; }
      /* 模型：一个**程序位**同时只有一个账号；一个**账号**可绑 0..N 个程序位。
         所以只清「同一个程序位」在**其他账号**上的绑定，
         **不动本账号的其他程序位**（把 IDE 切到 X，不该让 X 从 WorkBuddy 登出）。 */
      const before = curScope().accounts.map((o) => ({ id: o.id, on: Boolean(o.programs[a2]) }));
      acc.programs[a2] = true;
      curScope().accounts.forEach((o) => { if (o.id !== acc.id) o.programs[a2] = false; });
      renderAll();
      if (state.drawer && state.drawer.kind === "account") drawerAccount(a1);
      toast("ok", `已把 ${pg.label} 切到 ${acc.name}`, "客户端将自动重启以生效",
        { label: "撤销", run: () => {
          /* 撤销要回到操作前的**全部**绑定（含被清掉的其他账号）。 */
          before.forEach(({ id, on }) => { const o = curScope().accounts.find((x) => x.id === id); if (o) o.programs[a2] = on; });
          renderAll();
          if (state.drawer && state.drawer.kind === "account") drawerAccount(a1);
          toast("ok", "已撤销切换", "程序位已恢复");
        } });
      break;
    }
    case "wizard": stop(); drawerWizard(a1, 1); break;
    case "wiz-prog": stop(); state.drawer.opts.program = a1; drawerWizard(state.drawer.id, state.drawer.step, state.drawer.opts); break;
    case "wiz-src": {
      stop();
      const o = state.drawer.opts;
      o.source = { scope: a1, id: a2 };
      o.treeSel = defaultTreeSel(sourceAccount(o));
      o.openTree = allTreeOpen(sourceAccount(o));
      drawerWizard(state.drawer.id, state.drawer.step, o); break;
    }
    case "wiz-cross": stop(); state.drawer.opts.pickCross = !state.drawer.opts.pickCross; drawerWizard(state.drawer.id, state.drawer.step, state.drawer.opts); break;
    case "wiz-opt": {
      stop();
      const o = state.drawer.opts; o[a1] = !o[a1];
      drawerWizard(state.drawer.id, 2, o); break;
    }
    case "wiz-collapse": {
      stop();
      const o = state.drawer.opts;
      o.openTree.has(a1) ? o.openTree.delete(a1) : o.openTree.add(a1);
      drawerWizard(state.drawer.id, 2, o); break;
    }
    case "wiz-tree": {
      stop();
      const o = state.drawer.opts;
      const src = sourceAccount(o);
      const tree = src ? src.sessionTree : [];
      const keys = [];
      if (a1 === "type") { const ti = Number(a2); tree[ti].dirs.forEach((d, di) => d.sessions.forEach((_, si) => keys.push(`${ti}/${di}/${si}`))); }
      else if (a1 === "dir") { const [ti, di] = a2.split("/").map(Number); tree[ti].dirs[di].sessions.forEach((_, si) => keys.push(`${ti}/${di}/${si}`)); }
      else { keys.push(a2); }
      const allOn = keys.length > 0 && keys.every((k) => o.treeSel.has(k));
      keys.forEach((k) => allOn ? o.treeSel.delete(k) : o.treeSel.add(k));
      drawerWizard(state.drawer.id, 2, o); break;
    }
    case "wiz-tree-all": {
      stop();
      const o = state.drawer.opts;
      const src = sourceAccount(o);
      const tree = src ? src.sessionTree : [];
      if (a1 === "1") tree.forEach((t, ti) => t.dirs.forEach((d, di) => d.sessions.forEach((_, si) => o.treeSel.add(`${ti}/${di}/${si}`))));
      else o.treeSel.clear();
      drawerWizard(state.drawer.id, 2, o); break;
    }
    case "wiz-next": stop(); drawerWizard(state.drawer.id, state.drawer.step + 1, state.drawer.opts); break;
    case "wiz-back": stop(); drawerWizard(state.drawer.id, state.drawer.step - 1, state.drawer.opts); break;
    case "wiz-run": {
      stop();
      const a = curScope().accounts.find((x) => x.id === state.drawer.id);
      const o = state.drawer.opts, p = curPlatform();
      const srcAcc = sourceAccount(o);
      const stats = treeStats(srcAcc ? srcAcc.sessionTree : [], o.treeSel);
      p.programs.forEach((x) => { a.programs[x.id] = x.id === o.program; });
      closeDrawer(); renderAll();
      const parts = [];
      if (o.sessions) parts.push(`已复制 ${stats.selected} 个会话 / ${stats.dirsSelected} 个目录`);
      if (o.memory) parts.push("记忆已迁移");
      if (o.connectors) parts.push("连接器已同步");
      toast("ok", `已把 ${p.programs.find((x) => x.id === o.program).label} 切到 ${a.name}`,
        `来源 ${srcAcc ? srcAcc.name : "—"} · ${parts.join("、") || "未迁移额外内容"}`);
      break;
    }
    case "add-account": stop(); drawerAdd(); break;
    case "add-way": {
      stop();
      const label = { oauth: "OAuth 登录", local: "从本机导入", backup: "导入备份" }[a1];
      closeDrawer();
      toast("ok", `已开始「${label}」`, "这里会打开对应的流程抽屉");
      break;
    }
    case "act": {
      stop();
      /* 菜单里的动作点完就收起（刷新四项除外 —— 它们要在菜单里显示进行中/已更新）。 */
      closeAcctMenu();
      if (a1 === "delete") {
        const a = curScope().accounts.find((x) => x.id === a2);
        if (!a) break;
        /* 不可逆操作：先弹确认 Dialog，确认后才删。 */
        openConfirm({
          title: "删除账号？",
          body: `即将删除 <b>${esc(a.name)}</b>${a.email ? `（${esc(maskEmail(a.email))}）` : ""}。<br><span class="tiny muted">仅删除本地记录，不影响服务端账号。</span>`,
          okLabel: "删除",
          onOk: () => {
            curScope().accounts = curScope().accounts.filter((x) => x.id !== a2);
            closeDrawer(); renderAll();
            toast("ok", `已删除 ${a.name}`, "删除只影响本地记录", { label: "撤销", run: () => { curScope().accounts.push(a); renderAll(); toast("ok", "已恢复 " + a.name); } });
          },
        });
      } else if (a1 === "checkin") {
        const a = curScope().accounts.find((x) => x.id === a2);
        if (!a) break;
        a.checkedIn = true; renderAll();
        /* 只有抽屉正开着这个账号时才重绘 —— 从卡片菜单签到时**不该**顺手打开抽屉。 */
        if (state.drawer && state.drawer.kind === "account" && state.drawer.id === a2) drawerAccount(a2);
        toast("ok", `${a.name} 签到成功`, "获得 100 积分");
      } else if (a1 === "refresh-token") {
        toast("ok", "Token 已刷新", "凭据有效期已延长");
      } else if (a1 === "remark") {
        drawerAccount(a2, true); /* 真进编辑态：备注行变输入框 + 保存/取消 */
      } else if (a1 === "remark-save") {
        const a = curScope().accounts.find((x) => x.id === a2);
        const inp = $("#remark-input");
        if (a && inp) { a.remark = inp.value.trim(); renderAll(); drawerAccount(a2, false); toast("ok", "备注已保存", a.remark || "（已清空）"); }
      } else if (a1 === "remark-cancel") {
        drawerAccount(a2, false);
      } else if (a1 === "export") {
        toast("ok", "已导出账号", "保存为 JSON 备份文件");
      }
      break;
    }
    case "batch-clear": stop(); state.selected.clear(); renderContent(); break;
    case "batch-refresh": stop(); toast("ok", `已刷新 ${state.selected.size} 个账号的 Token`); break;
    case "batch-checkin": stop(); batchCheckin(); break;
    case "batch-export": stop(); toast("ok", `已导出 ${state.selected.size} 个账号`, "保存为 JSON 备份文件"); break;
    case "batch-delete": {
      stop();
      const n = state.selected.size;
      const removed = curScope().accounts.filter((a) => state.selected.has(a.id));
      curScope().accounts = curScope().accounts.filter((a) => !state.selected.has(a.id));
      state.selected.clear(); renderAll();
      toast("ok", `已删除 ${n} 个账号`, "删除只影响本地记录", { label: "撤销", run: () => { curScope().accounts.push(...removed); renderAll(); toast("ok", "已恢复 " + n + " 个账号"); } });
      break;
    }
    case "toggle": stop(); node.classList.toggle("on");
      toast("ok", `${node.classList.contains("on") ? "已开启" : "已关闭"}：${a1}`); break;
    /* ── 随机抖动（签到执行时刻）──
       必须**重渲染**而不是只翻 CSS 类：预览区的三次时刻要跟着变，
       否则评审点完开关看到的仍是准点数字，会以为功能没生效。 */
    case "jitter-toggle": {
      stop();
      state.jitter.on = !state.jitter.on;
      renderContent();
      toast(
        "ok",
        state.jitter.on ? `已开启随机抖动（±${state.jitter.minutes} 分钟）` : "已关闭随机抖动",
        state.jitter.on ? "每天在设定时刻前后随机偏移；同一天内恒定，可复现" : "恢复为严格准点触发",
      );
      break;
    }
    case "jitter-range": {
      stop();
      state.jitter.minutes = Number(a1);
      renderContent();
      break;
    }
    case "copy": stop();
      if (navigator.clipboard) navigator.clipboard.writeText(a1).catch(() => {});
      toast("ok", "已复制到剪贴板"); break;
    /* ── 密钥三态动作（吊销 / 恢复 / 删除）──
       ★ 三者都改 `curGateway().keys` 并重渲染：原型要让评审**点得动** —— 一张静态表
       看不出「恢复后回到启用态」。真实组件同样在每个动作后重载列表。 */
    case "revoke": {
      stop();
      const key = curGateway().keys.find((x) => x.id === a1);
      if (!key) break;
      openConfirm({
        title: "吊销 API Key",
        body: `吊销后「<b>${esc(key.name)}</b>」立即失效（401）。<br><span class="tiny muted">密钥仍留在列表里，可随时「恢复」；明文未变，客户端无需改配置。</span>`,
        okLabel: "吊销",
        onOk: () => { key.revoked = true; renderContent(); toast("ok", `已吊销「${key.name}」`, "使用该密钥的请求立即失败（401）；行内现在有「恢复」与「删除」两个出口"); },
      });
      break;
    }
    case "key-restore": {
      stop();
      const key = curGateway().keys.find((x) => x.id === a1);
      if (!key) break;
      openConfirm({
        title: "恢复 API Key",
        body: `恢复后「<b>${esc(key.name)}</b>」立即重新可用。<br><span class="tiny muted">库里存的是<b>同一把</b> Key（明文未变），客户端无需改配置。</span>`,
        okLabel: "恢复",
        danger: false,
        onOk: () => { key.revoked = false; renderContent(); toast("ok", `已恢复「${key.name}」`, "已回到启用态；明文与吊销前完全相同"); },
      });
      break;
    }
    case "key-delete": {
      stop();
      const keys = curGateway().keys;
      const key = keys.find((x) => x.id === a1);
      if (!key) break;
      openConfirm({
        title: "删除 API Key",
        body: `确定删除已吊销的「<b>${esc(key.name)}</b>」？<br><span class="tiny muted">此操作<b>不可撤销</b>（与可逆的「恢复」不同）；用该密钥的客户端需重新配置。</span>`,
        okLabel: "删除",
        onOk: () => { curGateway().keys = keys.filter((x) => x.id !== a1); renderContent(); toast("ok", `已删除「${key.name}」`, "记录已移除；此操作不可撤销"); },
      });
      break;
    }
    case "new-key": stop(); toast("ok", "已创建新密钥", "明文只显示这一次：bs_live_9f3a7d21c4e88b0f"); break;
    case "pal-run": {
      stop();
      const it = (window.__palItems || [])[Number(a1)];
      if (it) { closePalette(); it.run(); }
      break;
    }
    case "toast-action": {
      stop();
      const fn = pendingActions[a1];
      if (fn) fn();
      const el = document.getElementById(a1); if (el) el.remove();
      break;
    }
    case "noop": stop(); break;

    /* ── 错误态的可点入口与重试（P1-1）── */
    case "gw-verify": stop();
      state.errors.gateway = true; renderContent();
      toast("err", "上游凭据校验失败", "账号池中的上游凭据无法解密，网关暂时无法转发请求");
      break;
    case "usage-refresh-credits": stop();
      state.errors.usage = true; renderContent();
      toast("err", "积分刷新失败", "无法从上游读取积分余额");
      break;
    case "retry": stop();
      state.errors[a1] = false; renderContent();
      toast("ok", "已恢复", a1 === "gateway" ? "上游凭据已重新校验通过" : "积分余额已刷新");
      break;

    /* ── 权限错误升级引导（P1-3）── */
    case "perm-guide": stop(); drawerPermGuide(a1); break;
    case "perm-retry": {
      stop();
      /* 渲染端是 data-act="perm-retry|${id}"（只带一个参数）⇒ 账号 id 在 a1，不是 a2。 */
      const a = curScope().accounts.find((x) => x.id === a1);
      const p = curPlatform();
      const pg = p.programs.find((x) => x.installed);
      if (a && pg) {
        const before = curScope().accounts.map((o) => ({ id: o.id, on: Boolean(o.programs[pg.id]) }));
        a.programs[pg.id] = true;
        curScope().accounts.forEach((o) => { if (o.id !== a.id) o.programs[pg.id] = false; });
        closeDrawer(); renderAll();
        toast("ok", `已把 ${pg.label} 切到 ${a.name}`, "管理员权限已就绪，凭据写入成功",
          { label: "撤销", run: () => { before.forEach(({ id, on }) => { const o = curScope().accounts.find((x) => x.id === id); if (o) o.programs[pg.id] = on; }); renderAll(); toast("ok", "已撤销切换", "程序位已恢复"); } });
      } else { closeDrawer(); renderAll(); }
      break;
    }

    /* ── 部分失败汇总（P1-2）── */
    case "retry-checkin": {
      stop();
      const prev = state.checkinResult || { ok: 0, failIds: [] };
      const targets = curScope().accounts.filter((a) => prev.failIds.includes(a.id));
      if (!targets.length) { state.checkinResult = null; renderAll(); break; }
      const stillFail = targets.filter((a) => a.expired || a.needsRelogin);
      const recovered = targets.filter((a) => !(a.expired || a.needsRelogin));
      recovered.forEach((a) => { a.checkedIn = true; });
      state.checkinResult = { ok: prev.ok + recovered.length, failIds: stillFail.map((a) => a.id) };
      renderAll();
      if (stillFail.length) toast("err", `仍有 ${stillFail.length} 个账号失败`, "凭据过期或需重新登录的账号需先处理");
      else toast("ok", "重试成功", "失败项已全部签到");
      break;
    }
    case "dismiss-result": stop(); state.checkinResult = null; renderAll(); break;

    /* ── 抽屉 / 更新 ── */
    case "close-drawer": stop(); closeDrawer(); break;
    case "packs-toggle": stop();
      { const a = curScope().accounts.find((x) => x.id === a1); if (a) { a._packsOpen = !a._packsOpen; drawerAccount(a1); } }
      break;
    case "snap-save": stop();
      { const a = curScope().accounts.find((x) => x.id === a1);
        if (a) { a.snapshots = a.snapshots || []; a.snapshots.unshift({ ts: nowStamp(), size: (16 + a.snapshots.length * 1.6).toFixed(1) + " MB" }); drawerAccount(a1); toast("ok", "已保存登录态快照", `${a.name} · ${a.snapshots[0].size}`); } }
      break;
    case "snap-restore": {
      stop();
      const a = curScope().accounts.find((x) => x.id === a1);
      const snap = a && a.snapshots ? a.snapshots[Number(a2)] : null;
      if (!a || !snap) break;
      /* 恢复登录态不可逆：先确认。 */
      openConfirm({
        title: "恢复登录态？",
        body: `将用快照 <b>${esc(snap.ts)}</b>（${esc(snap.size)}）覆盖 <b>${esc(a.name)}</b> 的当前登录态。<br><span class="tiny muted">恢复不可撤销，当前登录态会被替换。</span>`,
        okLabel: "恢复",
        onOk: () => { drawerAccount(a1); toast("ok", `已恢复 ${snap.ts} 的登录态`, `${a.name} 的凭据已回滚到该快照`); },
      });
      break;
    }
    case "confirm-cancel": stop(); closeConfirm(); break;
    case "confirm-ok": { stop(); const fn = pendingConfirm; closeConfirm(); if (fn) fn(); break; }

    /* ── 升级前说明弹窗（版本内容）──
       「立即升级」之前**只有阅读**：下载动作一律发生在这个 case 里，
       所以用户总是先看到「这一版改了什么」再决定。 */
    case "upd-start": case "upd-retry": stop(); startUpdateDownload(); break;
    case "upd-later": case "upd-close": stop();
      if (updBusy()) break;          /* 下载中关闭会让安装半途而废 ⇒ 按钮本身也已禁用 */
      closeUpdateDialog();
      break;
    case "upd-older": stop();
      state.update.olderOpen = !state.update.olderOpen;
      renderUpdateDialog();
      break;
    case "upd-release": stop();
      toast("ok", "已在浏览器打开发布页", APP_UPDATE.releaseUrl);
      break;
    case "upd-restart": stop();
      closeUpdateDialog();
      toast("ok", `已重启到 v${APP_UPDATE.latest}`, "更新已生效，正在重新连接网关");
      break;
  }
});

/* 输入类控件：事件委托。搜索只重绘列表区，保住光标与中文输入法组词。 */
document.addEventListener("input", (e) => {
  const t = e.target;
  if (t.id === "acct-q") { state.q = t.value; renderAccountList(); return; }
  if (t.id === "palette-input") { state.palette.q = t.value; state.palette.cursor = 0; renderPalette(); return; }
});
document.addEventListener("change", (e) => {
  const t = e.target;
  if (t.dataset && t.dataset.input === "sort") { state.sort = t.value; renderAccountList(); }
  if (t.dataset && t.dataset.input === "group") { state.group = t.value; renderAccountList(); }
  /* 抽屉里的分组选择器：改完即存（单值枚举，没有「保存」这一步的必要）。 */
  if (t.dataset && t.dataset.accountGroup) {
    if (t.value === "__new__") {
      /* 选「＋ 新建分组…」⇒ 就地变输入框，并把焦点放进去（用户刚做了选择，下一步必然是打字）。 */
      state.groupNew = t.dataset.accountGroup;
      drawerAccount(t.dataset.accountGroup);
      setTimeout(() => { const i = $("#group-new-input"); if (i) i.focus(); }, 30);
    } else {
      setAccountGroup(t.dataset.accountGroup, t.value);
    }
  }
});

/* 全局快捷键 */
document.addEventListener("keydown", (e) => {
  const mod = e.metaKey || e.ctrlKey;
  if (mod && e.key.toLowerCase() === "k") { e.preventDefault(); state.palette.open ? closePalette() : openPalette(); return; }
  if (mod && /^[1-9]$/.test(e.key)) {
    const ids = Object.keys(SCOPES); const id = ids[Number(e.key) - 1];
    if (id) { e.preventDefault(); switchScope(id); }
    return;
  }
  /* 键盘可达：账号卡 / 表格行聚焦时，Enter / Space 打开抽屉。
     只处理落在 [data-act^="acct-open"] 本身的事件，内层勾选框/程序位按钮不受影响。 */
  if ((e.key === "Enter" || e.key === " " || e.key === "Spacebar") &&
      e.target && e.target.matches && e.target.matches('[data-act^="acct-open"]')) {
    e.preventDefault();
    drawerAccount(e.target.dataset.act.split("|")[1]);
    return;
  }
  if (state.palette.open) {
    const items = window.__palItems || [];
    if (e.key === "Escape") { e.preventDefault(); closePalette(); }
    else if (e.key === "ArrowDown") { e.preventDefault(); state.palette.cursor = Math.min(items.length - 1, state.palette.cursor + 1); renderPalette(); }
    else if (e.key === "ArrowUp") { e.preventDefault(); state.palette.cursor = Math.max(0, state.palette.cursor - 1); renderPalette(); }
    else if (e.key === "Enter") { e.preventDefault(); const it = items[state.palette.cursor]; if (it) { closePalette(); it.run(); } }
    return;
  }
  /* 卡片「更多」菜单：Esc 收起（菜单是浮层，键盘用户必须能不用鼠标退出）。 */
  if (e.key === "Escape" && state.acctMenu) { e.preventDefault(); closeAcctMenu(); return; }
  /* 升级说明弹窗在上层：Esc 先关它，且**下载中不响应**（关掉 = 安装半途而废）。 */
  if (state.update.open && e.key === "Escape") {
    e.preventDefault();
    if (!updBusy()) closeUpdateDialog();
    return;
  }
  if (e.key === "Escape") { if ($("#drawer").classList.contains("open")) closeDrawer(); }
  if (e.key === "/" && !/INPUT|SELECT|TEXTAREA/.test(document.activeElement.tagName)) {
    e.preventDefault();
    if (state.dest !== "accounts") { state.dest = "accounts"; state.selected.clear(); renderAll(); }
    const i = $("#acct-q");
    if (i) i.focus();
  }
});

/* 按钮绑定 */
$("#search-btn").addEventListener("click", openPalette);
$("#refresh-btn").addEventListener("click", doRefresh);
$("#theme-btn").addEventListener("click", toggleTheme);
$("#app-settings-btn").addEventListener("click", () => {
  openDrawer(`<div class="dsec"><h4>应用级设置（L0，不属于任何作用域）</h4>
    <div class="srow"><div class="lab"><b>界面语言</b><span>简体中文 / English</span></div><select class="inp"><option>简体中文</option><option>English</option></select></div>
    <div class="srow"><div class="lab"><b>开机自启</b><span>系统启动时在后台运行</span></div><button class="switch on"></button></div>
    <div class="srow"><div class="lab"><b>自动检查更新</b><span>每 30 分钟检查一次</span></div><button class="switch on"></button></div>
    <div class="srow"><div class="lab"><b>最小化到托盘</b><span>关闭窗口时保持后台运行</span></div><button class="switch on"></button></div>
  </div>
  <div class="dsec">
    <h4>关于与更新</h4>
    <!-- 第二个入口（与侧栏版本行同源）：设置里也得能查一次，不能只留一个角落里的图标。 -->
    <div class="srow">
      <div class="lab"><b>当前版本</b><span>v${esc(APP_UPDATE.current)} · ${APP_UPDATE.hasUpdate ? `有新版本 v${esc(APP_UPDATE.latest)}` : "已是最新"}</span></div>
      <button class="btn sm${APP_UPDATE.hasUpdate ? " primary" : ""}" data-act="check-update">${APP_UPDATE.hasUpdate ? "查看更新内容" : "检查更新"}</button>
    </div>
  </div>`, "应用设置");
});
/* 卡片「更多」菜单是 fixed 浮层：页面一滚动它就和锚点脱开了 —— 直接收起，
   比「跟着乱飘 / 悬在半空」好。捕获阶段监听，`#content` 自己的滚动也能收到。 */
window.addEventListener("scroll", () => { if (state.acctMenu) closeAcctMenu(); }, { passive: true, capture: true });
$("#drawer-close").addEventListener("click", closeDrawer);
$("#drawer-overlay").addEventListener("click", closeDrawer);
$("#palette-overlay").addEventListener("click", (e) => { if (e.target.id === "palette-overlay") closePalette(); });
/* 点遮罩关闭升级说明：与其它弹层同一套模式；下载中不关（同上，避免安装半途而废）。 */
$("#update-overlay").addEventListener("click", (e) => {
  if (e.target.id === "update-overlay" && !updBusy()) closeUpdateDialog();
});
$("#hint-close").addEventListener("click", () => $("#hint").classList.add("hidden"));

/* 「执行时刻」改了要立刻反映到抖动预览 —— 否则评审把 08:00 改成别的时刻后，
   预览仍显示旧基准，会误判成「预览是死的」。改动写回 SETTINGS 再重渲染。 */
document.addEventListener("change", (e) => {
  const el = e.target && e.target.closest && e.target.closest("[data-bind]");
  if (!el || !el.value) return;
  const group = SETTINGS.find((s) => s.id === "checkin");
  const row = group && group.groups.flatMap((g) => g.rows).find((r) => r.bind === el.dataset.bind);
  if (!row) return;
  row.value = el.value;
  renderContent();
});

/* 确认 Dialog：按钮走 data-act 委托；遮罩点击沿用现有 overlay 模式 */
$("#confirm-overlay").addEventListener("click", (e) => { if (e.target.id === "confirm-overlay") closeConfirm(); });

/* ══════════════════════ 启动 ══════════════════════
   URL 深链：原型也能被分享。例如
     ?scope=trae-cn&dest=accounts&view=table&q=code
     ?dest=gateway&gw=pool
     ?open=palette  /  ?open=account:t1  /  ?open=add
   这同时是「URL 是唯一真值」这条设计原则在原型里的体现。 */
function bootFromUrl() {
  const p = new URLSearchParams(location.search);
  if (p.get("scope") && SCOPES[p.get("scope")]) state.scope = p.get("scope");
  if (p.get("dest") && DESTINATIONS.some((d) => d.id === p.get("dest"))) state.dest = p.get("dest");
  if (p.get("gw")) state.gwSub = p.get("gw");
  /* Trae 上下文策略也可深链：`?max=off` 直接看「关闭」态，便于评审对照。 */
  if (p.get("max") === "off") state.maxMode = "off";
  if (p.get("api")) state.gwApi = p.get("api");
  if (p.get("pin")) state.pins[state.scope] = p.get("pin");
  if (p.get("set")) state.setSub = p.get("set");
  if (p.get("usage")) state.usageTab = p.get("usage");
  if (p.get("view")) state.view = p.get("view");
  if (p.get("group")) state.group = p.get("group");
  if (p.get("range")) {
    const r = p.get("range");
    if (r === "today") { state.rangeMode = "today"; state.range = 1; }
    else if (r === "custom" && p.get("from") && p.get("to")) {
      state.rangeMode = "custom"; state.customFrom = p.get("from"); state.customTo = p.get("to");
    } else { state.rangeMode = "preset"; state.range = Number(r) || 30; }
  }
  if (p.get("q")) state.q = p.get("q");
  /* 错误态也走深链：?err=gateway / ?err=usage（可逗号分隔），便于直接复现。 */
  if (p.get("err")) p.get("err").split(",").forEach((k) => { if (k) state.errors[k] = true; });
  if (p.get("theme") === "light") { state.theme = "light"; document.documentElement.classList.remove("dark"); }
  if (p.get("hint") === "0") $("#hint").classList.add("hidden");
  /* 侧栏版本号与弹窗里的「当前版本」必须同一个值：两处各写一遍必然漂移。 */
  const footVer = $("#foot-ver");
  if (footVer) footVer.textContent = `v${APP_UPDATE.current}`;
  renderAll();
  const open = p.get("open");
  if (open === "palette") openPalette();
  else if (open === "add") drawerAdd();
  else if (open && open.startsWith("account:")) drawerAccount(open.slice(8));
  else if (open && open.startsWith("wizard:")) drawerWizard(open.slice(7), 1);
  else if (open && open.startsWith("perm:")) drawerPermGuide(open.slice(5));
  /* 升级前说明弹窗：?open=update 直接看「有更新」；&upd=downloading|ready|error 审各阶段。 */
  else if (open === "update") {
    const stages = ["available", "downloading", "ready", "error"];
    const stage = stages.includes(p.get("upd")) ? p.get("upd") : "available";
    /* 下载态要**从说明态起跑**：startUpdateDownload 的守卫是「已在下载中就返回」，
       所以这里先落到 available，再让它自己切到 downloading。 */
    openUpdateDialog(stage === "downloading" ? "available" : stage);
    if (stage === "downloading") startUpdateDownload();
  }
  if (p.get("updolder") === "1") { state.update.olderOpen = true; renderUpdateDialog(); }
}

bootFromUrl();

/* 引导条：15 秒后自动退场，别一直压着内容 */
setTimeout(() => $("#hint").classList.add("hidden"), 15000);
