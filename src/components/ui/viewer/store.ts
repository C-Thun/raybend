/**
 * 看图的**状态与变换数学**（`design/main.md` §3.2 的「图片查看」）。
 *
 * ## 这一版的范围（人类 2026-09-16 定的）
 *
 * 「双击/回车等放大查看图片的功能，先做一版基础的出来（**不带胶片带**版本），
 * 这块不涉及 rust 原生，只用前端技术做，做的时候为后续的功能要考虑到扩展性，
 * 同时注意执行效率，避免卡顿。」
 *
 * 所以：
 *
 * * **只做前端**（DOM + CSS transform），不动 wgpu —— 真·查看器属于 M2-W3；
 * * 这一版**没有胶片带**（`FUTURE.md` H9 的对比模式、BROWSE.md 的胶片带都还没做）；
 * * **扩展性**：状态与视图分离。这里只管「看哪张、缩放多少、平移多少」，
 *   日后加胶片带 / 对比 / 评级，是往这个 store 上加状态，不是重写视图；
 * * **执行效率**：变换只走 `translate3d + scale`（合成器就能干，不触发重排），
 *   滚轮按帧合并（见视图），大图**按需**取（先给网格小图秒显，再换成屏幕档）。
 *
 * ## 与网格的关系
 *
 * 网格（`photo-grid`）拥有缩略图队列；看图**不抢**它的小图，而是自己再要一份
 * 「屏幕档」（长边 1920，`ThumbSize = "screen"`），并且**渐进显示**：
 * 网格里已有小图 → 立刻显示 → 大图到了再换上去（顺带把缩放换算好，画面不跳）。
 *
 * ## 视口状态归谁（`AGENTS.md` §6.1 的第一条红线，2026-09-18 定）
 *
 * **缩放 / 平移 / 适配 / 看得到哪一块，只有这一个地方说了算** ——
 * 就是这里的 `ViewerState`（`zoom` / `pan` / `fit` / `viewport` / `natural`）：
 *
 * * 视图层（`Viewer.tsx`）只把 `state()` 翻译成一条 `transform`，不做任何坐标数学；
 * * 右栏的**预览与视野框**（`features/browse/ViewerReadout.tsx`）读的也是同一个 state，
 *   并且调同一个 `visibleRect()` —— 不许自己再推一遍（推两遍就一定会错开）；
 * * 交互（滚轮、拖动、双击、快捷键）只表达**意图**（我要放大到 200%、我要适配窗口），
 *   换算全部落在这里的纯函数里（它们都有单测）。
 *
 * 将来接 Rust 原生视口（M2-W3 的 wgpu 直绘）时：**状态搬去 Rust 独有**
 * （`AGENTS.md` §6.1 的接口纪律），前端这份退化成一份镜像；
 * 那时这个 store 的函数就是「前端镜像」的更新器，`visibleRect()` 这类纯函数
 * 会因为「同一变换、两处使用」而变得更要紧 —— 所以它们**现在就是纯的、有测的**。
 *
 * ## 前端不碰像素（同一节的红线）
 *
 * 这个 store 与它的视图**只处理尺寸、位置、倍率**（数字），不碰像素：
 *
 * * 取图走**统一取图口**（`ViewerStoreDeps.loadScreen` / `loadThumb`，由调用方注入）；
 * * 拿到的字节只做一件事：`URL.createObjectURL` 交给 `<img>` —— 不解码、不读像素、不管色彩空间；
 * * 直方图/色彩/缩放插值这类的统计与处理全在 Rust 侧（`raybend::display::histogram`）。
 *
 * 这条有机器守着：`scripts/check-architecture.mjs` 的**规则 5「前端不碰像素」**
 * （`pnpm lint:arch`）—— `src/` 下出现 `getContext(` / `getImageData` / `colorSpace`
 * 这类 API 会直接报错（注释里提到不算）。
 */

import { createSignal } from "solid-js";

