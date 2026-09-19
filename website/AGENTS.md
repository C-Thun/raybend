# website — raybend 官方站点（AGENTS.md）

> 本目录是 **raybend（光伴）的官方站点 / 官网**源码，**不是应用本体**。
> 应用本体在仓库根：`src/` + `src-tauri/` + `crates/`（Solid 1.9 + Tauri 2 + Ark UI + Tabler）。
> 两者技术栈、依赖、构建与发布流程都不同 —— **在这里工作不要参照根目录的选型**（对照表见根 `AGENTS.md` §4）。
>
> **官网需要人做的事**（图片素材 / 截图 / 域名与 Pages 配置等）**直接在对话里提** ——
> 全项目统一（2026-09-19 起），不再有「待人类协助清单」这类文件。

建立时间：2026-09-17 13:17:05 CST ｜ 首页与发布流水线落地：2026-09-17

---

## 1. 技术选型

| 层 | 选型 | 实测版本 |
| --- | --- | --- |
| 元框架 | **SolidStart 2.0** —— 「turnkey 静态客户端」形态：`@solidjs/vite-plugin` 开 `start: true` + `filesystem-routing` 文件式路由；**未安装** `@solidjs/start` 单体包 | vite-plugin `3.0.0-next.38`、filesystem-routing `0.2.1` |
| UI 框架 | **SolidJS 2.0**（`solid-js` / `@solidjs/web`）—— 2.0 线，当前为 RC | `2.0.0-rc.6` |
| 路由 | `@solidjs/router`（约定式路由，页面在 `src/routes/`） | `2.0.0-next.21` |
| head 标签 | `@solidjs/meta`（运行时覆盖静态 head） | `1.0.0-next.2` |
| 样式 | **Tailwind CSS v4**（`@tailwindcss/vite`）。CSS-first：**没有** `tailwind.config.js`，令牌写在 `src/App.css` 的 `@theme` 里 | `4.3.3` |
| 图标 | **Lucide** —— 只取图标数据，生成本站自己的组件（见 §5.5） | `lucide-solid` `1.46.0`（**仅开发期**） |
| 字体 | **Inter + Noto Sans SC**，`@fontsource-variable` 自托管（无外链） | 各 `5.3.0` |
| 构建 | Vite 8 | `8.2.2` |
| 语言 | TypeScript（`strict`、`allowImportingTsExtensions`） | `5.9.3` |
| 测试 | Vitest 5 + jsdom（`vitest-setup.ts`；`@solidjs/testing-library` 用来写组件测试） | `5.0.0` |
| Lint | oxlint + `eslint-plugin-solid` 的 **v2 规则集**（见 `oxlint.config.ts`） | oxlint `1.79.0` / plugin `0.18.0` |
| 反应式诊断 | `@solidjs/diagnostics`（dev 专用，构建时 no-op） | `2.0.0-rc.6` |
| 运行环境 | Node ≥ 22.12（`@solidjs/vite-plugin` 的 engines 要求）；本机实测 Node `v26.5.0` + pnpm `12.3.4`；CI 用 Node 24 | — |

**站点形态**：纯静态。`pnpm build` 预渲染文档外壳到 `dist/client/index.html`，页面内容在浏览器端渲染
（**不打开 `ssr: true`**，用户明确要求）；整份产物零服务端依赖，直接丢静态托管即可。

---

## 2. 目录与约定

```text
website/
├── vite.config.ts        # solid({start:true}) + fileRoutes() + tailwindcss() + 构建期注入发布信息
├── ASSETS.md             # 素材清单：要截的图 / 要 AI 生的图（给人看的）
├── scripts/
│   ├── generate-icons.mjs # 从 lucide-solid 的图标数据生成 src/components/icons.tsx
│   └── generate-marks.mjs # 用 potrace 把品牌书法字描摹成 src/components/mark-paths.ts
├── src/
│   ├── App.tsx           # 站点外壳：吸顶导航 + <Loading> 包裹的路由内容 + 页脚
│   ├── router.ts         # createRouter(fileRoutes(pageRoutes))
│   ├── Document.tsx      # 文档外壳（「新的 index.html」）——**静态 head / SEO / JSON-LD 都在这**
│   ├── App.css           # 唯一样式入口：字体 import + @import 'tailwindcss' + @theme 令牌
│   ├── i18n/             # zh.ts（形状源）/ en.ts / index.ts（检测·切换·持久化）
│   ├── data/             # site.ts（外链常量）/ features.ts（结构）/ media.ts（素材+提示词）
│   │                     # tutorials.ts（B 站，当前空）/ release.ts（下载信息纯逻辑 + 单测）
│   ├── components/       # Button / BrandMark / Shot / WindowFrame / LangSwitch / Section
│   │                     # icons.tsx（生成物）/ mark-paths.ts（生成物）
│   ├── sections/         # Hero / Highlights / Workflows / Features / OpenSource / Download / Tutorials / Footer
│   ├── routes/           # index.tsx（首页）/ [...404].tsx
│   └── vite-env.d.ts     # 声明构建期注入的 import.meta.env.RB_RELEASE
├── marks/                # 品牌字：source/（描摹源位图）+ *.svg（矢量成果）+ preview.html（生成物）
├── public/               # 会原样拷贝进产物的静态资源（logo / favicon / 未来的截图与装饰图）
└── file-routes.d.ts      # 生成物，「Do not edit」
```

