#!/usr/bin/env node
/**
 * Windows 产物一致性检查（零依赖）。
 *
 * **为什么要有它**：2026-09-16 真的栽过一次 —— 改了前端却忘了 `pnpm build`，
 * 于是 `dist/` 还是旧的，Windows 的 exe 内嵌的也是旧界面，用户拿到的「修复版」
 * 其实根本没有修复。当时的核对方式是「exe 里含不含当前 `dist/assets/` 的资源名」，
 * 而那是个**假绿**：dist 自己就是旧的，旧名字当然对得上。
 *
 * 所以这里查两件事，缺一不可：
 *   1. **时间**：exe 必须比 `dist/` 里任何文件都新（dist 是编译期嵌进 exe 的）；
 *   2. **内容**：`dist/index.html` 引用到的每个 `assets/*` 都要能在 exe 里找到。
 *
 * 用法：pnpm check:win        （可用 WIN_EXE=/path/to/raybend-desktop.exe 覆盖路径）
 * 退出码：0 = 一致；1 = 产物过期或对不上（打印补救命令）。
 */
import { readFileSync, readdirSync, statSync, existsSync } from "node:fs";
import { join } from "node:path";

const DIST = "dist";
const DEFAULT_EXE = "/mnt/c/rb-target/raybend/debug/raybend-desktop.exe";
const exePath = process.env.WIN_EXE ?? DEFAULT_EXE;

const fail = (message, remedy) => {
  console.error(`✗ ${message}`);
  console.error(`  补救：\n${remedy}`);
  process.exit(1);
};

if (!existsSync(exePath)) {
  fail(`找不到 Windows 产物：${exePath}`, "先在 WSL 里构建（见 AGENTS.md §5.3 的 Windows 侧命令）");
}
if (!existsSync(join(DIST, "index.html"))) {
  fail(`找不到 ${DIST}/index.html（前端没构建过？）`, "  pnpm build");
}

/** dist/ 里最新的修改时间：只要 exe 比它新，就说明这次构建看得到全部前端改动 */
const newestInDist = () => {
  let newest = 0;
  const walk = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const full = join(dir, entry.name);
      if (entry.isDirectory()) walk(full);
      else newest = Math.max(newest, statSync(full).mtimeMs);
    }
  };
  walk(DIST);
  return newest;
};

const exeTime = statSync(exePath).mtimeMs;
const distTime = newestInDist();
if (exeTime <= distTime) {
  fail(
    `Windows 产物比前端新构建还旧 —— 它内嵌的是旧界面（差 ${Math.round((distTime - exeTime) / 1000)} 秒）`,
    "  pnpm build\n" +
      "  touch src-tauri/src/lib.rs   # 逼 cargo 重编，让新 dist 真的嵌进去\n" +
      "  CARGO_TARGET_DIR='C:\\rb-target\\raybend' WSLENV='CARGO_TARGET_DIR' \\\n" +
      "    cmd.exe /c 'pushd \\\\wsl.localhost\\Ubuntu-24.04\\home\\andares\\repos\\c-thun\\raybend & cargo build -p raybend-desktop --features custom-protocol'",
  );
}

/** 资源清单要从 index.html 里读，而不是「拿 dist 里最新的 js」——后者会把没引用的文件当成必须项 */
const html = readFileSync(join(DIST, "index.html"), "utf8");
const referenced = [...new Set([...html.matchAll(/assets\/[A-Za-z0-9._-]+/g)].map((m) => m[0]))];
if (referenced.length === 0) {
  fail(`${DIST}/index.html 里没有任何 assets/ 引用 —— 构建产物不对劲`, "  pnpm build");
}
const binary = readFileSync(exePath, "latin1");
const missing = referenced.filter((name) => !binary.includes(name));
if (missing.length > 0) {
  fail(
    `产物里找不到 ${missing.length} 个前端资源（内嵌的是别的构建）：${missing.join(", ")}`,
    "按上面的补救命令重新构建（重点是先 pnpm build）",
  );
}

console.log(
  `✓ Windows 产物与前端一致\n  exe:  ${exePath}（${new Date(exeTime).toISOString()}）\n  dist: ${referenced.length} 个引用资源全部命中（最新 ${new Date(distTime).toISOString()}）`,
);
