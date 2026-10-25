2026-10-26 02:57:57

# `pnpm debug:win` 第 ④ 步次次失败：清单与 target 目录不同源

## 症状

`pnpm debug:win` 前端构建、Windows cargo 构建、`check:win` 全部通过，最后一步

```text
✗ 单元清单里一个产物都没有：C:\rb-target\raybend\last-build.json（构建是不是根本没跑？）
✗ ④ target 瘦身（删掉失效的旧产物） 失败（退出码 1）
[ELIFECYCLE] Command failed with exit code 1.
```

——**构建其实是好的**，只是整条命令以 1 退出，读起来像「构建失败」。清单文件明明存在（554 KB、631 条
`compiler-artifact`、1346 个产物路径全部落在 `C:\rb-target\raybend\debug\` 下）。

## 根因

两个 Windows 构建目录被当成一个：

| 用途 | 目录 | 写它的脚本 |
| --- | --- | --- |
| 日常 debug | `C:\rb-target\raybend` | `scripts/debug-win.mjs`（写死） |
| 发版 | `%LOCALAPPDATA%\raybend\build\target` | `pnpm release`（`discoverWindowsReleasePaths`） |

`c56f7b0`（2026-10-15）把 `clean-target.mjs` 的默认目录从 `C:\rb-target\raybend` 改成了
「发版那套探测」，注释里写的理由是「debug 也走同一套探测」—— 但 `debug:win` 从来没搬过去。
于是 `debug:win` 第 ④ 步只传 `--json`（debug 的清单）、不传 `--target-dir`，瘦身脚本用**发版**目录
去比**debug** 的产物路径，`startsWith(DEBUG)` 一个都命中不了 → `live.files.size === 0` → 直接 fail。

顺带查出同族的第二个问题（未触发、但踩上就会静默降级）：`clean-target` 自己重建清单时把 feature
写死成 `custom-protocol`，而 `debug:win` 会带 `photo-ai-runtime`。谁单独跑一次 `pnpm clean:win`
（AGENTS.md §5.3 推荐的清理命令），就会在同一 target 里编出**无 AI 版** exe，把刚建好的 AI 版覆盖掉
（`RAYBEND_PHOTO_AI` / `TAURI_CONFIG` 也是 build script 的输入，与 feature 必须成对）。

## 改动

1. **`scripts/lib/product-build.mjs`（新增）** —— 产品构建（Windows debug）的**唯一一份**定义：
   `PRODUCT_PACKAGES`（`raybend-desktop` + `raybend`（含 RAW worker bin）+ `raybend-crashpad`）、
   `productBuildArgs()`、`productBuildCommand()`（可选把构建图重定向到 `last-build.json`）、
   `productBuildEnv()`（dav1d + `CARGO_TARGET_DIR` + AI 的两个变量）。`debug-win` 第 ② 步与
   `clean-target` 推导清单**共用**它 —— 包清单 / feature / 环境 / target 四处各写一份就是这次漂移的成因。
2. **`scripts/lib/windows-paths.mjs`** —— 导出 `DEBUG_WINDOWS_TARGET`（`C:\rb-target\raybend`），
   字面量只此一处。
3. **`scripts/debug-win.mjs`** —— 用共享常量与 `product-build`；第 ④ 步补 `--target-dir`（与第 ② 步
   的 `CARGO_TARGET_DIR` 同一个值）；产物 WSL 路径改为 `toHostPath()` 现算（不再写死 `/mnt/c/...`）。
4. **`scripts/clean-target.mjs`** —— 默认目标回到**日常 debug** 目录（AGENTS.md §5.3 的口径；
   发版目录仍可用 `RAYBEND_WIN_BUILD_DIR` / `--target-dir` 指定）；清单重建走 `product-build`
   并 honour `--ai`（默认 auto，与 `debug:win` 同义）；`--json` 与目录不同源时的报错改成说真话
   （原来那句「构建是不是根本没跑？」把人往错方向带）；另一个构建目录顺手打一行提示。
5. **`scripts/disk-report.mjs` / `docs/release.md`** —— 纠正「`C:\rb-target` 已弃用」「clean:win 与发版
   同一套探测」的旧说法；附录 J 两行改为「日常 debug 12 G / 发版 4.6 G」，并在 §8 记账本次教训。
6. **`scripts/lib/product-build.test.mjs`（新增）** —— 包清单、`--message-format=json` 重定向、AI 变量
   与 feature 成对、以及「两个调用方都必须走这份定义」。

## 关键决策

* **不把 debug 搬去 `%LOCALAPPDATA%`**：AGENTS.md §5.3、`docs/release.md`、`check:win`、`check-color-win`、
  `spike-win`、`perf-win` 全都按 `C:\rb-target\raybend` 走；搬目录是另一件事（涉及文档与多脚本），
  本次只让两边**记住自己走哪个目录**。要统一的话另开一条。
* **默认清理目标 = 日常 debug 目录**：它天天写、长得最快（12 G），发版目录只在发版时变（4.6 G）；
  而且清单重建本身是一次构建，指向发版目录会在那里平白多出一套 debug 产物。
* **清单重建必须与 `debug:win` 同一份参数与环境**：cargo 没有 dry-run 版 JSON，推导清单只能真编一遍，
  所以参数不同就会把产物换成另一套能力。

## 验证

* `node --check` 覆盖改动的 5 个脚本；`pnpm test:release` **108 passed**（含新增 4 条）。
* `node scripts/clean-target.mjs --target-dir 'C:\rb-target\raybend' --json 'C:\rb-target\raybend\last-build.json' --dry-run`
  修复前失败，修复后正常列出「617 文件 + 127 目录，约 6.7 GB」。
* `node scripts/clean-target.mjs --dry-run`（默认目标，自行重建清单）：第一次因 `dist/` 之外的原因重编一次
  `raybend-desktop`（1m38s），第二次 **3.17s / 无编译** —— 清单与产物稳定命中；AI 仍为启用
  （`resolveAiBuild` → `custom-protocol,photo-ai-runtime`）。
* **`pnpm debug:win` 全绿**（2026-10-25 18:52 实跑）：①②③ 通过，④ 清理 744 项、回收 6698.8 MB，
  末尾打印 `✓ Windows debug 构建完成`。`pnpm check:win` 复查：AI 启用、4 个前端资源命中、
  worker 独立进程且协议 `raybend-worker-proto-v7`。

## 遗留 / 注意

* 第 ② 步每次 `pnpm build` 之后重编 `raybend-desktop`（1–1.5 min）是**预期**的：`dist/` 是编译期嵌进
  exe 的，前端一变就要重编这个 crate。
* 其余脚本（`check-win-artifact` / `perf-win` / `check-color-win` / `spike-win` / `check-lens-ipc-win`）
  仍各自写着 `/mnt/c/rb-target/raybend` 字面量。它们只**读**产物、不推导构建图，暂不构成风险；
  若将来真要搬目录，再一起收敛到 `DEBUG_WINDOWS_TARGET`。
* 本次未跑 `clean:wsl`（WSL 侧 `target/` 与本次问题无关，重建清单要几分钟）。
* 未经人类验证：exe 的实际运行（本次只到 `check:win` 的产物核对与单元测试）。