/** 看图里的一张（id 与网格一致：用路径） */
export interface ViewerPhoto {
  id: string;
  path: string;
  fileName: string;
  /**
   * 元数据里的**已按方向换算**的宽高（可选）。
   *
   * 有它就能在 `onLoad` 之前把 `natural` 定下来 —— 对 RAW 尤其要紧：
   * 尺寸未知时 `clampPan` 会把拖动锁死（见那个函数的说明）。
   */
  natural?: { width: number; height: number };
  /**
   * 这张照片当前的标记（**壳层用**）。
   *
   * 看图件自己**不读它**（它只管显示照片），但看图态的底部状态栏要「左文件名 + 锁、
   * 右标记」（`BROWSE.md` §5.8）—— 那些字段调用方本来就有（`AssetItem` 里就在），
   * 顺手带进来，省掉一次「为了显示四个数再问一次后端」的往返。
   */
  marks?: {
    rating: number;
    colorLabel: string | null;
    likeState: string | null;
    lockLevel: number;
  };
  /** 旗标（内存态，不在 `AssetItem` 里，所以由调用方从 store 取）。 */
  flag?: "pick" | "reject" | null;
}

export interface ViewportSize {
  width: number;
  height: number;
}

export interface NaturalSize {
  width: number;
  height: number;
}

export interface ViewerStoreDeps {
  /** 取**屏幕档**大图（长边 1920）。返回 `null` 表示取不到（浏览器里就是这样） */
  loadScreen: (path: string) => Promise<Uint8Array | null>;
  /** 取网格小图（可能已经在网格的缓存里，用来秒显）。可省略 */
  loadThumb?: (path: string) => Promise<Uint8Array | null>;
  makeUrl?: (bytes: Uint8Array) => string;
  /** 缓存上限（张）—— 看图一次只显示一张，留前后几张足够来回翻 */
  cacheLimit?: number;
  revokeUrl?: (url: string) => void;
}

export interface ViewerState {
  active: boolean;
  photos: readonly ViewerPhoto[];
  index: number;
  /** 相对**原图像素**的倍率：1 = 100% */
  zoom: number;
  pan: { x: number; y: number };
  /** 是否处于「适配窗口」状态（用户一滚轮就退出） */
  fit: boolean;
  viewport: ViewportSize;
  natural: NaturalSize;
}

export const EMPTY_VIEWER: ViewerState = {
  active: false,
  photos: [],
  index: 0,
  zoom: 1,
  pan: { x: 0, y: 0 },
  fit: true,
  viewport: { width: 0, height: 0 },
  natural: { width: 0, height: 0 },
};

/** 缩放上下限：下限允许缩到很小（看全景），上限 8× 足够看像素（再大只是糊） */
export const MIN_ZOOM = 0.05;
export const MAX_ZOOM = 8;

export function clampZoom(zoom: number): number {
  if (!Number.isFinite(zoom) || zoom <= 0) return 1;
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom));
}

/** 适配窗口的倍率（contain：整张图都看得见）。任一尺寸未知时给 1，免得算出 NaN */
export function computeFitScale(
  viewport: ViewportSize,
  natural: NaturalSize,
): number {
  if (
    viewport.width <= 0 ||
    viewport.height <= 0 ||
    natural.width <= 0 ||
    natural.height <= 0
  ) {
    return 1;
  }
  return Math.min(
    viewport.width / natural.width,
    viewport.height / natural.height,
  );
}

/**
 * 以某点为锚缩放时，平移量该怎么变 —— **锚点下的内容不动**。
 *
 * 约定：图像**未变换时居中在视口里**（视图用 flex 居中），所以一切都可以相对视口中心算。
 *
 * ```text
 * 屏幕坐标 = 视口中心 + pan + (图像坐标 − 图像中心) × zoom
 * 要让 anchor 处的图像点不动 → 解出新的 pan
 * ```
 *
 * 纯函数，好测：滚轮缩放、双击、按钮缩放全都走它。
 */
export function zoomPanAt(args: {
  pan: { x: number; y: number };
  zoom: number;
  nextZoom: number;
  /** 锚点（视口坐标）。省略 = 视口中心 */
  anchor?: { x: number; y: number };
  viewport: ViewportSize;
}): { x: number; y: number } {
  const { pan, zoom, nextZoom, viewport } = args;
  if (zoom <= 0 || nextZoom <= 0) return pan;
  const anchor = args.anchor ?? {
    x: viewport.width / 2,
    y: viewport.height / 2,
  };
  const center = { x: viewport.width / 2, y: viewport.height / 2 };
  const ratio = nextZoom / zoom;
  return {
    x: anchor.x - center.x - (anchor.x - center.x - pan.x) * ratio,
    y: anchor.y - center.y - (anchor.y - center.y - pan.y) * ratio,
  };
}

