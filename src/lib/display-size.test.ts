/**
 * 显示尺寸矩阵（`specs/display-size-source.md` §6.1）：变体 → 尺寸 + 回退链。
 *
 * 这里只测**选择与回退**；方向 / 裁切 / 旋转的数学全在 Rust 侧，前端不算第二份。
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  displaySizeOf,
  issueIdOf,
  issueOf,
  sourceBaseOf,
  type DisplaySizeAssetRow,
  type DisplaySizeIssueRow,
} from "./display-size.ts";

/** 字段齐全的列表项；只覆盖本测试关心的尺寸字段。 */
function asset(overrides: Partial<DisplaySizeAssetRow> = {}): DisplaySizeAssetRow {
  return {
    width: 4000,
    height: 3000,
    rawWidth: 5208,
    rawHeight: 3904,
    latestWidth: null,
    latestHeight: null,
    latestSourceBase: null,
    ...overrides,
  };
}

function issue(overrides: Partial<DisplaySizeIssueRow> = {}): DisplaySizeIssueRow {
  return {
    id: 7,
    width: 2604,
    height: 1952,
    sourceBase: "raw",
    ...overrides,
  };
}

test("没选变体 / latest：编辑过用 latest 的输出尺寸，没编辑过退回基准", () => {
  const plain = asset();
  assert.deepEqual(displaySizeOf(plain, undefined, undefined), { width: 4000, height: 3000 });
  assert.deepEqual(displaySizeOf(plain, "latest", undefined), { width: 4000, height: 3000 });

  const edited = asset({ latestWidth: 2000, latestHeight: 1500 });
  assert.deepEqual(displaySizeOf(edited, undefined, undefined), { width: 2000, height: 1500 });
  assert.deepEqual(displaySizeOf(edited, "latest", undefined), { width: 2000, height: 1500 });
});

test("sooc 用基准尺寸；raw 只认 RAW 自己的尺寸（缺了给 null，不许拿基准充数）", () => {
  const both = asset();
  assert.deepEqual(displaySizeOf(both, "sooc", undefined), { width: 4000, height: 3000 });
  assert.deepEqual(displaySizeOf(both, "raw", undefined), { width: 5208, height: 3904 });

  /*
   * 崔总 2026-10-11 真机：「view 里 raw 图尺寸没有跟着本身走，而是缩进了 sooc 的尺寸里」——
   * 老库 RAW 行没回填时，`?? base` 就是那句症状的代码。这里钉住：**给 null**（= 还没拿到），
   * 由 `export_snapshots` 的懒补（读一次文件头）把值补上，调用方按「尺寸未知」处理。
   */
  const rawOnly = asset({ rawWidth: null, rawHeight: null });
  assert.equal(displaySizeOf(rawOnly, "raw", undefined), null, "RAW 行没尺寸 → null，不退到基准");
});

test("issue:N 只认定稿自己的输出尺寸；拿不到就给 null（不许拿别的变体凑）", () => {
  const current = issue();
  assert.deepEqual(displaySizeOf(asset(), "issue:7", current), { width: 2604, height: 1952 });
  assert.equal(
    displaySizeOf(asset(), "issue:7", undefined),
    null,
    "定稿列表还没到 → null（同一个「还不知道」，不是「退回基准」）",
  );
  assert.equal(
    displaySizeOf(asset(), "issue:8", current),
    null,
    "id 对不上号（旧列表）→ null",
  );
  assert.equal(
    displaySizeOf(asset(), "issue:7", issue({ width: null, height: null })),
    null,
    "定稿尺寸未回填 → null（渲染时后端会现算，下次读就有）",
  );
});

test("坏值（0 / 负数 / null）一律当未知：显示 0×0 不行，拿别的变体顶也不行", () => {
  assert.equal(
    displaySizeOf(asset({ rawWidth: 0, rawHeight: 3904 }), "raw", undefined),
    null,
  );
  assert.deepEqual(
    displaySizeOf(asset({ latestWidth: -1, latestHeight: 900 }), undefined, undefined),
    { width: 4000, height: 3000 },
  );
  const broken = asset({ width: 0, height: 0, rawWidth: null, rawHeight: null });
  assert.equal(displaySizeOf(broken, undefined, undefined), null, "库里什么都没有 → null（调用方走旧行为）");
});

test("认不出的变体串不猜：issue:0 / issue:abc / 其它一律给 null", () => {
  assert.equal(issueIdOf("issue:7"), 7);
  assert.equal(issueIdOf("issue:0"), null);
  assert.equal(issueIdOf("issue:"), null);
  assert.equal(issueIdOf("issue:abc"), null);
  assert.equal(issueIdOf("latest"), null);
  assert.equal(displaySizeOf(asset(), "issue:0", issue({ id: 0 })), null);
});

test("issueOf 从定稿列表里按 id 找那条（没有列表 / 找不到 → undefined）", () => {
  const library = { issues: [issue({ id: 3 }), issue({ id: 7 })] };
  assert.equal(issueOf(library, "issue:7")?.id, 7);
  assert.equal(issueOf(library, "issue:9"), undefined);
  assert.equal(issueOf(library, undefined), undefined);
  assert.equal(issueOf(null, "issue:7"), undefined);
});

/*
 * 来源基底（flowinfo 的「文件类型」靠它：崔总 2026-10-11 定「类型也跟随变体」）——
 * 与尺寸矩阵同一个变体映射，所以两条断言写在一个 test 里、一起看。
 */
test("sourceBaseOf：变体的来源基底（raw / sooc / 说不清）", () => {
  // SOOC 与 RAW 变体自己就是基底
  assert.equal(sourceBaseOf(asset(), "sooc", undefined), "sooc");
  assert.equal(sourceBaseOf(asset(), "raw", undefined), "raw");
  // latest（含没选）看编辑栈；没编辑栈说不清
  assert.equal(sourceBaseOf(asset({ latestSourceBase: "raw" }), undefined, undefined), "raw");
  assert.equal(sourceBaseOf(asset({ latestSourceBase: "sooc" }), "latest", undefined), "sooc");
  assert.equal(sourceBaseOf(asset(), undefined, undefined), null);
  // 定稿看它自己的基底；找不到那条 → 说不清（不猜成 raw）
  assert.equal(sourceBaseOf(asset(), "issue:7", issue({ sourceBase: "sooc" })), "sooc");
  assert.equal(sourceBaseOf(asset(), "issue:9", issue()), null);
  assert.equal(sourceBaseOf(asset(), "issue:7", undefined), null);
  // 认不出的串 / 后端的脏值一律说不清
  assert.equal(sourceBaseOf(asset({ latestSourceBase: "dng" }), undefined, undefined), null);
  assert.equal(sourceBaseOf(asset(), "issue:0", issue({ id: 0 })), null);
});
