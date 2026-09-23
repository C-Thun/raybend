/**
 * 浏览工作区：三列（`BROWSE.md` §1、`design/browse.md` §2）。
 *
 * ```text
 * ┌────────────┬──────────────────────────────┬──────────────┐
 * │ 左列 300   │ 中列（弹性，surface-bar）     │ 右列 300     │
 * │ 库目录选择器 │ 照片网格 + 底部控制条         │ 信息栏        │
 * └────────────┴──────────────────────────────┴──────────────┘
 * ```
 *
 * 这是**组装层**：把 feature 与工作区共享状态接起来，自己不做业务判断
 * （`ARCHITECTURE.md` §2）。库列表在这里拉一次 —— 属于工作区自己的生命周期。
 *
 * ⚠️ W1 的取舍（明确记下来，别当成遗漏）：
 * * 左右列宽度**固定**（拖拽调宽与导入工作区一样，属 W2 的活儿）；
 * * 控制条只有「尺寸档位」与「按时间」（筛选/排序的 UI 在 W2）；
 * * 看图 / 对比 / 胶片带在 W2 —— 这里是「看片、挑片」的第一屏。
 */

import {
  createEffect,
  createMemo,
  createSignal,
  For,
  onCleanup,
  onMount,
  Show,
  type JSX,
} from "solid-js";

import {
  getThumbBytes,
  getViewImage,
  listRepositories,
  remountRepository,
  syncDirectoryCounts,
} from "../../api/db.ts";
import type { RepositoryView } from "../../api/types.ts";
import {
  AssetInfo,
  BrowseLeftColumn,
  type BrowseStore,
} from "../../features/browse/index.ts";
import { FilterBar } from "../../features/browse/FilterBar.tsx";
import { browseSource } from "../../features/browse/grid-source.ts";
import {
  createPhotoViewingController,
  PhotoViewingStage,
} from "../../features/photo-grid/index.ts";
import { StateWatermark } from "../../components/ui/StateWatermark.tsx";
import {
  IconAlbumOff,
  IconAlertTriangle,
  IconFolder,
  IconPhoto,
  IconPhotoOff,
} from "@tabler/icons-solidjs";
import { createThumbQueue } from "../../components/ui/thumb-queue.ts";
import { buildFullscreenTarget } from "../../lib/fullscreen-target.ts";
import type { TilesViewingInfo } from "../../components/ui/tiles/index.ts";
import { createViewerStore, photosFromSource, viewingInfoOf } from "../../components/ui/viewer/index.ts";
import {
  browseDisplayByTime,
  browseDisplayTileStep,
  commitBrowseDisplayTileStep,
  setBrowseDisplayByTime,
  setBrowseDisplayTileStep,
} from "../../lib/display-prefs.ts";
import { browseInfoMode, cycleBrowseTileInfo } from "../../components/ui/tile-info.ts";
import { t } from "../../i18n/index.ts";
import { Button } from "../../components/ui/Button.tsx";
import { ConfirmDialog, Dialog } from "../../components/ui/Dialog.tsx";
import type { ToastStore } from "../../components/ui/Toast.tsx";
import { registerBrowseActions } from "../../features/browse/actions.ts";
import { SplitHandle } from "../../components/ui/SplitHandle.tsx";
import { nudgeWidth, resizeWidth } from "../../lib/column-resize.ts";
import { joinPath } from "../../lib/paths.ts";
import { LAYOUT_BOUNDS } from "../../lib/layout-prefs.ts";

/**
 * 侧栏宽度的上下限 = **唯一那一份**（`lib/layout-prefs.ts` 的 `LAYOUT_BOUNDS`）。
 *
 * 早先这里手抄过一份边界，改存储侧时拖拽侧不会同步。现在两边共用一条；
 * 2026-09-20 下限与 import left 对齐为 220px。
 */
const SIDEBAR_BOUNDS = LAYOUT_BOUNDS.browseLeftWidth;
import type { BrowseSort, DeleteFailure } from "../../api/types.ts";

