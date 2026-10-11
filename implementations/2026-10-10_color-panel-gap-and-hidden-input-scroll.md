完成时间：2026-10-10 01:52:19 +0800

# 色彩面板两处布局 bug：顶部留位空白 + 点开关整窗滚动

崔总 2026-10-10 在 Windows 实机测色彩管理时实报两处，均已定位到代码级根因并修复，附回归断言。
本轮只碰布局与控件宿主，不改色彩算法、存储、XMP、命令注册表。

## 现象与根因

### ① 色彩管理页签上部一大块空白（实测 287px）

不是「之后要显示的内容」，是排版回归。2026-10-07 做「曲线 / 预设等高容器」时，曲线页签改为
在流内定高、非曲线页签时 `invisible` **留位**（给绝对定位的预设页签当高度基准）。色彩管理页签
的 `ColorManagementTab` 挂在容器**外面**，于是切到色彩时，曲线那份留位（`CurveEditor` 是
`aspect-square w-full`，加上基础曲线块）原样空在色彩内容上方。实测：页签条底 → 色彩内容顶
= **303px**（287 留位 + 2×8 组内 gap）。

修法：`data-editor-advanced-body` 在 `curveTab() === "color"` 时整格 `hidden`
（`panels.tsx`）。等高容器只管曲线 ↔ 预设；色彩内容紧跟页签条，与 `design/editor.md` §3.12 /
`editor.pen` `Ic3cN` 一致（设计稿里本来就没有这块空白，属代码回归，不需要改稿）。

### ② 勾选打样 / 色域开关后「整个界面被抬上去、中下方一大片空白」

根因在 **Ark UI 的隐藏 input 定位**，与色彩功能本身无关：

- Ark 的 `Switch / Checkbox / RadioGroup / Slider` 都用 `position: absolute` 的 visually-hidden
  input（`clip: rect(0,0,0,0)`，1×1）。
- 本项目的封装里，这些 input 的宿主（`ArkSwitch.Root` 等）**没有定位祖先**，于是它们的包含块
  变成初始包含块（实测 `offsetParent = BODY`），坐标按「滚动容器的**未滚动**内容位置」解析。
- 右栏 `[data-editor-panels]` 是有内部滚动的面板：用户往下滚去找打样分区后，隐藏 input 实际
  落在**视口下方约 380–500px 的文档坐标**上，把 `document.documentElement.scrollHeight`
  从 761 撑到 **1763**（整页可编程滚动）。
- 点击开关时浏览器聚焦该隐藏 input 并把它滚进视野 → **整个文档滚走约 1002px**：整窗被抬上去，
  视口底部剩下无内容的大片空白。实测：`input.focus()` 后 `window.scrollY` 由 0 → 1002。

修法（`Form.tsx` 的 `Switch / Checkbox / RadioChoices`、`Slider.tsx` 的 `Control`）：给隐藏
input 的宿主加 `relative` 当包含块，隐藏 input 就钉在控件身边。`SegmentedControl`
（Item 本来就有 `relative`）、编辑 `SliderRow`（Control 本来就有）与 `FlowSwitcher`
（Item 本来就有）无需改动。

复现与验证（Chromium + dev server，真实 DOM）：

| 阶段 | 色彩页签留位 gap | 右栏滚到底整页溢出 | 聚焦隐藏 input 的页面滚动 |
| --- | --- | --- | --- |
| 修复前 | 303px | 1002px | +1002px |
| 修复后 | 8px（组内 gap） | 0 | 0 |

## 涉及文件

- `src/features/editor/panels.tsx`：`data-editor-advanced-body` 加 `classList={{ hidden: curveTab() === "color" }}`（该文件另有其它会话未提交的预设/渐隐改动，本轮只在容器这一处追加，未触碰其余）。
- `src/components/ui/Form.tsx`：`Checkbox` 根加 `relative`；`Switch` 根加 `relative`（含成因注释）；`RadioChoices` 的 `ArkRadioGroup.Item` 加 `relative`。
- `src/components/ui/Slider.tsx`：`ArkSlider.Control` 加 `relative`（与编辑 `SliderRow` 的既有写法对齐）。
- `scripts/ui-smoke.mjs`：编辑右栏新增两条回归断言 —— ① 色彩内容与页签条间距 ≤ 16px；② 右栏滚到底后 `documentElement.scrollHeight ≤ innerHeight + 1`。

## 纪律评估

- **命令注册表（§2.15）**：无新增用户可触发功能，不涉及。
- **XMP / 存储（§2.19、§2.16）**：纯布局与焦点行为，不改数据形状、不入侧车、无迁移。
- **confirm 节点（§2.21）**：未改任何编辑参数写入路径。
- **设计先行（§5.1）**：修的是实现偏离设计（空白不在稿里），`editor.pen` / `design/editor.md` 无需改。

## 验证

- 几何探针（CDP，1440×900，编辑 → 色彩页签）：修复前页签条底→内容顶 303px、`scrollHeight` 1763；
  修复后 8px、761（= 视口）。
- 焦点探针：修复前对色域开关隐藏 input `focus()` → `window.scrollY = 1002`；修复后 0，且右栏滚到底时
  不再有落在视口外的 checkbox/radio。
- `pnpm typecheck`：退出码 0。
- `pnpm test`：**1242 通过 / 0 失败**。
- `pnpm lint:colors` / `lint:arch` / `lint:i18n`：全部通过。
- `pnpm build`：通过（仅有既有的 chunk 体积提示）。
- `pnpm smoke:ui http://127.0.0.1:1439/`（dev server + Chromium，`CDP_PORT=9541`）：
  `problems: []`，新增字段 `advanced.colorGap = 8`、`advanced.pageOverflow = 0`；日志 `/tmp/raybend-smoke-1439.log`。
- 未跑 `cargo test`（本轮零 Rust 改动）；未做 Windows 实测与真机视觉复验 —— 两处修复的实机观感请崔总复验。

## 遗留

- 未逐一在 Windows 真机上复验其它工作流的开关/勾选（浏览勾选、导入开关）。机制上已由
  `Form.tsx` / `Slider.tsx` 的统一修复覆盖；若仍见整窗滚动，按「隐藏 input 是否落在视口外」
  这条判据抓现形。
- 色彩管理本身的验收路径与首版边界仍见 `implementations/2026-10-04_color-management-ready-for-acceptance.md`。
