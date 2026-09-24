# M3-W3 bug 批处理（§2 十条 + 格式范围登记）

完成时间：2026-09-24 14:55:42 CST

> 上游：`plans/HANDOFF-2026-09-24-w3-bugs.md`（换手文档，§2 是这批要处理的问题，§3 是**不动**的优化设想）。
> 本记录按 §5 建议顺序逐条收口；**§2.5 没做**（有语义歧义，见 §7），其余九条 + 格式范围登记全部完成。
> 门禁全绿：cargo test（raybend 947 / 桌面 66）、pnpm test 903、tsc 0 错、三条 lint 干净、clippy 仅剩
> 一条**既有**告警（`store/backfill.rs:396`，与本批无关）。

---

## 1. 这一批改了什么（按换手文档 §2 编号）

| # | 问题 | 改动 | 落点 |
| --- | --- | --- | --- |
| 2.1 | browse 右栏「预览」改名「总览」 | 译文（中英）+ `design/browse.md` / `browse.pen` 同步 | `i18n/{zh-CN,en-US}.ts`、`design/browse.*` |
| 2.2 | 编辑总览图框要复用 browse 那份 | 抽出 `components/ui/PreviewFrame.tsx`（固定 4:3 外框 + 原图比例内盒 + 百分比视野框），`ViewerReadout` 与编辑 `OverviewTab` **两边都用它** | `components/ui/PreviewFrame.tsx`、`features/browse/ViewerReadout.tsx`、`features/editor/panels.tsx` |
| 2.3 | RAW 进编辑方向不对 | **根因**：编辑的线性解码完全没管方向；且 worker 对 TIFF 家族（RW2）报的方向不可靠（实测报 1，文件头是 8）。抽出 `media::exif::raw_orientation`（**文件头优先**），缩略图那条路与编辑线性解码**共用**；新增 `LinearImage::from_raw16` / `oriented`（与 `apply_orientation` 逐像素交叉验证） | `crates/raybend/src/media/exif.rs`、`media/tiff.rs`、`develop/pipeline.rs`、`thumbnail/render.rs`、`src-tauri/src/editor.rs` |
| 2.4 | 双击不是 100%（1:1 是预览图的 1:1） | 视口 **逻辑尺寸与纹理档位分离**：`Viewport.image_size` = 原图尺寸，顶点按 uniform 里的逻辑尺寸铺（不再按 `textureDimensions`），`set_image(image, source_size)` 只在换照片时 refit ⇒ 换档位不重摆、不跳 | `render/gpu.rs`、`render/spike.wgsl`、`render/viewport.rs`、`src-tauri/src/editor.rs` |
| 2.5 | 切图优先载入 latest/SOOC/RAW | **未做**（语义歧义，见 §7） | — |
| 2.6 | flowbar info 在编辑流永远空 | `App.tsx` 的 `flowInfo` 把 `edit` 与 `browse` 合到同一条（编辑的「当前那张」= 浏览锚点，不另造选择模型） | `src/App.tsx` |
| 2.7 | 编辑信息页签要更全 / 去重 / 调节相关置顶 | 页签重组为**三组**（与调节相关 → 拍摄 → 文件）；删掉与 flowbar 重复的八项；**曝光补偿全链路**（`image` 无 SRational 支持 → 自写 `srational`：tiff 兜底 + kamadak + `ExifData` + `FileExifView` + 契约 JSON + TS 类型 + `EMPTY_FILE_EXIF`） | `media/tiff.rs`、`media/exif.rs`、`src-tauri/src/source.rs`、`src/api/*`、`features/exif-strip/*`、`features/editor/panels.tsx`、`workspaces/editor/EditorWorkspace.tsx` |
| 2.8 | 直方图恒为空 | **根因**：`histogram_of_file` 拿渲染管线的 **AVIF 字节**再 `image::load_from_memory`（avif 只有编码器没有解码器）⇒ 恒 `None`。改成走**像素口**（`display::pixels(Screen)` → `histogram_of_rgb8`），**不经过编码** | `display/histogram.rs`、`display/mod.rs`、`src-tauri/src/thumbs.rs`（文档） |
| 2.9 | 每次调节整条胶片带全图刷新 | **根因**：`commitDevelop()` 调了 `thumbs.clear()`（语义是丢弃整表）。新增 `ThumbQueue.refresh(path)`（只失效这一张，在飞的旧结果按路径代号丢弃）；顺带修 `defaultToUrl` 写死的 `image/jpeg` → **按字节文件头判 MIME**（`lib/image-mime.ts`，`viewer/store.ts` 同修） | `components/ui/thumb-queue.ts`、`lib/image-mime.ts`、`components/ui/viewer/store.ts`、`workspaces/editor/EditorWorkspace.tsx` |
| 2.10 | 「操作一会儿之后功能失效」 | 显影线程 `develop_loop` 改为**逐任务 `catch_unwind`**（panic 丢掉缓存的线性源、把错误当作该任务的结果交回渲染线程 → 界面可见），并把 `editor_set_params` 的失败接进面板错误通道（不再只进控制台） | `src-tauri/src/editor.rs`、`workspaces/editor/EditorWorkspace.tsx`、i18n 文案 |
| 2.11 | 曲线双击删点没实现 | **根因**：`pointerdown` 已经在加点/抓点，`dblclick` 才删 → 自我抵消。改为**第二次按下时判定**（时间 + 位置 + 第一下没拖动），判定抽成 `lib/curve.ts::isDoubleClick` 并补测试 | `lib/curve.ts`、`features/editor/CurveEditor.tsx` |
| — | 动态反差纳入常规参数框架（人类追加） | 去掉 `placement: "overview"` 与整个寄居机制，动态反差回到**影调页签**（与曝光/反差同组，落库/撤销/重置同链路） | `features/editor/params.ts`、`panels.tsx`、`model.test.ts`、`design/editor.md`、`AGENTS.md` §11.5 |
| — | 编辑总览页加扁平缩放控制（人类追加） | `− 100% ＋`：显示倍率、手动输入、一档一档加减；事实源是 Rust 视口（前端只发 `zoomBy` 意图，输入按当前值折算倍率） | `features/editor/zoom-control.tsx`、`panels.tsx`、`EditorWorkspace.tsx` |
| — | 导入/导出格式范围登记（人类追加，只登记不实现） | 位图：**导入 6 种（jpg/tiff/png/webp/avif/heic）、导出 5 种（不含 HEIC）**；**RAW：导出不支持、导入尽量支持**（跟 rawler 走：Bayer 先全、X3 可不支持、X-Trans 看 rawler）；**JXL** = 未来支持导入导出全流程；**issue 大图快照恒为 AVIF**（收回「将来换 JXL」的设想） | `FUTURE.md` §C8、`PLAN.md` §M4-W2、`AGENTS.md` §6.4 |

