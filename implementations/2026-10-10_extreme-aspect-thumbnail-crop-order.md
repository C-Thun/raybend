完成时间：2026-10-10 22:12:54 +0800

# 极端长宽比缩略图「先缩后截」修复

## 现象（崔总 2026-10-10）

库里一张 **369 × 3693**（约 1:10）的图，缩略图**非常糊**（「原来是好的」）。
崔总判读：小图口径应当是「算比例 → 按 3:1 居中截取 → 再缩到目标档」，
怀疑实现里做成了「先缩到目标档 → 再截中间 3:1」。**判读正确**。

## 根因

`clamp_display_aspect`（3:1 居中截取）本身没错，`encode` 内部也确实是「截取 → 缩放」；
错在**喂进去的源已经被缩过一遍**，截取只剩几十像素，而 `resize_for_thumb` 不放大：

| 路径 | 旧行为 | 结果（369×3693 举例） |
| --- | --- | --- |
| RAW 无编辑快路径 | `DecodeSpec::thumb(size.long_edge())` = 384 | 38×384 → 截 → **38×114** |
| RAW 有编辑 / V2 路径 | `geometry_input_edge` 只算编辑几何（×2 过采样）= 768 | 77×768 → 截 → **77×231** |
| 位图内嵌（EXIF 缩略图）快路径 | 内嵌图多大就多大，截完可能不足档 | 内嵌图小则同样糊 |
| 位图完整解码 | 整图 → 截 → 缩 | ✅ 本来就对（新增回归测试钉住） |

即：**取源尺寸没有把「3:1 截取会让长边缩短」算进去** —— 与 `geometry_input_edge`
（裁切要先多取像素）是同一类账，只是少算了展示夹取这一项。

## 修法

`crates/raybend/src/thumbnail/render.rs`：

1. **`display_aspect_scale(width, height) -> f32`（新，纯函数）**：
   3:1 夹取会让**长边**缩短多少倍（1:10 → 3.336；比例在范围内 = 1）。
   复用 `clamp_rect` 求截后尺寸，保证与真正的截取口径同源。
2. **`source_input_edge`（新）**：`geometry_input_edge`（只管编辑几何）× 展示夹取倍数，
   上限仍 8192。**取源**的三个调用点改用它：
   * `render_raw_file` 无编辑快路径（`want` 从 384 → 2563）；
   * `render_raw_file` 有编辑路径；
   * `render_file_with_profiles` 的 V2 色彩路径。
   `encode_with_lut` 内部那次 `geometry_input_edge` **保持原样** —— 那里是**截取之后**的调用点，
   再乘展示因子会重复计数（保留两个函数、职责分开就是为了这个）。
3. **`DecodeSpec::embedded_preview(max_edge)`（新）+ `embedded_only` 字段**：
   RAW 快路径仍**只读内嵌预览**（真解码一张 RAW 是 1.2s，见 `develop-probe`），
   只是把请求长边抬到截取需要的值 —— 否则 `max_edge > GRID_LONG_EDGE` 会掉进真解码。
   内嵌预览缺失时**退回常规请求**（必要时真解码），与老行为一致。
4. **`PIPELINE_VERSION` 15 → 16**（`render_sig` 里的三处 `v15` 字面量同改）：
   不抬版本，崔总库里那张**已经缓存的糊图**不会重渲染 —— 改完算法却仍命中旧缓存，
   这正是 `PIPELINE_VERSION` 注释里记过的事故。
5. **位图内嵌快路径加质量闸门**：先渲染内嵌结果，**长边达到目标档才直接用**；
   不够大就走完整解码，完整解码不出图（例如只有内嵌图可用的合成/损坏文件）再退回内嵌结果。
   闸门用「结果长边 ≥ 目标档」而不是「内嵌图尺寸」——顺带覆盖了「EXIF 缩略图本来就小」
   这一类糊法（大多数相机内嵌图够大，快路径不受影响）。

## 验证

* `cargo test -p raybend`：**1476 + 7 + 2 passed，0 failed**（新增 3 条：
  `display_aspect_scale_tells_how_much_the_long_edge_shrinks`、
  `extreme_aspect_requests_enough_source_before_the_3to1_crop`、
  `extreme_aspect_thumbnail_keeps_the_full_long_edge_after_the_crop`）。
* `cargo clippy -p raybend --all-targets`：本文件无告警。
* **真路径冒烟**（`examples/thumb-probe`，与应用同一条 `render_file`）：
  * `/mnt/c/src/tmp/pic/AM300135.ORF`（4640×3472，正常比例）→ `384×288` ✅ 快路径未回归；
  * `/mnt/c/src/tmp/tall-369x3693.png`（按崔总那张的尺寸合成）→ **`128×384`** ✅ 满档长边
    （旧口径会出 38×114）。
* 未验证：**RAW 的极端长宽比**没有真实样本（手上只有正常比例的 ORF/RW2），
  逻辑由 `source_input_edge` 单测钉住；真机观感归崔总。

## 遗留 / 需报告

* **别会话工作区的两处越界接触**（本会话只改了 `thumbnail/render.rs` 与 `memory/FUNCTION-IMAGING.md`）：
  1. `crates/raybend/src/store/organization.rs` 补了一行 `visibility: query::Visibility::default()`
     —— 别会话给 `Query` 加了字段（`store/query.rs`，回收站）但没更新这个字面量，
     `raybend` lib 编译不过，不修就没法验证本次改动。语义 = 默认 `Live`，与其 `Query::new` 一致。
  2. `cargo test --workspace` / `cargo clippy --workspace` 目前被 `raybend-desktop` 的
     `unresolved import crate::browse::DeleteResult`（别会话进行中的回收站重构）挡住，
     故本次只跑到 `-p raybend`。
* `display::full_cache::tests::gc_keeps_only_the_newest_latest_snapshots` 在**满载并行**下偶发失败
  （单独跑 3/3、整包再跑 1/1 均通过）—— 判为 mtime 排序在负载下的时序抖动，
  与本改动无关，留观察；若再复现就给它加显式 mtime 或改用确定性排序。
