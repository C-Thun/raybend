# 2026-11-15T15:37+08:00 — 官网（website/）基础依赖整体升级到最新可用线

只动 `website/` 的依赖，一次性把整条基础链升到当前可用最新版；应用本体（根 `package.json`、根 `pnpm-lock.yaml`、`src/`、`crates/`、`src-tauri/`）一个字节没改（根 `package.json` 与根 `pnpm-lock.yaml` 的 mtime 仍是 11-10）。

| 包 | 旧 | 新 | 备注 |
| --- | --- | --- | --- |
| `solid-js` / `@solidjs/web` | 2.0.0-rc.6 | 2.0.0-rc.21 | lockstep |
| `@solidjs/diagnostics` | 2.0.0-rc.6 | 2.0.0-rc.21 | 与 solid-js 同版本要求 |
| `@solidjs/testing-library` | 1.0.0-beta.3 | 1.0.0-rc.2 | |
| `@solidjs/vite-plugin` | 3.0.0-next.38 | 3.0.0-next.57 | `start: true` / `extensions` / `diagnostics` 三个选项仍在 |
| `@solidjs/router` | 2.0.0-next.21 | 2.0.0-next.44 | 要求 filesystem-routing ≥0.4、solid-js ≥rc.19 |
| `filesystem-routing` | 0.2.1 | 0.5.0 | `fileRoutes()` 签名未变；生成物 `file-routes.d.ts` 类型更精确 + 多了 `server?: boolean` |
| `@solidjs/meta` | 1.0.0-next.2 | 不变 | 已是 next 线最新 |
| `vite` | 8.2.2 | 8.3.8 | |
| `vitest` | 5.0.0 | 5.1.0 | |
| `jsdom` | 25.0.1 | 31.0.1 | 跨 6 个大版，测试全绿 |
| `tailwindcss` / `@tailwindcss/vite` | 4.3.3 | 4.3.5 | |
| `typescript` | 5.9.3 | 7.0.2（`~`） | 原生 Go 编译器，`tsc --noEmit` 0.24 s / 485 文件 |
| `oxlint` | 1.79.0 | 1.93.0 | |
| `eslint-plugin-solid` | 0.18.0 | 0.19.0 | v2 规则集不变 |
| `lucide-solid` | 1.46.0 | 1.73.0 | **只作图标数据源**（dev-only）；重新生成 `icons.tsx` 后**逐字节相同** |
| `@fontsource-variable/noto-sans-sc` | 5.3.0 | 5.3.1 | 子集切分微调（见下） |
| `@types/node` | 26.6.1 | 26.6.6 | |

## 关键验证（样式不变是硬要求）

1. **CSS 声明集合比对**（升级前先 `pnpm build` 打快照，升级后同法再比；规则层归一化：规则切分 + 规则内声明排序 + hover/层前缀保留）：
   - 归一化后 605 → 606 行，`@font-face` 108 = 108；
   - 差异仅 4 处，全部无视觉影响：
     - Tailwind 的能力探测从 `(not (margin-trim:inline))` 变为 `(not (margin-trim:block))`；
     - `var(--tw-blur,)` → `var(--tw-blur, )` 这类空回退空格（3 条）；
     - Noto Sans SC 5.3.1 把一段 `unicode-range` 从 `U+20AD-20C0` 扩到 `U+20AD-20C4`（新增货币符号，纯超集）。
2. **运行时冒烟（CDP + headless Chrome，产物预览与 dev server 各一遍）**：7 个 section、footer、108 个 font-face 注册、图片 naturalWidth 正常、**0 条 console error / 未捕获异常 / 资源加载失败**。
3. **交互**：点「中」切换语言 → `<html lang>` 变 `zh-CN`、`@solidjs/meta` 的 title 变中文、六个 h2 全变中文、`localStorage('raybend.locale')='zh'`（说明 Solid 2 rc.21 的信号写入 → DOM 更新链路正常）。
4. **路由**：`/@nope-404` 正确落到 404 页，title 为 `Page not found · RayBend`。
5. **dev server 诊断通道**：`GET /__solid/diagnostics` 返回 `{"ok":true,"methods":[...]}`，vite-plugin 的 diagnostics 选项迁移后仍工作。
6. **质量门**：`pnpm typecheck`（TS 7）通过 —— 并额外注入一个 `const x: number = "str"` 确认它真的在检查（报 TS2322）后还原；`pnpm test:run` 4 文件 / 28 用例全绿；`pnpm lint` 仍只有升级前那一条 `solid(prefer-structured-class)` 警告；`pnpm build` 通过；清空 `node_modules` 后 `pnpm install --frozen-lockfile` 通过（CI 走这条）。
7. **体积**：JS 总量 293.6 kB → 263.7 kB（rc.21 的打包把 6 个 chunk 合成 3 个，少了请求数），`dist/client` 仍 5.9 MB（字体占大头）。

