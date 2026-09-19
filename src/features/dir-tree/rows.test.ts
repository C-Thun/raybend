/**
 * 目录树的行模型测试。
 *
 * 重点是三条容易做错、而且在界面上不容易看出来的规则：
 *   1. 折叠节点的子树**整棵不出现**（不是隐藏，是不生成行）；
 *   2. 没读过的目录**乐观地**画展开箭头（否则用户点不开）；
 *   3. 读取失败或深度超限时**不能无限递归**（数据来自文件系统，防御性留闸）。
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import type { DirEntry, Volume } from "../../api/types.ts";
import { buildTreeRows, volumeDisplayName } from "./rows.ts";

function dir(path: string, name = path, hasChildren = true): DirEntry {
  // 默认 true = 「父一级的清单说它还有下一层」；旧测试的意图不变
  return { path, name, hasChildren };
}

function volume(path: string, kind: Volume["kind"] = "local"): Volume {
  return { path, kind, kindLabel: kind };
}

/** 用一张静态表当「已读子目录」 */
function table(entries: Record<string, DirEntry[]>) {
  return (path: string) => entries[path];
}

test("卷在顶层；未展开时不出现子行", () => {
  const rows = buildTreeRows({
    volumes: [volume("D:\\"), volume("E:\\")],
    childrenOf: table({ "D:\\": [dir("D:\\Photos")] }),
    isExpanded: () => false,
  });
  assert.deepEqual(
    rows.map((row) => [row.name, row.depth]),
    [
      ["D:\\", 0],
      ["E:\\", 0],
    ],
  );
});

test("展开的目录按深度优先摊平（顺序即显示顺序）", () => {
  const rows = buildTreeRows({
    volumes: [volume("D:\\")],
    childrenOf: table({
      "D:\\": [dir("D:\\Photos", "Photos"), dir("D:\\Work", "Work")],
      "D:\\Photos": [dir("D:\\Photos\\2024", "2024")],
    }),
    isExpanded: (path) => path === "D:\\" || path === "D:\\Photos",
  });

  assert.deepEqual(
    rows.map((row) => `${"  ".repeat(row.depth)}${row.name}`),
    ["D:\\", "  Photos", "    2024", "  Work"],
  );
});

test("多扫一层：父一级的清单说没有子目录 → 不画展开箭头", () => {
  /*
   * 人类 2026-09-19：「目录下没子目录了还显示展开图标误导人」。
   * 后端列目录时会顺手看一眼每个子目录里还有没有目录，这里就该用它 ——
   * 而不是像以前那样「没读过就先画上，点开才发现是空的」。
   */
  const rows = buildTreeRows({
    volumes: [volume("D:\\")],
    childrenOf: table({
      "D:\\": [dir("D:\\Leaf", "Leaf", false), dir("D:\\Branch", "Branch", true)],
    }),
    // 要看子行的箭头，得先把卷展开（折叠时子树整棵不出现）
    isExpanded: (path) => path === "D:\\",
  });
  const flags = Object.fromEntries(rows.map((row) => [row.name, row.expandable]));
  assert.equal(flags.Leaf, false, "清单说 Leaf 没有下一层 → 不给箭头");
  assert.equal(flags.Branch, true, "清单说 Branch 有下一层 → 给箭头");
});

test("多扫一层：真读过之后以实际子目录为准（清单过时也不能误导）", () => {
  // 清单说没有、实际读出来有 → 必须给箭头（数据比缓存新鲜）
  const rows = buildTreeRows({
    volumes: [volume("D:\\")],
    childrenOf: table({
      "D:\\": [dir("D:\\Changed", "Changed", false)],
      "D:\\Changed": [dir("D:\\Changed\\new", "new", false)],
    }),
    // 展开卷，子行才会出现
    isExpanded: (path) => path === "D:\\",
  });
  assert.equal(rows.find((row) => row.name === "Changed")?.expandable, true);
});

