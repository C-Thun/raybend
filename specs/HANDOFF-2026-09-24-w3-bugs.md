# 交接文档：M3-W3 的 bug 批（2026-09-24）

> **处理状态（2026-09-24 14:55 更新）**：§2 的十条里 **九条已完成**，
> 实施记录见 `implementations/2026-09-24_w3-bug-batch-histogram-orientation-panels.md`；
> **§2.5 未做**（语义歧义 + latest 缓存是 AVIF、当前没有 AV1 解码器，需要人类拍板）；
> §3 的优化设想按约定未动（已登记 `FUTURE.md` D1.5）。
>
> **交接时间**：2026-09-24 12:35:23 CST
> **上一段会话做了什么**：见 §1（六个提交，M3-W3 已交付：显影管线 / 编辑栈 / 曲线 / 撤销 / AVIF 缓存 / 大图缓存 / issue 解析）。
> **新会话的任务**：只处理 §2 里用户报的这一批问题（**不要顺手改别的**）。§3 是用户给的优化设想，先登记、别在修 bug 时动手。
> **本文件的性质**：§2 每一条都做了**查证预调研**（读代码 + 已定位的根因），不是转述用户的猜测。凡是我**没查实**的，都写明「未查实」。

---

## 0. 开工前必读（现状与命令）

```bash
# 门禁（提交前全绿：Rust 906 + 桌面 66 + 前端 895；tsc / clippy / 三条 lint 干净）
cargo test -q -p raybend --lib            # ~8s
cargo test -q -p raybend-desktop --lib    # ~0.03s
pnpm test
npx tsc --noEmit -p tsconfig.json         # **判据以它为准**（pi-lens 的 LSP 缓存会落后，会报假阳性）
pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n
```

真机（人类验）：

```bash
pnpm build
export CARGO_TARGET_DIR='C:\rb-target\raybend'; export WSLENV='CARGO_TARGET_DIR'
cmd.exe /c 'pushd \\wsl.localhost\Ubuntu-24.04\home\andares\repos\c-thun\raybend & cargo build -p raybend-desktop -p raybend --features custom-protocol'
(cd /mnt/c/rb-target/raybend/debug && ./raybend-desktop.exe >/dev/null 2>&1 &)
pnpm debug:win            # 上面这一整套 + 产物核对（worker 也查）
```

**两条这次刚立的规矩（§2 的很多 bug 都是它们的反面）**：

1. **`-p raybend` 不能漏**：`raybend-raw-worker` 是 `raybend` 包的 bin，只选 `raybend-desktop` 时根本不会被构建 ——
   前天就是这么栽的（主程序新、worker 旧 → 编辑器永远卡在「正在载入照片」）。现在有 `PROTOCOL_VERSION` 握手 + `pnpm check:win` 核对 worker 内容兜底。
2. **主程序对了 ≠ 子进程对了**（`AGENTS.md` §5.3.2 第 4 条）。

**并行会话的状态**：另有会话已提交**动态反差**（`6307da3` + `0e43c93`）：`develop/local_tone.rs`、`develop/filters.rs`、
参数 id `dynamicContrast`，且**它住在编辑右栏「总览」页签里、直方图下面**（`params.ts` 的 `placement: "overview"`，`AGENTS.md` §11.5 有定义）。
本轮 bug 里有几条（直方图 / 总览 / 面板）会与它打照面，**改面板时别把它碰掉**。

---

## 1. 上一段会话交付了什么（六个提交）

| commit | 内容 |
| --- | --- |
| `8e8f89b` | 显影管线（`develop/`）+ 编辑栈落库（`catalog_0005`）+ **RAW 线性解码** + 渲染线程 + 前端实时预览 |
| `ab51034` | 编辑可撤销（复用浏览的撤销栈）+ 二级锁禁用 + 撤销后重读 |
| `660ed9a` | 曲线编辑器（自研 SVG）+ 跨语言测试向量 |
| `d65cb5c` | 覆盖层变换契约（Rust 出矩阵）+ 重置全部调整命令 |
| `13ba305` | AVIF 缓存 + 库内大图缓存 + issue 解析 + 看图/缩略图接编辑 |
| `ed8370d` | **修「卡在正在载入照片」**：显影任务合并不许丢照片 + worker 协议握手（v2） |
| `c94fe10` / `2c2ed79` | 文档与实施记录 |

实施记录：`implementations/2026-09-24_m3-w3-develop-pipeline-and-cache.md`（含全部实测数字、AVIF 编码耗时、验收清单、遗留项）。
计划：`specs/M3-W3.md`（标注已交付）。

