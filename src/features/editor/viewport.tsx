/**
 * 编辑中列：**视口**（`design/editor.md` §3.2）。
 *
 * 这一版（M3-W1）它是一个**留好的洞口 + 空态水印**：
 *
 * ```text
 * ┌──────────────────────────────┐
 * │                              │   ← 这块矩形原样上报给 Rust（CSS 像素 + DPR）
 * │     （空态水印 / 待接入提示）    │      W2 起 wgpu 在它后面直绘照片
 * │                              │
 * └──────────────────────────────┘
 * ```
 *
 * 三条纪律：
 *
 * 1. **只上报原始事实**（矩形 / DPR / CSS 视口），换算全在 Rust —— `AGENTS.md` §6.1 红线 2；
 * 2. 上报要**三处都触发 + 尾样本**：`ResizeObserver`、窗口 `resize`、DPR 变化各一路，
 *    合并成每帧一次只发最新值（`lib/editor-viewport.ts` 里有测试）；
 * 3. **空态四态**用 `StateWatermark`（`DESIGN.md` §12.10 的水印口径：不成卡、不加边框）。
 *
 * ⚠️ 水印是**印在洞口里的**：照片出现之后（W2）它就消失，不是浮在照片上的一层。
 */

import { onCleanup, onMount, Show, type JSX } from "solid-js";
import {
  IconAlbumOff,
  IconFolder,
  IconPhoto,
  IconPhotoOff,
} from "@tabler/icons-solidjs";

import { StateWatermark } from "../../components/ui/StateWatermark.tsx";
import { t } from "../../i18n/index.ts";
import type { MessageKey } from "../../i18n/index.ts";
import { createViewportReporter } from "../../lib/editor-viewport.ts";
import { setEditorViewport } from "../../api/editor.ts";
import { isTauriRuntime } from "../../api/tauri-env.ts";
import {
  editorEmptyIcon,
  editorEmptyOffersImport,
  type EditorEmptyKind,
} from "./source.ts";
import { PendingNote } from "./parts.tsx";

export interface EditorViewportProps {
  /** 空态（四态之一）；`null` = 有照片（W2 起该出图） */
  empty: EditorEmptyKind;
  /** 「去导入」按钮（只有「没有库」那一态给） */
  onOpenImport?: () => void;
  /** 上报开关（开发页里可以关掉；默认开） */
  report?: boolean;
  class?: string;
}

/** 空态 → 图标（四态各一枚；与其它工作区用的图标同一套）。 */
const EMPTY_ICON = {
  library: IconAlbumOff,
  folder: IconFolder,
  photo: IconPhotoOff,
  select: IconPhoto,
} as const;

/** 空态 → 文案 key。 */
const EMPTY_TEXT = {
  "no-repository": "editor.empty.noRepository",
  "no-directory": "editor.empty.noDirectory",
  "no-photos": "editor.empty.noPhotos",
  "no-selection": "editor.empty.noSelection",
} as const satisfies Record<Exclude<EditorEmptyKind, null>, MessageKey>;

export function EditorViewport(props: EditorViewportProps): JSX.Element {
  let hole: HTMLDivElement | undefined;

  /**
   * 上报：**只在真运行时开**（浏览器预览里没有 `editor_set_viewport`，
   * 报了也只是把一个 promise 打红，没有意义）。
   */
  onMount(() => {
    if (props.report === false || !isTauriRuntime()) return;
    if (hole === undefined) return;
    const element = hole;

    const reporter = createViewportReporter({
      send: (payload) => {
        // 报错不吞：弹到控制台（终端诊断，不走语言包）——
        // 「一直没报上去」与「报错了」必须分得开
        void setEditorViewport(payload).catch((error: unknown) => {
          console.error("[editor] 视口上报失败", error); // i18n-exempt: 控制台诊断
        });
      },
    });

    const observe = (): void => {
      const rect = element.getBoundingClientRect();
      reporter.observe({
        rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
        // ⚠️ **运行时** DPR：含显示器 DPI + 系统文字缩放 + 页面缩放（§7.9 铁律 3）
        dpr: window.devicePixelRatio,
        viewport: { width: window.innerWidth, height: window.innerHeight },
      });
    };

    /// DPR 变化时重新订阅（媒体查询串里带着旧 DPR，不重订就收不到第二次变化）
    let dprQuery: MediaQueryList | null = null;
    const onDprChange = (): void => {
      observe();
      dprQuery = subscribeDpr(onDprChange, dprQuery);
    };

    observe(); // 首帧就报一次（不等第一次尺寸变化）
    dprQuery = subscribeDpr(onDprChange, null);

    const observer = new ResizeObserver(observe);
    observer.observe(element);
    window.addEventListener("resize", observe);

    onCleanup(() => {
      // 卸载前把挂起的那一帧发出去（尾样本不能丢），再断开
      reporter.flush();
      reporter.dispose();
      observer.disconnect();
      window.removeEventListener("resize", observe);
      dprQuery?.removeEventListener("change", onDprChange);
    });
  });

  return (
    <div
      ref={hole}
      data-editor-viewport
      data-viewport-empty={props.empty ?? "photo"}
      class={[
        "relative flex min-h-0 min-w-0 flex-1 items-center justify-center overflow-hidden bg-surface-bar",
        props.class ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
    >
      <Show when={props.empty} fallback={<ViewportPendingNotice />}>
        {(kind) => {
          const Icon = EMPTY_ICON[editorEmptyIcon(kind())];
          return (
            <StateWatermark
              icon={<Icon size={64} stroke-width={1} />}
              text={t(EMPTY_TEXT[kind()])}
              action={
                editorEmptyOffersImport(kind()) && props.onOpenImport !== undefined
                  ? { label: t("editor.empty.goImport"), run: () => props.onOpenImport?.() }
                  : undefined
              }
            />
          );
        }}
      </Show>
    </div>
  );
}

/**
 * 有照片、但 W1 还画不出来时的提示。
 *
 * 这是**这一波唯一允许出现的“待接入”正文**：照片确实在手上（胶片带能看见），
 * 只是 GPU 视口在 W2 落地 —— 与其留一块空白让人以为坏了，不如把话说清楚。
 */
function ViewportPendingNotice(): JSX.Element {
  return (
    <div class="flex max-w-80 flex-col items-center gap-2 text-center">
      <PendingNote text={t("editor.viewport.pending")} class="justify-center" />
    </div>
  );
}

/**
 * 订阅 DPR 变化。
 *
 * 媒体查询串里带着当前 DPR，所以每次变化后都要**重新订阅**（否则第二次变化收不到）——
 * 返回新的查询对象交给调用方保存。
 */
function subscribeDpr(
  onChange: () => void,
  previous: MediaQueryList | null,
): MediaQueryList | null {
  previous?.removeEventListener("change", onChange);
  if (typeof window.matchMedia !== "function") return null;
  const dpr = window.devicePixelRatio;
  if (!Number.isFinite(dpr) || dpr <= 0) return null;
  const query = window.matchMedia(`(resolution: ${dpr}dppx)`);
  query.addEventListener("change", onChange);
  return query;
}
