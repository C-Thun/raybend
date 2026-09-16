/**
 * 令牌表的**结构性检查**（`DESIGN.md` §8）。
 *
 * 盯两件事，都是踩过的坑：
 *
 * 1. **两档密度必须定义同一批令牌**。曾经把 `--dialog-pad` / `--selected-max-h`
 *    只写进紧凑块，结果宽松档下这两个引用**根本没有值** —— 弹窗一点内边距都没有、
 *    「已选目录」的高度上限也失效。这类错在紧凑档下完全看不出来。
 * 2. **同一个令牌在两档里必须给不同的值**（真的该随密度变的那些）——
 *    否则写了等于没写。白名单是「确定两档同值」的几项。
 *
 * 后来补了两条（都是「写了但没生效」的同一种病）：
 *
 * 3. **两个主题块定义同一批令牌**（同上，只是换成了主题维度）。
 * 4. **组件引用到的令牌必须在 tokens.css 里真的有定义** ——
 *    2026-09-16 的真实事故：`Tile.tsx` 写 `bg-(--tile-bar-scrim)` 而令牌没人定义，
 *    `background-color: var(--未定义)` 在计算值阶段直接失效成 `transparent`，
 *    于是信息条**只有字、没有底**，且控制台一声不响。
 */

import assert from "node:assert/strict";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import test from "node:test";

const SRC = new URL("..", import.meta.url).pathname;
const css = readFileSync(new URL("./tokens.css", import.meta.url), "utf8");

function densityBlock(name: "compact" | "loose"): Map<string, string> {
  const start = css.indexOf(`:root[data-density="${name}"]`);
  assert.ok(start >= 0, `tokens.css 里找不到 ${name} 密度块`);
  let depth = 0;
  let end = start;
  for (let index = start; index < css.length; index += 1) {
    if (css[index] === "{") depth += 1;
    else if (css[index] === "}") {
      depth -= 1;
      if (depth === 0) {
        end = index;
        break;
      }
    }
  }
  const body = css.slice(start, end);
  const out = new Map<string, string>();
  for (const match of body.matchAll(/^\s*(--[a-z0-9-]+):\s*([^;]+);/gm)) {
    out.set(match[1]!, match[2]!.trim());
  }
  return out;
}

const compact = densityBlock("compact");
const loose = densityBlock("loose");

test("两档密度定义的是**同一批**令牌", () => {
  const missingLoose = [...compact.keys()].filter((key) => !loose.has(key));
  const missingCompact = [...loose.keys()].filter((key) => !compact.has(key));
  assert.deepEqual(
    missingLoose,
    [],
    `这些令牌只在紧凑档里有，宽松档下它们是空值：${missingLoose.join(", ")}`,
  );
  assert.deepEqual(
    missingCompact,
    [],
    `这些令牌只在宽松档里有：${missingCompact.join(", ")}`,
  );
});

/** 两档**故意同值**的：圆角是观感基准；开关/勾选框/移除按钮有点得中的下限（DESIGN.md §8.3） */
const SAME_IN_BOTH = new Set([
  "--radius",
  "--switch-w",
  "--switch-h",
  "--switch-knob",
  "--checkbox-size",
  "--remove-hit",
  "--remove-icon",
  "--scrollbar-w",
  "--panel-w-left",
  "--panel-w-right",
  "--toggle-block-icon",
]);

test("该随密度变的令牌，两档的值真的不同", () => {
  const same: string[] = [];
  for (const [key, value] of compact) {
    if (SAME_IN_BOTH.has(key)) continue;
    if (loose.get(key) === value) same.push(key);
  }
  assert.deepEqual(
    same,
    [],
    `这些令牌两档同值，写进密度块等于没写（要么改值，要么加进白名单）：${same.join(", ")}`,
  );
});

/**
 * 把 CSS 拆成 (选择器, 块内容) —— 用来问「这条令牌到底定义在哪个块里」。
 * 踩过的两个坑都是「定义块挂错了属性」：
 *   ① 写进密度块 → 另一档没有值；② 写进 dark 主题块 → 浅色主题没有值。
 */