约定要点：

- **加一个页面 = 加一个文件**：放进 `src/routes/`，路径即 URL；`file-routes.d.ts` 由插件重写，别手改。
- **Tailwind v4 靠扫描源码里的完整类名**：类名必须以完整字符串出现，运行时拼字符串扫不到。
- **Solid 2 的 `class` 支持数组/对象**：`class={['base', active() && 'active']}` —— 比模板字符串更精确（本项目已统一成这种写法）。
- 产物 `dist/`、`node_modules/`、Windows 附加流残留 `*:Zone.Identifier` 都已在 `.gitignore` 里。

---

## 3. 命令（都在本目录里跑）

```bash
pnpm install         # 依赖各自独立，必须在这里装（根目录 pnpm install 不会装它）
pnpm dev             # 开发服务器 http://localhost:3000
pnpm build           # 生产构建 → dist/client （纯静态）
pnpm serve           # 本地预览构建产物
pnpm typecheck       # tsc --noEmit
pnpm test            # Vitest（watch）
pnpm test:run        # Vitest（跑一次，CI 用）
pnpm lint            # oxlint src
pnpm icons:generate  # 重新生成 src/components/icons.tsx
pnpm marks:generate  # 重新描摹品牌书法字 → src/components/mark-paths.ts
```

`RAYBEND_BASE=/raybend/ pnpm build` 可以产出「挂在 `https://c-thun.github.io/raybend/` 子路径下」的版本
（自定义域名生效前用它预览）。

---

## 4. 站点实现约定

### 4.1 文案与 i18n

- **文案只有一个来源**：`src/i18n/zh.ts` 是形状的**唯一事实来源**，`en.ts` 声明为 `Dictionary` 类型 ——
  少一个键就是编译错误，不会漏译。
- 语言判定：`navigator.languages`（中文 → `zh`，其它 → `en`；拿不到语言信息回落 `zh`），
  手动选择存 `localStorage('raybend.locale')`；切换时同步 `<html lang>` 与 `@solidjs/meta` 的 title/description。
- 页面里**不要写死中文或英文**，一律走 `messages()`。
- 写作口径：讲能力、不讲内部术语（`flowbar` / `catalog` 这类词不上官网）；不承诺日期。
- **英文产品名一律 `RayBend`**（根 `AGENTS.md` §1 的命名规则）：页面文案、`<title>`/meta、og、JSON-LD、
  footer 版权行、窗口框标题栏都是 `RayBend`；小写 `raybend` **只**允许出现在 URL / 仓库名 / 存储键里。

### 4.2 素材与占位（**当前图片还没到位**）

- 素材表在 `src/data/media.ts`：每个条目有 `source`（截图 / AI 图 / 已有素材）、建议尺寸、`hint`、`prompt`。
- **没有 `src` 的素材渲染成「版式正确的占位块」**（`components/Shot.tsx`）：与最终图同宽高比、同圆角，
  悬停能看到提示词，角标写着「待补」。**填上 `src` 即完成替换，版式不动。**
- 要给人的清单（截什么图、AI 提示词原文）在 **`ASSETS.md`**。
- 品牌图（`logo*` / `splash_v1-*`）在 `public/`；书法字的**源位图**在 `marks/source/`（不进产物）——见 `ASSETS.md` §1。

### 4.3 品牌字：矢量化 + 白色外扩边（**不是位图遮罩**）

`marks/source/name-*.webp` / `slogan-*.webp` 是「白色墨迹 + 透明底」的**源素材**（放 `marks/` 而不是 `public/`，是因为它只服务构建、不该进产物）；hero 上渲染的不是它们，
而是 `scripts/generate-marks.mjs` 用 potrace 描摹出来的**矢量路径**（`src/components/mark-paths.ts`），
由 `components/BrandMark.tsx` 渲染成「绿色内芯 + 白色外扩边」。

