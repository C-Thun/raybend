/**
 * 导入进度 store 的测试（`plans/M1-6.md` §5 的「假事件流」）。
 *
 * 喂的是**后端真实会发的那种快照**，不是手编的零碎字段 —— 单测替身必须与真 API 同形，
 * 否则测的是替身的行为（M1-5 踩过：替身把时间清成 null，于是「按时间分组」假绿）。
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import type {
  ImportBatchProgress,
  ImportRunProgress,
  ImportStage,
  ImportState,
} from "../../api/types.ts";
import { createImportStore, type ImportApi } from "./store.ts";

/** 最后一条调用（`Array.prototype.at` 要 es2022 的 lib，这里不引）。 */
function lastCall(calls: readonly string[]): string | undefined {
  return calls[calls.length - 1];
}

/* ══════════════════════════════════════════════════════════════
 * 造快照（与 Rust 的 BatchProgress 同形）
 * ══════════════════════════════════════════════════════════════ */

function runSnapshot(overrides: Partial<ImportRunProgress> = {}): ImportRunProgress {
  return {
    runId: 1,
    sourceRoot: "/src/a",
    stage: "scan",
    state: "running",
    scanned: 0,
    total: 0,
    done: 0,
    imported: 0,
    skipped: 0,
    duplicates: 0,
    failed: 0,
    bytes: 0,
    current: null,
    note: null,
    ...overrides,
  };
}

function snapshot(overrides: Partial<ImportBatchProgress> = {}): ImportBatchProgress {
  const runs = overrides.runs ?? [runSnapshot()];
  const sum = (key: keyof ImportRunProgress): number =>
    runs.reduce((acc, run) => acc + (run[key] as number), 0);
  return {
    batchId: "b1",
    stage: "scan",
    state: "running",
    runs,
    currentRun: 0,
    total: sum("total"),
    done: sum("done"),
    imported: sum("imported"),
    skipped: sum("skipped"),
    duplicates: sum("duplicates"),
    failed: sum("failed"),
    bytes: sum("bytes"),
    errors: [],
    errorsTotal: 0,
    freeBytes: null,
    startedAt: 1_789_516_800_000,
    finishedAt: null,
    ...overrides,
  };
}

function stageOf(stage: ImportStage, state: ImportState = "running") {
  return { stage, state };
}

/* ══════════════════════════════════════════════════════════════
 * 假 api：命令可断言、事件可手动推
 * ══════════════════════════════════════════════════════════════ */

interface FakeState {
  calls: string[];
  handlers: ((progress: ImportBatchProgress) => void)[];
  unlistened: number;
  startFails: boolean;
  statusSnapshot: ImportBatchProgress;
  commandSnapshot: ImportBatchProgress;
}

function fakeApi() {
  const state: FakeState = {
    calls: [],
    handlers: [],
    unlistened: 0,
    startFails: false,
    statusSnapshot: snapshot(),
    commandSnapshot: snapshot(),
  };

  const api: ImportApi = {
    async start(_repositoryId, sources, avoidDuplicates) {
      state.calls.push(
        `start:${sources.map((s) => s.path).join(",")}:${String(avoidDuplicates)}`,
      );
      if (state.startFails) throw new Error("库离线了");
      return {
        batchId: "b1",
        runs: sources.map((source, index) => ({ index, sourceRoot: source.path })),
      };
    },
    async pause() {
      state.calls.push("pause");
      return state.commandSnapshot;
    },
    async resume() {
      state.calls.push("resume");
      return state.commandSnapshot;
    },
    async cancel() {
      state.calls.push("cancel");
      return state.commandSnapshot;
    },
    async status(batchId) {
      state.calls.push(`status:${batchId}`);
      return state.statusSnapshot;
    },
    async exportErrors(batchId, path) {
      state.calls.push(`export:${batchId}:${path}`);
      return 2;
    },
    async subscribe(handler) {
      state.calls.push("subscribe");
      state.handlers.push(handler);
      return () => {
        state.unlistened += 1;
      };
    },
  };

  return {
    api,
    state,
    /** 推一条事件给所有订阅者。 */
    emit(progress: ImportBatchProgress) {
      for (const handler of state.handlers) handler(progress);
    },
  };
}

