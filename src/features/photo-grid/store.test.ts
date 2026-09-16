/**
 * 照片网格 store 的测试。
 *
 * 覆盖三类真会咬人的地方：
 *   1. **换目录要清干净**（选择 / 排除 / 缩略图），以及**迟到的结果要丢掉**；
 *   2. **按时间**：读真相 EXIF 后才分组，且显示顺序跟着分组走；
 *   3. **选择与排除**：区间、全选一组、批量排除的**反转**语义。
 *
 * 设置读写也在这里钉住（档位、按时间、时间片阈值）。
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import type { SourceItem, SourceScan, TimeEntry } from "../../api/types.ts";
import { itemId } from "./rows.ts";
import {
  createPhotoGridStore,
  DEFAULT_GAP_MINUTES,
  type PhotoGridApi,
} from "./store.ts";

const CST = 480;

function item(name: string, takenAtMs: number | null = null): SourceItem {
  return {
    path: `/src/${name}`,
    fileName: name,
    ext: "jpg",
    kind: "image",
    sizeBytes: 10,
    mtimeMs: null,
    takenAtMs,
    takenAtSource: takenAtMs === null ? null : "filename",
    takenAtOffsetMin: takenAtMs === null ? null : CST,
  };
}

function at(hour: number, minute = 0, day = 15): number {
  return Date.UTC(2026, 7, day, hour, minute) - CST * 60_000;
}

function scanOf(items: SourceItem[]): SourceScan {
  return { root: "/src", items, skipped: 0, problems: [], elapsedMs: 1 };
}

interface FakeState {
  items: SourceItem[];
  times: Map<string, TimeEntry>;
  settings: Map<string, string>;
  calls: string[];
  holdScan: boolean;
  failScan: boolean;
  scanRoots: string[];
}

function fakeApi(overrides: Partial<FakeState> = {}) {
  const state: FakeState = {
    items: [],
    times: new Map(),
    settings: new Map(),
    calls: [],
    holdScan: false,
    failScan: false,
    scanRoots: [],
    ...overrides,
  };
  const pending: Array<() => void> = [];

  const api: PhotoGridApi = {
    async scanSourceDir(path) {
      state.calls.push(`scan:${path}`);
      state.scanRoots.push(path);
      if (state.holdScan) {
        await new Promise<void>((resolve) => pending.push(resolve));
      }
      if (state.failScan) throw new Error(`读不了 ${path}`);
      return scanOf(state.items);
    },
    async readSourceTimes(paths) {
      state.calls.push(`times:${paths.length}`);
      return paths.map((path) => {
        const explicit = state.times.get(path);
        if (explicit) return explicit;
        // 真实实现是「EXIF → 文件名 → mtime」，绝不会把已知的时间抹成空；
        // 这里的替身也照这个口径：没显式指定就回显该条自己的值
        const source = state.items.find((entry) => itemId(entry) === path);
        return {
          path,
          takenAtMs: source?.takenAtMs ?? null,
          takenAtSource: "exif",
          takenAtOffsetMin: source?.takenAtOffsetMin ?? null,
        };
      });
    },
    async getThumbBytes(path) {
      state.calls.push(`thumb:${path}`);
      return new Uint8Array([1, 2, 3]);
    },
    async getSetting(key) {
      return state.settings.get(key) ?? null;
    },
    async setSetting(key, value) {
      state.settings.set(key, value);
    },
  };

  return {
    api,
    state,
    release() {
      const waiters = pending.splice(0, pending.length);
      for (const resolve of waiters) resolve();
    },
  };
}

async function flush(times = 3): Promise<void> {
  for (let i = 0; i < times; i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
}

/* ══════════════════════════════════════════════════════════════
 * 数据加载
 * ══════════════════════════════════════════════════════════════ */

