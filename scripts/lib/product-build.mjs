/**
 * **产品构建（Windows debug）的唯一一份定义** —— cargo 参数、清单重定向、环境变量。
 *
 * 谁在用：
 *
 *   * `scripts/debug-win.mjs` 第 ② 步 —— 真正编出 `raybend-desktop.exe`；
 *   * `scripts/clean-target.mjs` 的 Windows 清单 —— 瘦身脚本要一份「本次构建图里有谁」的
 *     `--message-format=json` 流，而它只能靠**真编一遍**拿到（cargo 没有 dry-run 版 JSON）。
 *
 * 为什么必须共用（2026-10-25 的事故就是这个）：
 *
 *   1. **包清单**：`raybend-raw-worker` 是 `raybend` 包的 bin 目标，`raybend-crashpad` 是独立
 *      助手 exe —— 少选一个包，那些单元就不在清单里，宽限期一过就被当垃圾删（下次白重编）。
 *      反过来，漏了 `-p raybend` 就是「主程序新、worker 旧」的协议不匹配（2026-09-24）。
 *   2. **AI feature**：`photo-ai-runtime` 缺了，清缓存这一下就把刚建好的 AI 版换成无 AI 版
 *      （`RAYBEND_PHOTO_AI` 与 `TAURI_CONFIG` 又是 build script 的输入，两者必须成对给）。
 *   3. **target 目录**：清单里的产物落在哪个 `debug/` 下，瘦身脚本就只认那个目录 ——
 *      目录与清单不同源时「一个产物都认领不到」（2026-10-25：`debug:win` 第 ④ 步天天失败）。
 *
 * 一句话：这两处只要各写一份，早晚会漂；所以只在这里写。
 */
import { aiBuildEnv } from "./ai-build.mjs";
import { windowsBuildEnv } from "./dav1d-win.mjs";

/**
 * 产品构建包含的包。
 *
 * `raybend-desktop` = Tauri 薄壳（进程入口）；`raybend` = 核心库 + `raybend-raw-worker` bin
 * （解码在独立进程，AGENTS.md §6.3）；`raybend-crashpad` = 崩溃助手（同目录第二个 exe，
 * 不靠主程序 re-exec 自己，`specs/logging-and-crash-report.md` §5.2）。
 */
export const PRODUCT_PACKAGES = ["raybend-desktop", "raybend", "raybend-crashpad"];

/**
 * `cargo build` 的参数数组。
 *
 * @param {object} options
 * @param {{features: string[]}} options.aiPlan `resolveAiBuild()` 的结果（feature 集合由它决定）
 * @param {boolean} [options.json] 追加 `--message-format=json`（构建图按行打到 stdout）
 */
export function productBuildArgs({ aiPlan, json = false }) {
  return [
    "build",
    ...PRODUCT_PACKAGES.flatMap((name) => ["-p", name]),
    "--features",
    aiPlan.features.join(","),
    ...(json ? ["--message-format=json"] : []),
  ];
}

/**
 * 拼成一条 `cmd.exe` 能跑的命令行（Windows 侧就地执行）。给了 `jsonPath` 就把构建图重定向过去。
 *
 * 进度信息走 stderr，所以终端上照旧能看到 `Compiling …`。
 */
export function productBuildCommand({ aiPlan, jsonPath }) {
  const command = `cargo ${productBuildArgs({ aiPlan, json: jsonPath !== undefined }).join(" ")}`;
  return jsonPath === undefined ? command : `${command} > ${jsonPath}`;
}

/**
 * 构建环境（经 `WSLENV` 透传给 cmd.exe）。
 *
 * `CARGO_TARGET_DIR` 一定给 Windows 本地盘路径（9p 共享不支持 rustc 的锁语义，AGENTS.md §5.3）；
 * AI 相关的两个变量从同一个 `aiPlan` 取 —— **feature 与环境同源**才不会在同一个 target 里
 * 编出两套产物。
 */
export function productBuildEnv({ aiPlan, cargoTargetDir }) {
  const ai = aiBuildEnv(aiPlan);
  return windowsBuildEnv({
    CARGO_TARGET_DIR: cargoTargetDir,
    TAURI_CONFIG: ai.TAURI_CONFIG,
    RAYBEND_PHOTO_AI: ai.RAYBEND_PHOTO_AI,
  });
}
