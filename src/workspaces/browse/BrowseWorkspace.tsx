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

import { getThumbBytes, getViewImage, listRepositories } from "../../api/db.ts";
import type { RepositoryView } from "../../api/types.ts";
import {
  AssetInfo,
  BrowseGrid,
  BrowseLeftColumn,
  ViewerStatusBar,
  type BrowseStore,
} from "../../features/browse/index.ts";
import {
  chromeShowsFilm,
  chromeShowsSides,
  nextChrome,
  type ViewerChrome,
} from "../../features/browse/chrome.ts";
import { FilmStrip } from "../../features/browse/FilmStrip.tsx";
import { FilterBar } from "../../features/browse/FilterBar.tsx";
import { CompareView } from "../../features/browse/CompareView.tsx";
import { compareIds } from "../../features/browse/compare.ts";
import { createThumbQueue } from "../../components/ui/thumb-queue.ts";
import { createViewerStore, Viewer } from "../../components/ui/viewer/index.ts";
import { clampTileStepIndex, DEFAULT_TILE_STEP_INDEX, TILE_SIZE_STEPS, tileSizeAt } from "../../lib/tile-flow.ts";
import { t } from "../../i18n/index.ts";
import { Slider } from "../../components/ui/Slider.tsx";
import { Button } from "../../components/ui/Button.tsx";
import { ConfirmDialog, Dialog } from "../../components/ui/Dialog.tsx";
import type { ToastStore } from "../../components/ui/Toast.tsx";
import { browseKeyIntent, shouldHandleKey } from "../../features/browse/keys.ts";
import { SplitHandle } from "../../components/ui/SplitHandle.tsx";
import { nudgeWidth, resizeWidth } from "../../lib/column-resize.ts";

/** 侧栏宽度的上下限（与 `lib/layout-prefs.ts` 的 LAYOUT_BOUNDS 一致；两处都要有：
 *  那边挡存储里的垃圾值，这里挡拖拽本身） */
const SIDEBAR_BOUNDS = { min: 220, max: 520 } as const;
import { Menu } from "../../components/ui/Menu.tsx";
import { IconArrowDown, IconArrowUp } from "@tabler/icons-solidjs";
import type { BrowseSort, DeleteFailure } from "../../api/types.ts";

export interface BrowseWorkspaceProps {
  store: BrowseStore;
  /**
   * 提示通道（`components/ui/Toast.tsx`）：删除这类**改磁盘**的动作必须给回执 ——
   * 谁删了什么、几个被锁挡住、哪几个失败了，都在提示与失败清单里说清楚。
   */
  toast?: ToastStore;
  /** 左列 / 右列宽度（像素，受控；拖拽松手时通过下面的回调落盘） */
  leftWidth?: number;
  rightWidth?: number;
  onLeftWidthChange?: (width: number) => void;
  onRightWidthChange?: (width: number) => void;
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
  const [tileStep, setTileStep] = createSignal(DEFAULT_TILE_STEP_INDEX);
  const [grouped, setGrouped] = createSignal(false);
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
      return photo === undefined ? [] : [photo];
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

