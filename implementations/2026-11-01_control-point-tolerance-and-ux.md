# 控制点容差重做（亮度×色相 / 白线=容差 / 调试面板）+ 5 条界面修正

> 完成时间：2026-11-01 20:11:35
> 输入：崔总 2026-11-01 的三段反馈（容差概念与算法、调试面板需求、5 条界面问题）
> 前置：`implementations/2026-11-01_editor-area-bugfix-batch.md`（上一批 12 条修复）

## 1. 容差的语义（这轮的核心）

### 1.1 白线 = 容差

* 引线（那条白线）**长度就是容差值**：`u ∈ 100..1000` 线性映射到 `lineMin..lineMax`
  （默认 **32 / 320** 屏幕像素，开发期可调）。
* 拖 handle **1:1 跟手**：换算用的是同一组端点，拖 `lineMax − lineMin` 像素正好走完 `100..1000`。
* 上下限：`u` 夹在 `100..=1000`（线最短 / 最长），初值 **300**（不是最小值）。
* 名字**贴在圆片那一端**、永远从左往右写；**拖 handle 时名字不动**（只有线在伸缩），
  名字可以比线长（不再受线长约束）。
* 方向：点在左半 ⇒ 线向右，右半 ⇒ 向左；`Ctrl` 移动采样点跨过中线时自动换边。

### 1.2 权重：亮度 × 色相（**不是**一个各向同性的距离）

旧模型把 `[luma, c1, c2]` 塞进**一个**欧氏距离、再比**一个**容差。真机实测反对它的证据：

| 像素 | Δluma | Δchroma | 合成距离 | 容差 0.15 |
| --- | --- | --- | --- | --- |
| 同衣服受光面 | 0.242 | 0.106 | 0.264 | ❌ 衣服自己没选全 |
| 同亮度红色 | 0.015 | 0.254 | 0.254 | ❌ |

要把衣服选全（容差 ≥ 0.27），同亮度的红色（0.254）就被一起拉进来了 —— 这正是崔总说的
「我点着绿衣服，凭什么红的也变」。

新模型（`crates/raybend/src/develop/areas.rs`）：

```text
seed = [Y′, Cb, Cr]（感知域：sRGB 编码 + BT.709）
α = band(band_L, |ΔY′|) × band(band_C, ‖Δ(Cb,Cr)‖)      ← 两个轴各自两段式，再相乘
band(b, x) = 1 / smoothstep / 0   （core 内满强度，end 外不处理）
```

* 容差 `u` 经 `POINT_WEIGHT_CONFIG` 映射成两轴各自的 `(core, end)`：每个字段是
  `[u=100 的值, u=1000 的值]`，中间线性插值；
* 默认：`luma_core [0.05, 0.95] / luma_end [0.12, 1.40] / chroma_core [0.015, 0.10] / chroma_end [0.05, 0.28]`
  —— 容差 300 时亮度放得开、色相收得紧（实测：同衣服受光面 α≈1，同亮度红色 α=0）；
* **感知域**（sRGB 编码，4097 项查表）而不是线性：线性下暗部挤在 0，同一个容差在阴影里很宽、
  高光里很窄，手感必然「一格一格」；
* **色度平面距离**而不是色相角：近中性像素没有稳定色相角，平面距离把「很淡的颜色」放在原点
  附近，中性种子只命中中性像素。

**种子定义换了一版**（线性 → 感知域），旧种子**无法换算**：`Area.seed_version` 记版本，
载入 latest 时按每个点的 `x/y` **逐点重新取样**一次（位置 / 容差 / 参数都不动），然后落库。

## 2. 开发期调试面板

