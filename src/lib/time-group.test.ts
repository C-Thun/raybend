/**
 * 按时间分组的单元测试（`DESIGN.md` §12.7）。
 *
 * 分组是「可一键选中的单位」，分错了会直接让用户把不该导入的照片导进来 ——
 * 而界面上看不出是分组错了还是数据错了。所以这里把边界钉死：
 * 阈值恰好等于间隔、跨天、跨时区、时间缺失、乱序输入、同一时刻多张。
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import { groupByTime, groupCount, sizeOf, type TimePhotoLike } from "./time-group.ts";

/** 东八区（测试用固定偏移，不受运行环境时区影响） */
const CST = 480;

function at(year: number, month: number, day: number, hour: number, minute = 0): number {
  // 东八区的墙上时间 → Unix 毫秒
  return Date.UTC(year, month - 1, day, hour, minute) - CST * 60_000;
}

function photo(id: string, takenAtMs: number | null): TimePhotoLike {
  return { id, takenAtMs };
}

test("空输入：没有天，也没有未知组", () => {
  const grouping = groupByTime([], { gapMinutes: 60, offsetMinutes: CST });
  assert.deepEqual(grouping.days, []);
  assert.equal(grouping.unknown, null);
  assert.equal(groupCount(grouping), 0);
});

test("单张照片：一天一片", () => {
  const grouping = groupByTime([photo("a", at(2026, 8, 15, 9))], {
    gapMinutes: 60,
    offsetMinutes: CST,
  });
  assert.equal(grouping.days.length, 1);
  assert.equal(grouping.days[0].id, "2026-08-15");
  assert.equal(grouping.days[0].slices.length, 1);
  assert.deepEqual(grouping.days[0].slices[0].photoIds, ["a"]);
  assert.equal(grouping.days[0].slices[0].startMs, grouping.days[0].slices[0].endMs);
  assert.equal(grouping.unknown, null);
});

test("同一天内：间隔不超过阈值算同一片", () => {
  const grouping = groupByTime(
    [
      photo("a", at(2026, 8, 15, 9, 0)),
      photo("b", at(2026, 8, 15, 9, 30)),
      photo("c", at(2026, 8, 15, 10, 0)),
    ],
    { gapMinutes: 60, offsetMinutes: CST },
  );
  const day = grouping.days[0];
  assert.equal(day.slices.length, 1);
  assert.deepEqual(day.slices[0].photoIds, ["a", "b", "c"]);
  assert.equal(day.slices[0].startMs, at(2026, 8, 15, 9, 0));
  assert.equal(day.slices[0].endMs, at(2026, 8, 15, 10, 0), "片标题要能显示首尾时间范围");
});

test("间隔恰好等于阈值：不切开（阈值是「>」才算断）", () => {
  const grouping = groupByTime(
    [photo("a", at(2026, 8, 15, 9, 0)), photo("b", at(2026, 8, 15, 10, 0))],
    { gapMinutes: 60, offsetMinutes: CST },
  );
  assert.equal(grouping.days[0].slices.length, 1);
});

test("间隔超过阈值：断为新片", () => {
  const grouping = groupByTime(
    [
      photo("a", at(2026, 8, 15, 9, 0)),
      photo("b", at(2026, 8, 15, 12, 1)),
      photo("c", at(2026, 8, 15, 12, 30)),
    ],
    { gapMinutes: 60, offsetMinutes: CST },
  );
  const slices = grouping.days[0].slices;
  assert.equal(slices.length, 2);
  assert.deepEqual(slices[0].photoIds, ["a"]);
  assert.deepEqual(slices[1].photoIds, ["b", "c"]);
  assert.equal(slices[0].id, "2026-08-15 #1");
  assert.equal(slices[1].id, "2026-08-15 #2");
});

test("跨天：一天一组，最近的在前", () => {
  const grouping = groupByTime(
    [
      photo("old", at(2026, 8, 14, 23, 30)),
      photo("new", at(2026, 8, 15, 0, 30)),
      photo("mid", at(2026, 8, 14, 10, 0)),
    ],
    { gapMinutes: 60, offsetMinutes: CST },
  );
  assert.deepEqual(
    grouping.days.map((day) => day.id),
    ["2026-08-15", "2026-08-14"],
  );
  // 23:30 与 00:30 只差 1 小时，但**跨天**，所以必然在不同片里
  assert.deepEqual(grouping.days[0].slices[0].photoIds, ["new"]);
  // 前一天的 10:00 与 23:30 差 13.5 小时 → 又是另一片
  assert.deepEqual(
    grouping.days[1].slices.map((slice) => slice.photoIds),
    [["mid"], ["old"]],
  );
  assert.equal(
    grouping.days[0].dayStartMs,
    at(2026, 8, 15, 0),
    "日组要给出当天 0 点（视图显示日期用）",
  );
});