  /** 进看图：顺便把展开的库列表收了（`BROWSE.md` §4.2 的第二个收起条件）。 */  function openViewer(photos: Parameters<typeof viewer.show>[0], index: number): void {
    setLibsExpanded(false);
    // 每次进看图都从「① 默认」开始（退出时重置，这两处合起来保证「左右栏必定回来」）
    setChrome("default");
    viewer.show(photos, index);
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

  const root = createMemo(
    () => repositories().find((r) => r.id === store.repositoryId())?.root ?? null,
  );

  /**
   * 右栏（与 flowbar 的 flowinfo）显示谁：多选时是**锚点**那张（`BROWSE.md` §5.10）。
   *
   * 规则本身住在 store 的 `anchorItem()` 里 —— 组装层读的是**同一个方法**，
   * 所以「右栏显示谁」与「flowinfo 显示谁」不可能两边走偏。
   */
  const anchor = createMemo(() => store.anchorItem());

  /** 状态栏中间那段：`库名 / 最后一级目录 · 文件名`（`BROWSE.md` §5.10）。 */
  const currentLabel = createMemo(() => {
    const repoName =
      repositories().find((r) => r.id === store.repositoryId())?.name ?? "";
    const scope = store.scopePath();
    const last = scope === null ? "" : (scope.split("/").pop() ?? "");
    const file = anchor()?.fileName ?? "";
    const left = [repoName, last].filter((s) => s !== "").join(" / ");
    return file === "" ? left : `${left}${left === "" ? "" : " · "}${file}`;
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
        <div class="relative flex min-h-0 flex-1 flex-col">
          <BrowseGrid
            store={store}
            root={root()}
            resetKey={`${store.repositoryId() ?? ""}:${store.scopePath() ?? ""}`}
            tileStep={tileStep()}
            grouped={grouped()}
            thumbs={thumbs}
            focusIndex={focusIndex()}
            onInteract={() => setLibsExpanded(false)}
            onFocusIndex={(index) => setFocusIndex(index)}
            onOpenViewer={openViewer}
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
                onFocus={(photo) => {
                  /*
                   * 2.5（`BROWSE.md` §5.7）：点哪张图就是**当前**那张 ——
                   * 两件事一起做：看图件切到它（状态栏/胶片带跟着），
                   * 选择集合里的**锚点**挪到它（右栏信息跟它）。
                   * 注意下边用的是 `setAnchor` 而不是 `select` —— 后者会散掉对比。
                   */
                  const at = viewer.state().photos.findIndex((item) => item.id === photo.id);
                  if (at >= 0) viewer.goTo(at);
                  store.setAnchor(Number(photo.id));
                }}
              />
            </Show>
          </Show>
        </div>

        {/*
          控制条（`design/browse.md` §2.3）：计数 — 当前目录/文件名 — 按时间 + 缩放。
          看图时**不出现** —— 那时中列下面是胶片带，底部那条是全宽的看图状态栏。
        */}
        <Show when={!viewer.state().active}>
          <div class="flex h-8 shrink-0 items-center gap-3 px-2 text-fs-2 text-fg-3">
          <span>{t("browse.count").replace("{n}", String(store.total()))}</span>
          <Show when={store.selectedCount() > 0}>
            <span class="text-fg-2">
              {t("browse.selected").replace("{n}", String(store.selectedCount()))}
            </span>
          </Show>

          <span class="min-w-0 flex-1 truncate text-center text-fg-3">
            {currentLabel()}
          </span>

          <button
            type="button"
            onClick={() => setGrouped(!grouped())}
            class={[
              "h-6 shrink-0 rounded-(--radius) px-2",
              grouped()
                ? "bg-state-selected text-fg-1"
                : "text-fg-3 hover:bg-state-hover",
            ].join(" ")}
          >
            {t("grid.by_time")}
          </button>

          {/* 排序：键 + 方向（画布 ② 的 SortBar 挪到这里，见 FilterBar 的文件头说明） */}
          <div class="flex shrink-0 items-center gap-1" data-sort>
            <span class="text-fg-3">{t("browse.sort")}</span>
            <Menu
              label={t("browse.sort")}
              placement="top"
              items={(
                Object.keys(SORT_LABELS) as NonNullable<BrowseSort["key"]>[]
              ).map((key) => ({
                value: key,
                label: SORT_LABELS[key](),
                selected: (store.sort().key ?? "takenAt") === key,
              }))}
              onSelect={(value) =>
                store.setSort({ ...store.sort(), key: value as NonNullable<BrowseSort["key"]> })
              }
            >
              {(triggerProps) => (
                <button
                  {...triggerProps()}
                  class="rounded-ui px-1.5 py-0.5 text-fs-2 text-fg-2 hover:bg-state-hover hover:text-fg-1"
                >
                  {SORT_LABELS[store.sort().key ?? "takenAt"]()}
                </button>
              )}
            </Menu>
            <button
              type="button"
              aria-label={store.sort().desc ? t("browse.sortDesc") : t("browse.sortAsc")}
              class="flex h-5 w-5 items-center justify-center rounded-ui text-fg-3 hover:bg-state-hover hover:text-fg-1"
              onClick={() => store.setSort({ ...store.sort(), desc: !store.sort().desc })}
            >
              {store.sort().desc ? <IconArrowDown size={13} /> : <IconArrowUp size={13} />}
            </button>
          </div>

          <span class="shrink-0 text-fg-3">{tileSizeAt(tileStep())}px</span>
          <div class="w-24 shrink-0">
            <Slider
              min={0}
              max={TILE_SIZE_STEPS.length - 1}
              step={1}
              value={tileStep()}
              label={t("grid.zoom")}
              onValueChange={(value) => setTileStep(clampTileStepIndex(value))}
            />
          </div>
          </div>
        </Show>

        {/*
          胶片带（`BROWSE.md` §5.5）：看图时在照片区下面形成，横向滚动选图。
          列表就是看图件手里那一份 —— 两边永远同源。
          `Tab` 第③态（`view-only`，只看图）把它藏起来（`chromeShowsFilm`）。
        */}
        <Show when={viewer.state().active && chromeShowsFilm(chrome())}>
          <FilmStrip
            viewer={viewer}
            store={store}
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
        <AssetInfo item={anchor()} viewer={viewer.state().active ? viewer : null} />
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