export interface BrowseWorkspaceProps {
  store: BrowseStore;
  /**
   * 点库卡片上的齿轮 → 打开**库设置**。
   *
   * 弹窗本身住在组装层（`App.tsx`）：导入侧与浏览侧点开的是同一个
   * `LibrarySettingsDialog`，而两个工作区都不该各自造一份弹窗状态。
   */
  onOpenLibrarySettings?: (repositoryId: string) => void;
  /**
   * 提示通道（`components/ui/Toast.tsx`）：删除这类**改磁盘**的动作必须给回执 ——
   * 谁删了什么、几个被锁挡住、哪几个失败了，都在提示与失败清单里说清楚。
   */
  toast?: ToastStore;
  /** 左列 / 右列宽度（像素，受控；拖拽松手时通过下面的回调落盘） */
  leftWidth?: number;
  /**
   * 右列宽度**不在 props 里**：它如今只有一份定义 —— `tokens.css` 的 `--panel-w-right`
   *（workspace 右列，所有工作流通用），容器上直接用 `w-panel-w-right`。
   *
   * 为什么之前是 props：那时 browse 右列有一个独立常量（`layout-prefs.ts` 的
   * `BROWSE_RIGHT_WIDTH`），与导入侧的令牌是**两份定义** —— 同一个东西两处写。
   * 人类 2026-09-23 定「统一宽度」，于是常量与 props 一起拆了。
   */
  onLeftWidthChange?: (width: number) => void;
  /** browse 自己的胶片带尺寸档位；与 import 分开存进 app.db。 */
  filmStripStep: number;
  onFilmStripStepChange: (step: number) => void;
  class?: string;
}

/** 左列把手：外面只给尺寸数学，拖动/节流/外观全在 `SplitHandle` 里（全项目唯一实现） */
function ColumnHandle(props: {
  onDragStart: () => void;
  onDrag: (dx: number) => void;
  onDragEnd: () => void;
  onNudge: (delta: number) => void;
}): JSX.Element {
  return (
    <SplitHandle
      orientation="vertical"
      /* 冒烟脚本（`scripts/check-browse-boot.mjs`）靠这个属性找它 */
      data-col-resizer="left"
      aria-label={t("common.resize_left")}
      tabindex="0"
      onDragStart={props.onDragStart}
      onDrag={props.onDrag}
      onDragEnd={props.onDragEnd}
      onKeyDown={(event) => {
        if (event.key === "ArrowLeft") {
          event.preventDefault();
          // 键盘调整：左右方向键各 8px，立刻落盘（一次按键就是一次「结束」）
          props.onNudge(-8);
        } else if (event.key === "ArrowRight") {
          event.preventDefault();
          props.onNudge(8);
        }
      }}
    />
  );
}

