#!/usr/bin/env node
/**
 * `aggregateByKey` 的**真实断言**（issue #76 / 规范 U4）。
 *
 * ## 为什么是脚本而不是 vitest
 *
 * 项目**没有前端测试框架**（`package.json` 没有 `test` 脚本，质量靠 `check:*` 门禁）。
 * 为一个纯函数引入 vitest + jsdom + CI 改造，成本远大于收益。
 *
 * 这里用**已有的** esbuild（vite 的依赖，无需新增）把 `src/lib/key-usage.ts`
 * 现场转译成 ESM、经 `data:` URL 导入，跑真断言 —— 测的是**生产代码本身**，
 * 不是复制一份逻辑（复制的那份必然与生产漂移）。
 *
 * 用法：`node scripts/check-key-usage.mjs`（已挂进 `npm run check:scope`）。
 */

import { readFileSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";
import path from "node:path";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const source = readFileSync(path.join(root, "src/lib/key-usage.ts"), "utf8");

// ★ Windows：绝对路径必须转 `file://` URL，否则 ESM loader 把 `D:` 当成协议。
const { transform } = await import(
  pathToFileURL(path.join(root, "node_modules/esbuild/lib/main.js")).href
);
const { code } = await transform(source, { loader: "ts", format: "esm" });
const mod = await import(
  `data:text/javascript;base64,${Buffer.from(code).toString("base64")}`
);
const { aggregateByKey, LEGACY_KEY_LABEL } = mod;

let failed = 0;
let passed = 0;
function check(name, fn) {
  try {
    fn();
    passed += 1;
  } catch (error) {
    failed += 1;
    console.error(`  ✗ ${name}\n    ${error.message}`);
  }
}
function eq(actual, expected, hint) {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  if (a !== e) throw new Error(`${hint ? hint + "：" : ""}期望 ${e}，实际 ${a}`);
}

const names = new Map([
  ["k1", "脚本用的 key"],
  ["k2", "CI 用的 key"],
]);

/* ── ① 两种「没有」必须分开 ───────────────────────────────────────── */
check("老条目（无 keyId 键）与无 Key 调用（keyId: null）分开成两行", () => {
  const rows = aggregateByKey(
    [
      { ts: 1, promptTokens: 10, completionTokens: 5 }, // 老条目：键不存在
      { ts: 2, promptTokens: 1, completionTokens: 1, keyId: null }, // 确实无 Key
    ],
    names,
    "wb",
  );
  eq(rows.length, 2, "应有两行（合并会让历史数据凭空多出「匿名调用」）");
  const legacy = rows.find((row) => row.name === LEGACY_KEY_LABEL);
  const none = rows.find((row) => row.name === "无密钥调用");
  if (!legacy) throw new Error("缺「升级前的历史条目」行");
  if (!none) throw new Error("缺「无密钥调用」行");
  eq(legacy.tokens, 15);
  eq(none.tokens, 2);
});

/* ── ② 同一 id 在两套台账里是两把 Key ───────────────────────────── */
check("同一 keyId 不同 keyScope 分成两行，且跨台账标出归属", () => {
  const rows = aggregateByKey(
    [
      { ts: 1, promptTokens: 10, keyId: "same", keyScope: "wb" },
      { ts: 2, promptTokens: 20, keyId: "same", keyScope: "unified" },
    ],
    names,
    "wb",
  );
  eq(rows.length, 2, "不同台账的同名 id 不能并成一行");
  const local = rows.find((row) => row.name === "脚本用的 key" || row.name.startsWith("same"));
  const cross = rows.find((row) => row.name.includes("统一网关"));
  if (!cross) throw new Error("跨台账的 Key 必须标出「统一网关」而不是拿短 id 冒充本地 Key");
  eq(cross.tokens, 20);
  if (!local) throw new Error("本台账的行丢了");
});

/* ── ③ 聚合与排序 ────────────────────────────────────────────────── */
check("按 token 降序，且累计 token 是 prompt + completion", () => {
  const rows = aggregateByKey(
    [
      { ts: 1, promptTokens: 100, completionTokens: 50, keyId: "k1", keyScope: "wb" },
      { ts: 2, promptTokens: 10, completionTokens: 5, keyId: "k2", keyScope: "wb" },
      { ts: 3, promptTokens: 1, completionTokens: 1, keyId: "k1", keyScope: "wb" },
    ],
    names,
    "wb",
  );
  eq(rows[0].name, "脚本用的 key", "token 多的排前面");
  eq(rows[0].tokens, 152);
  eq(rows[0].requests, 2);
  eq(rows[1].tokens, 15);
});

check("token 全 0（业务代理）时按请求数降序", () => {
  const rows = aggregateByKey(
    [
      { ts: 1, keyId: "k1", keyScope: "wb" },
      { ts: 2, keyId: "k2", keyScope: "wb" },
      { ts: 3, keyId: "k2", keyScope: "wb" },
    ],
    names,
    "wb",
  );
  eq(rows[0].name, "CI 用的 key", "请求数多的排前面");
  eq(rows[0].requests, 2);
});

/* ── ④ 取不到名字时的兜底 ───────────────────────────────────────── */
check("Key 名取不到时用短 id，不显示「未知」", () => {
  const rows = aggregateByKey(
    [{ ts: 1, promptTokens: 5, keyId: "abcdefghijklmnop", keyScope: "wb" }],
    names,
    "wb",
  );
  eq(rows.length, 1);
  if (!rows[0].name.startsWith("abcdefgh")) {
    throw new Error(`应显示短 id（前 8 位），实际 ${rows[0].name}`);
  }
});

/* ── ⑤ 空输入 ────────────────────────────────────────────────────── */
check("空输入返回空数组（不崩、不造假行）", () => {
  eq(aggregateByKey([], names, "wb"), []);
});

console.log(
  failed === 0
    ? `密钥用量聚合：${passed} 项通过`
    : `密钥用量聚合：${passed} 项通过，${failed} 项失败`,
);
if (failed > 0) process.exit(1);
