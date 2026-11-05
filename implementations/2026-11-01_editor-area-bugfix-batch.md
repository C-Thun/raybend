# 编辑区（area / 控制点）—— 真机反馈批修

> 完成时间：2026-11-01 03:26:55
> 输入：崔总 2026-11-01 的真机截图与逐条反馈（`specs/editor-area-bugfix-handoff.md` §3 之后的实际使用）

## 1. 这轮修了什么（按反馈逐条对照）

| # | 反馈 | 根因 | 修法 |
| --- | --- | --- | --- |
| 1 | 静圆片样式不对：白圈过厚、色点与白圈不同心、外面一圈是**同色**描边而不是黑边 | 圆片用「有边框的 div + 绝对定位的色点」拼，边框宽 2.5、色点按 padding-box 定位；选中态又叠了一圈 `outline: color()` | 改成**三环同心**：`--overlay-line` 白底 + `inset 0 0 0 1px --overlay-halo` 黑圈 + 13px 色芯（白圈正好 2.5px）；删掉同色 outline |
| 2 | 容差横线是一条同色细线、还是斜的、方向也错 | 引线画成 45° 斜线、颜色 = 控制点色、1px | **水平**引线（左半点向右、右半点向左，长 70 从圆片边缘起），白线 1.2px 叠在 2.6px 黑线上（细黑勾边） |
| 3 | 名字在 handle 边、宽度极小、纯黑字 | 名字放在引线末端外侧，还用 `max-w-40 + fade-x-end`（20px 渐隐吃掉短名字）；颜色走 `text-fg-1` | 名字放到引线**上方**（左端 +6、基线 −7），11px，**白字 + 细黑勾边**（SVG `paint-order: stroke`）；不再渐隐 |
| 4 | 拖 handle 完全没反应、直接拖图；`Ctrl` 拖圆片时图也动、点跑得比鼠标快 | **Solid 的 `onPointerDown` 是委托到 document 的** —— 洞口（祖先元素）上的原生 `pointerdown` 先跑，`stopPropagation()` 来得太晚，于是每次都顺手起了一次平移 | 标记与 handle 的按下改**原生监听** `on:pointerdown` + 就地 `stopPropagation`，洞口再也收不到这一下；位移改成「相对按下点的总位移」（原来是逐帧累加，叠上视口平移就是 2×） |
| 5 | 缩放 / 拖动时控制点不跟手，停手才跳过去 | 覆盖层矩阵只来自 250ms 的状态轮询 | 新增 `editor_overlay_state`（只锁一次、拷 6 个浮点）+ `lib/overlay-live.ts`：**手势期间每帧**读一次，静止时一次都不读；`ViewportOverlay` 与 `AreaMarkers` 都优先读实时那份 |
| 6 | 改控制点参数像全屏改 | 见 §3「核心问题」——这轮用**端到端测试**钉住 DTO → 层栈 → 像素这条链，并修掉真正让「层读不回来」的那个 TypeError（#7） | `editor::tests::point_area_params_only_change_pixels_that_match_the_seed` |
| 7 | 真机出现 `TypeError: Cannot convert undefined or null to object`，且「建过 area 再进来只剩默认一层」 | `Area` 的 `params` / `curves` 带 `skip_serializing_if = is_empty`：空层从 Rust 回来**整个字段不存在**，前端 `Object.entries(area.curves)` 抛错，整次 `loadDevelop` 作废 | ① Rust 侧**不再省字段**（空层也写 `"params": {}` / `"curves": {}`，`serde(default)` 仍能读旧数据）；② 前端 `loadDevelop` / `paramValue` / `setParam` / 曲线四处按可选读；③ 补 2 条回归测试（`areas_roundtrip_through_the_catalog`、`载入层时 params / curves 缺字段也不能炸`） |
| 8 | 层上下拖动没有复用 LUT 那一套 | 两处各写了一份拖动状态机（area 那份还少了幽灵、边缘滚、`label` 排除） | 抽出 `components/ui/RowDragSort.tsx` + `lib/row-drag-sort.ts`（几何原名 `lut-*`，已改名），LUT 与 area 共用；"不参与拖动"的选择器改**追加**在默认组之后（Ark 的 Switch 根是 `<label>`，之前 area 行点开关会起拖动） |
| 9 | 四种删除用了三种图标 | 历史遗留 | 统一成 `RemoveButton`（禁行图标）：area 行、定稿行改用它（LUT / 导入「最近」本来就是），并写进 `memory/COMPONENTS.md` §3.1 |
| 10 | LUT 在切到控制点层后不可用 | `LutPanel disabled={!onDefaultLayer()}` | 去掉这条门禁：LUT 恒可用，编辑的永远是**默认层那一段**；右侧真正只属于默认层的组（镜头/清晰度/局部色调/色彩管理/基础曲线）仍然灰掉 |
| 11 | 控制点八色过亮 | 原色是霓虹档，压在**白圈**上发飘 | 换成一档中明度饱和色（`tokens.css` `--point-0..7`），层里存槽位下标，不受影响 |
| 12 | 容差的概念要说清：一个绝对 100% 应用区 + 一个渐近应用区，两者都随 handle 缩放，且要能单独配置 | 旧模型是纯指数衰减（只有种子满强度） | 改成**两段式**：`core = tolerance × core_scale` 内 100%，`core..core+falloff` 用 smoothstep 降到 0；两个比例集中在 `develop/areas.rs::POINT_WEIGHT_CONFIG`。`PIPELINE_VERSION 18 → 19` |