**关键口径（别改错）**：参数只存非默认值；issue 只有 `latest` + 虚拟 SOOC/RAW；编辑落在 `_RAW/` 的 RAW 上（`REPOSITORY.md` §4.1）；
色温是绝对 K（基线 = as-shot，随 issue 存 `develop_stacks.as_shot_k`）；缓存图一律 AVIF 90 / 4:4:4。

---

## 2. 用户报的问题（逐条 + 预调研）

> 以下「用户原话」是 2026-09-24 12 时左右一次性报上来的。**每条都给：原话 → 现状/证据 → 落点 → 建议方向**。

### 2.1 browse 右栏的「预览」改名叫「总览」

* **用户原话**：「browse workspace right中的预览改名叫`总览`」
* **现状**：文案在 `src/i18n/zh-CN.ts:348` `"browse.preview": "预览"`（`en-US.ts:336` 是 `"Preview"`），
  渲染在 `src/features/browse/ViewerReadout.tsx:62`。
* **注意**：编辑侧同一个词已经叫「总览」（`zh-CN.ts:542` `"editor.group.view": "总览"`）—— 改完两边就一致了。
* **建议**：改译文（中英），别新增 key。顺带检查 `design/browse.md` 里那张图的标题是否也要同步。

### 2.2 编辑右栏的「总览」图框要**复用** browse 右栏的组件（4:3 固定）

* **用户原话**：「editor workspace right总览图框要复用browse workspace right中的组件（还是忘记了一定要复用复用复用，遇到任何功能需求优先查找有没有类似的现成组件看能否复用，这条我记得不是写在agents.md里的吗？），只要复用了很多问题不会出现，之前说了这是4:3固定的不会随图片比例发生变化或撑大，你直接复用不就行了？」
* **⚠️ 用户说得对，规矩是有的**：`AGENTS.md` §2.12「复用优先于重复硬编码」+「动手之前的第一件事 = 找现成的」。
  这次违反了：编辑侧**又写了一份** 4:3 框。
* **现状**：
  * browse 那份：`src/features/browse/ViewerReadout.tsx`（外层固定 4:3 框；内层盒子的 `aspect-ratio` = **原图比例**，
    按 `lib/preview-frame.ts` 的 `fitAxisFor()` 决定铺宽还是铺高；框是百分比定位的「视野框」）。
  * editor 那份：`src/features/editor/panels.tsx` 的 `OverviewTab`（约 281–305 行）——
    **只借用了 `PREVIEW_FRAME_ASPECT` 常量**，框与 `<img>` 是自己手写的，而且图源是**胶片带缩略图**（`props.thumbs`）。
  * 那个 `<img>` 是 `h-full w-full object-contain`：**照片按自己的比例被塞进 4:3 里**，
    视觉上就与 browse 那份（内层盒子=照片比例）不是一回事 —— 这就是用户说的「随图片比例变化/撑大」。
* **建议方向**：把这套「固定 4:3 外框 + 内层按原图比例的盒子 + 百分比视野框」抽成 `components/ui/` 的一个组件
  （`features/*` 之间不许互相 import —— `scripts/check-architecture.mjs` 规则 2，`HistogramPanel` 就是这么抽的，照抄它的做法）。
  编辑侧要不要「视野框」由参数控制（browse 的对比态已经有 `showVisibleBox` 这个先例）。
  **抽完之后 `ViewerReadout` 与编辑「总览」都必须用它** —— 不许留两份。

### 2.3 RAW 文件进编辑，图片方向不对

* **用户原话**：「raw文件进编辑图片方向不对」
* **证据（已查实）**：`crates/raybend/src/raw/rawler_backend.rs` 的两条步骤表都没有「应用方向」这一步 ——
  `BROWSING_STEPS` = Rescale / Demosaic / FujiRotate / CropActiveArea / WhiteBalance / Calibrate / CropDefault / **SRgb**；
  `LINEAR_STEPS` = 同样一串但**去掉 SRgb**。方向是作为**元数据**（响应里的 `orientation`）回到主程序、
  由下游（缩略图渲染器 / 看图）自己翻的。
* **fatal 的地方**：编辑这条路**完全没有管 orientation** —— `grep -rn orientation src-tauri/src/editor.rs` 是**空的**，
  `DevelopedImage { width, height, rgb }` 里也没有方向字段。所以：
  * 缩略图/看图（走 `thumbnail/render.rs`，那里有 `orientation_flips_do_not_change_size` 测试）方向是对的；
  * 编辑器（走 `editor.rs` 的显影线程 → 纹理）**不翻** → 竖拍 RAW 进编辑就是横的 ✓ 与用户描述一致。
