/**
 * 导入工作区共享状态的单元测试。
 *
 * 重点覆盖三类**真会咬人**的地方：
 *   1. **选中与勾选互不影响**（`AGENTS.md` §11.2 / `DESIGN.md` §12.4）——
 *      合并两者是最容易犯、也最难在界面上看出问题的错；
 *   2. **跨面板的路径比较**（`D:\Photos` 与 `d:/photos/` 是同一个目录）；
 *   3. **异步的计数与「最近」写入**：计数没回来时不能编一个数字出来。
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import type { DirEntry, RecentDir, RepositoryView, Volume } from "../../api/types.ts";
import { createImportStore, errorText, type ImportApi } from "./store.ts";

/* ══════════════════════════════════════════════════════════════
 * 测试替身
 * ══════════════════════════════════════════════════════════════ */

interface FakeState {
  recent: RecentDir[];
  repositories: RepositoryView[];
  volumes: Volume[];
  counts: Map<string, number>;
  /** 记录调用顺序（断言「勾选时记了一条最近」这类行为） */
  calls: string[];
  /** 让计数挂起（模拟大目录还在数） */
  holdCounts: boolean;
  /** 让「记一条最近」报错 */
  failRecent: boolean;
  /** 让「读最近列表」报错 */
  failListRecent: boolean;
  failCounts: boolean;
  /** 让「枚列驱动器」报错 */
  failVolumes: boolean;
  /** 重挂载能不能找到库 */
  remountFinds: boolean;
  /** 让重挂载报错 */
  failRemount: boolean;
  /** 设置表 */
  settings: Map<string, string>;
}

function fakeApi(overrides: Partial<FakeState> = {}) {
  const state: FakeState = {
    recent: [],
    repositories: [],
    volumes: [],
    counts: new Map(),
    calls: [],
    holdCounts: false,
    failRecent: false,
    failListRecent: false,
    failCounts: false,
    failVolumes: false,
    remountFinds: false,
    failRemount: false,
    settings: new Map(),
    ...overrides,
  };
  const pending: Array<() => void> = [];

  const api: ImportApi = {
    async listRecentDirs() {
      state.calls.push("listRecentDirs");
      if (state.failListRecent) throw new Error("读不了最近列表");
      return [...state.recent];
    },
    async rememberRecentDir(path, includeSubdirs) {
      state.calls.push(`remember:${path}:${includeSubdirs}`);
      if (state.failRecent) throw new Error("最近列表写不进去");
      const folded = path.toLowerCase();
      const existing = state.recent.find(
        (row) => row.path.toLowerCase() === folded,
      );
      if (existing) {
        existing.includeSubdirs = includeSubdirs;
        existing.usedAt += 1;
      } else {
        state.recent.unshift({
          path,
          includeSubdirs,
          usedAt: state.recent.length + 1,
          useCount: 1,
        });
      }
    },
    async setRepositoryTemplate(repositoryId, templateSource) {
      state.calls.push(`setTemplate:${repositoryId}:${templateSource}`);
      const row = state.repositories.find((item) => item.id === repositoryId);
      if (row) row.importTemplate = templateSource;
      return { importTemplate: templateSource };
    },
    async forgetRecentDir(path) {
      state.calls.push(`forget:${path}`);
      const before = state.recent.length;
      state.recent = state.recent.filter(
        (row) => row.path.toLowerCase() !== path.toLowerCase(),
      );
      return state.recent.length < before;
    },
    async countSourcePhotos(path, recursive) {
      state.calls.push(`count:${path}:${recursive}`);
      if (state.holdCounts) {
        await new Promise<void>((resolve) => pending.push(resolve));
      }
      if (state.failCounts) throw new Error("目录读不了");
      const key = `${path.toLowerCase()}:${recursive}`;
      return {
        photos: state.counts.get(key) ?? 0,
        skipped: 0,
        truncated: false,
      };
    },
    async listRepositories() {
      state.calls.push("listRepositories");
      return [...state.repositories];
    },
    async remountRepository(repositoryId) {
      state.calls.push(`remount:${repositoryId}`);
      if (state.failRemount) throw new Error("重挂载炸了");
      const found = state.repositories.find((row) => row.id === repositoryId);
      if (!found) throw new Error(`没有这个库：${repositoryId}`);
      return { ...found, online: state.remountFinds };
    },
    async getSetting(key) {
      return state.settings.get(key) ?? null;
    },
    async setSetting(key, value) {
      state.settings.set(key, value);
    },
    async listDirs(path) {
      state.calls.push(`listDirs:${path}`);
      return [] as DirEntry[];
    },
    async listVolumes() {
      state.calls.push("listVolumes");
      if (state.failVolumes) throw new Error("枚列驱动器失败");
      return [...state.volumes];
    },
  };

  return {
    api,
    state,
    /** 放行被 holdCounts 拦住的那一批计数 */
    release() {
      const waiters = pending.splice(0, pending.length);
      for (const resolve of waiters) resolve();
    },
  };
}