| 项 | 做法 |
| --- | --- |
| 入口 | titlebar **主题/密度开关左边**；`import.meta.env.DEV` + `lazy()` → **生产包整条分支被摇掉** |
| 内容（本轮） | 8 个权重数字（亮度/色相 × 完全容差/过渡终点 × 容差 100/1000）+ 白线长度两个端点 |
| 生效 | 写入 `lib/mask-debug.ts` → `developPayload().maskDebug` → `editor_set_params` → `DevelopStages.weight`；改一下立刻重出帧 |
| 持久化 | `localStorage`（`raybend.mask-debug.v1`）；「恢复默认」问 Rust 要（`editor_mask_defaults`，前端不抄数字） |
| 固化 | 调好后把数字写进 `POINT_WEIGHT_CONFIG` 并抬 `PIPELINE_VERSION`（本轮 19 → 20） |

只影响**编辑器真帧**：缩略图 / 导出 / 定稿走固化常量。

## 3. 五条界面修正

| # | 反馈 | 修法 |
| --- | --- | --- |
| 1 | 点「新建控制点」就凭空多一层，还有一条虚线幽灵行 | **先武装、点图才建层**：按钮变主色（再点=取消）+ flowbar 提示「在图片中点击选取采样点」；**删掉幽灵行与「待落点」空层**；`Esc` 只解除武装；点在图像外不建层（武装保留） |
| 2 | SOOC / JPG 下控制点不报错地被忽略 | 新建按钮**禁用**（tooltip 说明只能在 RAW 基准用）；面板里控制点行**置灰**、画布标记**半透明**；载荷里把控制点发成 `enabled: false`（权重 0，**层数据一个字不改**），切回 RAW 立刻恢复；取样失败改走 toast |
| 3 | 影调面板底下冒红字 | `developError` 内联整块删除 → **统一消息通道**（`props.toast.show`，3 秒去重）；`store.error()` 也转 toast；色彩管理页签的错误同样改走 `onError`。新纪律写进 `AGENTS.md` §2.29 与 `memory/COMPONENTS.md` §3.4 |
| 4 | 直方图下方留白比上方多一倍 | `HistogramPanel` 自带 `mb-3` + 父级 `gap-3` 叠成 20px ⇒ 调用方加 `!mb-0`，上下都只留 8px |
| 5 | flowbar 提示区左不左右不右 | 从 `justify-center` 改成**左对齐紧贴设置按钮**（锚点是左边那个按钮），右端仍留 100px 到 info 区 |

## 4. 涉及文件

**Rust**
* `crates/raybend/src/develop/areas.rs`：`Band` / `PointWeightConfig` / `POINT_WEIGHT_CONFIG` / `feature_vector`（感知域）/ `point_weight_with` / `chroma_distance` / `TOLERANCE_*`；`from_spec` 的容差与种子夹取；单测重写（含「同亮度不同色相必须排除」）。
* `crates/raybend/src/develop/pipeline.rs`：`AreaWeight::Point` 带 config、`DevelopStages.weight`、`LayerChain::build`。
* `crates/raybend/src/develop/mod.rs`：导出新类型。
* `crates/raybend/src/store/develop.rs`：`Area.tolerance` 数值域校验、`seed_version` 字段、`load` 里的旧容差换算。
* `crates/raybend/src/display/output.rs`、`develop/working.rs`：显式 `DevelopStages` 补 `..Default::default()`。
* `crates/raybend/assets/develop-params.json` + `develop/params.rs`：`areaTolerance` 100/1000/300。
* `crates/raybend/src/thumbnail/render.rs`：`PIPELINE_VERSION 19 → 20` + 四处签名字面量。
* `src-tauri/src/editor.rs`：`PointWeightDto`、`DevelopParamsDto.mask_debug`、`SetParams.weight`、`SessionParams.weight`、`DevelopJob.weight`、`editor_mask_defaults` 命令。
* `src-tauri/src/lib.rs`：注册命令。

