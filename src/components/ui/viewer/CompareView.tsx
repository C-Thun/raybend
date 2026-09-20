/**
 * 对比视图（`BROWSE.md` §5.7、`plans/M2-W2.md` 2.2–2.3）。
 *
 * ## 一张**虚拟画布**，几个窗口（人类 2026-09-20 定的新方案）
 *
 * ```text
 * ┌─ 窗口 = 分栏分到的那一格（2 张一排 / 3 张一排 / 4 张 2×2）────┐
 * │  ┌─ 虚拟画布（各图最大宽 × 最大高）──────────────────┐         │
 * │  │        ┌─────┐                                  │         │
 * │  │        │ 竖图 │   ← 每张图按**自己的原图尺寸**居中放进画布 │
 * │  │        └─────┘                                  │         │
 * │  │  ┌───────────┐                                  │         │
 * │  │  └───────────┘                                  │         │
 * │  └─────────────────────────────────────────────────┘         │
 * └──────────────────────────────────────────────────────────────┘
 * ```
 *
 * 这是**取代**旧方案（以第一幅比例扣等比例区域）的口径，起因是人类报的具体问题：
 * 竖图打头时横图被裁成竖比例，看着「没问题」，但**一放大就露馅** —— 用户想看被裁掉的
 * 那部分。新方案里「不裁不缩」：所有图按原图像素居中贴进同一张画布，小的图两头留空。
 *
 * ## 谁拥有缩放 / 平移
 *
 * 画布坐标 = **原图像素**，所以这里的倍率是**绝对倍率**（画布像素 → CSS 像素），
 * 1 = 100% = 1:1。这个单位与单张看图的 store 一致，但**适配语义不同**：
 * 对比的「适合窗口」是「**以某张图为基准** contain 进栏区」（双击哪张就按哪张算），
 * 而且同时有好几个窗口。所以 `zoom` / `pan` / `fitId` 住在**本视图**里，
 * 单张看图的 store 不被污染；数学仍只有一份 —— `lib/viewer-compare.ts` 的
 * `compareCanvas` / `canvasAspect` / `smallestByPixels` / `fitAspectWithin`
 * 与 store 的纯函数 `clampPan` / `clampZoom` / `computeFitScale` / `zoomPanAt`。
 *
 * ## 交互
 *
 * | 操作 | 行为 |
 * | --- | --- |
 * | 进入 / 改变对比集合 | 重算画布，重新**居中**，并按**像素数最小**的那张算「适合窗口」 |
 * | 滚轮 | 以**光标**为锚缩放（按帧合并，与单张看图共用一份实现） |
 * | 双击 | **适合窗口 ↔ 100%**：适合窗口按**双击的那张图**算，其他图跟着这个倍率 |
 * | 拖动 | 平移（按**画布**坐标夹取；图可能被移出窗口，但总有图在窗口里） |
 * | 点某一格 | 设为**当前照片**（右栏与底部状态栏跟着走，选择集合不变） |
 * | `+` / `-` / `0` / `1` | 与单张看图同一套（对比态下单张看图件没挂载，这些键在这儿接） |
 */

import {
  batch,
  createEffect,
  createMemo,
  createSignal,
  Index,
  onCleanup,
  onMount,
  Show,
  type JSX,
} from "solid-js";

import { t } from "../../../i18n/index.ts";
import {
  canvasAspect,
  COMPARE_MAX,
  compareCanvas,
  compareLayout,
  fitAspectWithin,
  smallestByPixels,
} from "../../../lib/viewer-compare.ts";
import {
  clampPan,
  clampZoom,
  computeFitScale,
  zoomPanAt,
  type ViewerPhoto,
  type ViewerStore,
  type ViewportSize,
} from "./store.ts";
import {
  createWheelZoom,
  isViewerControlTarget,
  takeViewerFocus,
  viewerControlsVisible,
} from "./interaction.ts";
import { ViewerControls } from "./ViewerControls.tsx";
import { registerViewerActions } from "./actions.ts";

/** 格间距与四周内边距（CSS px）——量栏区尺寸时要减掉它们 */
const COMPARE_GAP = 8;
const COMPARE_PADDING = 16;