* **建议方向**：方向属于「像素事实」，必须在**管道里**解决，不能只在某一条路上补：
  最小改动是让 `DevelopedImage` 带上 `orientation`，渲染线程在**上传纹理前**按它翻转（或把翻转并进管线的输出阶段）。
  ⚠️ 一旦翻转，**视口/覆盖层/命中测试用的尺寸与坐标系都要跟着变**（`render/viewport.rs` 的 `image_size` 是旋转前还是旋转后？
  `css_overlay_transform()` 的契约也会受影响）—— 这一条要连带考虑，别只翻纹理。
* **未查实**：FujiRotate（X-Trans 的传感器旋转）与 EXIF orientation 是两个不同的东西，别搞混；本机样本（P1000019.RW2）方向是 1。

### 2.4 编辑里双击图片「逻辑混乱」，明确没实现「双击 = 100%」

* **用户原话**：「editor中双击图片逻辑混乱，明确双击100%没有实现，放大范围肯定不是100%」
* **现状（链路已通，问题在语义）**：`viewport.tsx:215` 双击 → `{kind:"toggleFit"}` → `editor.rs:1393` `ViewportIntent::ToggleFit`
  → `fit_mode` 在 `Fit` ↔ `OneToOne` 之间切 + `refit()`。**代码是对的**，但有两个坑：
  1. **1:1 用的是「当前渲染出来的那张图」的尺寸**：`Developed` 处理器里 `context.set_image(render_image)`，
     而预览档的图是**缩放到 1920 长边**的（`ensure_output` 按 zoom 选档）→ 于是「1:1」= 预览图的 1:1，
     **不是原图的 100%** ✓ 与用户「放大范围肯定不是100%」完全对得上。而且 `state.image`（状态栏/信息里的「尺寸」）
     也是这个**渲染尺寸**，不是原图尺寸 —— 顺手一起错。
  2. 切到 1:1 要**异步**等全尺寸那张算完（24MP 带色度 ~200ms），期间视口已经按 1:1 摆了，
     等全图到了 `refit()` 又摆一次 ⇒ 观感就是「跳一下、逻辑混乱」。
* **建议方向**：视口与状态里的「图像尺寸」应当是**原始像素尺寸**（原图 / RAW 解码后的尺寸），
  与「当前渲染的是哪一档」分开表达；`ToggleFit` 到 1:1 时同时请求全尺寸档并**保持几何不变**（只在纹理到达后换纹理，不重摆视口）。
* **未查实**：用户说「逻辑混乱」是否还包含别的（例如工具态下的双击、或双击落到了胶片带上）—— 真机上先复现一次再定。

### 2.5 编辑里点胶片带切图时，要优先载入「latest / SOOC / RAW」中已存的那份

* **用户原话**：「editor中点film切图片的时候要优先载入图片的latest/sooc/raw中存的原片（按顺序来，能找到哪个显示哪个）」
* **现状**：切图走 `getDevelopEditTarget()`（`src-tauri/src/develop.rs` 的 `develop_edit_target` 命令）→
  `store::develop::edit_target()`，语义是「**RAW 优先**，没有 RAW 才用位图」。所以现在是**反的**：
  每次都去解码 RAW（20MP ≈ 1.6s），而磁盘上可能已经有 `cache/full/<asset>/latest-v6.avif` 或那张 JPG。
* **建议方向（有一层歧义，动手前先跟人类对齐）**：
  * 若只是「**先看到图**」：先显示 latest 缓存 / SOOC / RAW 的既有渲染（秒开），
    再在后台解码 RAW 供编辑用（编辑必须落在 RAW 上 —— `REPOSITORY.md` §4.1 的口径不变）；
  * 若人类要的是「**就用那份**」：那要注意 latest 缓存是**按当时参数渲染的**，拿它当编辑源会丢参数；
    真要这么做得先把参数读回来再套用（可行，但等于用缓存当「预览底」）。
  * 两条路都要**保持「编辑目标仍是 RAW」**，否则「编辑落在 RAW 上」这条口径就破了。
* **相关**：`store/develop.rs::choose_issue()` 已经有 `IssueChoice { Sooc, Raw, Latest }` 与优先级判断（看图/缩略图那条路在用），
  **切图这条路要复用它**，不要另写一套优先级（§2.12）。

### 2.6 编辑里选中任何图片，flowbar 右边的信息都显示「未选择照片」

* **用户原话**：「editor中选中了任何图片flowbar右边的info现在丢了，无论点哪个都显示`未选择照片`，这儿的功能要完整保留，在所有flow中」
* **根因（已查实，一行的事）**：`src/App.tsx:295` 的 `flowInfo` 只处理了 `import` 与 `browse`：

  ```ts
  switch (shell.workflow()) {
    case "import": return importExif();
    case "browse": { const item = browseStore.anchorItem(); return item === null ? null : assetItemExif(item); }
    default: return null;   // ← 编辑 / 导出 落到这里 = 永远空
  }
  ```

