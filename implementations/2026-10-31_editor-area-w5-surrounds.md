# W5 —— 周边：预设 / XMP / auto_adjust / 缩略图与缓存

> 完成时间：2026-10-31 21:44:52
> 规格：`specs/editor-area.md` §5.9 / §13.2 ｜ 交接书 `specs/editor-area-handoff.md` §5

## 改动范围与结论

| 面 | 做了什么 | 结论 / 依据 |
| --- | --- | --- |
| **预设** | 确认 `presetSnapshot` / `applyPresetSnapshot` 都只读写**默认层**；补两条单测 | 见下面「决策 1」 |
| **XMP** | `crates/raybend/src/xmp/mapping.rs` 新增 `area_layers_never_leak_into_crs`：带两层（全图 + 控制点）的栈 → `crs_block` **一个属性都没有**；`rb:profileJson` 里必须有 `areas` / `default_index` | 层只进 `rb:`，`crs:` 只写真正全局项（§13.2） |
| **Mask 参数归类** | 四条 `area*` 在 `NOT_MAPPED` 里各有一条理由 | `every_develop_param_is_classified_for_crs` 通过 |
| **auto_adjust** | `applyAutoAdjust` 本来就写 `params`（默认层）；补一条「非默认层在编时也写默认层」的单测 | 不会出现"自动结果写进控制点层"的语义错觉 |
| **缩略图 / 预览 / 导出** | 三条路都经 `render_develop16`（`display/output.rs::render_linear` / `render_working`），层循环**只有一份**：有额外层时进 `render_layered16` | 缩略图与编辑器显示同一画面 |
| **缓存** | `DevelopStack::signature()` 在 W1 已含 `areas`；`profile_hash` 走同一个规范 JSON（含 `areas`）；`PIPELINE_VERSION` 17→18 | `latest-<hash>` 与 `-full-` 两档都跟着换；旧缓存由既有 GC 收 |

## 关键决策与理由

1. **预设恒以「默认层」为作用对象**（不管当前在编哪一层）。
   理由：预设的六个大类里只有影调 / 色彩 / 曲线是逐像素的，镜头 / 清晰度 / 局部色调 / LUT / 基础曲线
   都**只能挂在默认层**；若按当前层捕获 / 套用，"一半大类"要么被静默丢弃、要么写到不该去的地方。
   `Mask` 四条参数不进预设（`paramsInGroup` 已按 `target` 过滤）。已写回 `specs/editor-area.md` §5.9。
2. **`crs:` 不表达层**：Lightroom 的局部调整是 `LrLocalCorrection` 一套完全不同的量表，
   硬映射只会让别的软件读出一堆没意义的数值；层随 `rb:profileJson` 走（本项目自己的域）。
3. **不做层级的"快速预设"**（如"把这一层做成预设"）：预设是跨照片的 look，
   层是这张照片的结构（有种子、有位置），二者的生命周期不同；需要时将来单开"层预设"特性。

## 单测

* `src/features/editor/store.test.ts`：`预设只认默认层；套预设不新建 / 不删除层（W5）`、
  `自动调整只写默认层（非默认层在编时也一样，W5）`
* `crates/raybend/src/xmp/mapping.rs`：`area_layers_never_leak_into_crs`

## 验证

```bash
cargo test -p raybend xmp::mapping     # 9 passed; 0 failed
pnpm test                              # 1370 passed; 0 failed
```

## 遗留

* `specs/xmp-sidecar.md` §5.1 的映射表补一句"多层怎么表达、旧版怎么读"（本轮已写进
  `specs/editor-area.md` §5.9 与 mapping.rs 的注释；sidecar 规格的那一页留给 W6 收尾时统一改，见 W6 记录）。
