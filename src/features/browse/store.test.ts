/**
 * 浏览 store 的测试。
 *
 * 重点在三条**真会咬人**的规则（写在 store 头部的说明里）：
 *
 * 1. **迟到的结果要丢掉** —— 换库/换筛选后旧响应不能覆盖新数据；
 * 2. **同一页不能重复请求** —— 滚动会反复问「这段有没有数据」；
 * 3. **换查询要清选择** —— 否则批量操作会作用到看不见的照片上。
 *
 * 另外钉住：分页合并（稀疏表）、标记后本地条目跟着更新、旗标按库隔离、
 * 空库/无库的边界。全部用假 API，不需要 Tauri。
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import type {
  AssetItem,
  BrowseFacets,
  BrowseQuery,
  DeleteResult,
  MarkAction,
  MarkResult,
  MarkingItem,
  TimelineEntry,
} from "../../api/types.ts";
import { createBrowseStore, PAGE_SIZE, type BrowseApi, type BrowseStore } from "./store.ts";

function item(id: number, overrides: Partial<AssetItem> = {}): AssetItem {
  return {
    id,
    relPath: `photos/${id}.jpg`,
    fileName: `${id}.jpg`,
    ext: "jpg",
    isRaw: false,
    hasRaw: false,
    author: null,
    description: null,
    gpsLat: null,
    gpsLon: null,
    country: null,
    provinceState: null,
    city: null,
    sublocation: null,
    createdMs: null,
    takenAt: 1_789_516_800_000 + id * 1000,
    takenAtOffsetMin: null,
    rating: 0,
    colorLabel: null,
    likeState: null,
    lockLevel: 0,
    cameraMake: null,
    cameraModel: null,
    lens: null,
    focalMm: null,
    fNumber: null,
    exposureMs: null,
    iso: null,
    width: 5184,
    height: 3888,
    orientation: 1,
    sizeBytes: 1000,
    missing: false,
    ...overrides,
  };
}

const EMPTY_FACETS: BrowseFacets = { ratings: [], colors: [], likes: [], locks: [] };
const EMPTY_MARK: MarkResult = {
  changed: 0,
  skippedLocked: [],
  undoLabel: null,
  redoLabel: null,
  canUndo: false,
  canRedo: false,
};
const EMPTY_DELETE: DeleteResult = {
  deleted: 0,
  blockedLocked: [],
  alreadyGone: 0,
  failed: [],
};

/** 假后端：一个内存里的照片库 + 调用记录。 */
function fakeApi(count: number) {
  const all = Array.from({ length: count }, (_, i) => item(i + 1));
  const calls = { page: [] as number[], timeline: 0, facets: 0, mark: [] as MarkAction[] };
  const marks = new Map<number, MarkingItem>();
  let flagState = { total: 0, picks: [] as number[], rejects: [] as number[] };
  /** 让测试能控制某个请求什么时候返回。 */
  const gates: Array<() => void> = [];

  const api: BrowseApi = {
    // 右栏要按 id 显示标签名 ⇒ store 会拉一次词典；测试里给空词典
    async tagList() {
      return [];
    },
    async page(_query: BrowseQuery, offset: number, limit: number) {
      calls.page.push(offset);
      if (gates.length > 0) {
        await new Promise<void>((resolve) => gates.push(resolve));
      }
      const slice = all.slice(offset, offset + limit);
      return { total: all.length, offset, items: slice };
    },
    async timeline(): Promise<{ total: number; entries: TimelineEntry[] }> {
      calls.timeline += 1;
      return {
        total: all.length,
        entries: all.map((a) => ({
          id: a.id,
          // 路径用 id 合成：片内排序要按文件名自然序，测试里也给它一个真名字
          relPath: `photos/P${String(a.id).padStart(4, "0")}.JPG`,
          takenAt: a.takenAt,
        })),
      };
    },
    async facets(): Promise<BrowseFacets> {
      calls.facets += 1;
      return EMPTY_FACETS;
    },
    async markings(_id: string, ids: readonly number[]): Promise<MarkingItem[]> {
      return ids.map(
        (id) =>
          marks.get(id) ?? {
            id,
            rating: all[id - 1]?.rating ?? 0,
            colorLabel: all[id - 1]?.colorLabel ?? null,
            likeState: all[id - 1]?.likeState ?? null,
            lockLevel: all[id - 1]?.lockLevel ?? 0,
            tagIds: [],
          },
      );
    },
    async mark(_id: string, ids: readonly number[], action: MarkAction): Promise<MarkResult> {
      calls.mark.push(action);
      for (const id of ids) {
        const current =
          marks.get(id) ??
          { id, rating: 0, colorLabel: null, likeState: null, lockLevel: 0, tagIds: [] };
        const next =
          action.kind === "rating"
            ? { ...current, rating: action.value }
            : action.kind === "color"
              ? { ...current, colorLabel: action.value }
              : current;
        marks.set(id, next);
      }
      return { ...EMPTY_MARK, changed: ids.length };
    },
    async undo(): Promise<MarkResult> {
      return EMPTY_MARK;
    },
    async redo(): Promise<MarkResult> {
      return EMPTY_MARK;
    },
    async remove(): Promise<DeleteResult> {
      return EMPTY_DELETE;
    },
    async flagsGet() {
      return flagState;
    },
    async flagsSet(_id: string, ids: readonly number[], flag: "pick" | "reject" | null) {
      const picks = new Set(flagState.picks);
      const rejects = new Set(flagState.rejects);
      for (const id of ids) {
        picks.delete(id);
        rejects.delete(id);
        if (flag === "pick") picks.add(id);
        if (flag === "reject") rejects.add(id);
      }
      flagState = { total: picks.size + rejects.size, picks: [...picks], rejects: [...rejects] };
      return flagState;
    },
    async flagsClear() {
      flagState = { total: 0, picks: [], rejects: [] };
      return flagState;
    },
  };

  return {
    api,
    calls,
    setMarking(id: number, marking: MarkingItem) {
      marks.set(id, marking);
    },
    /** 往库里添一张（模拟「导入刚落地」）—— 刷新看得见新照片那条要用 */
    addItem(entry: AssetItem) {
      all.push(entry);
    },
    /** 从库里删掉某张（模拟「在别处被删了/不再符合筛选」）—— 选择修剪那条要用 */
    removeItem(id: number) {
      const at = all.findIndex((entry) => entry.id === id);
      if (at >= 0) all.splice(at, 1);
    },
    resetCalls() {
      calls.page = [];
      calls.timeline = 0;
      calls.facets = 0;
      calls.mark = [];
    },
  };
}

