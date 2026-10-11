# 看图 = 真身像素（view 两段载入）+ 尺寸口径不再跨变体

**完成时间：2026-10-11 17:51:33**

规格：`specs/view-true-image.md`（崔总 2026-10-11 定：看图显示的一定是该变体自己的像素；
preview 只做载入占位；**禁止「按缩放档位选凑合尺寸」这种需求**——那是上一轮凭空发明的，已删并记为禁止项）。

---

## 1. 改动范围

### 1.1 尺寸口径（本轮真正的 bug：raw 的尺寸进到 sooc 里 / sooc 进到 raw 里）

| 文件 | 改动 |
| --- | --- |
| `crates/raybend/src/store/sizes.rs` | `preferred_size` 按**角色**收敛：`raw` 只认 RAW 行（只有「没有位图行」的 RAW-only 资产才退到 `assets`），`bitmap` 只认位图行/`assets`（没有位图行时才退 RAW）；`base_size` 改成「位图行 → 资产尺寸 → RAW 行」。**跨变体回退删干净** |
| `src/lib/display-size.ts` | `raw` / `issue:N` 缺值 → `null`（= 还没拿到），**不再 `?? base`**；唯一保留的 `?? base` 是 `latest`（没编辑栈时 latest 的定义就是基准，同一个变体） |
| `src/features/browse/display-variants.ts` | 选了变体就只认该变体的尺寸（没有 → `null`），不再用默认变体的尺寸顶上；`aspectOf` 同步 |

### 1.2 真身档（新的一档像素）

| 文件 | 改动 |
| --- | --- |
| `crates/raybend/src/thumbnail/render.rs` | `SizeClass::Full`（长边 = 图像自身，`FULL_LONG_EDGE` 哨兵「不缩放」）+ `FULL_JPEG_QUALITY=92`；`encode_with_lut` 对 Full 出 **JPEG**；`render_raw_file` 对 Full **完整解码**（`DecodeRequest::full` / `DecodeSpec::full`，**禁止内嵌预览**）；`geometry_input_edge` 对 Full 给源尺寸；`placeholder_image` 夹保险 |
| `crates/raybend/src/display/mod.rs` | `ImagePurpose::Full`（`"full"`）+ `takes_original_shortcut` 覆盖 Full；`BitmapBackend` 对「Full + 没编辑的位图」直接给**原文件字节**；Full **不进 `thumbs.db`**（9MB/张该住库内缓存） |
| `crates/raybend/src/display/full_cache.rs` | `CacheClass::{Preview,Full}`（`.avif` / `-full-….jpg`）；`read_class` 命中时**摸新 mtime**（LRU 近似——Windows 默认不记最后访问时间）；`gc_latest` **按档各保留 N 份**；`invalidate_source` **不限扩展名**（旧实现只看 `.avif`，真身档会漏删）；新增 `used_bytes` / `enforce_capacity(max)` / `maybe_enforce_capacity()`（5GB 或可用空间 5% 取小，进程内 5 分钟一次） |
| `src-tauri/src/thumbs.rs` | `render_full_cached`（真身渲染 + 库内缓存 + 按档 GC + 容量清扫）；`view_image` 对 `full` 分流：编辑过 → 全尺寸 latest；未编辑的 RAW → 全尺寸解码（缓存，变体名 `raw`）；未编辑的**位图** → 走原文件短路 |
| `src-tauri/src/export.rs` | `export_variant_image` 允许 `full`：定稿 → 全尺寸成片；`sooc`/`raw`/`latest`/export 变体 → 同上；`raw_original` 分支**排除 Full**（内嵌原样不是真身） |
| `src-tauri/src/issues.rs` | 自愈链的 `match` 补齐 `Full`（不在 `SNAPSHOT_ORDER` 里，进不来；`present` 顺手把真身判定补齐） |

### 1.3 前端：两段载入 + 提示