function blocks(): Array<{ selector: string; body: string }> {
  const out: Array<{ selector: string; body: string }> = [];
  const pattern = /([^{}]+)\{([^{}]*)\}/g;
  for (const match of css.matchAll(pattern)) {
    // 选择器要把前面的注释去掉：`/* 说明 */ :root` 的选择器就是 :root
    const selector = match[1]!.replace(/\/\*[\s\S]*?\*\//g, "").trim();
    out.push({ selector, body: match[2]! });
  }
  return out;
}

test("与主题/密度无关的令牌必须定义在**不带属性**的 :root 里", () => {
  for (const key of ["--dialog-pad", "--selected-max-h"]) {
    const owners = blocks()
      .filter((entry) => new RegExp(`${key}:`).test(entry.body))
      .map((entry) => entry.selector);
    assert.ok(owners.length > 0, `${key} 没有任何定义`);
    const themed = owners.filter((selector) => selector !== ":root");
    assert.deepEqual(
      themed,
      [],
      `${key} 被定义在带属性的选择器里（${themed.join(" / ")}）—— 换个主题或密度档就没有值了`,
    );
  }
});

/** 某个主题下**能拿到**的所有令牌：主题块自己 + 不带属性的 `:root`（它在两个主题下都生效） */
function tokensAvailableIn(theme: "dark" | "light"): Set<string> {
  const out = new Set<string>();
  for (const entry of blocks()) {
    const selectors = entry.selector.split(",").map((part) => part.replace(/\s+/g, ""));
    const themed = selectors.includes(`:root[data-theme="${theme}"]`);
    const bare = selectors.includes(":root");
    if (!themed && !bare) continue;
    for (const match of entry.body.matchAll(/^\s*(--[a-z0-9-]+):/gm)) out.add(match[1]!);
  }
  return out;
}

/** 作为**主题维度的覆盖**写下来的那些令牌（写错了就只在一个主题下存在） */
function themedTokens(): Set<string> {
  const out = new Set<string>();
  for (const entry of blocks()) {
    if (!/data-theme=/.test(entry.selector)) continue;
    for (const match of entry.body.matchAll(/^\s*(--[a-z0-9-]+):/gm)) out.add(match[1]!);
  }
  return out;
}

test("两个主题块定义的是**同一批**令牌", () => {
  const dark = tokensAvailableIn("dark");
  const light = tokensAvailableIn("light");
  const missingLight = [...themedTokens()].filter((key) => !light.has(key));
  const missingDark = [...themedTokens()].filter((key) => !dark.has(key));
  assert.deepEqual(
    missingLight,
    [],
    `这些令牌在深色主题里有效、浅色主题下却是空值：${missingLight.join(", ")}`,
  );
  assert.deepEqual(
    missingDark,
    [],
    `这些令牌在浅色主题里有效、深色主题下却是空值：${missingDark.join(", ")}`,
  );
});

/**
 * 组件引用了、但**不归 tokens.css 管**的变量。
 * 每条都要写清是谁给的 —— 不然这个白名单很快就会变成垃圾场。
 */
const PROVIDED_ELSEWHERE = new Map([
  // Ark（Zag）的分段指示块，运行时自己写内联变量（`SegmentedControl.tsx` 的注释里有教训）
  ["--left", "Ark SegmentGroup.Indicator"],
  ["--top", "Ark SegmentGroup.Indicator"],
  ["--width", "Ark SegmentGroup.Indicator"],
  ["--height", "Ark SegmentGroup.Indicator"],
  // 弹窗层级：Ark 的 Positioner 会吃掉内联 z-index，`index.css` 里按 data 属性赋值
  ["--z-index", "src/index.css（Ark Positioner 的层级栈）"],
]);

function sourceFiles(): string[] {
  const exts = [".ts", ".tsx", ".css"];
  const out: string[] = [];
  const walk = (dir: string): void => {
    for (const entry of readdirSync(dir)) {
      const full = join(dir, entry);
      if (statSync(full).isDirectory()) walk(full);
      else if (exts.some((ext) => full.endsWith(ext))) out.push(full);
    }
  };
  walk(SRC);
  // tokens.css 自己不算：它就是定义处
  return out.filter((file) => !file.endsWith(join("styles", "tokens.css")));
}

/** 注释里的引用不算（说明性文字），先剥掉 —— 与 `check-hardcoded-colors.mjs` 同一套 */
function stripComments(text: string): string {
  return text.replace(/\/\*[\s\S]*?\*\//g, "").replace(/(^|[^:])\/\/.*$/gm, "$1");
}

test("组件里引用到的令牌，tokens.css 里必须有定义", () => {
  const defined = new Set(
    [...css.matchAll(/^\s*(--[a-z0-9-]+):/gm)].map((match) => match[1]!),
  );

  /*
   * **先扫完所有文件、再判定** —— 不能边扫边判：
   * 「运行时赋值的令牌」可能出现在引用它的文件**之后**（`--tile-cell`：`Tile.tsx` 在前、
   * 给它赋值的 `PhotoGrid.tsx` 在后），边扫边判会漏掉这个豁免、白白报一条假阳性。
   */
  const references = new Map<string, Set<string>>();
  const assignedAtRuntime = new Set<string>();
  for (const file of sourceFiles()) {
    const source = stripComments(readFileSync(file, "utf8"));
    const refs = new Set<string>([
      ...[...source.matchAll(/var\((--[a-z0-9-]+)/g)].map((match) => match[1]!),
      // Tailwind v4 的简写：`bg-(--x)` 与 `translate-x-(--x)` 这类
      ...[...source.matchAll(/\((--[a-z0-9-]+)\)/g)].map((match) => match[1]!),
    ]);
    for (const match of source.matchAll(/["'](--[a-z0-9-]+)["']\s*:/g)) {
      assignedAtRuntime.add(match[1]!);
    }
    references.set(file, refs);
  }

  const missing = new Map<string, Set<string>>();
  for (const [file, refs] of references) {
    for (const key of refs) {
      if (defined.has(key) || assignedAtRuntime.has(key)) continue;
      if (PROVIDED_ELSEWHERE.has(key)) continue;
      if (!missing.has(key)) missing.set(key, new Set());
      missing.get(key)!.add(relative(SRC, file));
    }
  }

  assert.deepEqual(
    [...missing.keys()].sort(),
    [],
    `这些令牌被引用了、但 tokens.css 里没有定义（计算值会失效成透明/零值，不报错）：\n${[
      ...missing,
    ]
      .map(([key, files]) => `  ${key}  ← ${[...files].join(", ")}`)
      .join("\n")}`,
  );
});