---

## 2. 关键决策与理由

### 2.1 直方图为什么「不按格式分叉」（人类 2026-09-24 追加口径）

JPEG / PNG / WebP / AVIF / RAW 在**像素口之后**是同一件事：都是 SDR 8 位 sRGB 的 RGB8。
格式特殊性（HDR / gain map / ICC / 10–16 位）属于**解码与色彩管理层**；将来接 HDR 时改的是
像素口的契约（上游统一变换成显示空间），直方图这一层不需要第二条路径 —— 也就不会出现
「载入用快的算法、实时另起一套」的分裂。`histogram_of_rgb8` 是**唯一**计数实现，
实时刷新（W4 的编辑后直方图）由显影线程把**当前帧的 RGB8**直接交给它（不落盘、不解码、不编码）。

### 2.2 §2.3 的根因比换手文档写的更深一层

换手文档说「编辑侧完全没管 orientation」。查证时又发现：**worker 报的方向对 TIFF 家族不可靠**
（rawler 的 Panasonic 解码器把 orientation 写死成 `Normal`，源码里带着 `// TODO fixme`）——
实测 `P1000096.RW2`：worker 报 `Some(1)`，文件头里是 `8`。缩略图那条路早有一条「**文件头优先**」
的规则（`pick_orientation`，2026-09-18 的「tiles 正、view 歪」事故就是它修出来的），
而编辑侧连这条规则都没接上。所以这次不是「在编辑侧补一步」，而是把规则抽到
`media::exif::raw_orientation` 让**两条路共用**（`AGENTS.md` §2.12）。

