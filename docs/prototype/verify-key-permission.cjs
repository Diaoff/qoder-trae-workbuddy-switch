/* 密钥权限管理原型（issue #76）的**行为**验收套件。
 *
 * 立场：**不信任设计稿的自述**。所有断言都直接打开原型页面、点真实 DOM、读真实文本，
 * 不看任何"设计意图"注释。
 *
 * 依赖 jsdom（装在隔离工作区，不要往项目里装）：
 *   export NODE_PATH="<managed node workspace>/node_modules"
 *   node docs/prototype/verify-key-permission.cjs
 *
 * 变异模式（证明断言不是恒真的空转）：
 *   node docs/prototype/verify-key-permission.cjs --mutate=nosaveguard|noscope
 *   变异只写 os.tmpdir() 里的**副本**，交付目录下的原文件永不被写。
 *   变异模式下**全绿即为失败**（说明断言打不红）。
 */
const fs = require("fs");
const os = require("os");
const path = require("path");
const { JSDOM } = require("jsdom");

const FILE = "key-permission-design.html";
const results = [];
let dom = null;
let doc = null;

const MUT = (process.argv.find((a) => a.startsWith("--mutate")) || "")
  .replace("--mutate", "").replace(/^=/, "");
/* 每个变异都声明**它该打红哪几条断言**（`expectRed`）。
 * 判定口径：不是「全绿即失败」，而是「**该红的那几条真的红了**」——
 * 一个变异只坏一处行为，别的断言理应保持绿；全红反而说明变异把页面打瘫了
 * （那样测的就不是目标行为）。 */
const MUTATIONS = {
  nosaveguard: {
    desc: "去掉「指定账号 0 个时禁止保存」的守卫",
    from: "document.getElementById(\"dwSave\").disabled =",
    to: "document.getElementById(\"dwSave\").disabled = false; /* MUTATED: 去掉守卫 */\n  const _unused =",
    expectRed: ["「指定账号」但一个都没选"],
  },
  noscope: {
    desc: "把「指定账号」的账号列表渲染成空（不渲染可勾选项）",
    from: "document.getElementById(\"acctList\").innerHTML = list.length ? list.map(a => {",
    to: "document.getElementById(\"acctList\").innerHTML = list.length ? [].map(a => { /* MUTATED */",
    expectRed: ["切到「指定账号」才出现账号列表", "勾选账号后", "取消勾选后计数回落"],
  },
  nolimit: {
    desc: "配额上限写回时忽略输入值（保存后额度永远是旧值）",
    from: "k.quota = JSON.parse(JSON.stringify(state.draft.quota));",
    to: "k.quota = JSON.parse(JSON.stringify(state.draft.quota)); k.quota.limit = 0; /* MUTATED */",
    expectRed: ["保存后表格行的权限 chip"],
  },
};

function check(name, fn) {
  try {
    fn();
    results.push({ name, ok: true });
  } catch (error) {
    results.push({ name, ok: false, error: error.message });
  }
}

function assert(cond, message) {
  if (!cond) throw new Error(message);
}

function text(selector) {
  const el = doc.querySelector(selector);
  return el ? el.textContent.replace(/\s+/g, " ").trim() : "";
}

function click(selector) {
  const el = doc.querySelector(selector);
  assert(el, `找不到元素：${selector}`);
  el.dispatchEvent(new dom.window.MouseEvent("click", { bubbles: true }));
}

function clickAll(selector) {
  doc.querySelectorAll(selector).forEach((el) =>
    el.dispatchEvent(new dom.window.MouseEvent("click", { bubbles: true })));
}

function type(selector, value) {
  const el = doc.querySelector(selector);
  assert(el, `找不到输入框：${selector}`);
  el.value = String(value);
  el.dispatchEvent(new dom.window.Event("input", { bubbles: true }));
  el.dispatchEvent(new dom.window.Event("change", { bubbles: true }));
}

function load(mutated) {
  let html = fs.readFileSync(path.join(__dirname, FILE), "utf8");
  if (mutated) {
    const m = MUTATIONS[mutated];
    assert(m, `未知变异：${mutated}`);
    assert(html.includes(m.from), `变异锚点没命中：${m.from.slice(0, 60)}`);
    html = html.replace(m.from, m.to);
  }
  dom = new JSDOM(html, { runScripts: "dangerously", pretendToBeVisual: true, url: "http://localhost/" });
  doc = dom.window.document;
}

/* ★ 必须在**断言之前**装载页面：断言是立即执行的（`check(...)` 收到函数就调用）。 */
load(MUT || null);

function rowIndexByName(name) {
  const rows = [...doc.querySelectorAll("#rows tr")];
  const index = rows.findIndex((tr) => tr.textContent.includes(name));
  assert(index >= 0, `列表里没有密钥：${name}`);
  return index;
}

