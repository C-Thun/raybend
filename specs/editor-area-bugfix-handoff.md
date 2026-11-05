# 编辑区（area）—— 修 bug 换手交接书

> **给接手会话**：这是**唯一入口**。本轮（W2–W6 + 一轮代码评审修复）已全部落地并通过单元测试，
> 现在的任务是**修 bug / 补验收问题**。本文件只写「现在到哪了 / 已经怎么做的 / 还缺什么 / 别踩哪些坑」。
>
> 口径与设计事实：`specs/editor-area.md`（规格）、`design/editor.md`（画稿说明与决策）、
> `design/editor.pen` 四帧（`E6tDP` / `SPyhh` / `xjwv9` / `A0UG5v`）。
>
> 建立：2026-10-31（第三次换手：W0 → W1 → W2–W6 → **修 bug**）

## 0. 状态牌

| 项 | 状态 | 证据 |
| --- | --- | --- |
| W0 设计稿四帧 | ✅ 崔总 2026-10-31「没问题」 | `design/editor.pen` + `design/editor.md` §「2026-10-31 编辑区（area）面板」 |
| W1 类型 / 迁移 / 定稿 v3 / 参数 `target` | ✅ | `implementations/2026-10-31_editor-area-w1-types.md` |
| W2 渲染（层循环 / 层内固定管线 / 控制点权重 / `PIPELINE_VERSION` 18） | ✅ | `implementations/2026-10-31_editor-area-w2-render.md` |
| W3 撤销与持久化（`area_id` + 7 种结构 op） | ✅ | `implementations/2026-10-31_editor-area-w3-undo.md` |
| W4 面板与画布（area panel / 层名条 / 标记 / 眼睛 / 命令） | ✅ | `implementations/2026-10-31_editor-area-w4-panel-canvas.md` |
| W5 周边（预设 / XMP / auto_adjust / 缓存） | ✅ | `implementations/2026-10-31_editor-area-w5-surrounds.md` |
| W6 收尾（i18n / 文档 / 质量门 / Windows 产物） | ✅ | `implementations/2026-10-31_editor-area-w6-closeout.md` |
| 第三方评审 + 修复 | ✅ | `implementations/2026-10-31_editor-area-review-fixes.md` |
| **真机 / GUI 验收（第 1 轮）** | ⚠️ 崔总 2026-11-01 报回 12 条（样式 / 手势 / 持久化 / 复用 / 配色 / 容差语义） | `implementations/2026-11-01_editor-area-bugfix-batch.md` |
| **第 1 轮修复** | ✅ 已落地并通过质量门（**待崔总在新产物上复验**） | 同上；验收清单见 §4 |

**测试产物（含全部修复）**：`/mnt/c/rb-target/raybend/debug/raybend-desktop.exe`
（`pnpm debug:win` 生成，`pnpm check:win` 已核对）。

**当前质量门读数**（2026-10-31 23:55 前后）：

```text
cargo check --workspace --all-targets   通过（仅既有未使用告警）
cargo test --workspace                  1832 passed / 0 failed
pnpm typecheck                          通过
pnpm test                               1378 passed / 0 failed
pnpm lint:colors / lint:arch / lint:i18n 全过
pnpm build                              通过
```

## 1. 现在能用的功能（行为口径，修 bug 时不许破）

1. **三种层**：`默认`（栈本身，不可删、可移动）/ `全图` / `控制点`；新建追加到末尾，
   列表自上而下 = 计算顺序；默认层可以拖到任意位置（这就是「LUT 在某个全图层之前 / 之后」的唯一手段）。
2. **层行**：行高 30、类型图标（默认 = 滑杆图标 / 全图 = 方块 / 控制点 = 色点）、
   双击行内改名（≤30 字符）、启停开关、删除（禁行图标）、整行拖动排序；默认行右侧是锁。
3. **控制点**：面板「新建控制点」→ 待落点（视口变十字光标、面板末尾出描边幽灵行）→
   画布点一下 → Rust 取样「该层输入」的特征向量 → 自动成为当前层；`Esc` 取消（不留空层）。
