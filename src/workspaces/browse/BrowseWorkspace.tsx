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
  ViewerStatusBar,
  type BrowseStore,
} from "../../features/browse/index.ts";
import {
  chromeShowsFilm,
  chromeShowsSides,
  nextChrome,
  type ViewerChrome,
} from "../../lib/viewer-chrome.ts";
import { FilmStrip } from "../../components/ui/viewer/index.ts";
import { FilterBar } from "../../features/browse/FilterBar.tsx";
import { browseSource } from "../../features/browse/grid-source.ts";
import { PhotoGrid } from "../../features/photo-grid/index.ts";
import { StateWatermark } from "../../components/ui/StateWatermark.tsx";
import {
  IconAlbumOff,
  IconAlertTriangle,
  IconFolder,
  IconPhoto,
  IconPhotoOff,
} from "@tabler/icons-solidjs";
import { CompareView } from "../../components/ui/viewer/index.ts";
import { compareIds } from "../../lib/viewer-compare.ts";
import { createThumbQueue } from "../../components/ui/thumb-queue.ts";
import { TilesShell } from "../../components/ui/tiles/index.ts";
import { createViewerStore, Viewer } from "../../components/ui/viewer/index.ts";
import { clampTileStepIndex } from "../../lib/tile-flow.ts";
import {
  commitDisplayTileStep,
  displayByTime,
  displayTileStep,
  setDisplayByTime,
  setDisplayTileStep,
} from "../../lib/display-prefs.ts";
import { t } from "../../i18n/index.ts";
import { Button } from "../../components/ui/Button.tsx";
import { ConfirmDialog, Dialog } from "../../components/ui/Dialog.tsx";
import type { ToastStore } from "../../components/ui/Toast.tsx";
import { browseKeyIntent, shouldHandleKey } from "../../lib/viewer-keys.ts";
import { cycleTileInfo, infoKeyApplies } from "../../components/ui/tile-info.ts";
import { SplitHandle } from "../../components/ui/SplitHandle.tsx";
import { nudgeWidth, resizeWidth } from "../../lib/column-resize.ts";
import { joinPath } from "../../lib/paths.ts";
import { LAYOUT_BOUNDS } from "../../lib/layout-prefs.ts";

