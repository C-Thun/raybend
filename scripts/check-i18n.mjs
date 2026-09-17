#!/usr/bin/env node
/**
 * 「界面文案零硬编码」的落地守门（`DESIGN.md` §11.1 规则 1）。
 *
 * 为什么需要脚本：语言包本身有类型与 `locale-parity.test.ts` 双重保证 ——
 * 两个包**永远同步、永远不为空**。真正会漏的是**根本没进包的那句话**：
 * 直接在组件里写一句中文，类型检查不会报错、测试也不会红，
 * 直到有人切到英文才发现界面上有半句中文（而且往往是很久以后）。
 *
 * 判据只有一条：**字符串/模板串/JSX 文本里出现 CJK 字符**就算违规。
 * 中文是项目工作语言，所以「中文出现在代码里」几乎总是等于「这句该进语言包」。
 * 刻意**不查英文硬编码**：`px` / `RAW` / `ISO` / 示例路径…… 误报太多，
 * 信噪比撑不起一道门。
 *
 * 允许的三种情况（都要写清理由，不许默默放行）：
 *   1. 语言包自己（`src/i18n/`）；
 *   2. 开发期陈列室（`src/dev/`）与单测 —— 它们不进产品；
 *   3. 下面 `EXEMPT_FILES` 里逐文件列出的**非界面文字**（终端输出、控制台诊断）。
 * 另有**行级**豁免：同一行写 `// i18n-exempt: <理由>`（例如一行里混了一个
 * 必须保持中文的示例字符串）。
 *
 * 用法：node scripts/check-i18n.mjs
 */

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const ROOT = new URL("..", import.meta.url).pathname;
const SRC = join(ROOT, "src");

const EXTS = [".ts", ".tsx"];

/** 整个文件豁免：**只放非界面文字**，每条都得给出理由。 */
const EXEMPT_FILES = [
  {
    path: "api/window.ts",
    reason: "console.error 诊断（给开发者看的窗口操作日志，不面向用户）",
  },
  {
    path: "lib/easy-destroy.ts",
    reason: "console.error 诊断（移除操作失败时的开发者日志）",
  },
  {
    path: "lib/release-plan.ts",
    reason: "pnpm release 的终端输出（命令行，没有 i18n 运行时）",
  },
];

/** 目录 / 文件名豁免 */
const EXEMPT_DIRS = ["i18n", "dev"];

function walk(dir) {
  const out = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) out.push(...walk(full));
    else if (EXTS.some((ext) => full.endsWith(ext))) out.push(full);
  }
  return out;
}

/**
 * 注释里的中文不算违规（说明性文字），先剥掉注释。
 *
 * ⚠️ **块注释必须按行保留换行**（换成等量空白而不是直接删掉）：
 * 否则文件里每有一个多行注释，后面的行号就整体错位 ——
 * 报出来的位置会指到别的行上，白浪费排查时间（2026-09-17 自己踩过）。
 */
function stripComments(text) {
  return text
    .replace(/\/\*[\s\S]*?\*\//g, (block) => block.replace(/[^\n]/g, " "))
    .replace(/(^|[^:])\/\/.*$/gm, "$1");
}

const CJK = /[\u3400-\u9fff\u3040-\u30ff]/;

/** 字符串 / 模板串（够用即可：本脚本查的是「里面有没有中文」） */
const LITERAL = /"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|`(?:[^`\\]|\\.)*`/g;

const violations = [];
for (const file of walk(SRC)) {
  const rel = relative(SRC, file).split("\\").join("/");
  if (!/\.[jt]sx?$/.test(rel)) continue;
  if (rel.endsWith(".test.ts") || rel.endsWith(".test.tsx")) continue;
  if (EXEMPT_DIRS.some((dir) => rel === dir || rel.startsWith(`${dir}/`))) continue;
  if (EXEMPT_FILES.some((entry) => entry.path === rel)) continue;

  const raw = readFileSync(file, "utf8");
  const source = stripComments(raw);
  const lines = source.split("\n");
  const rawLines = raw.split("\n");

  lines.forEach((line, index) => {
    /*
     * 行级豁免：看**原始行**（注释已经被剥掉了，标记就藏在注释里）——
     * 允许写在本行或**上一行**（多行 JSX 里常常只能写在上一行）。
     */
    const marks = [rawLines[index] ?? "", rawLines[index - 1] ?? ""];
    if (marks.some((text) => text.includes("i18n-exempt"))) return;
    const at = `src/${rel}:${index + 1}`;

    let rest = line;
    for (const match of line.matchAll(LITERAL)) {
      if (CJK.test(match[0])) {
        violations.push({ at, snippet: match[0] });
      }
      // 把字面量换成等长空白，后面在「剩下的代码」里找 JSX 文本
      rest = rest.replace(match[0], " ".repeat(match[0].length));
    }
    if (CJK.test(rest)) {
      violations.push({ at, snippet: line.trim() });
    }
  });
}

if (violations.length > 0) {
  console.error(`✗ 发现 ${violations.length} 处硬编码中文文案（DESIGN.md §11.1）\n`);
  for (const { at, snippet } of violations) {
    console.error(`  ${at}`);
    console.error(`      ${snippet.slice(0, 100)}`);
  }
  console.error(
    [
      "",
      "怎么修：",
      "  · 文案进 src/i18n/zh-CN.ts 与 en-US.ts（同一条 key 两边都补），组件里走 t(\"…\")；",
      "  · 确实不该进语言包（终端输出 / 控制台诊断 / 示例数据）时：",
      "      - 行级：同一行加 `// i18n-exempt: 理由`；",
      "      - 整文件：加到 scripts/check-i18n.mjs 的 EXEMPT_FILES，并写清理由。",
      "",
    ].join("\n"),
  );
  process.exit(1);
}

console.log("✓ 无硬编码中文文案（界面文字都走语言包）");
