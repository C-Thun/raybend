# 多层 LUT（应用层链）—— 数据、渲染、界面

2026-10-31 17:03:47 CST 完成。

## 背景与范围

崔总 2026-10-31 口述（原文在 `todos/2026-10-31-lut-multilayer-ui.md`）：把「选中即唯一应用」改成
**有序的应用层链**——列表点选换底层、右上角圆圈加/减层、每层独立开关与强度、可拖动排序；
issue / preset / XMP / 撤销全部要跟着适配。规格：`specs/editor-lut-chain.md`；
画稿：`design/editor.pen`（主稿 `P1JiB` + `Components / Editor / LUT 应用层` `pj0oo`）、`design/editor.md` §3.1.3。

前置（LUT 上 GPU）见 `implementations/2026-10-31_lut-on-gpu.md`。

## 关键决策

1. **数据形状**：`DevelopStack.lut_id: Option<String>` → `lut_chain: Vec<LutLayer>`，
   `lut_enabled` 保留为**总开关**。`LutLayer { id, enabled, strength }`，上限 8 层、同 id 不重复。
2. **反序列化只有一个入口**：`#[serde(from = "DevelopStackWire")]`，旧 `lut_id` 在这里并入链。
   DB、XMP `rb:profileJson`、issue 快照、preset 全部走同一个 `Deserialize`，不会漏。
3. **哈希兼容（关键）**：`profile_hash` 是对 `DevelopStack` 的 JSON 做 FNV —— 加字段就会让
   **所有旧定稿哈希失效**（导出校验会当场报「快照无效」）。所以序列化也收成一个入口
   `DevelopStackOut`：**单层 @100% 且开着**时按旧形状写 `lut_id`、不写 `lut_chain`；
   空链写 `lut_id:null`。于是旧数据（含无 LUT 的绝大多数）**逐字节不变**，
   `signature()` 同样对这两种旧形状保持原字符串。多层 / 改强度才换新形状。
4. **CPU 与 GPU 同语义**：`DevelopStages.lut: Option<&LutChain>`；
   CPU 逐层 `apply_rgb`（显示域）/ `eval_extended` + `mix`（工作域）；
   `strength == 1` 时逐位等于改动前的单支。
5. **GPU 用一张 3D 图集**，不是 N 张纹理：WebGPU 默认 `max_sampled_textures_per_shader_stage = 16`，
   现有链路已占 5 张，8 层 ×（立方表 + shaper）会直接超限。图集每层占一段 Z，
   X/Y/Z 各留 1 texel 边界复制，按层自己的 `n` 插值也不会串味；shaper 用一张 `texture_2d_array`。
   uniform 里每层 3 个 `vec4`（域端点 / 尺寸+强度 / 偏移+shaper 长度+开关）。
   只有强度/开关变化时**只改 uniform**；增删层、换 LUT、改顺序才重建图集。
6. **LUT 仍然挂在 group 0**（`max_bind_groups` 默认 4），主图与参照帧各用自己的绑定组：
   参照帧固定用「关闭」那一套资源，结构上不可能被套上链。
7. **缺文件降级**：`lut::resolve_chain` 跳过不可用的层并记进 `missing` ——
   缩略图 / 编辑器继续出图，**导出遇到 missing 直接报错**（静默少一层就是另一张图）。
8. **界面两条通道**（画稿 §3.1.3）：点 tile = 换/移到第 1 层（主色内描边）；
   点右上角圆圈 = 加/减层（在链里常显实心勾，不在则指向出现空圈）。
   链行 = 缩略图 56×42 + 名称/单层开关 + 强度拉杆/数值/删除（删除位常占、指向才画图标）。
9. **一次手势 = 一次 confirm**：链的所有变更都经 `store.commitChain`（截断上限、总开关随链走），
   工作区在 `onChainChange` 里调 `confirmEdit()`；撤销的 Setting 从 `LutId` 改成 `LutChain`（JSON）。

## 涉及文件

