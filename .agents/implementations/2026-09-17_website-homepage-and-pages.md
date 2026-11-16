# 官网首页 + GitHub Pages 发布流水线

完成时间：2026-09-17 14:51:12 CST

计划：`plans/website-homepage.md`（人类已批，含「SEO 可以在 html 模版开头加一点」这条反馈）。
工作单元是 **`website/`（官网）**，与应用本体的进度无关（根 `AGENTS.md` §4：两套技术栈）。

---

## 1. 本次改动的范围

| # | 交付物 | 状态 |
| --- | --- | --- |
| ① | 首页（琥珀贴顶 hero + 三卖点 + 四阶段工作流 + 三条功能细节 + 开源与技术 + 下载区 + 教程占位 + 页脚 + 404） | ✅ |
| ② | 中英双语：浏览器语言识别 + 左上角切换 + `<html lang>`/meta 同步 | ✅ |
| ③ | 下载信息：构建期注入版本与直链（现在无 release → 「即将发布」占位态） | ✅ |
| ④ | 素材占位体系（`Shot` + `media.ts`）+ 给人看的清单 `website/ASSETS.md` | ✅ |
| ⑤ | `website/AGENTS.md` 重写（结构 / i18n / 素材 / 下载注入 / **部署与自定义域名步骤** / Solid 2 纪律） | ✅ |
| ⑥ | `.github/workflows/website.yml`（push master / 正式版 release / 手动 → GitHub Pages） | ✅ |
| ⑦ | 单测 20 项（下载纯逻辑 11 + i18n 9）；`tsc` / `oxlint` / `build` 全绿 | ✅ |
| ⑧ | pi-lens 收尾：把另一会话报的 **error 级**误报处理掉（见 §4） | ✅ |
| ⑨ | 许可登记：`THIRD-PARTY-NOTICES.md` 新增 §1c「官网依赖」 | ✅ |

**没有**做的事（计划内明确不做）：真图素材（要人给，见 `ASSETS.md`）、Pages 开关与域名（要人配）、B 站教程（还没录）。

---

## 2. 涉及文件

**新增**

| 文件 | 作用 |
| --- | --- |
| `.github/workflows/website.yml` | 构建 + 发布 Pages |
| `.yamllint.yaml` | GitHub 工作流用的 yamllint 配置（见 §4） |
| `knip.json` | 声明 `website` 是独立工作区（见 §4） |
| `website/ASSETS.md` | 素材清单：4 张截图 + 2 张 AI 装饰图（含英文提示词） |
| `website/scripts/generate-icons.mjs` | 从 lucide-solid 的图标数据生成图标组件 |
| `website/src/components/icons.tsx` | **生成物**，16 个 Lucide 图标 |
| `website/src/i18n/{zh.ts,en.ts,index.ts,index.test.ts}` | 文案（zh 为形状源）+ 检测/切换/持久化 + 单测 |
| `website/src/data/{site.ts,features.ts,media.ts,tutorials.ts,release.ts,release.test.ts}` | 常量 / 结构 / 素材表 / 教程 / 下载逻辑 + 单测 |
| `website/src/components/{BrandMark,LangSwitch,Button,Shot,WindowFrame,Section,GithubMark}.tsx` | 通用件 |
| `website/src/sections/{Hero,Highlights,Workflows,Features,OpenSource,DownloadSection,Tutorials,Footer}.tsx` | 页面区块 |
| `website/src/lib/asset.ts` | `public/` 资源的 base 感知 URL |
| `website/src/vite-env.d.ts` | 注入量的类型声明（改为 `ImportMetaEnv` 增强） |

**改动**：`website/vite.config.ts`（发布信息注入 + `RAYBEND_BASE`）、`App.tsx`（站点外壳 + Loading 边界 + i18n）、
`App.css`（字体 + `@theme` 令牌）、`Document.tsx`（**静态 SEO head / OG / JSON-LD / noscript**）、
`src/routes/{index,[...404]}.tsx`、`package.json`（`packageManager` / 脚本 / 依赖归位）、`tsconfig.json`、
`oxlint.config.ts`（未改）、`website/AGENTS.md`；根 `.gitignore` + `website/.gitignore`（`*:Zone.Identifier`）、
根 `AGENTS.md`、`PLAN.md`、`THIRD-PARTY-NOTICES.md`。

**删除（模板残留）**：`src/routes/users.tsx`、`src/routes/users/[id].tsx`、`src/components/Counter*.tsx`、
`src/logo.svg`、`public/users.json`、`public/*:Zone.Identifier`。

---

## 3. 关键决策与理由

