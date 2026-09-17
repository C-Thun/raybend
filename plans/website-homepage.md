# 官网首页（静态站）+ GitHub Actions 自动发布

> 工作单元：`website/`（官网）。**与 raybend 应用本体是两套东西**（技术栈、纪律都不同 —— 见根 `AGENTS.md` §4）。
> 计划文件：`plans/website-homepage.md` ｜ 状态：**待评审**

---

## Context

raybend 需要一个**官方落地页**当门面：一眼知道这是什么软件、能干什么、去哪下载。需求来自用户 2026-09-17 口述：

- **只做一个首页**（不是说明书、不是文档站）。版式常规但观感要**时尚、开朗有活力、简约、平面化**。
- **中英双语**：识别浏览器语言自动选择，左上角可手动切换。
- **hero 贴顶**：顶部不额外给白条/深色条；banner 就是 hero 的一部分，底色用**有活力的辅色**。
- **下载区**先「空着做出样子」，以后与发版同步：发版打标签 → 由标签算出下载地址 → 链到主页并展示版本号。
- **图片素材缺三块**：界面截图、样张/AI 美化图 —— 先放占位并**注明 AI 生成提示词**，之后用户补。
- **以后要加**：B 站视频教程区块（录完再上）。
- **扩展性**：文档里前两个工作流（导入 / 浏览）写得全，后两个（整理与检索 / 导出）只有特色功能条目，未来补 ——
  版式与数据结构要能继续加，不要推倒重来。
- **自动发布**：GitHub Actions 在发版时自动构建并发布官网；用户要启用自定义域名 `raybend.cthun.com`。

**已核实的事实**（不是假设）：

| 事实 | 值 |
| --- | --- |
| remote | `git@github.com:C-Thun/raybend.git`（owner `C-Thun`） |
| 仓库可见性 | **public**（Pages 免费可用） |
| **默认分支** | **`master`**（不是 `main`） |
| 自定义域名 | `raybend.cthun.com` 是**子域** → 只需要一条 `CNAME`，`base` 保持 `/` |
| 现有产物形态 | `website` 已是 turnkey 客户端模式：`pnpm build` → **纯静态 `website/dist/client`** |
| 品牌色 | 主色 `#52C6AB`、辅色 `#F0B033`、浅面 `#F6F8F4`、深面 `#202226`（`DESIGN.md` §1.1） |
| 品牌素材 | `public/` 里的 name/slogan 是**白色墨迹 + 透明底**（实测墨迹 `#FFFFFF`、alpha=0）→ 可直接做 CSS mask |
| Pages 自定义域名 | 走 Actions 发布时**不需要 CNAME 文件**；域名只存在仓库设置里（GitHub 官方文档原文核实） |

**0（本轮已随手改掉）**：`ASSISTANCE.md` 的阻塞规则**不适用于 `website/`** —— 已写入根 `AGENTS.md`
（顶部 IMPORTANT 块 + §2.12）与 `website/AGENTS.md`：官网要人做的事**直接在对话里提**。

---

## 已定的决定（两轮问答的结果）

| # | 问题 | 决定 |
| --- | --- | --- |
| 1 | hero 配色 | **辅色琥珀 `#F0B033` 满铺** + 深墨 `#202226` 文字；青绿 `#52C6AB` 只做点缀（按钮、几何纹、卡片） |
| 2 | 下载区占位 | 主按钮 → **`https://github.com/C-Thun/raybend/releases`**，版本号位置显示「即将发布」；接上 release 后自动变真 |
| 3 | 字体 | **Inter + Noto Sans SC 都自托管**（`@fontsource-variable`，与应用本体一致，无外链） |
| 4 | 教程区 | **现在就有「视频教程 · 即将上线」占位块**（版式先立起来） |
| 5 | hero 主视觉 | **应用窗口式截图占位**（等用户截图）；**现成 splash 艺术图拿去做中段/下载区装饰** |
| 6 | 预发布版 | **只上正式版**（GitHub 的 `latest` 天然排除 prerelease）；beta 只在 GitHub 发布页可见 |
| 7 | 功能状态标签 | **不逐项标状态**；改为 hero/下载区**统一声明「早期开发中」** |
| 8 | 开源区块 | **要一条「开源与技术」整宽带**（AGPL-3.0 / GitHub / 本地优先 / Rust+Tauri+wgpu） |

