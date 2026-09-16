/**
 * 导入工作区的共享状态（`ARCHITECTURE.md` §3：跨模块的状态归 app store）。
 *
 * 为什么这些状态必须放在**一个**store 里，而不是各 feature 自己存：
 *
 * | 状态 | 谁在用 |
 * | --- | --- |
 * | `selectedDir`（选中，全局唯一） | `recent` / `dir-tree` / `photo-grid` / 控制条 |
 * | `checkedDirs`（勾选，多选） | `recent` / `dir-tree` / `selected-dirs` / 右列的导入统计 |
 * | `repositories` + `selectedRepositoryId` | 右列库列表 / 底部的导入按钮可用性 |
 * | 照片选择与排除 | `photo-grid` / 外壳的 `toolsbar` |
 *
 * 而「展开集合」「最近列表的加载态」这类**只属于一个模块**的状态留在各自 feature 里
 * （见 `features/dir-tree/store.ts`），免得这个店变成一个什么都往里塞的垃圾抽屉。
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
import {
  createRepositoryState,
  type RepositoryStateApi,
} from "../../features/repositories/state.ts";

/**
 * 本模块用到的 `src/api/db.ts` 子集（注入以便测试）。
 *
 * 继承 `RepositoryStateApi`：**库的状态不在这里实现**，见 `createRepositoryState`
 * （中央状态，所有界面共读一份）。
 */
export interface ImportApi extends RepositoryStateApi {
  listRecentDirs: () => Promise<RecentDir[]>;
  rememberRecentDir: (path: string, includeSubdirs: boolean) => Promise<void>;
  forgetRecentDir: (path: string) => Promise<boolean>;
  countSourcePhotos: (
    path: string,
    recursive: boolean,
  ) => Promise<{ photos: number; skipped: number; truncated: boolean }>;
  listRepositories: () => Promise<RepositoryView[]>;
  remountRepository: (repositoryId: string) => Promise<RepositoryView>;
  getSetting: (key: string) => Promise<string | null>;
  setSetting: (key: string, value: string) => Promise<void>;
  listDirs: (path: string) => Promise<DirEntry[]>;
  listVolumes: () => Promise<Volume[]>;
  /** 一批路径现在还是不是目录（「最近」标灰用） */
  pathsStatus: (paths: string[]) => Promise<boolean[]>;
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
  /** 这个目录**现在**找不到（盘没插 / 目录被改名）—— 「最近」据此标灰 */
  isUnavailable: (path: string) => boolean;
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
  /** 有地方发现这个库读不到了（如库设置读 catalog 失败）→ 立刻降级为离线 */
  markRepositoryOffline: (repositoryId: string) => void;
  /** 模版改了 → 就地同步进列表 */
  applyRepositoryTemplate: (repositoryId: string, template: string) => void;
  /** 正在重新查找的库 id */
  remountingId: () => string | null;
  /** 重新查找失败的原因（库 id → 文案；成功则清掉） */
  remountErrors: () => Readonly<Record<string, string>>;
  /** 对所有登记路径重新查找一次（离线徽标点它） */
  remount: (repositoryId: string) => Promise<void>;

  /* ── 导入偏好 ─────────────────────────────── */
  /**
   * 「避免重复导入」——默认**开**（`REPOSITORY.md` §4.3）。
   * 它跨会话记住（放 `app.db` 的设置表），因为这是用户的稳定偏好。
   */
  avoidDuplicates: () => boolean;
  setAvoidDuplicates: (value: boolean) => void;
  /** 从设置里读回导入偏好（工作区挂载时调一次） */
  hydratePreferences: () => Promise<void>;
}

export interface ImportStoreDeps {
  api: ImportApi;
  /** 最近目录保留条数（与 Rust 侧的默认值一致） */
  recentLimit?: number;
}

export const DEFAULT_RECENT_LIMIT = 50;