## 2. 涉及文件

**后端（Rust）**
* `crates/raybend/src/develop/areas.rs`：`POINT_WEIGHT_CONFIG`（`core` / `falloff`）、`point_weight` 两段式、`weighted_distance` 抽出；测试重写。
* `crates/raybend/src/develop/pipeline.rs`：容差测试的取值跟随两段式（0.05/0.35 → 0.02/0.1）。
* `crates/raybend/src/store/develop.rs`：`Area.params` / `curves` 不再 `skip_serializing_if`；新增 `areas_roundtrip_through_the_catalog`。
* `crates/raybend/src/thumbnail/render.rs`：`PIPELINE_VERSION 18 → 19` + 4 处签名字面量。
* `src-tauri/src/editor.rs`：新增 `editor_overlay_state`（`OverlayLive`）+ 测试 `point_area_params_only_change_pixels_that_match_the_seed`。
* `src-tauri/src/lib.rs`：注册新命令。

**前端**
* `src/features/editor/area-markers.tsx`：整份重写（三环圆片 / 水平白线 + 勾边 / 线上名字 / 原生手势 / 实时跟随）。
* `src/features/editor/area-panel.tsx`：`RowDragSort` + `RemoveButton` + 幽灵行。
* `src/features/editor/lut-panel.tsx`：`RowDragSort`（删掉自己那份拖动状态机）+ 去掉 `disabled` 门禁的语义。
* `src/features/editor/left-panel.tsx`：定稿删除改 `RemoveButton`；LUT 面板不再因非默认层禁用。
* `src/features/editor/ViewportOverlay.tsx`、`src/features/editor/viewport.tsx`、`src/api/editor.ts`、`src/lib/overlay-live.ts`（新）：实时变换链路。
* `src/features/editor/store.ts`：`loadDevelop` / 参数 / 曲线按可选读；`layerRows` 复用行对象身份（否则改一层参数会把所有行 DOM 重建）。
* `src/components/ui/RowDragSort.tsx`（新）、`src/lib/row-drag-sort.ts`（原 `lut-chain-drag.ts`）、`src/lib/row-drag-sort.test.ts`。
* `src/styles/tokens.css`：`--point-0..7` 压暗一档。
* `src/features/editor/store.test.ts`：新增「缺字段载入」回归。

