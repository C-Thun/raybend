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

import { createMemo, createSignal, createEffect, onCleanup, onMount, Show } from "solid-js";

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
import { CompareView } from "../../features/browse/CompareView.tsx";
import { compareIds } from "../../features/browse/compare.ts";
import { createThumbQueue } from "../../components/ui/thumb-queue.ts";
import { createViewerStore, Viewer } from "../../components/ui/viewer/index.ts";
import { clampTileStepIndex, DEFAULT_TILE_STEP_INDEX, TILE_SIZE_STEPS, tileSizeAt } from "../../lib/tile-flow.ts";
import { t } from "../../i18n/index.ts";
import { Slider } from "../../components/ui/Slider.tsx";

export interface BrowseWorkspaceProps {
  store: BrowseStore;
  class?: string;
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

  /** 右栏显示谁：多选时是锚点那张（`BROWSE.md` §5.10）。 */
  const anchor = createMemo(() => {
    const id = store.anchorId();
    if (id === null) return null;
    for (const item of store.selectedItems()) {
      if (item.id === id) return item;
    }
    return store.selectedItems()[0] ?? null;
  });

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
        {/* 左列 */}
        <aside
        class={[
          "flex w-[300px] shrink-0 flex-col border-r border-line-1 bg-surface-main",
          // 看图 ②「关左右」时**藏起来但不卸载**：卸载会把目录树的展开状态与滚动位置清掉，
          // 按一下 Tab 就白跑一趟（而且回来要重新读盘）。
          chromeShowsSides(chrome()) ? "" : "hidden",
        ]
          .filter(Boolean)
          .join(" ")}
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
            onInteract={() => setLibsExpanded(false)}
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
          `Tab` 第③态（`no-film`）把它藏起来（`chromeShowsFilm`）。
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

      {/* 右列 */}
      <aside
        class={[
          "flex w-[300px] shrink-0 flex-col border-l border-line-1 bg-surface-main",
          chromeShowsSides(chrome()) ? "" : "hidden",
        ]
          .filter(Boolean)
          .join(" ")}
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
    </div>
  );
}