/**
 * 平移量夹取：图比视口大时不许拖出边界（边永远在视口外）；
 * 图比视口小时**锁在中间**（不给拖，免得用户以为图丢了）。
 *
 * ⚠️ **原图尺寸未知时不夹取**（2026-09-17 修）：`natural` 是 0 时，
 * 上面两条规则算出来的上限都是 0 —— 于是**任何缩放下都拖不动**。
 * 这正是「RAW 双击点开后鼠标拖不动」的机制：RAW 的尺寸以前读不到（EXIF 读不了
 * RW2 那种魔数，见 `crates/raybend/src/media/tiff.rs`），`natural` 就一直是 0。
 * 现在两头都补了：① 元数据能读到尺寸了；② 万一仍读不到，也**不许把拖动锁死** ——
 * 「不知道边界」不等于「不许移动」。
 */
export function clampPan(args: {
  pan: { x: number; y: number };
  zoom: number;
  viewport: ViewportSize;
  natural: NaturalSize;
}): { x: number; y: number } {
  const { pan, zoom, viewport, natural } = args;
  if (natural.width <= 0 || natural.height <= 0) {
    // 尺寸未知：不猜边界，原样放行（拖动至少是可用的）
    return { x: pan.x, y: pan.y };
  }
  const limit = (scaled: number, available: number): number =>
    Math.max(0, (scaled - available) / 2);
  const maxX = limit(natural.width * zoom, viewport.width);
  const maxY = limit(natural.height * zoom, viewport.height);
  return {
    x: Math.min(maxX, Math.max(-maxX, pan.x)),
    y: Math.min(maxY, Math.max(-maxY, pan.y)),
  };
}

export interface ViewerStore {
  state: () => ViewerState;
  /** 当前这张（`photos[index]`，没有就是 `null`） */
  current: () => ViewerPhoto | null;
  /** 视图用：当前该显示哪张图（可能是小图，也可能是大图） */
  imageUrl: () => string | null;
  imageStatus: () => "idle" | "loading" | "ready" | "error";
  /** 「正在显示大图」—— 视图据此显示一个极轻的指示（可选） */
  sharp: () => boolean;
  show: (photos: readonly ViewerPhoto[], index: number) => void;
  close: () => void;
  next: () => void;
  prev: () => void;
  /**
   * 跳到列表里的**任意一张**（胶片带点击、对比态定位都走它）。
   *
   * 与 `next` / `prev` 共用同一条路径：**重置成「适配窗口」**并只换图不换列表 ——
   * 所以从胶片带点过去不会继承上一张的缩放，也不会把列表换掉。
   */
  goTo: (index: number) => void;
  setViewport: (size: ViewportSize) => void;
  setNatural: (size: NaturalSize) => void;
  zoomBy: (factor: number, anchor?: { x: number; y: number }) => void;
  zoomTo: (zoom: number, anchor?: { x: number; y: number }) => void;
  panBy: (dx: number, dy: number) => void;
  toggleFit: () => void;
}

