# 展示尺寸的生命周期与两条来源统一

**完成时间：2026-10-11 19:47:07**（本条记录同时收尾本会话的三项相关定案）

## 1. 这次做了什么

三件事，一份口径（`specs/display-size-lifecycle.md` 是本轮的规格）：

1. **两条尺寸来源统一**（`specs/display-size-source.md` §2.1 新增）：
   * **位图**：认我们自己的文件头解析（`media::meta::read_photo_meta`）；
   * **RAW**：只认 **rawler 回传的 develop 输出尺寸**（`RawlerBackend::output_size`，照抄
     rawler `CropDefault` 的算法）—— 崔总 2026-10-11：「对 RAW 来说不能用私自解算像素数
     来替代正式 develop 输出的真实位图像素数」。
   * RAW 行**没值就是未知（NULL）**：`preferred_size("raw")` 直接 `None`，不许退到
     `assets` 的 EXIF/文件头尺寸（那是整帧，比出图大 1.5%）。
2. **缺值保底：现算 → 写回**（`ensure_sizes` / `ensure_sizes_conn`）：
   文件行 → latest 输出（`develop::backfill_output_sizes`）→ 各定稿输出
   （`issues::backfill_output_sizes`），幂等、只写 NULL、算不出就留 NULL。
   读侧回到**存值优先**（把上一轮我临时加的「现算优先」撤掉）。
3. **老数据修正：迁移 v20**（`catalog_0020_size_refresh.sql`）：只置 NULL，不重算 ——
   `asset_files.role='raw'` 的行、`source_base='raw'` 的 `develop_stacks` / `issues` 的输出尺寸；
   位图行与 `sooc` 基的行一个字节都不动。之后由上面的保底按新口径补回。

## 2. 涉及文件

* `crates/raybend/src/raw/rawler_backend.rs`：`output_size`（照抄 `CropDefault`）+ 单测；
* `crates/raybend/src/store/sizes.rs`：`raw_output_size` / `apply_display_size_source` /
  `file_display_size` / `ensure_sizes(_conn)`；`latest_size` 回到存值优先；
  `preferred_size("raw")` 不再退 `assets`；`backfill_dir_display_sizes` 跳过 RAW 行；
* `crates/raybend/src/store/develop.rs`：`backfill_output_sizes`（与 issues 对称）；
* `crates/raybend/src/store/migration.rs` + `migrations/catalog_0020_size_refresh.sql`：v20；
* `crates/raybend/src/store/backfill.rs`、`crates/raybend/src/import/sink.rs`：写入点统一走
  `store::sizes` 的两个函数；
* `src-tauri/src/source.rs`（`dir_meta_ensure`：RAW 的比例读数只认记录，记录没建好报 0；
  目录回填只写位图行）、`src-tauri/src/develop.rs`、`src-tauri/src/export.rs`：改走 `ensure_sizes`。

## 3. 验证

* `cargo test --workspace`：**1516 + 134** 全绿（新增 8 条：rawler 裁切规则、来源矩阵、
  RAW 缺值未知、存值优先、保底写回幂等、v20 迁移只动 RAW 链）。
* **真库 drill**（复制库 `/tmp/rbdrill6`，schema 19 → 20）：
  * 迁移刚跑完（未回填）：`sooc 3712×2784` / `raw None` / `latest None` / `issue:8 None`
    —— 宁可知未知，也不报错数；
  * `backfill-metadata --sizes` 之后：`sooc 3712×2784` / **`raw 5184×3888`** /
    **`latest 5184×3888`** / **`issue:8 5184×3888`**（该定稿的源是 RAW；此前库里存的错值
    3712×2784 —— 位图尺寸 —— 已修正）。
* 真身渲染仍旧 `5184×3888` —— **展示尺寸与出图像素一致**。

## 4. 允许的近似（写清楚，免得以后各写各的）

* 渲染时「要解多少像素」的**输入规模提示**（`thumbnail/render.rs::render_raw_file` 的
  `source_size`）仍读文件头 —— 它只影响解码请求的量（宁可多解一点），**不写库**，
  真实像素数以解码结果为准；
* 浏览网格的 RAW 比例读数优先用**库里的记录**；记录没建好时报 0（界面默认占位比例），
  不用私自解算的数顶。

## 5. 遗留

* 老库长期 NULL 的 RAW 行：已有逐张保底 + `--sizes` 整库回填；**开库后有界限的后台补齐**
  登记在 `memory/PLAN.md` 问题清单 #10（本轮不做）。
* 域外像素给镜头校正用（画质，不是尺寸）留在 `memory/FUTURE.md`，与尺寸互不依赖。