## 第三方复审补强（2026-11-15T16:18:42+08:00）

- **字体排版补充取证**：直接解包比较 Noto Sans SC 5.3.0 / 5.3.1 的 101 个共用 WOFF2 子集；虽然 `HVAR` 二进制表有编码差异，但用 fontTools 在 100 / 400 / 500 / 600 / 700 / 900 六档权重实例化后逐字比较 `hmtx` 排版宽度，**606 组字体/权重组合差异均为 0**，排除了本次字体升级改变字宽与换行的风险。
- **把临时浏览器检查固化成可重复 E2E**：新增 `website/scripts/smoke-ui.mjs` 与 `pnpm smoke:ui`，零新增依赖（Node WebSocket + 本机 Chrome），自动断言首页七段、图片与样式加载、语言切换及刷新后持久化、404、console / 未捕获异常 / 资源失败；根路径生产预览与 dev server 均通过。脚本用 `frameNavigated` 等待导航，避免 dev 下 reload 的旧/新文档竞态。
- **供应链审计与补救**：`pnpm audit` 初次发现 6 个 high，均在可更新的间接依赖上；按现有 semver 范围把 lockfile 中的 `brace-expansion` 升到 5.0.13、`source-map-js` 升到 1.2.2、`follow-redirects` 升到 1.16.3，未加 override。重跑 `pnpm icons:generate` / `pnpm marks:generate` 后所有生成物逐字节不变。审计剩 2 个 moderate：ESLint 链的 `minimatch` 需升到 11 才修复（当前上游约束仍是 10），potrace/Jimp 链的 `phin` 暂无修复版；两者都只在开发期工具链，未为消数字而强上 override。
- **另发现一个升级前已存在的子路径问题（不在本轮修）**：`RAYBEND_BASE=/raybend/` 构建后访问 `/raybend/` 为空白。用 `git archive HEAD` 在 `/tmp` 独立安装旧版（router next.21 / Vite 8.2.2）也复现，确认不是本轮回归。疑点为 `website/src/router.ts:5` 的 `createRouter` 未传 `base: import.meta.env.BASE_URL`；后续应在独立改动中修复并保留本次新增 smoke 的子路径用例。

## 已知遗留 / 判断

- `pnpm peers check` 常驻一条 `lucide-solid` 的 `unmet peer solid-js ^1.4.7`：lucide 到 1.73 仍只声明支持 Solid 1，但本站只用它的**图标数据**不走它的组件，属预期，已写进 `website/AGENTS.md` §4.5。
- `website/package.json` 的 `packageManager` 仍是 `pnpm@12.3.4`（本机 pnpm 提示可升 12.21）：CI 用 `pnpm/action-setup@v6` 读这个字段，升它属于另一件事，本轮不动。
- `website/file-routes.d.ts` 是生成物，随 filesystem-routing 0.5.0 自动重写（只变了类型精度，路由两条不变），一并提交。
- 类型桥不受影响：本站不涉及 Tauri IPC；TS 7 不带 compiler API 这件事对本站无影响（无依赖 `typescript` 库的工具）。

## 涉及文件

`website/package.json`、`website/pnpm-lock.yaml`、`website/file-routes.d.ts`、`website/AGENTS.md`（§1 版本表 + §3 smoke 命令 + §4.5 lucide 说明）、`website/scripts/generate-icons.mjs`（注释里的 lucide 版本号）、`website/scripts/smoke-ui.mjs`（复审新增的可重复浏览器冒烟）。
