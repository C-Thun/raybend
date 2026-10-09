完成时间：2026-10-09 00:32:10 +0800

# 工作区布局：默认留白比例调优（全部减半）

## 需求（崔总 2026-10-09）

> 可以，调优一下，左边和右边的剩余空白宽度，再砍一半，上下也各砍一半

即默认留白（相对**工作区**）由 左/右/上 5%、下 10% 改为 **左/右/上 2.5%、下 5%**；
「下 = 上 × 2」的比例关系保持不变。

## 改动

| 文件 | 改动 |
| --- | --- |
| `src-tauri/src/window_layout.rs` | `WorkAreaInsets::DEFAULT` → `0.025 / 0.025 / 0.025 / 0.05`；三个依赖默认值的单测期望值同步（1080p、负坐标多屏、奇数尺寸取整），默认比例单测改名为 `..._two_and_a_half_and_five_on_1080p` |
| `specs/window-work-area-layout.md` | 决策行比例改为 2.5% / 2.5% / 2.5% / 5% + 注明「2026-10-09 崔总调优」；§1「下留 5%」 |
| `design/main.md` §2.1 | 第 7 条比例更新（画布仍无变化） |
| `AGENTS.md` §11.5 | 术语表 `fitWorkArea` 条目的比例更新 |

命令接口、几何算法、前端判定与命令注册**均不变** —— 比例本来就是 Rust 常量 + 可选 `insets` 参数，
这次只动常量值与其文档描述（这正是当时「接口按参数设计」的用途）。

## 验证（Agent 冒烟，2026-10-09 00:3x 实跑）

- `cargo test -p raybend-desktop --lib --offline window_layout`：**6 passed / 0 failed**。
  其中 `default_insets_leave_two_and_a_half_and_five_on_1080p` 固定 1920×1080 工作区 →
  内容区 `(48, 27, 1824, 999)`（左侧 48px = 2.5%、上 27px、右 48px、下 54px）。
- 本轮无前端代码改动：上次全量结果（`pnpm typecheck` ✓、`pnpm test` 1228/1228、
  `lint:colors`/`arch`/`i18n` ✓、`pnpm build` ✓、`cargo check --workspace` ✓）仍适用。

## 待真机

- 崔总在 Windows 上确认 2.5% / 5% 的实际观感是否合适（本次口径来自崔总目视反馈）；
  若需微调，改 `WorkAreaInsets::DEFAULT` 一处即可，或将来从设置界面传 `insets` 参数（接口已留）。
