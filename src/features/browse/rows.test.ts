/**
 * 浏览网格行模型与时间分组的测试。
 *
 * 两组重点：
 *
 * 1. **分组口径必须与导入网格一致**（`lib/time-group.ts`）—— 最后一条用例拿同样的输入
 *    去比 `groupByTime` 的边界，口径一漂就红；
 * 2. **行模型的边界**：最后一行不满、空列表、分组标题行的高度与计数。
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import { groupByTime } from "../../lib/time-group.ts";
import {
  buildBrowseRows,
  sliceOrder,
  browseGroups,
  DEFAULT_GAP_MINUTES,
  GROUP_ROW_HEIGHT,
} from "./rows.ts";

/**
 * 「未知时间」在测试里用的**占位标签**（生产实现里它是 `unknown: true` + 空 label，
 * 真的标题由视图按当前语言渲染）。跨口径对比时两边都用它对齐。
 */
const UNKNOWN_KEY = "<unknown>";

/** 东八区 2026-08-15 的某个时刻（UTC 毫秒）。 */
function at(hour: number, minute = 0, day = 15): number {
  return Date.UTC(2026, 7, day, hour, minute) - 480 * 60_000;
}

// ─────────────────────────── 行模型 ───────────────────────────

test("平铺：按列数切行，最后一行可以不满", () => {
  const rows = buildBrowseRows({ total: 7, columns: 3, cellSize: 100 });
  assert.equal(rows.length, 3);
  assert.deepEqual(
    rows.map((r) => (r.kind === "tiles" ? [r.start, r.count] : null)),
    [
      [0, 3],
      [3, 3],
      [6, 1],
    ],
  );
});

test("空列表 → 空行数组（视图显示空态）", () => {
  assert.deepEqual(buildBrowseRows({ total: 0, columns: 4, cellSize: 100 }), []);
});

test("列数为 0 或非法时按一列处理（不做除零）", () => {
  const rows = buildBrowseRows({ total: 3, columns: 0, cellSize: 50 });
  assert.equal(rows.length, 3);
  assert.ok(rows.every((r) => r.kind === "tiles" && r.count === 1));
});

test("行高 = 格子边长 + 行距（信息条是覆盖层，不占高度）", () => {
  const rows = buildBrowseRows({ total: 3, columns: 3, cellSize: 200, gap: 6 });
  assert.equal(rows[0].height, 206);
});

test("分组模式：每组一个标题行，标题计数等于该组张数", () => {
  const rows = buildBrowseRows({
    total: 4,
    columns: 2,
    cellSize: 100,
    groups: [
      { start: 0, count: 3, label: "2026-08-15", unknown: false },
      { start: 3, count: 1, label: "", unknown: true },
    ],
  });
  const groups = rows.filter((r) => r.kind === "group");
  assert.equal(groups.length, 2);
  assert.equal(groups[0].height, GROUP_ROW_HEIGHT);
  assert.equal(groups[0].kind === "group" ? groups[0].count : 0, 3);
  assert.equal(groups[1].kind === "group" ? groups[1].unknown : false, true);
  assert.equal(
    groups[1].kind === "group" ? groups[1].label : "x",
    "",
    "未知时间组的 label 是空串 —— 标题由视图按语言渲染",
  );
  // 标题行之后跟着该组的 tile 行
  assert.equal(rows[1].kind, "tiles");
  assert.equal(rows[1].kind === "tiles" ? rows[1].start : -1, 0);
});

test("分组越界或空组不会产出行", () => {
  const rows = buildBrowseRows({
    total: 2,
    columns: 2,
    cellSize: 100,
    groups: [
      { start: 0, count: 0, label: "空组", unknown: false },
      { start: 10, count: 5, label: "越界", unknown: false },
      { start: 0, count: 2, label: "正常", unknown: false },
    ],
  });
  assert.equal(rows.filter((r) => r.kind === "group").length, 1);
});

// ─────────────────────────── 时间分组 ───────────────────────────

