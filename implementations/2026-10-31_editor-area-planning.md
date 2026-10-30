# 编辑区（area）系统：规划与记忆落盘

完成时间：2026-10-31 03:50:58（**只有文档，没有代码**；本轮不实施开发）

## 1. 做了什么

崔总口述了一套「多层编辑栈 + 控制点」的交互方案，本会话做的是**评估 + 调研 + 落盘**，不写产品代码：

* 读过并核对了现有实现，把方案的每一处都钉在真实代码上（不是纸面设计）。
* 四轮外部调研（U Point 专利原文与同族、DxO / 尼康现行产品口径、Lightroom `crs:MaskGroupBasedCorrections`、相关论文）。
* 崔总拍板 12 条口径（`specs/editor-area.md` §10），据此写出规格。
* 术语、红线、影响索引、远期项分别写进对应记忆体。

## 2. 涉及文件（全部为文档）

| 文件 | 变更 |
| --- | --- |
| `specs/editor-area.md` | **新增**（319 行）：数据契约、求值与阶段边界、交互规格、W0 设计清单、W0–W6 波次、W1 细化、影响面、决策账本、风险 |
| `AGENTS.md` | 新增 §2.27（层边界 = 管线阶段边界，红线）；§11.1 加 `area panel`；§11.5 加 `area` / `control point` / 眼睛；§13 加「编辑区」影响行 |
| `memory/PLAN.md` | §1 路线总览加一行；§4 新增「编辑区（`area`）系统」小节；§6 决策记录加三条；§7 变更记录加一条 |
| `memory/FUTURE.md` | §D4 补记 area 与蒙版显示口径；新增 D11（三路色彩圆盘）、D12（层复制/粘贴，标注「有人给我钱了再做」）、D13（`crs:MaskGroupBasedCorrections` 二期） |
| `docs/native-viewport-coordinate-guide.md` | 「四种量」→「五种量」，新增 `area space` 与 §「area space：编辑区坐标」 |
| `implementations/2026-10-31_editor-area-planning.md` | 本文件 |

## 3. 关键决策与理由

1. **层间顺序累积（不是归一化竞争）**：专利式 \[21\] 的归一化会让层序变成装饰、且调一层会改变另一层的强度
   （不可测、不可撤销）；顺序累积让每层是独立可测的一次编辑，与「层」的直觉一致。代价是重叠区叠加过头，
   本来打算用层不透明度收，但崔总 2026-10-31 明确「近相当长一段时间不做透明度」——于是 `opacity`
   字段保留、无 UI，代价记为已知取舍（规格 §10 D3 / R1）。
2. **层边界 = 管线阶段边界**：核对 `develop/pipeline.rs::render_develop16` 的实际顺序
   （镜头 warp → 降噪 → 逐像素影调/色彩/曲线 → 锐化 → LUT）后确认：层内只放逐像素算子。
   这条不只是"设计取舍"，它换来的是**合成不需要中间图** —— 一个像素里内循环走层即可，
   `map_rows` 的行并行与缓存签名都不用动。写进 `AGENTS.md` §2.27 当红线。
3. **v3 schema 按内容分支，不迁移旧数据**：现有 `profile_schema_version` 本来就是「有 color 才是 2」，
   照抄成「有额外层才是 3」+ `skip_serializing_if` 退化不序列化，于是没用到层的照片
   **序列化字节、`profile_hash`、侧车、缓存签名全部与今天逐字节相同**。这消掉了原先担心的
   「升版会让旧定稿打不开」的坑。
4. **`crs:` 不撒谎**：层的存在对 `crs:` 是不可表达的，写"合成后的扁平参数"等于埋雷（外部软件存一次就当真值写回）。
   口径定为：`crs:` 只写真正全局的参数，层只进 `rb:profileJson`。
5. **眼睛的语义收窄**：只控制控制点标记（圆点/引线/名字），纯视图偏好；蒙版可视化是另一套半透红区。
   控制点**不做**影响面可视化。

## 4. 遗留问题

* **唯一待确认**：LUT 的落点。现在 LUT 在全局链末端（锐化之后）；若严格按「自上而下第一个全图 area 的输出」
  套用，它会跑到锐化之前，单层照片画面会变（违反不变量 1）。规格 §4.3 已给出建议：v1 保持现状，
  把锚点登记为未来语义。**需要崔总一句话确认。**
* **求值方向**：崔总两轮口述分别说「从最下往上」与「从上向下」。规格按**底层→顶层**定义并在 §4.4 标注了差异，
  若本意相反，改动只在 §4.1 的循环方向。
* **W0 设计稿未画**：`design/editor-area.pen` + 同名 `.md` 六屏（规格 §6）。这是开工门禁，下一轮第一件事。
* 未登记 `todo`：波次在 W0 定案、正式开工那一刻一次性登记（`AGENTS.md` §5.5）。
* 未改动的相关文档（留到 W5/W6）：`specs/xmp-sidecar.md` §5.1、`specs/issue-xmp-contract.md`、
  `memory/FUNCTION-IMAGING.md`、`memory/ARCHITECTURE.md` §10 变更记录。

## 5. 验证方式

读了以下源码得出结论（未运行测试：无代码改动）：

* `crates/raybend/src/store/develop.rs` —— `DevelopStack` 字段全貌、`is_empty`/`len`/`validate`。
* `crates/raybend/src/store/issues.rs` —— `PROFILE_SCHEMA_VERSION`、`profile_schema_version`、`profile_hash`、
  `list()` 的两条版本/指纹检查（确认了"升版会打不开"的实际机制，以及按内容分支可以绕开）。
* `crates/raybend/src/store/marking.rs` —— `Op` 全部变体（确认**没有任何结构操作**，且 `DevelopParam` 只带 `param_id`）。
* `crates/raybend/src/develop/pipeline.rs` —— `render_develop16` 的阶段顺序（层边界结论的来源）。
* `crates/raybend/src/develop/params.rs` + `store/presets.rs` + `store/issues.rs` —— 参数表契约、预设 payload 版本形状。
* `src/features/editor/{toolbar,left-panel,panels}.tsx` —— toolsbar 左段现有两个开关、左列上下面板结构与右栏直方图位置。

外部调研（全部为一手或准一手）：US6865300B2 专利原文（Analyzing / Difference / Inversion / Controlling / Mixing
五零件与式 \[12\]\[16\]\[17\]\[21\]\[22\]）、US2008/0137952A1（区域版）、尼康 NX Studio 帮助页（现行控制点行为）、
DxO U Point 技术页与 PhotoLab 用户指南（Chroma / Luma 两根灵敏度滑杆）、Lightroom `MaskGroupBasedCorrections`
的字段与顺序行为、以及双边滤波 / 归一化卷积 / 交互式局部色调 / 编辑传播等论文。