| 决策 | 理由 |
| --- | --- |
| hero **辅色琥珀 `#F0B033` 满铺** + 深墨字 | 用户在第一轮问答里选定；导航与 hero 同底色、贴顶（无白条/深色条） |
| 名称/口号抠图用 **CSS `mask-image` + `currentColor`** | 素材是白色墨迹 + 透明底（实测）；遮罩染色后同一张图能在浅底染墨、深底染白，**换色不用重导图** |
| 语言切换**纯客户端**（无路由、无 SSR） | 站点本就是客户端渲染，天然没有 hydration 问题；省掉一套 i18n 路由与两份 HTML |
| 下载信息**构建期注入**（`import.meta.env.RB_RELEASE`） | 不在浏览器里请求 GitHub API：无限流、无闪烁、SEO head 也能带上；本地构建不联网 |
| **只上正式版**（prerelease 跳过） | 用户选择；`release: published` 触发时若 `prerelease == true` 直接跳过 |
| 预览版/正式版**不新开页面**，同一个首页两处消费 | 单页站，`resolveRelease()` 一处纯逻辑、两处渲染（hero CTA + 下载卡） |
| **不用 `lucide-solid` 组件**，只取它的图标数据自己生成 | 它的 1.x 组件 import `solid-js` 的 `splitProps`，**Solid 2 已删除该 API**（连 `mergeProps` 都搬去了 `@solidjs/web`），实测浏览器报 `does not provide an export named 'splitProps'`；官方也还没有 Solid 2 版本（最新 1.46.0 的 peer 仍是 `^1.4.7`） |
| 字体 **Inter + Noto Sans SC 全自托管** | 用户选择；与应用本体同一套选型，无外链（国内访问不受影响） |
| Pages 发布来源 = **GitHub Actions**，**不放 `CNAME` 文件** | GitHub 官方文档原文：Actions 发布时 *"any existing CNAME file is ignored and is not required"*；域名只存仓库设置里 |

---

## 4. pi-lens 收尾（另一会话报的 error 级误报）

用户转达：另一会话在 `website/tsconfig.json` 上收到 **error 级**「Comments are not permitted in JSON」。
按「能真改的真改、刻意的写清理由」处理，现在**全项目 error 级为 0**：

| 项 | 处理 |
| --- | --- |
| `tsconfig.json` JSON 注释（error） | **真改**：去掉注释（配置项一个没少），说明搬进 `website/AGENTS.md` §1。实测 `tsc -p` 退出码 0，`lens_diagnostics(source=lsp)` 复检 0 条 |
| `knip: vitest unlisted` | **真改**：仓库根新增 `knip.json`，把 `website` 声明成独立工作区（它有自带的 `package.json`，根目录看不到 vitest 才误报） |
| `prefer-structured-class`（4 处 warning） | **真改**成 Solid 2 的数组/对象 class 写法（`@solidjs/web` 支持，实测渲染正常） |
| `solid/style-prop`（2 处 warning） | **真改**：`Document.tsx` 的 noscript 内联样式改成对象写法 |
| `no-known-value-widening`（3 处 hint） | **真改**：`Record<...>` 注解改成 `satisfies`（顺便让字面量类型收窄） |
| `hyphenated-svg-attribute`（3 处，生成物） | **压制**：Solid 2 的 JSX 类型**只认** kebab-case SVG 属性（写驼峰会 `TS2322`，实测），属类型定义的硬要求 → 生成器里逐行写 `pi-lens-ignore: hyphenated-svg-attribute` + 注释说明 |
| `.github/workflows/website.yml` 的 yamllint error/warning | **真改 + 配置**：新增 `.yamllint.yaml`（GitHub 工作流的 `on:` 被 YAML 1.1 当布尔值是稳定误报、`---` 文档头不需要、行长放到 120），并把这行折短；`yamllint` 复检 0 条 |
| `solid(prefer-for)`、`onMount→onSettled`、`<Loading>` 缺失 | 开发期由 pi-lens 报出/由类型检查挡下的真问题，已按 Solid 2 写法修掉 |

> 仍留着的（**有意保留**，都是 hint 级）：`no-runtime-typeof` / `no-unknown-parameters` 出现在
> `i18n/index.ts`、`data/media.ts`、`data/release.ts` —— 那些 `typeof` 全是**运行环境守卫**
> （`localStorage` 可能抛、`navigator` 可能没有、注入值可能不存在），正是该在边界上做的运行时判断。

---

## 5. 验证方式（跑了什么、看到什么）

**Agent 侧（冒烟，全绿）**

