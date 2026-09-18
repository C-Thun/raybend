# W2 看图管线（1.3–1.6）：Tab 三态、看图状态栏、按需取大图、右栏预览与直方图

完成时间：2026-09-18 22:41:53 CST

范围：`plans/M2-W2.md` 的 1.3 / 1.4 / 1.5 / 1.6 四步（浏览模式看图态的壳、状态栏、渐进取图、右栏读数）。

---

## 一、1.3 看图态的三条 chrome（`Tab` = 并列三态）

`src/features/browse/chrome.ts`（新）：

```text
① 默认（左右列 + 胶片带都在）
   Tab ↓
② no-sides（左列与右列收起，胶片带还在）
   Tab ↓
③ no-film（左右列回来，胶片带收起）
   Tab ↓
① 默认
```

* **并列三态**（不是「逐个关掉」的累加）：从 ③ 再按一次回 ①，而不是继续关到只剩照片。
* 两个方向都有断言：`chromeShowsSides()` / `chromeShowsFilm()`。
* **回到 tiles 时左右列必定回来**（`BROWSE.md` §5.4）：workspace 在退出看图时重置成 ①。
* 左/右列用 CSS `hidden` 藏（**不卸载**）—— 卸载会把滚动位置和树的展开状态丢掉。

## 二、1.4 看图态底部状态栏

`src/features/browse/ViewerStatusBar.tsx`（新）：

```text
│ P0005.RW2 [🔒]                       ★★★☆☆  ●  ⚑  👍        │
```

* **左**：文件名，**紧挨着**文件名是锁徽标（人类点名的位置关系）；
* **右**：星 / 色标 / 旗标 / 喜欢；
* **全宽、在三列下面**（与画布的 `ViewerStatusBar` 一致），只在看图时出现；
  tiles 模式那条是控制条（计数 / 当前目录 / 视图控制），两者不是同一个东西；
* 数据来自 `ViewerPhoto` 自带的 `marks` / `flag`（BrowseGrid 填进去）——
  **不发请求**，不会为了显示四个数卡一下。

## 三、1.5 大图按需取 + 渐进显示

* 看图件先用**网格小图**秒显（可能已在网格缓存里），再换 `screen` 档大图
  （`getViewImage(path, "screen")` → `view_image` → 统一取图口）；
* 换了之后**不改缩放锚点**：大图上来时按小图的倍率换算（`store.ts` 的老逻辑），
  所以不会「换图时画面跳一下」；
* 冒烟里假后端对 `thumb_get` / `view_image` 都回 1×1 PNG，两条路都走到了。

## 四、1.6 右栏看图态：预览（含视野框）+ 直方图

**拍摄信息让位**（`BROWSE.md` §5.9）：看图时右栏顶部换成预览 + 直方图，文件信息不动。
`AssetInfo` 的 EXIF 段抽成 `ExifSection`，看图态换成 `ViewerReadout`。

### 4.1 预览 + 视野框

* 框的数学是新加的 **`visibleRect(state)`**（`components/ui/viewer/store.ts`）：把「屏幕四角」
  反解成「图像里的哪一块」，再夹进图像范围；尺寸不全时返回 `null`（不画框）。
  8 条单测盯着它（适配=整张、缩到更小=整张、100% 居中、拖动偏移、拖出界被夹、退化输入）。
* 画法：外层盒子的 `aspect-ratio` 就是原图比例、高度撑满 —— 于是**这层盒子就是照片的盒子**，
  视野框按**百分比**摆即可天然对齐，不需要去读 DOM 尺寸。
* 样式：1px `border-brand`（画布口径）。

### 4.2 直方图（24 柱合成）

**在 Rust 侧算**（`AGENTS.md` §6.1 红线：前端不碰像素）：

* `crates/raybend/src/display/histogram.rs`（新）
  * `histogram_of_image(&RgbImage, bins)` 纯函数：`桶 = 色阶 × bins / 256`（255 落最后一桶、不越界），
    `max` = **三通道合并**峰值（前端归一化用，免得前端再扫一遍）；
  * `histogram_of_file(path, bins)` 走**统一取图口**的 `grid` 档（长边 384）——
    比全尺寸解码快一两个数量级，24 桶对 ≈15 万像素足够稳；
  * 6 条单测：黑白中段落桶、通道分开计数、0/255 边界、空图与 `bins=0`、单桶全吞、缺文件不炸。
* IPC `image_histogram(path, bins?)`（`src-tauri/src/thumbs.rs`，已注册）；
* TS `getHistogram(path, bins = 24)`（`src/api/db.ts`）；
* 画图数学另抽 `src/features/browse/histogram.ts` + 8 条单测（归一化、除零、长度不齐补齐/截断、
  脏值过滤、后端换柱数、峰值字段不可信时以数据为准）。

**合成方式**：三通道的柱子叠着画、各带半透明（`--hist-alpha`），重叠处自然透出青/品红/黄。
不用 `mix-blend-mode: screen` 的原因写在 tokens 里：screen 在**浅色**主题的条带面上会把颜色冲成近白，
那样浅色主题下整条直方图等于看不见。

## 五、冒烟抓到的两个真 bug（都修了，见 1.0–1.2 那份记录的 §五）

1. `onDblClick` 挂在 `<Tile>` 上**不会生效** → 外包一层 `class="contents"` 的 div；
2. 回车打开看图后被**同一个仍在冒泡的事件**立即关掉 → 网格 `stopPropagation` +
   看图件 `if (event.defaultPrevented) return;`。

## 六、验证（本次跑过的）

```text
cargo test -p raybend                    791 passed / 1 ignored
cargo clippy --workspace --all-targets   0 warning / 0 error
npx tsc --noEmit                         0 error
pnpm test                                545 passed
pnpm lint:arch / lint:colors / lint:i18n 通过
pnpm build                               通过
pnpm check:browse                        通过（新增：右栏换成预览+直方图、直方图画出柱子）
```

> **未经人类验证的部分**（`AGENTS.md` §2.8）：预览框跟着缩放/拖动动的**观感**、
> 直方图三通道叠色的**实际观感**、以及浅色主题下直方图的可见度 ——
> 冒烟只能证明「元素画出来了、有 24 根柱子」，好不好看得人类在真机上看。

## 七、涉及文件

```text
新增  crates/raybend/src/display/histogram.rs
新增  src/features/browse/chrome.ts / chrome.test.ts
新增  src/features/browse/ViewerStatusBar.tsx
新增  src/features/browse/ViewerReadout.tsx
新增  src/features/browse/histogram.ts / histogram.test.ts
改动  crates/raybend/src/display/mod.rs（导出 histogram）
改动  src-tauri/src/thumbs.rs + lib.rs（image_histogram 命令）
改动  src/api/db.ts（getHistogram）
改动  src/components/ui/viewer/store.ts（visibleRect + 8 条测试）/ index.ts（导出）
改动  src/features/browse/BrowsePanels.tsx（EXIF 抽成 ExifSection、看图态换 ViewerReadout）
改动  src/workspaces/browse/BrowseWorkspace.tsx（chrome 三态、状态栏、右栏 viewer）
改动  src/styles/tokens.css（--hist-r/g/b、--hist-alpha）
改动  src/i18n/{zh-CN,en-US}.ts（preview / histogram / histogramHint / histogramEmpty）
改动  scripts/check-browse-boot.mjs（假 image_histogram + 三条新断言）
```
