/**
 * 对比视图（`BROWSE.md` §5.7、`plans/M2-W2.md` 2.2–2.3）。
 *
 * ## 两层盒子：栏区是**窗口**，画框是内容（人类 2026-09-20 纠正）
 *
 * ```text
 * ┌─ 窗口 = 分栏分到的那一格（2 张一排 / 3 张一排 / 4 张 2×2）────┐
 * │   ┌─ 画框（基准比例，居中）──┐                                  │
 * │   │        图片内容           │   ← 放大时画框变大，超出部分由窗口裁掉 │
 * │   └────────────────────────┘                                  │
 * └───────────────────────────────────────────────────────────────┘
 * ```
 *
 * 曾经把**画框**当成裁剪边界（外层盒子缩到图片比例、`overflow` 挂在它上面），
 * 症状就是人类报的「放大后图片被限制在自己的图片幅面宽高内」——
 * 窗口明明还有地方，那一格却永远只在自己那个小盒子里动。现在的口径：
 *
 * * **窗口（栏区）是可见边界**：`overflow-hidden` 挂在窗口上，放大后图片能铺满整格；
 * * **画框不是边界**，它只决定「扣取区在适配时映射到哪里」—— 所有画幅共用同一个画框，
 *   所以位移天然同步（见 `lib/viewer-compare.ts` 的文件头）；
 * * 画框是 `contain` 出来的 ⇒ 图片**永远等比例**，不拉伸。
 *
 * ## 缩放 / 平移的单位：相对画框的倍数，住在**本视图**里
 *
 * 对比里各图的像素尺寸本来就不同（4000×3000 与 6000×4000），「同一个倍率」只能是
 * **相对画框**的倍数（`rel`）。这个单位不属于单张看图的 store（那是「原图像素 × zoom」），
 * 所以 `rel` / `pan` 归这里：对比不会污染单张看图进入前的倍率与位置，
 * 单张看图也不必理解画框。数学仍只有一份 —— `lib/viewer-compare.ts::compareGeometry`
 * 与 `store.ts` 的 `clampPan` / `zoomPanAt`（都是纯函数、有单测）。
 *
 * 平移是**同一个 CSS 像素位移对所有画幅生效**：所有扣取区都映射到同一个画框，
 * 所以同一个屏幕位移对每一幅而言就是同一个画框百分比
 * （`plans/M2-W2.md` 2.3 的「位移按百分比同步」）。
 *
 * ## 交互
 *
 * | 操作 | 行为 |
 * | --- | --- |
 * | 滚轮 | 以**光标**为锚缩放（按帧合并，与单张看图共用 `interaction.ts` 那一份） |
 * | 双击 | **适配 ↔ 100%**（100% = 基准那幅的原图像素 1:1） |
 * | 拖动 | 平移；内容比窗口小时锁在中间，放大后不许拖出边界 |
 * | 点某一格 | 设为**当前照片**（右栏与底部状态栏跟着走，选择集合不变） |
 * | `+` / `-` / `0` / `1` | 与单张看图同一套（对比态下单张看图件没挂载，这些键在这儿接） |
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

import { t } from "../../../i18n/index.ts";
import { COMPARE_MAX, compareGeometry, compareLayout } from "../../../lib/viewer-compare.ts";
import {
  clampPan,
  MAX_ZOOM,
  MIN_ZOOM,
  zoomPanAt,
  type ViewerPhoto,
  type ViewerStore,
  type ViewportSize,
} from "./store.ts";
import {
  createWheelZoom,
  isViewerControlTarget,
  viewerControlsVisible,
} from "./interaction.ts";
import { ViewerControls } from "./ViewerControls.tsx";

/** 格间距与四周内边距（CSS px）——量栏区尺寸时要减掉它们 */
const COMPARE_GAP = 8;
const COMPARE_PADDING = 16;

export interface CompareViewProps {
  /** 参与对比的照片（已按显示顺序、已截到上限，顺序里第一个是画幅比例基准） */
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
  /** 相对画框的倍数：1 = 适配；`oneToOneRel` = 基准图 1:1 */
  const [rel, setRel] = createSignal(1);
  /** 平移（CSS 像素，相对窗口中心）—— 所有画幅共用同一个值 */
  const [pan, setPan] = createSignal({ x: 0, y: 0 });
  const [dragging, setDragging] = createSignal<{ x: number; y: number } | null>(null);
  const [cursor, setCursor] = createSignal<{ x: number; y: number } | null>(null);