/**
 * 侧栏宽度的上下限 = **唯一那一份**（`lib/layout-prefs.ts` 的 `LAYOUT_BOUNDS`）。
 *
 * 早先这里手抄了一份（220/520），于是把下限抬到 264 时**只有存储那一侧生效**、
 * 拖拽这一侧纹丝不动 —— 同一件事两处写，改一处另一处不生效（人类 2026-09-19 抓到的
 * 正是这类问题）。现在两边共用一条。
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
   * 右列宽度（像素）。**不受控、不可拖** —— 按 `DESIGN.md` §8.6，把手只加在左侧边界上，
   * 右列宽度是常量（`BROWSE_RIGHT_WIDTH`），所以这里没有对应的 `on…Change`。
   */
  rightWidth?: number;
  onLeftWidthChange?: (width: number) => void;
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
   * 「按时间」与格子尺寸档位都是**共享的设备级偏好**（`lib/display-prefs.ts`）：
   * 与导入侧读同一份、且跨会话还原。
   *
   * 以前这里是两个本地信号（`createSignal(false)` / 默认档），于是浏览里开了「按时间」、
   * 切走再回来就重置（人类 2026-09-20 报的）；导入侧的同类状态也各存各的。
   */
  const tileStep = displayTileStep;
  const setTileStep = setDisplayTileStep;
  const grouped = displayByTime;
  const setGrouped = setDisplayByTime;
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
  /**
   * 右列宽度**固定**，不参与拖拽（人类 2026-09-19 定：右列之后另有安排）。
   * 左列才需要可调 —— 用户改 tile 尺寸时，右列固定会冒出"网格撑不满"的空档。
   */
  const rightWidth = (): number => props.rightWidth ?? 300;
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

  /** 看图的三种显示状态（`Tab` 循环；退出看图时重置为默认）。 */
  const [chrome, setChrome] = createSignal<ViewerChrome>("default");
  /**
   * 对比态的「只看对比图」胶片带（`BROWSE.md` §5.5）：对比中再按一次**回车**。
   *
   * 退出条件写在派生逻辑里（`createEffect`）而不是各处手动清 ——
   * 「只剩一幅就退出」是**选择状态**决定的事，手动清一定会漏。
   */
  const [compareStrip, setCompareStrip] = createSignal(false);

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

  /**
   * 对比态（`plans/M2-W2.md` 2.2）：**没有「进入对比」这个动作**。
   *
   * 它直接由**选择状态**派生 —— 看图模式 + 选中 ≥ 2 张 ⇒ 对比；
   * 反选到只剩 1 张 ⇒ 自动回到单张看图。所以上面这几个都是 `createMemo` 意义上的
   * 派生值，而不是一份可以「忘了同步」的额外状态。
   */
  const comparedIds = () =>
    compareIds(
      store.selection().ids,
      viewer.state().photos.map((photo) => photo.id),
      store.selection().anchor,
    );
  /**
   * 实际进画幅的那几张。
   *
   * ⚠️ **按窗口顺序映射**，不是按显示顺序过滤 —— 窗口从左到右就是选择先后
   * （最早选中的在左、它是画幅基准），与胶片带也同序。
   */
  const comparePhotos = () => {
    const byId = new Map(viewer.state().photos.map((photo) => [photo.id, photo]));
    return comparedIds().flatMap((id) => {
      const photo = byId.get(id);
      if (photo === undefined) return [];
      /*
       * 宽高**以 store 为准**：看图件手里那份是进看图那一刻的快照，
       * 而老库的尺寸可能是回来之后才补读到的（`ensureNatural`）——
       * 不覆盖这一层的话，对比画幅会一直卡在「还没读到这张的尺寸」。
       */
      const natural = store.naturalOf(Number(id));
      return [natural === null ? photo : { ...photo, natural }];
    });
  };
  /** 选中总数（>4 时界面上说明「只对比最近选中的 4 张」，不静默截断） */
  const compareSelectedCount = () => store.selectedCount();
  const comparing = () => viewer.state().active && comparePhotos().length >= 2;

  /*
   * 「只剩一幅」时自动退出「只看对比图」那种胶片带状态（`BROWSE.md` §5.5 的收尾）。
   *
   * 胶片带的滚动定位交给胶片带自己：它盯的是「当前那张」，模式一变就会重新滚进视野 ——
   * 那正是「退出时定位到这最后一张图的位置」要的效果。
   */
  createEffect(() => {
    if (!comparing()) setCompareStrip(false);
  });

  /*
   * 看图关掉 ⇒ 通知网格把焦点要回去（`BROWSE.md` §5.4 的键盘接续）。
   *
   * 挂在「看图开没开」这个状态上，是因为**所有**关闭路径最后都会落到它
   * （`Esc`、对比态的「返回」、看图件自己的关闭按钮）—— 只做一次、只有一份实现。
   */
  const [focusNudge, setFocusNudge] = createSignal(0);
  createEffect<boolean | undefined>((wasActive) => {
    const active = viewer.state().active;
    if (wasActive === true && !active) setFocusNudge((n) => n + 1);
    return active;
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

  /**
   * 进看图**之前**要做的事（`PhotoGrid.onOpeningViewer`）：收起展开的库列表、
   * 把三态复位到「① 默认」。
   *
   * 真正的 `viewer.show()` 由网格做（它手上有显示序的完整清单）——
   * 工作区负责的是「外壳状态」，两件事分开（所以这个回调不接参数）。
   */
  function prepareViewer(): void {
    setLibsExpanded(false);
    // 每次进看图都从「① 默认」开始（退出时重置，这两处合起来保证「左右栏必定回来」）
    setChrome("default");
  }

  /*
   * `Tab` 循环三态（`BROWSE.md` §5.4，人类 2026-09-18 定为**并列三态**）。
   *
   * 监听挂在window上而不是看图件里：这是**外壳**的事（左右两列与胶片带的显隐），
   * 看图件只负责照片本身的缩放/平移（职责分开，换渲染层时这里不用动）。
   */
  /*
   * 键盘（`plans/M2-W2-tail.md` 5.2）：「事件 → 意图」的映射在 `features/browse/keys.ts`
   * （纯函数、逐条有测），这里只把意图落到 store / viewer / 弹窗上。
   *
   * 冲突与分工：
   * * `←`/`→`、`Esc`、`Enter` 在**看图里**由看图件自己接（它知道切哪张、怎么退），
   *   这里只在网格里执行 `move`；
   * * **数字打星只在网格里生效** —— 看图里的 `0` / `1` 是既有的缩放快捷键（适配 / 100%），
   *   不让打星覆盖它；看图时打标用 `P` / `X` / `U` 与工具条；
   * * `Delete` 永远要确认（人类 2026-09-19 的批注：删除能批量，不需要 easy destroy）。
   */
  onMount(() => {
    const onKey = (event: KeyboardEvent): void => {
      const modal = document.querySelector('[role="dialog"]') !== null;
      if (!shouldHandleKey(event.target as HTMLElement | null, modal)) return;
      /*
       * `i`：切 tiles 的「信息」档位 —— **只在 tiles / film 下生效**
       * （人类 2026-09-19 定的范围；2026-09-20 补上浏览侧这一半 —— 之前只有导入侧接了，
       * 浏览里按 `i` 毫无反应）。判据走 `components/ui/tile-info.ts` 那**一份**实现。
       */
      if (event.key === "i" || event.key === "I") {
        const viewing = viewer.state().active;
        if (!infoKeyApplies({ viewing, filmVisible: chromeShowsFilm(chrome()) })) return;
        event.preventDefault();
        cycleTileInfo();
        return;
      }
      const intent = browseKeyIntent(event, {
        viewing: viewer.state().active,
        hasSelection: store.selectedCount() > 0,
      });
      if (intent === null) return;
      switch (intent.kind) {
        case "move":
          if (!viewer.state().active) {
            event.preventDefault();
            moveFocus(intent.delta);
          }
          return;
        case "rating":
          if (!viewer.state().active) {
            void store.mark({ kind: "rating", value: intent.value });
          }
          return;
        case "flag":
          void store.setFlag(store.selectedIds(), intent.value);
          return;
        case "delete":
          event.preventDefault();
          setPendingDelete(store.selectedCount());
          return;
        case "clear-selection":
          store.clearSelection();
          return;
        case "select-all":
          // 只挡浏览器默认行为（否则整页文字会被选中变蓝），选择本身交给 store
          event.preventDefault();
          store.selectAll();
          return;
        default:
          // viewer-prev / viewer-next / open-viewer / close-viewer 各有接的人了
          return;
      }
    };
    window.addEventListener("keydown", onKey);
    onCleanup(() => window.removeEventListener("keydown", onKey));
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

  onMount(() => {
    const onKey = (event: KeyboardEvent): void => {
      if (!viewer.state().active) return;
      /*
       * 对比态下的**回车**（`BROWSE.md` §5.5、`plans/M2-W2.md` 2.4）：
       * 胶片带切成「只显示参与对比的图」那种特殊状态，再按一次回去。
       *
       * 为什么这一条在**外壳**而不是对比视图里：它改的是**胶片带显示什么**
       * （布局级的状态），而不是照片本身的缩放/平移。
       */
      if (event.key === "Enter") {
        if (!comparing()) return;
        event.preventDefault();
        setCompareStrip((on) => !on);
        return;
      }
      /*
       * 对比态下的 **Esc**：退回 tiles。
       *
       * 为什么这一条必须在外壳里：单张看图件自己接 Esc（关闭自己），但**对比态下
       * 单张看图件根本没挂载** —— 没人接这个键，用户按 Esc 会没反应
       * （2026-09-18 冒烟实测：对比后按 Esc，看图还开着）。
       */
      if (event.key === "Escape" && comparing()) {
        event.preventDefault();
        setChrome("default");
        viewer.close();
        return;
      }
      if (event.key !== "Tab") return;
      event.preventDefault();
      setChrome((current) => nextChrome(current));
    };
    window.addEventListener("keydown", onKey);
    onCleanup(() => window.removeEventListener("keydown", onKey));
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
      // 档位落盘：拖拽结束时写一次（与导入侧同一份实现、同一个键）
      commitTileStep: () => commitDisplayTileStep(),
      grouped,
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

  /*
   * 进对比（选中 ≥ 2 张）就把**对比那几张**的宽高补齐。
   *
   * 与导入侧同一条纪律、同一个理由：网格只为可见 tile 读过元数据，
   * 而且老库里 `assets.width/height` 可能是 NULL（2026-09-18 之前的导入不写 EXIF）——
   * 缺了它，对比的基准比例算不出来，画幅只能显示「还没读到这张的尺寸」。
   * 只补对比集（通常 ≤4 张），不去碰整个目录。
   */
  createEffect(() => {
    const ids = comparedIds();
    if (ids.length < 2) return;
    const base = root();
    if (base === null) return;
    const entries = ids.flatMap((id) => {
      const item = store.itemById(Number(id));
      return item === null ? [] : [{ id: item.id, path: joinPath(base, item.relPath) }];
    });
    void store.ensureNatural(entries);
  });

  /**
   * 右栏（与 flowbar 的 flowinfo）显示谁：多选时是**锚点**那张（`BROWSE.md` §5.10）。
   *
   * 规则本身住在 store 的 `anchorItem()` 里 —— 组装层读的是**同一个方法**，
   * 所以「右栏显示谁」与「flowinfo 显示谁」不可能两边走偏。
   */
  const anchor = createMemo(() => store.anchorItem());

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
            // 看图 ②「关左右」时**藏起来但不卸载**：卸载会把目录树的展开状态与滚动位置清掉，
            // 按一下 Tab 就白跑一趟（而且回来要重新读盘）。
            chromeShowsSides(chrome()) ? "" : "hidden",
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

        {/* 拖拽手柄：左列 ↔ 中列（看图「关左右」时手柄一起收起来） */}
        <Show when={chromeShowsSides(chrome())}>
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
         * 三态在 DOM 上留痕：左右两列的显隐已经生效（就是那两个 `hidden`），
         * **胶片带那一条要等 2.1 才落地** —— 但状态本身现在就可观测，
         * 冒烟断言与以后接胶片带都读这两个属性，不用另加一个全局状态。
         */
        data-chrome={chrome()}
        data-film={chromeShowsFilm(chrome()) ? "on" : "off"}
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
        {/*
          tiles = **网格 + 下面那条状态条**（人类 2026-09-19：业务上不可分割）。
          这里与导入侧用的是同一个 `TilesShell` / `TilesControlBar`；
          差异按人列的清单走**显式配置**：浏览侧把 `sort` 传上（导入侧暂时不传）。

          看图态不给 `bar`：那时中列下面是胶片带，状态条让位给它。
        */}
        <TilesShell
          bar={
            viewer.state().active
              ? null
              : {
                  count: store.total(),
                  selectedCount: store.selectedCount(),
                  // 中间那段的左侧：**库名 / 最后一级目录**（浏览侧不显示完整路径）
                  label: currentLead(),
                  // 右侧：当前那张（多选时是锚点那张）
                  fileName: anchor()?.fileName ?? null,
                  byTime: grouped(),
                  onByTimeChange: (value) => setGrouped(value),
                  tileStep: tileStep(),
                  onTileStepChange: (step) => setTileStep(clampTileStepIndex(step)),
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
                }
          }
        >
          {/*
            网格是**全项目唯一那一份**（`features/photo-grid/PhotoGrid.tsx`）。
            浏览侧的差异（分页 / 显示序置换 / 标记 / 绝对路径）全在数据源适配器里
            （`features/browse/grid-source.ts`）—— 人类 2026-09-19：「同一个东西
            两个组件本身就是 bug」，`BrowseGrid` 已删除。
          */}
          <PhotoGrid
            source={gridSource()}
            viewer={viewer}
            /* 与胶片带共用同一个缩略图队列 / viewer，所以这两样都不传（由适配器与上面提供） */
            focusNudge={focusNudge()}
            focusId={focusId()}
            pinsKey={`${store.repositoryId() ?? ""}:${store.scopePath() ?? ""}:${store.filterMode() ? "on" : "off"}:${JSON.stringify(store.filter())}`}
            onInteract={() => setLibsExpanded(false)}
            onFocusIndex={(index) => setFocusIndex(index)}
            onOpeningViewer={() => prepareViewer()}
            watermark={() => gridWatermark()}
          />

          {/*
            看图盖在**照片区**上（不是整个中列）—— 胶片带要留在它下面可见。
            网格**不卸载** —— 滚动位置才留得住，M2-W1 踩过这个坑。
            `Viewer` 自己就是 `absolute inset-0`，所以这里只要求父级 `relative`。
          */}
          <Show when={viewer.state().active}>
            {/*
              选中 ≥ 2 张时是**对比**（`plans/M2-W2.md` 2.2）：同一个看图件提供倍率与位移，
              对比视图负责把多幅画幅摆开并做百分比同步。
              否则是单张看图。
            */}
            <Show
              when={comparing()}
              fallback={
                <Viewer
                  store={viewer}
                  class="z-10"
                  onClose={() => {
                    // 退回 tiles 时左右栏必定回来（BROWSE.md §5.4）
                    setChrome("default");
                  }}
                />
              }
            >
              <CompareView
                photos={comparePhotos()}
                selectedCount={compareSelectedCount()}
                store={viewer}
                /* 对比态的返回（左上角那颗）：与单张看图同一条规矩 —— 外壳复位三态 */
                onClose={() => setChrome("default")}
                onFocus={(photo) => {
                  /*
                   * 2.5（`BROWSE.md` §5.7）：点哪张图就是**当前**那张 ——
                   * 两件事一起做：看图件切到它（状态栏/胶片带跟着），
                   * 选择集合里的**锚点**挪到它（右栏信息跟它）。
                   * 注意下边用的是 `setAnchor` 而不是 `select` —— 后者会散掉对比。
                   */
                  const at = viewer.state().photos.findIndex((item) => item.id === photo.id);
                  // 对比里只换「当前照片」，不能像胶片带那样重置共同缩放与位移。
                  if (at >= 0) viewer.focus(at);
                  store.setAnchor(Number(photo.id));
                }}
              />
            </Show>
          </Show>
        </TilesShell>

        {/*
          胶片带（`BROWSE.md` §5.5）：看图时在照片区下面形成，横向滚动选图。
          列表就是看图件手里那一份 —— 两边永远同源。
          `Tab` 第③态（`view-only`，只看图）把它藏起来（`chromeShowsFilm`）。
        */}
        <Show when={viewer.state().active && chromeShowsFilm(chrome())}>
          <FilmStrip
            viewer={viewer}
            selectedIds={store.selection().ids}
            onSelect={(id, mode) =>
              store.select(
                Number(id),
                mode,
                viewer.state().photos.map((photo) => photo.id),
              )
            }
            thumbs={thumbs}
            onlyIds={compareStrip() ? comparedIds() : undefined}
          />
        </Show>
      </main>

      {/* 右列**没有把手**：宽度固定（人类 2026-09-19 定，之后另有安排） */}

      {/* 右列 */}
      <aside
        class={[
          "flex shrink-0 flex-col bg-surface-main",
          chromeShowsSides(chrome()) ? "" : "hidden",
        ]
          .filter(Boolean)
          .join(" ")}
        style={{ width: `${rightWidth()}px` }}
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
              comparing={comparing()}
              /* 「所属库」那一行：名字住在工作区（它拿着库列表） */
              repositoryName={
                repositories().find((repo) => repo.id === store.repositoryId())?.name ?? null
              }
            />
      </aside>
      </div>

      {/*
        看图态的底部状态栏（`BROWSE.md` §5.8）：**全宽、在三列下面** ——
        与画布的 `ViewerStatusBar` 一致（它不在中列里，而是在整个工作区下面）。
        只在看图时出现；tiles 模式下底部那条是控制条（计数/当前目录/视图控制）。
      */}
      <Show when={viewer.state().active}>
        <ViewerStatusBar photo={viewer.current()} />
      </Show>

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
