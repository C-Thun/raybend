# 发版后登记：调色 / 编辑性能 / 工程信用

> 完成时间：2026-11-02 20:54:51
> 范围：只改 `memory/FUTURE.md`（外加本记录），不动代码、不动测试。

## 1. 做了什么

按崔总指示，把本轮评估里提到的后续项统一登记到 `memory/FUTURE.md` 的
**「🧊 发版后（after release）可立即做」** 板块，供发版后参考：

- **调色与编辑性能后续**
  - `area` 层链 GPU 化，升为发版后第一优先；写明 `working_preview.rs` 的 `fast` 判据、
    24MP 基线约 75ms、多层链尚无真机基准、建议的 WGSL 做法与 CPU/GPU 离屏对照。
  - 空层快速路悬崖：`LayerStack::is_degenerate()` 只看 `areas.is_empty()`，
    空 `全图` 层会误关 GPU 快速路；`render_layered16` 其实有「空层不跑」短路。
  - 局部色盘（area 层）与 `crs:ColorGrade*` 二期；后者没有 Lightroom 样本前不写 `crs:`。
  - 四路色盘的实现口径：**不要按每像素 luma 三段权重**；Resolve 的 lift/gamma/gain 本质是
    每通道 1D 曲线，参数期解析成 `lift/gain/gamma/offset` 三通道系数，像素期折进现有 1D 表。
  - 数据模型建议（12 个标量参数 vs `ColorGrade` 结构体的取舍）与 UI/设计口径
    （2×2、level 条、双击复位、`design/editor.pen` 先行、对外不用 DaVinci 商标）。
- **调色工具面补齐**：tint（绿/洋红）+ 白平衡吸管、HSL/颜色混合器、渐变/画笔蒙版 + 封闭控制点、历史面板/快照。
- **工程信用与可维护性**：应用质量门 CI（明确不是发版 CI，发版 CI 仍按 G24 不排期）、
  `cargo fmt`/clippy 门禁、文档真相审计、`CONTRIBUTING.md` + 架构概览 + issue 模板、解析器 fuzz、流程分档。
- **真机 beta 与性能基准**：24/60MP、三档 DPI、多显示器、ICC、真实 1000+ 图库；
  area/色盘性能做成可重复基准，不用推测代替数字。

同时更新了：

- 已有「编辑计算继续 GPU 化」条目：把 **area 层链**加入「还在 CPU」并升为建议顺序第一。
- **D11**：标题从「三路色彩圆盘 / 本轮不做」改为「四路色彩圆盘（阴影/中间调/高光/偏移）/
  2026-11-02 改为发版前做」，并把「按 luma 权重混合」修正为 per-channel 1D 实现建议；
  原「本轮不做」作废。
- 变更记录新增 2026-11-02 一行。

## 2. 关键决策与理由

1. **四路色盘本身放发版前**（崔总原口径），发版后板块只登记它的后续：
   局部色盘、`crs:` 映射、以及它依赖的 area GPU 化。避免把「发版前要做的事」误放进发版后背板。
2. **per-channel 1D 而不是逐像素三段权重**：后者跨通道，不能折进现有 1D 表，
   会让 GPU 快速路与 area 层性能同时退化；前者能折进默认层 8192×3 影调表与 area 层 `DisplayLuts`，
   中性短路还能保持旧签名/旧字节不变。
3. **`crs:ColorGrade*` 先不写**：Lightroom 的 Shadow/Highlight Hue/Sat 仍走 `SplitToning*` 旧键，
   `Blending/Balance` 语义未公开；没有逐项对照样本时，近似值被外部软件当真值写回，比不写更糟。
4. **应用 CI 与发版 CI 分开**：G24 已明确发版 CI 不排期；这里登记的是公开仓库的质量门
   （`cargo test --workspace`、typecheck、test、三个 lint、build），不碰发版流程。

## 3. 验证

- `memory/FUTURE.md` 的两处编辑均通过 Markdown 诊断（`✓ Markdown clean`）。
- 文档改动，未跑代码测试；引用的代码行为（`fast` 判据、`is_degenerate`、`render_layered16` 短路、
  24MP 基线）均来自当前工作树与既有实施记录。

## 4. 遗留

- 四路色盘的正式规格 `specs/editor-color-grade.md` 与 `design/editor.pen` 四帧尚未写；
  按项目纪律，开工前先补这两份，不在本轮范围。
- `memory/PLAN.md` 里「编辑区未实施 / W2–W6 待做」的过期状态没有在本轮改；
  已作为「文档真相审计」条目登记到发版后板块，避免顺便扩大本轮范围。