* **建议方向**：编辑 flow 要接**编辑里当前那张**（工作区知道是谁：`strip.current()` / `EditorPhotoInfo`）。
  组合层（`App.tsx`）是唯一该做这件事的地方 —— 把「当前照片 → ExifData」的映射从工作区传上来
  （现在 workspace 已经把 `EditorPhotoInfo` 算好了，缺的是 `ExifData` 那一份形状：`features/exif-strip/types.ts`）。
  ⚠️ 口径（`AGENTS.md` §11.5）：**follow 当前工作流**，没选中就清空，不留上一个 flow 的残留 —— 导出流将来同样要接。

### 2.7 编辑右栏「信息」要显示更全的 EXIF；删掉与 flowbar 重复的；把与调节关系最紧的放最上面

* **用户原话**：「editor workspace right信息里要显示更详细的exif信息，尽量显示全，同时，这里显示地exif中，移除所有在flowbar右边已经显示的信息，不重复，另外你要判断一下，在这里与下面的调节功能关系特别紧密的信息要优先显示在上面，比如说色温，曝光补偿等等」
* **现状**：
  * 编辑右栏「信息」页签：`features/editor/panels.tsx` 的 `InfoTab`，数据是 `EditorPhotoInfo`（`panels.tsx:65`）：
    `fileName / relativePath / format / camera / lens / iso / shutter / aperture / focal / dimensions / megapixels`。
  * flowbar 右侧（`shell/FlowBar.tsx` → `features/exif-strip/`）显示三组：**机型+镜头 / ISO·快门·光圈·焦距 / 尺寸·格式**。
  * ⇒ **几乎全是重复的** ✓ 与用户说的「不重复」冲突。
* **数据侧的现实（这一条决定可行性）**：`catalog.db` 的 `assets` 表只有
  `taken_at / camera_make / camera_model / lens / focal_mm / f_number / exposure_ms / iso / width / height / orientation / rating / flag`
  （`store/migrations/catalog_0001_init.sql`）——**色温、曝光补偿、白平衡模式、测光模式、拍摄模式、色彩空间…… 都不在库里**。
  现在有一个 EXIF 读取器（`crates/raybend/src/media/exif.rs`，目前只取 orientation）。
* **建议方向**（要人类拍板一次）：
  * **色温**：编辑侧本来就有权威值 —— 渲染线程算出来的 **as-shot 色温**（`RenderState.asShotTemperature`，
    本机样本 4350K）以及当前色温值；这两条应当显示在「与调节关系最紧」那一组里（用户的「比如说色温」）。
  * **曝光补偿**：`ExposureBiasValue` 得新读（EXIF 或在 RAW 元数据里），库里没有 ⇒ 要么加字段（**走迁移框架**，`AGENTS.md` §2.16），
    要么做一个「按需读文件」的命令（不落库）。**建议后者**（EXIF 是文件的属性，不是我们的事实源；DB 只存我们需要的）。
  * 分组建议：① 与调节强相关（色温 / 曝光补偿 / 测光 / WB 模式）② 拍摄（时间 / 机型 / 镜头 / 光圈 / 快门 / ISO / 焦距）
    ③ 文件（路径 / 尺寸 / 格式 / 大小）—— 其中 ②③ 与 flowbar 重复的**在这里删掉**。
  * ⚠️ 别把 `InfoTab` 做成第二份 exif-strip。

### 2.8 直方图没能显示出来 —— **已定位到根因（AVIF 回归）**

* **用户原话**：「直方图没能显示出来」
* **根因（已查实）**：`crates/raybend/src/display/histogram.rs:106` 的 `histogram_of_file()` 这样取像素：

  ```rust
  let request = ImageRequest::plain(path, ImagePurpose::Grid);
  let Some(image) = display_image(&request)? else { return Ok(None) };
  let Ok(decoded) = image::load_from_memory(&image.bytes) else { return Ok(None) };  // ← 这里
  ```

  而 `display/mod.rs` 的 `rendered()`（`RawBackend` **永远**走它）在本轮被我改成
  `mime: ImageMime::Avif` + **AVIF 字节**（人类定的缓存格式）。`image` crate 的 `avif` feature **只有编码器、没有解码器**
  ⇒ `load_from_memory` 对 AVIF **必定失败** ⇒ `Ok(None)` ⇒ **直方图恒为空**。
  这也解释了下一段 §3 里「曲线背景的直方图底纹没实现」—— **同一个取数口**（`CurveTab` 的 `loadHistogram`）。
