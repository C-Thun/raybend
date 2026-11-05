# 编辑区（area）—— 第三方代码评审与修复

> 完成时间：2026-10-31 23:39:52
> 范围：本轮 W2–W6 交付及其后续评审修复，不处理范围外的历史代码。

## 评审发现并已修复

| 问题 | 位置 | 修复 |
| --- | --- | --- |
| **种子取错层**：新建控制点的取样把「目标层自身及之后的层」也合成进去，种子被污染 | `crates/raybend/src/develop/pipeline.rs`、`src-tauri/src/develop.rs`、`src/workspaces/editor/EditorWorkspace.tsx` | `compose_linear_at` 改为按行号合成到目标层**之前**；`develop_area_seed` 增加 `layer` 参数；前端按待落点层所在行取样，并排除待落点层 |
| **落点坐标错域**：把窗口绝对 CSS 坐标直接送给洞口矩阵，窗口偏移会算进取样位置 | `src/features/editor/viewport.tsx` | 改为传「洞口左上角相对坐标」，与 `Viewport::css_overlay_transform` 的 `clip_rect` 口径一致；`overlay-math.ts` 注释钉死该契约 |
| **取样缺 LUT**：默认层在目标层之前时，LUT 没有进入「该层输入」 | `src-tauri/src/develop.rs`、`pipeline.rs` | 取样命令通过 `resolve_stack_chain` 解析 LUT；`compose_linear_at` 的默认层带上 LUT |
| **层间被夹取**：`DisplayLuts` 的表域只有 `0..1`，上一层推出的高光会在下一层之前被削掉 | `pipeline.rs` | 额外地层的表外值改走 `encode_and_curve_extended`（signed OETF + `Curve::eval_extended`）；默认层保留旧夹取语义，保住单层逐位等价 |
| **退化路径丢 LUT**：所有层都为空/关闭时提前走快路径，漏掉默认层的 LUT | `pipeline.rs::render_layered16` | 快路径返回前仍应用 LUT；有局部色调也走原 `render_rgb16_with_local_tone` |
| **层启停与渲染边界**：层关闭不能残留效果；坏数据的全局/掩膜参数不能进层 | `develop/areas.rs`、`pipeline.rs` | `AreaSpec`/`AreaLayer` 带上 `enabled`；关闭层权重为 0 但不从链删除；`LayerStack::from_specs` 拒绝 Global/Mask 参数 |
| **非有限值扩散**：NaN/Inf 的坐标、种子、容差、局部色调倍率可能把整帧变黑 | `develop/areas.rs`、`pipeline.rs` | 点坐标/种子/容差解析时归一化；`ratio_for_rgb` 非有限倍率回退为 1 |
| **标记拖动被 DOM 重建打断**：`<For>` 按对象引用重建，拖一下圆片就丢指针捕获 | `src/features/editor/area-markers.tsx` | 改 `<Index>`，位置、颜色、选中态、尺寸都走响应式访问器 |
| **取消拖动留下半截状态**：`Ctrl` 拖圆片出窗取消后，采样位停在半路且未落库 | `area-markers.tsx` | 取消时恢复起始采样位，不提交撤销节点 |
| **只读状态仍可编辑**：空态/二级锁下新建按钮、圆片、handle 仍可能响应 | `area-panel.tsx`、`area-markers.tsx` | 新建按钮与画布手势统一受 `enabled` 控制；标记只读时不拦视口平移 |
| **非默认层的基准曲线整块消失**：与「灰掉而非整块消失」的规格不符 | `src/features/editor/panels.tsx` | 基准曲线选择器保留显示并禁用（含「不使用基础曲线」与改名按钮） |
| **取消待落点未落库**：命令面板先落库待落点层，`Esc` 只在内存取消 | `area-panel.tsx` | `Esc` 取消后接 `onConfirm`，连库里的待落点层一起撤掉 |
| **载荷退化形状不一致**：空层时 `defaultIndex` 前后端一个发 `null` 一个发 `0` | `src/features/editor/store.ts` | 载荷恒发当前默认层位置；Rust 的退化序列化仍只在有额外层时写 `areas/default_index` |
| **空层载荷被 DTO 拒绝**：`areas: null` 会让普通编辑（还没建层）无法落库 | `src-tauri/src/develop.rs`、`src/api/editor.ts` | DTO 的 `areas` 改为 `Option<Vec<Area>>`，`null` 与缺省都归一化为空；IPC 载荷统一发 `[]`，并补 `dto_accepts_null_areas_as_default_layer_only` 回归测试 |

## 验证（命令原文与结果）

```bash
cargo check --workspace --all-targets
# Finished `dev` profile ...（通过；仅有既有未使用项告警）

cargo test --workspace
# raybend (lib)            1623 passed; 0 failed
# raybend-crashpad           7 passed; 0 failed
# raybend (raw_worker)       4 passed; 0 failed
# raybend-desktop (lib)    193 passed; 0 failed; 2 ignored
# raybend-desktop (main)     2 passed; 0 failed
# raybend-writer             2 passed; 0 failed
# 合计 1832 passed / 0 failed

pnpm typecheck
# tsc --noEmit 通过

pnpm test
# 1370 passed; 0 failed

pnpm lint:colors
# ✓ 无硬编码色值

pnpm lint:arch
# ✓ 分层依赖合规

pnpm lint:i18n
# ✓ 无硬编码中文文案

pnpm build
# ✓ built in 8.36s

pnpm debug:win
# ✓ Windows 产物与前端一致
# exe: /mnt/c/rb-target/raybend/debug/raybend-desktop.exe
```

新增/更新回归测试：`seed_sampling_only_uses_layers_before_the_target`、
`out_of_range_values_survive_the_display_domain_between_layers`、
`disabled_area_is_bypassed_without_being_removed`、层启停/颜色槽/预设等前端测试。

## 未修复 / 需要人类确认

* GUI 与真实照片上的手势、色彩与性能仍需崔总验收；本记录只证明单元测试与构建。
* 局部色调引导图在默认层移动后是近似（规格已登记）；工作域（v2 色彩路径）层链仍有 >1 高光夹取的已知取舍。
* 「该层输入已变 → 重新取样」入口未实现（数据与命令已就位，缺 UI），不是本轮新增回归。
* `debug:win` 已重新生成，包含上述修复；未做真机 E2E。