---

## 页面结构

```text
┌───────────────────────────────────────────────────────────────┐
│ [logo] 光伴      功能 · 下载 · 教程                     中|EN  [⬇ 下载] │ ← 与 hero 同底色，贴顶、无分界
│                                                               │
│  光伴            ← 名称抠图（CSS mask 染深墨）                  │
│  你的理想摄影伴侣  ← 口号抠图（CSS mask 染深墨）                 │
│  一句话定位：本地优先的开源相片管理软件 …                        │
│  [ ■ 下载 Windows 版 ]  [ □ 在 GitHub 上看源码 ]                │
│  Windows 10/11 · 免费开源（AGPL-3.0）· 照片只留在你自己的硬盘      │
│                                        ┌────────────────────┐ │
│   早期开发中（小字声明）                 │ 应用窗口截图（占位）  │ │
│                                        └────────────────────┘ │
├───────────────────────────────────────────────────────────────┤
│ 三张卖点小卡：本地优先 / 快（内嵌预览 + 虚拟网格）/ 开源             │
├───────────────────────────────────────────────────────────────┤
│ 工作流总览：导入 → 浏览 → 整理与检索 → 导出（4 张卡，数据驱动）      │
├───────────────────────────────────────────────────────────────┤
│ 特性 A（左文右图）：导入与建库（模版 · 序号 · RAW 分流 · 内嵌预览）   │
├───────────────────────────────────────────────────────────────┤
│ 特性 B（右文左图）：浏览与评片（三列 · 时间分组 · 标记 · 筛选）       │
├───────────────────────────────────────────────────────────────┤
│ 特性 C（整宽带，splash 艺术图做背景）：键盘流 · 命令面板             │
├───────────────────────────────────────────────────────────────┤
│ 「开源与技术」整宽带：AGPL-3.0 · GitHub · 本地优先 · Rust/Tauri/wgpu │
├───────────────────────────────────────────────────────────────┤
│ 下载区（大卡，splash 艺术图点缀）：版本 / 平台 / 许可 / 直链 / 系统要求 │
├───────────────────────────────────────────────────────────────┤
│ 教程区：视频教程（B 站）· 即将上线                                 │
├───────────────────────────────────────────────────────────────┤
│ Footer：品牌 / 链接（GitHub · 发布页 · 许可）/ AGPL-3.0 / ©          │
└───────────────────────────────────────────────────────────────┘
```

版式上覆盖**三种常规功能版式**（左文右图 / 右文左图 / 整宽带）+ 卡片网格，全部由 `src/data/features.ts` 驱动。

---

## 技术方案

### 1. 纯静态、不走 SSR

- 沿用现有 turnkey 客户端模式（`website/vite.config.ts` 的 `solid({ start: true })`）→ `pnpm build` 产出
  `website/dist/client`（预渲染文档外壳 + 客户端渲染的页面，**不打开 `ssr: true`**）。
- 因此**语言检测没有 hydration 问题**：页面内容本来就是浏览器端渲染的。
- `base` 保持 `/`（自定义域名在根）。留一个 `RAYBEND_BASE` 环境变量口子：在自定义域名生效之前，
  想用 `https://c-thun.github.io/raybend/` 预览时临时设成 `/raybend/`。
- 路由：保留文件式路由，`src/routes/index.tsx` = 首页；`src/routes/[...404].tsx` 改成一张「页面不存在 → 回首页」的静态页。

### 2. i18n（不引第三方库）

- `src/i18n/index.ts`：`locale` signal + `t(key)` + `setLocale()`；文案**全部**在 `src/i18n/zh.ts` / `en.ts`。
- 首访判定：`navigator.languages` 里 `zh*` → 中文，否则英文；无 `navigator` 时回落中文。
- 选择写 `localStorage`（键 `raybend.locale`），下次直接生效。
- 切换时同步：`document.documentElement.lang`（`zh-CN` / `en`）+ `@solidjs/meta` 的 title/description。
- 控件：左上角 `中 | EN` 分段切换（样式与站点一致）。

