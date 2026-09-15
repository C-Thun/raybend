#!/usr/bin/env node
/**
 * 禁止硬编码色值（DESIGN.md §9.1 的纪律落地）。
 *
 * 为什么需要：色值只允许出现在 `DESIGN.md` 与 `src/styles/tokens.css` 两处。
 * 一旦组件里写了 `#8DB8AA` 或 `rgb(...)`，改色时就会漏改 ——
 * 而「主辅色出图后还要调」是用户明确的需求，漏改会直接导致两套颜色并存。
 *
 * 允许的写法：
 *   - Tailwind 语义工具类：bg-surface-bar / text-fg-1 / bg-state-selected
 *   - 令牌引用：var(--brand) / var(--surface-main)
 *
 * 用法：node scripts/check-hardcoded-colors.mjs
 */

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const ROOT = new URL("..", import.meta.url).pathname;
const SRC = join(ROOT, "src");

/** 令牌定义文件本身当然要有色值 */
const ALLOWLIST = [join(SRC, "styles", "tokens.css")];

const EXTS = [".ts", ".tsx", ".css", ".js", ".jsx"];

/** 匹配 #rgb / #rrggbb / #rrggbbaa、rgb()/rgba()/hsl()/hsla() */
const PATTERNS = [
 { name: "十六进制色值", re: /#[0-9a-fA-F]{3,8}\b/g },
 { name: "rgb()/hsl() 字面量", re: /\b(?:rgba?|hsla?)\s*\(/g },
];

/** 注释里的色值不算违规（说明性文字），先剥掉注释 */
function stripComments(text) {
 return text
  .replace(/\/\*[\s\S]*?\*\//g, "")
  .replace(/(^|[^:])\/\/.*$/gm, "$1");
}

function walk(dir) {
 const out = [];
 for (const entry of readdirSync(dir)) {
  const full = join(dir, entry);
  if (statSync(full).isDirectory()) out.push(...walk(full));
  else if (EXTS.some((e) => full.endsWith(e))) out.push(full);
 }
 return out;
}

const violations = [];
for (const file of walk(SRC)) {
 if (ALLOWLIST.includes(file)) continue;
 const source = stripComments(readFileSync(file, "utf8"));
 source.split("\n").forEach((line, i) => {
  for (const { name, re } of PATTERNS) {
   const hits = line.match(re);
   if (hits)
    violations.push({
     file: relative(ROOT, file),
     line: i + 1,
     name,
     text: line.trim(),
    });
  }
 });
}

if (violations.length === 0) {
 console.log("✓ 无硬编码色值（色值仅出现在 tokens.css）");
 process.exit(0);
}

console.error(`✗ 发现 ${violations.length} 处硬编码色值：\n`);
for (const v of violations) {
 console.error(`  ${v.file}:${v.line}  [${v.name}]`);
 console.error(`    ${v.text}`);
}
console.error(`
修正方式：改用语义工具类（bg-surface-bar / text-fg-1 / bg-state-selected …）
或用令牌引用 var(--brand)。色值只允许写在 src/styles/tokens.css —— 见 DESIGN.md §9。
`);
process.exit(1);
