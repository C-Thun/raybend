# 官网素材清单（要你出手的部分）

> 这份清单只讲**还缺什么图、怎么给**。素材到位后官网会自动从占位块换成真图，版式不用改。
> 相关：`website/AGENTS.md`（站点结构与约定）、`src/data/media.ts`（素材表，代码侧唯一来源）。

**当前状态：除已有品牌素材外，图片都还是占位块。** 占位块上写着它该是什么、多大尺寸；
鼠标悬停能看到提示词。

---

## 1. 已有素材（不用动）

| 文件（`website/public/`） | 用途 | 备注 |
| --- | --- | --- |
| `logo.webp` / `logo-small.webp` | 应用图标：导航、页脚、分享卡片（`og:image`） | 550×550 / 275×275，深底圆角方块 |
| `name-cn.webp` / `name-en.webp` | 产品名书法字（hero 大字） | **白色墨迹 + 透明底**，官网用 CSS 遮罩染色 |
| `slogan-cn.webp` / `slogan-en.webp` | 口号书法字 | 同上 |
| `splash_v1-cn.webp` / `splash_v1-en.webp` | 下载区的主视觉（艺术合成图） | 1086×814，白底 + 透明边缘，放白卡片上正好 |
| `favicon.ico` | 站点图标 | — |

> 这些是**项目自有素材**，来源与生成方式记录在 `implementations/2026-09-17_branding-and-splash.md`。

---

## 2. 要截的图（4 张）

四张都是**应用界面截图**，放在「窗口外框」里展示。建议：

- **窗口尺寸**：尽量接近表中的目标尺寸（按 1× 截就行，太小的图放大发虚）；
- **主题**：优先**浅色主题**（与官网浅色区块更合），深色以后要换再说；
- **内容**：用真实照片，但**别用涉及隐私/未成年人的素材**；可以在库里放一批风景/静物样张；
- **避免**：文件管理器窗口、任务栏、通知、个人路径（`C:\Users\<你的名字>\…`）、聊天窗口；
- **命名**：见下表「放哪」一列，直接丢进 `website/public/`。

| # | 用途 | 目标尺寸 | 放哪（文件名） | 画面要点 |
| --- | --- | --- | --- | --- |
| 1 | hero 主视觉 | 1600×1000 | `shot-browse-hd.webp` | 浏览工作区：左=库+目录树、中=照片网格（有真图）、右=信息栏（含直方图）；窗口拉宽一点 |
| 2 | 特性 A 配图 | 1440×900 | `shot-import.webp` | 导入工作区：左=来源树、中=待导入照片列表（带勾选框）、右=目标库与命名模版 |
| 3 | 特性 B 配图 | 1440×900 | `shot-grid-time.webp` | 浏览网格**按时间分组**态：能看到分组标题、tile 上的评分/色标徽标、几张照片处于选中态 |
| 4 | 特性 C 配图 | 1440×900 | `shot-palette.webp` | 命令面板打开、盖在网格上，列表里有若干条匹配命令与右侧快捷键 |

**截完怎么接上**：在 `website/src/data/media.ts` 里给对应条目加 `src: 'shot-xxx.webp'`，占位块立刻消失。

---

## 3. 要 AI 生的图（2 张，可选但推荐）

两张都是**透明底装饰图**，用不上照片素材。模型：任意支持透明背景的（本机习惯用哪个就用哪个），
**英文提示词**效果一般更好，色板取品牌色（青绿 `#52C6AB`、琥珀 `#F0B033`）。

### 3.1 胶片带（中段整宽带背景）

- **放哪**：`website/public/deco-film-strip.webp`，1600×600，**透明背景**
- **prompt**：

  ```text
  Flat vector illustration, transparent background: a long strip of 35mm film curving
  diagonally across the frame, drawn with clean 3px strokes and simple geometric shapes,
  teal (#52C6AB) as the main color with warm amber (#F0B033) accents, no text, no logos,
  no people, minimalist, generous negative space, crisp edges, poster-like
  ```

- **负面词**：`3d render, glossy, gradient mesh, drop shadow, photorealism, text, watermark, logo`

### 3.2 下载区柔光（右上角装饰）

- **放哪**：`website/public/deco-glow.webp`，1200×800，**透明背景**
- **prompt**：

  ```text
  Flat vector illustration, transparent background: soft abstract light rays and a few
  small sparkles spreading from the top-right corner, teal (#52C6AB) and amber (#F0B033)
  on a warm cream base, very subtle, low contrast, no text, no logos, no people,
  lots of empty space in the center, minimalist
  ```

- **负面词**：`3d, glossy, heavy contrast, busy, text, watermark`

> 两张都是**可选**：不放，页面也完整（现在是同尺寸占位块）。

---

## 4. 以后才要的图（先不做）

| 用途 | 什么时候 |
| --- | --- |
| 整理与检索 / 导出两个模块的界面截图 | 那两个模块做出来之后（官网只展示已经有的东西） |
| B 站教程封面 | 录完教程、往 `src/data/tutorials.ts` 加条目时再说 |
| 分享卡片（`og:image`）专用图 1200×630 | 想优化社交分享观感时；现在用 `logo.webp` |

---

## 5. 图的规格要求（所有图通用）

| 项目 | 要求 |
| --- | --- |
| 格式 | **WebP 优先**（截图可用 PNG，我转）；统一放 `website/public/` 根下 |
| 背景 | 装饰图要**透明**；截图不用（外面套了窗口框） |
| 体积 | 单张尽量 < 300 KB（截图多的那种可以到 500 KB） |
| 命名 | 全小写、中划线，`shot-*` 是截图、`deco-*` 是装饰，中英两版用 `-cn` / `-en` 后缀 |
| 许可 | 自己截的图没问题；AI 生成的图确认模型允许商用即可（本清单里的两张只作装饰，无实质内容） |

---

## 6. 图标出处（记录用）

站内图标取自 **Lucide**（ISC 许可，<https://lucide.dev>），但**不直接用 `lucide-solid` 包**：
它的 1.x 组件依赖 Solid 1 的 `splitProps`，在本站的 Solid 2 下无法运行。
因此由 `scripts/generate-icons.mjs` 只取图标数据、生成 `src/components/icons.tsx`。

加图标：改脚本里的 `ICONS` 清单 → 跑 `pnpm icons:generate`。许可登记见根 `THIRD-PARTY-NOTICES.md`。