| 文件 | 改动 |
| --- | --- |
| `crates/raybend/src/store/develop.rs` | `LutLayer` / `MAX_LUT_LAYERS` / `lut_chain`；`DevelopStackWire`（读）+ `DevelopStackOut`（写，旧形状兼容）；`validate`/`is_empty`/`len`/`signature`；`Setting::LutChain`；`load`/`save`/`prune_empty_stack` |
| `crates/raybend/src/store/migrations/catalog_0022_lut_chain.sql` | 新列 `lut_chain TEXT`（不做 SQL 回填，读时归一化） |
| `crates/raybend/src/store/migration.rs` | 注册 v22 |
| `crates/raybend/src/develop/lut.rs` | `LutChain` / `LutChainLayer`；`apply_rgb`（逐层）、`eval_extended`（逐层 + 强度混合） |
| `crates/raybend/src/develop/pipeline.rs`、`working.rs` | `DevelopStages.lut` 改成链 |
| `crates/raybend/src/render/creative_lut.rs` | 3D 图集 + shaper 数组 + 每层 uniform；`Arc<LutChain>` 指针相等即跳过重建 |
| `crates/raybend/src/render/spike.wgsl` | 逐层循环（编码域、超域残差、strength）；shaper 走 `texture_2d_array` |
| `crates/raybend/src/render/{gpu.rs,working_preview.rs}` | 绑定组带链、`gpu_lut` 类型、快速路判据 |
| `crates/raybend/src/thumbnail/*`、`export*`、`external_editor`、`display/output.rs` | 参数类型改成链 |
| `src-tauri/src/lut.rs` | `LutCache`（按 id）+ `resolve_chain` / `resolve_stack_chain`（含 `missing`） |
| `src-tauri/src/{editor,develop,thumbs,issues,export,sidecar}.rs` | 全链路改用 `resolve_chain`；DTO 换成 `lutChain` |
| `src/api/{editor,types}.ts`、`src/features/editor/{store,lut-panel,left-panel}.tsx`、`src/lib/presets.ts`、`src/workspaces/editor/EditorWorkspace.tsx`、i18n | 前端链状态机、应用层列表、tile 圈选、preset 的链快照 |
| `design/editor.pen`、`design/editor.md` | 应用层画稿 + §3.1.3 规格 |
| `specs/editor-lut-chain.md`、`specs/xmp-sidecar.md`、`specs/issue-xmp-contract.md`、`memory/FUNCTION-IMAGING.md` | 规格与映射说明 |

## 验证

```
cargo test -p raybend
  → 1602 passed / 0 failed / 11 ignored

cargo check -p raybend-desktop            → 通过
cargo test --workspace
  → core 1602 绿；desktop 187 passed / 1 failed
    ✗ window_state::tests::corners_allow_exact_edges_and_reject_overflow
      —— 失败在 **另一个会话的在途改动**（`src-tauri/src/window_state.rs` 是未跟踪的新文件，
      `window_layout.rs` 同一批），与本次 LUT 改动无关；本次没有动这两个文件。

cargo run -p raybend --example lut-offscreen
  关 LUT：max=1 / cube LUT：max=1 / shaper+cube：max=1~3 / **三层链：max=1**
  自检（开 vs 关）：max=38，4900+ 通道超差 → 确实在着色器里逐层套了
  → 全部通过（容差 3/255）

pnpm typecheck            → 通过
pnpm test                 → 1348 passed（新增 2 条链状态机用例）
pnpm lint:colors / lint:arch / lint:i18n → 三条全绿
```

新增单测：

- `store::develop::legacy_single_lut_is_normalized_into_the_chain`（旧 JSON → 一层；已有链不被覆盖）
- `store::develop::single_layer_at_full_strength_keeps_the_legacy_json_and_signature`（哈希字节兼容）
- `store::develop::chain_validation_rejects_empty_duplicate_and_too_many_layers`
- `store::develop::signature_tracks_chain_order_and_strength`
- `store::develop::chain_round_trips_through_the_catalog`
- `store::issues::profile_hash_and_signature_stay_byte_identical_with_display_sizes_in_the_catalog`（原有，仍绿）
- 前端 `应用层链：点选换底层、圈选加减层、拖动排序、强度与上限`、`应用层链最多 8 层`

**未经人类验证**：真实照片上的多层观感、拖动排序的手感、Windows 真机帧率；
`pnpm smoke:ui` 与真机 E2E 没跑（需要 `pnpm dev` 与 Windows 产物）。

## 遗留

1. **设计稿派生帧未同步**：`States / Editor / 裁切|旋转|对比` 里的左列是旧版（没有「应用 LUT」开关行），
   应用层没有可挂的上下文，已在 `design/editor.md` §1 标成待同步项。
2. **改强度会重建图集**：链里的 `LutChain` 带强度，改强度 = 新 `Arc` → 重建（33³×3 层约十几毫秒，
   65³ 几十毫秒）。松手才 confirm 所以在拖动中只会在末端重建一次；若将来觉得卡，
   可以把强度从图集键里拆出来（只改 uniform）。
3. **多层的导出成本**：CPU 逐层全画幅，3 层 20MP 大约多两三百毫秒，可接受（导出本来不是交互路径）。
4. 历史 issue 若引用了已删除的 LUT：`missing` 会跳过该层 —— 编辑器/缩略图可见降级，
   导出报错；要不要在定稿列表上给更强的提示，等崔总看真机再定。