/* ───────────────────────── 断言 ───────────────────────── */

check("列表渲染出全部密钥行，且工具条计数与行数一致", () => {
  const rows = doc.querySelectorAll("#rows tr").length;
  assert(rows === 5, `行数应为 5，实际 ${rows}`);
  assert(text("#kTotal") === "5", `工具条总数应为 5，实际 ${text("#kTotal")}`);
  assert(text("#kLive") === "4", `工具条启用数应为 4，实际 ${text("#kLive")}`);
});

check("每行都有用量、权限两列的实体（进度条 + 两枚可点 chip）", () => {
  const rows = [...doc.querySelectorAll("#rows tr")];
  rows.forEach((tr, i) => {
    const chips = tr.querySelectorAll(".chip");
    assert(chips.length === 2, `第 ${i} 行权限列应有 2 枚 chip，实际 ${chips.length}`);
    chips.forEach((c) => assert(c.dataset.open, "chip 必须可点（带 data-open）"));
  });
  const withBar = [...doc.querySelectorAll("#rows tr")].filter((tr) => tr.querySelector(".bar-fill"));
  assert(withBar.length === 4, `有限额的行才有进度条，实际 ${withBar.length}`);
});

check("「不限」的密钥不画进度条，用量列写「不限」", () => {
  const tr = [...doc.querySelectorAll("#rows tr")].find((x) => x.textContent.includes("额度 不限"));
  assert(tr, "列表里应有「额度 不限」的密钥");
  assert(!tr.querySelector(".bar-fill"), "不限额不应有进度条");
  assert(tr.textContent.includes("不限"), "用量列应显示「不限」");
});

check("配额用量按百分比渲染（62% / 86% / 100%）", () => {
  const pcts = [...doc.querySelectorAll("#rows .bar-fill")].map((el) => el.style.width);
  assert(pcts.includes("62%"), `应有 62% 的进度条，实际 ${JSON.stringify(pcts)}`);
  assert(pcts.includes("100%"), `应有 100% 的进度条，实际 ${JSON.stringify(pcts)}`);
});

check("已用尽的密钥状态徽标是「已用尽」，接近上限是「接近上限」", () => {
  const body = doc.querySelector("#rows").textContent;
  assert(body.includes("已用尽"), "缺「已用尽」状态");
  assert(body.includes("接近上限"), "缺「接近上限」状态");
});

check("点权限 chip 打开抽屉，并显示两个分区", () => {
  click("#rows tr:nth-child(1) .chip");
  assert(doc.querySelector("#drawer").style.display !== "none", "抽屉未打开");
  assert(text("#dwTitle").includes("我的笔记本"), `抽屉标题应是密钥名，实际 ${text("#dwTitle")}`);
  const heads = [...doc.querySelectorAll(".section-head h2")].map((h) => h.textContent);
  assert(heads.length === 2, `应有两个分区，实际 ${heads.length}`);
  assert(heads[0].includes("Token 配额"), `第一段应是 Token 配额，实际 ${heads[0]}`);
  assert(heads[1].includes("可访问账号"), `第二段应是 可访问账号，实际 ${heads[1]}`);
});

check("切到「指定账号」才出现账号列表；默认「全部账号」不出现", () => {
  assert(doc.querySelector("#scopePicker").style.display === "none", "默认不该显示账号列表");
  click('[data-s="selected"]');
  assert(doc.querySelector("#scopePicker").style.display !== "none", "切到指定账号应显示列表");
  assert(doc.querySelectorAll("#acctList .acct[data-uid]").length === 6, "应渲染 6 个账号");
});

check("勾选账号后「已选 N 个」与预览同步更新", () => {
  clickAll('#acctList .acct[data-uid="u-001"]');
  assert(text("#scopeHint").includes("已选 1"), `已选计数未更新：${text("#scopeHint")}`);
  assert(text("#preview").includes("1 个指定账号"), `预览未同步：${text("#preview")}`);
});

check("取消勾选后计数回落", () => {
  /* ★ 先断言前置（此刻应已选 1）：否则「列表是空的」时这条断言会**恒真**，
     变异（不渲染账号）打不红它。 */
  assert(text("#scopeHint").includes("已选 1"), `前置：此时应已选 1，实际 ${text("#scopeHint")}`);
  clickAll('#acctList .acct[data-uid="u-001"]');
  assert(text("#scopeHint").includes("已选 0"), `取消后计数应归零：${text("#scopeHint")}`);
});

check("「指定账号」但一个都没选 ⇒ 保存按钮禁用且给出原因", () => {
  assert(doc.querySelector("#dwSave").disabled === true, "0 选中时保存必须禁用");
  assert(text("#scopeNoteText").includes("至少要选一个账号"), "缺「至少选一个账号」的提示");
});