### 3. 品牌素材怎么用（用户提示：透明底抠图 + CSS 变色）

- `name-cn/en.webp`、`slogan-cn/en.webp`：**实测是白色墨迹 + 透明底** → 用 `mask-image` + `background-color`
  渲染，颜色随设计走（hero 里染深墨，深色区可染白或青绿）。
- `logo.webp` / `logo-small.webp`：应用图标（深底圆角方块 + 青绿胶片带 + 琥珀字），**按原色**用（导航、下载卡、footer）。
- 落地在 `components/BrandMark.tsx`：`<BrandMark kind="name|slogan" />` 内部按当前语言选图 + 遮罩着色 + 尺寸。
- Safari 兼容：同时给 `mask-image` 与 `-webkit-mask-image`。

### 4. 配色与字体

- Tailwind v4 的 CSS-first 配置：在 `src/App.css` 用 `@theme` 定义
  `--color-brand`(`#52C6AB`) / `--color-accent`(`#F0B033`) / `--color-surface-*`(`#F6F8F4` / `#EDF0E9` / `#202226` / `#2A2D33`)，
  组件里**只引用令牌，不写死色值**。
- 字体自托管：`website` 新增依赖 `@fontsource-variable/inter`、`@fontsource-variable/noto-sans-sc`（与本体的 OFL 选型一致），
  在 `App.css` 里 import + 定义 `--font-sans`（拉丁在前、CJK 在后）。
- 观感基调：大留白、面相接不描边、圆角一致、动效极克制（只在 hero 与卡片 hover 用短过渡；不用毛玻璃/大阴影）。

### 5. 下载信息（与发版同步）

**构建期注入**，不在浏览器里请求 GitHub API（避免限流、避免闪烁、无 JS 也能显示版本）：

- `website/vite.config.ts` 加一个小插件，在构建开始时解析一次版本信息，`define` 注入 `__RB_RELEASE__`：
  1. `RAYBEND_TAG`（发布流程里由 `github.event.release.tag_name` 给出，如 `v0.1.0`）；
  2. 否则**仅在 CI 或显式 `RAYBEND_FETCH_RELEASE=1` 时**调 GitHub API `/repos/C-Thun/raybend/releases/latest`
     （带 `GITHUB_TOKEN` 时用它；5s 超时；失败只告警不失败构建）；
  3. 都没有 → `null`（页面走「即将发布」占位态）。
- **纯逻辑单独成模块** `src/data/release.ts`：把 `{ tag, assets }`（或 `null`）算成
  `{ state: 'available' | 'pending', version, downloadUrl, releaseUrl, sizeLabel?, fileName? }`。
  选择顺序：`*.exe`（NSIS 安装包）→ `*.msi` → `*.zip` → 退化为 release 页；`prerelease` 一律忽略。
- **单测** `src/data/release.test.ts` 覆盖：正常 tag + exe 资产 / 只有 zip / 无资产 → release 页 /
  `null` → pending / 非法 tag / prerelease 被忽略。
- 页面两处用它：hero 主按钮 + 下载区大卡（版本号、文件名、大小、平台、系统要求）。

### 6. 占位图与提示词

- `components/ImagePlaceholder.tsx`：统一渲染「版式正确的占位块」——品牌色浅底 + 胶片/光圈几何纹 +
  角标文案（开发期可见），尺寸/圆角与最终图一致，**换真图不动版式**。
- 占位数据在 `src/data/media.ts`：每项含 `id / 用途 / 建议尺寸 / 来源（截图 | AI 生图 | 已有素材）/ prompt / alt`。
- 集中产物 `website/ASSETS.md`：给用户的**素材清单**（哪些要截图、怎么取景；哪些要 AI 生图、提示词原文，
  英文 prompt + 中文说明 + 负面词 + 风格锚点 `#52C6AB`/`#F0B033`/平面插画风/无文字无 logo）。
- 需要 AI 生的（预计很少）：整理与检索、导出这两个未来模块的概念图；纯美化的背景/装饰图。
  样张类用户自己有大量素材，走「用户提供」。

### 7. 扩展性（后两个模块）

- `src/data/features.ts` 的条目结构固定：`{ id, icon, titleKey, bodyKey, media, layout }`；
  **加一条数据 = 多一块版式**，工作流总览的 4 张卡也是同一份数据（`workflows` 数组）。