test("跨天必断", () => {
  const groups = browseGroups([
    { takenAt: at(23) },
    { takenAt: at(9, 0, 15) },
    { takenAt: at(1, 0, 16) },
  ]);
  assert.deepEqual(
    groups.map((g) => g.label),
    ["2026-08-15", "2026-08-15", "2026-08-16"],
  );
  assert.deepEqual(
    groups.map((g) => [g.start, g.count]),
    [
      [0, 1],
      [1, 1],
      [2, 1],
    ],
  );
});

test("同一天内间隔超过阈值断片，未超过连成一片", () => {
  const groups = browseGroups([
    { takenAt: at(10, 0) },
    { takenAt: at(10, 30) }, // 差 30 分钟 → 同片
    { takenAt: at(12, 30) }, // 与上一张差 2 小时 → 断
  ]);
  assert.equal(groups.length, 2);
  assert.equal(groups[0].count, 2);
  assert.equal(groups[1].start, 2);
});

test("阈值可以调；非法阈值 = 只按天断", () => {
  const photos = [{ takenAt: at(9) }, { takenAt: at(17) }];
  assert.equal(browseGroups(photos, 0).length, 1, "0 → 不按间隔切");
  assert.equal(browseGroups(photos, -5).length, 1);
  assert.equal(browseGroups(photos, DEFAULT_GAP_MINUTES).length, 2, "默认 60 分钟会断");
});

test("没有拍摄时间的归到最后一组「未知时间」", () => {
  const groups = browseGroups([
    { takenAt: at(10) },
    { takenAt: null },
    { takenAt: null },
  ]);
  assert.equal(groups.length, 2);
  assert.equal(groups[1].unknown, true);
  assert.equal(groups[1].count, 2);
});

test("空输入 → 没有分组", () => {
  assert.deepEqual(browseGroups([]), []);
});

test("时区按照片自带的偏移算（不在本机时区上猜）", () => {
  // 同一时刻：东八区看是 15 日 08:00，UTC 看是 15 日 00:00 —— 都还是 15 日
  const ms = at(8);
  assert.equal(browseGroups([{ takenAt: ms, takenAtOffsetMin: 480 }])[0].label, "2026-08-15");
  // 但按 UTC（偏移 0）看：15 日 00:00 之前一刻属于 14 日
  const beforeMidnightUtc = Date.UTC(2026, 7, 14, 23, 0);
  assert.equal(
    browseGroups([{ takenAt: beforeMidnightUtc, takenAtOffsetMin: 0 }])[0].label,
    "2026-08-14",
  );
  assert.equal(
    browseGroups([{ takenAt: beforeMidnightUtc, takenAtOffsetMin: 480 }])[0].label,
    "2026-08-15",
    "东八区看它已经是 15 日 07:00",
  );
});

// ─────────────────────────── 与导入网格的口径交叉校验 ───────────────────────────

test("分组划分与 lib/time-group 的 days/slices 一致", () => {
  /*
   * 比的是**同一个划分**，不是遍历顺序 —— 两者的顺序口径本就不同，而且必须不同：
   *   * `browseGroups` **保持列表顺序**（网格按当前排序显示，可能是升序也可能是降序）；
   *   * `groupByTime` 固定「按天倒序 + 片内升序」（照片流的方向）。
   * 所以这里把两组 (标签, 张数) 排序后再比。
   */
  // 构造跨 3 天、含间隔断片与「无时间」的输入
  const entries = [
    { takenAt: at(20), takenAtOffsetMin: 480 },
    { takenAt: at(20, 30), takenAtOffsetMin: 480 },
    { takenAt: at(9, 0, 16), takenAtOffsetMin: 480 },
    { takenAt: at(9, 10, 16), takenAtOffsetMin: 480 },
    { takenAt: at(14, 0, 16), takenAtOffsetMin: 480 }, // 与上一张差 ≈5 小时 → 断
    { takenAt: at(11, 0, 17), takenAtOffsetMin: 480 },
    { takenAt: null },
  ];

  const mine = browseGroups(entries);

  // 用同一份输入跑「导入网格那一套」
  const grouping = groupByTime(
    entries.map((e, i) => ({ id: String(i), takenAtMs: e.takenAt, offsetMinutes: 480 })),
    { gapMinutes: DEFAULT_GAP_MINUTES, offsetMinutes: 480 },
  );
  const theirs: Array<{ label: string; count: number }> = [];
  for (const day of grouping.days) {
    for (const slice of day.slices) {
      theirs.push({ label: day.id, count: slice.photoIds.length });
    }
  }
  if (grouping.unknown) {
    theirs.push({ label: UNKNOWN_KEY, count: grouping.unknown.photoIds.length });
  }

  const canonical = (
    list: readonly { label: string; count: number }[],
  ): { label: string; count: number }[] =>
    [...list].sort((a, b) => a.label.localeCompare(b.label) || a.count - b.count);

  assert.deepEqual(
    canonical(
      mine.map((g) => ({ label: g.unknown ? UNKNOWN_KEY : g.label, count: g.count })),
    ),
    canonical(theirs),
    "分组口径必须与 lib/time-group 一致（漂了就说明两处规则分家了）",
  );
});