async function startedStore() {
  const fake = fakeApi();
  const store = createImportStore({ api: fake.api });
  await store.begin({
    repositoryId: "repo-1",
    sources: [{ path: "/src/a", includeSubdirs: true }],
    avoidDuplicates: true,
  });
  return { ...fake, store };
}

/* ══════════════════════════════════════════════════════════════
 * 测试
 * ══════════════════════════════════════════════════════════════ */

test("begin：调 start、订阅事件、取一次快照", async () => {
  const { state, store } = await startedStore();
  assert.deepEqual(state.calls, ["start:/src/a:true", "subscribe", "status:b1"]);
  assert.equal(store.batchId(), "b1");
  assert.equal(store.open(), true);
  assert.equal(store.error(), null);
});

test("事件推进阶段：扫描百分比未知 → 导入阶段按 done/total → 完成 100", async () => {
  const { store, emit } = await startedStore();

  emit(snapshot({ runs: [runSnapshot({ stage: "scan", scanned: 40 })], stage: "scan" }));
  assert.equal(store.stage(), "scan");
  assert.equal(store.percent(), null, "扫描阶段不编造百分比");
  assert.equal(store.progress()?.runs[0].scanned, 40);

  emit(
    snapshot({
      stage: "plan",
      runs: [runSnapshot({ stage: "plan", scanned: 40 })],
    }),
  );
  assert.equal(store.percent(), null, "规划阶段也还没有总数");

  emit(
    snapshot({
      stage: "import",
      runs: [
        runSnapshot({
          stage: "import",
          total: 4,
          done: 1,
          imported: 1,
          current: { source: "a.jpg", target: "photos/2026/MYa.jpg" },
        }),
      ],
    }),
  );
  assert.equal(store.percent(), 25);
  assert.equal(store.currentLabel(), "a.jpg → photos/2026/MYa.jpg");

  emit(
    snapshot({
      stage: "done",
      state: "done",
      finishedAt: 2,
      runs: [runSnapshot({ stage: "done", state: "done", total: 4, done: 4, imported: 4 })],
    }),
  );
  assert.equal(store.percent(), 100);
  assert.equal(store.finished(), true);
  assert.equal(store.state(), "done");
});

test("多源目录：计数是聚合值，标签给 [2/3] 目录名", async () => {
  const fake = fakeApi();
  const store = createImportStore({ api: fake.api });
  await store.begin({
    repositoryId: "repo-1",
    sources: [
      { path: "/src/a", includeSubdirs: true },
      { path: "/src/b", includeSubdirs: true },
      { path: "/src/c", includeSubdirs: false },
    ],
    avoidDuplicates: true,
  });

  fake.emit(
    snapshot({
      stage: "import",
      currentRun: 1,
      runs: [
        runSnapshot({ runId: 1, sourceRoot: "/src/a", stage: "done", state: "done", total: 10, done: 10, imported: 9, failed: 1 }),
        runSnapshot({
          runId: 2,
          sourceRoot: "/src/b",
          stage: "import",
          total: 5,
          done: 2,
          imported: 2,
          current: { source: "b.jpg", target: null },
        }),
        runSnapshot({ runId: 3, sourceRoot: "/src/c" }),
      ],
      finishedAt: null,
    }),
  );

  assert.equal(store.runLabel(), "[2/3] /src/b");
  assert.deepEqual(store.counts(), {
    total: 15,
    done: 12,
    imported: 11,
    skipped: 0,
    duplicates: 0,
    failed: 1,
  });
  assert.equal(store.percent(), 80, "12/15");
  assert.equal(store.currentLabel(), "b.jpg", "目标还没算出来时只显示源");
});

test("单源目录不显示 [1/1]", async () => {
  const { store, emit } = await startedStore();
  emit(snapshot());
  assert.equal(store.runLabel(), null);
});

