/**
 * 导入工作区的共享状态（`ARCHITECTURE.md` §3：跨模块的状态归 app store）。
 *
 * 为什么这些状态必须放在**一个**store 里，而不是各 feature 自己存：
 *
 * | 状态 | 谁在用 |
 * | --- | --- |
 * | `selectedDir`（选中，全局唯一） | `recent` / `source-tree` / `photo-grid` / 控制条 |
 * | `checkedDirs`（勾选，多选） | `recent` / `source-tree` / `selected-dirs` / 右列的导入统计 |
 * | `repositories` + `selectedRepositoryId` | 右列库列表 / 底部的导入按钮可用性 |
 * | 照片选择与排除 | `photo-grid` / 外壳的 `toolsbar` |
 *
 * 而「展开集合」「最近列表的加载态」这类**只属于一个模块**的状态留在各自 feature 里
 * （见 `features/source-tree/store.ts`），免得这个店变成一个什么都往里塞的垃圾抽屉。
 *
 * ⚠️ **选中与勾选是两件事**（`AGENTS.md` §11.2）：
 *   - **选中**：当前正在浏览哪个目录 —— 同一时间只有一个，跨面板同步；
 *   - **勾选**：哪些目录要一起导入 —— 可多选，与选中互不影响。
 * 这两条在这一层用类型与命名钉死（`selectedDir` vs `checkedDirs`），
 * 免得日后有人「顺手」把它们合并成一个。
 */

import { createSignal } from "solid-js";
import type {
  DirEntry,
  RecentDir,
  RepositoryView,
  Volume,
} from "../../api/types.ts";
import {
  hasUncounted,
  sumPhotoCounts,
  type CheckedDir,
} from "../../lib/checked-dir.ts";
import type { LoadStatus } from "../../lib/load-status.ts";
import { samePath } from "../../lib/tree.ts";

/** 本模块用到的 `src/api/db.ts` 子集（注入以便测试）。 */
export interface ImportApi {
  listRecentDirs: () => Promise<RecentDir[]>;
  rememberRecentDir: (path: string, includeSubdirs: boolean) => Promise<void>;
  forgetRecentDir: (path: string) => Promise<boolean>;
  countSourcePhotos: (
    path: string,
    recursive: boolean,
  ) => Promise<{ photos: number; skipped: number; truncated: boolean }>;
  listRepositories: () => Promise<RepositoryView[]>;
  listDirs: (path: string) => Promise<DirEntry[]>;
  listVolumes: () => Promise<Volume[]>;
}

export interface ImportStore {
  /* ── 选中（全局唯一）───────────────────────── */
  selectedDir: () => string | null;
  /** 选中某个目录（`null` = 取消选中）。跨面板比较一律走 `samePath` */
  selectDir: (path: string | null) => void;
  isSelected: (path: string) => boolean;

  /* ── 勾选（多选）──────────────────────────── */
  checkedDirs: () => readonly CheckedDir[];
  isChecked: (path: string) => boolean;
  /**
   * 勾选 / 取消勾选一条目录。
   *
   * 勾选时会顺手做两件事：**记进「最近」**（`design/main.md` §3.1.1：不需要用户收藏）
   * 与**开始数照片**（喂「已选择 N 张照片」）。
   */
  toggleChecked: (path: string, includeSubdirs?: boolean) => void;
  setIncludeSubdirs: (path: string, value: boolean) => void;
  removeChecked: (path: string) => void;
  /** 已勾选目录里**已知**的照片总数；还有目录没数完时返回 `null`（界面显示「统计中」而不是编一个数） */
  checkedPhotoCount: () => number | null;
  /** 还有目录在数照片 */
  countingPhotos: () => boolean;

  /* ── 最近 ─────────────────────────────────── */
  recentDirs: () => readonly RecentDir[];
  recentStatus: () => LoadStatus;
  recentError: () => string | null;
  reloadRecent: () => Promise<void>;
  /** 从最近里移除（**不动磁盘**，`AGENTS.md` §11.3 的「移除」） */
  forgetRecent: (path: string) => Promise<void>;

  /* ── 来源树的第一层（驱动器 / 挂载点）───────── */
  volumes: () => readonly Volume[];
  /**
   * 读一个目录的直接子目录（树的懒加载用）。
   *
   * 只是把注入的 api 透出去 —— 这样视图不必自己 import `src/api`，
   * 工作区里也就只有 store 这一个「数据入口」。
   */
  loadDirs: (path: string) => Promise<DirEntry[]>;
  volumesStatus: () => LoadStatus;
  volumesError: () => string | null;
  reloadVolumes: () => Promise<void>;