export interface CompareViewProps {
  /** 参与对比的照片（已按显示顺序、已截到上限） */
  photos: readonly ViewerPhoto[];
  /** 选中总数；大于 4 时说明界面只显示最近选择的 4 张。 */
  selectedCount?: number;
  /** 点哪幅就把哪幅设为当前照片；不改变选择集合。 */
  onFocus?: (photo: ViewerPhoto) => void;
  /** 取图与「当前照片」的来源（缩放/平移**不用**它，见文件头）。 */
  store: ViewerStore;
  /** 返回 tiles（左上角按钮）。 */
  onClose?: () => void;
  class?: string;
}

export function CompareView(props: CompareViewProps): JSX.Element {
  let host: HTMLDivElement | undefined;

  /** 栏区（窗口）尺寸：由 host 尺寸与行列数算出（每格一样大，所以只量一次） */
  const [pane, setPane] = createSignal<ViewportSize>({ width: 0, height: 0 });
  /**
   * 用户自己给的绝对倍率（画布像素 → CSS 像素，1 = 100% = 1:1）。
   *
   * 它只是「**不在适配状态**时用哪个倍率」；真正生效的倍率是下面那个 memo ——
   * 适配状态（`fitId` 非空）以**那张图**算出来的倍率为准。
   * 这样「按哪张图适配」就不会出现「谁最后写 zoom 谁赢」的自激（2026-09-20 踩过：
   * effect 里写 zoom 会把双击刚设好的 100% 立刻改回适配值）。
   */
  const [manualZoom, setManualZoom] = createSignal(1);
  /** 平移（CSS 像素，相对窗口中心）—— 所有画幅共用同一个值 */
  const [pan, setPan] = createSignal({ x: 0, y: 0 });
  /**
   * 当前「适合窗口」是**按哪张图**算的（照片 id）；`null` = 用户自己缩放过 / 已是 100%。
   *
   * 有它才做得到两件人类点名的事：① 双击在哪张图上就按哪张算；② 尺寸是**后来才补读**
   * 到的（老库）或栏区被拉大了，只要还在「适合窗口」状态就跟着重算。
   */
  const [fitId, setFitId] = createSignal<string | null>(null);
  const [dragging, setDragging] = createSignal<{ x: number; y: number } | null>(null);
  const [cursor, setCursor] = createSignal<{ x: number; y: number } | null>(null);

  const layout = () => compareLayout(props.photos.length);
  const canvas = createMemo(() => compareCanvas(props.photos));
  /**
   * 画布在窗口里的**布局盒**（contain）：缩放只是它上面的一条 `transform`，
   * 不改布局 ⇒ 滚轮缩放不触发重排（四幅一起缩放也不掉帧）。
   */
  const layoutBox = createMemo(() => fitAspectWithin(pane(), canvasAspect(canvas())));
  /** 画布像素 → 布局盒 CSS 像素（布局比例尺）。倍率换算都从它出发 */
  const layoutBase = (): number => {
    const width = canvas().size.width;
    return width > 0 ? layoutBox().width / width : 0;
  };
  /**
   * 某张图「适合窗口」的倍率：把**它自己** contain 进当前栏区。
   *
   * 栏区尺寸与这张图的尺寸都会**自动**重新触发它（尺寸是老库后来才补读到的也一样），
   * 不需要额外去盯事件。
   */
  const fitZoomFor = (photo: ViewerPhoto | undefined): number =>
    photo === undefined
      ? 1
      : clampZoom(computeFitScale(pane(), photo.natural ?? { width: 0, height: 0 }));

  /**
   * 生效的倍率（唯一事实来源）：
   *
   * * `fitId` 非空 ⇒ 按**那张图**重新算（这就是「统一倍率，其他图跟着调」）；
   * * 否则用用户自己给的那个（滚轮 / 加减 / 100%）。
   */
  const zoom = createMemo(() => {
    const id = fitId();
    if (id === null) return manualZoom();
    const photo = props.photos.find((candidate) => candidate.id === id);
    return photo === undefined ? manualZoom() : fitZoomFor(photo);
  });

  /** 画布在 `transform` 里的缩放：布局盒早就 contain 好了，剩下的倍率由它补 */
  const transformScale = (): number => {
    const base = layoutBase();
    return base > 0 ? zoom() / base : 1;
  };
  /** 内容盒（= 画布 × 倍率）：平移夹取看它 */
  const contentBox = (scale: number): ViewportSize => ({
    width: canvas().size.width * scale,
    height: canvas().size.height * scale,
  });

  /** 适配到某张图（居中）。其他图跟着同一个倍率走 —— 这就是「统一倍率」 */
  const fitTo = (photo: ViewerPhoto): void => {
    batch(() => {
      setFitId(photo.id);
      setPan({ x: 0, y: 0 });
    });
  };

  /** 100%：画布 1:1（与单张看图同一口径） */
  const goToOneToOne = (): void => {
    batch(() => {
      setFitId(null);
      setManualZoom(clampZoom(1));
      setPan({ x: 0, y: 0 });
    });
  };

  /** 缩放：以 `anchor`（窗口坐标）为锚，锚点下的内容不动；手动缩放即退出「适合窗口」状态 */
  const zoomAt = (factor: number, anchor?: { x: number; y: number }): void => {
    if (!Number.isFinite(factor) || factor <= 0) return;
    const viewport = pane();
    if (viewport.width <= 0 || viewport.height <= 0) return;
    const current = zoom();
    const next = clampZoom(current * factor);
    if (next === current) return;
    const moved = zoomPanAt({
      pan: pan(),
      zoom: current,
      nextZoom: next,
      viewport,
      ...(anchor === undefined ? {} : { anchor }),
    });
    // 三件事一次落：退出适配状态 + 记下新倍率 + 夹取平移（中间态不该被看见）
    batch(() => {
      setFitId(null);
      setManualZoom(next);
      setPan(clampPan({ pan: moved, content: contentBox(next), viewport }));
    });
  };

  /** 平移：按**画布**夹取（画布比窗口小时锁在中间） */
  const panBy = (dx: number, dy: number): void => {
    const viewport = pane();
    setPan((previous) => {
      const next = clampPan({
        pan: { x: previous.x + dx, y: previous.y + dy },
        content: contentBox(zoom()),
        viewport,
      });
      return next.x === previous.x && next.y === previous.y ? previous : next;
    });
  };

  /** 当前那张（右栏/状态栏跟着走的那张）——「适合窗口」按钮以它为准 */
  const currentPhoto = (): ViewerPhoto | undefined => {
    const id = props.store.current()?.id;
    return props.photos.find((photo) => photo.id === id) ?? props.photos[0];
  };

  /** 是不是正处在 100%（画布 1:1） */
  const atOneToOne = (): boolean => Math.abs(zoom() - 1) < 1e-6;

  /**
   * **双击**某格：在 **100% ↔ 适合窗口** 之间切（人类 2026-09-20 报「双击放大缩小还是不行」）。
   *
   * 口径（与单张看图一致、但锚点不同）：
   *
   * * 已经在 100% ⇒ 按**被双击的那张**算适合窗口；
   * * 其它任何状态（适配中、或者用户滚到过 250%）⇒ **直接到 100%**。
   *
   * 为什么不是「先按这张适配、再点一次才到 100%」：那样在**适配目标不是这张**时，
   * 第一下双击看起来什么都没发生（换了个适配基准而已）—— 人只会说「双击没反应」。
   * 双击是「我要看像素」的动作，一次就要到 100%。
   */
  const toggleZoom = (photo: ViewerPhoto): void => {
    if (atOneToOne()) {
      fitTo(photo);
      return;
    }
    goToOneToOne();
  };

  /**
   * 右下角那颗「适配」键：**按当前那张图适配 ↔ 100%** 之间切。
   *
   * 人类 2026-09-20：「compare 下点右下角缩放可以切到适应窗口，但再次点击切不到 100%（单图 view 下可以）」——
   * 单张看图那颗键本来就是 `toggleFit()`（适配 ↔ 100%），对比这边之前只做了「适配」那一半。
   */
  const toggleFitOfCurrent = (): void => {
    const photo = currentPhoto();
    if (photo === undefined) return;
    if (fitId() === photo.id) {
      goToOneToOne();
      return;
    }
    fitTo(photo);
  };

  /**
   * 量**栏区**：host 减掉内边距与格间距，再按行列均分。
   * 量一次就够（每格一样大），不必给每幅都挂一个 observer。
   */
  const measure = (): void => {
    if (host === undefined) return;
    const { columns, rows } = layout();
    setPane({
      width: Math.max(
        0,
        (host.clientWidth - COMPARE_PADDING - COMPARE_GAP * (columns - 1)) / columns,
      ),
      height: Math.max(
        0,
        (host.clientHeight - COMPARE_PADDING - COMPARE_GAP * (rows - 1)) / rows,
      ),
    });
  };

  onMount(() => {
    /*
     * 对比面也**接管键盘焦点**（与单张看图同一条规矩，
     * 见 `interaction.ts::takeViewerFocus`）：多图对比可能从 tiles 里多选后直接进来，
     * 焦点如果留在 tile 上，回车就会被 tile 当成「激活」吃掉。
     */
    takeViewerFocus(host);
    if (host === undefined) return;
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(host);
    onCleanup(() => observer.disconnect());
  });

  createEffect(() => {
    // 行列数变了 host 尺寸未必变，显式补量一次
    layout();
    measure();
  });

  createEffect(() => {
    // 窗口尺寸或倍率变了：把已有平移收回新边界（画布比窗口小时会被锁回中间）
    const viewport = pane();
    const scale = zoom();
    setPan((previous) => {
      const next = clampPan({ pan: previous, content: contentBox(scale), viewport });
      return next.x === previous.x && next.y === previous.y ? previous : next;
    });
  });

  /**
   * 换了一组画幅（选择变化）才重置：重算画布、画面**居中**、按**像素数最小**的那张算适合窗口。
   *
   * 判据是**照片集合的指纹**而不是 `props.photos` 的引用 —— 后者每次读都是新数组
   * （调用方现算的），拿它当依赖会把「点某一格切当前照片」也当成换组。
   */
  let photoKey = "";
  createEffect(() => {
    const nextKey = props.photos.map((photo) => `${photo.id}\u0000${photo.path}`).join("\u0001");
    if (nextKey === photoKey) return;
    photoKey = nextKey;

    // 同一套 store 负责取图；这里只表达「这几张现在都需要」，不另写第二套加载器
    for (const photo of props.photos) void props.store.ensureImage(photo);

    const smallest = smallestByPixels(props.photos);
    if (smallest === null) {
      /*
       * 尺寸都还没读到：先按「适配到第一张」摆着（它的尺寸一旦补读回来，
       * `zoom` 那个 memo 会自己按新尺寸重算 —— 不需要额外的 effect 盯）。
       */
      const first = props.photos[0];
      if (first === undefined) {
        setFitId(null);
        setManualZoom(clampZoom(1));
        setPan({ x: 0, y: 0 });
      } else {
        fitTo(first);
      }
    } else {
      fitTo(smallest);
    }

    const currentId = props.store.current()?.id;
    if (!props.photos.some((photo) => photo.id === currentId)) {
      const first = props.photos[0];
      if (first !== undefined) props.onFocus?.(first);
    }
  });

  /*
   * 滚轮：按帧合并 + 指数映射，**与单张看图共用** `interaction.ts::createWheelZoom`
   * （两处的滚轮手感必须一致，也不能各写一份累计逻辑）。
   *
   * 锚点取「光标落在哪一格的本地坐标」：所有格子一样大、显示的是**同一张画布**，
   * 所以在哪一格上缩放，锚到的都是同一个画布点。
   */
  const paneRectFor = (target: EventTarget | null): DOMRect | null => {
    if (!(target instanceof Element)) return null;
    return target.closest<HTMLElement>("[data-compare-frame]")?.getBoundingClientRect() ?? null;
  };
  const wheel = createWheelZoom({
    resolveAnchor: (event) => {
      const rect = paneRectFor(event.target);
      return rect === null
        ? null
        : { x: event.clientX - rect.left, y: event.clientY - rect.top };
    },
    apply: (factor, anchor) => zoomAt(factor, anchor ?? undefined),
  });
  onCleanup(wheel.dispose);

  /*
   * 键盘交给**命令分发器**（`plans/M2-W3.md` §2.5 步骤 3）：对比态的缩放/适配与单张看图
   * 是**同一条命令**（`viewer.zoomIn` / `viewer.fit` …），只是实现不同 ——
   * 这里把对比自己那份（逐幅画幅的百分比同步）注册进 `viewer/actions.ts`。
   * `Esc`（返回）与 `Tab`（四态）由工作区那边注册的动作负责。
   */
  onMount(() => {
    registerViewerActions({
      zoomIn: () => zoomAt(1.25),
      zoomOut: () => zoomAt(1 / 1.25),
      // 适配 = 以当前那张为准（与右下那颗「适配」按钮同一条口径：再按一次去 100%）
      toggleFit: () => toggleFitOfCurrent(),
      actual: () => goToOneToOne(),
      next: () => props.store.next(),
      prev: () => props.store.prev(),
      // 注意用**内部**的 `close()`（它会连 `onClose` 一起叫 —— 工作区靠它复位四态），
      // 不是 `props.store.close()`（那只会关掉 store）
      close: () => close(),
    });
    onCleanup(() => registerViewerActions(null));
  });

  const controlsVisible = (): boolean =>
    viewerControlsVisible(cursor(), {
      width: host?.clientWidth ?? 0,
      height: host?.clientHeight ?? 0,
    });

  const close = (): void => {
    props.store.close();
    props.onClose?.();
  };

  /** 读数：绝对倍率（100% = 1:1）；处在「适合窗口」状态时只说「适配」 */
  const zoomLabel = (): string => {
    if (fitId() !== null && fitId() === props.store.current()?.id) return t("viewer.fit");
    return `${Math.round(zoom() * 100)}%`;
  };

  const focusFromTarget = (target: EventTarget | null): ViewerPhoto | undefined => {
    if (!(target instanceof Element)) return undefined;
    const frame = target.closest<HTMLElement>("[data-compare-photo-id]");
    const id = frame?.dataset.comparePhotoId;
    if (id === undefined) return undefined;
    const photo = props.photos.find((candidate) => candidate.id === id);
    if (photo !== undefined) props.onFocus?.(photo);
    return photo;
  };

  const onPointerDown = (event: PointerEvent): void => {
    if (event.button !== 0 || isViewerControlTarget(event.target)) return;
    // pointer capture 会改变后续 click 的目标，所以在捕获前就确定当前画幅
    focusFromTarget(event.target);
    setDragging({ x: event.clientX, y: event.clientY });
    host?.setPointerCapture(event.pointerId);
  };

  const trackCursor = (event: PointerEvent): void => {
    const rect = host?.getBoundingClientRect();
    if (rect !== undefined) {
      setCursor({ x: event.clientX - rect.left, y: event.clientY - rect.top });
    }
  };

  const onPointerMove = (event: PointerEvent): void => {
    trackCursor(event);
    const from = dragging();
    if (from === null) return;
    setDragging({ x: event.clientX, y: event.clientY });
    // pan 的单位就是 CSS px：鼠标走 80px，画面也走 80px
    panBy(event.clientX - from.x, event.clientY - from.y);
  };

  const endDrag = (event: PointerEvent): void => {
    if (dragging() === null) return;
    setDragging(null);
    if (host?.hasPointerCapture(event.pointerId)) host.releasePointerCapture(event.pointerId);
  };

  return (
    <div
      ref={host}
      data-compare="open"
      data-compare-count={props.photos.length}
      data-compare-cols={layout().columns}
      data-compare-rows={layout().rows}
      data-compare-zoom={zoom()}
      data-compare-fit={fitId() ?? "none"}
      class={[
        "absolute inset-0 z-10 grid overflow-hidden bg-surface-bar",
        // 接管焦点用（`takeViewerFocus`）；`outline-none`：整块画面不该出现聚焦环
        "outline-none",
        dragging() === null ? "cursor-grab" : "cursor-grabbing",
        props.class ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
      // `-1`：可编程聚焦，但不进 Tab 序列
      tabindex="-1"
      style={{
        gap: `${COMPARE_GAP}px`,
        padding: `${COMPARE_PADDING / 2}px`,
        "grid-template-columns": `repeat(${layout().columns}, minmax(0, 1fr))`,
        "grid-template-rows": `repeat(${layout().rows}, minmax(0, 1fr))`,
      }}
      onWheel={wheel.onWheel}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerLeave={() => setCursor(null)}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
      onDblClick={(event) => {
        // 双击落在缩放/返回按钮上时不当成「切换适配」—— 那两个按钮自己有点击行为
        if (isViewerControlTarget(event.target)) return;
        const photo = focusFromTarget(event.target) ?? currentPhoto();
        if (photo !== undefined) toggleZoom(photo);
      }}
    >
      <Show when={(props.selectedCount ?? 0) > COMPARE_MAX}>
        <span
          data-compare-note="open"
          class="absolute bottom-1 left-1/2 z-10 -translate-x-1/2 rounded-ui bg-surface-layer px-2 py-1 text-fs-0 text-fg-3"
        >
          {t("browse.compareLimit").replace("{n}", String(props.selectedCount ?? 0))}
        </span>
      </Show>

      {/*
        ⚠️ 必须用 `Index` 而不是 `For`（2026-09-20 真机踩过）：
        `canvas()` 是个 memo，而 `compareCanvas()` **每次都返回新的落点对象** ——
        用 `For`（按**引用**认身份）时，任何一次重算都会把这一格的 DOM 整个销毁重建。
        后果不只是闪：**真双击的第二下会落在新建的元素上，浏览器根本不发 dblclick**
        （人类两次报「双击不行」的根因就是这个；合成的 `new MouseEvent("dblclick")` 绕过它，
        所以冒烟一直是假绿）。
        `Index` 按**下标**认身份 —— 格子始终是同一批 DOM，只有内容跟着信号更新。
      */}
      <Index each={canvas().placements}>
        {(placement, at) => (
          <div
            data-compare-frame={at}
            data-compare-photo-id={placement().photo.id}
            data-current={props.store.current()?.id === placement().photo.id ? "true" : undefined}
            aria-label={placement().photo.fileName}
            /* 窗口：可见/裁剪的边界（画布放大后铺满整格，而不是被画布关住） */
            class={[
              "relative flex h-full min-w-0 cursor-pointer items-center justify-center overflow-hidden rounded-ui bg-surface-main",
              // 当前那张（点哪格就是哪张）用主色描边点明；其余只用底色分格
              props.store.current()?.id === placement().photo.id
                ? "border border-brand"
                : "border border-transparent",
            ].join(" ")}
            /* 保留 click 入口给键盘/自动化；真实指针在 pointerdown 已先切焦点 */
            onClick={() => props.onFocus?.(placement().photo)}
          >
            <Show
              when={canvas().size.width > 0 && placement().natural.width > 0}
              fallback={
                <span class="text-fs-2 text-fg-3">{t("browse.compareNoSize")}</span>
              }
            >
              {/*
                **同一张画布**在每一格里的一个副本：尺寸与变换完全一样，差别只是里面
                只放这一格的图。所以「统一倍率、按画布对位」是构造出来的，不靠逐帧同步。

                位置用**百分比**（相对画布）：与倍率、与栏区大小都无关 ——
                缩放只改父级那条 `transform`，图片自身的样式一动不动。
              */}
              <div
                data-compare-canvas
                class="relative shrink-0"
                style={{
                  width: `${layoutBox().width}px`,
                  height: `${layoutBox().height}px`,
                  transform: `translate3d(${pan().x}px, ${pan().y}px, 0) scale(${transformScale()})`,
                  "transform-origin": "center",
                  "will-change": "transform",
                }}
              >
                <Show when={props.store.imageUrlFor(placement().photo)}>
                  {(url) => (
                    <img
                      class="pointer-events-none absolute max-w-none select-none"
                      src={url()}
                      alt=""
                      draggable={false}
                      style={{
                        left: `${(placement().offset.x / canvas().size.width) * 100}%`,
                        top: `${(placement().offset.y / canvas().size.height) * 100}%`,
                        width: `${(placement().natural.width / canvas().size.width) * 100}%`,
                        height: `${(placement().natural.height / canvas().size.height) * 100}%`,
                      }}
                    />
                  )}
                </Show>
              </div>
            </Show>
          </div>
        )}
      </Index>

      <ViewerControls
        store={props.store}
        onClose={close}
        visible={controlsVisible()}
        zoomLabel={zoomLabel}
        onZoomOut={() => zoomAt(1 / 1.25)}
        onZoomIn={() => zoomAt(1.25)}
        onFit={toggleFitOfCurrent}
      />
    </div>
  );
}