test("拍摄时间缺失：归「未知时间」组放最后，且不参与切分", () => {
  const grouping = groupByTime(
    [
      photo("no-time-1", null),
      photo("timed", at(2026, 8, 15, 9)),
      photo("no-time-2", null),
    ],
    { gapMinutes: 60, offsetMinutes: CST },
  );
  assert.equal(grouping.days.length, 1);
  assert.deepEqual(grouping.days[0].photoIds, ["timed"]);
  assert.ok(grouping.unknown);
  assert.deepEqual(
    grouping.unknown?.photoIds.slice().sort(),
    ["no-time-1", "no-time-2"],
  );
  assert.equal(grouping.unknown?.startMs, 0, "未知组没有时间范围可显示");
  assert.equal(groupCount(grouping), 2);
  assert.equal(sizeOf(grouping.unknown!), 2);
});

test("阈值非法（0 / 负数 / NaN）：当天不切分", () => {
  const photos = [
    photo("a", at(2026, 8, 15, 1)),
    photo("b", at(2026, 8, 15, 20)),
  ];
  for (const gapMinutes of [0, -5, Number.NaN]) {
    const grouping = groupByTime(photos, { gapMinutes, offsetMinutes: CST });
    assert.equal(
      grouping.days[0].slices.length,
      1,
      `gapMinutes=${gapMinutes} 时不该切分`,
    );
  }
});

test("输入顺序不影响结果（内部自己排序）", () => {
  const photos = [
    photo("c", at(2026, 8, 15, 14)),
    photo("a", at(2026, 8, 15, 9)),
    photo("b", at(2026, 8, 15, 9, 30)),
  ];
  const first = groupByTime(photos, { gapMinutes: 60, offsetMinutes: CST });
  const shuffled = groupByTime([...photos].reverse(), {
    gapMinutes: 60,
    offsetMinutes: CST,
  });
  assert.deepEqual(first.days[0].slices[0].photoIds, ["a", "b"]);
  assert.deepEqual(
    first.days[0].photoIds,
    shuffled.days[0].photoIds,
    "换个顺序进来，分组结果必须一样",
  );
});

test("同一时刻多张：顺序确定（按 id 兜底），不会随机抖", () => {
  const same = at(2026, 8, 15, 9);
  const grouping = groupByTime(
    [photo("z", same), photo("a", same), photo("m", same)],
    { gapMinutes: 60, offsetMinutes: CST },
  );
  assert.deepEqual(grouping.days[0].slices[0].photoIds, ["a", "m", "z"]);
});

test("时区：偏移不同 → 「哪一天」也不同（同一时刻可以落在相邻两天）", () => {
  // 该时刻：UTC 2026-08-15 23:30 = 东八区 08-16 07:30 = 西五区 08-15 18:30
  const instant = Date.UTC(2026, 7, 15, 23, 30);
  const cst = groupByTime([photo("a", instant)], {
    gapMinutes: 60,
    offsetMinutes: 480,
  });
  const est = groupByTime([photo("a", instant)], {
    gapMinutes: 60,
    offsetMinutes: -300,
  });
  assert.equal(cst.days[0].id, "2026-08-16");
  assert.equal(est.days[0].id, "2026-08-15");
});

test("不传偏移：按运行环境的本地时区分组（不报错、结果自洽）", () => {
  const grouping = groupByTime(
    [photo("a", at(2026, 8, 15, 12))],
    { gapMinutes: 60 },
  );
  assert.equal(grouping.days.length, 1);
  assert.match(grouping.days[0].id, /^\d{4}-\d{2}-\d{2}$/);
});

test("片 id 稳定且唯一（选中与折叠都靠它）", () => {
  const grouping = groupByTime(
    [
      photo("a", at(2026, 8, 15, 9)),
      photo("b", at(2026, 8, 15, 20)),
      photo("c", at(2026, 8, 14, 9)),
    ],
    { gapMinutes: 60, offsetMinutes: CST },
  );
  const ids = grouping.days.flatMap((day) => day.slices.map((slice) => slice.id));
  assert.deepEqual(ids, ["2026-08-15 #1", "2026-08-15 #2", "2026-08-14 #1"]);
  assert.equal(new Set(ids).size, ids.length, "不能有重复的片 id");
});

test("几百张跨多天：结构与张数对得上（也顺带当性能冒烟）", () => {
  const photos: TimePhotoLike[] = [];
  for (let day = 1; day <= 5; day += 1) {
    for (let i = 0; i < 100; i += 1) {
      // 每天两段：上午 9 点与晚上 20 点
      const hour = i < 50 ? 9 : 20;
      photos.push(
        photo(`d${day}-${i}`, at(2026, 8, day, hour, i % 60)),
      );
    }
  }
  const grouping = groupByTime(photos, { gapMinutes: 60, offsetMinutes: CST });
  assert.equal(grouping.days.length, 5);
  const total = grouping.days.reduce((sum, day) => sum + day.photoIds.length, 0);
  assert.equal(total, 500, "一张都不能丢");
  for (const day of grouping.days) {
    // 每天两段（上午一段、晚上一段）—— 夜里 20 点那段跨小时但间隔 < 60 分钟
    assert.equal(day.slices.length, 2, `${day.id} 应当切成两段`);
  }
});