test("切目录：加载出清单，状态走 loading → ready", async () => {
  const { api, state } = fakeApi();
  state.items = [item("a.jpg"), item("b.jpg")];
  const store = createPhotoGridStore({ api });

  assert.equal(store.status(), "idle");
  store.setSourceDir("/src");
  assert.equal(store.status(), "loading");
  await flush();

  assert.equal(store.status(), "ready");
  assert.equal(store.dir(), "/src");
  assert.deepEqual(
    store.items().map((entry) => entry.fileName),
    ["a.jpg", "b.jpg"],
  );
  assert.equal(store.error(), null);
});

test("同一个目录重复设置是空操作（不重新扫描）", async () => {
  const { api, state } = fakeApi();
  const store = createPhotoGridStore({ api });
  store.setSourceDir("/src");
  await flush();
  store.setSourceDir("/src");
  await flush();
  assert.equal(state.calls.filter((call) => call.startsWith("scan:")).length, 1);
});

test("换目录：选择与排除都被清空", async () => {
  const { api, state } = fakeApi();
  state.items = [item("a.jpg"), item("b.jpg")];
  const store = createPhotoGridStore({ api });
  store.setSourceDir("/src");
  await flush();

  store.clickItem(itemId(state.items[0]), "replace");
  store.toggleExcludedSelected();
  assert.equal(store.selectedCount(), 1);
  assert.equal(store.excludedCount(), 1);

  store.setSourceDir("/other");
  await flush();
  assert.equal(store.selectedCount(), 0);
  assert.equal(store.excludedCount(), 0, "上一个目录的排除不能带过来");
});

test("迟到的扫描结果被丢掉（用户已经换了目录）", async () => {
  const { api, state } = fakeApi();
  state.holdScan = true;
  const store = createPhotoGridStore({ api });

  store.setSourceDir("/first");
  await flush(1);
  state.items = [item("second.jpg")];
  state.holdScan = false;
  store.setSourceDir("/second");
  await flush();

  assert.equal(store.dir(), "/second");
  assert.deepEqual(
    store.items().map((entry) => entry.fileName),
    ["second.jpg"],
    "旧目录的结果不能覆盖新列表",
  );
  assert.equal(store.status(), "ready");
});

test("扫描失败：状态是 error 且带上原因", async () => {
  const { api } = fakeApi({ failScan: true });
  const store = createPhotoGridStore({ api });
  store.setSourceDir("/broken");
  await flush();
  assert.equal(store.status(), "error");
  assert.match(store.error() ?? "", /读不了 \/broken/);
});

test("清空目录（null）：回到 idle 并清掉列表", async () => {
  const { api, state } = fakeApi();
  state.items = [item("a.jpg")];
  const store = createPhotoGridStore({ api });
  store.setSourceDir("/src");
  await flush();

  store.setSourceDir(null);
  assert.equal(store.dir(), null);
  assert.equal(store.status(), "idle");
  assert.deepEqual(store.items(), []);
});

/* ══════════════════════════════════════════════════════════════
 * 按时间
 * ══════════════════════════════════════════════════════════════ */

test("按时间：只对「还不是 exif」的那些补读真相，并就地更新", async () => {
  const { api, state } = fakeApi();
  state.items = [
    // a.jpg 已经是 exif 来源 → 不该被重读
    { ...item("a.jpg", at(9, 0)), takenAtSource: "exif" },
    item("b.jpg", null),
  ];
  state.times.set("/src/b.jpg", {
    path: "/src/b.jpg",
    takenAtMs: at(20, 0),
    takenAtSource: "exif",
    takenAtOffsetMin: CST,
  });
  const store = createPhotoGridStore({ api });
  store.setSourceDir("/src");
  await flush();

  store.setByTime(true);
  await flush();

  assert.equal(store.loadingTimes(), false);
  assert.ok(store.grouping(), "读到时间后应该能分组");
  const updated = store.items().find((entry) => entry.fileName === "b.jpg");
  assert.equal(updated?.takenAtSource, "exif");
  assert.equal(updated?.takenAtMs, at(20, 0));
  assert.ok(
    state.calls.includes("times:1"),
    "只补读那一张（a.jpg 已经是文件名来源，不需要重读）",
  );
});

