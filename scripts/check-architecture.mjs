#!/usr/bin/env node
/**
 * 分层依赖检查（ARCHITECTURE.md §4 的落地）。
 *
 * 为什么需要脚本而不是靠自觉：分层规则一旦被违反，**不会报错、不会崩**，
 * 只会在半年后表现为「改一个模块要连着改五处」。那种债只能靠机器拦住。
 *
 * 规则（`ARCHITECTURE.md` §1/§2）：
 *   1. import 只能来自**允许的更低层**（见 LAYERS 的 mayImport）
 *   2. `src/features/` 下的模块**互不 import**（同一个模块内部怎么互相引都行）
 *   3. `src/dev/**` 是开发期陈列室，**豁免**（它天生要 import 一切）
 *
 * 刻意不解析语法树：规则本身就是路径级的，正则足够；简单才不容易自己出错。
 *
 * 用法：node scripts/check-architecture.mjs
 */

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative, resolve, sep } from "node:path";

const ROOT = new URL("..", import.meta.url).pathname;
const SRC = join(ROOT, "src");

/** 层定义：`dir` 用相对 src 的路径表示；`mayImport` 是允许被依赖的层名 */
const LAYERS = [
  { name: "styles", dir: "styles", mayImport: [] },
  { name: "lib", dir: "lib", mayImport: [] },
  { name: "i18n", dir: "i18n", mayImport: [] },
  { name: "api", dir: "api", mayImport: ["lib", "i18n", "styles"] },
  { name: "ui", dir: "components/ui", mayImport: ["lib", "i18n", "styles"] },
  {
    name: "features",
    dir: "features",
    mayImport: ["lib", "i18n", "styles", "api", "ui"],
  },
  {
    name: "shell",
    dir: "shell",
    mayImport: ["lib", "i18n", "styles", "api", "ui", "features"],
  },
  {
    name: "workspaces",
    dir: "workspaces",
    mayImport: ["lib", "i18n", "styles", "api", "ui", "features", "shell"],
  },
  // 组装层：src/App.tsx、src/index.tsx（其余散落文件按此层处理）
  {
    name: "app",
    dir: ".",
    mayImport: [
      "lib",
      "i18n",
      "styles",
      "api",
      "ui",
      "features",
      "shell",
      "workspaces",
    ],
  },
];

/** 豁免目录（相对 src） */
const EXEMPT = ["dev"];

const EXTS = [".ts", ".tsx"];

function walk(dir) {
  const out = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) out.push(...walk(full));
    else if (EXTS.some((ext) => full.endsWith(ext))) out.push(full);
  }
  return out;
}

/** 文件属于哪一层；取**最长匹配**，否则 `src/App.tsx` 会落进错误的层 */
function layerOf(absPath) {
  const rel = relative(SRC, absPath).split(sep).join("/");
  let best;
  for (const layer of LAYERS) {
    if (layer.dir === ".") continue;
    if (rel === layer.dir || rel.startsWith(`${layer.dir}/`)) {
      if (!best || layer.dir.length > best.dir.length) best = layer;
    }
  }
  return best ?? LAYERS.find((layer) => layer.dir === ".");
}

/** `src/features/<name>/...` 里的模块名；不在 features 下则返回 undefined */
function featureOf(absPath) {
  const rel = relative(SRC, absPath).split(sep).join("/");
  const match = /^features\/([^/]+)\//.exec(rel);
  return match ? match[1] : undefined;
}

function isExempt(absPath) {
  const rel = relative(SRC, absPath).split(sep).join("/");
  return EXEMPT.some((dir) => rel === dir || rel.startsWith(`${dir}/`));
}

const IMPORT_RE = /^\s*import\s[^"']*["']([^"']+)["']/gm;

const violations = [];
for (const file of walk(SRC)) {
  if (isExempt(file)) continue;
  const source = readFileSync(file, "utf8");
  const fromLayer = layerOf(file);
  const fromFeature = featureOf(file);

  for (const match of source.matchAll(IMPORT_RE)) {
    const specifier = match[1];
    if (!specifier.startsWith(".")) continue; // 外部包与别名不在本脚本职责内

    const target = resolve(file, "..", specifier);
    if (!target.startsWith(SRC)) continue;

    const toLayer = layerOf(target);
    const line = source.slice(0, match.index).split("\n").length;

    // 规则 2：feature 之间不许互相 import
    const toFeature = featureOf(target);
    if (fromFeature && toFeature && fromFeature !== toFeature) {
      violations.push({
        file: relative(ROOT, file),
        line,
        rule: "features 互不依赖",
        detail: `${fromFeature} → ${toFeature}（跨模块请走组合层或 app store）`,
      });
      continue;
    }

    // 规则 1：跨层时必须向下（同层内部互相引用是正常的）
    if (
      fromLayer.name !== toLayer.name &&
      !fromLayer.mayImport.includes(toLayer.name)
    ) {
      violations.push({
        file: relative(ROOT, file),
        line,
        rule: "只能向下依赖",
        detail: `${fromLayer.name} → ${toLayer.name}（${specifier}）`,
      });
    }
  }
}

if (violations.length === 0) {
  console.log("✓ 分层依赖合规（ARCHITECTURE.md §1/§2）");
  process.exit(0);
}

console.error(`✗ 发现 ${violations.length} 处分层违规：\n`);
for (const v of violations) {
  console.error(`  ${v.file}:${v.line}  [${v.rule}]`);
  console.error(`    ${v.detail}`);
}
console.error(`
允许的依赖方向见 ARCHITECTURE.md §1；一级模块之间要通信，请经过
src/workspaces/ 的组合层或 App 提供的 app store，不要在模块之间直接 import。
`);
process.exit(1);