**为什么不用位图遮罩**：`mask-image` 只能给墨迹本身上色，**做不到向外扩张的白边**；
`drop-shadow` 叠出来只是模糊光晕，`feMorphology` 在部分浏览器上不可靠。矢量路径就简单了：

```text
fill=var(--color-brand) + stroke=var(--color-white) + paint-order="stroke"
```

描边是**居中**画的，`paint-order` 让描边压在填充下面 ⇒ 只剩向外那一半 = 外扩边；
`vector-effect="non-scaling-stroke"` 让边宽以**屏幕像素**计（`outline` prop 就是它）。

- 改了源素材要重跑：`pnpm marks:generate`（生成物 4 条路径共 ~125 KB，压缩后小得多）。
- 生成前的 `-blur 0x3` 是关键：不磨掉毛笔飞白/扫描噪点，路径会大 5 倍（实测 97 KB → 19 KB）；
  二值化阈值用 **60%**（不是默认 50%）抵消模糊带来的笔画肿胀。
- ⚠️ **渲染必须带 `fill-rule="evenodd"`**：potrace 把孔洞输出成**绕向相反的子路径**，
  用默认的 `nonzero` 会把「光/伴/RayBend」的封闭部件填实（已踩过，写在 `App.css` 的 `.rb-mark-path` 里）。
- **保真度实测**（按正确的 fill-rule 把矢量光栅化回源尺寸后逐像素比）：
  **IoU 97.9%～98.8%**，**遗漏 ≤ 0.6%**（细尖几乎不丢），**多出 1.0%～1.6%**
  （细笔画略粗，来自「模糊降噪 + 二值化」）。想更保真可以：减模糊 / 换 `vtracer`；
  或者干脆不用矢量（见下）。
- **另一种做法**（知道就好，不必改）：位图 + 内联 SVG 的 `feMorphology` 膨胀 alpha 做白边 ——
  更简单、更保真，但缩放会糊、且各浏览器对 filter 的边界处理不一样；
  我们选了矢量，因为官网要无限缩放、要控描边宽度、以后可能换色/做路径动画。
- `potrace` 是 **GPL-2.0 的开发期工具**，只产坐标、不进产物；许可登记见根 `THIRD-PARTY-NOTICES.md` §1c。

### 4.4 下载信息（与发版同步）

- **不在浏览器里请求 GitHub API**：版本号与下载直链是**构建期注入**的
  `import.meta.env.RB_RELEASE`（`vite.config.ts` 里解析一次，`define` 进产物）。
- 解析优先级：`RAYBEND_TAG`（发布流程从 `github.event.release.tag_name` 传入）→ CI 里查
  `releases/latest`（带 `GITHUB_TOKEN`）→ 都没有就注入 `null`（页面走「即将发布」占位态）。
  本地 `pnpm build` **不联网**。
- 纯逻辑在 `src/data/release.ts`（`resolveRelease()`：选安装包 → 退化为发布页，忽略 prerelease，
  校验 tag 形状），单测在 `release.test.ts`。**页面只消费它的结果**，不要在组件里写解析逻辑。
- 预发布版（prerelease）**不上官网**。

### 4.5 图标（Lucide，但不直接用 `lucide-solid`）

`lucide-solid` 1.x 的组件从 `solid-js` import `splitProps`，**Solid 2 已经没有这个 API**（连 `mergeProps`
都搬去了 `@solidjs/web`），直接用会在浏览器里报 `does not provide an export named 'splitProps'`；
官方也还没有支持 Solid 2 的版本（最新 `1.46.0` 的 peer 仍是 `solid-js: ^1.4.7`）。

所以 `scripts/generate-icons.mjs` **只取 Lucide 的图标数据**（ISC 许可），生成本站自己的
`src/components/icons.tsx`（Solid 2 友好、产物里只留用到的十几个图标）。加图标：改脚本里的
`ICONS` 清单 → `pnpm icons:generate`。许可登记见根 `THIRD-PARTY-NOTICES.md` §1c。

> 注意生成物里的 SVG 属性是 **kebab-case**（`stroke-width`）：Solid 2 的 JSX 类型只认这种写法，
> 写成驼峰会 `TS2322`。那几行带 `pi-lens-ignore` 注释，是**刻意的**，不要「顺手修掉」。

### 4.6 SEO

静态 head 在 `Document.tsx`（title / description / keywords / canonical / og / twitter / theme-color /
JSON-LD `SoftwareApplication`）—— 这是**爬虫唯一能直接读到的东西**（页面内容是客户端渲染的）。
运行时由 `@solidjs/meta` 按语言覆盖 title 与 description。`robots.txt` 与 `sitemap.xml` 在 `public/`。

