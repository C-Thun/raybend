import { defineConfig, type Plugin } from "vite";
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

/* ══════════════════════════════════════════════════════════════
 * CJK 字体的 metrics 覆盖（"中文上飘" 的根因与修法）
 *
 * ## 现象
 *
 * 小控件（标题栏密度芯片、`按时间`的「全选当天 / 全选此段」药丸、分段控件…）里的中文
 * 看着**偏高**。人类 2026-09-16 连续两轮反馈，而且点出了三个排查方向。
 *
 * ## 根因（实测，不是猜的）
 *
 * 用**像素级**量法（截屏后算墨水质心相对盒子中心，精度 ±0.1px）确认了三件事：
 *   1. 不是 CSS 排版错：这些控件都是 `flex items-center`，改成什么 `line-height`
 *      （`1` / `1.2` / `1.45` / 写死 px）都几乎不动 —— 因为半行距是上下对称分的，
 *      **墨水在行盒里的位置跟 `line-height` 无关**。
 *   2. 不是「多段字体冒充加粗」：加粗走的就是 `font-weight`（可变字体真有 100–900 轴：
 *      Inter 400→600 的字符宽度会变、CJK 的笔画会变粗，不是合成粗体）。
 *   3. 是**字体自己声明的 metrics 与它画的字不一致**：Noto Sans SC 声明 ascent/descent
 *      约 `1.16em / 0.29em`，而方块字的**墨水**只占 `0.85em / 0.09em` ——
 *      行盒把墨水分得偏上。我们字体栈里 Inter 又排在前面（拉丁走 Inter），
 *      同一条行盒里两套 metrics 还会互相拉扯，所以偏移量还会随字号/字体组合变。
 *
 * ## 修法
 *
 * 用 CSS Fonts 4 的 `ascent-override` / `descent-override` 把这套字体的 metrics
 * 改成**方块字的 em 盒**（88% / 12%，即 Noto 自己的 OS/2 typo 度量 880/−120）——
 * 这样墨水中心就正好落在行盒中心。实测：覆盖前偏 −0.87px（Inter strut）/ +1.40px（CJK strut），
 * 覆盖后都在 ±0.1px 内。
 *
 * ## 为什么写成编译期插件而不是手写一份 CSS
 *
 * @fontsource 把这个字体切成 **94 个 `unicode-range` 切片**（每个一个 `@font-face`，
 * 这样浏览器只下载用到的那些字）。手抄一份必然漂移；放这里给每个切片补三行描述符，
 * 字体包升级时自动跟上。
 *
 * ⚠ 覆盖的是**度量**，不是字号 —— 不会把字改大改小，只改它在行盒里的垂直位置。
 * 配套断言：`pnpm smoke:ui` 会把墨水质心与盒中心的偏移量真测一遗（超 1px 就红）。
 * ══════════════════════════════════════════════════════════════ */
