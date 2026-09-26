/**
 * 全屏清单构造的冒烟（`lib/fullscreen-target.ts`）。
 *
 * 盯一件事：**锚点不在清单里必须给 `null`**（宁可不看也不错位，别猜）。
 */

import assert from "node:assert/strict";
import test from "node:test";

import { buildFullscreenTarget } from "./fullscreen-target.ts";

const photos = [
  { id: "1", path: "/lib/a.jpg", fileName: "a.jpg" },
  { id: "2", path: "/lib/b.ORF", fileName: "b.ORF" },
  { id: "3", path: "/lib/c.jpg", fileName: "c.jpg" },
];

test("清单：下标跟着锚点走，字段只带三个", () => {
  const target = buildFullscreenTarget(photos, "2");
  assert.notEqual(target, null);
  assert.equal(target!.index, 1);
  assert.deepEqual(target!.items, [
    { id: "1", path: "/lib/a.jpg", fileName: "a.jpg" },
    { id: "2", path: "/lib/b.ORF", fileName: "b.ORF" },
    { id: "3", path: "/lib/c.jpg", fileName: "c.jpg" },
  ]);
});

test("清单：没有锚点 / 锚点不在清单里 → null（不猜、不错位）", () => {
  assert.equal(buildFullscreenTarget(photos, null), null);
  assert.equal(
    buildFullscreenTarget(photos, "99"),
    null,
    "被筛掉的那张不该开出来",
  );
  assert.equal(buildFullscreenTarget([], "1"), null, "空清单开不了");
});

test("导出全屏保留明确的 issue 身份，同照片两稿不合并", () => {
  const items = ["latest", "issue:7"].map((variant) => ({
    id: variant,
    path: "/库/中文.jpg",
    fileName: variant,
    exportVariant: { repositoryId: "repo", reference: { assetId: 1, variant } },
    ignored: "extra",
  }));
  const target = buildFullscreenTarget(items, "issue:7")!;
  assert.equal(target.index, 1);
  assert.equal(target.items.length, 2);
  assert.deepEqual(target.items[1]?.exportVariant, items[1]?.exportVariant);
  assert.equal("ignored" in target.items[1]!, false);
});
