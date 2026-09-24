# 编辑基准切换（SOOC / RAW）+ 人类三项拍板的登记

完成时间：2026-09-24 16:43:21 CST

---

## 0. 背景

`IMAGING.md` §9 的三个未决点拿去问人类，当场拍板：

1. **AVIF 解码**：接 `avif-native`（dav1d）—— Rust 侧真解码
   （「进来先读 preview」、avif 导入、§2.5 切图优先都等它）。
2. **preview 源（RAW+JPG 成对）**：总览图下给 **SOOC / RAW 切换按钮，默认 RAW**；
   将来 issue 打「基于 sooc 编辑 / 基于 raw 编辑」标签，点 issue 同步按钮 ——
   **标签那部分先记录，本轮只实现切换**。
3. **§2.5 切图优先**：等 AVIF 解码器一起做。

## 1. 本轮实现：编辑基准切换

### Rust

* `crates/raybend/src/store/develop.rs`
  * 新枚举 `EditBase { Sooc, Raw }`（默认 `Raw`）+ `parse`（认不出给 `None`，调用方报错，
    **不静默回退** —— 否则界面显示的和实际编的不是同一张）+ `as_str`；
  * `edit_target(conn, asset_id, base)`：RAW 基准 → `raw.or(bitmap)`；SOOC 基准 → `bitmap.or(raw)`
    （只有 RAW 的照片切到 SOOC 也退回 RAW，没得选）；缺文件的位图不算可用；
  * `edit_base_available(conn, asset_id) -> (bool, bool)`：两侧各有没有可用文件；
  * 测试重写为 `edit_target_follows_the_base`（含缺位图退 RAW）+ 新增 `edit_base_parses_only_the_two_known_words`。
* `src-tauri/src/develop.rs`
  * `develop_edit_target` 加 `base: Option<String>` 参数（缺省 `"raw"`，认不出的词**报错**）；
  * 返回改为 `EditTargetDto { path, has_bitmap, has_raw }` —— 可用性给界面**禁用**切不过去的那一侧。

### 前端

* `src/api/types.ts`：`DevelopEditBase`（`"sooc" | "raw"`）+ `DevelopEditTarget`（DTO 镜像）。
* `src/api/editor.ts`：`getDevelopEditTarget(repositoryId, assetId, base = "raw")` 返回 DTO。
* `src/features/editor/store.ts`：`editBase` 信号（默认 `"raw"`，**会话级** —— 跟着用户在照片间走，
  重进编辑器回到默认）+ `editBaseAvailable`。
* `src/features/editor/panels.tsx`：`OverviewTab` 在总览图**下面**加 `SegmentedControl`
  （SOOC / RAW，缺文件的一侧禁用；注释写明将来 issue 标签同步这里的计划）。
* `src/workspaces/editor/EditorWorkspace.tsx`：解析时带上 `store.editBase()`
  （它是 effect 的依赖 —— 切一下就重新解析并重解这张图），回填两侧可用性。
* i18n：`editor.base.label`（编辑基准 / Edit base）+ `editor.base.sooc` / `editor.base.raw`（两侧语言包）。

## 2. 登记（只记不做）

* `IMAGING.md` §4.3「编辑基准」：切换的规格 + issue 打标签的点 issue 同步按钮。
* `FUTURE.md`「issue 与 SOOC」：issue 标签四条（含「基准必须跟着 issue 存，不能只是一份全局偏好」）。

## 3. 后续排队（人类已拍板方向）

1. **接 `avif-native`（dav1d）**：新增依赖 + Windows 打包验证（dav1d 是 C 库，构建链要先趟一遍）；
   之后的顺序：「先读 preview」（编辑进图）→ **§2.5 切图优先**（latest → SOOC → RAW）
   → avif **导入**支持。HEIC 是另一套（libheif 系），另行安排。
2. issue 打「基于 sooc / 基于 raw」标签 + 点 issue 同步切换按钮：等 issue 体系。

## 4. 验证

* `cargo test -q -p raybend --lib` → **950 passed / 1 ignored**（949 → 950：edit_base 解析测试）
* `cargo test -q -p raybend-desktop --lib` → **66 passed**
* `cargo clippy --all-targets` → 只剩既有那条（`store/backfill.rs:396`）
* `pnpm test` → **906 passed**；`npx tsc --noEmit` → **0 错**
* `pnpm lint:colors` / `lint:arch` / `lint:i18n` → 全绿

**未经人类验证（真机 E2E）**：总览下按钮的实际观感与位置、切换 SOOC/RAW 后画面确实换底图重解、
只有 RAW / 只有 JPG 的照片按钮禁用态、编辑基准切换后撤销/落库的行为。

## 5. 遗留

* `design/editor.pen`：总览下的编辑基准按钮还没画（Pencil 要人类把该文件打开）。
* `REPOSITORY.md` §4.1「编辑落在 RAW 上」的表述现在多了一个用户可切的基准 —— 措辞待下次顺手对齐。