async function tick(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

/**
 * 选库 + 选目录 —— 进入浏览是**两步**（人类 2026-09-18 定：点库不铺整库照片，
 * 必须再点库下的一个目录）。`scope` 是库内相对路径（`photos/2026-08-15`）。
 */
function open(store: BrowseStore, id = "RepoA", scope = "photos/2026-08-15"): void {
  store.setRepository(id);
  store.setScope(scope);
}

// ─────────────────────────── 空态与边界 ───────────────────────────

test("没有库时不发任何请求", async () => {
  const { api, calls } = fakeApi(3);
  const store = createBrowseStore({ api });
  await store.reload();
  assert.equal(store.total(), 0);
  assert.equal(store.query(), null);
  assert.deepEqual(calls.page, []);
});

test("空库：total 为 0，没有条目，也不算错误", async () => {
  const { api } = fakeApi(0);
  const store = createBrowseStore({ api });
  open(store);
  await tick();
  assert.equal(store.total(), 0);
  assert.equal(store.itemAt(0), null);
  assert.equal(store.error(), null);
});

// ─────────────────────────── 加载与分页 ───────────────────────────

test("selectAll：覆盖**整个 scope**（时间线全量），不只是已加载的那一页", async () => {
  /*
   * 人类 2026-09-20：「tiles 里要支持 ctrl+a 全选，**即使未显示的部分也要设置选中状态**」。
   *
   * 列表是**按页取**的（`PAGE_SIZE`），所以「只选已加载的那些」是个很容易犯的错 ——
   * 这条用一个跨页的库把它钉住：加载只有第一页，全选必须覆盖 `total` 那么多张。
   */
  const total = PAGE_SIZE + 5;
  const { api } = fakeApi(total);
  const store = createBrowseStore({ api });
  open(store);
  await tick();

  assert.equal(store.total(), total, "fixture 要跨页，否则这条测不到东西");
  let loaded = 0;
  for (let i = 0; i < total; i += 1) if (store.itemAt(i) !== null) loaded += 1;
  assert.ok(loaded <= PAGE_SIZE, `已加载的应当只有第一页（实测 ${loaded}）`);

  store.selectAll();
  assert.equal(
    store.selection().ids.size,
    total,
    "全选要把整个范围里的每一张都选上（含没加载出来的）",
  );
});



test("选了库但还没选目录：不发任何查询，网格是「请选目录」的空态", async () => {
  const { api, calls } = fakeApi(10);
  const store = createBrowseStore({ api });
  store.setRepository("RepoA");
  await tick();

  assert.equal(store.query(), null, "没选目录就不该构造查询");
  assert.equal(store.total(), 0);
  assert.equal(store.timeline().length, 0);
  assert.deepEqual(calls.page, [], "点库本身不该把整库照片铺出来");
  assert.equal(calls.timeline, 0);

  // 再点一个目录 —— 这时才真的去读库
  store.setScope("photos/2026-08-15");
  await tick();
  assert.equal(store.total(), 10);
  assert.deepEqual(calls.page, [0]);
});

test("setRepository 会加载第一页、时间线与分面", async () => {
  const { api, calls } = fakeApi(10);
  const store = createBrowseStore({ api });
  open(store);
  await tick();
  assert.equal(store.total(), 10);
  assert.equal(store.itemAt(0)?.id, 1);
  assert.equal(store.timeline().length, 10);
  assert.notEqual(store.facets(), null);
  assert.deepEqual(calls.page, [0]);
  assert.equal(calls.timeline, 1);
});

test("ensureRange 只请求缺的那几页，且同一页不重复请求", async () => {
  const { api, calls } = fakeApi(2000);
  const store = createBrowseStore({ api });
  open(store);
  await tick();
  calls.page = [];

  // 第一页已经在手里（首屏加载过）
  await store.ensureRange(0, 10);
  assert.deepEqual(calls.page, [], "已有数据不该再请求");

  await store.ensureRange(0, PAGE_SIZE * 2 + 5);
  assert.deepEqual(calls.page, [PAGE_SIZE, PAGE_SIZE * 2], "只补缺的两页");

  await store.ensureRange(0, PAGE_SIZE * 2 + 5);
  assert.deepEqual(calls.page, [PAGE_SIZE, PAGE_SIZE * 2], "再问一遍也不该重复请求");
});

test("分页数据合进稀疏表：未加载的位置是 null", async () => {
  const { api } = fakeApi(1000);
  const store = createBrowseStore({ api });
  open(store);
  await tick();
  await store.ensureRange(PAGE_SIZE * 3, PAGE_SIZE * 3 + 4);

  assert.equal(store.total(), 1000);
  assert.equal(store.itemAt(0)?.id, 1, "首屏加载的第一页在");
  assert.equal(store.itemAt(PAGE_SIZE), null, "没请求过的页就是空的（稀疏表）");
  assert.equal(store.itemAt(PAGE_SIZE * 2), null);
  assert.equal(store.itemAt(PAGE_SIZE * 3)?.id, PAGE_SIZE * 3 + 1, "请求过的那页在");
});

// ─────────────────────────── 刷新（同查询重读）───────────────────────────

/*
 * 这一组盯的是人类 2026-09-22 报的那条：
 * 「先进 browse，再去导入，导入的文件没有出现在列表里，得关了程序再进才行」。
 *
 * 根因是列表只在**查询变化**时重读 —— 于是 `refresh`（挂在工作区挂载时）
 * 必须做到两件互相矛盾的事：**把数据换成最新的**，但不把用户的东西碰掉
 *（选择、已铺出来的内容）。下面五条就是这五个面。
 */

test("refresh：把新导入的照片读进来", async () => {
  const { api, addItem } = fakeApi(3);
  const store = createBrowseStore({ api });
  open(store);
  await tick();
  assert.equal(store.total(), 3);

  // 用户去导入了一圈：库里多了一张（真实程序里这一步由导入自己写库）
  addItem(item(4));
  await store.refresh();

  assert.equal(store.total(), 4);
  assert.equal(store.itemAt(3)?.id, 4, "新照片必须出现在列表里");
});

test("refresh：不清空已有内容（刷新没回来之前照旧显示上一份）", async () => {
  const { api } = fakeApi(3);
  const store = createBrowseStore({ api });
  open(store);
  await tick();

  // 卡住刷新用的那一次取页（时间线与分面会立刻回来，但 Promise.all 要等它）
  const gate: Array<() => void> = [];
  const originalPage = api.page;
  api.page = async (query, offset, limit) => {
    await new Promise<void>((resolve) => gate.push(resolve));
    return originalPage(query, offset, limit);
  };
  const pending = store.refresh();
  await tick();

  assert.equal(store.total(), 3, "刷新期间显示的仍是上一份内容（清空会白一下）");
  assert.equal(store.itemAt(0)?.id, 1);

  for (const release of gate) release();
  await pending;
  assert.equal(store.itemAt(0)?.id, 1, "刷新完成后是第一页的新数据");
});

test("refresh：不弄丢选择，只剔掉真的不在列表里的", async () => {
  const { api, removeItem } = fakeApi(5);
  const store = createBrowseStore({ api });
  open(store);
  await tick();

  store.select(2, "replace");
  store.select(4, "toggle");
  assert.deepEqual([...store.selection().ids].sort(), ["2", "4"]);

  removeItem(4); // 这张在别处没了（删了，或者不再符合筛选）
  await store.refresh();

  assert.deepEqual([...store.selection().ids], ["2"], "还在的照样选中，消失的剔掉");
  assert.equal(store.anchorItem()?.id, 2, "锚点被剔掉后退回第一张选中的");
});

test("refresh：第一页之外的页作废（新照片会把下标推移）", async () => {
  const { api, addItem } = fakeApi(1000);
  const store = createBrowseStore({ api });
  open(store);
  await tick();
  await store.ensureRange(0, PAGE_SIZE + 2);
  assert.equal(store.itemAt(PAGE_SIZE)?.id, PAGE_SIZE + 1, "先真的取回第二页");

  addItem(item(9999));
  await store.refresh();

  assert.equal(store.total(), 1001, "总数跟着库里变了");
  assert.equal(store.itemAt(0)?.id, 1, "第一页是新的");
  assert.equal(
    store.itemAt(PAGE_SIZE),
    null,
    "旧的第二页必须作废 —— 它的下标可能已经挪过了（留着会显示错人）",
  );

  // 用户滚下去时按需重取
  await store.ensureRange(PAGE_SIZE, PAGE_SIZE + 1);
  assert.equal(store.itemAt(PAGE_SIZE)?.id, PAGE_SIZE + 1, "重取之后就有内容了");
});

test("refresh：只读一趟首屏，不把已加载的页逐页重取", async () => {
  /*
   * 这是**成本口径**钉在测试里（人类 2026-09-22 问过这笔账）：
   * 刷新 = 再付一次「进目录首屏」的钱（10 万条挤一个目录时量到
   * 取页 0.5ms + 时间线 P95 142ms + 分面 P95 79ms）；
   * 而不是「把用户滚过的每一页都重读一遍」那种精细活。
   */
  const { api, calls } = fakeApi(2000);
  const store = createBrowseStore({ api });
  open(store);
  await tick();
  await store.ensureRange(PAGE_SIZE * 2, PAGE_SIZE * 2 + 1);
  calls.page = [];
  calls.timeline = 0;
  calls.facets = 0;

  await store.refresh();

  assert.deepEqual(calls.page, [0], "只读第一页");
  assert.equal(calls.timeline, 1, "时间线一次");
  assert.equal(calls.facets, 1, "分面一次");
});

test("refresh：把在飞的旧分页请求作废（迟到的响应不会盖回去）", async () => {
  const { api } = fakeApi(2000);
  const store = createBrowseStore({ api });
  open(store);
  await tick();

  // 卡住第二页那次请求
  const gate: Array<() => void> = [];
  const originalPage = api.page;
  api.page = async (query, offset, limit) => {
    if (offset > 0) await new Promise<void>((resolve) => gate.push(resolve));
    return originalPage(query, offset, limit);
  };
  const range = store.ensureRange(PAGE_SIZE, PAGE_SIZE + 1);
  await tick();
  await store.refresh();

  // 现在放行那个属于刷新之前的第二页
  for (const release of gate) release();
  await range;

  assert.equal(
    store.itemAt(PAGE_SIZE),
    null,
    "按刷新之前的名单取回来的页必须丢掉",
  );
});

test("refresh：没选库/没选目录时什么也不读", async () => {
  const { api, calls } = fakeApi(3);
  const store = createBrowseStore({ api });
  await store.refresh();

  assert.deepEqual(calls.page, []);
  assert.equal(calls.timeline, 0);
  assert.equal(calls.facets, 0);
  assert.equal(store.total(), 0);
  assert.equal(store.loading(), false, "别把「加载中」留在屏幕上");
});

// ─────────────────────────── 迟到的结果 ───────────────────────────

test("换库之后，旧库迟到的结果会被丢掉", async () => {
  const { api } = fakeApi(50);
  const store = createBrowseStore({ api });

  // 让第一次请求挂起
  const gate: Array<() => void> = [];
  const originalPage = api.page;
  api.page = async (query, offset, limit) => {
    await new Promise<void>((resolve) => gate.push(resolve));
    return originalPage(query, offset, limit);
  };

  open(store, "Slow");
  await tick();
  open(store, "Fast");
  await tick();

  // 放行所有挂起的请求（此时 Slow 的结果已经过期）
  for (const release of gate) release();
  await tick();
  await tick();

  assert.equal(store.repositoryId(), "Fast");
  assert.equal(store.total(), 50, "最终显示的是新库的数据");
});

test("换筛选会清空已加载的窗口与选择", async () => {
  const { api } = fakeApi(20);
  const store = createBrowseStore({ api });
  open(store);
  await tick();
  store.select(3, "replace");
  assert.equal(store.selectedCount(), 1);

  store.patchFilter({ minRating: 5 });
  assert.equal(store.selectedCount(), 0, "换筛选后选择要清掉");
  await tick();
  assert.equal(store.itemAt(0)?.id, 1, "数据重新加载");
});

test("anchorItem：\u300c\u5f53\u524d\u90a3\u5f20\u300d\u7684\u89c4\u5219\u53ea\u6b64\u4e00\u5904\uff08\u591a\u9009\u65f6\u7ed9\u951a\u70b9\uff0c\u6ca1\u9009\u5c31\u662f null\uff09", async () => {
  // \u4eba\u7c7b 2026-09-19\uff1a\u8fd9\u6761\u89c4\u5219\u4ee5\u524d\u540c\u65f6\u5199\u5728\u5de5\u4f5c\u533a\u4e0e\u7ec4\u88c5\u5c42\uff08flowinfo \u4e5f\u8981\u7528\uff09\u2014\u2014\u73b0\u5728\u53ea\u80fd\u4ece\u8fd9\u91cc\u8bfb
  const { api } = fakeApi(5);
  const store = createBrowseStore({ api });
  open(store);
  await tick();

  assert.equal(store.anchorItem(), null, "\u4e00\u5f20\u6ca1\u9009 \u2192 null");

  store.select(2, "replace");
  assert.equal(store.anchorItem()?.id, 2, "\u5355\u9009\u5c31\u662f\u5b83\u81ea\u5df1");

  store.select(4, "toggle");
  assert.equal(store.anchorItem()?.id, 4, "\u65b0\u9009\u7684\u90a3\u5f20\u6210\u4e3a\u951a\u70b9");

  store.setAnchor(2);
  assert.equal(store.anchorItem()?.id, 2, "\u951a\u70b9\u53ef\u4ee5\u663e\u5f0f\u6307\u5b9a\uff08\u5bf9\u6bd4\u6001\u8981\u7528\uff09");

  store.clearSelection();
  assert.equal(store.anchorItem(), null, "\u6e05\u7a7a\u4e4b\u540e\u56de\u5230 null");
});

test("换范围（目录）也会清选择", async () => {
  const { api } = fakeApi(20);
  const store = createBrowseStore({ api });
  open(store);
  await tick();
  store.select(2, "replace");
  store.setScope("photos/2026-08-16");
  assert.equal(store.selectedCount(), 0);
});

// ─────────────────────────── 选择 ───────────────────────────

test("点击 / Ctrl / Shift 三种模式按 BROWSE.md 的语义走", async () => {
  const { api } = fakeApi(5);
  const store = createBrowseStore({ api });
  open(store);
  await tick();

  store.select(1, "replace");
  assert.deepEqual(store.selectedIds(), [1]);

  store.select(3, "replace");
  assert.deepEqual(store.selectedIds(), [3], "无修饰键点击 = 只选这一张");

  store.select(5, "range");
  assert.deepEqual(
    store.selectedIds().sort((a, b) => a - b),
    [3, 4, 5],
    // 锚点（3）**不翻转** ⇒ 它保持选中；3 与 5 之间的 4、5 翻转（4 变选中、5 变选中）
    "区间翻转：不含锚点、含本次点中的那张",
  );

  store.select(4, "toggle");
  assert.deepEqual(store.selectedIds().sort((a, b) => a - b), [3, 5], "Ctrl 再点一次是反选");

  store.selectAll();
  assert.equal(store.selectedCount(), 5);
  store.clearSelection();
  assert.equal(store.selectedCount(), 0);
});

test("选不了没加载到的照片（选择模型只看得到当前列表）", async () => {
  const { api } = fakeApi(1000);
  const store = createBrowseStore({ api });
  open(store);
  await tick();
  // 只补第 3 页；第 1、2 页仍然是「没请求过」
  await store.ensureRange(PAGE_SIZE * 3, PAGE_SIZE * 3 + 2);

  // 加载过的那张可以选
  store.select(PAGE_SIZE * 3 + 1, "replace");
  assert.equal(store.selectedCount(), 1);
  assert.equal(store.selectedItems().length, 1);

  // 没加载到的那张选不了 —— 照片网格只渲染可见的格子，
  // 用户不可能点到看不见的格子，所以「点不动」才是对的（选择保持不变）
  store.select(PAGE_SIZE + 1, "replace");
  assert.deepEqual(
    store.selectedIds(),
    [PAGE_SIZE * 3 + 1],
    "第 1 页还没加载：这次点击不产生任何变化",
  );
});

// ─────────────────────────── 标记 ───────────────────────────

test("标记之后本地条目与标记缓存都跟着更新", async () => {
  const { api, calls } = fakeApi(5);
  const store = createBrowseStore({ api });
  open(store);
  await tick();

  store.select(2, "replace");
  await tick();
  const result = await store.mark({ kind: "rating", value: 4 });
  assert.equal(result?.changed, 1);
  assert.deepEqual(calls.mark, [{ kind: "rating", value: 4 }]);
  assert.equal(store.itemAt(1)?.rating, 4, "网格上的星标要立刻反映出来");
  assert.equal(store.markings().get(2)?.rating, 4, "三态控件读的是这份缓存");
});

test("没选中任何照片时打标记是无操作", async () => {
  const { api, calls } = fakeApi(5);
  const store = createBrowseStore({ api });
  open(store);
  await tick();
  const result = await store.mark({ kind: "rating", value: 3 });
  assert.equal(result, null);
  assert.deepEqual(calls.mark, []);
});

test("标记会把后端返回的「被锁跳过」原样带回来", async () => {
  const { api } = fakeApi(5);
  api.mark = async () => ({ ...EMPTY_MARK, changed: 1, skippedLocked: [3] });
  const store = createBrowseStore({ api });
  open(store);
  await tick();
  store.selectAll();
  const result = await store.mark({ kind: "rating", value: 5 });
  assert.deepEqual(result?.skippedLocked, [3]);
});

// ─────────────────────────── 旗标 ───────────────────────────

test("旗标是内存态：打上、读回、清空", async () => {
  const { api } = fakeApi(5);
  const store = createBrowseStore({ api });
  open(store);
  await tick();

  await store.setFlag([1, 2], "pick");
  assert.deepEqual([...store.picks()].sort((a, b) => a - b), [1, 2]);
  assert.equal(store.rejects().size, 0);

  await store.setFlag([1], "reject");
  assert.deepEqual([...store.picks()], [2], "两态互斥：改成 reject 就不在 pick 里了");
  assert.deepEqual([...store.rejects()], [1]);

  await store.setFlag([2], null);
  assert.equal(store.picks().size, 0);

  await store.clearFlags();
  assert.equal(store.picks().size, 0);
  assert.equal(store.rejects().size, 0);
});

test("换库会重新拉旗标（旗标跨库，但界面只看当前库）", async () => {
  const { api } = fakeApi(5);
  const store = createBrowseStore({ api });
  open(store);
  await tick();
  await store.setFlag([1], "pick");

  open(store, "RepoB");
  await tick();
  await tick();
  assert.equal(store.repositoryId(), "RepoB");
  // 假后端把所有库的旗标混在一起，这里只要求「换库后会去问一次」
  assert.equal(store.picks().has(1), true);
});

// ─────────────────────────── 设置 ───────────────────────────

test("setFilter / setSort 会带上完整查询重新加载", async () => {
  const { api, calls } = fakeApi(5);
  const store = createBrowseStore({ api });
  open(store);
  await tick();
  calls.page = [];

  store.setFilter({ minRating: 3 });
  await tick();
  store.setSort({ key: "fileName", desc: false });
  await tick();

  assert.deepEqual(calls.page, [0, 0], "每次查询变化都要重新取第一页");
  assert.equal(store.query()?.filter?.minRating, 3);
  assert.equal(store.query()?.sort?.key, "fileName");
});

test("重复设同一个库不会重复加载", async () => {
  const { api, calls } = fakeApi(5);
  const store = createBrowseStore({ api });
  open(store);
  await tick();
  calls.page = [];
  open(store);
  await tick();
  assert.deepEqual(calls.page, []);
});

// ─────────────────── 显示序映射（片内自然序交给 store）───────────────────

test("可见顺序由调用方给：Shift 区间选择跟着它走（数据层不猜显示规则）", async () => {
  /*
   * 人类 2026-09-18 定的边界：分组、片内排序都是**显示规则**，
   * 所以顺序由显示层算好传进来；store 只按传进来的顺序做区间展开。
   * 这里用「2、1、3…」这个顺序模拟「片内按文件名重排」的结果。
   */
  const { api } = fakeApi(6);
  const store = createBrowseStore({ api });
  open(store);
  await tick();

  const visible = ["2", "1", "3", "4", "5", "6"];
  store.select(2, "replace", visible);
  store.select(1, "range", visible);
  assert.deepEqual(store.selectedIds().sort((a, b) => a - b), [1, 2]);

  // 不传顺序 → 退回查询顺序（只有未分组时才是对的，所以调用方必须传）
  store.clearSelection();
  store.select(1, "replace");
  assert.equal(store.itemAt(0)?.id, 1, "数据层仍按查询顺序给数据");
});

test("按需取数只认数据下标：可见区间给什么就取什么页", async () => {
  // 显示序是置换时，取数区间由显示层换算好（`BrowseGrid.rowIndexRange` 取 min..max 超集），
  // store 这边只按数据下标算页 —— 它不知道也不该知道显示顺序。
  const { api, calls } = fakeApi(600);
  const store = createBrowseStore({ api });
  open(store);
  await tick();

  calls.page = [];
  await store.ensureRange(599, 600);
  assert.ok(
    calls.page.includes(512),
    `第 599 个数据下标落在第 2 页（offset 512），实际取了 ${calls.page.join(",")}`,
  );
  assert.ok(!calls.page.includes(0), "第 0 页已经加载过，不该重复取");
});

// ─────────────────── 边界：显示规则不许长回数据层（人类 2026-09-18 定）───────────────────

test("数据层只剩「查询顺序」：没有显示序的入口（分组/排序归显示层）", async () => {
  /*
   * 人类原话：「排序在显示端做是正确的，数据源怎么可能需要去理解展示的逻辑。」
   * 上一版为了实现「片内按文件名自然序」，在 store 里塞过 `setDisplayOrder()` / `at()`，
   * 于是「取下来的页」与「屏幕上那一格」可能错位。这两条断言把它钉住：
   * 谁要再加，先回来看这段注释。
   */
  const { api } = fakeApi(5);
  const store = createBrowseStore({ api });
  const surface = store as unknown as Record<string, unknown>;
  assert.equal(surface["setDisplayOrder"], undefined, "不许把显示序灌进数据层");
  assert.equal(surface["at"], undefined, "不许在数据层按下标取「显示用的那一张」");
  // 显示层要的顺序由调用方显式传（`select(id, mode, order)` / `ensureRange(数据下标)`）
  assert.equal(typeof store.ensureRange, "function");
  assert.equal(typeof store.itemAt, "function");
});

// ─────────────────── 只挪锚点（对比视图点某一幅画幅） ───────────────────

test("setAnchor：锚点挪到**已选中**的那张上，选择集合一点不动", async () => {
  const { api } = fakeApi(5);
  const store = createBrowseStore({ api });
  open(store);
  await tick();

  store.select(1, "replace");
  store.select(2, "toggle");
  store.select(3, "toggle");
  assert.deepEqual(store.selectedIds(), [1, 2, 3]);
  assert.equal(store.anchorId(), 3, "最后一次点中的是锚点");

  store.setAnchor(1);
  assert.equal(store.anchorId(), 1);
  assert.deepEqual(store.selectedIds(), [1, 2, 3], "选择集合保持不变（否则对比会散掉）");
});

test("setAnchor：目标不在选择集合里就**不动**（锚点必须是被选中的那张）", async () => {
  const { api } = fakeApi(5);
  const store = createBrowseStore({ api });
  open(store);
  await tick();

  store.select(2, "replace");
  assert.equal(store.anchorId(), 2);
  store.setAnchor(4);
  assert.equal(store.anchorId(), 2, "没被选中的图不能当锚点");
  assert.deepEqual(store.selectedIds(), [2]);
});

test("setAnchor：没有选择时也不动", async () => {
  const { api } = fakeApi(3);
  const store = createBrowseStore({ api });
  open(store);
  await tick();

  store.clearSelection();
  store.setAnchor(1);
  assert.equal(store.anchorId(), null);
  assert.deepEqual(store.selectedIds(), []);
});

// ─────────────────── 删除（走回收站） ───────────────────

test("removeSelected：删完清空选择、重新取一遍页面数据、把结果原样带回", async () => {
  const { api, calls } = fakeApi(6);
  const store = createBrowseStore({ api });
  open(store);
  await tick();
  const pagesBefore = calls.page.length;

  store.select(1, "replace");
  store.select(2, "toggle");
  const result = await store.removeSelected();

  assert.deepEqual(result, EMPTY_DELETE, "后端回什么就带什么（界面按它决定提示）");
  assert.deepEqual(store.selectedIds(), [], "删完选择清空");
  assert.ok(
    calls.page.length > pagesBefore,
    "删完要重新取一遍（否则网格上还留着已经删掉的照片）",
  );
  assert.equal(store.error(), null);
});

// ─────────────────── 真实宽高：老库那批 NULL 的兜底 ───────────────────

/** 一个「老库」：条目的 width/height 是 NULL（2026-09-18 之前的导入就是这种）。 */
function legacyApi(count: number) {
  const all = Array.from({ length: count }, (_, i) =>
    item(i + 1, { width: null, height: null }),
  );
  const asked: { dir: string; names: string[] }[] = [];
  const api: BrowseApi = {
    async page(_query: BrowseQuery, offset: number, limit: number) {
      return { total: all.length, offset, items: all.slice(offset, offset + limit) };
    },
    async timeline() {
      return {
        total: all.length,
        entries: all.map((a) => ({
          id: a.id,
          relPath: a.relPath,
          takenAt: a.takenAt,
        })),
      };
    },
    async facets(): Promise<BrowseFacets> {
      return EMPTY_FACETS;
    },
    async tagList() {
      return [];
    },
    async markings() {
      return [];
    },
    async mark() {
      return EMPTY_MARK;
    },
    async undo() {
      return EMPTY_MARK;
    },
    async redo() {
      return EMPTY_MARK;
    },
    async remove() {
      return EMPTY_DELETE;
    },
    async flagsGet() {
      return { total: all.length, picks: [], rejects: [] };
    },
    async flagsSet() {
      return { total: all.length, picks: [], rejects: [] };
    },
    async flagsClear() {
      return { total: all.length, picks: [], rejects: [] };
    },
  };
  return { api, asked, all };
}

test("naturalOf：数据库里没宽高时返回 null（界面按占位比例显示，不报错）", async () => {
  const { api } = legacyApi(3);
  const store = createBrowseStore({ api });
  open(store);
  await tick();
  assert.equal(store.naturalOf(1), null);
});

test("naturalOf：数据库里有宽高就直接用（不必补读）", async () => {
  const { api } = fakeApi(3);
  const store = createBrowseStore({ api });
  open(store);
  await tick();
  assert.deepEqual(store.naturalOf(1), { width: 5184, height: 3888 });
});

test("ensureNatural：把补读回来的宽高**合并**进缓存，并只问缺的那些", async () => {
  const { api } = legacyApi(3);
  const asked: { dir: string; names: string[] }[] = [];
  const store = createBrowseStore({
    api,
    metaEnsure: async (dir, files) => {
      asked.push({ dir, names: files.map((file) => file.relative) });
      return files.map((file) => ({
        relative: file.relative,
        width: 4000,
        height: 3000,
        orientation: 1,
      }));
    },
  });
  open(store);
  await tick();

  await store.ensureNatural([
    { id: 1, path: "D:/lib/photos/1.jpg" },
    { id: 2, path: "D:/lib/photos/2.jpg" },
  ]);
  assert.deepEqual(store.naturalOf(1), { width: 4000, height: 3000 });
  assert.deepEqual(store.naturalOf(2), { width: 4000, height: 3000 });
  assert.equal(asked.length, 1, "同一个目录只问一次");
  assert.deepEqual(asked[0]?.names, ["1.jpg", "2.jpg"]);
  assert.equal(asked[0]?.dir, "D:/lib/photos");

  // 再问一次：已经有了，不该再打扰后端
  await store.ensureNatural([{ id: 1, path: "D:/lib/photos/1.jpg" }]);
  assert.equal(asked.length, 1, "已有宽高就不该重复读盘");
});

test("ensureNatural：不同目录分开问；读不到不报错（静默退回占位）", async () => {
  const { api } = legacyApi(2);
  const asked: string[] = [];
  const store = createBrowseStore({
    api,
    metaEnsure: async (dir, files) => {
      asked.push(dir);
      if (dir.endsWith("bad")) throw new Error("读不了");
      return files.map((file) => ({
        relative: file.relative,
        width: 0,
        height: 0,
        orientation: 1,
      }));
    },
  });
  open(store);
  await tick();

  await store.ensureNatural([
    { id: 1, path: "D:/lib/photos/1.jpg" },
    { id: 2, path: "D:/lib/bad/2.jpg" },
  ]);
  assert.deepEqual(asked.sort(), ["D:/lib/bad", "D:/lib/photos"]);
  assert.equal(store.naturalOf(1), null, "宽高为 0 的读数不算数");
  assert.equal(store.naturalOf(2), null, "抛错也不影响后续");
});

test("ensureNatural：没有注入补读口子时静默不动（浏览器预览）", async () => {
  const { api } = legacyApi(1);
  const store = createBrowseStore({ api });
  open(store);
  await tick();
  await store.ensureNatural([{ id: 1, path: "D:/lib/photos/1.jpg" }]);
  assert.equal(store.naturalOf(1), null);
});

// ─────────────────── 旗标筛选：id 由 store 填 ───────────────────

test("旗标条件：发查询时把当前旗标集合填进 ids（界面只给 mode）", async () => {
  const { api } = fakeApi(5);
  const store = createBrowseStore({ api });
  open(store);
  await tick();

  await store.setFlag([1, 2], "pick");
  store.patchFilter({ flag: { mode: "pick" } });
  let sent = store.query()?.filter?.flag;
  assert.equal(sent?.mode, "pick");
  assert.deepEqual([...(sent?.ids ?? [])].sort(), [1, 2], "有旗标 ⇒ 只看这些 id");

  store.patchFilter({ flag: { mode: "none" } });
  sent = store.query()?.filter?.flag;
  assert.equal(sent?.mode, "none");
  assert.deepEqual([...(sent?.ids ?? [])].sort(), [1, 2], "无旗标 ⇒ 排除有旗标的那些");

  store.patchFilter({ flag: null });
  assert.equal(store.query()?.filter?.flag ?? null, null, "取消条件后不带旗标条件");
});
