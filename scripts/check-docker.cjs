#!/usr/bin/env node
"use strict";

// Docker 部署护栏（零依赖，仅用 Node 标准库）。
//
// ## 为什么需要它
//
// 容器化引入了四处**没有编译期保护**的字符串契约，任一处漂移的症状都是
// 「构建全绿、运行时静默失效」：
//
//   ① Dockerfile 里的 `COPY --from=<stage>` 必须指向**真实存在的阶段名** ——
//      写错一个字母，`docker build` 才报错（本地没装 Docker 时根本发现不了）。
//   ② `COPY --from=builder /src/target/release/<bin>` 的 `<bin>` 必须等于
//      `crates/buddy-switch-server/Cargo.toml` 的 `[[bin]] name`。
//   ③ 镜像里 `ENV BUDDY_SWITCH_HOST` / `BUDDY_SWITCH_PORT` 的名字必须等于
//      `main.rs` 里那两个常量 —— 名字对不上 ⇒ 容器内仍绑 `127.0.0.1` ⇒
//      宿主访问不到，而容器日志一切正常（只打印一行 webui 地址）。
//   ④ `.dockerignore` **必须**排除 `dist/` —— 否则宿主机那份旧前端可能被编进二进制，
//      表现为「页面正常但内容是旧的」。
//
// 用法：node scripts/check-docker.cjs   （npm run check:docker）

const fs = require("fs");
const path = require("path");

const ROOT = path.resolve(__dirname, "..");
const DOCKERFILE = path.join(ROOT, "Dockerfile");
const DOCKERIGNORE = path.join(ROOT, ".dockerignore");
const COMPOSE = path.join(ROOT, "docker-compose.yml");
const ENTRYPOINT = path.join(ROOT, "docker", "entrypoint.sh");
const SERVER_MAIN_RS = path.join(ROOT, "crates", "buddy-switch-server", "src", "main.rs");
const SERVER_CARGO_TOML = path.join(ROOT, "crates", "buddy-switch-server", "Cargo.toml");

const failures = [];
const notes = [];

function fail(message) {
  failures.push(message);
}

function read(file) {
  if (!fs.existsSync(file)) {
    fail(`缺少文件：${path.relative(ROOT, file)}`);
    return null;
  }
  return fs.readFileSync(file, "utf8");
}

/** 从 Rust 源码里取 `const <NAME>: &str = "<value>";` 的字面量。 */
function rustConstValue(source, constName) {
  const match = source.match(
    new RegExp(`const\\s+${constName}\\s*:\\s*&str\\s*=\\s*"([^"]+)"`),
  );
  return match ? match[1] : null;
}

const dockerfile = read(DOCKERFILE);
const dockerignore = read(DOCKERIGNORE);
const compose = read(COMPOSE);
read(ENTRYPOINT);
const mainRs = read(SERVER_MAIN_RS);
const serverCargo = read(SERVER_CARGO_TOML);

// ---------------------------------------------------------------------------
// ① 阶段名引用必须存在
// ---------------------------------------------------------------------------
if (dockerfile) {
  const declared = new Set(
    [...dockerfile.matchAll(/^\s*FROM\s+\S+\s+AS\s+(\S+)/gim)].map((m) => m[1]),
  );
  if (declared.size === 0) {
    fail("Dockerfile 里没有多阶段构建（`FROM … AS <name>`）—— 前端产物无法被 Rust 阶段取到");
  }
  for (const match of dockerfile.matchAll(/COPY\s+--from=(\S+)/g)) {
    const stage = match[1];
    if (!declared.has(stage)) {
      fail(`Dockerfile 的 \`COPY --from=${stage}\` 指向不存在的阶段（已声明：${[...declared].join(", ")}）`);
    }
  }
  notes.push(`Dockerfile 阶段：${[...declared].join(" → ")}`);
}