  /* ── 库（右列）────────────────────────────── */
  repositories: () => readonly RepositoryView[];
  repositoriesStatus: () => LoadStatus;
  repositoriesError: () => string | null;
  reloadRepositories: () => Promise<void>;
  selectedRepositoryId: () => string | null;
  selectRepository: (id: string | null) => void;
  /** 当前选中的库（没选或已被移除时是 `null`） */
  selectedRepository: () => RepositoryView | null;
  /** 替换一个库的视图（建库/重挂载后局部刷新，不必重载整张表） */
  upsertRepository: (view: RepositoryView) => void;
}

export interface ImportStoreDeps {
  api: ImportApi;
  /** 最近目录保留条数（与 Rust 侧的默认值一致） */
  recentLimit?: number;
}

export const DEFAULT_RECENT_LIMIT = 50;

export function createImportStore(deps: ImportStoreDeps): ImportStore {
  const limit = deps.recentLimit ?? DEFAULT_RECENT_LIMIT;

  /* ── 选中 ─────────────────────────────────── */
  const [selectedDir, setSelectedDir] = createSignal<string | null>(null);

  /* ── 勾选 ─────────────────────────────────── */
  const [checkedDirs, setCheckedDirs] = createSignal<readonly CheckedDir[]>([]);

  /* ── 最近 ─────────────────────────────────── */
  const [recentDirs, setRecentDirs] = createSignal<readonly RecentDir[]>([]);
  const [recentStatus, setRecentStatus] = createSignal<LoadStatus>("idle");
  const [recentError, setRecentError] = createSignal<string | null>(null);

  /* ── 来源树的第一层 ────────────────────────── */
  const [volumes, setVolumes] = createSignal<readonly Volume[]>([]);
  const [volumesStatus, setVolumesStatus] = createSignal<LoadStatus>("idle");
  const [volumesError, setVolumesError] = createSignal<string | null>(null);

  /* ── 库 ──────────────────────────────────── */
  const [repositories, setRepositories] = createSignal<readonly RepositoryView[]>(
    [],
  );
  const [repositoriesStatus, setRepositoriesStatus] =
    createSignal<LoadStatus>("idle");
  const [repositoriesError, setRepositoriesError] = createSignal<string | null>(
    null,
  );
  const [selectedRepositoryId, setSelectedRepositoryId] = createSignal<
    string | null
  >(null);

  /* ══════════════════════════════════════════════════════════
   * 选中
   * ══════════════════════════════════════════════════════════ */

  const isSelected = (path: string): boolean =>
    samePath(selectedDir(), path);

  const selectDir = (path: string | null): void => {
    setSelectedDir(path);
  };

  /* ══════════════════════════════════════════════════════════
   * 勾选
   * ══════════════════════════════════════════════════════════ */

  const isChecked = (path: string): boolean =>
    checkedDirs().some((entry) => samePath(entry.path, path));

  const patchChecked = (
    path: string,
    patch: Partial<CheckedDir>,
  ): void => {
    setCheckedDirs((prev) =>
      prev.map((entry) =>
        samePath(entry.path, path) ? { ...entry, ...patch } : entry,
      ),
    );
  };

  /**
   * 数一个目录的照片数（后台跑，失败就退回 `null` —— 界面显示「—」而不是 0）。
   *
   * 这里**不取消**上一次的请求：用户来回勾选时，旧结果写回会被 `patchChecked`
   * 的路径匹配挡掉（那一条已经不在列表里了）。
   */
  const startCount = async (
    path: string,
    includeSubdirs: boolean,
  ): Promise<void> => {
    patchChecked(path, { counting: true });
    try {
      const result = await deps.api.countSourcePhotos(path, includeSubdirs);
      patchChecked(path, { photoCount: result.photos, counting: false });
    } catch {
      patchChecked(path, { photoCount: null, counting: false });
    }
  };

  /** 勾选时把它记进「最近」（这是「最近」唯一的写入时机，见 `plans/M1-5.md` §8）。 */
  const remember = async (
    path: string,
    includeSubdirs: boolean,
  ): Promise<void> => {
    try {
      await deps.api.rememberRecentDir(path, includeSubdirs);
      await reloadRecent();
    } catch {
      // 记不进「最近」不该妨碍勾选本身 —— 它只是个便利列表
    }
  };

  const toggleChecked = (path: string, includeSubdirs = false): void => {
    if (isChecked(path)) {
      removeChecked(path);
      return;
    }
    setCheckedDirs((prev) => [
      ...prev,
      { path, includeSubdirs, photoCount: null, counting: false },
    ]);
    // 两条后台任务互不依赖：记「最近」与数照片
    void remember(path, includeSubdirs);
    void startCount(path, includeSubdirs);
  };

  const setIncludeSubdirs = (path: string, value: boolean): void => {
    patchChecked(path, { includeSubdirs: value, photoCount: null });
    void remember(path, value);
    void startCount(path, value);
  };

  function removeChecked(path: string): void {
    setCheckedDirs((prev) =>
      prev.filter((entry) => !samePath(entry.path, path)),
    );
  }

  /**
   * 已勾选目录的照片总数。
   *
   * ⚠️ **写成普通函数而不是 `createMemo`** —— 这是踩过的坑，别再改回去：
   * 测试跑在 Node 里，而 `solid-js` 的 `node` 导出条件指向 **SSR 构建**，
   * 那里 `createMemo(fn)` 只是**求值一次**、之后永不更新。
   * 于是「在浏览器里正常、在测试里永远返回初始值」这种最难查的假绿就出现了
   * （实测：勾了两条目录，`checkedPhotoCount()` 一直返回 0）。
   * 派生量用普通函数：读取时照常参与 Solid 的依赖追踪，只是不做缓存 ——
   * 这几个数字小到不值得缓存。
   */
  const checkedPhotoCount = (): number | null => sumPhotoCounts(checkedDirs());

  const countingPhotos = (): boolean => hasUncounted(checkedDirs());

  /* ══════════════════════════════════════════════════════════
   * 最近
   * ══════════════════════════════════════════════════════════ */

  async function reloadRecent(): Promise<void> {
    setRecentStatus("loading");
    try {
      const rows = await deps.api.listRecentDirs();
      setRecentDirs(rows.slice(0, limit));
      setRecentError(null);
      setRecentStatus("ready");
    } catch (error) {
      setRecentError(errorText(error));
      setRecentStatus("error");
    }
  }

  async function forgetRecent(path: string): Promise<void> {
    try {
      await deps.api.forgetRecentDir(path);
    } catch (error) {
      setRecentError(errorText(error));
      return;
    }
    // 本地先摘掉（不等重载）：用户点了「移除」就该立刻看到它消失
    setRecentDirs((prev) => prev.filter((row) => !samePath(row.path, path)));
  }

  /* ══════════════════════════════════════════════════════════
   * 来源树的第一层
   * ══════════════════════════════════════════════════════════ */

  async function reloadVolumes(): Promise<void> {
    setVolumesStatus("loading");
    try {
      const rows = await deps.api.listVolumes();
      setVolumes(rows);
      setVolumesError(null);
      setVolumesStatus("ready");
    } catch (error) {
      setVolumesError(errorText(error));
      setVolumesStatus("error");
    }
  }

  /* ══════════════════════════════════════════════════════════
   * 库
   * ══════════════════════════════════════════════════════════ */

  async function reloadRepositories(): Promise<void> {
    setRepositoriesStatus("loading");
    try {
      const rows = await deps.api.listRepositories();
      setRepositories(rows);
      setRepositoriesError(null);
      setRepositoriesStatus("ready");
      // 选中的库如果已经不在了（被移除/改名），把选中清掉
      const current = selectedRepositoryId();
      if (current !== null && !rows.some((row) => row.id === current)) {
        setSelectedRepositoryId(null);
      }
    } catch (error) {
      setRepositoriesError(errorText(error));
      setRepositoriesStatus("error");
    }
  }

  const selectRepository = (id: string | null): void => {
    setSelectedRepositoryId(id);
  };

  /** 当前选中的库（同上：普通函数，不用 `createMemo`） */
  const selectedRepository = (): RepositoryView | null => {
    const id = selectedRepositoryId();
    if (id === null) return null;
    return repositories().find((row) => row.id === id) ?? null;
  };

  const upsertRepository = (view: RepositoryView): void => {
    setRepositories((prev) => {
      const index = prev.findIndex((row) => row.id === view.id);
      if (index === -1) return [...prev, view];
      const next = [...prev];
      next[index] = view;
      return next;
    });
  };

  return {
    selectedDir,
    selectDir,
    isSelected,
    checkedDirs,
    isChecked,
    toggleChecked,
    setIncludeSubdirs,
    removeChecked,
    checkedPhotoCount,
    countingPhotos,
    volumes,
    volumesStatus,
    volumesError,
    reloadVolumes,
    loadDirs: (path) => deps.api.listDirs(path),
    recentDirs,
    recentStatus,
    recentError,
    reloadRecent,
    forgetRecent,
    repositories,
    repositoriesStatus,
    repositoriesError,
    reloadRepositories,
    selectedRepositoryId,
    selectRepository,
    selectedRepository,
    upsertRepository,
  };
}

/** 把任意异常转成给用户看的一句话。 */
export function errorText(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return String(error);
}
