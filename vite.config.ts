import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
import tailwindcss from "@tailwindcss/vite";
import process from "node:process";

const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [solid(), tailwindcss()],

  /*
   * 预打包的依赖白名单。
   *
   * ⚠️ **`@tabler/icons-solidjs` 不能放进来**（实测踩坑）：
   * 它的 ESM 产物（`dist/esm/*.js`）里带的是**未编译的 JSX**，而 Vite 的依赖预打包
   * 会把产物写成 `.js` —— 于是 import 分析读到 `<title>{…}</title>` 直接报
   * “content contains invalid JS syntax”，那个模块 500，动态 import 挂掉，页面白屏。
   *
   * 代价是开发期冷缓存下第一次打开陈列室会慢（Vite 要逐个模块提供 6000+ 图标，
   * 实测 ~15 秒 / 6440 个请求），之后就有缓存了。
   * **生产构建不受影响**：Rollup 会把未被引用的图标全部摇掉（主包 36KB）。
   */
  optimizeDeps: {
    include: ["solid-js", "solid-js/web"],
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