**文档 / 记忆**
* `memory/COMPONENTS.md`（新）：组件库方案（五步复用流程 / 分层 / 图标纪律 / 已合并的重复实现）。
* `AGENTS.md` §12：指向组件库方案 + 补 `RowDragSort`；记忆库索引补一行。
* `memory/DESIGN.md` §15.5：八色新值 + 「压在白圈上」的取色口径。
* `specs/editor-area.md` §4.2（两段式权重）、§4.3（LUT 恒可用）、§5.3（水平引线 / 线上名字）。
* `design/editor.md`：新增 9a 条（真机修正以它为准）。

## 3. 「核心问题」的结论（改控制点参数像全屏改）

代码链逐段核对过，**渲染侧的局部性本来就是对的**（掩膜读本层输入、权重只看向量距离），
本轮补的端到端测试（`point_area_params_only_change_pixels_that_match_the_seed`，走
`DevelopParamsDto` JSON → `into_parts` → `render_develop16`）把它钉住了。

真机上「像全屏改」最可能的实际原因是**同一条反馈里的两件事叠在一起**：

1. **层根本没读回来**（#7 的 TypeError 让整次 `loadDevelop` 作废）—— 于是「当前层」永远是默认层，
   拖参数改的就是默认层（= 全屏）；
2. 即便读回来了，**当前层焦点**在换图 / 撤销重读后回到默认层（规格 §5.6 的口径），
   此时面板高亮在默认行，用户容易以为还在控制点层。

修完 #7、再把「选中哪一层」与右栏层名条对齐之后，这一条需要崔总在新产物上复验；
若仍复现，用 `memory/DEBUGGING.md` 的 CDP 探针看 `editor_set_params` 的 `areas` 是否为空
（那是「载荷没带层」与「渲染没吃到层」的分界线）。

## 4. 验证（命令原文与结果）

```bash
cargo check --workspace --all-targets
# Finished（只有既有的未使用变量/未使用 mut 警告）

cargo test --workspace
# raybend (lib)            1624 passed / 0 failed / 11 ignored
# raybend (raw_worker)        4 passed
# raybend-crashpad            7 passed
# raybend-writer              2 passed
# raybend-desktop (lib)     194 passed / 2 ignored
# raybend-desktop (main)      2 passed
# 合计 1834 passed / 0 failed

pnpm typecheck        # 通过
pnpm test             # 1379 passed / 0 failed
pnpm lint:colors      # ✓ 无硬编码色值
pnpm lint:arch        # ✓ 分层依赖合规
pnpm lint:i18n        # ✓ 无硬编码中文文案
pnpm build            # ✓ 通过
```

### 4.1 Windows 测试产物

`pnpm debug:win` 在写这份记录时**没能跑完**：崔总的调试实例（PID 41176，2026/11/01 02:14 启动）还开着，
脚本按纪律拒绝杀掉活着的应用（也不该杀）。关掉那个窗口后重跑即可：

```bash
pnpm build && pnpm debug:win && pnpm check:win
```

## 5. 遗留 / 需要人类确认

* **GUI 与真机**：标记观感、拖动跟手、缩放跟随、四种删除图标的一致性、八色在深浅两套主题下的辨识度 ——
  这些只能由崔总在 `pnpm debug:win` 的新产物上确认（本记录只到单元测试）。
* **控制点跟着几何变化漂移**（handoff §3 P1-1）：本轮未动（语义要先拍板）。
* **容差默认值 15 的手感**：两段式之后「命中范围 ≈ 2 × 容差」，同一个 15 比以前宽不少；
  若觉得过宽，改 `POINT_WEIGHT_CONFIG` 的 `core` / `falloff`（记得抬 `PIPELINE_VERSION`），
  或改参数表里 `areaTolerance` 的默认值（两个数都只在一处）。
* **颜色管理路径（v2）下控制点的种子域**：工作域是 Rec.2020，取样走的是 Rec.709 线性解码，
  两者在色彩状态启用时不完全等价（与 handoff §3 P1-5 同源）。当前照片大多没有输入色彩状态，
  因此未处理；要处理得让取样与渲染共用一份「该层输入」的定义。