* **实测（不是推断）**：`cargo run -p raybend --example hist-probe -- <RAW> <JPG>` 对**两种文件都是 ✗**：

  ```text
  ✗ /mnt/c/src/tmp/pic/P1000019.RW2
      **拿不到直方图**（`Ok(None)`）—— 界面上就是空态
  ✗ /mnt/c/src/tmp/pic/P1000001.JPG
      **拿不到直方图**（`Ok(None)`）—— 界面上就是空态
  ```

  比预想更宽：**位图也一样挂** —— `BitmapBackend::render` 只要不是 `ImagePurpose::Original`
  也会走 `rendered()`（同样吐 AVIF）。所以现在**任何照片的直方图都是空的**，
  浏览右栏与编辑右栏一起空。
* **探针**：`crates/raybend/examples/hist-probe.rs`（本轮新加，专门用来钉这件事；修好之后它应当打出 ✓）。
* **影响面（已核对）**：同族读回自己渲染字节的地方只有这一处 ——
  `thumbnail/render.rs:318` 与 `:479` 读的是**输入位图文件的字节**（不是我们的输出），不受影响。
  但顺带记一笔**同族的潜在坑**：`ImageMime::of_path()` 把 `.avif` 映射成 `Jpeg`，
  而 `render_bytes_with_edit` / `decode_bitmap` 用 `image::load_from_memory` 解码**输入文件** ——
  哪天用户往库里放一张 `.avif`，那条路也会 `Ok(None)`（不是本轮 bug，别混进来修）。
* **建议方向**：**别去装 AVIF 解码器**（纯 Rust 那条路没有，装 dav1d/libavif 是重依赖）。
  正解是让直方图**不经过编码**：直接拿管线输出的 RGB 字节算（新增一个「取像素而不是取图」的口子，
  `display` 里已有 `pixels()` 这条同族 API），顺带更快（省掉一次编码 + 一次解码）。
  ⚠️ 这条修完，**编辑侧「编辑后的直方图」就顺手成立了**（W4 原本排的事）——
  但注意现在的直方图是 SOOC 的（`panels.tsx` 里有一句 `editor.panel.histogramSooc` 的提示），
  要改成「跟着编辑走」得让直方图的取数也带上编辑栈（与缩略图那条 `render_sig_with_edit` 同一套）。

### 2.9 【最严重】每次调节参数都会让整个胶片带全图刷新

* **用户原话**：「最严重的，每次调节参数，都会导致整个film部分组件全图刷新，检查为什么会这样，按理说我调一张图跟其他图没有任何关系，顶多在film里替换当然图的latest缩略图而已」
* **根因（已查实，是我这一轮加的）**：`src/workspaces/editor/EditorWorkspace.tsx:333` 的 `commitDevelop()` 里调了
  `thumbs.clear()` —— 而 `clear()` 的语义（`components/ui/thumb-queue.ts` 的文档）是
  「**丢弃整张表并回收 URL**，同时推进代号丢弃在飞的请求」。
  ⇒ 每落一次库（松手一次），胶片带**每一格**的 `blob:` URL 都被回收 → 全部重新请求 → 整条带子全量重画 ✓ 与用户描述一致。
* **为什么当时这么写**：本轮让 `thumb_get`「编辑过的照片返回带编辑栈的缩略图」，所以落库后当前那张**必须**失效。
  但我用了最粗的手段（全清）。
* **建议方向**：给共享的 `ThumbQueue` 加一个**按路径失效**的能力（`invalidate(path)` / `refresh(path)`），
  编辑落库后只失效**当前这一张**；**禁止**为此再写第二套缩略图队列（§2.12）。
  ⚠️ `clear()` 仍然要留给「换目录」那种真需要全清的场景。
* **顺带一个同族的错**：`thumb-queue.ts` 的 `defaultToUrl()` 把 Blob 的 MIME **写死成 `image/jpeg`**：
  现在 `thumb_get` 返回的是 **AVIF** 字节 ⇒ MIME 对不上。浏览器多数情况下会嗅探内容，但这是明确写错的声明，
  必须改成按后端给的 `mime` 走（后端 `ImageMime::Avif` 已经加好了）。
* **未查实**：是否还有第二条刷新源（例如「整个 fil 组件」被重建 —— 那属于 §2.17 的 `untrack` 家族）。
  真机上复现时按 `AGENTS.md` §2.17 的手法（给 `Node.prototype.insertBefore` 挂钩子 + 比节点身份）判一次；
  **建议顺手加一条 `scripts/check-browse-boot.mjs` 式的回归断言**（拖一次拉杆，断言其它格的 `<img>` 节点身份不变）。

### 2.10 调节渲染极慢；有些调节只作用于「总览图」；**操作一会儿之后功能就失效**