- M3 / M4 未来细化时只补文案键与截图，不动组件。
- 教程区 `src/data/tutorials.ts`：现在是占位状态（`state: 'coming-soon'`），录完教程后填 B 站条目即自动成真。

---

## 文件清单

**新增**

| 文件 | 作用 |
| --- | --- |
| `.github/workflows/website.yml` | 构建 + 发布 Pages（release / push / 手动） |
| `website/ASSETS.md` | 素材清单：截图取景说明 + AI 生图提示词 |
| `website/src/i18n/{index.ts,zh.ts,en.ts}` | 语言检测 / 切换 / 全站文案 |
| `website/src/data/{features.ts,media.ts,tutorials.ts,release.ts,release.test.ts}` | 站点数据与下载逻辑（+ 单测） |
| `website/src/components/{BrandMark.tsx,LangSwitch.tsx,ImagePlaceholder.tsx,Button.tsx,Card.tsx}` | 通用件 |
| `website/src/sections/{Hero.tsx,Highlights.tsx,Workflows.tsx,FeatureRows.tsx,OpenSource.tsx,DownloadSection.tsx,Tutorials.tsx,Footer.tsx}` | 页面区块 |
| `website/public/robots.txt`、`website/public/sitemap.xml` | 基础 SEO（单页） |

**改动**

| 文件 | 改什么 |
| --- | --- |
| `website/src/App.tsx` | 从模板演示改成站点布局（Nav + 各区块 + Footer），并接 i18n |
| `website/src/App.css` | `@theme` 品牌令牌 + 字体 import + 少量基础样式 |
| `website/src/Document.tsx` | 静态 head：title / description / og / theme-color / favicon / 默认 `lang="zh-CN"` |
| `website/src/routes/index.tsx` | 首页（渲染 sections） |
| `website/src/routes/[...404].tsx` | 改成站点风格 404 |
| `website/vite.config.ts` | 版本注入插件 + `RAYBEND_BASE` 支持 |
| `website/src/vite-env.d.ts` | 注入量的类型声明 |
| `website/package.json` | 补 `packageManager: pnpm@12.3.4`、`typecheck` / `test:run` 脚本、两个字体依赖 |
| `website/AGENTS.md` | 站点结构、i18n 约定、占位图与素材约定、下发版流程、字体与配色落地位置 |
| 根 `.gitignore` + `website/.gitignore` | 加 `*:Zone.Identifier`（Windows 附加流残留） |
| 根 `AGENTS.md` / `PLAN.md` | 登记这条支线 |

**删除（模板残留）**

`website/src/routes/users.tsx`、`website/src/routes/users/[id].tsx`、`website/src/components/Counter.tsx`（+ `Counter.test.tsx`）、
`website/src/logo.svg`、`website/public/users.json`、`website/public/*:Zone.Identifier`。

---

## Reuse（复用现有的东西，别新造）

| 需要 | 已有的 |
| --- | --- |
| 静态构建 + 文件式路由 + 预渲染外壳 | `website/vite.config.ts`、`website/src/router.ts`、`Document.tsx` |
| 品牌色/字体选型 | `DESIGN.md` §1.1 / §7.1 |
| 产品与技术文案 | `README.md`、`PLAN.md`（M1–M4）、`BROWSE.md` §0–§3、`REPOSITORY.md` §3–§4、`AGENTS.md` §7.4 |
| 品牌素材 | `website/public/{logo*,name-*,slogan-*,splash_v1-*}` |
| 测试基建 | `website` 已有 Vitest + jsdom + `@solidjs/testing-library` + `@solidjs/diagnostics` |
| 标签约定 | `scripts/release-plan.ts`：tag 形如 `v<version>`（人类执行 tag/push） |

---

## 发布流水线（`.github/workflows/website.yml` 草案）

