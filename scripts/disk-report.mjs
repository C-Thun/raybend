#!/usr/bin/env node
/**
 * **磁盘占用报告**（只读）：回答「几十 G 去哪了、哪些该清、哪些是故意留的」。
 *
 * 为什么要它：构建缓存分布在四处（WSL `target/`、Windows 构建 `target/`、旧构建目录
 * `C:\rb-target`、全局包仓库），**没有任何一个地方会告诉你总账** ——
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
import { existsSync } from "node:fs";
import { join, win32 } from "node:path";
import { fileURLToPath } from "node:url";
import { discoverWindowsReleasePaths, windowsPowerShell } from "./lib/windows-paths.mjs";

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
  { label: "Windows 构建 target", win: win?.windowsTarget, note: "发版 / 调试构建缓存（刻意保留，让下次构建不冷启）", action: "pnpm clean:win" },
  { label: "旧构建目录（2026-09-29 前）", win: "C:\\rb-target", note: "已弃用：脚本早就不往这写，也没人清", action: "pnpm clean:win --target-dir 'C:\\rb-target\\raybend' --keep-days 0" },
  { label: "发布产物 release-out/", host: join(ROOT, "release-out"), note: "保留：对应 GitHub Release 的字节副本", action: "旧 test-* 目录可手动删" },
  { label: "AI 输入快照 .release/", host: join(ROOT, ".release"), note: "保留：模型 + CPU 运行库的冻结副本，构建复用", action: "删除后下次构建重新校验 / 复制" },
  { label: "前端产物 dist/", host: join(ROOT, "dist"), note: "小，随时可重建", action: "pnpm build" },
  { label: "node_modules/", host: join(ROOT, "node_modules"), note: "依赖", action: "pnpm install 重建" },
  { label: "pnpm store（全局共享）", host: storePath, note: "所有 pnpm 项目的包仓库", action: "pnpm store prune" },
  { label: "cargo registry（全局）", host: join(process.env.HOME ?? "", ".cargo/registry"), note: "crate 下载缓存", action: "保留（删了要重新下载）" },
  { label: "应用数据（含上面构建缓存）", win: win ? win32.dirname(win.windowsBase) : null, note: "日志 / 缩略图缓存 / 数据库备份等运行数据", action: "按需手工清理子目录" },
];

const width = Math.max(...rows.map(row => row.label.length)) + 2;
console.log("RayBend 磁盘占用报告（只读；Windows 侧用 PowerShell 统计，大目录需要一会儿）\n");
for (const row of rows) {
  const size = row.win ? duWindows(row.win) : duHost(row.host);
  console.log(`${row.label.padEnd(width)}${(size ?? "（不存在）").padStart(9)}  ${row.note}`);
  if (size !== null) console.log(`${" ".repeat(width)}${"".padStart(9)}  → ${row.action}`);
}
console.log("\n提示：清理只删「不再是当前构建图一部分」的产物；cargo / pnpm 缓存删了会拖慢下次构建。");