export function createViewerStore(deps: ViewerStoreDeps): ViewerStore {
  const [state, setState] = createSignal<ViewerState>(EMPTY_VIEWER);
  const [imageUrl, setImageUrl] = createSignal<string | null>(null);
  const [imageStatus, setImageStatus] =
    createSignal<"idle" | "loading" | "ready" | "error">("idle");
  const [sharp, setSharp] = createSignal(false);

  const makeUrl =
    deps.makeUrl ??
    ((bytes: Uint8Array): string => {
      // 复制进一块**自己的** ArrayBuffer：`Uint8Array` 可能是 SharedArrayBuffer 视图，
      // 直接进 Blob 在某些运行时会被拒（与 `photo-grid/thumbnails.ts` 同一做法）
      const buffer = new ArrayBuffer(bytes.byteLength);
      new Uint8Array(buffer).set(bytes);
      return URL.createObjectURL(new Blob([buffer], { type: "image/jpeg" }));
    });
  const revokeUrl = deps.revokeUrl ?? ((url: string): void => URL.revokeObjectURL(url));

  /** 换图时的作废令牌：迟到的结果直接丢掉（换得快时尤其重要） */
  let generation = 0;
  /** 已生成的 URL：换图/关闭时回收上一个 */
  let currentUrl: string | null = null;

  function replaceUrl(url: string | null): void {
    const previous = currentUrl;
    currentUrl = url;
    setImageUrl(url);
    if (previous !== null && previous !== url) revokeUrl(previous);
  }

  const photos = (): readonly ViewerPhoto[] => state().photos;
  const index = (): number => state().index;
  const current = (): ViewerPhoto | null => photos()[index()] ?? null;

  /** 处在适配状态时，任何尺寸变化都要重新算适配倍率 */
  function refit(next: ViewerState): Partial<ViewerState> {
    if (!next.fit) return {};
    return {
      zoom: clampZoom(computeFitScale(next.viewport, next.natural)),
      pan: { x: 0, y: 0 },
    };
  }

  /** 取图：小图先上、大图再换（渐进），换图期间旧图留着不闪空 */
  async function loadFor(photo: ViewerPhoto, ticket: number): Promise<void> {
    setImageStatus("loading");
    setSharp(false);

    if (deps.loadThumb !== undefined) {
      try {
        const bytes = await deps.loadThumb(photo.path);
        if (ticket !== generation) return;
        if (bytes !== null) {
          replaceUrl(makeUrl(bytes));
          setImageStatus("ready");
        }
      } catch {
        // 小图取不到不是问题：接着试大图
      }
    }

    try {
      const bytes = await deps.loadScreen(photo.path);
      if (ticket !== generation) {
        if (bytes !== null) URL.revokeObjectURL(makeUrl(bytes)); // 迟到的大图直接丢掉
        return;
      }
      if (bytes === null) {
        if (imageUrl() === null) setImageStatus("error");
        return;
      }
      replaceUrl(makeUrl(bytes));
      setSharp(true);
      setImageStatus("ready");
    } catch {
      if (ticket === generation) {
        if (imageUrl() === null) setImageStatus("error");
      }
    }
  }

  function show(nextPhotos: readonly ViewerPhoto[], nextIndex: number): void {
    if (nextPhotos.length === 0) return;
    const clamped = Math.min(nextPhotos.length - 1, Math.max(0, nextIndex));
    generation += 1;
    const ticket = generation;
    const photo = nextPhotos[clamped]!;
    setState((prev) => ({
      ...prev,
      active: true,
      photos: nextPhotos,
      index: clamped,
      fit: true,
      /*
       * 尺寸的来路，按可信度排序：
       *   1. 这张照片**元数据**里的宽高（有它就一步到位，RAW 靠它才拖得动）；
       *   2. 旧图尺寸当估计（原来的行为，等 onLoad 回填真实尺寸再修正，避免闪一下大白块）。
       */
      ...refit({
        ...prev,
        natural: photo.natural ?? prev.natural,
        photos: nextPhotos,
        index: clamped,
      }),
    }));
    void loadFor(photo, ticket);
  }

  function close(): void {
    generation += 1;
    replaceUrl(null);
    setSharp(false);
    setImageStatus("idle");
    setState({ ...EMPTY_VIEWER, viewport: state().viewport });
  }

  function goTo(nextIndex: number): void {
    const list = photos();
    if (list.length === 0) return;
    // 到头就停住（不循环）：循环会让人以为还有更多照片
    const clamped = Math.min(list.length - 1, Math.max(0, nextIndex));
    if (clamped === index()) return;
    generation += 1;
    const ticket = generation;
    setState((prev) => {
      const photo = list[clamped];
      return {
        ...prev,
        index: clamped,
        fit: true,
        ...refit({ ...prev, natural: photo?.natural ?? prev.natural, index: clamped }),
      };
    });
    void loadFor(list[clamped]!, ticket);
  }

  function setViewport(size: ViewportSize): void {
    setState((prev) => {
      if (
        prev.viewport.width === size.width &&
        prev.viewport.height === size.height
      ) {
        return prev;
      }
      const next = { ...prev, viewport: size };
      return { ...next, ...refit(next) };
    });
  }

  function setNatural(size: NaturalSize): void {
    setState((prev) => {
      if (prev.natural.width === size.width && prev.natural.height === size.height) {
        return prev;
      }
      /*
       * 大图上来时原图尺寸会从小图（384）变成 1920 —— 此时**不能**让画面跳：
       *   * 适配状态：重算适配 ✓
       *   * 用户自己缩放过的：按比例换算 zoom，让**屏幕上的大小不变** ✓
       */
      const ratio =
        prev.natural.width > 0 ? size.width / prev.natural.width : 1;
      const next = { ...prev, natural: size };
      if (prev.fit) return { ...next, ...refit(next) };
      /*
       * 屏幕上的大小 = `natural × zoom`。原图尺寸变成 `natural × ratio` 之后，
       * 要让它不变，`zoom` 就得**除以** ratio —— 乘的话画面会突然放大（测试抓到过）。
       */
      const zoom = clampZoom(prev.zoom / ratio);
      return {
        ...next,
        zoom,
        pan: clampPan({
          pan: prev.pan,
          zoom,
          viewport: next.viewport,
          natural: size,
        }),
      };
    });
  }

  function applyZoom(nextZoom: number, anchor?: { x: number; y: number }): void {
    setState((prev) => {
      const zoom = clampZoom(nextZoom);
      if (zoom === prev.zoom && !prev.fit) return prev;
      return {
        ...prev,
        zoom,
        fit: false,
        pan: clampPan({
          pan: zoomPanAt({
            pan: prev.pan,
            zoom: prev.zoom,
            nextZoom: zoom,
            viewport: prev.viewport,
            ...(anchor === undefined ? {} : { anchor }),
          }),
          zoom,
          viewport: prev.viewport,
          natural: prev.natural,
        }),
      };
    });
  }

  function panBy(dx: number, dy: number): void {
    setState((prev) => ({
      ...prev,
      pan: clampPan({
        pan: { x: prev.pan.x + dx, y: prev.pan.y + dy },
        zoom: prev.zoom,
        viewport: prev.viewport,
        natural: prev.natural,
      }),
    }));
  }

  /** 双击：适配 ↔ 100%（100% 时以视口中心为锚，保证看到的是中心那块） */
  function toggleFit(): void {
    setState((prev) => {
      if (prev.fit) {
        const zoom = clampZoom(1);
        return { ...prev, fit: false, zoom, pan: { x: 0, y: 0 } };
      }
      const next = { ...prev, fit: true };
      return { ...next, ...refit(next) };
    });
  }

  return {
    state,
    current,
    imageUrl,
    imageStatus,
    sharp,
    show,
    close,
    goTo,
    next: () => goTo(index() + 1),
    prev: () => goTo(index() - 1),
    setViewport,
    setNatural,
    zoomBy: (factor, anchor) => applyZoom(state().zoom * factor, anchor),
    zoomTo: (zoom, anchor) => applyZoom(zoom, anchor),
    panBy,
    toggleFit,
  };
}