### 2.3 §2.4 的做法：逻辑尺寸进 uniform，而不是缩放矩阵

顶点着色器原先按 `textureDimensions(image, 0)` 铺四边形，而矩阵按 `Viewport.image_size` 归一 ——
两者一致时没问题，一旦「逻辑尺寸 ≠ 纹理尺寸」就会按纹理尺寸画。修法是给 uniform 加
`image_size: vec4`（96 字节：mat4x4 + params + image_size），顶点按**逻辑尺寸**铺，
纹理用 UV 0..1 拉伸上去。于是换档位（预览 1920 ↔ 全尺寸 6000）**只换清晰度、不动几何**，
「切到 1:1 等全图算完」那段不再跳。

### 2.4 曝光补偿：`image` 的 `rational` 助手会滤掉 0

`ExifData` 的既有 `rational()` 有 `.filter(|v| *v != 0.0)`（对光圈/焦距这类「0 无意义」的字段是对的），
而曝光补偿的 **0 是有效值**（无补偿）。所以另写 `srational()`（只滤非有限值），
`media::tiff` 侧同样单独一条 `Entry::srational`（类型 10 = SRATIONAL，分子**有符号**）。

---

## 3. 验证方式（区分「已冒烟」与「未经人类验证」）

**已冒烟（Agent 可判定的部分，全部实测）**：

- `cargo test`：raybend **947** / 桌面 **66**；测试净增 **+7**（直方图 2、方向 3、曝光补偿 3，
  减去移走的 `pick_orientation` 1 条 —— 逐文件核对过增量与预期完全一致，无重名、无意外增删）。
- `pnpm test`：**903**（+7：MIME 嗅探 2、thumb-queue refresh 3、双击判定 2）；`tsc` 0 错；三条 lint 干净。
- **`examples/hist-probe.rs` 从 ✗ 转 ✓**（RAW + JPG 都能出直方图）——这是 §2.8 的验收判据。
- **离屏像素证据**（lavapipe，不依赖窗口）：
  * `editor-offscreen` 新增 ④ 组：纹理 32×32、逻辑 64×64 时，1:1 下照片仍占 **64×64** 物理像素
    （边界外一格是底色、四块中心颜色正确）—— §2.4 的回归断言；
  * `spike-offscreen` 全部 5 组断言通过（uniform 从 80 → 96 字节没破坏渲染）。
- **真照片方向验证**：`develop-probe P1000096.RW2` → worker 报方向 1 → 文件头优先取到 **8** →
  线性源 **3888×5184**（竖）、预览档 1440×1920，PNG 证据落盘。
- **浏览回归脚本** `pnpm check:browse` 通过（`PreviewFrame` 抽取后右栏 4:3 断言仍成立）。

**未经人类验证（按 `AGENTS.md` §2.8 归人类）**：Windows 真机上的观感与交互 ——
双击 1:1 的视觉、拖杆跟手与胶片带是否只动当前格、缩放控制的手感、信息页签的实际排版、
竖拍 RAW 进编辑的真实方向、直方图在界面上的出现。

---

## 4. 过程事故与教训（必须记）

### 4.1 我跑了 `cargo fmt`，把整个 workspace 重排了（已回退）

处理 §2.4/§2.3 时顺手跑了 `cargo fmt -p raybend -p raybend-desktop`，**62 个我没碰过的文件被重排**
（`import/runner.rs` 143 行、`store/backfill.rs` 62 行……）。发现后：
1. 把 62 个非本次文件 `git checkout HEAD --` 全部回退；
2. 我改过的 14 个 Rust 文件里也混进了格式噪音 —— 尝试写「保 HEAD 格式、只搬语义改动」的
   合并脚本，产出有损（双逗号、丢换行、缩进错位），**放弃**；
3. 最终把 14 个文件全部回退到 HEAD，**逐条重放**本批改动，再用全量测试验证。