```bash
cd website
pnpm install                 # 锁定文件不变；新增 @types/node、字体包
npx tsc --noEmit             # 0 错误
pnpm lint                    # oxlint src：0 条
pnpm test:run                # 20 项通过（下载逻辑 11 + i18n 9），0.5–1.0s
pnpm build                   # 818ms + 113ms；dist/client 6.0 MB
```

- **构造注入干跑**：`RAYBEND_TAG=v9.9.9 pnpm build` → 产物 JS 里出现 `9.9.9`（tag 查不到时退化为
  「发布页链接」的路径也验证到了）；不带变量 → 注入 `null`，走「即将发布」态。
- **生产产物 headless 复核**（`vite preview` + CDP）：

  | 检查 | 结果 |
  | --- | --- |
  | `<title>` / `html lang` | `RayBend — Open-source photo ma…` / `en`（按浏览器语言切换成功） |
  | 区块 | `top, why, workflow, features, open-source, download, tutorials` 七段齐全 |
  | 下载按钮 | `https://github.com/C-Thun/raybend/releases`（pending 态） |
  | 版本显示 | `Coming soon` |
  | 字体 | `Inter Variable` + `Noto Sans SC Variable` 已加载（自托管） |
  | **外部请求数** | **0**（无 CDN、无 Google Fonts） |
- **视觉**：用 CDP 截图逐段看过（1440 宽桌面 + 390 宽手机 + 中英两版），hero/卡片/下载区/页脚版式与配色正常；
  截图属开发过程产物，未入库。
- **SEO**：预渲染的 `index.html` 里已含 title / description / keywords / canonical / og / twitter /
  theme-color / JSON-LD（`SoftwareApplication`）/ favicon，以及 `<noscript>` 降级提示。

**未经人类验证（按 `AGENTS.md` §2.8 归人类）**：真机浏览器观感、要不要调色/留白、Pages 开关与
自定义域名 + HTTPS 的实际生效、**第一次发版后官网版本号与下载链接是否自动跟着变**（要等第一个正式版）。

---

## 6. 遗留问题

1. **素材没到位**：4 张应用截图 + 2 张装饰图（可选）。清单、尺寸、取景要点、AI 提示词都在
   `website/ASSETS.md`；到位后在 `src/data/media.ts` 填 `src` 即完成替换。
2. **Pages 与自定义域名要人配**：步骤见 `website/AGENTS.md` §5（顺序：先仓库设置填域名 → 再配 DNS
   `CNAME raybend → c-thun.github.io` → 等绿 → 勾 Enforce HTTPS）。用 Cloudflare 时配置期要灰云。
3. **B 站教程**：录完把条目加进 `src/data/tutorials.ts`，教程区自动从占位变视频卡片。
4. **深链接**：产物是「预渲染外壳 + 客户端路由」，现在只有 `/` 一个页面所以无碍；将来加多页时要在
   Pages 上处理 404 回退（或改成逐页预渲染）。
5. **深色主题未做**：站点目前只有浅色一版（用户没要求）。
6. **部署体积**：`dist/client` 6 MB 里 5.6 MB 是字体全量分块（浏览器按 `unicode-range` 只取所需）。
   在意 Pages 上传量时再考虑自定义子集，已在 `website/AGENTS.md` 记一笔。

---

## 7. 追加（2026-09-17 15:13:17 CST）：命名规则 + 发布配置复核

**① 命名规则落地**（用户 2026-09-17 定）：开源/技术侧小写 `raybend`（仓库、路径、URL、存储键、包名），
**对外产品宣称英文一律 `RayBend`**。已写进根 `AGENTS.md` §1 与 `website/AGENTS.md` §4.1。
按这条改掉的对外展示位置（小写 → `RayBend`）：

| 位置 | 改动 |
| --- | --- |
| 截图窗口框的标题栏（`WindowFrame` 默认值 + hero / 特性三条的 title） | `raybend — browse` → `RayBend — browse` 等 |
| 页脚版权行（中英语言包） | `© 2026 raybend` → `© 2026 RayBend` |
| `<meta name="author">` | `raybend` → `RayBend` |
| JSON-LD 的 `alternateName` | 去掉小写项（搜索引擎本就不区分大小写），只留「光伴」 |

保留小写的地方（**故意的**）：`REPO_URL` / 域名 / `og:url` / `RAYBEND_*` 环境变量 / `localStorage` 键 —— 那些是 URL 与标识符，不是产品宣称。

**② 发布配置逐项复核**（回答「现在直接发版就会自动部署了是吧」）：