```yaml
name: website
on:
  push:
    branches: [master]                      # ← 本仓默认分支是 master
    paths: ['website/**', '.github/workflows/website.yml']
  release:
    types: [published]                      # 发版即刷新官网（版本号 + 下载地址）
  workflow_dispatch:

permissions:
  contents: read
  pages: write
  id-token: write

concurrency:
  group: pages
  cancel-in-progress: false

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
        # release 事件默认检出 tag；官网内容要最新的 → 显式检出默认分支
        with: { ref: master }
      - uses: pnpm/action-setup@v6            # 版本从 website/package.json 的 packageManager 读
      - uses: actions/setup-node@v7
        with:
          node-version: 24
          cache: pnpm
          cache-dependency-path: website/pnpm-lock.yaml
      - run: pnpm install --frozen-lockfile
        working-directory: website
      - run: pnpm test:run && pnpm typecheck && pnpm lint
        working-directory: website
      - run: pnpm build
        working-directory: website
        env:
          RAYBEND_TAG: ${{ github.event.release.tag_name }}   # 发版事件里是 vX.Y.Z
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
      - uses: actions/configure-pages@v6
        with: { enablement: true }            # 尝试自动开启 Pages（失败则按下面步骤手开）
      - uses: actions/upload-pages-artifact@v5
        with: { path: website/dist/client }

  deploy:
    needs: build
    runs-on: ubuntu-latest
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    steps:
      - id: deployment
        uses: actions/deploy-pages@v5
```

**纪律对齐**：这套自动化**只发布官网**，不 push、不打 tag、不产安装包 —— 发版/推送仍由人类执行（`AGENTS.md` §2.1）。
触发源也都是人的动作（推 master / 发布 release / 手动点）。

**版本语义**：`release: published` 事件只在**正式发布**时带 `RAYBEND_TAG`；
prerelease 事件会被忽略（`if: github.event.release.prerelease == false`），页面继续显示 `latest` 的版本。

---

## 人类要做的事（Pages + 自定义域名）

> 这段会同时写进 `website/AGENTS.md`，方便以后照抄。**顺序很重要**：先 GitHub 后 DNS。

1. **打开 Pages**：仓库 → `Settings` → `Pages` → `Build and deployment` → `Source` 选 **GitHub Actions**。
   （若上面的 `enablement: true` 生效，这一步已自动完成。）
2. **填自定义域名**：同一页 `Custom domain` 填 `raybend.cthun.com` → `Save`。
   ⚠️ GitHub 官方要求**先在仓库设置里填域名、再去 DNS 配置** —— 反过来有被别人抢注子域的风险。
   ⚠️ 走 Actions 发布时**不需要** `CNAME` 文件（官方原文：*"any existing CNAME file is ignored and is not required"*）。
3. **配 DNS**（在 `cthun.com` 的 DNS 服务商处，新增一条记录）：

   | 类型 | 名称 / 主机 | 值 | 说明 |
   | --- | --- | --- | --- |
   | `CNAME` | `raybend` | `c-thun.github.io` | 指向**账号**的默认域，**不要**带仓库名，**不要**填 `*.pages.github.io` |

   - 别把这条 CNAME 指到 apex（`cthun.com`）—— 官方明确说会导致 HTTPS 强制失败。
   - 若用 Cloudflare：配置期请设 **DNS only（灰云）**，否则 GitHub 签证书可能失败；稳定后再决定是否开代理。
4. 回到 `Settings → Pages` 等 DNS 检查通过（绿色 ✓），然后勾上 **`Enforce HTTPS`**。
   证书自动签发，通常几分钟，最多 24 小时；DNS 本身也可能要等最多 24 小时生效。
5. 验证：`dig raybend.cthun.com +nostats +nocomments +nocmd`（Windows 用 `Resolve-DnsName`）应看到
   `CNAME → c-thun.github.io`；然后开 `https://raybend.cthun.com/` 看首页。

> 域名生效前，`https://c-thun.github.io/raybend/` 这条默认地址会因为 `base=/` 而样式错乱 ——
> 想临时预览就用 `RAYBEND_BASE=/raybend/ pnpm build`。域名生效后 GitHub 会把默认地址重定向到自定义域名。

---

## Steps

