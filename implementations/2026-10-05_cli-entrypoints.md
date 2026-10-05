# 补 4 个 CLI 入口 + README/FUTURE 同步

**完成时间：2026-10-05 18:44:52**

---

## 1. 需求（崔总 2026-10-05 追加）

1. `.ori` / `.fff` 那件事「记一下 FUTURE，**级别不高**」；
2. 上一轮遗留的四个「有脚本、没 pnpm 入口」的探针**补入口并补 README**。

## 2. 改动

| 文件 | 改动 |
| --- | --- |
| `package.json` | 新增 4 个脚本入口：`check:color-status`、`check:color-win`、`check:lens-ipc-win`、`perf:grid`（`grid-bench.mjs` 归到 `perf:*` 一族，与 `perf:browse` / `perf:win` 并列；都沿用既有「`node --check` 先语法自检、再运行」的写法） |
| `README.md` / `README.zh-CN.md` | 冒烟表加 `check:color-status`、性能表加 `perf:grid`、Windows 诊断表加 `check:color-win` / `check:lens-ipc-win`；删掉上一轮那句「少数探针还没接进 pnpm」 |
| `memory/FUTURE.md` | B-6 标注「**优先级低**」并记下崔总 2026-10-05 的口径（等真样本，不急） |

## 3. 验证

* `node --check` 三个探针脚本全部通过；`pnpm perf:grid 20000` 实跑通过（输出：分组 60.48 ms / 行模型 8.08 ms / 窗口 49.47 µs 每次）。
* README 自检脚本复跑：中英各 **37 个 `pnpm` 命令全部存在**、13 个相对链接全部有效、RAW 扩展名 24/24 与 `kind.rs` 一致、两版均 165 行同构。
* `check:color-status` 需要 `pnpm dev`、`check:color-win` / `check:lens-ipc-win` 需要 Windows 目标 —— 按文档口径只做语法与入口自检，实跑留待对应环境。

## 4. 遗留

* `.ori` / `.fff`：仍缺真样本（FUTURE B-6，**低优先级**）。拿到样本后：确认魔数 → 进 `media::kind` 扩展名表 → 跑 `thumb-probe`/`raw-smoke` 冒烟 → 补单测。
* `grid-bench.mjs` 依赖 `--experimental-strip-types`（直接 import `.ts`）。Node 该标志将来转正时可撤掉，属抄底清理，不单独排期。

## 5. 纪律自查

* **XMP 侧影响（`AGENTS.md` §2.19）**：无（脚本入口与文档）。
* **命令体系接入（§2.15）**：不适用 —— 新增的是**开发/诊断命令行入口**，不是应用内用户可触发功能，不进命令面板注册表。