test("失败条目进错误清单，计数跟着走", async () => {
  const { store, emit } = await startedStore();
  emit(
    snapshot({
      stage: "import",
      runs: [runSnapshot({ stage: "import", total: 3, done: 2, imported: 1, failed: 1 })],
      errors: [
        { source: "bad.jpg", target: null, reason: "读不了", status: "failed" },
      ],
      errorsTotal: 1,
    }),
  );
  assert.equal(store.errors().length, 1);
  assert.equal(store.errors()[0].reason, "读不了");
  assert.deepEqual(store.counts(), {
    total: 3,
    done: 2,
    imported: 1,
    skipped: 0,
    duplicates: 0,
    failed: 1,
  });
});

test("暂停 / 继续：走命令，用返回的快照更新", async () => {
  const { store, state } = await startedStore();
  state.commandSnapshot = snapshot({
    state: "paused",
    runs: [runSnapshot({ state: "paused", stage: "import", total: 5, done: 2, imported: 2 })],
  });
  await store.pause();
  assert.equal(lastCall(state.calls), "pause");
  assert.equal(store.state(), "paused");
  assert.equal(store.counts().imported, 2, "暂停时已有的进度要保住");

  state.commandSnapshot = snapshot({ state: "running" });
  await store.resume();
  assert.equal(lastCall(state.calls), "resume");
  assert.equal(store.state(), "running");
});

test("取消：已导入的保留，终态后自动退订", async () => {
  const { store, state } = await startedStore();
  state.commandSnapshot = snapshot({
    state: "cancelled",
    finishedAt: 9,
    runs: [
      runSnapshot({
        state: "cancelled",
        stage: "import",
        total: 5,
        done: 2,
        imported: 2,
      }),
    ],
  });
  await store.cancel();
  assert.equal(lastCall(state.calls), "cancel");
  assert.equal(store.state(), "cancelled");
  assert.equal(store.finished(), true);
  assert.equal(store.counts().imported, 2, "取消时已导入的保留（界面上要说清）");
  assert.equal(state.unlistened, 1, "终态之后不该再占着订阅");
});

test("命令失败：记下原因，但不清掉已有进度", async () => {
  const fake = fakeApi();
  const store = createImportStore({
    api: {
      ...fake.api,
      async pause() {
        throw new Error("批次不在了");
      },
    },
  });
  await store.begin({
    repositoryId: "repo-1",
    sources: [{ path: "/src/a", includeSubdirs: true }],
    avoidDuplicates: true,
  });
  fake.emit(
    snapshot({
      stage: "import",
      runs: [runSnapshot({ stage: "import", total: 5, done: 1, imported: 1 })],
    }),
  );
  await store.pause();
  assert.equal(store.error(), "批次不在了");
  assert.equal(store.counts().imported, 1, "出错不该把已有进度清掉");
});

test("开始导入失败：记原因、没有批次、不订阅", async () => {
  const fake = fakeApi();
  fake.state.startFails = true;
  const store = createImportStore({ api: fake.api });
  await store.begin({
    repositoryId: "repo-1",
    sources: [{ path: "/src/a", includeSubdirs: true }],
    avoidDuplicates: false,
  });
  assert.equal(store.error(), "库离线了");
  assert.equal(store.batchId(), null);
  assert.equal(fake.state.handlers.length, 0, "没开起来就别订阅");
  assert.equal(fake.state.calls.includes("subscribe"), false);
});

test("浏览器降级：没有后端时给一句人话，不订阅、不空转", async () => {
  const fake = fakeApi();
  const store = createImportStore({
    api: {
      ...fake.api,
      // 浏览器里 `api/import.ts` 就是这么返回的
      async start() {
        return { batchId: "", runs: [] };
      },
    },
  });
  await store.begin({
    repositoryId: "repo-1",
    sources: [{ path: "/src/a", includeSubdirs: true }],
    avoidDuplicates: true,
  });
  assert.equal(store.batchId(), null);
  assert.match(store.error() ?? "", /开发预览/);
  assert.equal(fake.state.calls.includes("subscribe"), false, "没有后端就别订阅");
  assert.equal(fake.state.calls.includes("status:"), false);
  // 快照仍是空的：弹窗该显示「原因」，而不是一个永远转圈的进度条
  assert.equal(store.progress(), null);
  assert.equal(store.finished(), false, "不是「完成」，是「没法开始」");
});

