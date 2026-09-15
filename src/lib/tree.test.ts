/**
 * 目录树行模型 + 路径身份键的单元测试（DESIGN.md §12.4.1、AGENTS.md §7.3）。
 *
 * 覆盖重点：空树 / 单节点 / 折叠 / 深层 / 环形引用 / 非法深度 /
 * 路径的大小写、分隔符、尾斜杠、Unicode 规范化（NFC vs NFD）。
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import {
  flattenVisible,
  indentPx,
  pathKey,
  samePath,
  type TreeLike,
} from "./tree.ts";

interface Node extends TreeLike<Node> {
  id: string;
}

const leaf = (id: string): Node => ({ id });
const branch = (id: string, ...children: Node[]): Node => ({ id, children });

// ─── flattenVisible ──────────────────────────────────────

test("空树 → 空行", () => {
  assert.deepEqual(flattenVisible([], () => true), []);
});

test("单节点：深度 0、无子节点", () => {
  const rows = flattenVisible([leaf("a")], () => true);
  assert.equal(rows.length, 1);
  assert.deepEqual(rows[0], { node: { id: "a" }, depth: 0, hasChildren: false });
});

test("展开的父节点：子节点紧随其后且深度 +1", () => {
  const tree = [branch("a", leaf("a1"), leaf("a2")), leaf("b")];
  const rows = flattenVisible(tree, () => true);
  assert.deepEqual(
    rows.map((r) => [r.node.id, r.depth]),
    [
      ["a", 0],
      ["a1", 1],
      ["a2", 1],
      ["b", 0],
    ],
  );
});

test("折叠的父节点：子树整棵不出现（不是隐藏，是不存在）", () => {
  const tree = [branch("a", leaf("a1"), leaf("a2"))];
  const rows = flattenVisible(tree, () => false);
  assert.deepEqual(
    rows.map((r) => r.node.id),
    ["a"],
  );
  assert.equal(rows[0].hasChildren, true, "折叠不影响 hasChildren（箭头仍要画）");
});

test("展开与否只取决于 isExpanded —— 别的状态传进来也不该影响行集", () => {
  const tree = [branch("a", leaf("a1"))];
  // 模拟「选中了 a」但没展开：行集不变（禁止为显示选中而展开）
  const selected = new Set(["a"]);
  const rows = flattenVisible(tree, () => false, {
    childrenOf: (n) => n.children,
  });
  assert.equal(selected.has("a"), true);
  assert.equal(rows.length, 1);
});

test("深层嵌套：深度如实累加", () => {
  let node: Node = leaf("d4");
  for (let i = 3; i >= 0; i -= 1) node = branch(`d${i}`, node);
  const rows = flattenVisible([node], () => true);
  assert.deepEqual(
    rows.map((r) => r.depth),
    [0, 1, 2, 3, 4],
  );
});

test("空 children 数组视为叶子（不画展开箭头）", () => {
  const rows = flattenVisible([{ id: "a", children: [] }], () => true);
  assert.equal(rows[0].hasChildren, false);
});

test("环形引用不会无限递归：到 maxDepth 就停", () => {
  const a: Node = { id: "a" };
  (a as Node).children = [a];
  const rows = flattenVisible([a], () => true, { maxDepth: 5 });
  assert.equal(rows.length, 6, "深度 0..5 共 6 行");
});

test("maxDepth 非法（负数 / NaN）时回退到默认上限，而不是抛错或全摊开", () => {
  for (const bad of [-1, Number.NaN, Number.POSITIVE_INFINITY]) {
    const rows = flattenVisible([branch("a", leaf("a1"))], () => true, {
      maxDepth: bad,
    });
    assert.deepEqual(
      rows.map((r) => r.node.id),
      ["a", "a1"],
    );
  }
});

test("childrenOf 覆盖默认取子方式（兼容没有 children 字段的数据形状）", () => {
  interface Odd {
    key: string;
    sub?: Odd[];
  }
  const tree: Odd[] = [{ key: "a", sub: [{ key: "a1" }] }];
  const rows = flattenVisible(tree, () => true, { childrenOf: (n) => n.sub });
  assert.deepEqual(
    rows.map((r) => r.node.key),
    ["a", "a1"],
  );
});

// ─── indentPx ────────────────────────────────────────────

test("缩进：深度 × 单位", () => {
  assert.equal(indentPx(0, 10), 0);
  assert.equal(indentPx(3, 10), 30);
  assert.equal(indentPx(2, 16), 32);
});

test("缩进：非法输入不产出 NaN / 负数", () => {
  for (const [d, u] of [
    [Number.NaN, 10],
    [2, Number.NaN],
    [-3, 10],
    [2.7, 10.9],
    [Number.POSITIVE_INFINITY, 10],
  ] as const) {
    const px = indentPx(d, u);
    assert.ok(Number.isFinite(px), `indentPx(${d}, ${u}) 不应是 ${px}`);
    assert.ok(px >= 0, "缩进不应为负");
  }
  assert.equal(indentPx(2.7, 10.9), 21.8, "深度取整（2），单位是可带小数的长度，原样保留");
});

// ─── pathKey / samePath ──────────────────────────────────

test("路径键：大小写折叠（Windows / macOS 语义）", () => {
  assert.equal(pathKey("D:\\Photos"), pathKey("d:\\photos"));
  assert.equal(samePath("D:\\Photos", "d:/photos"), true);
});

test("路径键：可以关掉大小写折叠（Linux 语义 —— /A 与 /a 是两个目录）", () => {
  assert.equal(samePath("/mnt/A", "/mnt/a", { caseFold: false }), false);
  assert.equal(samePath("/mnt/A", "/mnt/A", { caseFold: false }), true);
});

test("路径键：两种分隔符等价", () => {
  assert.equal(pathKey("D:\\A\\B\\C"), pathKey("D:/A/B/C"));
  assert.equal(pathKey("\\\\server\\share\\dir"), pathKey("//server/share/dir"));
});

test("路径键：尾部分隔符与重复分隔符不影响身份", () => {
  assert.equal(pathKey("D:\\Photos\\"), pathKey("D:\\Photos"));
  assert.equal(pathKey("D:\\\\Photos"), pathKey("D:\\Photos"));
  assert.equal(pathKey("/a//b///c/"), pathKey("/a/b/c"));
});

test("路径键：驱动器根与 POSIX 根不会塌成空串", () => {
  assert.equal(pathKey("D:\\"), "d:");
  assert.equal(pathKey("/"), "/");
  assert.equal(pathKey("//"), "//");
  assert.equal(pathKey("///"), "//", "多余的前导斜杠合并成一个 UNC 前导");
});

test("路径键：Unicode 规范化（macOS 的 NFD 与 Windows 的 NFC 视为同一路径）", () => {
  const nfc = "/Users/me/Pictures/caf\u00e9"; // é 单码点
  const nfd = "/Users/me/Pictures/cafe\u0301"; // e + 组合重音
  assert.notEqual(nfc, nfd, "原始字符串确实不同（否则这条测试没意义）");
  assert.equal(pathKey(nfc, { caseFold: false }), pathKey(nfd, { caseFold: false }));
});

test("路径键：中文与特殊字符原样保留", () => {
  assert.equal(pathKey("D:\\照片\\2024 秋\\北京"), "d:/照片/2024 秋/北京".toLowerCase());
  assert.equal(samePath("D:\\照片", "d:/照片"), true);
});

test("路径键：空串与非法输入 → 空键，且 two null 相等", () => {
  assert.equal(pathKey(""), "");
  assert.equal(samePath("", ""), true);
  assert.equal(samePath(null, null), true);
  assert.equal(samePath(undefined, ""), false, "null 与空串不是同一件事");
  assert.equal(samePath("", "D:\\a"), false);
});

test("samePath：一边为空 → 不相等（避免「没选中」被当成选中了某个目录）", () => {
  assert.equal(samePath(null, "D:\\a"), false);
  assert.equal(samePath("D:\\a", undefined), false);
});
