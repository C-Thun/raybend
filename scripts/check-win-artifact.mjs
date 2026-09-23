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
 *   2. **内容**：`dist/index.html` 与 `dist/splash.html` 引用到的每个资源都要能在 exe 里找到。
 *      （闪屏那页也要查：它的图要是没嵌进去，闪屏就是一个**透明空窗** —— 不报错、不明显，
 *        正好是最难查的那种假绿。2026-09-17 加闪屏时补上。）
 *
 * 2026-09-24 又栽了一次**同一族的**坑（这次是 Rust 侧）：主程序重建了、`raybend-raw-worker`
 * 没有（它是 `raybend` 包里的 bin，构建命令只选 `raybend-desktop` 时不会被构建），
 * 于是编辑器的线性解码在真机上一直失败 —— 界面只表现为「永远卡在正在载入照片」。
 * 所以现在多查一件：
 *   3. **RAW worker**：存在、不比源码旧、而且**二进制里有当前的 `PROTOCOL_TAG`**
 *      （时间戳只是线索，内容才是证据 —— 与上面第 1/2 条同一个道理）。
 *
 * 用法：pnpm check:win        （可用 WIN_EXE=/path/to/raybend-desktop.exe 覆盖路径）
 * 退出码：0 = 一致；1 = 产物过期或对不上（打印补救命令）。
 */
import { readFileSync, readdirSync, statSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";

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

/** Rust 源码里最新的修改时间（worker 必须不比它旧） */
const newestRustSource = () => {
  let newest = 0;
  const walk = (dir) => {
    if (!existsSync(dir)) return;
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const full = join(dir, entry.name);
      if (entry.isDirectory()) walk(full);
      else if (entry.name.endsWith(".rs")) newest = Math.max(newest, statSync(full).mtimeMs);
    }
  };
  walk("crates");
  walk("src-tauri/src");
  return newest;
};

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

/**
 * 资源清单要从 HTML 与资源目录里读，而不是「拿 dist 里最新的文件」——后者会把没引用的文件当成必须项。
 *
 * 应用外壳：`index.html` 里引用到的每个 `assets/*`。
 * 启动闪屏：`splash.html` 里的图片路径是**运行时拼**出来的（`"splash/splash-" + lang + ".webp"`，
 * 因为要随机抽中/英），所以不按字面量匹配，改为查两件事：
 *   ① 页面里确实有 `splash/splash-` 这个拼装前缀；② `dist/splash/` 下的**每一个**文件都嵌进了 exe。
 * ②才是有分量的那条：少嵌一张，恰好抽到那张时闪屏就是个**透明空窗**（不报错、不明显）。
 */
const referencedFromShell = (() => {
  const html = readFileSync(join(DIST, "index.html"), "utf8");
  return [...new Set([...html.matchAll(/assets\/[A-Za-z0-9._-]+/g)].map((m) => m[0]))];
})();
if (referencedFromShell.length === 0) {
  fail(`${DIST}/index.html 里没有任何 assets/ 引用 —— 构建产物不对劲`, "  pnpm build");
}

const splashPage = join(DIST, "splash.html");
if (!existsSync(splashPage)) fail(`找不到 ${DIST}/splash.html`, "  pnpm build");
if (!readFileSync(splashPage, "utf8").includes("splash/splash-")) {
  fail(
    `${DIST}/splash.html 里没有拼装闪屏图片路径的代码 —— 闪屏会是个空窗`,
    "  pnpm build",
  );
}
const splashDir = join(DIST, "splash");
if (!existsSync(splashDir)) fail(`找不到 ${DIST}/splash/`, "  pnpm build");
const referencedSplash = readdirSync(splashDir).map((name) => `splash/${name}`);
if (referencedSplash.length === 0) {
  fail(`${DIST}/splash/ 里一张图都没有 —— 闪屏会是个空窗`, "  pnpm build");
}

const referenced = [...new Set([...referencedFromShell, ...referencedSplash])];
const binary = readFileSync(exePath, "latin1");
const missing = referenced.filter((name) => !binary.includes(name));
if (missing.length > 0) {
  fail(
    `产物里找不到 ${missing.length} 个前端资源（内嵌的是别的构建）：${missing.join(", ")}`,
    "按上面的补救命令重新构建（重点是先 pnpm build）",
  );
}

// ── ③ RAW worker：独立进程，主程序要用它解码（它过期 = 编辑功能静默失灵）──
const workerExe = join(dirname(exePath), "raybend-raw-worker.exe");
if (!existsSync(workerExe)) {
  fail(
    `找不到 RAW worker：${workerExe}`,
    `    cmd.exe /c 'pushd \\\\wsl.localhost\\Ubuntu-24.04\\home\\andares\\repos\\c-thun\\raybend & cargo build -p raybend-desktop -p raybend --features custom-protocol'`,
  );
}

/** 源码里当前的协议标签（从 Rust 源码读，避免两处各写一份会漂移） */
const PROTOCOL_TAG = (() => {
  const source = readFileSync("crates/raybend/src/raw/worker.rs", "utf8");
  const found = /pub const PROTOCOL_TAG: &str = "([^"]+)"/.exec(source);
  if (!found) {
    fail(
      "在 crates/raybend/src/raw/worker.rs 里找不到 `pub const PROTOCOL_TAG`",
      "  这个脚本与源码有个约定：标签常量必须叫这个名字（改了源码就同步改这里）",
    );
  }
  return found[1];
})();

const workerTime = statSync(workerExe).mtimeMs;
const workerBinary = readFileSync(workerExe, "latin1");
if (!workerBinary.includes(PROTOCOL_TAG)) {
  fail(
    `RAW worker 过期：二进制里没有 ${PROTOCOL_TAG}（主程序新、worker 旧 —— 编辑功能会静默失灵）`,
    `    cmd.exe /c 'pushd \\\\wsl.localhost\\Ubuntu-24.04\\home\\andares\\repos\\c-thun\\raybend & cargo build -p raybend-desktop -p raybend --features custom-protocol'`,
  );
}
if (workerTime < newestRustSource()) {
  fail(
    `RAW worker 比 Rust 源码旧（worker ${new Date(workerTime).toISOString()} < 源码 ${new Date(newestRustSource()).toISOString()}）`,
    "    cmd.exe /c 'pushd \\\\wsl.localhost\\Ubuntu-24.04\\home\\andares\\repos\\c-thun\\raybend & cargo build -p raybend-desktop -p raybend --features custom-protocol'",
  );
}

console.log(
  `✓ Windows 产物与前端一致\n  exe:  ${exePath}（${new Date(exeTime).toISOString()}）\n  dist: ${referenced.length} 个引用资源全部命中（最新 ${new Date(distTime).toISOString()}）\n  worker: ${workerExe}（${new Date(workerTime).toISOString()}，含 ${PROTOCOL_TAG}）`,
);
