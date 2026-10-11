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
import type {
  PhotoMeta,
  SourceItem,
  SourceScan,
  TakenAtSource,
} from "../../api/types.ts";
import { itemId } from "./rows.ts";
import {
  importDisplayByTime,
  importDisplayTileStep,
  resetDisplayPrefsForTests,
} from "../../lib/display-prefs.ts";
import {
  createPhotoGridStore,
  DEFAULT_GAP_MINUTES,
  type PhotoGridApi,
} from "./store.ts";
import { DEFAULT_TILE_STEP_INDEX, TILE_SIZE_STEPS } from "../../lib/tile-flow.ts";

test("同路径离线与恢复保留选择并重读照片清单", async () => {
  const f = fakeApi({ items: [item("甲.jpg")] }); const store = createPhotoGridStore({ api: f.api });
  store.setSourceDir("/src"); await flush(); store.clickItem("/src/甲.jpg", "replace");
  store.refreshSource(false); assert.equal(store.items().length, 1); assert.equal(store.selection().anchor, "/src/甲.jpg");
  f.state.items = [item("甲.jpg"), item("乙.jpg")]; store.refreshSource(true); await flush();
  assert.equal(store.items().length, 2); assert.equal(store.selection().anchor, "/src/甲.jpg");
  assert.equal(store.selectedCount(), 1); assert.equal(f.state.calls.filter(call => call.startsWith("scan:")).length, 2);
});

test("离线使在途扫描失效，不把迟到清单当当前盘内容", async () => {
  const f = fakeApi({ items: [item("甲.jpg")] }); const store = createPhotoGridStore({ api: f.api });
  store.setSourceDir("/src"); await flush(); f.state.holdScan = true;
  store.refreshSource(true); store.refreshSource(false); f.state.items = [item("旧盘.jpg")]; f.release(); await flush();
  assert.equal(store.items()[0]?.fileName, "甲.jpg"); assert.equal(store.error(), null);
});

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

/**
 * 测试里给某一张指定的「真实 EXIF 时间」。
 *
 * 以前它喂给 `readSourceTimes`；2026-10-11 起时间跟着 `dir_meta_ensure` 一起回来
 * （`specs/source-time-inline.md`），所以这里改成假后端的「读到的时间」。
 */
interface FakeTime {
  takenAtMs: number | null;
  takenAtSource: TakenAtSource | null;
  takenAtOffsetMin: number | null;
}

/** 一条元信息（测试里的字面量帮手；不写时间 = 这张读不到） */
function meta(
  relative: string,
  width: number,
  height: number,
  orientation = 1,
  times: Partial<FakeTime> = {},
): PhotoMeta {
  return {
    relative,
    width,
    height,
    orientation,
    takenAtMs: times.takenAtMs ?? null,
    takenAtSource: times.takenAtSource ?? null,
    takenAtOffsetMin: times.takenAtOffsetMin ?? null,
  };
}

function scanOf(items: SourceItem[]): SourceScan {
  return { root: "/src", items, skipped: 0, problems: [], elapsedMs: 1 };
}