test("折叠的目录不生成子行（子树整棵不出现）", () => {
  const rows = buildTreeRows({
    volumes: [volume("D:\\")],
    childrenOf: table({
      "D:\\": [dir("D:\\Photos", "Photos")],
      "D:\\Photos": [dir("D:\\Photos\\2024", "2024")],
    }),
    isExpanded: (path) => path === "D:\\", // Photos 没展开
  });
  assert.deepEqual(
    rows.map((row) => row.name),
    ["D:\\", "Photos"],
  );
});

test("没读过的目录先乐观画箭头；读回来是空的就收掉", () => {
  // 一个字都没读：卷可展开
  const before = buildTreeRows({
    volumes: [volume("D:\\")],
    childrenOf: () => undefined,
    isExpanded: () => false,
  });
  assert.equal(before[0].expandable, true);

  // 读过了、确实是空的：不再画箭头
  const after = buildTreeRows({
    volumes: [volume("D:\\")],
    childrenOf: table({ "D:\\": [] }),
    isExpanded: () => false,
  });
  assert.equal(after[0].expandable, false);

  // 读过了、有内容：画箭头
  const withChildren = buildTreeRows({
    volumes: [volume("D:\\")],
    childrenOf: table({ "D:\\": [dir("D:\\a", "a")] }),
    isExpanded: () => false,
  });
  assert.equal(withChildren[0].expandable, true);
});

test("展开状态如实反映在行上（箭头方向由视图决定）", () => {
  const rows = buildTreeRows({
    volumes: [volume("D:\\")],
    childrenOf: table({ "D:\\": [dir("D:\\a", "a")] }),
    isExpanded: (path) => path === "D:\\",
  });
  assert.equal(rows[0].expanded, true);
  assert.equal(rows[1].expanded, false);
});

test("深度上限会拦住病态深的树（数据来自文件系统，不能无限递归）", () => {
  // 每一层都「已读且展开」，理论上可以无限往下
  const childrenOf = (path: string) => [dir(`${path}/x`, "x")];
  const rows = buildTreeRows({
    volumes: [volume("/root")],
    childrenOf,
    isExpanded: () => true,
    maxDepth: 4,
  });
  assert.equal(rows.length, 5, "根 + 4 层");
  assert.equal(rows[rows.length - 1].depth, 4);
});

test("maxDepth 传脏值（NaN / 负数）不会炸，退回默认", () => {
  const rows = buildTreeRows({
    volumes: [volume("/root")],
    childrenOf: (path) => [dir(`${path}/x`, "x")],
    isExpanded: () => true,
    maxDepth: Number.NaN,
  });
  assert.ok(rows.length > 5, "NaN 应当被忽略，不至于只出一层");

  const negative = buildTreeRows({
    volumes: [volume("/root")],
    childrenOf: (path) => [dir(`${path}/x`, "x")],
    isExpanded: () => true,
    maxDepth: -3,
  });
  assert.equal(negative.length, 1, "负数按 0 处理：只有根");
});

test("空输入是空列表，不是异常", () => {
  const rows = buildTreeRows({
    volumes: [],
    childrenOf: () => undefined,
    isExpanded: () => true,
  });
  assert.deepEqual(rows, []);
});

test("卷的显示名与路径一致（同一卷在不同平台都好认）", () => {
  assert.equal(volumeDisplayName("D:\\"), "D:\\");
  assert.equal(volumeDisplayName("/"), "/");
  assert.equal(volumeDisplayName("/mnt/nas"), "/mnt/nas");
  assert.equal(volumeDisplayName("/media/用户/U 盘"), "/media/用户/U 盘");
});

test("卷的类型带在行上（视图据此选图标）", () => {
  const rows = buildTreeRows({
    volumes: [
      volume("C:\\", "local"),
      volume("E:\\", "removable"),
      volume("//nas/photos", "network"),
    ],
    childrenOf: () => undefined,
    isExpanded: () => false,
  });
  assert.deepEqual(
    rows.map((row) => row.volumeKind),
    ["local", "removable", "network"],
  );
  // 子目录不再有卷类型
  const expanded = buildTreeRows({
    volumes: [volume("C:\\", "local")],
    childrenOf: () => [dir("C:\\a", "a")],
    isExpanded: () => true,
  });
  assert.equal(expanded[1].volumeKind, null);
});
