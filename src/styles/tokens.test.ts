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
 */

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

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