// ──────────────── 片内按文件名自然序（人类 2026-09-17 定的口径）────────────────

test("sliceOrder：同一片内按文件名自然排，JPG 与 RAW 挨着", () => {
  // 时间线是**后端的时间序**：RAW 因为拍摄时间来源不同（mtime 兜底）落在后面
  const timeline = [
    { relPath: "photos/P1000019.JPG" },
    { relPath: "photos/P1000020.JPG" },
    { relPath: "photos/P1000019.RW2" },
    { relPath: "photos/P1000020.RW2" },
  ];
  const groups = [{ start: 0, count: 4, label: "2026-09-13", unknown: false }];
  const order = sliceOrder(timeline, groups);
  assert.deepEqual(
    order.map((index) => timeline[index]?.relPath),
    [
      "photos/P1000019.JPG",
      "photos/P1000019.RW2",
      "photos/P1000020.JPG",
      "photos/P1000020.RW2",
    ],
  );
});

test("sliceOrder：片与片的先后、每片的边界都不动，只有片内换位", () => {
  const timeline = [{ relPath: "b.JPG" }, { relPath: "a.JPG" }, { relPath: "d.JPG" }, { relPath: "c.JPG" }];
  const groups = [
    { start: 0, count: 2, label: "2026-09-13", unknown: false },
    { start: 2, count: 2, label: "2026-09-12", unknown: false },
  ];
  const order = sliceOrder(timeline, groups);
  // 仍是一个置换，且每一片只在自己那段区间里换位
  assert.deepEqual([...order].sort((a, b) => a - b), [0, 1, 2, 3]);
  assert.deepEqual(order.slice(0, 2).sort((a, b) => a - b), [0, 1]);
  assert.deepEqual(order.slice(2, 4).sort((a, b) => a - b), [2, 3]);
  assert.deepEqual(order.map((i) => timeline[i]?.relPath), ["a.JPG", "b.JPG", "c.JPG", "d.JPG"]);
});

test("sliceOrder：未知时间组也按名字排；空输入、缺字段、越界区间都不许炸", () => {
  assert.deepEqual(sliceOrder([], []), []);
  const timeline = [{ relPath: "z.JPG" }, { relPath: "a.JPG" }];
  const unknownGroup = [{ start: 0, count: 2, label: "", unknown: true }];
  assert.deepEqual(
    sliceOrder(timeline, unknownGroup).map((i) => timeline[i]?.relPath),
    ["a.JPG", "z.JPG"],
  );
  // 没有 relPath（防御老数据）→ 退化成保持原顺序
  assert.deepEqual(sliceOrder([{}, {}], unknownGroup), [0, 1]);
  // 组区间越界：只重排交叠的那部分，且不越界写
  assert.deepEqual(sliceOrder(timeline, [{ start: 1, count: 99, label: "", unknown: false }]), [0, 1]);
});