* **用户原话**：「调节图片渲染速度极慢，而且有些调节在中间view区有效，有些似乎只作用于workspace right的总览图。这里有两个维度的问题，一个好像是操作一会儿之后，功能就失效了」
* **慢（有实测数字，不是错觉）**：预览档（1920）**8–13ms**、全尺寸 24MP **75–126ms（带色度 208ms）**
  （实施记录 §2.1）。看起来不慢，但**每一帧都要跑一整条管线**，而且：
  * `develop_loop` 里**每帧都重跑整张图**（`render_rgb8(source, params, curves)`），没有「只算屏幕上看得见的那块」；
  * 曲线/LUT 那条路是 3×4097 LUT（已做），但**色度**那几个算子每次都是全图逐像素；
  * 前端**每帧一条 IPC**（已做帧合并），但**落库**（`develop_commit`）会触发缩略图重渲染 + 大图缓存作废。
  ⇒ 用户的优化设想（§3）正是冲这个来的，**方案见 §3，别在修 bug 时顺手动**。
* **「有些调节只作用于总览图」**：**未查实**。最可能的解释有两个，真机复现时按这个顺序排除：
  1. 编辑右栏「总览」的小图是**胶片带缩略图**（`thumb_get`，编辑过的照片会带编辑栈渲染）——
     它在**落库之后**才更新；若中间视口那条路断了（见下一条），观感就正好是「只有总览变了」；
  2. 中间视口在**预览档**（1920）下有些算子在缩略图上不明显（例如动态反差的局部性）。
* **「操作一会儿之后功能就失效」（这条最像 bug，重点查）**：现有的**静默失败路径**有三处，按可能性排序：
  1. **显影线程 panic**：`develop_loop` **没有 `catch_unwind`**。线程一死，`developer.send()` 就失败 →
     `editor_set_params` 返回 Err → 而前端那条链是 `void setEditorParams(...)`（**错误没人看**）⇒
     界面继续画最后一帧纹理，**拉什么杆都没反应** ✓ 完全符合「操作一会儿之后就失效」。
     修法：`develop_loop` 外面套 `catch_unwind` + 上报（**这与渲染线程已有的「捕获 panic + 重启 + 上报」是同一条纪律**，`AGENTS.md` §7.9），
     并且让 `editor_set_params` 的失败**出现在界面上**（别只 `console.error`）。
  2. **worker 挂了 / 超时**：看门狗会把错误归类成 `Timeout`；错误现在会走到界面（本轮修过），但**下一次换图会重新解码**。
  3. **`latest_job` 与任务号错位**：本轮已把「每条任务必有结果」这条链闭合（`merge_jobs` + 粘住 `wanted_photo`），
     但**参数修订号**（`params_rev` / `applied_params_rev` / 前端的 `developRev`）**没有做同等的收口**：
     前端轮询 250ms 看 `appliedParamsRev`，若某次结果被丢弃且没有后继任务，界面会一直显示「还没算完」的旧值。
     真机复现时**先看这两处日志**：`[editor] 参数任务先到…`（本轮新加）与渲染线程的 panic 记录。
* **建议动手顺序**：先加「panic 捕获 + 错误可见」（工程纪律，不改行为），再复现——很可能第一条就把这个症状解决了。

### 2.11 曲线：双击取消点没实现；背景直方图底纹没实现

* **用户原话**：「曲线双击取消点功能未实现，曲线背景中的直方图底纹没实现」
* **现状（代码在，但很可能被别的事挡住了）**：
  * 双击：`src/features/editor/CurveEditor.tsx:169` 的 `onDoubleClick`（`nearestPoint` 命中 → `removePoint` → 提交）**已经写了**；
  * 底纹：同文件 208–216 行 `<Show when={channelValues(props.histogram, channel()).length > 0}>` 画 `data-curve-histogram`。
* **为什么两条都像「没实现」**：
  1. **直方图恒为空**（§2.8 的 AVIF 回归，**已实测**）⇒ 那个 `<Show>` 永远不成立 ⇒ 底纹不可见 ✓；
     注意 §2.8 修完这一条会**自动好**（同一个取数口），但**别在修的时候顺手把「编辑后的直方图」也当成它的一部分** ——
     现在这一段是 SOOC 的直方图（面板里已经写了提示），要跟着编辑走是另一件事（W4）。
  2. 双击那条：**指针按下就已经在加点/拖点了** —— 双击的第一次 `pointerdown` 会把点加到光标处（或抓住最近的点），
     第二次 `pointerdown` 再动一次，然后 `dblclick` 才删 —— 净效果是「看到点被加出来又删掉/或删错了那个」，
     用户主观上就是「双击没用」。
