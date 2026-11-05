/**
 * **画布上的控制点标记**（`design/editor.md` 2026-10-31，画稿 `xjwv9`）。
 *
 * ```text
 *    名字
 *  ─────●        ← 引线：水平 70px（左半向右、右半向左），白线 + 细黑勾边
 *  ◉             ← 圆片：20 外框 / 1px 黑圈 / 2.5 白圈 / 12 内芯（同心）
 * ```
 *
 * # 口径（画稿 + 崔总 2026-11-01 真机反馈）
 *
 * 1. **圆片三环同心**：外黑圈 1px、白圈 2.5px、内芯 12px —— 不是「白底 + 描边 + 一个偏移的色点」；
 * 2. **引线是水平的**：点在左半 → 向右；右半 → 向左（`specs/editor-area.md` §5.3）。
 *    一条线由「细黑勾边 + 白线」两层叠出来，亮背景上也读得清；
 * 3. **名字在线上**（引线左端 +6、上方 18），白字 + 细黑勾边，不再放在 handle 旁边；
 * 4. **位置走 Rust 的变换矩阵**（`ViewportOverlay`），尺寸是**屏幕像素固定**的 ——
 *    所以每个标记自己的容器里再按 `overlayScale` / `overlayRotation` 反向抵消；
 * 5. **手势不穿透**：圆片 / handle 的 `pointerdown` 用**原生监听**（`on:` 前缀）
 *    并在本元素 `stopPropagation` —— Solid 的 `onPointerDown` 是**委托**到 document 的，
 *    那时洞口的平移早就吃掉了这一下（2026-11-01 的「拖 handle 变成拖图」就是它）；
 * 6. **没有范围圆**：`tolerance` 是**向量容差**（差多少还算相近 + 渐变宽度），不是空间半径；
 * 7. 眼睛关掉时**整组标记不画、抓取同时关掉**（看不见的东西不能点）。
 */

import { Index, Show, createEffect, createSignal, onCleanup, onMount, type JSX } from "solid-js";

import { t } from "../../i18n/index.ts";
import { maskLineRange } from "../../lib/mask-debug.ts";
import { POINT_TOLERANCE } from "./params.ts";
import { clearFlowHint, showFlowHint } from "../../lib/flow-hint.ts";
import { imageDeltaFromCss } from "../../lib/overlay-math.ts";
import { overlayLive } from "../../lib/overlay-live.ts";
import { trackPointerDrag } from "../../lib/pointer-drag.ts";
import type { EditorRenderState } from "../../api/types.ts";
import type { Area } from "../../api/editor.ts";
import type { EditorStore } from "./store.ts";
import { POINT_COLORS } from "./area-panel.tsx";

/** 圆片外框直径（屏幕像素）。 */
const DISC = 20;
/** 最外圈：细黑线宽度。 */
const RING_BLACK = 1;
/** 白圈宽度（画稿 2.5）。 */
const RING_WHITE = 2.5;
/** 内芯直径：由外框与两圈推出（= 13 → 白圈正好 2.5px，黑圈 1px）。 */
const CORE = DISC - 2 * (RING_BLACK + RING_WHITE);
/**
 * 引线（= 容差白线）的长度：**它本身就是容差的体现**（崔总 2026-11-01）。
 *
 * 端点来自 `lib/mask-debug.ts`（开发期可调，默认 32 / 320）；容差 100..1000 线性映射。
 * 长度是**屏幕像素**，与缩放无关。
 */
/** handle 直径（屏幕像素）。 */
const HANDLE = 9;
/** 名字基线（相对引线）—— 画稿里名字在线上方 18px。 */
const LABEL_BASELINE = -7;

type PointArea = Extract<Area, { type: "point" }>;

/**
 * 提示区的归属键（`lib/flow-hint.ts`）：指到圆片时在 flowbar 上说一句
 * 「按住 Ctrl 键可拖动」——这是只有做过才知道的操作，用完即走，不占任何常驻文案。
 */
const HINT_OWNER = "editor.area.point";