**前端**
* `src/features/editor/area-markers.tsx`：白线长度=容差、1:1 拖动、名字锚在采样点端、SOOC 半透明。
* `src/features/editor/area-panel.tsx`：武装态按钮（主色、再点取消）、删幽灵行、SOOC 禁用与置灰、删除改用共享 `RowDragSort`（上一批）。
* `src/features/editor/store.ts`：`pointArmed` / `armPointArea` / `disarmPointArea` / `placePointArea`（去掉 `pendingPointAreaId` / `placePendingPoint` / `cancelPendingPoint` / `seedArea`）、容差夹取、`seedVersion`、载荷里的 `maskDebug` 与 SOOC 映射、`adoptAreaSeed`。
* `src/workspaces/editor/EditorWorkspace.tsx`：落点新流程、旧种子迁移、错误 → toast（含 `store.error()`）、武装态 flowbar 提示、SOOC 自动解除武装、`inactive` 传给标记。
* `src/features/editor/panels.tsx`：层名条（上一批）+ 删掉内联错误（`error` 属性 → `onError`）、直方图 `!mb-0`。
* `src/shell/FlowBar.tsx`：提示区左锚定。
* `src/shell/TitleBar.tsx`：DEV 调试入口。
* `src/dev/MaskDebugDialog.tsx`（新）、`src/lib/mask-debug.ts`（新）、`src/lib/flow-hint.ts`（上一批）。
* `src/api/editor.ts` / `src/api/types.ts`：`seedVersion`、`maskDebug`、`getMaskDefaults`。
* `src/i18n/{zh-CN,en-US}.ts`：`debug.mask.*`、`editor.area.pointNeedsRaw`、`editor.area.ctrlDragHint`。

**文档**
* `specs/editor-area.md` §4.2（权重公式与数值域）、§5.2（层名条）、§5.3/§5.4（白线 / 名字 / handle）、§5.6（新建流程与 SOOC）。
* `design/editor.md`（9b 条）、`design/main.md`（flowbar 提示区）。
* `memory/COMPONENTS.md` §3.2（开发期工具）/ §3.4（错误唯一通道）。
* `AGENTS.md` §2.29（错误只走统一消息通道）。

## 5. 验证

```bash
cargo check --workspace --all-targets    通过
cargo test --workspace                   1626 + 1 + 7 + 4 + 194 + 2 + 2 = 1836 passed / 0 failed
pnpm typecheck                           通过
pnpm test                                1380 passed / 0 failed
pnpm lint:colors / lint:arch / lint:i18n  全过
pnpm build                               通过
```

新增/改写的关键测试：
`same_luma_different_hue_is_rejected`、`point_weight_is_the_product_of_the_two_bands`、
`band_is_flat_inside_the_core_and_smooth_down_to_the_end`、`feature_vector_is_perceptual_and_neutrals_have_no_chroma`、
`武装不建层…`、`控制点：落点写采样位与种子；容差是 100..1000 的数值域`。

## 6. 需要崔总验证 / 注意

* **调试面板**：`pnpm debug:win` 出的 DEV 包里 titlebar 主题开关左边有个虫子按钮；
  调完把数字发我，我固化进常量并抬 `PIPELINE_VERSION`。
* **旧控制点会被自动重新取样一次**（种子定义变了）；旧**定稿**里的控制点不会迁移，
  需要重新定稿。
* **默认容差 300** 的手感：亮度轴在 300 时 `core ≈ 0.25`，比上一版宽不少 —— 这是「选全一件衣服」
  所必需的；如果整体太松，先调 `luma_core` 的 1000 端，不要动求值代码。
* `grep -rn 'text-danger' src` 剩下的命中：对话框内联校验（`ImportProgressDialog` /
  `ExternalEditorDialog` / `TagDialog` / `AiDialog` / `BucketDialog` / `LicenseSettingsDialog`…）、
  以及浏览左列的「库列表读取失败」常显状态（它是持久状态不是瞬时错误）——**这两类按纪律保留**。
* Windows 产物：写这份记录时**尚未重编**（下一步 `pnpm build && pnpm debug:win`）。
