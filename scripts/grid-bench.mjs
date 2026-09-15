#!/usr/bin/env node
/**
 * 网格管线的吞吐基准（`plans/M1-5.md` 步骤 14）。
 *
 * 量的是**滚动时每一帧都要重算的那几件事**：
 *   1. `groupByTime`（按时间模式下每次数据变化算一次）
 *   2. `buildGridRows`（行模型：换档位 / 换模式 / 数据变化时算）
 *   3. `computeVirtualWindow`（**每次滚动都要算**，所以它才是最关键的）
 *
 * 为什么用 Node 量而不是在浏览器里量：
 *   - WSLg 是软件渲染，帧率数字没有意义（`AGENTS.md` §2.8：体感归人类在真机确认）；
 *   - 但「算法本身会不会在几万张时炸掉」是纯逻辑问题，这里能量得清清楚楚，
 *     而且能进 CI（不像真机测试那样依赖环境）。
 *
 * 用法：
 *   node --experimental-strip-types scripts/grid-bench.mjs
 *   node --experimental-strip-types scripts/grid-bench.mjs 50000
 */

import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const HERE = dirname(fileURLToPath(import.meta.url));
const SRC = join(HERE, "..", "src");

const { groupByTime } = await import(join(SRC, "lib", "time-group.ts"));
const { computeVirtualWindow } = await import(join(SRC, "lib", "virtual-window.ts"));
const { buildGridRows } = await import(join(SRC, "features", "photo-grid", "rows.ts"));

const counts = process.argv.slice(2).map(Number).filter(Number.isFinite);
const SIZES = counts.length > 0 ? counts : [300, 3000, 30000];

/** 造一批「像真照片」的数据：跨多天、每天多段、每段几十张 */
function makeItems(count) {
  const items = [];
  const base = Date.UTC(2026, 7, 15, 1, 0);
  let taken = base;
  for (let i = 0; i < count; i += 1) {
    // 每 12 张换一段时间（模拟「一会儿拍一会儿停」），每 120 张过一天
    if (i % 12 === 0) taken += 40 * 60_000;
    if (i % 120 === 0) taken += 8 * 60 * 60_000;
    items.push({
      path: `/src/photo-${i}.jpg`,
      fileName: `photo-${i}.jpg`,
      ext: "jpg",
      kind: "image",
      sizeBytes: 1_000_000,
      mtimeMs: null,
      takenAtMs: taken,
      takenAtSource: "exif",
      takenAtOffsetMin: 480,
    });
  }
  return items;
}

function ms(fn) {
  const started = process.hrtime.bigint();
  const value = fn();
  return [Number(process.hrtime.bigint() - started) / 1e6, value];
}

function pad(value, width) {
  return String(Math.round(value * 100) / 100).padStart(width);
}

console.log("网格管线基准（Node，与真机渲染无关）\n");
console.log("张数 | 分组 ms | 行模型 ms | 行数 | 窗口 µs/次 | 窗口 1000 次 ms");

for (const size of SIZES) {
  const items = makeItems(size);

  const [groupMs, grouping] = ms(() =>
    groupByTime(
      items.map((item) => ({
        id: item.path,
        takenAtMs: item.takenAtMs,
        offsetMinutes: item.takenAtOffsetMin,
      })),
      { gapMinutes: 60 },
    ),
  );

  const [rowsMs, rows] = ms(() =>
    buildGridRows({
      items,
      columns: 6,
      cellWidth: 208,
      captionHeight: 26,
      grouping,
    }),
  );

  // 模拟连续滚动：每次滚动位置前进一屏
  const [windowMs, sample] = ms(() => {
    let last = { startIndex: 0, endIndex: 0, paddingTop: 0, paddingBottom: 0, totalHeight: 0 };
    for (let i = 0; i < 1000; i += 1) {
      last = computeVirtualWindow({
        rows,
        viewportHeight: 900,
        scrollTop: (i * 900) % Math.max(1, rows.length * 250),
        overscan: 2,
      });
    }
    return last;
  });

  console.log(
    [
      String(size).padStart(4),
      pad(groupMs, 7),
      pad(rowsMs, 9),
      String(rows.length).padStart(4),
      pad((windowMs / 1000) * 1000, 10),
      pad(windowMs, 15),
    ].join(" | "),
  );
  console.log(
    `     窗口采样：start=${sample.startIndex} end=${sample.endIndex} 总高=${sample.totalHeight}px（最后一行窗口）`,
  );
}

console.log(
  "\n判读口径：窗口计算要远低于 16ms（60fps 的一帧）；行模型与分组只在数据变化时算一次。",
);