| 检查 | 证据 |
| --- | --- |
| CI 步骤本地干跑 | `pnpm install --frozen-lockfile`（锁文件一致）→ `test:run`（20 项）→ `typecheck`（0 错）→ `lint`（0 条）→ `build` 全通 |
| CI 路径的版本注入 | `CI=true pnpm build`（不传 tag）→ 走 API 查 `releases/latest` → 404（还没有 release）→ **注入 `null`** → 产物里是「即将发布 / Coming soon」占位态，构建不失败 |
| 用到的 action 是否存在 | 逐个查 GitHub ref：`checkout@v7`、`setup-node@v7`、`configure-pages@v6`、`upload-pages-artifact@v5`、`deploy-pages@v5`、`pnpm/action-setup@v6` —— **全部存在** |
| 产物路径一致 | 构建实出 `website/dist/client/index.html`，与 `upload-pages-artifact` 的 `path` 一致 |
| YAML 规范 | `yamllint`（含新增的 `.yamllint.yaml`）0 条 |

**结论**：把仓库 `Settings → Pages` 的 Source 设成 GitHub Actions（并填自定义域名）之后 ——
推 `master`、发**正式版** release、手动触发这三条都会自动构建并部署；第一个正式版发布后，
官网的版本号与安装包直链会自动跟着变（预发布版不上官网）。

**③ 域名实测（2026-09-17 15:2x，从本机查）：**

| 查到的东西 | 结果 |
| --- | --- |
| DNS | `raybend.cthun.com` → `CNAME c-thun.github.io` → 四个 GitHub Pages IP —— **配得对** |
| 证书 | `Let's Encrypt CN=raybend.cthun.com`，GitHub 已自动签发（09-17 06:05 UTC） |
| HTTPS 强制 | `http://` → `301` 到 `https://` —— **已开** |
| ⚠️ **发布源** | 域名现在服务的是**仓库根**（`/src/index.tsx`、`/package.json`、`/vite.config.ts` 都 200），即 Pages 的 `Source` 还停在 **`Deploy from a branch`**（启用 Pages 时的默认值，快照时间 04:51 UTC，早于 `website/` 存在） |

**这一条必须先改**：`Source` 停在 `Deploy from a branch` 时，`actions/deploy-pages` 会直接失败
（`Get Pages site failed … configured to build using GitHub Actions`）——
见 `website/AGENTS.md` §5 新增的排查提示。改成 `GitHub Actions` 后，下一次 push 即上线。

---

## 8. 追加（2026-09-17 15:42:19 CST）：hero 配色修正 + 品牌字矢量化

**用户反馈**：青绿不能切在琥珀上（两者中明亮度相近，看不出）；hero 上的绿色圆环与「胶片带」
改成与下方浅色同色（做成镂空感）；name / slogan 要「**白色宽幅外扩边 + 绿色内芯**」，
并问「CSS 是不是无法对透明位图做到这点，能不能转 SVG」。

### 8.1 hero 装饰改成镂空

| 元素 | 之前 | 现在 |
| --- | --- | --- |
| 右上同心圆环 | `text-brand/30`（青绿 30%） | `text-paper`（与下方区块同色 ⇒ 像打孔） |
| 底边胶片孔带 | `bg-brand/55` | `bg-paper`（与下一段连成一体） |
| 右侧小青绿圆点 | `bg-brand/70` | **删掉** |

### 8.2 品牌字：位图 → 矢量（回答“CSS 做不到”）

位图 + CSS 确实做不到干净的「外扩白边」：`mask-image` 只能给**墨迹本身**上色，无法向外扩张；
`drop-shadow` 叠出来的是模糊光晕；`feMorphology` 在部分浏览器不可靠。所以走矢量化：

| 步骤 | 做法 |
| --- | --- |
| 描摹 | 新增 `website/scripts/generate-marks.mjs`：ImageMagick 放大 3× → **`-blur 0x3` 磨掉毛笔飞白/噪点** → 二值化 → potrace → 写 `src/components/mark-paths.ts`（4 条路径） |
| 渲染 | `BrandMark.tsx` 改成渲染 `<path>`：`fill=var(--color-brand)` + `stroke=var(--color-white)` + `paint-order="stroke"`。描边居中、被填充盖住一半 ⇒ 只剩**向外那一半** = 外扩边；`vector-effect="non-scaling-stroke"` 让边宽以屏幕像素计（`outline` prop） |
| 落地位置 | 这几个属性全是 kebab-case（`stroke-width` / `paint-order`…），写在 JSX 里会被 lint 当驼峰建议反复误报（而 Solid 2 的 JSX 类型**只认** kebab-case）——所以移到 `App.css` 的 `.rb-mark-path`，JSX 只传 `--rb-mark-outline` 一个变量；hero 的圆环同理（`.rb-hero-rings`） |