export function BrowseWorkspace(props: BrowseWorkspaceProps) {
  const store = props.store;
  const [repositories, setRepositories] = createSignal<RepositoryView[]>([]);
  /** 还在读库列表；用来把「还没读到」与「真的没有库」分开显示。 */
  const [reposLoading, setReposLoading] = createSignal(true);
  /** 读库列表失败时的原因（不再是静默空态）。 */
  const [reposError, setReposError] = createSignal<string | null>(null);
  /*
   * 「按时间」与格子尺寸档位走 browse 自己的设备级偏好（`lib/display-prefs.ts`）：
   * 与 import 复用同一套迁移/持久化实现，但两边的值互相隔离并可跨会话还原。
   * 以前这里是本地信号，切走再回来会重置。
   */
  const tileStep = browseDisplayTileStep;
  const setTileStep = setBrowseDisplayTileStep;
  const grouped = browseDisplayByTime;
  const setGrouped = setBrowseDisplayByTime;
  /**
   * 库列表是否展开（`BROWSE.md` §4.2）。
   *
   * 状态住在工作区而不是左列里：**收起要由网格侧的点击触发**，
   * 而那个点击发生在左列之外。切走工作流时整个工作区卸载 → 自然回到缩起态（
   * 这也正是三条收起条件里的「切换 flow」）。
   */
  const [libsExpanded, setLibsExpanded] = createSignal(false);
  /**
   * 排序的五个键（`plans/M2-W2-tail.md` 3.5；引擎 `BrowseSort` 早就支持）。
   *
   * 文案用**静态映射**而不是拼键名：`t()` 的键是字面量联合类型，拼出来的字符串过不了类型检查
   * （这是有意为之 —— 拼错的键不会等到运行时才发现）。
   */
  const SORT_LABELS: Record<NonNullable<BrowseSort["key"]>, () => string> = {
    takenAt: () => t("browse.sortTakenAt"),
    importedAt: () => t("browse.sortImportedAt"),
    fileName: () => t("browse.sortFileName"),
    rating: () => t("browse.sortRating"),
    camera: () => t("browse.sortCamera"),
  };

  /*
   * 左列拖拽：用全项目**唯一的**把手组件 `SplitHandle`（拖动、指针捕获、rAF 节流、
   * 拖动期禁选择都在它里面）。这里只管尺寸数学与落盘时机：
   * 数学在 `lib/column-resize.ts`（有单测），**松手才落盘**。
   *
   * 2026-09-19 统一：这里与导入工作区本来是两份手写副本，另一份还走了 Ark 的 Splitter
   * （量了又写、写了又量 → 拖起来卡死）。现在两处都只是「把手 + 尺寸数学」。
   */
  const [leftWidth, setLeftWidth] = createSignal(props.leftWidth ?? 300);
  /*
   * 右列宽度**固定**（`--panel-w-right`，不参与拖拽）：人类 2026-09-19 定「把手只加在左侧边界」，
   * 2026-09-23 又把它统一成**所有工作流共用的一个值** —— 所以这里不再算宽度，
   * 容器上直接挂 `w-panel-w-right`（与导入侧那一列同一个类名、同一个令牌）。
   */
  /** 拖动起点的左列宽度（用「起点 + dx」而不是逐帧累加，丢帧时不会漂移） */
  let leftDragStart = 0;

  function beginLeftResize(): void {
    leftDragStart = leftWidth();
  }
  function dragLeft(dx: number): void {
    setLeftWidth(resizeWidth({ start: leftDragStart, dx, bounds: SIDEBAR_BOUNDS }));
  }
  function endLeftResize(): void {
    props.onLeftWidthChange?.(leftWidth());
  }
  function nudgeLeft(delta: number): void {
    const next = nudgeWidth(leftWidth(), delta, SIDEBAR_BOUNDS);
    setLeftWidth(next);
    props.onLeftWidthChange?.(next);
  }

  /** 「当前那张」在列表里的下标 —— 键盘导航与「把它滚进视野」都靠它 */
  const [focusIndex, setFocusIndex] = createSignal<number | undefined>(undefined);
  /** 待确认的删除（张数；`null` = 没在确认） */
  const [pendingDelete, setPendingDelete] = createSignal<number | null>(null);
  /** 删除失败清单（有它就弹模态逐条列出来） */
  const [deleteFailures, setDeleteFailures] = createSignal<DeleteFailure[]>([]);
  /*
   * 看图（`components/ui/viewer/` 的共享件）。
   *
   * 取图走**统一取图口**（`getViewImage` → Rust 侧 `display` 模块，
   * 见 `plans/M2-W2.md` §2.1）：屏幕档由它决定「给原图还是渲染」，
   * 小图（秒显的底）还是网格那套缓存。这里只负责「谁触发打开」。
   */
  const viewer = createViewerStore({
    loadScreen: (path) => getViewImage(path, "screen"),
    loadThumb: (path) => getThumbBytes(path, "grid"),
  });

  /*
   * 缩略图队列：**网格与胶片带共用同一个**（`plans/M2-W2.md` 2.1）。
   *
   * 为什么提到工作区这一层：两个消费方都在中列，展示的也是同一个目录的照片 ——
   * 各建一个的话，同一张照片会被取两遍（多一趟 IPC + 两份内存），
   * 而共用之后「网格里已缓存的，胶片带里立刻就有」。
   * 换库时清空（否则不同库的同名相对路径会串图，M2-W1 踩过）。
   */
  const thumbs = createThumbQueue({
    load: async (path) => {
      const bytes = await getThumbBytes(path, "grid");
      return bytes ?? null;
    },
  });
  createEffect(() => {
    if (root() === null) return;
    thumbs.clear();
  });
  onCleanup(() => thumbs.clear());

  /*
   * view / film / compare 的状态、锚点与尺寸补读只在共享控制器里维护。
   * 两个工作区的差异只剩数据适配：这里把库 id 转成绝对路径交给 browse store。
   */
  const viewing = createPhotoViewingController({
    viewer,
    /* browse 保持 M2 的**四档**（人类 2026-09-23 明确：browse 不动） */
    chromeMode: () => "browse",
    selection: store.selection,
    setAnchor: (id) => store.setAnchor(Number(id)),
    naturalOf: (id) => store.naturalOf(Number(id)),
    ensureNatural: (ids) => {
      const base = root();
      if (base === null) return;
      const entries = ids.flatMap((id) => {
        const item = store.itemById(Number(id));
        return item === null ? [] : [{ id: item.id, path: joinPath(base, item.relPath) }];
      });
      return store.ensureNatural(entries);
    },
    onPreparingViewer: () => setLibsExpanded(false),
  });

  /** 「当前那张」的 id（键盘导航换了它之后把那一行滚进视野） */
  const focusId = (): string | undefined => {
    const item = store.anchorItem();
    return item === null ? undefined : String(item.id);
  };

  /**
   * 空态 / 加载 / 错误的水印（文案是浏览侧的：没选库 / 没选目录 / 空库）。
   * 这些条件本来写在 `BrowseGrid` 里 —— 那是**视图配置**，谁开这个视图谁给文案。
   */
  function gridWatermark(): JSX.Element | null {
    if (store.repositoryId() === null) {
      return (
        <StateWatermark
          icon={<IconAlbumOff size={64} stroke-width={1} />}
          text={t("browse.noRepository")}
        />
      );
    }
    if (store.scopePath() === null) {
      return (
        <StateWatermark
          icon={<IconFolder size={64} stroke-width={1} />}
          text={t("browse.pickDirectory")}
        />
      );
    }
    if (store.error() !== null && store.total() === 0) {
      return (
        <StateWatermark
          tone="error"
          icon={<IconAlertTriangle size={64} stroke-width={1} />}
          text={t("browse.load_error", { message: store.error() ?? "" })}
          action={{ label: t("common.retry"), run: () => void store.reload() }}
        />
      );
    }
    if (store.loading() && store.total() === 0) {
      return (
        <StateWatermark
          animate
          icon={<IconPhoto size={64} stroke-width={1} />}
          text={t("browse.loading")}
        />
      );
    }
    if (store.total() === 0) {
      return (
        <StateWatermark
          icon={<IconPhotoOff size={64} stroke-width={1} />}
          text={t("browse.empty_lib")}
        />
      );
    }
    return null;
  }

  /*
   * `Tab` 循环四态（`BROWSE.md` §5.4）：默认 → 仅关左 → 关两侧 → 仅 view。
   *
   * 监听挂在window上而不是看图件里：这是**外壳**的事（左右两列与胶片带的显隐），
   * 看图件只负责照片本身的缩放/平移（职责分开，换渲染层时这里不用动）。
   */
  /*
   * 键盘（`plans/M2-W2-tail.md` 5.2）：「事件 → 意图」的映射在 `features/browse/keys.ts`
   * （纯函数、逐条有测），这里只把意图落到 store / viewer / 弹窗上。
   *
   * 冲突与分工：
   * * `←`/`→`、`Esc` 在看图里走看图命令；单张 view 的 `Enter` 内建退出，compare 的
   *   `Enter` 走命令切换胶片带范围；这里只在网格里执行 `move`；
   * * **数字打星只在网格里生效** —— 看图里的 `0` / `1` 是既有的缩放快捷键（适配 / 100%），
   *   不让打星覆盖它；看图时打标用 `P` / `X` / `U` 与工具条；
   * * `Delete` 永远要确认（人类 2026-09-19 的批注：删除能批量，不需要 easy destroy）。
   */
  /*
   * 键盘与「主要操作」的入口统一交给**命令注册表**（`plans/M2-W3.md` §2.5）：
   * 这里只把自己那份**动作**注册进 `features/browse/actions.ts`，
   * 命令（`mark.rating.3` / `nav.open` / `viewer.close` / `view.chrome.cycle` …）通过它取用。
   *
   * 为什么不再自己挂 window 监听：那样「改键」会落空 ——
   * 用户把「打 3 星」改到别的键，这里却仍然只认 `3`。
   *
   * 分工（与 W2 的键位表一致；意图表 `lib/viewer-keys.ts` 仍保留作参考与单测）：
   * * 看图里的 `Esc` / 方向键 / `+−01` → `components/ui/viewer/actions.ts`（看图件自己注册）；
   * * 网格里的数字打星、`P/X/U`、`Delete`、`Ctrl+A`、`←/→`、`i` → 命令；
   * * **内建**（不走注册表）：单张 view 的 `Enter` 返回、网格里的 roving focus、弹窗与菜单内部；
   *   compare 的 `Enter` 已登记为 `viewer.compareOnly` 命令。
   */
  onMount(() => {
    registerBrowseActions({
      viewing: () => viewer.state().active,
      comparing: viewing.comparing,
      filmVisible: viewing.filmVisible,
      requestDelete: () => setPendingDelete(store.selectedCount()),
      moveFocus,
      openViewer: viewing.requestOpen,
      cycleChrome: viewing.cycleChrome,
      resetChrome: viewing.resetChrome,
      toggleCompareStrip: viewing.toggleCompareStrip,
      fullscreenTarget,
    });
    onCleanup(() => registerBrowseActions(null));
  });

  /** 移动「当前那张」（网格里 `←` / `→`）：等价于点它一下 —— 与点击同一套选择语义 */
  function moveFocus(delta: -1 | 1): void {
    const total = store.total();
    if (total === 0) return;
    const current = focusIndex();
    const next =
      current === undefined
        ? delta > 0
          ? 0
          : total - 1
        : Math.min(total - 1, Math.max(0, current + delta));
    const item = store.itemAt(next);
    if (item === null) {
      // 那一段还没取到（分页）：让它去取，并把焦点先挪过去，下一按就能落上
      void store.ensureRange(next, next + 1);
      setFocusIndex(next);
      return;
    }
    setFocusIndex(next);
    store.select(item.id, "replace");
  }

  /** 删除选中的照片：走回收站；结果用提示说清楚，失败清单进模态逐条列 */
  async function runDelete(): Promise<void> {
    setPendingDelete(null);
    const result = await store.removeSelected();
    if (result === null) return;
    if (result.deleted > 0) {
      props.toast?.show({
        tone: "success",
        message: t("browse.deleteDone").replace("{n}", String(result.deleted)),
      });
    }
    if (result.blockedLocked.length > 0) {
      props.toast?.show({
        tone: "danger",
        message: t("browse.deleteBlocked").replace("{n}", String(result.blockedLocked.length)),
      });
    }
    if (result.alreadyGone > 0) {
      props.toast?.show({
        tone: "info",
        message: t("browse.deleteGone").replace("{n}", String(result.alreadyGone)),
      });
    }
    if (result.failed.length > 0) {
      setDeleteFailures([...result.failed]);
    }
  }


  /*
   * 进浏览时把当前目录重读一遍（人类 2026-09-22 报的「导入完回浏览看不到新照片」）。
   *
   * 症状：先进浏览、再去导入、回浏览 —— 新导入的照片不在列表里，得重启程序才看得到。
   * 根因：列表只在**查询变化**时重新加载（`setRepository` / `setScope` / 筛选 / 排序），
   * 而「导入往里加了东西」不改变查询 —— 于是那份清单一直停在上一次读的样子。
   *
   * 为什么直接重读、不先做个「库变了吗」的判定（人类 2026-09-22 问过这笔账）：
   * 重读的代价就是**再付一次「进目录首屏」的钱** —— 而浏览的范围是**一个目录**
   *（真实库就是几百到几千张，时间线亚毫秒级）。就算拿 10 万张挤在一个目录里的
   *最坏情况考它（`cargo run --release -p raybend --example query-bench`）：
   *取页 0.5ms + 时间线 P95 142ms + 分面 P95 79ms，两百多毫秒、一次。
   * 换来的是「看到的就是库里的」这条硬口径（`AGENTS.md` §2.13），而代价是
   * 一个每次都要维护的「什么算更新」（updated_at / 版本号）——那个机制一旦漏埋一处，
   * 就变成「偶尔不更新」这种最难查的 bug。所以：**宁可多读一遍，不建第二套真相**。
   */
  onMount(() => {
    void store.refresh();
  });

  onMount(() => {
    void (async () => {
      try {
        const list = await listRepositories();
        setRepositories(list);
        // 第一次进来：默认选最近打开的库（没有就选第一个在线的）
        if (store.repositoryId() === null && list.length > 0) {
          const preferred =
            list.find((r) => r.online && r.lastOpenedAt !== null) ??
            list.find((r) => r.online) ??
            list[0];
          store.setRepository(preferred.id);
        }
      } catch (error) {
        // 拿不到库列表不该让工作区崩掉；但**不能装作「你没有库」** —— 把原因显示在左列。
        // 同时打一条控制台：只上界面、日志里查不到，排障时只能靠人转述一句文案。
        // 这是**控制台诊断**，不走语言包（`DESIGN.md` §11.1 的豁免项：终端/控制台输出）。
        console.error("[browse] 读库列表失败", error); // i18n-exempt: 控制台诊断，不是界面文案
        setRepositories([]);
        setReposError(error instanceof Error ? error.message : String(error));
      } finally {
        setReposLoading(false);
      }
    })();
  });

  /** 重新读一次库列表（重挂载、重建、改模版之后都会调它）。 */
  async function refreshRepositories(): Promise<void> {
    try {
      setRepositories(await listRepositories());
    } catch (error) {
      console.error("[browse] 重读库列表失败", error); // i18n-exempt: 控制台诊断，不是界面文案
    }
  }

  /**
   * 点离线图标：对登记过的路径重新找一遍（与导入侧同一套语义）。
   * 找不到**不是错误** —— 列表照旧显示离线徽标。
   *
   * ⚠️ 名字别叫 `remountRepository`（与上面 import 进来的那个**同名会自己调自己**，
   * 类型还会退化成 `void`）—— 这类影子错误编译期只说「类型不匹配」，很难一眼看出。
   */
  async function remountLibrary(id: string): Promise<void> {
    try {
      const updated = await remountRepository(id);
      setRepositories((prev) => prev.map((repo) => (repo.id === id ? updated : repo)));
    } catch (error) {
      console.error("[browse] 重挂载失败", error); // i18n-exempt: 控制台诊断
      await refreshRepositories();
    }
  }

  /** 库根目录（库内相对路径 → 绝对路径） */
  const root = createMemo(
    () => repositories().find((r) => r.id === store.repositoryId())?.root ?? null,
  );

  /**
   * 网格数据源（适配器）：把浏览 store 包成网格契约。
   *
   * 依赖里带上 `thumbs`（与胶片带共用那一条队列）与档位三个回调（受控）——
   * 网格自己不持有这些状态。
   *
   * ⚠️ 它**必须排在 `root` 之后**：`createMemo` 会立刻求值一次，
   * 而 `root()` 在它声明之前访问会触发 TDZ（真机冒烟实测：
   * `ReferenceError: Cannot access 'root' before initialization`，
   * 表现为「点了『浏览』状态切了但界面不动」）。
   */
  const gridSource = createMemo(() =>
    browseSource({
      store,
      // 传取值函数（不是值）：库列表是异步来的，见 `BrowseSourceDeps.root` 的说明
      root,
      thumbs,
      tileStep,
      setTileStep,
      // 档位落盘：拖拽结束时写一次（与导入侧同一份实现、不同作用域）
      commitTileStep: () => commitBrowseDisplayTileStep(),
      grouped,
      infoMode: browseInfoMode,
    }),
  );

  /*
   * **进目录时同步计数**（人类 2026-09-19 的数量体系）。
   *
   * 每次 scope 变化都读盘数一次这个目录（本目录 + 它自己的 `_RAW`），与 `app.db` 里那行对比；
   * 不一样就写回去、并把差值滚到库级汇总 —— 于是「程序外面往目录里加了照片」这种事实
   * 立刻体现在卡片上，不需要谁去点刷新。
   */
  createEffect(() => {
    const id = store.repositoryId();
    const scope = store.scopePath();
    if (id === null || scope === null) return;
    void syncDirectoryCounts(id, scope)
      .then((result: [[number, number], [number, number]] | null) => {
        // 数字变了才刷新列表（避免每次滚动都重读一遍库）
        if (result === null) return;
        const [, totals] = result;
        const current = repositories().find((repo) => repo.id === id);
        if (current === undefined) return;
        if (current.photosCount === totals[0] && current.imagesCount === totals[1]) return;
        void refreshRepositories();
      })
      .catch(() => {
        // 同步失败不打扰用户：卡片上的数字保持上一次已知的值
      });
  });

  /**
   * 右栏（与 flowbar 的 flowinfo）显示谁：多选时是**锚点**那张（`BROWSE.md` §5.10）。
   *
   * 规则本身住在 store 的 `anchorItem()` 里 —— 组装层读的是**同一个方法**，
   * 所以「右栏显示谁」与「flowinfo 显示谁」不可能两边走偏。
   */
  const anchor = createMemo(() => store.anchorItem());

  /**
   * 全屏看图要的清单（flowbar 那个全屏按钮 + `viewer.fullscreen` 命令）。
   *
   * 显示序用的是**与网格同一个函数、同一个数据源**（`photosFromSource(gridSource())`）——
   * 所以全屏里的 ←/→ 与网格里的邻居逐张一致，筛选 / 排序也一致；
   * 这里不另建一份清单（§2.12）。
   */
  const fullscreenTarget = createMemo(() =>
    buildFullscreenTarget(photosFromSource(gridSource()), store.selection().anchor),
  );

  /**
   * 状态条中间那段的**左侧**：`库名 / 最后一级目录`（`BROWSE.md` §5.10）。
   *
   * 文件名**不在这里拼**：那条状态条是两侧共用的组件，它自己会在后面接上
   * 「· 当前那张的文件名」（人类 2026-09-19：导入侧以前漏了这一段，统一时一起补）。
   */
  const currentLead = createMemo(() => {
    const repoName =
      repositories().find((r) => r.id === store.repositoryId())?.name ?? "";
    const scope = store.scopePath();
    const last = scope === null ? "" : (scope.split("/").pop() ?? "");
    return [repoName, last].filter((s) => s !== "").join(" / ");
  });

  /**
   * 看图态那条状态栏的内容（人类 2026-09-20：与 tiles **同一条**，只换内容）。
   *
   * 字段来自看图件当前那张（`ViewerPhoto.marks` / `flag`）—— 调用方本来就有，
   * 不为了显示四个数再问一次后端。
   */
  const viewingInfo = (): TilesViewingInfo => viewingInfoOf(viewer.current());

  return (
    <div
      class={["flex min-h-0 flex-1 flex-col", props.class ?? ""].filter(Boolean).join(" ")}
    >
      {/* 三列（看图的①③态就是这两列在不在） */}
      <div class="flex min-h-0 flex-1">
        {/* 左列（宽度可拖拽：手柄在它右边） */}
        <aside
          class={[
            "flex shrink-0 flex-col bg-surface-main",
            // 看图第 ② 档起左列**藏起来但不卸载**：卸载会把目录树的展开状态与滚动位置清掉，
            // 按一下 Tab 就白跑一趟（而且回来要重新读盘）。
            viewing.showsLeft() ? "" : "hidden",
          ]
            .filter(Boolean)
            .join(" ")}
          style={{ width: `${leftWidth()}px` }}
        >
          <BrowseLeftColumn
            store={store}
            repositories={repositories()}
            reposLoading={reposLoading()}
            reposError={reposError()}
            libsExpanded={libsExpanded()}
            onExpandLibs={() => setLibsExpanded(true)}
            onCollapseLibs={() => setLibsExpanded(false)}
            onOpenSettings={(id) => props.onOpenLibrarySettings?.(id)}
            onRemount={(id) => void remountLibrary(id)}
          />
        </aside>

        {/* 拖拽手柄：左列 ↔ 中列（看图第 ② 档起与左列一起收起来） */}
        <Show when={viewing.showsLeft()}>
          <ColumnHandle
            onDragStart={beginLeftResize}
            onDrag={dragLeft}
            onDragEnd={endLeftResize}
            onNudge={nudgeLeft}
          />
        </Show>

      {/* 中列 */}
      <main
        class="relative flex min-h-0 min-w-0 flex-1 flex-col bg-surface-bar"
        /*
         * 四态在 DOM 上留痕：左右两列的显隐已经生效（就是那两个 `hidden`），
         * **胶片带那一条要等 2.1 才落地** —— 但状态本身现在就可观测，
         * 冒烟断言与以后接胶片带都读这两个属性，不用另加一个全局状态。
         */
        data-chrome={viewing.chrome()}
        data-film={viewing.filmVisible() ? "on" : "off"}
      >
        {/** 筛选结果区（chips + 共 N 张 + 任一/全部）：看图时不占位置 */}
        <Show when={!viewer.state().active}>
          <FilterBar store={store} />
        </Show>

        {/**
         * 照片区：网格 + 看图盖层；看图时它下面接胶片带（`design/browse.md` §2.5）。
         *
         * `flex flex-col` 不能省：网格自己的根节点靠 `flex-1` 撑高，
         * 外面换成普通块级盒子的话它会直接塌成 0 高（虚拟列表一格都不渲染）。
         */}
        <PhotoViewingStage
          source={gridSource()}
          controller={viewing}
          thumbs={thumbs}
          filmStripStep={props.filmStripStep}
          onFilmStripStepChange={props.onFilmStripStepChange}
          tilesBar={{
            count: store.total(),
            selectedCount: store.selectedCount(),
            label: currentLead(),
            fileName: anchor()?.fileName ?? null,
            byTime: grouped(),
            onByTimeChange: (value) => setGrouped(value),
            infoMode: browseInfoMode(),
            onInfoToggle: cycleBrowseTileInfo,
            tileStep: tileStep(),
            onTileStepChange: setTileStep,
            onTileStepCommit: () => commitBrowseDisplayTileStep(),
            sort: {
              keys: Object.keys(SORT_LABELS) as NonNullable<BrowseSort["key"]>[],
              value: store.sort().key ?? "takenAt",
              labelOf: (key) => SORT_LABELS[key as NonNullable<BrowseSort["key"]>](),
              desc: store.sort().desc === true,
              onKeyChange: (key) =>
                store.setSort({ ...store.sort(), key: key as NonNullable<BrowseSort["key"]> }),
              onDirectionToggle: () =>
                store.setSort({ ...store.sort(), desc: !store.sort().desc }),
            },
          }}
          viewingInfo={viewingInfo()}
          focusId={focusId()}
          pinsKey={`${store.repositoryId() ?? ""}:${store.scopePath() ?? ""}:${store.filterMode() ? "on" : "off"}:${JSON.stringify(store.filter())}`}
          onInteract={() => setLibsExpanded(false)}
          onFocusIndex={(index) => setFocusIndex(index)}
          watermark={() => gridWatermark()}
        />
      </main>

      {/* 右列**没有把手**：宽度固定（人类 2026-09-19 定「把手只加左边界」） */}

      {/* 右列（宽度 = `--panel-w-right`，所有工作流通用；不支持拖拽） */}
      <aside
        class={[
          "w-panel-w-right flex shrink-0 flex-col bg-surface-main",
          viewing.showsRight() ? "" : "hidden",
        ]
          .filter(Boolean)
          .join(" ")}
      >
        {/*
          右栏在看图态换成**预览 + 直方图**（`BROWSE.md` §5.9）——
          看图件的 store 本身就是「当前看哪张 + 看到哪一块」的唯一事实来源，直接传进去。
        */}
        <AssetInfo
              store={store}
              item={anchor()}
              viewer={viewer.state().active ? viewer : null}
              /* 对比态不画视野框：好几个窗口，一个框描述不了 */
              comparing={viewing.comparing()}
              /* 「所属库」那一行：名字住在工作区（它拿着库列表） */
              repositoryName={
                repositories().find((repo) => repo.id === store.repositoryId())?.name ?? null
              }
            />
      </aside>
      </div>

      {/*
        删除确认（`AGENTS.md` §11.3 的定案 + 人类 2026-09-19 的批注）：
        **只走确认这一条路，不给 Shift 快通道，也不用 `easy destroy`** ——
        删除支持多选批量，而 easy destroy 那套是给「不做批量界面、又要连续快速删单张」准备的。
      */}
      <ConfirmDialog
        open={pendingDelete() !== null}
        title={t("browse.deleteTitle")}
        message={t("browse.deleteConfirm").replace("{n}", String(pendingDelete() ?? 0))}
        onConfirm={() => void runDelete()}
        onCancel={() => setPendingDelete(null)}
      />

      {/* 删除失败清单：一条都不许闷掉（用户得知道哪几个没删掉、为什么） */}
      <Dialog
        open={deleteFailures().length > 0}
        onOpenChange={(open) => {
          if (!open) setDeleteFailures([]);
        }}
        title={t("browse.deleteFailedTitle").replace("{n}", String(deleteFailures().length))}
        description={t("browse.deleteFailedHint")}
        footer={
          <Button variant="secondary" onClick={() => setDeleteFailures([])}>
            {t("common.close")}
          </Button>
        }
      >
        <ul class="flex max-h-64 flex-col gap-1 overflow-y-auto" data-delete-failures>
          <For each={deleteFailures()}>
            {(failure) => (
              <li class="flex flex-col gap-0.5 border-b border-line-1 pb-1 last:border-0">
                <span class="break-all text-fs-2 text-fg-1">{failure.path}</span>
                <span class="text-fs-0 text-danger">{failure.reason}</span>
              </li>
            )}
          </For>
        </ul>
      </Dialog>
    </div>
  );
}