| 文件 | 改动 |
| --- | --- |
| `src/components/ui/viewer/store.ts` | `deps.loadFull`（可选）；`loadFor` = `loadPlaceholder`（占位，快）+ `loadFullFor`（真身，原地替换）；`fullPending` 新状态；`sharp` 语义改成「屏上已是真身」（`publishScreen` 不再置它）；借多图缓存那条也认真身；**预取下一张真身**（只向后一张、失败静默）；`close`/换图复位 |
| `src/components/ui/viewer/Viewer.tsx` | 「正在载入原图…」角标（`pointer-events-none`，只在 `fullPending` 时出现） |
| `src/api/db.ts` / `src/api/export.ts` | `ImagePurpose` / 尺寸联合加 `"full"` |
| `BrowseWorkspace` / `PhotoGrid` / `ImportWorkspace` / `FullscreenViewer` | 四处 `createViewerStore` 注入 `loadFull`（浏览走变体键 → `export_variant_image`，普通路径 → `view_image`） |
| `src/features/browse/display-variants.ts` | 同上（尺寸口径） |

### 1.4 编辑器：提示只在真帧结束时消失

| 文件 | 改动 |
| --- | --- |
| `src/features/editor/source.ts` | `editorViewportNotice`：**过渡帧（`paintedPath` 有值、`decode` 仍 loading）不再算载入结束**；只有「`decode=ready` + `paintedPath` 有值」才撤提示；「真帧算完但还没画上」报「正在载入照片…」（原先是「初始化」） |

> 根因（上一轮的三步观感）：`RenderOutcome::Drawn` 在过渡帧画上时就把 `painted_path` 标了，
> 而提示的结束条件原本只看 `paintedPath` ⇒ 提示在**预览帧**就到了终点，真帧晚到。
> Rust 侧本来就分得开（过渡帧不置 `decode=ready`），所以只改前端判定即可。

---

## 2. 关键决策与理由

1. **真身与占位是同一张照片的两种像素，不是两个状态**：布局尺寸（`natural`）与 zoom/pan 在替换时
   **一个都不动** —— 这是「原地替换」的定义，也是 100% 下倍率唯一可信的前提。
2. **100% = 该变体自己的尺寸的 1:1**（布局盒 = 该变体尺寸；像素 = 该变体的真身）。
3. **不允许「按缩放档位选尺寸」**：真身是**唯一终态**；editor 的那两档（`render/tier.rs`）只属于
   编辑器像素，且档位之间永远是「同一张图、同一套参数」的分辨率差。
4. **缓存 = 纯派生工作缓存**（可删、可重建），只负责「来回切要快」；首开老实算。
   按变体更新：`sooc` 不缓存（直读原文件）、`raw` 首次看时写、`latest` **必更新**（键含栈指纹，
   只留最新 2 份 → 撤销/重做直接命中）、`issue` 冻结长期有效（删定稿连带删）。
5. **真身档用 JPEG q92**：20MP 的 AVIF 编码是秒级到十几秒，JPEG 实测 ~0.8s、浏览器原生解码。
6. **容量上限 5GB 或可用空间 5% 取小**（一处常量），真身档优先于占位档删除。

---

## 3. 验证

### 3.1 门（全绿）

| 命令 | 结果 |
| --- | --- |
| `cargo test --workspace` | **1500 + 134** 通过（新增：`size_class_contract` 含 Full、`full_class_renders_the_source_pixels_without_downscaling`、`full_class_keeps_the_crop_pixels`、`full_class_has_its_own_extension_and_invalidates_with_the_rest`、`gc_latest_keeps_each_class_separately`、`capacity_sweep_deletes_full_class_first_and_keeps_the_tiny_placeholders`、`full_purpose_serves_the_original_file_for_an_unedited_bitmap`、`sooc_never_borrows_the_raw_size_when_the_bitmap_row_is_unfilled`） |
| `pnpm test` | **1283** 通过（新增：两段载入 3 条、洞口提示 2 条、尺寸矩阵按新口径重写） |
| `pnpm typecheck` / `lint:colors` / `lint:arch` / `lint:i18n` / `build` | 全过 |
| `pnpm smoke:ui` | `problems: []` |