  const layout = () => compareLayout(props.photos.length);
  const geometry = createMemo(() => compareGeometry(props.photos, pane()));
  /** 适配状态 = 相对倍数回到 1（它们是一回事，不另设一个会失同步的标志） */
  const fitted = () => rel() === 1;

  /** 内容盒 = 画框 × 相对倍数 —— 平移夹取看它（窗口比它大就锁在中间） */
  const contentBox = (scale: number): ViewportSize => ({
    width: geometry().frame.width * scale,
    height: geometry().frame.height * scale,
  });

  /** 相对倍数的上下限：换算自单张看图那一套像素倍率界限（`MIN_ZOOM`..`MAX_ZOOM`） */
  const relLimits = (): { min: number; max: number } => {
    const oneToOne = geometry().oneToOneRel;
    if (oneToOne === null || !(oneToOne > 0)) return { min: 1, max: 1 };
    return { min: MIN_ZOOM * oneToOne, max: MAX_ZOOM * oneToOne };
  };

  const clampRel = (value: number): number => {
    if (!Number.isFinite(value) || value <= 0) return 1;
    const { min, max } = relLimits();
    return Math.min(max, Math.max(min, value));
  };

  /** 缩放：以 `anchor`（窗口坐标）为锚，**锚点下的内容不动** */
  const zoomAt = (factor: number, anchor?: { x: number; y: number }): void => {
    if (!Number.isFinite(factor) || factor <= 0) return;
    const viewport = pane();
    if (viewport.width <= 0 || viewport.height <= 0) return;
    const current = rel();
    const next = clampRel(current * factor);
    if (next === current) return;
    const moved = zoomPanAt({
      pan: pan(),
      zoom: current,
      nextZoom: next,
      viewport,
      ...(anchor === undefined ? {} : { anchor }),
    });
    setRel(next);
    setPan(clampPan({ pan: moved, content: contentBox(next), viewport }));
  };

  /** 平移：夹取到「内容盒 ↔ 窗口」之间 */
  const panBy = (dx: number, dy: number): void => {
    const viewport = pane();
    setPan((previous) => {
      const next = clampPan({
        pan: { x: previous.x + dx, y: previous.y + dy },
        content: contentBox(rel()),
        viewport,
      });
      return next.x === previous.x && next.y === previous.y ? previous : next;
    });
  };

  /** 适配：画框铺成窗口内最大的等比例盒子，居中 */
  const fitTo = (): void => {
    setRel(1);
    setPan({ x: 0, y: 0 });
  };

  /** 100%：基准那幅回到原图像素 1:1（尺寸未知时没有 1:1 可言，退到适配） */
  const goToOneToOne = (): void => {
    const oneToOne = geometry().oneToOneRel;
    if (oneToOne === null) {
      fitTo();
      return;
    }
    setRel(clampRel(oneToOne));
    setPan({ x: 0, y: 0 });
  };

