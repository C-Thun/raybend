完成时间：2026-10-10 18:09:04 +0800

# 编辑会话管道 W4：白平衡（解码真值基线 + 落库一致性 + 预设偏移量 v3）

## 依据

`specs/editor-session-pipeline.md` §6（W4），含崔总 2026-10-10 两条定案：
「白平衡在预设里存偏移量」与「旧预设按跟随拍摄色温处理」。

## 一、渲染侧：基线用解码真值（§6.2）

| 文件 | 改动 |
| --- | --- |
| `src-tauri/src/editor.rs` | `run_develop_job` 在跑管线前用 `entry.as_shot_temperature` 覆盖 `params.as_shot_temperature()`（legacy） |
| `src-tauri/src/editor_working.rs` | `run` 里 `shot = source.shot` 后覆盖同名字段（V2），参考帧 / 直方图也走同一份覆盖后的参数 |
| `crates/raybend/src/develop/params.rs` | 新增单测 `decoded_baseline_overrides_the_payload_placeholder` |

口径：payload 的 `asShotTemperature` 退化为「解码还没有值时的占位」；真值一到就以它为准 ——
切图瞬间前端把基线写 null 时首帧不再按静态默认 6250K 算（现象 3 的白平衡那一半），
编辑器视口与预览 / 缩略图 / 导出从此同一条基线。

## 二、落库一致性（§6.3）

- 前端 `EditorWorkspace.currentDevelopStack()`：`asShotK` 用 store 里最近一次已知真值；
  若 `asShot == null` 且 `values.temperature` 是显式值 → **不写温度项**（视为跟随 as-shot）
  并提示一次（`editor.wb.baselineUnknown`）。
- 后端 `develop_commit`：温度 dirty 且 `as_shot_k` 缺失 → 用**会话里的解码真值**回填
  （新 `editor::decoded_as_shot`：只在渲染线程当前装的正是这张照片的源文件时才认）；
  两边都拿不到 → `eprintln!` 记诊断，**不静默写 null 基线**。

## 三、预设 payload v3：白平衡存偏移量（§6.4 / §6.5）

- 新 `src/lib/white-balance.ts`：`mired` / `kelvinToWbMired` / `wbMiredToKelvin`（纯函数 + 单测）。
- `src/lib/presets.ts`：
  - `sanitizePresetSnapshot` 输出**恒为 v3**；读 v1/v2 时把 `color.temperature` 按
    「跟随拍摄色温」处理（删该项、`wbMired = 0`）；新增 `hasLegacyWbPresets`（迁移提示用）。
  - `buildPresetSnapshot` 的 `color` 写全字段但不写 `temperature`，改写 `wbMired`（0 = 跟随）。
  - `planPresetApply` 新增 `asShotK` 入参；`wbMired ≠ 0` 换算成目标 K，`= 0` 写回该照片
    as-shot（不算显式值），`asShotK === null` → 跳过白平衡项并报 `wbSkipped`。
- `src/features/editor/store.ts`：`applyPresetSnapshot` 传入 `asShot()`、按 `wbExplicit`
  决定 `temperatureExplicit`、返回 `{ wbSkipped }`。
- `src/workspaces/editor/EditorWorkspace.tsx`：预设库读入时检测旧版本 → 一次性提示
  `editor.preset.wbMigrated`；应用时 `wbSkipped` → 一次性提示 `editor.preset.wbSkipped`。
- i18n 两语言各加 3 个 key（`wbMigrated` / `wbSkipped` / `wb.baselineUnknown`）。
- 后端 `store/presets.rs`：受支持版本集合加 3（v1 / 带 colorManagement 的 v2 / v3）；
  测试同步（v4 拒绝、v3 通过、旧行读回**不被后端改写** —— 迁移只在前端做一份）。
- 契约文档 `specs/editor-presets.md` §3 更新为 v3 形状 + 规则 6（换算、保存、应用、迁移）。

### 关键决策

1. **Δmired 的符号**：按规格的公式本体实现
   （`Δ = mired(asShot) − mired(target)`，`target = 1e6/(mired(asShot) − Δ)`），
   即 **Δ 负 = 更暖**。规格原文旁注「正 = 更暖」与公式矛盾，已在 `specs/editor-presets.md` §3
   与代码注释里改正并写了示例。往返、clamp、asShot 未知、极端值都有单测。
2. **迁移只在前端做一份**：后端 `preset_library` 原样返回 DB 行（`DB 行不批量改写`，
   用户下次保存自然进 v3），语义迁移唯一实现在 `sanitizePresetSnapshot`。
3. **`wbMired = 0` 的应用语义**：写回该照片的 as-shot 且**不标显式**
   （否则「没动过白平衡」的预设会把基线变成一个显式值）。
4. **asShot 未知时跳过白平衡**：其余色彩项照常覆盖；提示而不是静默。

## 四、存量修复工具（§6.8，**待崔总裁决后执行**）

- `crates/raybend/src/store/develop.rs`：新 `stacks_missing_as_shot_baseline`（候选查询 + 单测，
  只选「显式温度 + `as_shot_k IS NULL`」）。
- `crates/raybend/examples/fix-as-shot-k.rs`：一次性 CLI。默认**预演（只读）**，
  打印每条候选的 asset、当前 `temperature`、将回填的解码真值与文件路径；
  加 `--apply` 才写库（走 `develop::save` 正常通道）。

```bash
cargo build -p raybend --bin raybend-raw-worker
cargo run -p raybend --release --example fix-as-shot-k -- "D:\photos\repo"          # 预演
cargo run -p raybend --release --example fix-as-shot-k -- "D:\photos\repo" --apply  # 人类拍板后
```

**未执行**：本案要崔总先看「修复前后同一画面」两张图再决定（会改变观感：
asset 3 那类会从偏暖回到中性）。工具与候选判定已就绪，留证与执行待人类。

## 验证

- `cargo test --workspace`：1475 + 7 + 123 + 2 + doctests 全绿（含新
  `decoded_baseline_overrides_the_payload_placeholder`、
  `missing_baseline_candidates_are_exactly_the_dirty_temperature_rows`、
  `legacy_payloads_read_back_untouched_for_the_frontend_to_migrate`、v3 契约）。
- `cargo clippy -p raybend -p raybend-desktop --all-targets`：无新增告警。
- `pnpm typecheck && pnpm test`（1259 通过；新增 `white-balance.test.ts` 7 条 +
  presets v3/迁移/应用 6 条）`&& pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n && pnpm build` 全绿。
- **未经人类验证**：真机首帧白平衡不再跳变、跨照片预设的冷暖倾向一致、
  旧预设的迁移提示、以及 xmp sidecar 无新增字段（本期不动 XMP，§1.2）。

## 遗留

- §6.8 的存量修复（含 asset 3）**未执行**，命令与候选查询已交付。
- 预设的 `wbMired` 目前没有可见的位移读数（只有滑块位置与提示）；若崔总想看到
  「+X mired」字样，属界面新增，另开设计稿。
- 前端在 asShot 未知时丢弃一次显式温度（按 §6.3 的口径），提示只出现一次；
  若用户随后拿到真值重调，需要重新拖动白平衡（预期行为，记录备查）。
