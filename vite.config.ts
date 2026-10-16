import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "node:path";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;
// GitHub Pages serves only the public demo from the repository subpath.
// Normal WebUI and Tauri builds intentionally keep Vite's root base.
//
// Output-dir convention (keep these separate!):
//   `npm run build`       → `dist/`       base "/"                  — 被三处消费：
//                                       rust-embed(`crates/buddy-switch-server`)、
//                                       Tauri `frontendDist`、`scripts/fix-app.sh`
//   `npm run build:demo`  → `dist-demo/`  base "/trae-workbuddy-switch/" — 仅供 GitHub Pages
// 两者曾共用 `dist/`，导致「把演示构建编进 server/Tauri」→ index.html 请求
// `/trae-workbuddy-switch/assets/*`（embed 中不存在）→ 回退成 HTML → 模块脚本 MIME 校验失败
// → webui/桌面端空白页。`api::tests::embedded_index_html_references_only_embedded_assets`
// 是这条约定的回归护栏。
// ⚠️ `VITE_PAGES_DEMO` 同时决定两件事：**资源 base** 与 **路由类型**
// （`App.tsx` 用它选 `HashRouter`）。但本地验证需要「HashRouter + base=/」这一组合：
// Pages 形态的产物在本地预览下资源全部 404（`/trae-workbuddy-switch/assets/*` 不存在于
// 本地静态根）⇒ 页面一行都不渲染，CDP 断言全假红。
// ⇒ 把 base 抽成可覆盖的 `VITE_BASE`（缺省行为**逐字不变**）。
// @ts-expect-error process is a nodejs global
const base =
  process.env.VITE_BASE ?? (process.env.VITE_PAGES_DEMO === "1" ? "/trae-workbuddy-switch/" : "/");

// https://vite.dev/config/
export default defineConfig(async () => ({
  base,
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