  /**
   * 双击：**适配 ↔ 100%**（与单张看图同一条口径）。
   *
   * 已在 100% 上再双击也回适配 —— 判据用 `fitted()`，而不是「等于 oneToOne」：
   * 用户滚到 100% 附近再双击，期望的也是「回适配」。
   */
  const toggleFit = (): void => {
    if (fitted()) {
      goToOneToOne();
      return;
    }
    fitTo();
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
    // 窗口尺寸或倍数变了：把已有平移收回新边界（内容比窗口小时会被锁回中间）
    const viewport = pane();
    const scale = rel();
    setPan((previous) => {
      const next = clampPan({ pan: previous, content: contentBox(scale), viewport });
      return next.x === previous.x && next.y === previous.y ? previous : next;
    });
  });

  /**
   * 换了一组画幅（选择变化）才重置成「适配」。
   *
   * 判据是**照片集合的指纹**而不是 `props.photos` 的引用 —— 后者每次读都是新数组
   * （调用方现算的），拿它当依赖会把「点某一格切当前照片」也当成换组。
   */
  let photoKey = "";
  createEffect(() => {
    const nextKey = props.photos.map((photo) => `${photo.id}\u0000${photo.path}`).join("\u0001");
    if (nextKey === photoKey) return;
    photoKey = nextKey;
    fitTo();

    // 同一套 store 负责取图；这里只表达「这几张现在都需要」，不另写第二套加载器
    for (const photo of props.photos) void props.store.ensureImage(photo);

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
   * 锚点取「光标落在哪一格」，再换算成**那一格的本地坐标**：各格一样大、显示的内容
   * 也按同一个画框对齐，所以在哪一格上缩放，锚到的都是同一个内容点。
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

  /* 键盘：与单张看图同一套（那边在对比态下没有挂载，所以这些键在这儿接） */
  onMount(() => {
    const onKey = (event: KeyboardEvent): void => {
      if (event.defaultPrevented || !props.store.state().active) return;
      switch (event.key) {
        case "+":
        case "=":
          event.preventDefault();
          zoomAt(1.25);
          break;
        case "-":
          event.preventDefault();
          zoomAt(1 / 1.25);
          break;
        case "0":
          event.preventDefault();
          fitTo();
          break;
        case "1":
          event.preventDefault();
          goToOneToOne();
          break;
        default:
          break;
      }
    };
    window.addEventListener("keydown", onKey);
    onCleanup(() => window.removeEventListener("keydown", onKey));
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

  /** 读数：相对倍数 → 基准那幅的**原图像素比例**（100% 就是 1:1） */
  const zoomLabel = (): string => {
    const oneToOne = geometry().oneToOneRel;
    if (fitted() || oneToOne === null || !(oneToOne > 0)) return t("viewer.fit");
    return `${Math.round((rel() / oneToOne) * 100)}%`;
  };

  const focusFromTarget = (target: EventTarget | null): void => {
    if (!(target instanceof Element)) return;
    const frame = target.closest<HTMLElement>("[data-compare-photo-id]");
    const id = frame?.dataset.comparePhotoId;
    if (id === undefined) return;
    const photo = props.photos.find((candidate) => candidate.id === id);
    if (photo !== undefined) props.onFocus?.(photo);
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
      class={[
        "absolute inset-0 z-10 grid overflow-hidden bg-surface-bar",
        dragging() === null ? "cursor-grab" : "cursor-grabbing",
        props.class ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
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
        toggleFit();
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

      <For each={geometry().frames}>
        {(frame, at) => (
          <div
            data-compare-frame={at()}
            data-compare-photo-id={frame.photo.id}
            data-baseline={at() === 0 ? "true" : undefined}
            data-current={props.store.current()?.id === frame.photo.id ? "true" : undefined}
            aria-label={frame.photo.fileName}
            /* 窗口：可见/裁剪的边界（图片放大后铺满整格，而不是被自己的画框关住） */
            class={[
              "relative flex h-full min-w-0 cursor-pointer items-center justify-center overflow-hidden rounded-ui bg-surface-main",
              // 当前那张（点哪格就是哪张）用主色描边点明；其余只用底色分格
              props.store.current()?.id === frame.photo.id
                ? "border border-brand"
                : "border border-transparent",
            ].join(" ")}
            /* 保留 click 入口给键盘/自动化；真实指针在 pointerdown 已先切焦点 */
            onClick={() => props.onFocus?.(frame.photo)}
          >
            <Show
              when={frame.crop.width > 0 && frame.image.width > 0}
              fallback={
                <span class="text-fs-2 text-fg-3">{t("browse.compareNoSize")}</span>
              }
            >
              {/*
                画框：等比例内容盒，由 flex 居中；放大 = 整体缩放（`transform-origin: center`）。
                `overflow-hidden` 在这一层是**为了扣取区**：扣出来的那块映射满画框，
                画框之外的原图内容（比例不一致时被扣掉的部分）不该露出来。
                超出窗口的部分由**窗口**裁掉 —— 这一层不是可见边界。
              */}
              <div
                data-compare-canvas
                class="relative shrink-0 overflow-hidden"
                style={{
                  width: `${geometry().frame.width}px`,
                  height: `${geometry().frame.height}px`,
                  transform: `translate3d(${pan().x}px, ${pan().y}px, 0) scale(${rel()})`,
                  "transform-origin": "center",
                  "will-change": "transform",
                }}
              >
                <Show when={props.store.imageUrlFor(frame.photo)}>
                  {(url) => (
                    <img
                      class="pointer-events-none absolute max-w-none select-none"
                      src={url()}
                      alt=""
                      draggable={false}
                      style={{
                        width: `${frame.image.width}px`,
                        height: `${frame.image.height}px`,
                        left: `${frame.imageOffset.x}px`,
                        top: `${frame.imageOffset.y}px`,
                      }}
                    />
                  )}
                </Show>
              </div>
            </Show>
          </div>
        )}
      </For>

      <ViewerControls
        store={props.store}
        onClose={close}
        visible={controlsVisible()}
        zoomLabel={zoomLabel}
        onZoomOut={() => zoomAt(1 / 1.25)}
        onZoomIn={() => zoomAt(1.25)}
        onFit={fitTo}
      />
    </div>
  );
}