export interface AreaMarkersProps {
  store: EditorStore;
  renderState: () => EditorRenderState | null;
  /** 松手（拖圆片改位置 / 拖 handle 改容差）—— 工作区据此 `confirmEdit()`。 */
  onCommit: () => void;
  /**
   * 采样位变了（`Ctrl` 拖圆片松手）：**重新取样**种子，随后由工作区落库。
   *
   * ❗这是本组件的职责边界：标记只负责「位置变了」，取样要走 Rust（解码 1024 图）；
   * 没有这个回调时退回普通 `onCommit`（只存位置，种子不动）。
   */
  onResample?: (id: number) => void;
  /** 照片不可编辑（空态 / 还没就绪 / 二级锁）—— 标记只读 */
  enabled: boolean;
  /**
   * 当前基准下控制点**不参与计算**（SOOC / 位图）—— 标记半透明。
   *
   * 只降透明度，**不禁用操作**（崔总 2026-11-01：拖也不计算，但没必要拦住手）。
   */
  inactive?: boolean;
}

export function AreaMarkers(props: AreaMarkersProps): JSX.Element {
  const [toleranceDraft, setToleranceDraft] = createSignal<{ id: number; value: number } | null>(null);
  /*
   * `Ctrl` 是否按着 —— 只为把圆片的光标换成「取样位移动」那一枚。
   * 监听挂在 window：指针在画布上移动时焦点可能在视口 / 按钮上，不给单个元素挂 keydown。
   * 失去焦点也清掉（否则切回来会一直挂着一枚错误的光标）。
   */
  const [ctrlDown, setCtrlDown] = createSignal(false);
  onMount(() => {
    const down = (event: KeyboardEvent): void => {
      if (event.key === "Control") setCtrlDown(true);
    };
    const up = (event: KeyboardEvent): void => {
      if (event.key === "Control") setCtrlDown(false);
    };
    const reset = (): void => {
      setCtrlDown(false);
    };
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    window.addEventListener("blur", reset);
    onCleanup(() => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
      window.removeEventListener("blur", reset);
    });
  });

  // 标记整组消失（换照片 / 关掉眼睛）时，别把提示留在 flowbar 上
  onCleanup(() => clearFlowHint(HINT_OWNER));
  // `Show` 只卸载内部 DOM，不会卸载本组件，所以**关眼睛**还要显式清一次；
  // 照片变为不可编辑（换图 / 二级锁）同理。
  createEffect(() => {
    if (!props.store.showPoints() || !props.enabled) clearFlowHint(HINT_OWNER);
  });

  const image = () => props.renderState()?.image ?? null;
  /**
   * 变换 / 比例 / 旋转**优先读实时那一份**（手势期间每帧更新），
   * 没有就退回 250ms 轮询的快照 —— 这就是「缩放与拖动时标记跟着动」。
   */
  const live = () => overlayLive();
  const scale = () => live()?.scale ?? props.renderState()?.overlayScale ?? 1;
  const rotation = () => live()?.rotation ?? props.renderState()?.overlayRotation ?? 0;
  /** 图像像素 → 洞口内 CSS 像素（含平移；算「贴没贴到洞顶」要用它）。 */
  const matrix = () => live()?.transform ?? props.renderState()?.overlayTransform ?? null;

  /** 画布上要画的那些层（只画控制点）。 */
  const points = (): PointArea[] =>
    props.store
      .layerRows()
      .filter((row): row is Extract<typeof row, { kind: "area" }> => row.kind === "area")
      .map((row) => row.area)
      .filter((area): area is PointArea => area.type === "point");

  /** 标记的「屏幕像素固定」容器：抵消父层的缩放与旋转。 */
  const markerTransform = (): string => {
    const factor = scale();
    const inverse = factor > 1e-6 ? 1 / factor : 1;
    return `rotate(${-rotation()}deg) scale(${inverse})`;
  };

  const toleranceOf = (area: PointArea): number => {
    const draft = toleranceDraft();
    if (draft !== null && draft.id === area.id) return draft.value;
    return area.tolerance;
  };

  /**
   * **`Ctrl`（macOS 上是 `⌘`）+ 左键拖动圆片 = 移动采样位**（崔总 2026-10-31）。
   *
   * 为什么不给「直接拖」：圆片与视口平移是同一个手势区，直接拖会让「想平移却拖走了点」。
   * 加了修饰键，普通拖动**不拦事件**（继续交给洞口去平移）。
   *
   * 位移按「从按下点起算的总位移」算，**不是**逐帧累加 —— 累加会和视口平移叠在一起
   * （真机上表现为「点比鼠标跑得快一倍」，2026-11-01 修）。
   * 移动的只有 `x` / `y`：**种子不重取**（`specs/editor-area.md` §3.3 的「冻结种子」）。
   */
  const dragPoint = (event: PointerEvent, area: PointArea): void => {
    if (event.button !== 0 || !props.enabled) return;
    props.store.selectArea(area.id);
    if (!(event.ctrlKey || event.metaKey)) return; // 普通拖动：交给视口平移
    // 本元素自己的手势，**不许**再被洞口当成平移（原生监听 + 就地停播）
    event.stopPropagation();
    event.preventDefault();
    const size = image();
    if (size === null) return;
    const originX = event.clientX;
    const originY = event.clientY;
    const startX = area.x;
    const startY = area.y;
    trackPointerDrag(event, {
      capture: true,
      threshold: 2,
      cancelOutsideWindow: true,
      start: () => undefined,
      move: (point) => {
        const shift = imageDeltaFromCss(
          scale(),
          rotation(),
          point.x - originX,
          point.y - originY,
        );
        props.store.setAreaPoint(area.id, {
          x: startX + shift.x / size.width,
          y: startY + shift.y / size.height,
        });
      },
      end: (point, cancelled) => {
        if (cancelled) {
          // `cancelOutsideWindow` = 放弃这次手势：不能把拖了一半的位置留在 store 里
          props.store.setAreaPoint(area.id, { x: startX, y: startY });
          return;
        }
        // 极小的移动 = 只是想选中这一层，不要占一步撤销
        if (Math.abs(point.x - originX) < 2 && Math.abs(point.y - originY) < 2) return;
        /*
         * 采样位变了 ⇒ **重新取种子**（崔总 2026-11-01）。
         *
         * 以前这里只 `onCommit()`，种子永远停在第一次落点的那组数 —— 把点拖到别处，
         * 画面却一点不变，「移动采样点」等于白拖。取样需要 Rust 解码，所以只在本手势
         * 结束时做一次；工作区取完种子会自己 `confirmEdit()`（位置 + 种子同一次撤销）。
         */
        if (props.onResample !== undefined) props.onResample(area.id);
        else props.onCommit();
      },
    });
  };

  /**
   * 拖引线末端的 handle = 改**容差**（横向，往外变大）。
   *
   * 容差同时缩放「100% 应用区」与「渐变区」（`develop/areas.rs::POINT_WEIGHT_CONFIG`），
   * 所以拖它时画面上的变化是「命中范围整体变大 / 变小」。
   */
  const dragTolerance = (event: PointerEvent, area: PointArea): void => {
    if (event.button !== 0 || !props.enabled) return;
    event.stopPropagation();
    event.preventDefault();
    const originX = event.clientX;
    const start = area.tolerance;
    const side = area.x >= 0.5 ? -1 : 1;
    /*
     * **手柄 1:1 跟手**：白线长度与容差用的是同一组端点 ——
     * 拖 `lineMax − lineMin` 像素正好走完 100..1000，所以线头永远在指针下面。
     */
    const line = maskLineRange();
    const perPixel = (POINT_TOLERANCE.max - POINT_TOLERANCE.min)
      / Math.max(1, line.max - line.min);
    trackPointerDrag(event, {
      capture: true,
      threshold: 2,
      cancelOutsideWindow: true,
      start: () => setToleranceDraft({ id: area.id, value: start }),
      move: (point) => {
        const delta = (point.x - originX) * side * perPixel;
        setToleranceDraft({
          id: area.id,
          value: Math.min(
            POINT_TOLERANCE.max,
            Math.max(POINT_TOLERANCE.min, Math.round(start + delta)),
          ),
        });
      },
      end: (_point, cancelled) => {
        const draft = toleranceDraft();
        setToleranceDraft(null);
        if (cancelled || draft === null) return;
        if (draft.value !== start) {
          props.store.setAreaPoint(area.id, { tolerance: draft.value });
          props.onCommit();
        }
      },
    });
  };

  return (
    <Show when={props.store.showPoints() && image() !== null}>
      <Index each={points()}>
        {(areaAccessor) => {
          const point = () => areaAccessor() as PointArea;
          if (areaAccessor().type !== "point") return null;
          const size = () => image()!;
          const px = () => point().x * size().width;
          const py = () => point().y * size().height;
          // 点在左半 ⇒ 引线向右；右半 ⇒ 向左（规格 §5.3：按图像中线翻边，不量文字宽度）
          const side = () => point().x >= 0.5 ? -1 : 1;
          /**
           * 引线长度 = 容差的体现：`100..1000` 线性映射到 `lineMin..lineMax`（屏幕像素）。
           * 线**超出图像边界也没关系**（洞口本身可缩放平移），不做夹取。
           */
          const lineLength = (): number => {
            const range = maskLineRange();
            const position = Math.min(
              1,
              Math.max(
                0,
                (toleranceOf(point()) - POINT_TOLERANCE.min)
                  / (POINT_TOLERANCE.max - POINT_TOLERANCE.min),
              ),
            );
            return range.min + (range.max - range.min) * position;
          };
          /** 引线：从圆片边缘出发。 */
          const startX = () => side() * (DISC / 2);
          const endX = () => side() * (DISC / 2 + lineLength());
          /**
           * 名字：**永远贴在圆片这一端**（采样点端），文字始终从左往右写 ——
           * 所以拖 handle 改容差时名字不动，线只在另一头伸缩。
           */
          const labelX = () => side() * (DISC / 2 + 6);
          const labelAnchor = () => side() > 0 ? "start" : "end";
          /** 名字上方没有空间（贴着洞顶）时翻到线下方 —— 只在**布局**上翻，不随缩放抖。 */
          const labelBelow = () => {
            const transform = matrix();
            if (transform === null) return py() * scale() < 26;
            // CSS matrix(a,b,c,d,e,f)：y′ = b·x + d·y + f。
            // 只看 `py × scale` 会漏掉**平移**，把已经移出洞口的点误判成贴顶。
            const cssY = transform[1] * px() + transform[3] * py() + transform[5];
            return cssY < 26;
          };
          const labelY = () => (labelBelow() ? 16 : LABEL_BASELINE);
          const color = () => POINT_COLORS[point().color % POINT_COLORS.length];
          const active = () => props.store.activeAreaId() === point().id;
          const name = () => point().name !== "" ? point().name : t("editor.area.point");
          return (
            <div
              data-area-marker={point().id}
              class="pointer-events-none absolute"
              style={{
                left: `${px()}px`,
                top: `${py()}px`,
                "transform-origin": "0 0",
                transform: markerTransform(),
                // 关掉的层 / 当前基准下不参与计算的层：降透，但仍然看得见、点得到
                opacity: point().enabled && props.inactive !== true ? "1" : "0.45",
              }}
            >
              {/*
                引线 + 名字：一个零尺寸的 SVG，坐标原点就在圆片中心。
                `overflow-visible` 让它不占布局、也不被自己的视框裁掉。
              */}
              <svg
                class="pointer-events-none absolute overflow-visible"
                style={{ left: "0px", top: "0px" }}
                width="1"
                height="1"
                aria-hidden="true"
              >
                {/* 细黑勾边：白线放在亮背景上也不会消失（崔总 2026-11-01） */}
                <line
                  x1={startX()}
                  y1="0"
                  x2={endX()}
                  y2="0"
                  stroke="var(--overlay-halo)"
                  stroke-width="2.6"
                  stroke-linecap="round"
                />
                <line
                  x1={startX()}
                  y1="0"
                  x2={endX()}
                  y2="0"
                  stroke="var(--overlay-line)"
                  stroke-width="1.2"
                  stroke-linecap="round"
                />
                <text
                  x={labelX()}
                  y={labelY()}
                  text-anchor={labelAnchor()}
                  fill="var(--overlay-line)"
                  stroke="var(--overlay-halo)"
                  stroke-width="2.4"
                  stroke-linejoin="round"
                  style={{ "paint-order": "stroke", "font-size": "11px", "font-weight": "500" }}
                >
                  {name()}
                </text>
              </svg>
              {/* 拖 handle 时的读数（带数字的小条，规格 §5.4） */}
              <Show when={toleranceDraft()?.id === point().id}>
                <div
                  class="absolute whitespace-nowrap rounded-ui bg-surface-layer/85 px-1.5 py-0.5 text-fs-0 tabular-nums text-fg-1"
                  style={{
                    left: `${labelX()}px`,
                    top: `${labelBelow() ? 20 : 8}px`,
                    transform: labelAnchor() === "end" ? "translateX(-100%)" : "none",
                  }}
                >
                  {t("editor.area.tolerance", {
                    value: Math.round(toleranceOf(point())),
                  })}
                </div>
              </Show>
              {/* handle：拖它改**容差**（往外 = 变大；不是空间半径） */}
              <div
                data-area-handle={point().id}
                class="absolute rounded-full"
                style={{
                  left: `${endX() - HANDLE / 2}px`,
                  top: `${-HANDLE / 2}px`,
                  width: `${HANDLE}px`,
                  height: `${HANDLE}px`,
                  "background-color": "var(--overlay-line)",
                  "box-shadow": "0 0 0 1px var(--overlay-halo)",
                  "pointer-events": props.enabled ? "auto" : "none",
                  cursor: "ew-resize",
                }}
                on:pointerdown={(event) => dragTolerance(event, point())}
                aria-hidden="true"
              />
              {/* 圆片：单击选中；`Ctrl` 拖 = 移动采样位并重新取样 */}
              <div
                data-area-disc={point().id}
                class="absolute rounded-full"
                classList={{ "rb-cursor-sample": ctrlDown() }}
                style={{
                  left: `${-DISC / 2}px`,
                  top: `${-DISC / 2}px`,
                  width: `${DISC}px`,
                  height: `${DISC}px`,
                  "pointer-events": props.enabled ? "auto" : "none",
                  // 按住 Ctrl 时让给 `rb-cursor-sample`（行内样式会盖掉类，所以不写）
                  cursor: ctrlDown() ? undefined : active() ? "grab" : "pointer",
                }}
                title={t("editor.area.moveHint")}
                on:pointerdown={(event) => dragPoint(event, point())}
                // 指到圆片：flowbar 提示区说一句「按住 Ctrl 键可拖动」（离开即撤）
                on:pointerenter={() => {
                  if (props.enabled) showFlowHint(HINT_OWNER, t("editor.area.ctrlDragHint"));
                }}
                on:pointerleave={() => clearFlowHint(HINT_OWNER)}
                aria-label={name()}
              >
                {/* 三环同心：白圈 + 内芯；最外 1px 黑线用 inset 阴影画在 20px 圈**内侧** */}
                <span
                  class="absolute inset-0 rounded-full"
                  style={{
                    "background-color": "var(--overlay-line)",
                    "box-shadow": `inset 0 0 0 ${RING_BLACK}px var(--overlay-halo)`,
                  }}
                />
                <span
                  class="absolute rounded-full"
                  style={{
                    left: `${(DISC - CORE) / 2}px`,
                    top: `${(DISC - CORE) / 2}px`,
                    width: `${CORE}px`,
                    height: `${CORE}px`,
                    "background-color": color(),
                  }}
                />
              </div>
            </div>
          );
        }}
      </Index>
    </Show>
  );
}