4. **画布手势**：
   * 单击圆片 = 选中该层；**普通拖动不接管**（继续交给视口平移）；
   * **`Ctrl`（macOS `⌘`）+ 左键拖圆片 = 移动采样位**，种子**不重取**；
   * 拖引线末端的 handle = 改**容差**（横向，往外变大，末端显示 `容差 N`）；
   * 没有范围圆（容差是向量尺度，不是空间半径）。
5. **眼睛（`显示控制点`）**：toolsbar right，纯视图偏好（`localStorage` v4 的 `showPoints`），
   关掉时标记不画、抓取同时关闭。
6. **非默认层禁用**：右侧「镜头 / 清晰度 / 局部色调 / 色彩管理 / 基础曲线」与左列 LUT 面板
   灰掉 + 浮条 `当前编辑区域下不可用`；`validate()` 里还有硬约束（Global/Mask 参数不许进层）。
7. **撤销**：建层 / 删层 / 排序 / 改名 / 启停 / 换色 / 控制点六数（位置·种子·容差）/ 逐层参数曲线，
   全部走同一个撤销栈；`ChangeSet::invert` 是**逆序**重放（层排序是多步 `AreaMove`）。
8. **退化形状**：没有额外层时 `areas` 不序列化、`profile_hash` / `signature()` / 侧车字节
   与改造前**逐字节相同**；渲染走原快路径（`map_rows` / `LocalPass`）。
9. **LUT 次序**：LUT 现在挂在**默认层那一段的末尾**（= 单层照片从「锐化之后」变成「锐化之前」），
   这是**有意的画面变化**，靠 `PIPELINE_VERSION 17 → 18` 让旧缓存失效。
10. **预设**：恒以**默认层**为作用对象（捕获与套用都是），不新建 / 不删除层，Mask 参数不进预设。
11. **XMP**：`crs:` 只写默认层；层只进 `rb:profileJson`（`areas` / `default_index`，snake_case）。

## 2. 代码地图（按功能找落点）

| 面 | 文件 / 符号 |
| --- | --- |
| 层的渲染形态与权重 | `crates/raybend/src/develop/areas.rs`：`AreaSpec` / `AreaLayer` / `LayerStack::from_specs` / `feature_vector` / `point_weight` / `mask_weights` |
| 层链求值 | `crates/raybend/src/develop/pipeline.rs`：`PreparedArea` / `LayerChain` / `render_layered16`（约 1248 行）、`compose_linear_at`（落点取样，约 1351 行）、`DevelopStages.areas` |
| LUT 次序 | 同文件 `render_develop16`（LUT 在默认层段末尾、锐化之前） |
| 数据形状 / 校验 / 写入口 | `crates/raybend/src/store/develop.rs`：`Area` / `AreaKind` / `DevelopStack.areas` / `default_index`、`area_rows`、`set_area_param` / `set_area_curve` / `add_area` / `remove_area` / `move_area` / `rename_area` / `toggle_area` / `set_area_color` / `set_area_point`、`Area::to_spec` |
| 撤销 op | `crates/raybend/src/store/marking.rs`：`Op::Area*`、`ChangeSet::invert`（逆序） |
| 差量生成 | `src-tauri/src/develop.rs`：`diff_stack` / `diff_values` / `diff_areas`；`DevelopStackDto.areas/defaultIndex`；`develop_area_seed` |
| 编辑器渲染载荷 | `src-tauri/src/editor.rs`：`DevelopParamsDto.areas/defaultIndex`、`SessionParams.areas`、`DevelopJob.areas`；`editor_working.rs` |
| 显示 / 缩略图 / 导出 | `crates/raybend/src/display/output.rs`（`stack_layers`、`render_linear`、`render_working`）；`thumbnail/render.rs::PIPELINE_VERSION` |
| 前端 store | `src/features/editor/store.ts`：`layerRows` / `rowsToLayers` / `layersToRows` / `activeAreaId` / `createArea` / `requestPointArea` / `placePendingPoint` / `cancelPendingPoint` / `removeArea` / `moveAreaRow` / `setAreaPoint` / `showPoints` / `developPayload` |
| 面板 | `src/features/editor/area-panel.tsx`（行、拖动、改名、待落点幽灵行、32 层禁用） |
| 画布标记 | `src/features/editor/area-markers.tsx`（`<Index>` 复用 DOM、`Ctrl` 拖动、容差 handle） |
| 覆盖层数学 | `src/lib/overlay-math.ts`（**只做 Rust 矩阵的逆变换**）；`src/features/editor/ViewportOverlay.tsx` |
| 右栏层名条 / 全局禁用 | `src/features/editor/panels.tsx`（层名条 22 高；`onDefaultLayer` 判定） |
| 工具条 / 左列 | `src/features/editor/toolbar.tsx`（第三开关 + 眼睛）、`left-panel.tsx`、`lut-panel.tsx`（`disabled`） |
| 命令 | `src/features/commands/catalog.ts`（`editor.area.*` / `editor.area.showPoints`）、`src/App.tsx`（`editor:` 依赖实现） |
| 布局偏好 | `src/lib/editor-chrome.ts`（`leftPanel: "lut" \| "issues" \| "areas"`）、`src/lib/editor-prefs.ts`（**v4**，`showPoints`） |
| 令牌 | `src/styles/tokens.css`：`--point-0..7`、`--overlay-text-shadow` |
| 命令参数 / 契约 | `crates/raybend/assets/develop-params.json`（含 4 条 `target: "mask"`）、`src/features/editor/params.ts`（`POINT_TOLERANCE` / `AREA_OPACITY`） |