test("按时间：显示顺序跟着分组走（日倒序 → 片升序）", async () => {
  const { api, state } = fakeApi();
  state.items = [item("old.jpg", at(9, 0, 14)), item("new.jpg", at(9, 0, 15))];
  const store = createPhotoGridStore({ api });
  store.setSourceDir("/src");
  await flush();
  store.setByTime(true);
  await flush();

  assert.deepEqual(
    store.displayItems().map((entry) => entry.fileName),
    ["new.jpg", "old.jpg"],
    "最近的日期在前",
  );
  assert.deepEqual(
    store.items().map((entry) => entry.fileName),
    ["old.jpg", "new.jpg"],
    "原始清单顺序不变（显示顺序是另一回事）",
  );
});

test("按时间：分组按每张自己的时区偏移算（没有偏移的按 UTC）", async () => {
  const { api, state } = fakeApi();
  // 同一时刻：UTC 23:30 → 东八区是次日 07:30
  const instant = Date.UTC(2026, 7, 15, 23, 30);
  state.items = [
    { ...item("cst.jpg", instant), takenAtOffsetMin: 480 },
    { ...item("utc.jpg", instant), takenAtOffsetMin: null },
  ];
  const store = createPhotoGridStore({ api });
  store.setSourceDir("/src");
  await flush();
  store.setByTime(true);
  await flush();

  const days = store.grouping()?.days.map((day) => day.id);
  assert.deepEqual(
    days?.slice().sort(),
    ["2026-08-15", "2026-08-16"],
    "同一毫秒，偏移不同 → 「哪一天」也不同",
  );
});

test("时间片阈值来自设置（hydrate 读回）", async () => {
  const { api, state } = fakeApi();
  state.settings.set("grid.time_gap_minutes", "5");
  state.items = [item("a.jpg", at(9, 0)), item("b.jpg", at(9, 30))];
  const store = createPhotoGridStore({ api });
  await store.hydrate();
  assert.equal(store.gapMinutes(), 5);

  store.setSourceDir("/src");
  await flush();
  store.setByTime(true);
  await flush();

  assert.equal(
    store.grouping()?.days[0].slices.length,
    2,
    "阈值 5 分钟 → 30 分钟的间隔要断成两片",
  );
});

test("按时间关掉：分组消失，回到平铺顺序", async () => {
  const { api, state } = fakeApi();
  state.items = [item("old.jpg", at(9, 0, 14)), item("new.jpg", at(9, 0, 15))];
  const store = createPhotoGridStore({ api });
  store.setSourceDir("/src");
  await flush();
  store.setByTime(true);
  await flush();
  store.setByTime(false);
  assert.equal(store.grouping(), undefined);
  assert.deepEqual(
    store.displayItems().map((entry) => entry.fileName),
    ["old.jpg", "new.jpg"],
  );
});

/* ══════════════════════════════════════════════════════════════
 * 选择与排除
 * ══════════════════════════════════════════════════════════════ */

test("选择：单点 / Ctrl 增减 / Shift 区间 / 全选", async () => {
  const { api, state } = fakeApi();
  state.items = [item("a.jpg"), item("b.jpg"), item("c.jpg"), item("d.jpg")];
  const store = createPhotoGridStore({ api });
  store.setSourceDir("/src");
  await flush();
  const ids = store.items().map(itemId);

  store.clickItem(ids[1], "replace");
  assert.equal(store.selectedCount(), 1);

  store.clickItem(ids[3], "range");
  assert.deepEqual([...store.selectedIds()].sort(), [ids[1], ids[2], ids[3]].sort());

  store.clickItem(ids[0], "toggle");
  assert.equal(store.selectedCount(), 4);

  store.clearSelection();
  assert.equal(store.hasSelection(), false);

  store.selectAll();
  assert.equal(store.selectedCount(), 4);
});

