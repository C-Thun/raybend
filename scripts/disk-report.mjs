#!/usr/bin/env node
/**
 * **磁盘占用报告**（只读）：回答「几十 G 去哪了、哪些该清、哪些是故意留的」。
 *
 * 为什么要它：构建缓存分布在四处（WSL `target/`、Windows 日常 debug 的
 * `C:\rb-target\raybend`、发版的 `…\raybend\build\target`、全局包仓库），**没有任何一个地方会告诉你总账** ——
 * 2026-10-14 崔总实测：WSL 58 G + 旧目录 16 G + 当前缓存 4.2 G + pnpm store 4 G ≈ 84 G。
 *
 * 本命令**只量不删**；每行末尾给出各自的清理入口。逐项「算完即打」，几十万文件的目录
 * 也能看到进展。WSL 侧用 `du`，**Windows 侧用 PowerShell**（WSL 的 9p 文件系统下
 * `du` 遍历深目录会报 `Cannot allocate memory`，量不准）。
 *
 * ```bash
 * pnpm clean:report
 * ```
 */
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { join, win32 } from "node:path";
import { fileURLToPath } from "node:url";
import { DEBUG_WINDOWS_TARGET, discoverWindowsReleasePaths, windowsPowerShell } from "./lib/windows-paths.mjs";

const ROOT = fileURLToPath(new URL("..", import.meta.url));

function human(bytes) {
  if (!Number.isFinite(bytes) || bytes <= 0) return null;
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes, unit = 0;
  while (value >= 1024 && unit < units.length - 1) { value /= 1024; unit += 1; }
  return `${value.toFixed(value >= 100 ? 0 : 1)}${units[unit]}`;
}

/** WSL 侧目录：`du -sh`（stderr 里的 9p 噪声丢弃，失败显示 `?`）。 */
function duHost(path) {
  if (!path || !existsSync(path)) return null;
  try {
    const out = String(execFileSync("du", ["-sh", path], { encoding: "utf8", maxBuffer: 4 * 1024 * 1024, stdio: ["ignore", "pipe", "ignore"] }));
    return out.split("\t")[0].trim();
  } catch {
    return "?（读取失败）";
  }
}

/** Windows 侧目录：PowerShell 递归求和（大目录较慢，但准）。 */
function duWindows(windowsPath) {
  if (!windowsPath) return null;
  try {
    const literal = `'${windowsPath.replaceAll("'", "''")}'`;
    const out = String(windowsPowerShell(`$sum=(Get-ChildItem -LiteralPath ${literal} -Recurse -File -Force -ErrorAction SilentlyContinue | Measure-Object -Property Length -Sum).Sum; if ($null -eq $sum) { 0 } else { $sum }`)).trim();
    const bytes = Number(out);
    return Number.isFinite(bytes) ? human(bytes) : "?（读取失败）";
  } catch {
    return "?（读取失败）";
  }
}

/** 应用运行时数据目录的标识符（`%LOCALAPPDATA%\<identifier>`）—— 从 `tauri.conf.json` 取，别写死。 */
const APP_IDENTIFIER = (() => {
  try { return JSON.parse(readFileSync(join(ROOT, "src-tauri/tauri.conf.json"), "utf8")).identifier; }
  catch { return null; }
})();
/** `%LOCALAPPDATA%`（问 Windows 要，拼 `C:\Users\<名字>` 会把人名写死）。 */
const LOCAL_APP_DATA = (() => {
  try { return String(windowsPowerShell("[Environment]::GetFolderPath('LocalApplicationData')")).trim(); }
  catch { return null; }
})();

/**
 * `C:\rb-target` 下**除日常 debug target 以外**的残留（旧发版 target、AI 冒烟副本、探针数据…）。
 *
 * 它们不在任何脚本的清理范围（`clean:win` 只打自己那条 target），所以历史上一直隐形：
 * 2026-10-26 翻账才发现 `raybend-release`（2026-09-27，4.4 G）与五份 AI 冒烟副本（0.9 G）
 * 从来没人清。报告只把它们摆出来 —— 删不删由人定，脚本不碰。
 */