### 3.2 真库 drill（`cargo run -p raybend --example full-tier-probe -- <库根> [片段]`）

复制崔总的库到 `/tmp/rbdrill` 后跑 1000037：

```text
display_size(sooc) = (3712, 2784)      display_size(raw) = (5264, 3904)
display_size(latest) = (5264, 3904)    display_size(issue:8) = (3712, 2784)
真身（Full）· 源 = _RAW/MYP1000037.RW2 · base = raw
  冷缓存真身：8074 ms · 5184×3888 · 8.3 MB（JPEG）
  写缓存 929 ms · 命中缓存 1 ms · 一致：true
  cache/full/3/latest-5ee0…-src983f…-raw-full-v16.jpg
```

* 逐变体尺寸 = 各自那一行 ✓（不再是「raw 拿到 sooc 的 3712」）；
* 真身是**原像素**：5184×3888（不缩放；20MP RAW 的完整解码）✓；
* 命中缓存 **1ms** vs 冷缓存 8s —— 缓存值得留的定量证据 ✓。

### 3.3 RAW 的尺寸 = 出图尺寸（崔总 2026-10-11 定案）

**过程**：真库 drill 发现 RAW 文件头 = 整帧（5264×3904）而 rawler 出图 = 官方 4:3（5184×3888），
差 1.5%。我先把展示尺寸对齐到出图尺寸（5184），崔总一度说这个功能不做、遂撤回；随后崔总定案：

> 「原则上 rawler 出图是多少，那就应该是多少，怎么能对不上呢？……对外展示的分辨率不能对不上。」

于是**重新接上**，并且口径从「相机官方尺寸」改成「**rawler 出图尺寸**」——照抄 rawler
`CropDefault` 那一段算法（`rawler/src/imgop/develop.rs` 2026-10-11 逐行对照）：目标矩形 =
`crop_area` → 无则 `active_area` → 都无则不裁；有 `active_area` 时适配；四色 CFA（superpixel）
半分辨率。落点：`raw/rawler_backend.rs::output_size` + `store::sizes::{raw_output_size,
apply_display_size_source}`，三处读盘（导入登记 / 懒回填 / 批量回填）都走它。

**验证**：`cargo test --workspace` 1503 + 134 全绿（新增 `raw_output_size_follows_rawlers_crop_default_rule`）；
真库 `/tmp/rbdrill5`（清掉 RAW 行后跑 `backfill-metadata --sizes`）→ `raw 5184×3888` / `bitmap 3712×2784`；
真身渲染 5184×3888 —— **尺寸与出图严格一致** ✓。

**域外像素**（那圈 80×16）是**另一回事**（画质）：对镜头校正的挪移有用，登记在 `memory/FUTURE.md`
（改法：镜头校正早于裁切；做那件事时输出尺寸仍保持 5184×3888）。

**同一轮抓到的连带问题**：`latest` 的尺寸原先**优先用库里存的** `develop_stacks.output_*`
（当年按 5264 算的）⇒ 源尺寸改成出图尺寸后它还是 5264，仍然对不上。改成**现算优先**
（当前源尺寸 + 栈几何），存值只兜底；真库复测 `latest = 5184×3888` ✓，回归测试
`latest_recomputes_when_the_stored_output_is_stale` 钉住。

---

## 4. 遗留 / 未做

1. **RAW 裁切感知尺寸**（§3.3）—— 已登记。
2. **对比视图（DOM 多格）现在也取真身**（2–4 张、cached 之后很快）；若觉得首次进对比偏慢，
   可以退回「每格按需（放大到 100% 才升）」——但那属于档位思路，**要崔总点头才做**。
3. editor 的「只渲染可见区域」加速仍是 `memory/FUTURE.md` 的方向（本轮没做）。
4. 人类（Windows 真机）目视：四个变体逐一比对 editor / browse、两段载入的观感、100% 的锐度。
