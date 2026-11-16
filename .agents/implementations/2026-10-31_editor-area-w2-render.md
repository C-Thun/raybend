# W2 —— 渲染：层循环 + 层内固定管线 + 控制点权重

> 完成时间：2026-10-31 19:42:10
> 规格：`specs/editor-area.md` §4 ｜ 交接书：`specs/editor-area-handoff.md` §2

## 改动范围

| 文件 | 改了什么 |
| --- | --- |
| `crates/raybend/src/develop/areas.rs`（新） | `AreaSpec` / `AreaLayer` / `LayerStack`（层列表 + 默认层位置 + 启停）、`feature_vector`、`point_weight`、`mask_weights` |
| `crates/raybend/src/develop/mod.rs` | 注册 `areas` 模块与再导出 |
| `crates/raybend/src/develop/pipeline.rs` | `DevelopStages.areas`；层链求值（`PreparedArea` / `LayerChain` / `render_layered16`）；`compose_linear_at`（落点取样）；LUT 移到默认层段末尾 |
| `crates/raybend/src/develop/working.rs` | `WorkingTonePlan` 记住原始参数 / 曲线；有额外层时整条走 `render_develop16`（工作域回退，见"取舍"） |
| `crates/raybend/src/develop/params.rs` + `assets/develop-params.json` | 四条 `Mask` 参数：`areaTolerance` / `areaOpacity` / `areaChroma` / `areaLuma` |
| `crates/raybend/src/xmp/mapping.rs` | 四条 Mask 参数归 `NOT_MAPPED`（并证明层不进 `crs:`） |
| `crates/raybend/src/display/output.rs` | `stack_layers()`（编辑栈 → `LayerStack`）；`render_linear` / `render_working_params` 接上 |
| `crates/raybend/src/render/working_preview.rs` | 有额外层时**退出 GPU 快速路**；`render` / `histogram_rgb8` 增加 `areas` 入参 |
| `crates/raybend/src/thumbnail/render.rs` | `PIPELINE_VERSION` **17 → 18**，`render_sig` 五处版本字面量同步 |
| `crates/raybend/src/store/develop.rs` | `Area::to_spec()`（store → develop 的唯一桥） |
| `src-tauri/src/editor.rs` / `editor_working.rs` | `DevelopParamsDto.areas/defaultIndex`、`DevelopJob.areas`、`SessionParams.areas` |
| `crates/raybend/examples/*` | 两处 `WorkingPreview::render` 调用补参数 |

## 关键决策与理由

1. **层内一套管线，逐像素内循环**（`render_layered16`）：层间只传 f32 线性、不落地成图；
   跨显示域的那一段用 `encode_signed/decode_signed`（±1 以上按同一份 sRGB 曲线外推），
   所以"高光逐层被夹掉"这件事不会发生。
2. **掩膜读本层输入 `prev`**（红线一）。控制点权重 `α = 0.5^(d/tolerance·5.6439)`，
   `V(1)≈0.02` 保证边界平滑淡出；**没有空间项**（`AreaKind::Point` 连 x/y 都不进 develop 侧）。
3. **关掉的层 = 权重 0**（`enabled = false` ⇒ `opacity` 传 0），整层旁路但**留在链里**：
   顺序、名字、参数都不动 —— 再打开就是原样。
4. **特征向量 `[luma, (r−g)·½+½, (r−b)·½+½]`**：分量都在 0..1，落点取样与求值共用 `feature_vector`。
   `areaLuma` / `areaChroma` 的默认值（50 = 中性 1.0）直接进权重，本轮不给界面。
4. **LUT 移到默认层段末尾**（§4.7 的有意画面变化）：单层照片从"锐化之后"变成"锐化之前"，
   所以 `PIPELINE_VERSION` 必须加一，否则旧缓存停在旧次序（毒缓存）。
5. **每层预建查表**（线性半条链 / 显示半条链 / signed 解码）而不是纯 f32 标量：
   24MP × 33 层的标量写法慢两个数量级；表与快路径同精度，空层逐位一致到 1 个 16bit 级。
6. **没有额外层时走原快路径**（`map_rows` / `LocalPass`），单层照片语义与逐位行为不变。
7. **v2 色彩路径的取舍**：有额外层时把工作域（Rec.2020 f32）转成 16bit 线性图再走整条管线，
   `>1` 的高光在这一段被夹；无额外层的老路一点没变。写在这里备查，不是遗漏。
8. **GPU 快速路只认默认层**：有额外层时 `WorkingPreview` 自动落到 CPU 慢速路（着色器一遍过不了层循环）。

## 单测

`crates/raybend/src/develop/{areas.rs,pipeline.rs}` 新增：

* `feature_vector_normalizes_into_the_unit_cube`
* `point_weight_is_one_on_the_seed_and_decays_to_two_percent_at_the_tolerance`
* `point_weight_grows_with_tolerance_and_ignores_position`
* `layer_stack_keeps_the_default_position_and_parses_layers`
* `identity_area_layer_is_bit_identical_to_the_single_layer_path`（≤1 个 16bit 级；8bit 输出逐位一致）
* `layer_order_changes_the_picture`
* `larger_tolerance_affects_more_pixels`
* `moving_the_point_sample_does_not_change_the_pixels`
* `disabled_area_is_bypassed_without_being_removed`（层关掉 = 权重 0，但留在链里）

## 验证

```bash
cargo test -p raybend --lib                 # 1619 passed; 0 failed（当时）
cargo test --workspace                      # 1620 + 192 + … 全绿（见 W6 记录的完整读数）
```

## 遗留

* 工作域（v2 色彩路径）下的层链精度取舍（见决策 7），将来若要 1:1 的高光细节需要把层循环搬进工作域。
* 层数多时的查表构建成本（每层 ~0.5ms × 3 张）暂未缓存；33 层全部带参数时约 50ms 的一次性构建，
  当前不构成问题（层少时更快）。