check("选回「全部账号」⇒ 账号列表收起、保存按钮恢复可用", () => {
  click('[data-s="all"]');
  assert(doc.querySelector("#scopePicker").style.display === "none", "账号列表应收起");
  assert(doc.querySelector("#dwSave").disabled === false, "保存应恢复可用");
});

check("切「累计上限」并改额度 ⇒ 预览写明新额度", () => {
  click('[data-q="total"]');
  type("#qTotal", 200);
  assert(text("#preview").includes("200万"), `预览未反映新额度：${text("#preview")}`);
});

check("把上限改到低于已用量 ⇒ 提示「保存后立即超限」", () => {
  type("#qTotal", 10); // 10 万 < 已用 62 万
  assert(text("#quotaNoteText").includes("立即超限"), `缺立即超限提示：${text("#quotaNoteText")}`);
  assert(text("#preview").includes("立即超限"), `预览缺立即超限提示：${text("#preview")}`);
});

check("保存后表格行的权限 chip 与用量条同步更新", () => {
  click('[data-q="total"]');
  type("#qTotal", 100); // 100 万
  click('[data-s="selected"]');
  clickAll('#acctList .acct[data-uid="u-002"]');
  click("#dwSave");
  assert(doc.querySelector("#drawer").style.display === "none", "保存后抽屉应关闭");
  const tr = doc.querySelectorAll("#rows tr")[0];
  assert(tr.textContent.includes("账号 1 个"), `行内权限 chip 未更新：${tr.textContent}`);
  assert(tr.textContent.includes("额度 100万"), `行内额度 chip 未更新：${tr.textContent}`);
  assert(tr.textContent.includes("100万"), `用量列没显示上限：${tr.textContent}`);
  const fill = tr.querySelector(".bar-fill");
  assert(fill && fill.style.width === "62%", `用量条未更新：${fill && fill.style.width}`);
});

check("ESC 关闭抽屉", () => {
  click("#rows tr:nth-child(1) .chip");
  assert(doc.querySelector("#drawer").style.display !== "none", "抽屉未打开");
  doc.dispatchEvent(new dom.window.KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  assert(doc.querySelector("#drawer").style.display === "none", "ESC 未关闭抽屉");
});

check("创建对话框：可打开、可填名、可创建出新行", () => {
  const before = doc.querySelectorAll("#rows tr").length;
  click("#btnCreate");
  assert(doc.querySelector("#dlg").style.display !== "none", "创建对话框未打开");
  type("#cName", "设计稿测试");
  click("#cOk");
  const after = doc.querySelectorAll("#rows tr").length;
  assert(after === before + 1, `创建后应多一行，实际 ${after - before}`);
  assert(doc.querySelector("#rows").textContent.includes("设计稿测试"), "新行名称不对");
});

check("创建对话框里也能选配额模式（默认不限）", () => {
  click("#btnCreate");
  const on = [...doc.querySelectorAll("[data-cq]")].find((b) => b.dataset.cq === "unlimited");
  assert(on && on.style.background !== "transparent", "默认应选中「不限」");
  click('[data-cq="daily"]');
  assert(doc.querySelector("#cQuotaRow").style.display !== "none", "选每日上限后应出现数值行");
});

/* ───────────────────────── 变异模式 ───────────────────────── */

if (MUT) {
  const spec = MUTATIONS[MUT];
  const red = results.filter((r) => !r.ok).map((r) => r.name);
  console.log(`\n▶ 变异模式：${MUT} —— ${spec.desc}`);
  console.log(`  期望打红：${spec.expectRed.join(" / ")}\n`);
  for (const r of results) {
    console.log(`  ${r.ok ? "· 仍绿" : "✓ 变红"}  ${r.name}`);
  }
  const missed = spec.expectRed.filter((name) => !red.some((r) => r.includes(name)));
  const allRed = results.every((r) => !r.ok);
  if (missed.length) {
    console.log(`\n❌ 期望变红却仍然绿：${missed.join(" / ")} —— 断言打不红`);
    process.exit(1);
  }
  if (allRed) {
    console.log("\n❌ 全部断言都红了 —— 变异把页面打瘫了，测的不是目标行为");
    process.exit(1);
  }
  console.log("\n✅ 变异被目标断言打红 —— 断言有效");
  process.exit(0);
}

console.log("");
let failed = 0;
for (const r of results) {
  if (!r.ok) failed += 1;
  console.log(`${r.ok ? "✓" : "✗"} ${r.name}`);
  if (!r.ok) console.log(`    ${r.error}`);
}
console.log(`\n${results.length - failed} passed, ${failed} failed`);
process.exit(failed === 0 ? 0 : 1);