---

## 5. 部署（GitHub Pages + 自定义域名）

**流水线**：`.github/workflows/website.yml`

| 触发 | 做什么 |
| --- | --- |
| push 到 `master`（只改 `website/**` 或该 workflow 文件时） | 构建 + 发布 |
| **正式版 release 发布**（`release: published`，prerelease 跳过） | 重新构建：把新版本号与安装包直链刷上官网 |
| `workflow_dispatch`（可填 `tag` 覆盖版本号） | 手动重建 |

流程是 `pnpm install --frozen-lockfile` → `test:run` + `typecheck` + `lint` → `pnpm build`（注入版本）
→ `actions/configure-pages@v6`（`enablement: true`，尝试自动开 Pages）→ `upload-pages-artifact@v5`（路径
`website/dist/client`）→ `actions/deploy-pages@v5`。

> 这套自动化**只发布官网**：不 push、不打 tag、不产安装包（根 `AGENTS.md` §2.1 仍然成立）。

### 一次性配置（**由人做**，顺序很重要）

1. **打开 Pages**：仓库 → `Settings` → `Pages` → `Build and deployment` →
   `Source` 选 **GitHub Actions**。（工作流里的 `enablement: true` 若能自动开，这步已省。）

   > ⚠️ **`Source` 不能停在 `Deploy from a branch`**（那是启用 Pages 时的默认值）。
   > 分支发布模式下 `actions/deploy-pages` 会直接失败，日志里是
   > `Error: Get Pages site failed… Please verify that the repository has Pages enabled and
   > configured to build using GitHub Actions`（或 `Failed to create deployment (status: 400) …
   > Deployments are only allowed from gh-pages`）。第一次部署遇到这类报错，先看这个开关。
2. **先填自定义域名**：同一页 `Custom domain` 填 `raybend.cthun.com` → `Save`。
   ⚠️ GitHub 官方要求**先在仓库设置里填域名、再去配 DNS**（反了会有子域被抢注的风险）。
   ⚠️ 发布来源是 Actions 时**不需要 `CNAME` 文件**（官方原文：*"any existing CNAME file is ignored
   and is not required"*），域名只存在仓库设置里。
3. **再配 DNS**（在 `cthun.com` 的 DNS 服务商处加一条记录）：

   | 类型 | 名称 / 主机 | 值 |
   | --- | --- | --- |
   | `CNAME` | `raybend` | `c-thun.github.io` |

   - 指向**账号**的默认域，**不要**带仓库名，**不要**填设置页里那个 `*.pages.github.io` 子域。
   - 不要把这条 CNAME 指向 apex（`cthun.com`）—— 官方明确说会导致 HTTPS 强制失败。
   - 用 Cloudflare 的话，配置期请设 **DNS only（灰云）**，否则证书签发可能失败。
4. 回 `Settings → Pages` 等 DNS 检查变绿，然后勾上 **`Enforce HTTPS`**（证书自动签发，通常几分钟，
   最多 24 小时；DNS 本身也可能要等最多 24 小时）。
5. 验证：`dig raybend.cthun.com +nostats +nocomments +nocmd`（Windows 用 `Resolve-DnsName`）
   应看到 `CNAME → c-thun.github.io`；然后开 <https://raybend.cthun.com/>。

> 域名生效前，`https://c-thun.github.io/raybend/` 会因为 `base=/` 而样式错乱 ——
> 想临时预览就用 `RAYBEND_BASE=/raybend/ pnpm build`。域名生效后 GitHub 会把默认地址重定向过去。

**产物体积须知**：`dist/client` 约 6 MB，其中 ~5.6 MB 是**自托管字体的全量分块**
（Inter 1.2 MB / Noto Sans SC 4.6 MB）。这是**部署体积**，不是访问体积 —— `@font-face` 带
`unicode-range`，浏览器只下载页面真正用到的分块（实测首屏几百分 KB）。为了这点体积去手写
子集 `@font-face` 不值得，但如果你以后在意 Pages 上传量，可从 `@fontsource-variable/*` 里挑子集自己拼。

---

## 6. 边界与硬规矩

1. **不跨边界引用**：这里**不要** import 仓库根 `src/` 的任何东西（那边是 Solid 1.9 + Tauri 语境），
   本体也不要 import 这里。两套 Solid 版本混用必炸。
2. **依赖只落在本目录**：给官网装包用这里的 `package.json` / `pnpm-lock.yaml`；**不要**为了官网去改根
   `package.json`，也不要把官网依赖加进本体。