/**
 * 当前**看得见的那块图像区域**（图像像素坐标）—— 右栏预览上的「视野框」用它。
 *
 * 约定的变换（与视图里那条 `transform` 一字不差）：
 *
 * ```text
 * 屏幕坐标 = 视口中心 + pan + (图像坐标 − 图像中心) × zoom
 * ```
 *
 * 反过来解出「屏幕的四个角对应图像里的哪一块」，再夹进图像范围。
 * 放大到超过整张时框会小于整张（正常）；缩到比适配还小时框就是整张（夹取的结果）。
 *
 * 尺寸不全（没量到视口、还没拿到原图尺寸）时返回 `null` —— 调用方据此不画框，
 * 而不是画一个乱跳的矩形。
 */
export function visibleRect(state: ViewerState): {
  x: number;
  y: number;
  width: number;
  height: number;
} | null {
  const { natural, viewport, zoom, pan } = state;
  if (
    natural.width <= 0 ||
    natural.height <= 0 ||
    viewport.width <= 0 ||
    viewport.height <= 0 ||
    !Number.isFinite(zoom) ||
    zoom <= 0
  ) {
    return null;
  }
  const centerX = viewport.width / 2 + pan.x;
  const centerY = viewport.height / 2 + pan.y;
  const toImageX = (screenX: number): number =>
    (screenX - centerX) / zoom + natural.width / 2;
  const toImageY = (screenY: number): number =>
    (screenY - centerY) / zoom + natural.height / 2;

  const left = Math.min(Math.max(toImageX(0), 0), natural.width);
  const right = Math.min(Math.max(toImageX(viewport.width), 0), natural.width);
  const top = Math.min(Math.max(toImageY(0), 0), natural.height);
  const bottom = Math.min(Math.max(toImageY(viewport.height), 0), natural.height);
  return {
    x: left,
    y: top,
    width: Math.max(0, right - left),
    height: Math.max(0, bottom - top),
  };
}
