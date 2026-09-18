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
 *   4. `src/assets/**` 是**纯数据**（图片/字体），没有方向可言：除它自己外每层都允许 import
 *   5. **前端不碰像素**（`AGENTS.md` §6.1 的红线）：`src/` 下不许出现读/写像素的浏览器 API
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
  // 素材层：**纯数据**（图片、字体…），它自己不 import 任何东西，
  // 而**任何层**都可能用到它 —— 所以除它以外的每层都把它列进 mayImport。
  // （反过来的例外情况见 MATERIAL_LAYERS：素材层自己不需要遵守方向。）
  { name: "assets", dir: "assets", mayImport: [] },
  { name: "styles", dir: "styles", mayImport: ["assets"] },
  { name: "lib", dir: "lib", mayImport: ["assets"] },
  { name: "i18n", dir: "i18n", mayImport: ["assets"] },
  { name: "api", dir: "api", mayImport: ["assets", "lib", "i18n", "styles"] },
  { name: "ui", dir: "components/ui", mayImport: ["assets", "lib", "i18n", "styles"] },
  {
    name: "features",
    dir: "features",
    mayImport: ["assets", "lib", "i18n", "styles", "api", "ui"],
  },
  {
    name: "shell",
    dir: "shell",
    mayImport: ["assets", "lib", "i18n", "styles", "api", "ui", "features"],
  },
  {
    name: "workspaces",
    dir: "workspaces",
    mayImport: ["assets", "lib", "i18n", "styles", "api", "ui", "features", "shell"],
  },
  // 组装层：src/App.tsx、src/index.tsx（其余散落文件按此层处理）
  {
    name: "app",
    dir: ".",
    mayImport: [
      "assets",
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

/**
 * 规则 5：**前端不碰像素**（`AGENTS.md` §6.1）。
 *
 * 为什么单独拦一条：读像素这件事**写起来太容易了** —— 一个 `getContext("2d")` 加上
 * 十几行就能在浏览器里算出直方图，而一旦开了这个头，色彩空间、缩放插值、视口变换
 * 就会慢慢都搬到前端，等到做 RAW/广色域/GPU 渲染时要回头拆掉一大堆。
 * 像素属于 Rust，前端只收字节与 URL。
 *
 * 命中范围：读/写像素与色彩空间相关的浏览器 API，以及 canvas 上下文。
 * 文案里（注释、字符串）提到这些名字不算 —— 所以匹配前先去注释。
 */
const PIXEL_API_RE =
  /getImageData|createImageData|putImageData|ImageData|Uint8ClampedArray|drawImage|createImageBitmap|getContext\(|OffscreenCanvas|HTMLCanvasElement|colorSpace|colorProfile|iccProfile/;

/**
 * 去掉注释再查（只查「真代码」）。
 *
 * `://` 的行不做截断，否则 `https://…` 后面的真代码会被当成注释吃掉。
 */
function stripComments(source) {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, " ")
    .split("\n")
    .map((line) => (line.includes("://") ? line : line.split("//")[0]))
    .join("\n");
}

const violations = [];
for (const file of walk(SRC)) {
  if (isExempt(file)) continue;
  const source = readFileSync(file, "utf8");
  const fromLayer = layerOf(file);
  const fromFeature = featureOf(file);

  const code = stripComments(source);
  const pixelMatch = PIXEL_API_RE.exec(code);
  if (pixelMatch) {
    violations.push({
      file: relative(ROOT, file),
      line: code.slice(0, pixelMatch.index).split("\n").length,
      rule: "前端不碰像素",
      detail: `${pixelMatch[0]}（像素处理属于 Rust：AGENTS.md §6.1，前端只拿字节与 URL）`,
    });
  }

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

如果命中的是「前端不碰像素」：像素/色彩的处理属于 Rust 侧（AGENTS.md §6.1 的红线），
前端只应该拿到**字节**（走统一取图口）或**对象 URL**，判读与统计交给 Rust。
`);
process.exit(1);