3. **保持「零服务端依赖」**：产物要能直接丢到任意静态托管上。
4. **组件与图标自成一套**：图标走 §4.5 的生成流程；**不要**把本体的 Tabler / Ark UI 引进来（反之亦然）。
   需要复杂交互控件时，倾向原生元素 + Tailwind 自己写。
5. **文案不写死**：见 §4.1。根目录的 `pnpm lint:i18n` **只管应用本体**，不检查这里 —— 靠约定和 review。

---

## 7. 现状与待办（截至 2026-09-17）

- ✅ 首页七段 + 双语 + 下载区 + 教程占位 + 404 + 发布流水线 + 单测（20 项）。
- ⏳ **图片素材没到位**：4 张应用截图 + 2 张装饰图（可选）。清单与提示词见 `ASSETS.md`；
  填进 `src/data/media.ts` 即完成替换。**不阻塞任何开发。**
- ⏳ **Pages 与自定义域名**：见 §5 的一次性配置（要人做）。
- ⏳ **B 站教程**：录完把条目加进 `src/data/tutorials.ts`，教程区自动从「录制中」变成视频卡片。
- ⏳ 以后要加的图（整理/检索、导出模块的截图）等那两个模块做出来再说 —— 官网只展示已经有的东西。
- 深链接：产物是「预渲染外壳 + 客户端路由」，目前只有 `/` 一个页面，所以不成问题；将来加多页时
  要在 Pages 上处理 404 回退。

---

## 8. Solid 2 反应式纪律

这是 SolidJS 2.x 项目。Solid 不是 React：**组件只跑一次**（没有 re-render），响应式是细粒度的 signal，effect/memo 的语义也是 Solid 特有的。**不要移植 React 的写法。**（Solid 1 的心智也不能直接照搬：所有权、清理与写入调度在 2.0 里都动过。）

**三条最容易踩的**（都是本站实测踩过的）：

1. **信号写入是批量的**：`setSignal(v)` / `setStore(...)` 之后**立刻读**还是旧值；测试里必须
   `await flush()`（`flush` 从 `solid-js` 导入）才能看到新值。组件事件处理器里不用管（Solid 自己会 flush）。
2. **`splitProps` / `mergeProps` 已不在 `solid-js`**（后者在 `@solidjs/web`）；`onMount` **改名为 `onSettled`**。
   为 Solid 1 编译的库（如 `lucide-solid`）因此不能直接用 —— 见 §4.5。
3. **路由内容是异步加载的**，必须用 `<Loading>` 边界包一层；没有它，Solid 会把整个根挂载推迟到异步就绪。

### 版本对齐的技能文档（在 `node_modules` 里，按需读）

安装的包里带着与其**确切版本**配套的 agent 技能文档：

- `node_modules/solid-js/skills/reactivity-diagnostics/SKILL.md` —— 修复指南，把每个 dev 模式的诊断码（如 `REACTIVE_WRITE_IN_OWNED_SCOPE`、`STRICT_READ_UNTRACKED`、`FLUSH_IN_EFFECT_CALLBACK`）映射到规定的修法。测试输出或浏览器控制台里一出现 Solid 诊断码，就读它。
- `node_modules/@solidjs/diagnostics/skills/agent-loops/SKILL.md` —— 怎么采集反应式证据（哪些 scope 重跑了、为什么重跑、白跑的计算、开销表），并在测试里与真实页面上断言预算。

### 反应式问题：先取证，别猜

- **在测试里**：`@solidjs/diagnostics` 的 `captureArtifact()` 包住一个场景，返回可序列化的诊断 + 重跑归因产物；`@solidjs/diagnostics/vitest` 的匹配器（`toHaveNoDiagnostics`、`toStayWithinRerunBudget`、`toHaveNoWaste` …）对它做断言。**不需要浏览器**。
- **对运行中的 dev server**（`vite.config.ts` 里 `diagnostics: true`；仅 dev 生效，构建时是 no-op）。需要一个打开的页面连着 dev server（例如通过浏览器工具）：
  - `GET /__solid/diagnostics` —— 状态与已连接客户端数量
  - `POST /__solid/diagnostics`，JSON `{"method":"begin"}` 后再 `{"method":"end"}` —— 采集一个会话成产物
  - `{"method":"whyDidRun","params":{"name":"<scope name>"}}` —— 该命名 scope 在当前会话里记录到的重跑
  - `{"method":"costs"}` —— 当前会话的开销表

给 signal / memo / effect **起名字**（`{ name: "..." }` 选项）—— 归因报告是按名字报 scope 的。