/** 设置键：「避免重复导入」（与 `src/api/db.ts` 的 `SETTING_KEYS` 一致） */
const IMPORT_AVOID_DUPLICATES_KEY = "import.avoid_duplicates";

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
  /*
   * 库的**中央状态**：列表、在线/离线、重挂载状态、模版都在一份里。
   * 这里只做转发（这层的公开 API 不变），好处是别的地方（库设置弹窗、
   * 将来的浏览侧）可以直接读写同一份 —— 一个地方变了，所有界面都跟着变。
   */
  const repos = createRepositoryState({ api: deps.api });
  const [selectedRepositoryId, setSelectedRepositoryId] = createSignal<
    string | null
  >(null);
  const [avoidDuplicates, setAvoidDuplicatesSignal] = createSignal(true);

  /* ══════════════════════════════════════════════════════════
   * 选中
   * ══════════════════════════════════════════════════════════ */

  const isSelected = (path: string): boolean =>
    samePath(selectedDir(), path);

  const selectDir = (path: string | null): void => {
    setSelectedDir(path);
    // **每次重新选中都重查一次**（人类 2026-09-16）：盘可能刚插上/刚拔掉
    if (path !== null) void recheckAvailability(path);
  }

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

  /** 重挂载：实现在中央状态里（它要就地把结果同步给所有界面） */
  const remount = (repositoryId: string): Promise<void> =>
    repos.remount(repositoryId);

  const setAvoidDuplicates = (value: boolean): void => {
    if (value === avoidDuplicates()) return;
    setAvoidDuplicatesSignal(value);
    void deps.api
      .setSetting(IMPORT_AVOID_DUPLICATES_KEY, value ? "1" : "0")
      .catch(() => {
        // 存不下偏好不影响本次使用
      });
  };

  async function hydratePreferences(): Promise<void> {
    // 启动就把「最近」的挂载情况认一遍（不等用户点）
    void refreshRecentAvailability();
    try {
      const raw = await deps.api.getSetting(IMPORT_AVOID_DUPLICATES_KEY);
      if (raw === null) return;
      setAvoidDuplicatesSignal(raw === "1" || raw.toLowerCase() === "true");
    } catch {
      // 读不到就用默认（开）
    }
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
   * 「最近目录」还在不在（盘没插 → 标灰）
   *
   * 人类 2026-09-16：*「recent 目录也有可能未挂载……除了启动时要识别出哪些未挂载把颜色标灰，
   * 每次重新选中时也要检测一遍有没有挂载」*。
   * 判据在后端很轻（`is_dir`，不读目录内容）；这里只存一份路径集合。
   * ══════════════════════════════════════════════════════════ */

  const [unavailable, setUnavailable] = createSignal<ReadonlySet<string>>(
    new Set(),
  );

  /** 比路径用的键：与 `samePath` 同口径（大小写折叠），但集合键要的是稳定字符串 */
  const availabilityKey = (path: string): string => path.toLowerCase();

  const isUnavailable = (path: string): boolean =>
    unavailable().has(availabilityKey(path));

  function setAvailability(paths: readonly string[], available: boolean[]): void {
    setUnavailable((prev) => {
      const next = new Set(prev);
      let touched = false;
      paths.forEach((path, index) => {
        const key = availabilityKey(path);
        const ok = available[index] ?? true;
        if (ok && next.delete(key)) touched = true;
        else if (!ok && !next.has(key)) {
          next.add(key);
          touched = true;
        }
      });
      return touched ? next : prev;
    });
  }

  /** 启动时（`hydrate`）把整份「最近」查一遍 */
  async function refreshRecentAvailability(): Promise<void> {
    const paths = recentDirs().map((row) => row.path);
    if (paths.length === 0) return;
    try {
      const available = await deps.api.pathsStatus(paths);
      setAvailability(paths, available);
    } catch {
      // 查不了不该打扰用户：保持上一次的判断（新装的盘顶多显示成旧的灰）
    }
  }

  /** 选中某个目录时**重查这一条** —— 盘可能刚插上，也可能刚拔掉 */
  async function recheckAvailability(path: string): Promise<void> {
    try {
      const [available] = await deps.api.pathsStatus([path]);
      setAvailability([path], [available ?? true]);
    } catch {
      // 同上：查不动就不动
    }
  }

  /* ══════════════════════════════════════════════════════════
   * 库
   * ══════════════════════════════════════════════════════════ */

  async function reloadRepositories(): Promise<void> {
    await repos.load();
    // 选中的库如果已经不在了（被移除/改名），把选中清掉
    const current = selectedRepositoryId();
    if (current !== null && repos.byId(current) === undefined) {
      setSelectedRepositoryId(null);
    }
  }

  const selectRepository = (id: string | null): void => {
    setSelectedRepositoryId(id);
  };

  /** 当前选中的库（同上：普通函数，不用 `createMemo`） */
  const selectedRepository = (): RepositoryView | null => {
    const id = selectedRepositoryId();
    if (id === null) return null;
    return repos.byId(id) ?? null;
  };

  /** 就地更新一条库（走中央状态，所有挂着的界面同步） */
  const upsertRepository = (view: RepositoryView): void => {
    repos.upsert(view);
  };

  /** 有地方发现这个库读不到了（如库设置读 catalog 失败）→ 立刻降级为离线 */
  const markRepositoryOffline = (repositoryId: string): void => {
    repos.markOffline(repositoryId);
  };

  /** 模版改了 → 就地同步进列表（不用整表重拉） */
  const applyRepositoryTemplate = (
    repositoryId: string,
    template: string,
  ): void => {
    repos.patch(repositoryId, { importTemplate: template });
  };

  return {
    selectedDir,
    selectDir,
    isSelected,
    checkedDirs,
    isChecked,
    isUnavailable,
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
    repositories: repos.list,
    repositoriesStatus: repos.status,
    repositoriesError: repos.error,
    reloadRepositories,
    selectedRepositoryId,
    selectRepository,
    selectedRepository,
    upsertRepository,
    markRepositoryOffline,
    applyRepositoryTemplate,
    remountingId: repos.remountingId,
    remountErrors: repos.remountErrors,
    remount,
    avoidDuplicates,
    setAvoidDuplicates,
    hydratePreferences,
  };
}

/** 把任意异常转成给用户看的一句话。 */
export function errorText(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return String(error);
}