## 2.5 2026-11-01 第 1 轮真机反馈与修复（**先读这一节**）

12 条反馈全部落地，逐条对照表在 `implementations/2026-11-01_editor-area-bugfix-batch.md` §1。要点：

| 类别 | 结论 | 现在的事实 |
| --- | --- | --- |
| 标记样式 | 圆片改**三环同心**（1px 黑 / 2.5px 白 / 13px 色芯）；引线改**水平**、白线 + 细黑勾边；名字在**线上**、白字 + 勾边 | `area-markers.tsx`（规格 §5.3 已改；`design/editor.md` 9a 条为准） |
| 画布手势 | 标记/handle 用**原生** `on:pointerdown`（Solid 的 `onPointerDown` 是委托事件，挡不住洞口的平移） | 拖 handle 只改容差、`Ctrl` 拖圆片只移采样位，都不再带动画面 |
| 实时跟随 | 新增 `editor_overlay_state` + `lib/overlay-live.ts`：手势期间**每帧**读一次矩阵，静止不读 | `ViewportOverlay` / `AreaMarkers` 都优先读实时那份 |
| 持久化 | `Area.params/curves` 不再 `skip_serializing_if`（空层省字段 ⇒ 前端 `Object.entries(undefined)` ⇒ 整次载入作废） | 建过 area 退出再进来**层还在**；issue 里也带层（`profile_schema_version` v3） |
| 容差语义 | **两段式**：`core = tolerance × core_scale` 内 100%，`core..core+falloff` smoothstep 降到 0 | 两个比例在 `develop/areas.rs::POINT_WEIGHT_CONFIG`；`PIPELINE_VERSION 19` |
| 复用 | LUT 链与 area 层的整行拖动合成一份 `RowDragSort`；四种删除统一 `RemoveButton` | 组件库方案：`memory/COMPONENTS.md`（`AGENTS.md` §12 指向它） |
| LUT | **恒可用**（编辑的永远是默认层那一段），不再因「当前是控制点层」变灰 | `lut-panel.tsx` / `left-panel.tsx` |
| 配色 | 控制点八色压暗一档（压在白圈上要质感） | `tokens.css` `--point-0..7` |

## 3. 已知问题清单（修 bug 的作业单）

> 优先级：**P0 = 阻断 / 明显错**、**P1 = 功能缺口或体验明显不对**、**P2 = 打磨**。
> 每条都给了位置与建议方案；**先复现、再改，改完补一条测试**。

### P0

*（无已知 P0。若真机验出「拖动卡死 / 画面错位 / 撤销跳步」这类，按 P0 处理并在
`implementations/` 起新文件，不要改历史记录。）*

### P1

