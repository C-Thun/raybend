/**
 * 画廊里的「导入进度」演示（`/dev/kitchen-sink`）。
 *
 * 为什么要有它：进度弹窗只有在**真有库、真勾了目录**时才打得开，
 * 而画廊页（以及冒烟脚本跑的 Chromium）里没有后端 —— 于是这个组件
 * 用**真的 store + 假的后端**把它驱动起来：
 *
 * * 真 store：`createImportStore` 与生产同一份代码（事件订阅、派生值、暂停取消都真的走）；
 * * 假后端：`ImportApi` 的实现返回固定的快照，按钮把快照**推**进订阅里。
 *
 * 于是 `scripts/ui-smoke.mjs` 能对它的结构做断言（阶段条、计数、当前项、
 * 取消的二次确认、结束摘要），人也能点着看。
 */

import { createSignal, onMount, Show } from "solid-js";
import {
  createImportStore,
  ImportProgressDialog,
  type ImportApi,
} from "../features/import/index.ts";
import type {
  ImportBatchProgress,
  ImportRunProgress,
} from "../api/types.ts";
import { Button } from "../components/ui/Button.tsx";

const T0 = 1_789_516_800_000;

/** 造一个与 Rust 同形的快照。 */
function snapshot(overrides: Partial<ImportBatchProgress> = {}): ImportBatchProgress {
  const runs: ImportRunProgress[] = overrides.runs ?? [run()];
  const sum = (pick: (run: ImportRunProgress) => number): number =>
    runs.reduce((acc, item) => acc + pick(item), 0);
  return {
    batchId: "demo",
    stage: "scan",
    state: "running",
    runs,
    currentRun: 0,
    total: sum((r) => r.total),
    done: sum((r) => r.done),
    imported: sum((r) => r.imported),
    skipped: sum((r) => r.skipped),
    duplicates: sum((r) => r.duplicates),
    failed: sum((r) => r.failed),
    bytes: sum((r) => r.bytes),
    errors: [],
    errorsTotal: 0,
    freeBytes: null,
    startedAt: T0,
    finishedAt: null,
    // 覆盖必须放在最后 —— 少了这一行，`state`/`stage`/`errors` 会被上面的默认值吃掉，
    // 演示就永远停在「扫描中/没有错误」（步骤 15 的冒烟抓到的，值 4 条断言）
    ...overrides,
  };
}

function run(overrides: Partial<ImportRunProgress> = {}): ImportRunProgress {
  return {
    runId: 1,
    sourceRoot: "D:\\照片\\2026 旅行",
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

/** 三个可点的状态 + 结束态（画廊里按一下就看得到）。 */
const SCRIPTS = {
  running: () =>
    snapshot({
      stage: "import",
      runs: [
        run({
          stage: "import",
          scanned: 120,
          total: 120,
          done: 86,
          imported: 82,
          skipped: 3,
          duplicates: 2,
          failed: 1,
          bytes: 149_406_208,
          current: {
            source: "P1040733.ORF",
            target: "photos/2026-08-15/_RAW/MYP0733.ORF",
          },
        }),
      ],
      errors: [
        {
          source: "P1040733.ORF",
          target: null,
          reason: "磁盘写满：只剩 12 MB",
          status: "failed",
        },
      ],
      errorsTotal: 1,
    }),
  paused: () =>
    snapshot({
      stage: "import",
      state: "paused",
      runs: [
        run({
          stage: "import",
          state: "paused",
          total: 120,
          done: 40,
          imported: 38,
          skipped: 1,
          failed: 1,
        }),
      ],
    }),
  done: () =>
    snapshot({
      stage: "done",
      state: "done",
      finishedAt: T0 + 90_000,
      runs: [
        run({
          stage: "done",
          state: "done",
          scanned: 120,
          total: 120,
          done: 120,
          imported: 117,
          skipped: 3,
          duplicates: 2,
          failed: 1,
        }),
      ],
    }),
};

export function ImportProgressDemo() {
  const [revealCount, setRevealCount] = createSignal(0);
  let handlers: ((progress: ImportBatchProgress) => void)[] = [];
  let latest: ImportBatchProgress = snapshot();

  const api: ImportApi = {
    async start() {
      return { batchId: "demo", runs: [{ index: 0, sourceRoot: "D:\\照片\\2026 旅行" }] };
    },
    async pause() {
      return SCRIPTS.paused();
    },
    async resume() {
      return SCRIPTS.running();
    },
    async cancel() {
      return { ...SCRIPTS.paused(), state: "cancelled", finishedAt: T0 + 5_000 };
    },
    async status() {
      return latest;
    },
    async exportErrors() {
      return 1;
    },
    async subscribe(handler) {
      handlers.push(handler);
      return () => {
        handlers = handlers.filter((item) => item !== handler);
      };
    },
  };

  const store = createImportStore({ api });

  function push(next: ImportBatchProgress): void {
    latest = next;
    for (const handler of handlers) handler(next);
  }

  onMount(() => {
    // 真 store 的完整流程：开批次 → 订阅 → 取一次快照
    void store
      .begin({
        repositoryId: "demo",
        sources: [{ path: "D:\\照片\\2026 旅行", includeSubdirs: true }],
        avoidDuplicates: true,
      })
      // 挂载完先收起来：画廊里别一上来就占满整页；而且「点击打开」正是真机路径
      // （用户点「导入」→ 弹窗出现），冒烟断言的也就是它。
      .then(() => store.dismiss());
  });

  return (
    <div class="flex flex-col gap-2">
      <div class="flex flex-wrap items-center gap-2">
        {/* 弹窗的开关归 **store** 管（生产里就是 begin() 打开、dismiss() 收起）——
            演示不要再留一份自己的 open，否则两个信号会打架（这里踩过一次） */}
        <Button
          variant={store.open() ? "secondary" : "primary"}
          onClick={() => {
            if (store.open()) store.dismiss();
            else store.openDialog();
          }}
        >
          {store.open() ? "关闭导入弹窗" : "打开导入弹窗"}
        </Button>
        <Show when={store.open()}>
          <Button variant="secondary" onClick={() => push(SCRIPTS.running())}>
            推一条「导入中」
          </Button>
          <Button variant="secondary" onClick={() => push(SCRIPTS.paused())}>
            推一条「已暂停」
          </Button>
          <Button variant="secondary" onClick={() => push(SCRIPTS.done())}>
            推到「结束」
          </Button>
        </Show>
        <Show when={revealCount() > 0}>
          <span class="text-fs-1 text-fg-2">
            点了「在库中查看」{revealCount()} 次（演示里只计数）
          </span>
        </Show>
      </div>

      <ImportProgressDialog
        store={store}
        onRevealInLibrary={() => {
          setRevealCount(revealCount() + 1);
          store.dismiss();
        }}
      />
    </div>
  );
}