**关键参数与代价**（实测）：

- 磨飞白这一步不能省：不磨时路径 97 KB，磨完 19 KB（同一个字）。
- 四条路径合计 **125 KB**（name-zh/en 各 19 KB、slogan-zh 56 KB、slogan-en 30 KB）；
  打包后首页 JS 144.89 kB / **gzip 63.76 kB**。
- `potrace`（`node-potrace`）是 **GPL-2.0 的开发期工具**，只产出坐标数字、**不进产物**；
  已登记 `THIRD-PARTY-NOTICES.md` §1c，并写了想避开 GPL 工具时的退路（`imagetracerjs`）。
- 源位图 `public/name-*.webp`、`slogan-*.webp` 保留 —— 它们现在是**描摹的源**，
  改了要重跑 `pnpm marks:generate`（已写进 `website/AGENTS.md` §4.3 与 `ASSETS.md`）。

### 8.3 验证

`tsc --noEmit` 0 错、`oxlint` 0 条、`pnpm test:run` 20 项通过、`pnpm build` 563ms；
目视了桌面（中/英）与 390px 窄屏三张截图：白边+绿芯在窄屏小尺寸下仍然清晰、
胶片孔与下一段浅色连成一体（镂空感成立）；旧的 `.rb-mark`（位图遮罩）CSS 已成死代码，一并删除。

---

## 9. 追加（2026-09-17 16:03:15 CST）：hero 装饰定型 + 矢量化保真度实测 + hero 高度压回

### 9.1 hero 装饰（按用户第二轮意见定型）

- **恢复**左下角「低浓度品牌色圆角方块切进来」（`bg-brand/20`，用户点名说这个 OK）；
- **保留**底边白色胶片孔带，且压在绿块**之上** —— 过琥珀处像镂空、过绿块处像打孔；
- 右上同心圆环由白改回**与方块同色系**（`text-brand/35`；线条细，浓度略抬才看得出）。

### 9.2 矢量化保真度：实测数据（不是“看着差不多”）

把矢量光栅化回源尺寸后与源二值图逐像素比（墨迹=白，交集=min、并集=max）：

| 标记 | 源墨迹 | 矢量墨迹 | 遗漏 | 多出 | **IoU** |
| --- | --- | --- | --- | --- | --- |
| name-zh（光伴） | 29.43% | 29.59% | 0.32% | 0.87% | **98.82%** |
| name-en（RayBend） | 28.57% | 29.64% | 0.37% | 4.10% | **95.70%** |
| slogan-zh（你的理想摄影伴侣） | 29.77% | 31.03% | 0.56% | 4.79% | **94.90%** |
| slogan-en（After the light bends） | 22.38% | 24.41% | 0.52% | 9.60% | **90.77%** |

读法：**遗漏极小（≤0.6%，细尖基本不丢）**，但细笔画会**被拉粗**（多出 0.9%～9.6%，
手写体最明显）—— 这是「模糊降噪 + 二值化」的固有偏差。做了一次参数扫描后把二值化阈值
从 50% 抬到 **60%**（多出量降了 0.2～0.7 个百分点，路径体积不变）。

对比图（位图原图 / 矢量轮廓 / 上线形态 / 200% 局部放大，含上述指标）已存到：
`/mnt/c/src/tmp/rb-markcmp-name-zh.png`、`/mnt/c/src/tmp/rb-markcmp-slogan-en.png`（开发期产物，未入库）。

**另一种做法的取舍**（已记进 `website/AGENTS.md` §4.3）：位图 + 内联 SVG 的 `feMorphology`
膨胀 alpha 做白边，更简单也更保真，但缩放会糊、浏览器对 filter 边界处理不一致；
选矢量是因为官网要缩放、要控描边宽度、以后可能换色/做路径动画。

### 9.3 hero 高度压回（用户：不能再涨高）

白边会让字视觉上更胖，所以同步收了排版：品牌字 `w-` 从 `min(17rem,64vw)`/`min(24rem,80vw)`
缩到 `min(15rem,58vw)`/`min(21rem,72vw)`，段落节奏 `mt-7/8` → `mt-5/6`，
上下留白 `pt-10 pb-20 sm:pt-16 sm:pb-24` → `pt-8 pb-16 sm:pt-12 sm:pb-20`。

| 量 | 之前 | 现在 |
| --- | --- | --- |
| hero 高度（1440 宽） | 719 px | **620 px** |
| 品牌字高度（名/口号，en） | 113 / 91 px | **100 / 80 px** |
| 整页高度（en） | 6062 px | **5963 px** |