- [x] 0. `ASSISTANCE.md` 豁免规则写入根 `AGENTS.md` + `website/AGENTS.md`（**本轮已改**）
- [x] 1. 清理模板残留（users 路由 / Counter / logo.svg / users.json / `*:Zone.Identifier`）+ 两处 `.gitignore`
- [x] 2. `App.css` 品牌令牌与字体；`Document.tsx` 静态 head；`base` 支持
- [x] 3. i18n（检测 / 持久化 / `<html lang>` / meta 同步）+ 中英文案初稿
- [x] 4. 数据层：`features.ts` / `media.ts`（含占位提示词）/ `tutorials.ts`
- [x] 5. 通用组件：`BrandMark`（mask 染色）、`LangSwitch`、`Button`、`Card`、`ImagePlaceholder`
- [x] 6. Hero（贴顶琥珀底 + 名称/口号遮图 + 一句话 + 双 CTA + 截图占位 + 早期开发声明）
- [x] 7. 三卖点卡 + 工作流总览（导入 / 浏览 / 整理与检索 / 导出）
- [x] 8. 三条特性版式（左文右图 / 右文左图 / 整宽带，用 splash 做背景）
- [x] 9. 「开源与技术」整宽带
- [x] 10. 下载区（版本 / 平台 / 许可 / 直链 / 系统要求 / 占位态）
- [x] 11. 教程区占位 + Footer + 404 页
- [x] 12. `release.ts` 纯逻辑 + 单测；`vite.config.ts` 版本注入
- [x] 13. 其它单测（i18n 判定与切换、占位块、下载态渲染）
- [x] 14. `.github/workflows/website.yml`
- [x] 15. `website/ASSETS.md`（截图清单 + AI 提示词 + 尺寸表）
- [x] 16. 文档：`website/AGENTS.md`（结构/i18n/素材/发版）、根 `AGENTS.md` 与 `PLAN.md` 登记、`implementations/` 实施记录
- [x] 17. 本地验收（见下）

---

## Verification

**Agent 侧（冒烟，可自动化）**

1. `cd website && pnpm install && npx tsc --noEmit && pnpm lint && pnpm test:run` 全绿、秒级。
2. `pnpm build` → `website/dist/client/index.html` 与 `assets/` 存在；**无外部 CDN 请求**（字体/图片全本地）；
   记录首屏 JS/CSS/字体分块的体积。
3. `pnpm serve` 起静态服务：用脚本/浏览器工具核对关键文案、`中|EN` 切换、下载按钮 href 与版本占位态。
4. 注入干跑：`RAYBEND_TAG=v9.9.9 pnpm build` → 产物里出现 `9.9.9` 与对应 release 链接；不设变量 → 占位态。
5. 退化：JS 未加载时页面不出现「半截错乱」；`<html lang>`、title、description 有静态默认值。
6. 窄屏自检（浏览器工具取几个宽度截图，检查有无横向溢出/文字重叠）。

**人类侧（我不代劳，做完提醒你）**

7. 目视观感（配色、留白、动效克制）、中英两版、真机窄屏。
8. 域名配好后看 `https://raybend.cthun.com/` + HTTPS 是否可用。
9. 以后第一次真正发版时，确认官网版本号与下载链接**自动**跟着变。

---

## 风险与注意

| 风险 | 处置 |
| --- | --- |
| 页面是客户端渲染，SEO 弱 | 静态 head 给默认 title/description/og；文案不依赖图片。若以后要强 SEO，再评估开启预渲染（用户已明确不走 SSR） |
| 无 release 时链接无效 | 主按钮指向 release 列表页（永远打得开），版本位显示「即将发布」 |
| 素材未到位 | 占位块与最终版式同尺寸，替换即完成；`ASSETS.md` 里给全提示词 |
| Noto Sans SC 体积 | 用 `@fontsource-variable`（按 unicode-range 分块，浏览器只取所需块）；构建后核对实际体积 |
| 官方站点与本体文档串味 | 站点文案只讲**能力**，不抄内部术语（`flowbar` 这类词不出现在官网上） |

---

## 以后再说（不阻塞本次）

- B 站教程数据接入（录完再填 `tutorials.ts`）。
- 路线图 / 更新日志区块。
- 英文站独立 URL 或 i18n 路由（现在是单 URL + JS 切换）。
- 下载加速镜像 / CDN（如国内访问慢再评估）。
- 站点自身的 OG 封面设计（先用 logo 或脚本合成一张占位）。
