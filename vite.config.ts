import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
import tailwindcss from "@tailwindcss/vite";
import process from "node:process";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

const host = process.env.TAURI_DEV_HOST;

/* ══════════════════════════════════════════════════════════════
 * 构建信息：版本 / 通道 / 构建时间 / git 状态
 *
 * 为什么要在这里算：这些值只有构建时才知道，而「关于」弹窗与排障都要用。
 * 通道由打包脚本通过环境变量指定（见 scripts/release.mjs），本地开发默认 dev。
 * ══════════════════════════════════════════════════════════════ */

function git(args: string[]): string | undefined {
  try {
    return execFileSync("git", args, { encoding: "utf8" }).trim() || undefined;
  } catch {
    // 构建机没装 git / 不是 git 仓库：不影响构建，只是少一信息
    return undefined;
  }
}

/** 从 package.json 读版本号；读不到就用 0.0.0（构建不该因为一个版本号而失败） */
function readPackageVersion(): string {
  try {
    const pkg = JSON.parse(
      readFileSync(new URL("./package.json", import.meta.url), "utf8"),
    ) as { version?: string };
    const version = pkg.version?.trim();
    return version && version.length > 0 ? version : "0.0.0";
  } catch (error) {
    console.warn("[build] 读 package.json 失败，版本号回落 0.0.0", error);
    return "0.0.0";
  }
}

function readBuildInfo() {
  const dirty = (git(["status", "--porcelain"]) ?? "").length > 0;

  return {
    version: process.env.RAYBEND_VERSION?.trim() || readPackageVersion(),
    channel: process.env.RAYBEND_CHANNEL?.trim() || "dev",
    // 显式传入的构建时间优先（可复现构建），否则取当下
    builtAt: process.env.RAYBEND_BUILD_TIME?.trim() || new Date().toISOString(),
    gitHash: process.env.RAYBEND_GIT_HASH?.trim() || git(["rev-parse", "--short", "HEAD"]),
    dirty: process.env.RAYBEND_DIRTY === "1" || dirty,
  };
}

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [solid(), tailwindcss()],

  /**
   * 注入构建信息（`src/lib/build-info.ts` 读取）。
   * 用 `define` 而不是虚拟模块：少一层间接，且 dev 与 build 行为一致。
   */
  define: {
    __RAYBEND_BUILD__: JSON.stringify(readBuildInfo()),
  },

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
