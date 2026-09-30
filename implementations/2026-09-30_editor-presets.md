# editor 预设面板 + 右栏第 1 组等高 — 实施记录

完成时间：2026-09-30 02:29:07 CST

## 改动范围

崔总 2026-09-30 需求：editor 右栏第 3 组新增「预设」页签（面板 / 存储 / 应用），
附带右栏第 1 组（总览 / 定稿 / 信息）三页签默认等高。设计稿当日定案
（`design/editor.pen` `e4WYe` / `nxMA2` / `WCT5O`，`design/editor.md` §3.10 / §3.11），
规格 `specs/editor-presets.md`。原登记 `memory/PLAN.md` §4 #4 与 `memory/FUTURE.md`
锁定块同日移除（崔总授权「future 里有就删掉」）。

## 涉及文件

**新建**

- `crates/raybend/src/store/migrations/app_0008_presets.sql` — 两张表（`preset_dirs` / `presets`，同目录名唯一 NOCASE）
- `crates/raybend/src/store/presets.rs` — 存储层（CRUD + default 保护 + 移动自动后缀 + 坏 payload 跳行），11 条单测
- `src-tauri/src/presets.rs` — 六个命令（library / create_directory / delete_directory / create / delete / move），统一返回最新整库
- `src/lib/presets.ts` — 类型 + 纯函数（清洗 / 快照构建 / 应用计划 / 落点 / 重名 / id 生成）
- `src/lib/presets.test.ts` — 18 条单测
- `src/api/presets.ts` — IPC 封装（照 `api/lut.ts`）
- `src/features/editor/preset-panel.tsx` — 面板组件

**修改**

- `crates/raybend/src/store/migration.rs` — APP_MIGRATIONS v8（presets）
- `crates/raybend/src/store/mod.rs` — 挂 `presets` 模块
- `src-tauri/src/lib.rs` — 模块声明 + 六命令注册
- `src/features/editor/store.ts` — 预设状态（整库 / 选中 / 展开）+ `presetSnapshot`（构建）+ `applyPresetSnapshot`（应用，batch 单次 bump）
- `src/features/editor/panels.tsx` — 第 3 组双页签（曲线 | 预设）+ 第 1 组等高 + 5 个 props 透传
- `src/workspaces/editor/EditorWorkspace.tsx` — 预设库 onMount 拉取 + 五个持久化回调接线；顺手给既有 `issueThumbs` 的 `JSON.parse` 包了 try/catch（pi-lens 拦截：裸 parse 异常路径会崩队列）
- `src/i18n/zh-CN.ts` / `en-US.ts` — `editor.preset.*` 34 键（中英同步）

## 关键决策与理由

1. **payload = 大类完整快照**（勾选的大类写全字段，含等于默认值的项）——「覆盖」语义要求应用时能把该大类**恢复到预设保存时的状态**；只存 dirty 项会让「当前改了、预设没改」的参数残留。应用走 `planPresetApply`（纯函数）：数值逐项 setParam、曲线四通道写满（缺 = 恒等）、LUT 丢失（库里找不到或不可用）**静默忽略**、`lut.id = null` 表示清除当前 LUT。
2. **大类 → 参数 id 从 `PARAMS` 派生**（`PRESET_GROUP_PARAMS`），不写第二份；参数表加项预设自动跟上。detail/lens 的附加字段（`nrMethod` / `profile` / `enabled`）按契约写死名字。TS 侧 `PresetDetailGroup` / `PresetLensGroup` 用放宽索引签名保持**扁平 JSON** 契约（`Record<string,number> & {…}` 交叉类型无法赋值，实测）。
3. **存储设备级 `app.db`**（照 LUT 分类口径——预设是应用级资产，换库可用）；迁移走既有框架（v8）。
4. **移动重名自动加 ` 2` 后缀**（后端 `move_to`，循环找第一个可用名，上限 1000）——拖拽流程不打断。
5. **拖拽复用 `lib/pointer-drag.ts`**：拖起后目录全折叠是**显示层覆盖**（`dragging()` 时 `expanded()` 恒 false），展开记录从未被改 → 释放自动还原；目标目录由 movePresets 回调显式展开。边缘自动滚动用 rAF 循环（指针停住也持续滚）。
6. **命令注册表不接入**（§2.15）：应用 / 新建 / 删除 / 拖拽均为面板内交互，依赖面板选中态与弹窗 / 指针语境，命令面板无从表达；与 LUT 面板口径一致（其「新建分类 / 导入」也未登记）。默认热键：无（未登记命令即无热键，理由同上）。
7. **等高口径**（崔总定案「含缩放行」）：`calc((var(--panel-w-right) - var(--panel-pad) - var(--panel-pad-scroll)) / 1.5 + 253px)`，253 = gap 8 + 缩放行 24 + gap 8 + 直方图块 213（标题 ~19 + mb 6 + py 12 + h-44 176）。定稿 / 信息**固定该高度**（`height`）+ 内部滚动；总览保持自然高度。⚠️ 直方图块高度是组件现实推导，`HistogramPanel` 改版时该常数要跟着核（`panels.tsx` 有注释）。
8. **预设树最小高 224px**：对齐「曲线页签在 RAW + 基础曲线块」时的内容高度（~250）——SOOC 下曲线较短会有留白，可接受（完美等高需两边都定死，收益不成比例）。
9. 面板选中 / 展开态放 store（`Show` 切页签会卸载内容组件，本地状态会丢）；不持久化（会话内状态）。

## 验证

- `cargo test --workspace`：1238 + 102 + 6 + 2 + 2 全过（含新增 `store/presets` 11 条）
- `pnpm typecheck && pnpm test`：1172 过（含新增 `lib/presets` 18 条）
- `pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n`：全过（i18n 中英 34 键同步由 parity 测试守门）
- `pnpm build`：过（chunk >500kB 警告为既有状态）
- `cargo check --workspace`：0 error

## 已验证（冒烟）与未经人类验证

冒烟：编译 / 单测 / lint / 构建 / 命令注册。**未经崔总真机验证**：面板交互手感
（拖拽含边缘滚动 / 多选 / 悬浮删除）、应用观感、LUT 丢失路径、等高与页签切换的
实际观感、Windows 下整体表现。

## 遗留问题

- 预设重命名未做（YAGNI，删除重建即可；规格 §1「不做」已记）。
- 拖拽的 Escape 取消未接（trackPointerDrag 无外部 cancel 口；越窗 / pointercancel 已覆盖）。
- `editor.preset.title` 用于第 3 组页签名；若未来命令化再补 catalog 条目。
- E2E 由崔总验收后，`todos/2026-09-30-editor-presets.md` 按约定删除或移走。