// ---------------------------------------------------------------------------
// ② 拷贝的二进制名必须等于 Cargo.toml 的 [[bin]] name
// ---------------------------------------------------------------------------
if (dockerfile && serverCargo) {
  const binMatch = serverCargo.match(/\[\[bin\]\][\s\S]*?name\s*=\s*"([^"]+)"/);
  const binName = binMatch ? binMatch[1] : null;
  if (!binName) {
    fail("crates/buddy-switch-server/Cargo.toml 里找不到 [[bin]] name");
  } else {
    const copy = dockerfile.match(/COPY\s+--from=\S+\s+\S*\/release\/(\S+)\s/);
    if (!copy) {
      fail("Dockerfile 里找不到把 release 产物拷进运行阶段的 `COPY --from=… /release/…`");
    } else if (copy[1] !== binName) {
      fail(
        `Dockerfile 拷贝的二进制名 \`${copy[1]}\` 与 Cargo.toml 的 [[bin]] name \`${binName}\` 不一致`,
      );
    } else {
      notes.push(`二进制名一致：${binName}`);
    }
  }
}

// ---------------------------------------------------------------------------
// ③ 环境变量名必须与 Rust 常量逐字一致
// ---------------------------------------------------------------------------
if (mainRs) {
  const hostConst = rustConstValue(mainRs, "BUDDY_SWITCH_HOST_ENV");
  const portConst = rustConstValue(mainRs, "BUDDY_SWITCH_PORT_ENV");
  if (!hostConst || !portConst) {
    fail("main.rs 里找不到 BUDDY_SWITCH_HOST_ENV / BUDDY_SWITCH_PORT_ENV 常量");
  } else {
    // Dockerfile 与 compose 都必须用这两个名字。
    for (const [label, source] of [
      ["Dockerfile", dockerfile],
      ["docker-compose.yml", compose],
    ]) {
      if (!source) continue;
      for (const name of [hostConst, portConst]) {
        if (!source.includes(name)) {
          fail(`${label} 里没有出现环境变量 ${name}（main.rs 里的常量值）—— 容器内会回落 127.0.0.1`);
        }
      }
    }
    // BUDDY_SWITCH_HOME 是 CLI 的既有硬约束，镜像必须显式设置（否则写到容器内 home）。
    for (const [label, source] of [
      ["Dockerfile", dockerfile],
      ["docker-compose.yml", compose],
    ]) {
      if (source && !source.includes("BUDDY_SWITCH_HOME")) {
        fail(`${label} 没有设置 BUDDY_SWITCH_HOME —— 数据会落在容器内 home，重建即丢`);
      }
    }
    notes.push(`环境变量：${hostConst} / ${portConst} / BUDDY_SWITCH_HOME`);
  }
}

// ---------------------------------------------------------------------------
// ④ .dockerignore 必须排除 dist/（防「编进旧前端」）
// ---------------------------------------------------------------------------
if (dockerignore) {
  const lines = dockerignore
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line && !line.startsWith("#"));
  if (!lines.includes("dist")) {
    fail(
      ".dockerignore 没有排除 `dist` —— 宿主机那份旧前端可能被编进二进制" +
        "（症状：页面正常但内容是旧的）",
    );
  }
  if (!lines.includes("target")) {
    fail(".dockerignore 没有排除 `target` —— 构建上下文会带上几十 GB 的编译产物");
  }
  if (!lines.includes("node_modules")) {
    fail(".dockerignore 没有排除 `node_modules`");
  }
  notes.push(".dockerignore：已排除 dist / target / node_modules");
}

// ---------------------------------------------------------------------------
// ⑤ 运行阶段必须以非 root 起、且入口脚本存在
// ---------------------------------------------------------------------------
if (dockerfile) {
  if (!/^USER\s+(?!root)\S+/m.test(dockerfile)) {
    fail("Dockerfile 运行阶段没有 `USER <非 root>` —— 容器会以 root 跑整个管理面");
  }
  if (!dockerfile.includes("entrypoint.sh")) {
    fail("Dockerfile 没有引用 docker/entrypoint.sh —— BUDDY_SWITCH_HOME 不会被创建，CLI 会拒绝启动");
  }
}

if (failures.length > 0) {
  console.error("[check-docker] 失败：");
  for (const failure of failures) console.error(`  ✗ ${failure}`);
  process.exit(1);
}

console.log("[check-docker] 通过：");
for (const note of notes) console.log(`  · ${note}`);