test("只认自己那一批的事件（别的批次的快照被忽略）", async () => {
  const { store, emit } = await startedStore();
  emit(snapshot({ batchId: "别的批次", stage: "done", state: "done", imported: 99 }));
  assert.equal(store.counts().imported, 0, "别批次的进度不该污染这一批");
  emit(snapshot({ stage: "import", imported: 3, runs: [runSnapshot({ total: 3, done: 3, imported: 3 })] }));
  assert.equal(store.counts().imported, 3);
});

test("忙碌时重复的命令不会发两遍", async () => {
  const { store, state } = await startedStore();
  const first = store.pause();
  const second = store.pause();
  await Promise.all([first, second]);
  assert.equal(state.calls.filter((call) => call === "pause").length, 1, "按钮连点只该发一次");
});

test("导出错误清单：拿到条数；失败时返回 0 并记原因", async () => {
  const { store, state } = await startedStore();
  assert.equal(await store.exportErrors("/tmp/errors.json"), 2);
  assert.equal(lastCall(state.calls), "export:b1:/tmp/errors.json");

  const fake = fakeApi();
  const broken = createImportStore({
    api: {
      ...state_api(fake.state),
      async exportErrors() {
        throw new Error("写不进去");
      },
    },
  });
  await broken.begin({
    repositoryId: "repo-1",
    sources: [{ path: "/src/a", includeSubdirs: true }],
    avoidDuplicates: true,
  });
  assert.equal(await broken.exportErrors("/root/errors.json"), 0);
  assert.equal(broken.error(), "写不进去");
});

test("run 级提示会浮上来（缩略图入队失败那类）", async () => {
  const { store, emit } = await startedStore();
  emit(
    snapshot({
      stage: "thumbs",
      runs: [runSnapshot({ stage: "thumbs", note: "缩略图入队失败：队列写不进去" })],
    }),
  );
  assert.equal(store.runNote(), "缩略图入队失败：队列写不进去");
});

test("dispose 退订，且不影响后续读快照", async () => {
  const { store, state } = await startedStore();
  store.dispose();
  assert.equal(state.unlistened, 1);
  store.dispose();
  assert.equal(state.unlistened, 1, "退订要幂等");
});

test("弹窗开关：openDialog / dismiss 只影响显示", async () => {
  const { store } = await startedStore();
  assert.equal(store.open(), true, "开始导入时弹窗就该开着");
  store.dismiss();
  assert.equal(store.open(), false);
  store.openDialog();
  assert.equal(store.open(), true);
  assert.equal(store.finished(), false, "收起弹窗不等于结束");
});

test("没有批次时命令是空操作（不会抛）", async () => {
  const fake = fakeApi();
  const store = createImportStore({ api: fake.api });
  await store.pause();
  await store.resume();
  await store.cancel();
  assert.deepEqual(fake.state.calls, [], "还没开始就别发命令");
  assert.equal(await store.exportErrors("/tmp/x.json"), 0);
});

/* ══════════════════════════════════════════════════════════════
 * 小工具：给「换掉某一个方法」的测试复用同一份假实现
 * ══════════════════════════════════════════════════════════════ */

function state_api(state: FakeState): ImportApi {
  return {
    async start() {
      return { batchId: "b1", runs: [{ index: 0, sourceRoot: "/src/a" }] };
    },
    async pause() {
      return state.commandSnapshot;
    },
    async resume() {
      return state.commandSnapshot;
    },
    async cancel() {
      return state.commandSnapshot;
    },
    async status() {
      return state.statusSnapshot;
    },
    async exportErrors() {
      return 0;
    },
    async subscribe(handler) {
      state.handlers.push(handler);
      return () => {
        state.unlistened += 1;
      };
    },
  };
}

// 让 `stageOf` 不被当成死代码（它给上面几处快照读起来更直观）
void stageOf;