test("全选一组：日 / 时间片的「全选当天」「全选此段」", async () => {
  const { api, state } = fakeApi();
  state.items = [item("a.jpg"), item("b.jpg"), item("c.jpg")];
  const store = createPhotoGridStore({ api });
  store.setSourceDir("/src");
  await flush();
  const ids = store.items().map(itemId);

  store.selectGroup([ids[0], ids[1]]);
  assert.equal(store.selectedCount(), 2);
  store.selectGroup([ids[2]]);
  assert.equal(store.selectedCount(), 3, "默认是并集（可以连着选几段）");

  store.selectGroup([ids[2]], false);
  assert.deepEqual([...store.selectedIds()], [ids[2]], "替换式全选");

  store.selectGroup([]);
  assert.equal(store.selectedCount(), 1, "空组不动选择");
});

test("批量排除：只反转**选中项**，且是可逆的", async () => {
  const { api, state } = fakeApi();
  state.items = [item("a.jpg"), item("b.jpg"), item("c.jpg")];
  const store = createPhotoGridStore({ api });
  store.setSourceDir("/src");
  await flush();
  const ids = store.items().map(itemId);

  store.clickItem(ids[0], "replace");
  store.toggleExcludedSelected();
  assert.deepEqual([...store.excluded()], [ids[0]]);
  assert.equal(store.excludedCount(), 1);

  // 换一张再排除：原来的还在
  store.clickItem(ids[1], "replace");
  store.toggleExcludedSelected();
  assert.equal(store.excludedCount(), 2);

  // 把两张都选上再反转一次：都恢复
  store.selectAll();
  store.toggleExcludedSelected();
  assert.equal(store.excludedCount(), 1, "第二张恢复，第一张未选中所以不动");

  // 没有选中项时是空操作（buttons 侧也会禁用，这里再兜一层）
  store.clearSelection();
  store.toggleExcludedSelected();
  assert.equal(store.excludedCount(), 1);
});

/* ══════════════════════════════════════════════════════════════
 * 偏好与缩略图
 * ══════════════════════════════════════════════════════════════ */

test("档位：夹到合法范围并写回设置", async () => {
  const { api, state } = fakeApi();
  const store = createPhotoGridStore({ api });

  store.setTileStep(99);
  assert.equal(store.tileStep(), 8, "9 档 → 最大下标 8");
  store.setTileStep(-5);
  assert.equal(store.tileStep(), 0);
  await flush();
  assert.equal(
    state.settings.get("grid.tile_step"),
    undefined,
    "拖动过程中**不写设置**（一拖几百次 IPC + 数据库写正是卡的原因）",
  );
  store.commitTileStep();
  await flush();
  assert.equal(state.settings.get("grid.tile_step"), "0", "拖拽结束才落盘一次");

  store.setTileStep(Number.NaN);
  assert.equal(store.tileStep(), 4, "非法值回到默认档（正中间）");
});

test("按时间：状态写回设置，hydrate 能读回来", async () => {
  const { api, state } = fakeApi();
  const store = createPhotoGridStore({ api });
  store.setByTime(true);
  await flush();
  assert.equal(state.settings.get("grid.by_time"), "1");

  const reopened = createPhotoGridStore({ api });
  await reopened.hydrate();
  assert.equal(reopened.byTime(), true);
});

test("hydrate：设置里有非法值时退回默认", async () => {
  const { api, state } = fakeApi();
  state.settings.set("grid.tile_step", "abc");
  state.settings.set("grid.time_gap_minutes", "-3");
  const store = createPhotoGridStore({ api });
  await store.hydrate();
  assert.equal(store.tileStep(), 4);
  assert.equal(store.gapMinutes(), DEFAULT_GAP_MINUTES);
});

test("缩略图：按需请求，命中后不再重复取", async () => {
  const { api, state } = fakeApi();
  const store = createPhotoGridStore({ api });

  assert.deepEqual(store.thumb("/src/a.jpg"), { status: "idle", url: null });
  store.requestThumb("/src/a.jpg");
  await flush();
  assert.equal(store.thumb("/src/a.jpg").status, "ready");
  store.requestThumb("/src/a.jpg");
  await flush();
  assert.equal(
    state.calls.filter((call) => call === "thumb:/src/a.jpg").length,
    1,
  );
});
