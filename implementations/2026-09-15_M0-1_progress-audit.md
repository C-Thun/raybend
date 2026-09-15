# M0 进度逐项核实 + Pencil 连接确认

完成时间：2026-09-15 10:30:30 CST

## 本次改动范围

用户指出 plannotator 进度中「已做完但未勾选」的项，要求逐项核实。核实过程中发现并修复了一个真实的依赖问题，另外完成了 Pencil MCP 连接确认与辅色改色。

## 涉及文件

| 路径 | 改动 |
| --- | --- |
| `package.json` / `pnpm-lock.yaml` | **移除未使用的 `@tauri-apps/api`**（见下「发现」） |
| `DESIGN.md` | 品牌辅色 `#775588`（紫）→ `#EECC55`（金黄），同步色板、令牌映射与变更记录 |
| `design/main.pen` | 插入连接测试标记 `ZZ_PENCIL_CONNECTION_TEST`（节点 ID `D9ZRU`，**待用户确认后删除**） |

## 核实结论（逐项）

### 已做完但此前漏勾的（3 条，已补勾）

| 步骤 | 项 | 核实方式 |
| --- | --- | --- |
| 67 | WSL 与 Windows 两侧能起窗口 + check/clippy 全绿 | WSL `pnpm tauri dev`（用户目视确认）；Windows 侧新验证：`MainWindowHandle=10815460`（非零＝窗口真实存在）、`MainWindowTitle=RayBend`；`cargo clippy --workspace --all-targets` 在 Windows 上 `Finished in 1m41s`、0 warning；无残留进程 |
| 69 | 许可 / 中文名 / 工具链纪律 / 去个人化已落实到文档 | `LICENSE`=AGPL-3.0、`package.json`=`AGPL-3.0-only`、根 `Cargo.toml`=`AGPL-3.0-only`；`光伴` 在 README+AGENTS；`工具链只有 pnpm + cargo` 在 AGENTS §2；「个人」的仅存命中都在 `plans/M0.md` 与 `implementations/`（是**描述该需求**的文字，非项目文档中的个人化表述） |
| 70 | 无模板残留 / 死代码 / 未使用依赖 | 模板产物搜索（greet、plugin-opener、App.css、logo.svg、vite.svg、tauri.svg）零命中；`src/App.css`/`src/assets`/`public` 确认已删；**发现并修复未使用依赖**（见下） |

### 发现并修复的真实问题

- **`@tauri-apps/api` 是未使用依赖**：删掉模板的 `greet` 命令后，`src/` 下对它 0 处引用。
  已从 `dependencies` 移除。移除后 `pnpm typecheck` 与 `pnpm build` 仍通过，且 **JS 产物体积完全不变（28.62 kB）**——反证它此前已被 tree-shake 掉、确实没进产物。
  M1 出现第一个 IPC 契约时按需加回（一行命令）。

### 未勾选项的真实状态（50 条）

| 分组 | 条数 | 状态 |
| --- | --- | --- |
| M0-2 渲染 spike | 11 | 未开始（等 Windows 侧配合验证） |
| M0-3 RAW 解码 spike | 7 | 未开始（缺 RAW 样本） |
| M0-4 存储基准 | 9 | 未开始 |
| M0-5 缩略图吞吐 | 6 | 未开始（依赖 M0-3 样本） |
| M0-6 前端骨架 | 8 | 未开始 |
| M0-7 架构定稿 | 6 | 未开始 |
| **步骤 61（更新 AGENTS.md）** | 1 | **部分完成（8 个子项完成 6 个）**：许可✅／中文名✅／工具链纪律✅／Rust 1.98.1 与工具链路径✅／Windows 构建运行命令✅／`plans/` 入 §10✅／UI 选型回填 §7.6·§7.7✅；**缺** wgpu 版本（M0-2 才定）与架构章节（需 spike 结论）→ 因此**不勾** |
| DoD 66 / 68 | 2 | 未达成（六个 spike 有记录 / 四个架构未知有结论——均依赖 M0-2～M0-6） |

**结论**：不存在其他「已做完但漏勾」的项。剩余 50 条要么尚未开工，要么被 spike 结论阻塞。

## Pencil MCP 连接确认

- `pencil_get_app_state` 可读到用户打开的画布：`design/main.pen`，含顶层 frame `bi8Au`（800×600，`layout: none`，白底，空）
- 已插入可见标记（`#EECC55` 底 + 中文说明文字），节点 `ZZ_PENCIL_CONNECTION_TEST` = `D9ZRU`，子文本节点 `M82h1s`
- `TakeScreenshot` 返回的截图确认渲染正确（未塌陷/未裁切）
- **待办**：用户确认后删除该标记（`Delete("D9ZRU")`）

## 验证方式（冒烟）

| 检查 | 结果 |
| --- | --- |
| `pnpm remove @tauri-apps/api` 后 `pnpm typecheck` | ✅ 通过 |
| 同上 `pnpm build` | ✅ 通过，28.62 kB（体积不变） |
| Windows `cargo clippy --workspace --all-targets` | ✅ `Finished in 1m41s`，0 warning |
| Windows exe 运行 → PowerShell 查窗口 | ✅ `MainWindowHandle=10815460`、`MainWindowTitle=RayBend` |
| 进程清理 | ✅ 无残留 |
| Pencil `TakeScreenshot` | ✅ 标记渲染正常 |

## 遗留问题

1. `design/main.pen` 中的测试标记待删除（等用户确认）。
2. 剩余 50 步按用户指示暂停；M0-2 的前置（Windows 构建路径）已就绪，待用户配合 GPU 验证（`ASSISTANCE.md` A2）。