/** 让挂起的微任务跑完（假 api 全是立即 resolve 的 Promise） */
async function flush(times = 3): Promise<void> {
  for (let i = 0; i < times; i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
}

function repository(id: string, name = id): RepositoryView {
  return {
    id,
    name,
    importTemplate: null,
    createdAt: 0,
    lastOpenedAt: null,
    online: true,
    root: `/libs/${id}`,
    displayPath: `/libs/${id}`,
    paths: [],
    photoCount: 0,
    triedPaths: 0,
  };
}

/* ══════════════════════════════════════════════════════════════
 * 选中 vs 勾选
 * ══════════════════════════════════════════════════════════════ */

test("选中：同一时间只有一个，且跨面板按折叠路径比较", () => {
  const { api } = fakeApi();
  const store = createImportStore({ api });

  assert.equal(store.selectedDir(), null);
  store.selectDir("D:\\Photos\\2024");
  assert.equal(store.selectedDir(), "D:\\Photos\\2024");
  assert.ok(store.isSelected("d:/photos/2024/"), "写法不同也是同一个目录");
  assert.ok(!store.isSelected("D:\\Photos\\2023"));

  store.selectDir("E:\\别的");
  assert.ok(!store.isSelected("D:\\Photos\\2024"), "选新的会替换旧的");
  assert.equal(store.selectedDir(), "E:\\别的");

  store.selectDir(null);
  assert.equal(store.selectedDir(), null);
  assert.ok(!store.isSelected("E:\\别的"));
});

test("选中与勾选互不影响（不要合并这两件事）", async () => {
  const { api } = fakeApi();
  const store = createImportStore({ api });

  store.selectDir("D:\\Photos");
  assert.ok(!store.isChecked("D:\\Photos"), "选中不等于勾选");
  assert.equal(store.checkedDirs().length, 0);

  store.toggleChecked("E:\\Import");
  assert.ok(store.isChecked("E:\\Import"));
  assert.ok(!store.isSelected("E:\\Import"), "勾选不等于选中");
  assert.equal(store.selectedDir(), "D:\\Photos", "勾选不该动选中");

  await flush();
  // 勾选期间取消选中，勾选状态不受影响
  store.selectDir(null);
  assert.ok(store.isChecked("E:\\Import"));
});

test("勾选：可多选、可移除，且大小写不同的同一目录不会重复", async () => {
  const { api, state } = fakeApi();
  const store = createImportStore({ api });

  store.toggleChecked("D:\\Photos");
  store.toggleChecked("E:\\2024");
  assert.equal(store.checkedDirs().length, 2);

  store.toggleChecked("d:/photos/"); // 与第一条是同一个目录 → 取消勾选
  assert.equal(store.checkedDirs().length, 1);
  assert.ok(!store.isChecked("D:\\Photos"));
  assert.ok(store.isChecked("E:\\2024"));

  store.removeChecked("E:\\2024");
  assert.equal(store.checkedDirs().length, 0);
  await flush();
  assert.ok(state.calls.includes("remember:E:\\2024:false"));
});

test("勾选默认不透传子目录（design/main.md §3.1.3）", async () => {
  const { api, state } = fakeApi();
  const store = createImportStore({ api });

  store.toggleChecked("D:\\Photos");
  await flush();
  const entry = store.checkedDirs()[0];
  assert.equal(entry.includeSubdirs, false);
  assert.ok(
    state.calls.includes("count:D:\\Photos:false"),
    "计数要按当前开关来",
  );
});

/* ══════════════════════════════════════════════════════════════
 * 计数
 * ══════════════════════════════════════════════════════════════ */

test("照片数：没数完时是 null（不编数字），数完才是和", async () => {
  const { api, state, release } = fakeApi({ holdCounts: true });
  state.counts.set("d:\\photos:false", 120);
  state.counts.set("e:\\2024:false", 30);
  const store = createImportStore({ api });

  assert.equal(store.checkedPhotoCount(), 0, "一条都没勾时是 0");
  assert.equal(store.countingPhotos(), false);

  store.toggleChecked("D:\\Photos");
  store.toggleChecked("E:\\2024");
  assert.equal(store.checkedPhotoCount(), null, "还在数就不能给数字");
  assert.equal(store.countingPhotos(), true);

  release();
  await flush();
  assert.equal(store.checkedPhotoCount(), 150);
  assert.equal(store.countingPhotos(), false);
});

test("照片数：数失败退回 null，而不是当成 0 张", async () => {
  const { api } = fakeApi({ failCounts: true });
  const store = createImportStore({ api });

  store.toggleChecked("D:\\Photos");
  await flush();
  assert.equal(store.checkedDirs()[0].photoCount, null);
  assert.equal(store.checkedPhotoCount(), null);
  assert.equal(store.checkedDirs()[0].counting, false, "不要把加载态永远挂在那");
});

test("改「包含子目录」会带着新设置重新计数", async () => {
  const { api, state } = fakeApi();
  state.counts.set("d:\\photos:false", 10);
  state.counts.set("d:\\photos:true", 99);
  const store = createImportStore({ api });

  store.toggleChecked("D:\\Photos");
  await flush();
  assert.equal(store.checkedPhotoCount(), 10);

  store.setIncludeSubdirs("D:\\Photos", true);
  await flush();
  assert.equal(store.checkedDirs()[0].includeSubdirs, true);
  assert.equal(store.checkedPhotoCount(), 99);
  assert.ok(
    state.calls.includes("remember:D:\\Photos:true"),
    "开关状态要记进「最近」，下次勾选时按它来",
  );
});

/* ══════════════════════════════════════════════════════════════
 * 最近
 * ══════════════════════════════════════════════════════════════ */

test("勾选目录会顺手记一条「最近」（不需要用户收藏）", async () => {
  const { api, state } = fakeApi();
  const store = createImportStore({ api });

  await store.reloadRecent();
  assert.equal(store.recentDirs().length, 0);
  assert.equal(store.recentStatus(), "ready");

  store.toggleChecked("D:\\Photos");
  await flush();
  assert.equal(store.recentDirs().length, 1);
  assert.equal(store.recentDirs()[0].path, "D:\\Photos");
  assert.ok(state.calls.includes("remember:D:\\Photos:false"));
});

test("最近：按上限截断（Rust 侧也会裁，界面上不该多显示）", async () => {
  const rows: RecentDir[] = Array.from({ length: 8 }, (_, i) => ({
    path: `/d${i}`,
    includeSubdirs: false,
    usedAt: 100 - i,
    useCount: 1,
  }));
  const { api } = fakeApi({ recent: rows });
  const store = createImportStore({ api, recentLimit: 5 });

  await store.reloadRecent();
  assert.equal(store.recentDirs().length, 5);
});

test("最近：移除是本地立刻生效 + 通知后端", async () => {
  const { api, state } = fakeApi();
  const store = createImportStore({ api });
  store.toggleChecked("D:\\Photos");
  store.toggleChecked("E:\\2024");
  await flush();
  assert.equal(store.recentDirs().length, 2);

  await store.forgetRecent("d:/photos/");
  assert.equal(store.recentDirs().length, 1, "写法不同也要认得出是同一条");
  assert.equal(store.recentDirs()[0].path, "E:\\2024");
  assert.ok(state.calls.includes("forget:d:/photos/"));
  // 移除「最近」里的一条，不影响它的勾选状态（两件事）
  assert.ok(store.isChecked("D:\\Photos"));
});

test("最近：读失败要留下错误信息，而不是假装空列表", async () => {
  const { api, state } = fakeApi();
  const store = createImportStore({ api });
  state.failListRecent = true;

  await store.reloadRecent();
  assert.equal(store.recentStatus(), "error");
  assert.match(store.recentError() ?? "", /读不了最近列表/);
});

/* ══════════════════════════════════════════════════════════════
 * 来源树的第一层
 * ══════════════════════════════════════════════════════════════ */

test("驱动器：加载、失败要有错误信息（树显示空态而不是假装没盘）", async () => {
  const { api, state } = fakeApi();
  state.volumes = [
    { path: "D:\\", kind: "local", kindLabel: "本地磁盘" },
    { path: "//nas/photos", kind: "network", kindLabel: "网络位置" },
  ];
  const store = createImportStore({ api });

  assert.equal(store.volumesStatus(), "idle");
  assert.deepEqual(store.volumes(), []);

  await store.reloadVolumes();
  assert.equal(store.volumesStatus(), "ready");
  assert.deepEqual(
    store.volumes().map((volume) => volume.kind),
    ["local", "network"],
  );

  state.failVolumes = true;
  await store.reloadVolumes();
  assert.equal(store.volumesStatus(), "error");
  assert.match(store.volumesError() ?? "", /枚列驱动器失败/);
  assert.equal(store.volumes().length, 2, "失败不该把已有列表清空");
});

/* ══════════════════════════════════════════════════════════════
 * 库
 * ══════════════════════════════════════════════════════════════ */

test("库：加载、选中、局部刷新", async () => {
  const { api } = fakeApi({
    repositories: [repository("a", "甲"), repository("b", "乙")],
  });
  const store = createImportStore({ api });

  await store.reloadRepositories();
  assert.equal(store.repositories().length, 2);
  assert.equal(store.repositoriesStatus(), "ready");
  assert.equal(store.selectedRepository(), null);

  store.selectRepository("b");
  assert.equal(store.selectedRepository()?.name, "乙");

  store.upsertRepository({ ...repository("c", "丙"), photoCount: 7 });
  assert.equal(store.repositories().length, 3);
  store.upsertRepository({ ...repository("c", "丙（改名）"), photoCount: 9 });
  assert.equal(store.repositories().length, 3, "同 id 是更新而不是追加");
  assert.equal(store.selectedRepository()?.name, "乙");
});

test("库：重新加载后选中的库没了，就把选中清掉", async () => {
  const { api, state } = fakeApi({ repositories: [repository("a")] });
  const store = createImportStore({ api });
  await store.reloadRepositories();
  store.selectRepository("a");

  state.repositories = [repository("b")];
  await store.reloadRepositories();
  assert.equal(store.selectedRepositoryId(), null);
  assert.equal(store.selectedRepository(), null);
});

test("重挂载：找到就转在线并刷新视图；找不到只记一句提示（不是错误）", async () => {
  const { api, state } = fakeApi({
    repositories: [{ ...repository("a"), online: false }],
  });
  const store = createImportStore({ api });
  await store.reloadRepositories();

  await store.remount("a");
  assert.equal(store.remountingId(), null, "结束后要给放掉转圈状态");
  assert.match(store.remountErrors()["a"] ?? "", /未找到该库/);
  assert.equal(store.repositories()[0].online, false);

  state.remountFinds = true;
  await store.remount("a");
  assert.equal(store.repositories()[0].online, true, "找到了就转在线");
  assert.equal(store.remountErrors()["a"], undefined, "成功要把上次的提示清掉");
});

test("重挂载失败（命令报错）：错误记在那一张卡片上，不影响其它库", async () => {
  const { api, state } = fakeApi({
    repositories: [{ ...repository("a"), online: false }, repository("b")],
  });
  state.failRemount = true;
  const store = createImportStore({ api });
  await store.reloadRepositories();

  await store.remount("a");
  assert.match(store.remountErrors()["a"] ?? "", /重挂载炸了/);
  assert.equal(store.remountErrors()["b"], undefined);
  assert.equal(store.remountingId(), null);
});

test("避免重复导入：默认开、写回设置、能读回", async () => {
  const { api, state } = fakeApi();
  const store = createImportStore({ api });

  assert.equal(store.avoidDuplicates(), true, "默认勾上（REPOSITORY.md §4.3）");
  store.setAvoidDuplicates(false);
  await flush();
  assert.equal(state.settings.get("import.avoid_duplicates"), "0");

  const reopened = createImportStore({ api });
  await reopened.hydratePreferences();
  assert.equal(reopened.avoidDuplicates(), false);
});

/* ══════════════════════════════════════════════════════════════
 * 小工具
 * ══════════════════════════════════════════════════════════════ */

test("errorText：Error / 字符串 / 其它都能变成一句话", () => {
  assert.equal(errorText(new Error("炸了")), "炸了");
  assert.equal(errorText("字符串错误"), "字符串错误");
  assert.equal(errorText({ code: 1 }), "[object Object]");
  assert.equal(errorText(null), "null");
  assert.equal(errorText(undefined), "undefined");
});