**教训**：本仓在 HEAD 上**不是 rustfmt-clean**（pinned rustfmt 1.9.0 + edition 2024 会重排既有代码，
`style_edition=2021` 也不匹配），门禁里**没有** fmt 这一步 —— 所以**不要跑 `cargo fmt`**。
「顺手格式化」就是 `AGENTS.md` §2.14/§5.4 说的「顺手改别的」。

### 4.2 pi-lens 报的 7 个 i18n 键「不存在」是**陈旧快照误报**

新加的 `editor.info.*` 七个键，LSP 反复报「类型里没有」。硬证据：
`MessageKey[]` 临时证明文件 + `tsc` 退出码 **0**、两侧语言包 grep 各 **7** 条。
`src/i18n/index.ts` 的文档里写着这个模式的处置办法（「加完键先跑一次 tsc，再改一次本文件，
检查器的缓存就会跟着刷新」）——已照做，并把新键登记进那个「缓存刷新锚点」注释。

---

## 5. 复用与收敛（§2.12 的账）

- **新抽的共享件**：`components/ui/PreviewFrame.tsx`（4:3 预览框）、`lib/image-mime.ts`（字节 → MIME）、
  `media::exif::raw_orientation`（RAW 方向规则）、`lib/curve.ts::isDoubleClick`（双击判定）。
  每一处都有「为什么扩不了现有那份」的说明或直接是唯一实现。
- **删掉的重复**：编辑侧手写的 4:3 框（改复用 `PreviewFrame`）、`pick_orientation`（并入
  `raw_orientation`）、`params.ts` 的 `placement` 寄居机制（动态反差归组后整个移除）、
  `defaultToUrl` 与 `viewer/store` 两处写死的 `image/jpeg`（并成 `imageMimeOfBytes`）。

---

## 6. 顺带修掉 / 同步的文档

- `AGENTS.md` §6.4：快照口径从「将来换 JXL」改为「恒为 AVIF」；§11.5 动态反差条目改「住影调页签」。
- `design/editor.md`：第 1 组内容表（总览加缩放控制、信息三组）、动态反差归位说明。
- `design/browse.md` + `design/browse.pen`：「预览」→「总览」（.pen 是文本层改名，Pencil 当时连的是 browse.pen）。
- `FUTURE.md` §C8、`PLAN.md` §M4-W2：格式范围登记（位图 6 进 5 出 + HEIC 只在导入、RAW 口径、
  JXL 定位、快照 AVIF）。

---

## 7. 遗留与未做（**不要当它已经好了**）

1. **§2.5 没做**（切图优先载入 latest/SOOC/RAW）：换手文档标了「有一层歧义，动手前先跟人类对齐」。
   查证时又发现一个硬约束：**latest 缓存是 AVIF，而 `image` 没有 AVIF 解码器** ——
   「先显示 latest 缓存」在当前依赖下做不到（要么加 AV1 解码器，要么改用内嵌预览/原文件当过渡图）。
   两条路都保持「编辑目标仍是 RAW」，需要人类选一条（见对话里的提问）。
2. **`editor.pen` 未更新**：缩放控制与信息页签三组只进了 `.md`；Pencil 当时打开的
   是 `browse.pen`，需要人在 VS Code 里打开 `design/editor.pen` 才能改。
3. **胶片带「只刷新当前格」没有脚本化回归**：`thumb-queue.refresh` 有单测，
   但「拖一次拉杆 → 其它格 `<img>` 节点身份不变」这条端到端断言还没进
   `scripts/check-browse-boot.mjs`（那个假后端目前不覆盖 editor 流）。
4. **信息页签「尽量显示全」只做了一半**：这轮补了曝光补偿（与调节最相关）；
   软件 / GPS / 测光 / WB 模式等还没进 `FileExifView`（每加一个字段都要动
   契约 JSON + 三处镜像，单独一批更合适）。
5. **AVIF 导入 / HEIC 导入仍是占位图**（`FUTURE.md` §C8 已登记实现前提：要接解码器）。
6. §3 的优化设想（限流 / 只算视口 / 分期缓存 / 精度）**未动**，按约定先登记（`FUTURE.md` D1.5）。