* **建议方向**：双击要**从按下那一刻就区分**（例如 `pointerdown` 记时间戳 + 位置，第二次落下时若与第一次足够近，
  按「双击」处理并吃掉这次按下），或者改成「按在点上 + 双击」才删、并保证按下不移动点；
  **并且为它写测试**（`dblclick` 的语义是纯逻辑，可以在 `src/lib/curve.test.ts` 层面钉死）。
* ⚠️ 用户此前对这条控件的口径（不要忘）：点在线上加点、拖动、双击删点、**两端方块可拖 = 黑场/白场**、
  **格子压在直方图之上**（与 `Histogram` 组件相反）、每个通道各一份直方图底纹（RGB = 三通道包络）。

---

## 3. 用户给的优化设想（**本轮不动手，先登记**）

> 原话照录，别改口径。新会话处理完 §2 之后再谈要不要开工；开工前应当走 plannotator 规划。

**（a）拖动限流**：「用户在拖动调节杆时，给一个限流，每 0.2 秒才触发一次计算」

**（b）只算看得见的那块**：「对当前画面中看到的图片部分进行截图，保存在内存中，仅计算这个区分部分实际在屏幕上显示的像素，
这样在用户松开调节杆时，这个计算只发生在当前这个实际像素裁切的副本上，让用户在拖动的时候的管线计算成本大大降低，
当用户松开调节杆后，才触发全图管线处理、更新latest、预览/缩略图更新这套链路，这样能避免过多无意义的全像素计算。」

**（c）管线分期 + 缓存**：「对raw来说处理分2层，第一层decode拿到CFA mosaic…第二层是develop将马赛克变成rgb…decode 一定是只做一次，
decode后的结果是缓存在内存中的，然后就是能在develop做的尽量在develop做，适合在rgb做的就只放在rgb做，这样可以对develop后的结果做缓存。
甚至还有个优化点，比如将develop的过程分层，分为前、中、后三期：前期 = 白平衡/曝光/高光恢复；中期 = 去马赛克/色彩校正/色彩空间；
后期 = 反差曲线/饱和度/自然饱和度。…为各个节点做缓存，那么如果是靠后的操作，前面的流程就可以不必再算，能更快。」

**（d）精度**：「同时，调研当前计算管线，看能否让管线的计算过程更加精确。」

**用户给的参考阶段表（原文照录）**：

| 操作 | 是否在 decode | 最佳阶段 | 空间 | 英文 |
|---|---|---|---|---|
| 解容器/解压/读元数据 | 是 | decode | raw | parsing / unpacking |
| 黑电平/线性化 |  | 边界，常在 decode/develop 早期 | 线性 raw | black level / linearization |
| 坏点/镜头阴影 |  | 边界，常算 develop 早期 | 线性 raw | bad pixel / lens shading |
| 白平衡 | 否，元数据在 decode 读，应用在 develop | develop 早期，去马赛克前或后 | 线性 CFA / camera RGB | white balance |
| 曝光/数码亮度 | 否 | develop 早期 | 线性 scene-referred | exposure compensation |
| 高光恢复 | 否 | develop 早期，去马赛克前 | 线性 raw | highlight recovery |
| 去马赛克 | 否 | develop 中期 | CFA → camera RGB | demosaicing / debayering |
| 色彩校正矩阵 | 否 | develop 中期，去马赛克后 | camera RGB | CCM |
| 色彩空间转换 | 否 | develop 中期 | camera RGB → working RGB | color space conversion |
| 反差/曲线 | 否 | develop 后段或输出阶段 | display-referred / 工作 RGB | tone curve / contrast |
| 饱和度/自然饱和度 | 否 | develop 后段 | working RGB / Lab / OkLab | saturation / vibrance |
| 锐化/降噪 | 否 | develop 各阶段 | raw 或 RGB | sharpening / denoising |
| 输出 gamma | 否 | develop 末段 | 输出 RGB | output transfer function |

> 表里「黑电平 / 线性化」与「坏点 / 镜头阴影」两行原文**少一个单元格**（缺「是否在 decode」那列），
> 上面按**空值补齐**，词句一字未动。

**给新会话的现状对照（当前实现离那张表有多远）**：

| 用户表里的阶段 | 我们现在在哪 |
| --- | --- |
| 解容器 / 黑电平 / 坏点 / 镜头阴影 | **在 rawler 的 worker 里**（`rawler_backend.rs` 的 `LINEAR_STEPS`），我们只拿结果 |
| 白平衡 | **已在 develop 早期**（`LinearImage` 上，`color.rs` 的 `temperature_gain_ratio`）✓ |
| 曝光 | **已在 develop 早期**（线性域）✓ |
| 高光恢复 | **没有**（现在的 highlights 是显示域曲线，不是恢复） |
| 去马赛克 | **在 worker 里**（rawler 的 PPG）—— 用户的「decode 只做一次」我们现在**做到了**（线性源常驻显影线程），但**一换照片就丢**（换回要重解 ~1.6s） |
| CCM / 色彩空间 | 在 worker 里（Calibrate 步）；`develop/color.rs` 有 sRGB 传输函数 |
| 反差/曲线 | **在 develop 后段**（显示域）✓ |
| 饱和度/自然饱和度 | **在显示域**（已在实施记录里写明这是刻意的取舍：为了整条链只用一个 LUT） |
| 锐化/降噪 | 未做（M3-W4） |
| 输出 gamma | sRGB 编码在曲线之前（⚠️ 与用户表的「输出 gamma 在末段」不同 —— **这一点要在 W4 一并讨论**，因为现在的顺序影响了「曲线在黑场/白场上的语义」） |