1. **控制点在几何（裁切 / 旋转）变化后会漂**
   位置：`crates/raybend/src/develop/areas.rs::AreaKind::Point { x, y }`（存的是**显示画幅**归一化坐标）、
   `src-tauri/src/develop.rs::develop_area_seed`（先 apply 几何再取样）。
   现象：先放控制点，再确认裁切 —— 控制点跟着「归一化坐标」贴着新画幅，视觉上不再指向原来那团颜色；
   种子没变，所以效果会落到别的区域。
   建议：几何确认时把已有控制点的 `(x, y)` 一起重映射（用几何的逆变换把「旧输出归一化」→「源归一化」→「新输出归一化」）；
   若嫌重，至少在改几何时给一次提示。**注意**：这是语义问题，改之前先找崔总确认口径。

2. **「该层输入已变 → 重新取样」没有入口**
   位置：规格 §5.6 / §5.9 D4；UI 在 `area-panel.tsx` / `panels.tsx` 的层名条右侧（未做）。
   现状：种子是冻结的（这是对的），但用户改了下面的层之后没有任何提示与补救手段。
   建议：在**创建**控制点时记一个「输入指纹」（例如「该层之前所有层 + 默认层参数」的
   `DevelopStack::signature()` 子集，或前端对这些数据做一次稳定序列化），层名条右侧在指纹变化时出现
   「重新取样」；点击走 `sampleAreaSeed(..., layer)`（命令已支持 `layer` 参数）并只写 `seed`
   （不动 `x/y/tolerance`）。落库仍是一次 `confirmEdit`。

3. **有额外层时的渲染性能（24MP）需要真机确认**
   位置：`pipeline.rs::render_layered16`（每帧重建每层 `LinearChainLuts` / `DisplayLuts` / `DecodeLut`，
   再跑全画幅逐像素内循环）。
   风险：拖动一个控制点层的参数时，如果层数多 / 参数多，可能明显掉帧。
   建议（按代价从低到高）：① 把 `PreparedArea` 的表按「层参数指纹」缓存（参数没变就不重建）；
   ② 参数变化时只重建受影响那一层；③ 若仍不够，考虑把「只含线性项的全图层」预折叠（规格 §4.6 的可选项）。
   **先用真机测**（`develop_ms` 在渲染状态里），不要凭猜优化。

4. **局部色调的引导图是近似（默认层被移动时）**
   位置：`src-tauri/src/editor.rs` 的 `LocalToneState::analyze(&chained, …)` 与
   `crates/raybend/src/display/output.rs::render_linear` 的 `chain_image(linear, &params)`。
   现状：引导图按「源图 + 默认层线性参数」算，**不含**默认层之前的其它层。
   建议：默认层不在第 1 位时，改为按「默认层之前所有层」的合成结果生成分析图；或至少在规格里确认这是
   可接受的近似（已登记在 `specs/editor-area.md` §4.3，**先问再改**）。

5. **v2 色彩路径有额外层时 `>1` 高光被夹**
   位置：`crates/raybend/src/develop/working.rs::render_working_reference`（有额外层时中转一次
   `0..1` 的 16bit 线性图）。
   建议：把层循环搬进工作域（`WorkingTonePlan` + f32），而不是中转。改动大，属"有余力再做"。

6. ~~**标记在图像边缘会被裁**~~ ✅ 2026-11-01 部分修复
   位置：`src/features/editor/area-markers.tsx`。
   现状：引线已是**水平**（左半点向右、右半点向左），横向不会再顶出洞外；
   竖直方向：贴洞顶（上方不足 26 CSS px）时名字翻到线下方。
   仍存在：左右极窄画面下名字末端可能被洞口裁掉（未做按文字宽度翻边 —— 会随缩放抖）。

### P2

1. **撤销 / 重做后当前层回到默认层**（`loadDevelop` 会把 `activeAreaId` 重置为 `null`）。
   规格只要求「换照片回默认层」；撤销后回默认层可能让用户以为层没了。建议：撤销重读时保留
   `activeAreaId`（仍然存在就保留，不存在才回默认层）。

2. **层不透明度没有 UI**（`Area.opacity` + 撤销 op 都已就位）。
   规格 §10 明确「近期不给」，要做先问。