function cjkMetricsOverride(): Plugin {
  const PACKAGE_CSS = "@fontsource-variable/noto-sans-sc/wght.css";
  const FAMILY = "Noto Sans SC Variable";
  const DESCRIPTORS = [
    "ascent-override: 88%",
    "descent-override: 12%",
    "line-gap-override: 0%",
  ];
  /** 每个 @font-face 块（这些 CSS 都是扁平的，不会有嵌套块） */
  const FACE = /@font-face\s*\{[^}]*\}/g;
  const IMPORT = /@import\s+["']@fontsource-variable\/noto-sans-sc\/wght\.css["']\s*;/;

  /** 给每个 CJK 的 @font-face 块补上三行描述符；返回补了几块 */
  const patch = (css: string): { css: string; patched: number } => {
    let patched = 0;
    const next = css.replace(FACE, (block) => {
      if (!block.includes(FAMILY)) return block;
      if (block.includes("ascent-override")) return block; // 已经补过（HMR 重复进管线）
      patched += 1;
      return block.replace(/\s*\}$/, `\n  ${DESCRIPTORS.join(";\n  ")};\n}`);
    });
    if (patched > 0 && patched < 90) {
      console.warn(
        `[font] 只给 ${patched} 个 @font-face 补了 metrics 覆盖（预期 ~94 个切片）—— ` +
          "@fontsource 的产物形状可能变了，中文可能重新开始上飘",
      );
    }
    return { css: next, patched };
  };

  return {
    name: "raybend:cjk-metrics-override",
    enforce: "pre",
    transform(code, id) {
      /*
       * ⚠️ id 上可能挂着查询串（Vite 对 CSS 会加 `?direct` / `?inline`），
       * 直接 `endsWith(".css")` 会把这些情况全部漏掉 —— 实测踩过：
       * 包 CSS 那份补上了、而真正进页面（带查询串）的那份一声不响地没补。
       */
      const file = id.split("?")[0] ?? id;
      if (!file.endsWith(".css")) return undefined;

      /*
       * ① 引用它的那一处（`src/styles/fonts.css`，此时还没内联）—— 自己把补好的内容内联进去。
       *
       * ⚠️ 这一步必须**排在 ② 前面**：那个文件的头部注释里就写着 `Noto Sans SC Variable`，
       * 如果先按「代码里出现了这个字体名」去走 ②，会因「没有 @font-face 可改」早早 return，
       * 这分支永远轮不到（实测踩过：包 CSS 补上了、真正进页面的那份没补）。
       *
       * 而且不能指望「我改包 CSS、postcss 再内联」：postcss-import 是**直接读盘**的，
       * 不走插件管线。
       *
       * `url(./files/…)` 是相对包目录写的，换到本文件后相对基准变了，
       * 所以一并改写成相对路径（`../../node_modules/…`），Vite 照常解析并散列。
       */
      if (IMPORT.test(code)) {
        const raw = readFileSync(
          new URL(`./node_modules/${PACKAGE_CSS}`, import.meta.url),
          "utf8",
        );
        const inlined = patch(raw).css.replace(
          /url\(("?)\.\/files\//g,
          "url($1../../node_modules/@fontsource-variable/noto-sans-sc/files/",
        );
        return { code: code.replace(IMPORT, inlined), map: null };
      }

      // ② 含 @font-face 的包 CSS 本体（也可能已被 postcss 内联进别的文件）—— 就地补
      if (code.includes("@font-face") && code.includes(FAMILY)) {
        const result = patch(code);
        return result.patched > 0 ? { code: result.css, map: null } : undefined;
      }

      return undefined;
    },
  };
}

function readBuildInfo() {
  const dirty = (git(["status", "--porcelain"]) ?? "").length > 0;

  return {
    version: process.env.RAYBEND_VERSION?.trim() || readPackageVersion(),
    channel: process.env.RAYBEND_CHANNEL?.trim() || "dev",
    // 显式传入的构建时间优先（可复现构建），否则取当下
    builtAt: process.env.RAYBEND_BUILD_TIME?.trim() || new Date().toISOString(),
    gitHash: process.env.RAYBEND_GIT_HASH?.trim() || git(["rev-parse", "--short", "HEAD"]),
    dirty: process.env.RAYBEND_DIRTY !== undefined ? process.env.RAYBEND_DIRTY === "1" : dirty,
  };
}

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [cjkMetricsOverride(), solid(), tailwindcss()],

  /**
   * 注入构建信息（`src/lib/build-info.ts` 读取）。
   * 用 `define` 而不是虚拟模块：少一层间接，且 dev 与 build 行为一致。
   */
  define: {
    __RAYBEND_BUILD__: JSON.stringify(readBuildInfo()),
    __RAYBEND_UPDATER_PUBLIC_KEY__: JSON.stringify(process.env.RAYBEND_UPDATER_PUBLIC_KEY || ""),
    __RAYBEND_DISTRIBUTION__: JSON.stringify(process.env.RAYBEND_DISTRIBUTION || "direct"),
    __RAYBEND_DIAGNOSTICS__: JSON.stringify(["dev","test"].includes(process.env.RAYBEND_CHANNEL?.trim() || "dev")),
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
