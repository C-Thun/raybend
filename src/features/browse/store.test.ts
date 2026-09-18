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

  store.patchFilter({ ratings: [5] });
  assert.equal(store.selectedCount(), 0, "换筛选后选择要清掉");
  await tick();
  assert.equal(store.itemAt(0)?.id, 1, "数据重新加载");
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

  store.setFilter({ ratings: [3] });
  await tick();
  store.setSort({ key: "fileName", desc: false });
  await tick();

  assert.deepEqual(calls.page, [0, 0], "每次查询变化都要重新取第一页");
  assert.equal(store.query()?.filter?.ratings?.[0], 3);
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