3. **toolsbar 挤压态几何未重量**：左段从 2 个开关变 3 个，560 宽时渐隐 / 覆盖需要重新对一遍
   （`design/editor.md` 已注明）。

4. **派生的三张状态帧（裁切 / 旋转 / 对比）左列还是旧版**（两开关），没跟着加「编辑区」。

5. **32 层上限只有禁用 + tooltip**（`editor.area.maximum` = 「最多 32 层」），没有 toast。
    若要补，走 `Toast`（`src/components/ui/Toast.tsx`）。

6. **行内改名的 `maxLength=30` 按 UTF-16 code unit**（Rust 按字符）。
    粘贴大量 emoji 时前端会先截到 30 units（少于 30 字符）。低危，要修就在 `onBlur` 统一按
    `Array.from(name)` 截断（Rust 侧已有同样的截断）。

7. **`DevelopStack::len()` 不计 `areas`**（`is_empty()` 已计）。目前没有消费者依赖它，
    改之前先 grep 调用方。

## 4. 崔总的验收清单（修 bug 的输入）

按这个顺序验，出问题就记「第几条 + 期望 vs 实际」：

1. 新建两个控制点、各自调曝光/饱和度 → **只有预期区域变**；
2. 调容差（拖 handle）→ 渐变宽度跟着变；容差 0 时只影响种子像素；
3. `Ctrl` 拖圆片 → 采样位移动、**画面不变**（种子没重取，这是预期）；重新落点才会换种子；
4. 建两个全图层，把默认层拖到它们之间 → **LUT 的前后位置**跟着变（这是唯一手段）；
5. 层改名 / 启停 / 删除 / 排序 → 每一步都能 `Ctrl+Z` 原样回来（位置、名字、颜色、容差）；
6. 存定稿 → 切 SOOC → 切回定稿 → 层与画面一起回来；
7. 存预设 → 套到另一张 → 层不变、默认层参数按预设覆盖；
8. 导出 → 与编辑器画面一致（LUT 次序已变，注意对比）；
9. 32 层上限：两个新建按钮变灰、tooltip 显示「最多 32 层」；
10. 眼睛关掉 → 标记消失且点不到；再打开 → 原样回来（含选中态）。

## 5. 开工纪律（每次改动都要守）

* **质量门**（缺一条都不算完）：

  ```bash
  cargo check --workspace --all-targets
  cargo test --workspace          # ❗必须 --workspace，-p 会漏别的 crate 的测试目标
  pnpm typecheck && pnpm test
  pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n
  pnpm build                      # 动了前端就要跑
  ```

* **改渲染 / 缓存签名**：动 `pipeline.rs` 的逐像素语义或 LUT 次序 ⇒ **必须**抬
  `thumbnail/render.rs::PIPELINE_VERSION` 与 `render_sig` 的四处字面量（否则毒缓存）。
* **退化形状**：`DevelopStack` 加字段一律 `#[serde(skip_serializing_if = ...)]` 退化掉；
  `areas` 为空时字节必须与改造前一致（`profile_hash` / `signature()` 有测试钉着）。
* **加参数**（将来若要给控制点加新滑杆）：按 `AGENTS.md` §13.1 的 8 步走，
  `xmp/mapping.rs` 必须归 `SCALARS` 或 `NOT_MAPPED`。
* **撤销节点**：新加任何会改 profile 的交互，必须在 `EditorWorkspace.confirmEdit` 的节点表里补一行，
  并接到 `onConfirm`（漏一个就跳步）。
* **不要动**工作树里**别的会话**的改动（LUT 链、窗口恢复、弹窗描边等）；
  本轮内容也**尚未提交**，需要提交时按文件分组提交，别把别人的半成品带上。
* **别扩大范围**：本文件 §3 之外的"看着不顺眼"先记录，不动手。

## 6. 明确不做（这一轮）

* 封闭控制点（`memory/FUTURE.md` D14）、蒙版 / 画笔 / 智能选区；
* 层复制 / 粘贴（D12）、三路色彩圆盘（D11）；
* 层不透明度 UI（规格 §10 明确「近期不给」）；
* 控制点的"影响面可视化"（容差是向量尺度，圈不出来）。