interface FakeState {
  items: SourceItem[];
  times: Map<string, FakeTime>;
  settings: Map<string, string>;
  calls: string[];
  /** 元信息命令要返回什么（不给就是空） */
  dirMeta?: import("../../api/types.ts").PhotoMeta[];
  holdScan: boolean;
  failScan: boolean;
  /** 头部缓存那一步卡住不返回（测载入门与超时） */
  holdMeta: boolean;
  /** 读头失败（不该把网格变成错误态） */
  failMeta: boolean;
  /** 缓存里什么都没有：只查缓存时应当如实返回「不知道」（0×0 / 时间 null） */
  cacheMisses: boolean;
  /** 每次调 `dirMetaEnsure` 时前端传的 `cacheOnly`（钉住「界面的时间线路永不读文件」） */
  cacheOnlyFlags: boolean[];
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
    holdMeta: false,
    failMeta: false,
    cacheMisses: false,
    cacheOnlyFlags: [],
    scanRoots: [],
    ...overrides,
  };
  const pending: Array<() => void> = [];
  const pendingMeta: Array<() => void> = [];

  const api: PhotoGridApi = {
    async dirMetaEnsure(_dir, files, cacheOnly = false) {
      state.calls.push(`dirMeta:${files.length}`);
      state.cacheOnlyFlags.push(cacheOnly);
      if (state.holdMeta) {
        await new Promise<void>((resolve) => pendingMeta.push(resolve));
      }
      if (state.failMeta) throw new Error("读不了文件头");
      // **只查缓存**而缓存里没有 → 如实报「不知道」（宽高 0、时间 null）
      if (cacheOnly && state.cacheMisses) {
        return files.map((file) => meta(file.relative, 0, 0));
      }
      // 显式给了就照给（测比例那些用例）；没给就按真实实现的口径合成：
      // 「读头成功 → 时间来源 exif」，其中某一张的真实时间可以用 state.times 指定
      if (state.dirMeta) return state.dirMeta;
      return files.map((file) => {
        const entry = state.items.find((candidate) => candidate.fileName === file.relative);
        const explicit = entry === undefined ? undefined : state.times.get(itemId(entry));
        return meta(file.relative, 4000, 3000, 1, {
          takenAtMs: explicit?.takenAtMs ?? entry?.takenAtMs ?? null,
          takenAtSource: explicit?.takenAtSource ?? "exif",
          takenAtOffsetMin: explicit?.takenAtOffsetMin ?? entry?.takenAtOffsetMin ?? null,
        });
      });
    },
    async scanSourceDir(path) {
      state.calls.push(`scan:${path}`);
      state.scanRoots.push(path);
      if (state.holdScan) {
        await new Promise<void>((resolve) => pending.push(resolve));
      }
      if (state.failScan) throw new Error(`读不了 ${path}`);
      return scanOf(state.items);
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
    /** 放行卡住的头部缓存请求（测载入门与迟到结果） */
    releaseMeta() {
      const waiters = pendingMeta.splice(0, pendingMeta.length);
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

test("换目录：选择被清空（排除不归它管，见 workspaces/import/store.ts）", async () => {
  const { api, state } = fakeApi();
  state.items = [item("a.jpg"), item("b.jpg")];
  const store = createPhotoGridStore({ api });
  store.setSourceDir("/src");
  await flush();

  store.clickItem(itemId(state.items[0]), "replace");
  assert.equal(store.selectedCount(), 1);

  store.setSourceDir("/other");
  await flush();
  assert.equal(store.selectedCount(), 0);
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
  /*
   * 时间现在**不写回 `items`**（那是整表信号 → 会引发整表重排 ✗），只落进增量维护的分组结构。
   * 所以这里断言**用户看得见的那个结果**：这张落进了它该在的日期分类。
   */
  const grouped = store.grouping();
  assert.ok(grouped, "读到时间后应该能分组");
  assert.ok(
    grouped.days.some((day) => day.photoIds.includes("/src/b.jpg")),
    "补到真时间后，b 应当出现在对应的日期分类里",
  );
  assert.equal(
    store.items().find((entry) => entry.fileName === "b.jpg")?.takenAtSource,
    null,
    "`items` 保持清单层的值不动（真相在分组结构里）",
  );
  assert.ok(
    state.calls.includes("dirMeta:1"),
    "只补读那一张（a.jpg 已经是 exif 来源，不需要重读）",
  );
});

test("按时间：显示顺序跟着分组走（日升序 → 片升序）", async () => {
  const { api, state } = fakeApi();
  state.items = [item("old.jpg", at(9, 0, 14)), item("new.jpg", at(9, 0, 15))];
  const store = createPhotoGridStore({ api });
  store.setSourceDir("/src");
  await flush();
  store.setByTime(true);
  await flush();

  assert.deepEqual(
    store.displayItems().map((entry) => entry.fileName),
    ["old.jpg", "new.jpg"],
    "日期层先到后（升序）",
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

test("整段开关：日 / 时间片那颗药丸（全选中 → 全取消，否则 → 全选中）", async () => {
  const { api, state } = fakeApi();
  state.items = [item("a.jpg"), item("b.jpg"), item("c.jpg")];
  const store = createPhotoGridStore({ api });
  store.setSourceDir("/src");
  await flush();
  const ids = store.items().map(itemId);

  store.toggleGroup([ids[0], ids[1]]);
  assert.equal(store.selectedCount(), 2);
  store.toggleGroup([ids[2]]);
  assert.equal(store.selectedCount(), 3, "另一段未全选 → 并进现有选择（连着点几段是加法）");

  store.toggleGroup([ids[0], ids[1]]);
  assert.deepEqual(
    [...store.selectedIds()],
    [ids[2]],
    "这一整段已全选 → 整段取消，其它段不动",
  );

  // 部分选中也是「全开」而不是逐项反转：
  store.clearSelection();
  store.toggleGroup([ids[0]]);
  assert.equal(store.selectedCount(), 1);
  store.toggleGroup([ids[0], ids[1], ids[2]]);
  assert.equal(store.selectedCount(), 3, "部分选中 = 补齐，不是把已选的翻掉");

  store.toggleGroup([]);
  assert.equal(store.selectedCount(), 3, "空组不动选择");
});

/*
 * 批量排除的用例**搬走了**（2026-09-16）：排除不再住在网格 store 里 ——
 * 它换目录时会被清空（用户切一圈回来发现排除全丢了）。
 * 现在住在导入工作区的 store：见 `workspaces/import/store.test.ts`
 * 的「排除」一节，纯函数在 `lib/excluded.test.ts`。
 */

/* ══════════════════════════════════════════════════════════════
 * 偏好与缩略图
 * ══════════════════════════════════════════════════════════════ */

test("档位：夹到合法范围；拖动中只改内存、松手才落盘", async () => {
  const { api, state } = fakeApi();
  const store = createPhotoGridStore({ api });
  resetDisplayPrefsForTests();

  store.setTileStep(99);
  assert.equal(store.tileStep(), TILE_SIZE_STEPS.length - 1, "越界夹到最大档");
  store.setTileStep(-5);
  assert.equal(store.tileStep(), 0);
  await flush();
  assert.equal(
    state.settings.get("grid.tile_step"),
    undefined,
    "拖动过程中**不写任何存储**（一拖几百次写正是卡的原因）",
  );
  store.commitTileStep();
  assert.equal(importDisplayTileStep(), 0, "松手时落盘的是当前 import 档位");
  assert.equal(
    state.settings.get("grid.tile_step"),
    undefined,
    "档位已经不存 app.db 了（搬去 lib/display-prefs.ts 的 localStorage）",
  );

  store.setTileStep(Number.NaN);
  assert.equal(store.tileStep(), DEFAULT_TILE_STEP_INDEX, "非法值回到默认档（几何中点那档）");
  resetDisplayPrefsForTests();
});

test("按时间：写进 import 显示偏好（新开的 import store 立刻读到）", async () => {
  const { api, state } = fakeApi();
  const store = createPhotoGridStore({ api });
  resetDisplayPrefsForTests();

  store.setByTime(true);
  await flush();
  assert.equal(importDisplayByTime(), true, "import 偏好已更新");
  assert.equal(
    state.settings.get("grid.by_time"),
    undefined,
    "这条也不再写 app.db（以前只有导入侧写，所以浏览侧每次都重置）",
  );

  const reopened = createPhotoGridStore({ api });
  assert.equal(reopened.byTime(), true, "同一会话里新开的 store 立刻看到同一个值");
  resetDisplayPrefsForTests();
});

test("hydrate：设置里有非法值时退回默认", async () => {
  const { api, state } = fakeApi();
  state.settings.set("grid.tile_step", "abc");
  state.settings.set("grid.time_gap_minutes", "-3");
  const store = createPhotoGridStore({ api });
  await store.hydrate();
  // 时间间隔阈值仍来自 app.db；档位来自显示偏好（那一条的校验在 display-prefs.test.ts）
  assert.equal(store.gapMinutes(), DEFAULT_GAP_MINUTES);
  assert.equal(store.tileStep(), importDisplayTileStep());
});

test("换卡/覆盖保存：同一个路径换了内容就作废那条小图", async () => {
  const f = fakeApi({ items: [item("甲.jpg")] });
  const store = createPhotoGridStore({ api: f.api });
  store.setSourceDir("/src");
  await flush();
  store.requestThumb("/src/甲.jpg");
  await flush();
  assert.equal(store.thumb("/src/甲.jpg").status, "ready");

  // ① 同一张卡再看一眼（内容没变）→ 不许重取：这就是「临时缓存」要保住的东西
  store.setSourceDir("/other");
  await flush();
  store.setSourceDir("/src");
  await flush();
  assert.equal(
    store.thumb("/src/甲.jpg").status,
    "ready",
    "文件身份没变，内存里那份就该继续用",
  );

  // ② 换了一张卡（或文件被重新导出覆盖）：同一个路径下是**另一张照片** ——
  //    少了身份比对，用户会看到上一张卡的照片（崔总 2026-10-11 点出的场景）
  store.setSourceDir("/other");
  await flush();
  f.state.items = [
    { ...item("甲.jpg"), sizeBytes: 4096, mtimeMs: 1_700_000_000_000 },
  ];
  store.setSourceDir("/src");
  await flush();
  assert.equal(
    store.thumb("/src/甲.jpg").status,
    "ready",
    "取回来的还是同一格（旧图先顶着，不闪白）",
  );
  assert.equal(
    f.state.calls.filter((call) => call === "thumb:/src/甲.jpg").length,
    2,
    "身份变了必须重新取一次 —— 否则显示的会是上一张卡的照片",
  );
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

test("元信息：拿到的宽高决定展示比例（竖图就是竖的）", async () => {
  const { api, state } = fakeApi();
  state.dirMeta = [
    meta("a.jpg", 4000, 3000),
    // 竖拍：后端已经应用过方向，所以这里宽高是反的
    meta("b.jpg", 3000, 4000, 6),
    // 全景：超出 3:1 → 夹到 3:1
    meta("c.jpg", 8000, 1000),
  ];
  state.items = [item("a.jpg"), item("b.jpg"), item("c.jpg")];
  const store = createPhotoGridStore({ api });
  store.setSourceDir("/photos");
  await flush();
  await store.ensureNatural(store.items().map(item => item.path));

  const items = store.items();
  const byName = new Map(items.map((item) => [item.fileName, item.path]));
  assert.ok(Math.abs(store.aspectOf(byName.get("a.jpg")!) - 4 / 3) < 1e-9);
  assert.ok(
    Math.abs(store.aspectOf(byName.get("b.jpg")!) - 0.75) < 1e-9,
    "竖拍比例要小于 1（说明方向被用上了）",
  );
  assert.equal(store.aspectOf(byName.get("c.jpg")!), 3, "全景夹到 3:1");
});

test("元信息：读不到就用默认占位比例，不报错", async () => {
  const { api, state } = fakeApi(); // dirMeta 不给 → 空
  state.items = [item("a.jpg")];
  const store = createPhotoGridStore({ api });
  store.setSourceDir("/photos");
  await flush();
  const first = store.items()[0];
  assert.ok(first !== undefined);
  // 默认占位 3:2
  assert.ok(Math.abs(store.aspectOf(store.items()[0]!.path) - 1.5) < 1e-9);
});

test("慢盘目录立即 ready，不读任何原片 metadata；view 才补方向与自然尺寸", async () => {
  const { api, state, releaseMeta } = fakeApi({ holdMeta: true });
  state.items = Array.from({ length: 500 }, (_, i) => item(`照片${i}.jpg`));
  state.dirMeta = [meta("照片0.jpg", 3000, 4000, 6)];
  const store = createPhotoGridStore({ api });
  store.setSourceDir("/photos"); await flush();
  assert.equal(store.status(), "ready");
  assert.equal(state.calls.some(c => c.startsWith("dirMeta:")), false);
  const first = store.items()[0]!.path;
  assert.equal(store.naturalOf(first), null);
  const read = store.ensureNatural([first]);
  assert.equal(state.calls.filter(c => c.startsWith("dirMeta:")).slice(-1)[0], "dirMeta:1");
  releaseMeta(); await read;
  assert.deepEqual(store.naturalOf(first), { width: 3000, height: 4000 });
  assert.equal(store.aspectOf(first), 0.75);
});

test("view 迟到的原片尺寸不能污染新目录", async () => {
  const { api, state, releaseMeta } = fakeApi({ holdMeta: true });
  state.items = [item("old.jpg")]; state.dirMeta = [meta("old.jpg", 3000, 4000, 6)];
  const store = createPhotoGridStore({ api }); store.setSourceDir("/old"); await flush();
  const oldPath = store.items()[0]!.path; const read = store.ensureNatural([oldPath]);
  state.items = [item("new.jpg")]; store.setSourceDir("/new"); await flush();
  releaseMeta(); await read;
  assert.equal(store.naturalOf(oldPath), null); assert.equal(store.status(), "ready");
});

test("按时间：同名 JPG 与 RW2 补读 EXIF 后落进同一片，并且片内挨着", async () => {
  /*
   * 人类两级上报的那条：源目录里 JPG 与 RAW 各自成组、同一天的分组标题出现两次。
   *
   * 机制在**兜底时间**上：扫描阶段给的是「文件名 / mtime」兜底，两种格式的真时间不同就会分开。
   * 这里刻意把兜底值拉开：JPG 一侧 00:54 / 01:08，RAW 一侧 03:58 / 04:26 ——
   * 中间差 2 小时 50 分 > 1 小时阈值，于是**断成两片**（这就是人类看到的画面）。
   * 补读 EXIF 之后四个都在 01:08 之内，应当只剩一片，且片内按文件名自然序。
   */
  const { api, state } = fakeApi();
  state.items = [
    { ...item("P1000019.JPG", at(0, 54)), takenAtSource: "file_mtime" },
    { ...item("P1000020.JPG", at(1, 8)), takenAtSource: "file_mtime" },
    { ...item("P1000019.RW2", at(3, 58)), ext: "rw2", kind: "raw", takenAtSource: "file_mtime" },
    { ...item("P1000020.RW2", at(4, 26)), ext: "rw2", kind: "raw", takenAtSource: "file_mtime" },
  ];
  const withExif = (name: string, ms: number): void => {
    state.times.set(`/src/${name}`, {
      takenAtMs: ms,
      takenAtSource: "exif",
      takenAtOffsetMin: CST,
    });
  };
  // 同一张照片的 RAW 与 JPG 差十几秒（真实情况就是几秒到几十秒）
  withExif("P1000019.JPG", at(0, 54));
  withExif("P1000019.RW2", at(0, 54, 15));
  withExif("P1000020.JPG", at(1, 8));
  withExif("P1000020.RW2", at(1, 8, 15));

  const store = createPhotoGridStore({ api });
  store.setSourceDir("/src");
  await flush();
  store.setByTime(true);
  await flush();

  const grouped = store.grouping();
  assert.ok(grouped, "补读时间之后应当能分组");
  assert.equal(grouped.days.length, 1, "四个文件属于同一天");
  assert.equal(grouped.days[0]?.slices.length, 1, "而且只落进一片 —— 不再按格式分层");
  assert.deepEqual(
    store.displayItems().map((entry) => entry.fileName),
    ["P1000019.JPG", "P1000019.RW2", "P1000020.JPG", "P1000020.RW2"],
    "片内按文件名自然序，同名的 JPG 与 RAW 挨着",
  );
});


test("按时间补读分块（48/条），关掉之后不再读后续块", async () => {
  resetDisplayPrefsForTests();
  /*
   * 为什么是「小批」而不是「一趟读完」（2026-10-11 崔总拍板的改法）：
   * 以前 `TIMES_BATCH = 512`——一条命令读整目录，真机 306 张要 6 秒，
   * 那 6 秒里所有缩略图请求和整个界面都被按住（诊断
   * `implementations/2026-10-11_import-still-slow-diagnosis.md`）。
   * 现在一条命令最多 48 张（几十毫秒），块间让出一帧，界面插得进空。
   */
  const f = fakeApi({ items: Array.from({ length: 200 }, (_, i) => item(`${i}.JPG`)) });
  const batches: number[] = [];
  let release: (() => void) | undefined;
  const inner = f.api.dirMetaEnsure;
  f.api.dirMetaEnsure = async (dir, files) => {
    batches.push(files.length);
    await new Promise<void>((resolve) => { release = resolve; });
    return inner(dir, files);
  };
  const store = createPhotoGridStore({ api: f.api });
  store.setSourceDir("/src"); await flush();
  store.setByTime(true); await flush();
  assert.deepEqual(batches, [48], "一条命令最多 48 张");
  store.setByTime(false);
  release?.();
  await flush();
  await flush();
  assert.deepEqual(batches, [48], "关掉之后不再读后续块");
  assert.equal(store.loadingTimes(), false);
  resetDisplayPrefsForTests();
});

test("按时间：只查缓存，缓存没有就保持兜底（界面这条线永远不读文件）", async () => {
  resetDisplayPrefsForTests();
  const f = fakeApi();
  f.state.cacheMisses = true; // 缓存里什么都没有（谁还没出过图）
  f.state.items = [item("a.jpg", at(9, 0)), item("b.jpg", null)];
  const store = createPhotoGridStore({ api: f.api });
  store.setSourceDir("/src");
  await flush();
  store.setByTime(true);
  await flush();

  const items = store.items();
  assert.equal(items[0]?.takenAtSource, "filename", "缓存没有 → 兜底时间原样留着");
  assert.equal(items[0]?.takenAtMs, at(9, 0));
  assert.equal(items[1]?.takenAtMs, null, "连兜底都没有 → 未知时间组");
  assert.ok(
    f.state.cacheOnlyFlags.length > 0 && f.state.cacheOnlyFlags.every(Boolean),
    "每一次查询都必须带 cacheOnly（这条线不许读文件）",
  );
  resetDisplayPrefsForTests();
});

test("按时间：分块补读不把一对 JPG/RAW 拆进两次请求", async () => {
  resetDisplayPrefsForTests();
  // 25 对（50 个文件）：一条命令最多 48 张，但边界必须落在「主名变化」处，
  // 否则后端那一批里只有 RAW 没有它的位图兄弟，配对继承就断了
  //（规格 `specs/source-time-inline.md` §2；规则本身在 Rust 的 `media::pairing`）。
  const items: SourceItem[] = [];
  for (let i = 0; i < 25; i += 1) {
    const stem = `P10000${String(i).padStart(2, "0")}`;
    items.push({ ...item(`${stem}.JPG`), ext: "jpg", kind: "image" });
    items.push({ ...item(`${stem}.RW2`), ext: "rw2", kind: "raw" });
  }
  const f = fakeApi({ items });
  const batches: string[][] = [];
  const inner = f.api.dirMetaEnsure;
  f.api.dirMetaEnsure = async (dir, files) => {
    batches.push(files.map((file) => file.relative));
    return inner(dir, files);
  };
  const store = createPhotoGridStore({ api: f.api });
  store.setSourceDir("/src");
  await flush();
  store.setByTime(true);
  await flush();

  assert.equal(batches.length, 2, "50 张按 48 切 → 两块");
  for (const batch of batches) {
    const stems = new Set(batch.map((name) => name.replace(/\.[^.]+$/, "")));
    assert.equal(
      stems.size * 2,
      batch.length,
      `一对不许被拆开：${batch.slice(0, 3).join(",")} …`,
    );
  }
  resetDisplayPrefsForTests();
});

test("按时间：分块到达就分块重排（渐进，不用等整目录）", async () => {
  resetDisplayPrefsForTests();
  // 96 张 = 两块。先让第一块回来，分组框架就该已经按真时间排好第一块
  const f = fakeApi({ items: Array.from({ length: 96 }, (_, i) => item(`${i}.JPG`)) });
  const store = createPhotoGridStore({ api: f.api });
  store.setSourceDir("/src");
  await flush();
  store.setByTime(true);
  await flush(); // 第一块落地（假后端每块立即返回）
  assert.ok(store.grouping(), "第一块回来就能分组");
  assert.equal(store.loadingTimes(), false);
  resetDisplayPrefsForTests();
});

test("按时间：低可信度的时间不许把已读到的高可信度顶掉（只升不降）", async () => {
  resetDisplayPrefsForTests();
  const f = fakeApi();
  // 清单里已经是「姊妹文件继承」来的时间（rank 2，比 mtime 高）……
  f.state.items = [
    { ...item("a.jpg", at(9, 0)), takenAtSource: "sibling", takenAtOffsetMin: CST },
  ];
  // ……而这次读头只拿到 mtime（rank 0，例如缓存没建全、EXIF 一时读不到）
  f.state.dirMeta = [
    meta("a.jpg", 4000, 3000, 1, {
      takenAtMs: at(23, 0),
      takenAtSource: "file_mtime",
      takenAtOffsetMin: CST,
    }),
  ];
  const store = createPhotoGridStore({ api: f.api });
  store.setSourceDir("/src");
  await flush();
  store.setByTime(true);
  await flush();
  assert.equal(store.items()[0]?.takenAtSource, "sibling", "低来源不许覆盖高来源");
  assert.equal(store.items()[0]?.takenAtMs, at(9, 0), "时间也没被改写");
  resetDisplayPrefsForTests();
});