---

## 4. 纪律提醒（新会话别踩）

1. **§2.12 复用优先** —— §2.2 就是这个规矩的反面教材；动手前先 `ls components/ui/`、先读同类工作区那一份。
2. **§2.14**：一批报上来的问题要**连续做完**，不要做一两项就回来确认；卡住了先做别的，最后一次性汇报。
3. **§2.8 / §5.3.2**：Agent 只做冒烟（编译、单测、命令跑通）；**GUI 视觉、性能体感、色彩正确性一律归人类**。
   报结论时必须区分「已验证（冒烟）」与「未经人类验证」。
4. **实施记录**：每完成一批改动写 `implementations/2026-09-24_<简述>.md`，首行精确到秒。
5. **不要用 todo 和 plannotator 两套清单**（`~/.pi/agent/AGENTS.md`）：走 plannotator 就只用它的 progress，否则只用 `todo`。
6. **数据库改动只走迁移框架**（§2.16）—— §2.7 若决定加 EXIF 字段就会碰到这条。
7. **禁止全盘搜索**（§2.18）：只在 cwd / 已知路径里找。

---

## 5. 建议的处理顺序（按「一个根因修一片」排）

| 顺序 | 事 | 为什么排这儿 |
| --- | --- | --- |
| 1 | §2.8 直方图取数改成不经过编码 | **一个根因同时解决三条**（直方图 / 曲线底纹 / 编辑后直方图的地基），而且改动小、可测；`examples/hist-probe.rs` 现成可验（现在 ✗ → 修完应当 ✓） |
| 2 | §2.10 的「功能失效」：显影线程 `catch_unwind` + 参数失败可见 | 工程纪律，且它可能是「失效」的直接原因；修完再复现一次别的症状 |
| 3 | §2.9 缩略图按路径失效（+ MIME 修正） | 用户说「最严重」，且根因已确定，改动集中在 `thumb-queue.ts` + 一处调用 |
| 4 | §2.6 flowbar info 接编辑流 | 一行 switch 的事，用户明确要求「所有 flow 都要有」 |
| 5 | §2.2 抽 4:3 总览框并两边复用 + §2.1 改名 | 面板层的整理，为 §2.7 铺路 |
| 6 | §2.4 双击 1:1（原始尺寸与渲染档分离） | 牵涉视口几何，单独一刀 |
| 7 | §2.3 RAW 方向 | 同样牵涉视口/覆盖层坐标系，与上一条一起想清楚 |
| 8 | §2.7 信息页签的 EXIF 分组 | 要先跟人类定「哪些字段、从哪来」（可能要加读 EXIF 的命令） |
| 9 | §2.5 切图的 issue 优先级 | 有一层语义歧义，先跟人类对齐再动 |
| 10 | §2.11 曲线双击语义 | 独立小改，但要补测试 |
| — | §3 的优化设想 | **不在这一批**：先登记，等人类说开工再走 plannotator |

---

## 6. 这一轮没做完 / 没验证的事（别当它已经好了）

* **AVIF 的编码代价**（实测：网格 62ms vs JPEG 5ms、1920 档 539ms vs 100ms；体积省 2.3–3 倍）——
  人类已拍板「全系统缓存用 AVIF」，但**导入 1000 张的缩略图会从 ~5s 变 ~60s**（后台队列）；
  若真机体感不可接受，`thumbnail/render.rs` 一处常量就能切回 JPEG。
* **换照片会重新解码**（显影线程只缓存当前一张的线性源）—— §3(c) 的缓存想法直接冲它。
* **大图缓存的容量 GC** 还没做（只有 `invalidate` 与版本自然失效）；**浏览侧的缩略图不会自动刷新**（编辑后要重进目录）。
* **`develop_edit_target` 与「编辑落在 RAW 上」** 这条口径**没有被真机验证过**（人类还没验到那一步）。
* 真机 E2E 的其余项见 `implementations/2026-09-24_m3-w3-develop-pipeline-and-cache.md` §6 的验收清单。
