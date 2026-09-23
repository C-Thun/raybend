/**
 * 编辑工作区：三列（`design/editor.md` §2）。
 *
 * ```text
 * ┌────────────┬──────────────────────────────┬──────────────┐
 * │ 左列 280    │ 中列（弹性，surface-bar）      │ 右列 255/267 │
 * │ LUT 面板    │ 视口 + 胶片带 + 照片状态栏      │ 三组页签      │
 * │ （可整列关） │ （档位控制显隐）               │ （档位控制）  │
 * └────────────┴──────────────────────────────┴──────────────┘
 * ```
 *
 * 这是**组装层**（`ARCHITECTURE.md` §2）：把浏览侧的数据（库 / 目录 / 清单 / 选择）
 * 与编辑器自己的界面状态（档位 / 工具 / 参数）接起来，自己不做业务判断。
 *
 * # 三条结构纪律（`AGENTS.md` §11.1 / §11.4）
 *
 * 1. **workspace 下只有纵向分列，没有跨列行** —— 每列各自到底；
 * 2. **当前在编哪张 = browse store 的锚点**，编辑不另造选择模型
 *    （胶片带的点选写回 `browse.select` / `setAnchor`）；
 * 3. 胶片带 / 状态栏 / 三点把手**复用同一份组件**，不重做。
 *
 * # 与瀏览的关系
 *
 * 编辑**没有 tiles 模式**（要看网格回「浏览」）；进编辑时直接用浏览侧选定的库 / 目录 / 照片，
 * 挂载时重读一遍当前目录（`AGENTS.md` §2.13：本地应用以「看到真相」为先）。
 */

import { createEffect, createMemo, createSignal, onCleanup, onMount, Show, type JSX } from "solid-js";

import {
  bindEditorRenderer,
  getEditorRenderState,
  setEditorPhoto,
  unbindEditorRenderer,
  sendEditorViewportIntent,
} from "../../api/editor.ts";
import { isTauriRuntime } from "../../api/tauri-env.ts";
import type { EditorRenderState, EditorViewportIntent } from "../../api/types.ts";
import { getHistogram, getThumbBytes, listRepositories } from "../../api/db.ts";
import type { AssetItem, RepositoryView } from "../../api/types.ts";
import { browseSource } from "../../features/browse/grid-source.ts";
import type { BrowseStore } from "../../features/browse/store.ts";
import {
  EditorPanels,
  EditorViewport,
  EDITOR_ZOOM_STEP,
  LutPanel,
  createEditorStrip,
  editorEmptyKind,
  type EditorPhotoInfo,
  type EditorStore,
} from "../../features/editor/index.ts";
import { registerEditorActions, type EditorActions } from "../../features/editor/actions.ts";
import {
  assetItemExif,
  formatAperture,
  formatDimensions,
  formatFocalLength,
  formatIso,
  formatMegapixels,
  formatShutter,
  formatText,
} from "../../features/exif-strip/index.ts";
import { createThumbQueue } from "../../components/ui/thumb-queue.ts";
import { FilmStrip } from "../../components/ui/viewer/index.ts";
import { photosFromSource, viewingInfoOf } from "../../components/ui/viewer/index.ts";
import { buildFullscreenTarget } from "../../lib/fullscreen-target.ts";
import { registerViewerActions, type ViewerActions } from "../../components/ui/viewer/actions.ts";
import { PhotoStatusBar } from "../../components/ui/tiles/index.ts";
import { browseInfoMode } from "../../components/ui/tile-info.ts";
import {
  browseDisplayByTime,
  browseDisplayTileStep,
  commitBrowseDisplayTileStep,
  setBrowseDisplayTileStep,
} from "../../lib/display-prefs.ts";
import { t } from "../../i18n/index.ts";

export interface EditorWorkspaceProps {
  store: EditorStore;
  /** 浏览侧的 store（库 / 目录 / 清单 / 选择 / 锚点都在它手里） */
  browse: BrowseStore;
  /** browse 自己的胶片带尺寸档位（editor 用**自己那一份**，见 `lib/film-strip-prefs.ts`） */
  filmStripStep: number;
  onFilmStripStepChange: (step: number) => void;
  /** 「去导入」（空态里那颗按钮） */
  onOpenImport: () => void;
  class?: string;
}