function leftoverWindowsDirs() {
  const keep = `'${DEBUG_WINDOWS_TARGET.replaceAll("'", "''")}'`;
  try {
    const out = String(windowsPowerShell(`
      $root = Split-Path -Parent ${keep}
      $keep = ${keep}
      $items = @()
      foreach ($dir in @(Get-ChildItem -LiteralPath $root -Directory -Force -ErrorAction SilentlyContinue)) {
        if ($dir.FullName.TrimEnd('\\') -eq $keep) { continue }
        $sum = (Get-ChildItem -LiteralPath $dir.FullName -Recurse -File -Force -ErrorAction SilentlyContinue | Measure-Object -Property Length -Sum).Sum
        if ($null -eq $sum) { $sum = 0 }
        $items += @{ name = $dir.Name; path = $dir.FullName; bytes = [double]$sum }
      }
      ConvertTo-Json -InputObject @($items) -Compress
    `)).trim();
    const parsed = JSON.parse(out === "" ? "[]" : out);
    return (Array.isArray(parsed) ? parsed : [parsed]).filter((item) => item && typeof item.path === "string");
  } catch {
    return []; // 探测失败不是报告失败的理由
  }
}

const win = (() => {
  try { return discoverWindowsReleasePaths({ directory: process.env.RAYBEND_WIN_BUILD_DIR }); }
  catch { return null; }
})();
const storePath = (() => {
  try { return String(execFileSync("pnpm", ["store", "path"], { encoding: "utf8" })).trim(); }
  catch { return null; }
})();

const rows = [
  { label: "WSL cargo target", host: join(ROOT, "target"), note: "构建 / 测试缓存（只进不出，越攒越大）", action: "pnpm clean:wsl" },
  { label: "Windows debug target", win: DEBUG_WINDOWS_TARGET, note: "日常 `pnpm debug:win` 的缓存（天天写、长得最快）", action: "pnpm clean:win" },
  { label: "Windows 发版 target", win: win?.windowsTarget, note: "发版构建缓存（只在发版时用，刻意保留）", action: "保留；别用 clean:win 清它（会往那编一份 debug）" },
  ...leftoverWindowsDirs().map((item) => ({
    label: `C:\\rb-target\\${item.name}`,
    win: item.path,
    bytes: item.bytes,
    note: "残留：不在任何脚本的清理范围内（旧发版 target / AI 冒烟副本 / 探针数据）",
    action: "确认无用后手动删（脚本不碰）",
  })),
  { label: "发布产物 release-out/", host: join(ROOT, "release-out"), note: "保留：对应 GitHub Release 的字节副本", action: "旧 test-* 目录可手动删" },
  { label: "AI 输入快照 .release/", host: join(ROOT, ".release"), note: "保留：模型 + CPU 运行库的冻结副本，构建复用", action: "删除后下次构建重新校验 / 复制" },
  { label: "前端产物 dist/", host: join(ROOT, "dist"), note: "小，随时可重建", action: "pnpm build" },
  { label: "node_modules/", host: join(ROOT, "node_modules"), note: "依赖", action: "pnpm install 重建" },
  { label: "pnpm store（全局共享）", host: storePath, note: "所有 pnpm 项目的包仓库", action: "pnpm store prune" },
  { label: "cargo registry（全局）", host: join(process.env.HOME ?? "", ".cargo/registry"), note: "crate 下载缓存", action: "保留（删了要重新下载）" },
  { label: "应用运行时数据", win: LOCAL_APP_DATA && APP_IDENTIFIER ? win32.join(LOCAL_APP_DATA, APP_IDENTIFIER) : null, note: "日志 / 崩溃记录 / app.db / 数据库备份（不是构建缓存）", action: "保留；日志与 backups/ 可按需手清" },
];

const width = Math.max(...rows.map(row => row.label.length)) + 2;
console.log("RayBend 磁盘占用报告（只读；Windows 侧用 PowerShell 统计，大目录需要一会儿）\n");
for (const row of rows) {
  /* 残留行已经带回了 PowerShell 一次算好的字节数，不必再为每个目录单开一次扫描。 */
  let size;
  if (row.bytes !== undefined) size = human(row.bytes) ?? "0";
  else size = row.win ? duWindows(row.win) : duHost(row.host);
  console.log(`${row.label.padEnd(width)}${(size ?? "（不存在）").padStart(9)}  ${row.note}`);
  if (size !== null) console.log(`${" ".repeat(width)}${"".padStart(9)}  → ${row.action}`);
}
console.log("\n提示：清理只删「不再是当前构建图一部分」的产物；cargo / pnpm 缓存删了会拖慢下次构建。");