export function EditorWorkspace(props: EditorWorkspaceProps): JSX.Element {
  const store = props.browse;
  const [repositories, setRepositories] = createSignal<RepositoryView[]>([]);

  /**
   * 胶片带与右栏「总览」**共用的**缩略图队列（同一份缓存 → 中列里立刻有图）。
   * 换库时清空（不同库的同名相对路径会串图），卸载时也清。
   */
  const thumbs = createThumbQueue({
    load: async (path) => (await getThumbBytes(path, "grid")) ?? null,
  });
  onCleanup(() => thumbs.clear());

  onMount(() => {
    void (async () => {
      try {
        setRepositories(await listRepositories());
      } catch (error) {
        // 读不到库列表不该让工作区崩掉；空态会显示「还没有库」
        console.error("[editor] 读库列表失败", error); // i18n-exempt: 控制台诊断
      }
    })();
  });

  /** 库根目录（拼缩略图绝对路径用）。 */
  const root = createMemo(
    () => repositories().find((repo) => repo.id === store.repositoryId())?.root ?? null,
  );

  /*
   * 数据源：与浏览侧**同一个适配器**（`browseSource`）—— 显示序、分页、选择语义
   * 三者都与网格里看到的完全一致，编辑不另算一份顺序。
   */
  const source = createMemo(() =>
    browseSource({
      store,
      root,
      thumbs,
      tileStep: browseDisplayTileStep,
      setTileStep: setBrowseDisplayTileStep,
      commitTileStep: commitBrowseDisplayTileStep,
      grouped: browseDisplayByTime,
      infoMode: browseInfoMode,
    }),
  );

  const photos = createMemo(() => photosFromSource(source()));

  /**
   * 全屏看图要的清单（flowbar 的全屏按钮 + `viewer.fullscreen` 命令）。
   *
   * 与 browse / import 同一份显示序函数 —— 编辑里看到的邻居就是浏览里看到的邻居。
   */
  const fullscreenTarget = createMemo(() =>
    buildFullscreenTarget(photos(), store.selection().anchor),
  );

  /**
   * 胶片带适配：**读**锚点、**写**回 browse store 的选择（`AGENTS.md` §11.4 红线）。
   *
   * 点一格 = 把选择换成那一张（与网格里点一下同义），锚点随之移动 ——
   * 于是「编辑里换一张、回浏览还是它」自然成立。
   */
  const strip = createEditorStrip({
    photos,
    anchorId: () => store.selection().anchor,
    goTo: (index) => {
      const photo = photos()[index];
      if (photo === undefined) return;
      store.select(Number(photo.id), "replace");
    },
  });

  const current = createMemo(() => strip.current());

  /* ── GPU 视口（M3-W2）：握手 / 轮询 / 换照片 / 动作槽 ─────────────
   *
   * 工作区在这一块里的职责是「接线」，不是像素：
   *
   * | 谁 | 管什么 |
   * | --- | --- |
   * | 前端 | 何时起/停渲染线程、当前是哪张、把状态摊给视口与根节点 |
   * | Rust | 视口变换、解码、纹理、设备丢失与重启 |
   */
  const [rendererBound, setRendererBound] = createSignal(false);

  /** 当前照片的**绝对路径**（锚点那张；没有就是 `null`）。 */
  const currentPath = createMemo(() => current()?.path ?? null);

  /**
   * 状态回写：渲染线程说「画出来了」才把洞口切成透明。
   *
   * 三条都要：
   * * `bound` —— 会话还在（线程走了就得退回 DOM，否则洞口会停在一张旧帧上）；
   * * `ready` —— 设备与 surface 都就绪；
   * * `paintedPath` —— **真的出过图**；没有它就没有「谁在画这块矩形」的保证。
   */
  const applyRenderState = (state: EditorRenderState | null): void => {
    props.store.setRenderState(state);
    props.store.setHoleActive(
      state !== null && state.bound && state.ready && state.paintedPath !== null,
    );
  };

  /** 发一条视口意图（缩放 / 平移 / 适配）——失败只记控制台，不带崩界面。 */
  const sendIntent = (intent: EditorViewportIntent): void => {
    void sendEditorViewportIntent(intent).catch((error: unknown) => {
      console.error("[editor] 视口意图失败", error); // i18n-exempt: 控制台诊断
    });
  };

  /** 起渲染线程（挂载时一次；「重试」也走它 —— Rust 侧对已死的线程会重新起一个）。 */
  const startRenderer = (): void => {
    void bindEditorRenderer()
      .then((state) => {
        applyRenderState(state);
        setRendererBound(true);
      })
      .catch((error: unknown) => {
        // 起不来不是致命的：视口退化成 DOM（水印 + 一句说明），编辑器其余部分照常可用
        console.error("[editor] 渲染线程起不来", error); // i18n-exempt: 控制台诊断
        setRendererBound(false);
      });
  };

  onMount(() => {
    if (!isTauriRuntime()) return;
    startRenderer();
    // 轮询：既是握手（ready / paintedPath），也是**上报通道**（重启次数 / 当前错误）
    const timer = window.setInterval(() => {
      void getEditorRenderState()
        .then((state) => applyRenderState(state))
        .catch(() => {
          // 读状态失败不弹错：下一轮再试（它的失败不能把编辑器搞成错误页）
        });
    }, 250);
    onCleanup(() => {
      window.clearInterval(timer);
      void unbindEditorRenderer().catch(() => {
        // 停不掉也只是线程多活一会儿；窗口关了进程就没了
      });
      applyRenderState(null);
      setRendererBound(false);
    });
  });

  /** 锚点一变就换纹理（渲染线程自己负责解码与两档切换）。 */
  createEffect(() => {
    if (!rendererBound()) return;
    const path = currentPath();
    void setEditorPhoto(path).catch((error: unknown) => {
      console.error("[editor] 换照片失败", error); // i18n-exempt: 控制台诊断
    });
  });

  /** 胶片带里的上一张 / 下一张（看图命令 `viewer.prev` / `viewer.next` 走这里）。 */
  const stepAnchor = (delta: -1 | 1): void => {
    const list = photos();
    const at = list.findIndex((photo) => photo.id === store.selection().anchor);
    if (at < 0) return;
    const next = list[at + delta];
    if (next === undefined) return;
    store.select(Number(next.id), "replace");
  };

  /*
   * 把「缩放 / 适配」注册给看图命令槽（`components/ui/viewer/actions.ts`）：
   * 命令只有一份（`viewer.zoomIn` / `fit` / `actual`，键 `=` `-` `0` `1`），
   * 谁挂载谁提供实现 —— 编辑器不另立一套命令 id。
   *
   * `close`（`Esc`）**故意是空动作**：在编辑器里 `Esc` 的语义是「退出当前工具」
   * （W1 已定），不是「退出编辑器」—— 顺手退出编辑会让人莫名其妙掉回浏览。
   */
  const viewerActionsImpl: ViewerActions = {
    zoomIn: () => sendIntent({ kind: "zoomBy", factor: EDITOR_ZOOM_STEP }),
    zoomOut: () => sendIntent({ kind: "zoomBy", factor: 1 / EDITOR_ZOOM_STEP }),
    toggleFit: () => sendIntent({ kind: "toggleFit" }),
    actual: () => sendIntent({ kind: "fit", mode: "oneToOne" }),
    next: () => stepAnchor(1),
    prev: () => stepAnchor(-1),
    close: () => {},
  };

  /** 工作区动作槽（`App.tsx` 的 `viewing()` / `filmVisible()` 读数靠它）。 */
  const editorActionsImpl: EditorActions = {
    viewing: () => current() !== null && props.store.holeActive(),
    filmVisible: () => props.store.showsFilm(),
    comparing: () => false,
    cycleChrome: () => props.store.cycleTab(),
    resetChrome: () => props.store.resetChrome(),
    hasPhoto: () => current() !== null,
    fullscreenTarget,
  };

  onMount(() => {
    registerViewerActions(viewerActionsImpl);
    registerEditorActions(editorActionsImpl);
    onCleanup(() => {
      registerViewerActions(null);
      registerEditorActions(null);
    });
  });

  /** 四态空态（`features/editor/source.ts` 的纯函数，顺序即优先级）。 */
  const empty = createMemo(() =>
    editorEmptyKind({
      loadingRepositories: false,
      repositoryId: store.repositoryId(),
      scopePath: store.scopePath(),
      photoCount: store.total(),
      hasSelection: current() !== null,
      loadingPhotos: store.loading(),
    }),
  );

  /** 右栏控件的可用性：有照片才动得了（空态一律禁用）。 */
  const enabled = (): boolean => current() !== null;

  /** 「信息」页签的字段（格式化的唯一实现在 `features/exif-strip/exif-format.ts`）。 */
  const info = createMemo<EditorPhotoInfo | null>(() => {
    const item: AssetItem | null = store.anchorItem();
    if (item === null) return null;
    const exif = assetItemExif(item);
    return {
      fileName: item.fileName,
      relativePath: item.relPath,
      format: formatText(exif.format) ?? null,
      camera: formatText(exif.camera) ?? null,
      lens: formatText(exif.lens) ?? null,
      iso: formatIso(exif.iso) ?? null,
      shutter: formatShutter(exif.exposureSeconds) ?? null,
      aperture: formatAperture(exif.fNumber) ?? null,
      focal: formatFocalLength(exif.focalLengthMm) ?? null,
      dimensions: formatDimensions(exif.widthPx, exif.heightPx) ?? null,
      megapixels: formatMegapixels(exif.widthPx, exif.heightPx) ?? null,
    };
  });

  /**
   * 直方图取数（注入给右栏组件 —— 组件那一层不碰 `api`，`components/ui` 不许 import `api`）。
   *
   * 这里直接用 `api/db.ts` 的取数口，与浏览右栏**同一个函数**（`HistogramPanel` 管缓存）。
   */
  const loadHistogram = (path: string, bins: number) => getHistogram(path, bins);

  /*
   * 进编辑 = 重新读一遍当前目录（`AGENTS.md` §2.13）：在别处导入/删掉的照片，
   * 切过来就该看到 —— 不是「等谁点刷新」。
   */
  onMount(() => {
    void store.refresh();
  });

  /*
   * `Esc`：退出当前工具，等价于控制块的「取消」（`prompts/editor.pd` 的三种退出方式之一）。
   *
   * 不进命令注册表：这是**工具会话内部键**（与弹窗的确认/取消同性质），
   * `design/editor.md` §5 的接入评估里已经写明这个判断。
   * 弹窗打开时让位给弹窗（否则按一下 Esc 会同时关弹窗与工具）。
   */
  onMount(() => {
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key !== "Escape") return;
      if (props.store.tool() === null) return;
      if (document.querySelector('[role="dialog"]') !== null) return;
      event.preventDefault();
      props.store.closeTool();
    };
    window.addEventListener("keydown", onKeyDown);
    onCleanup(() => window.removeEventListener("keydown", onKeyDown));
  });

  return (
    <div
      class={["flex min-h-0 flex-1 flex-col", props.class ?? ""].filter(Boolean).join(" ")}
      data-editor-workspace
    >
      <div class="flex min-h-0 flex-1">
        {/*
          左列：**没有把手**（`.pd`：LUT 面板「不可调宽度」）。
          面板关掉时整列不占位置（`editorLutVisible` 同时管着档位与开关两个轴）。
        */}
        <aside
          class={[
            "flex min-h-0 shrink-0 flex-col bg-surface-main",
            props.store.lutVisible() ? "" : "hidden",
          ]
            .filter(Boolean)
            .join(" ")}
        >
          <LutPanel store={props.store} />
        </aside>

        {/*
          中列：视口 + 胶片带 + 照片状态栏。
          三块都在**同一列**里（`AGENTS.md` §11.1：没有跨列行，每列各自到底）。
        */}
        <main
          class={[
            "relative flex min-h-0 min-w-0 flex-1 flex-col",
            // 出图时整列透明（洞口之上一直到根节点都不能有底色，`bg-surface-bar` 会挡住 GPU）
            props.store.holeActive() ? "bg-transparent" : "bg-surface-bar",
          ].join(" ")}
          data-chrome={props.store.chromeName()}
          data-film={props.store.showsFilm() ? "on" : "off"}
        >
          <EditorViewport
            empty={empty()}
            hasPhoto={current() !== null}
            renderState={props.store.renderState}
            onOpenImport={props.onOpenImport}
            onRetry={startRenderer}
          />

          {/* 胶片带（自带顶部三点缩放把手，全项目唯一那一份） */}
          <Show when={props.store.showsFilm()}>
            <FilmStrip
              viewer={strip}
              selectedIds={source().selection().ids}
              onSelect={(id, mode) => source().select(id, mode)}
              thumbs={thumbs}
              sizeStep={props.filmStripStep}
              onSizeStepChange={props.onFilmStripStepChange}
              class="shrink-0"
            />
          </Show>

          {/* 状态栏：与看图态**同一条**（`PhotoStatusBar`），只换内容 */}
          <PhotoStatusBar info={viewingInfoOf(current())} />
        </main>

        {/* 右列：固定宽（`--panel-w-right`，所有工作流通用），三个页签组 */}
        <aside
          class={[
            "flex w-panel-w-right min-h-0 shrink-0 flex-col bg-surface-main",
            props.store.showsRight() ? "" : "hidden",
          ]
            .filter(Boolean)
            .join(" ")}
        >
          <EditorPanels
            store={props.store}
            enabled={enabled()}
            current={current()}
            info={info()}
            thumbs={thumbs}
            loadHistogram={loadHistogram}
          />
        </aside>
      </div>
    </div>
  );
}

/** 无障碍名（胶片带的把手等组件的默认文案来自语言包，这里只留一个锚点）。 */
export const EDITOR_WORKSPACE_LABEL = t("flow.edit");
