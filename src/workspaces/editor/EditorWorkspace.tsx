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

import {
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  onMount,
  Show,
  untrack,
  type JSX,
} from "solid-js";

import {
  bindEditorRenderer,
  commitDevelopStack,
  getDevelopEditTarget,
  getDevelopStack,
  getEditorRenderState,
  refreshDevelopPreview,
  resetDevelopStack,
  setEditorParams,
  setEditorPhoto,
  unbindEditorRenderer,
  sendEditorViewportIntent,
} from "../../api/editor.ts";
import { createLatestCoalescer } from "../../lib/editor-intent.ts";
import { isTauriRuntime } from "../../api/tauri-env.ts";
import type {
  DevelopParamsPayload,
  EditorRenderState,
  EditorViewportIntent,
  FileExif,
} from "../../api/types.ts";
import type { AssetItem, RepositoryView } from "../../api/types.ts";
import { getHistogram, getThumbBytes, listRepositories, readFileExif } from "../../api/db.ts";
import { formatDateTime } from "../../lib/datetime.ts";
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
import { formatExposureBias } from "../../features/exif-strip/index.ts";
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
import { locale, t } from "../../i18n/index.ts";

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
  /** 编辑栈落库 / 读取的错误（**不静默**：画面还是对的，但改动会丢，必须让人看见）。 */
  const [developError, setDevelopError] = createSignal<string | null>(null);

  const applyRenderState = (state: EditorRenderState | null): void => {
    props.store.setRenderState(state);
    props.store.setHoleActive(
      state !== null && state.bound && state.ready && state.paintedPath !== null,
    );
    /*
     * 拍摄色温（K）由渲染线程从 RAW 元数据算出来 —— 它是**色温拉杆的基线**
     * （`AGENTS.md` §11.5：载入照片时标尺要挪到照片自己的色温上）。
     * 写进 store 之后，载荷里的 `asShotTemperature` 跟着变 ⇒ 参数自动重发一次。
     */
    props.store.setAsShotTemperature(state?.asShotTemperature ?? null);
  };

  /** 发一条视口意图（缩放 / 平移 / 适配）——失败只记控制台，不带崩界面。 */
  const sendIntent = (intent: EditorViewportIntent): void => {
    void sendEditorViewportIntent(intent).catch((error: unknown) => {
      console.error("[editor] 视口意图失败", error); // i18n-exempt: 控制台诊断
    });
  };

  /**
   * 手动输入的目标缩放（`1.0` = 100%）：按当前值折成 `zoomBy` 的倍率。
   *
   * 为什么不新增一个 `zoomTo` 意图：视口的夹取与错点保持都在 Rust 的 `zoom_at` 里，
   * 一个倍率就够表达「到那个倍率」—— 少一条命令、少一处要同步的语义。
   */
  const zoomTo = (target: number): void => {
    const current = props.store.renderState()?.zoom ?? null;
    if (current === null || current <= 0 || !Number.isFinite(target) || target <= 0) return;
    sendIntent({ kind: "zoomBy", factor: target / current });
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

  /**
   * **参数 → 管线**：载荷一变就发（帧合并 + 尾样本必发，见 `createLatestCoalescer`）。
   *
   * 拖动中每帧一条完全没问题：Rust 侧按「最新者优先」丢弃过期任务，
   * 而且管线跑在**显影线程**上 —— 渲染线程永远只管画上一张，拖动与缩放不会卡。
   */
  const paramsSender = createLatestCoalescer<DevelopParamsPayload>({
    send: (payload) => {
      void setEditorParams(payload).catch((error: unknown) => {
        // 参数被拒（非法值 / 渲染线程没了）**不能只进控制台**：画面会停在最后一帧，
        // 用户看到的是「拉什么杆都没反应」。走面板那条错误通道，让原因留在界面上。
        console.error("[editor] 显影参数被拒", error); // i18n-exempt: 控制台诊断
        setDevelopError(String(error));
      });
    },
  });
  onCleanup(() => {
    paramsSender.dispose();
  });

  createEffect(() => {
    const payload = props.store.developPayload();
    if (!rendererBound()) return;
    paramsSender.push(payload);
  });

  /** 锚点一变就换纹理（渲染线程自己负责解码与两档切换）。 */
  createEffect(() => {
    if (!rendererBound()) return;
    const assetId = current()?.id;
    const repositoryId = store.repositoryId();
    if (assetId === null || assetId === undefined || repositoryId === null) {
      void setEditorPhoto(null).catch(() => undefined);
      return;
    }
    /*
     * **编辑落在 RAW 上**（`REPOSITORY.md` §4.1）：位图 + RAW 时要编辑 `_RAW/` 里那个 RAW。
     * 哪个文件、路径怎么拼由 Rust 侧解析（`develop_edit_target`）—— 前端不拼路径。
     * 解析失败就退回当前显示的路径（至少还能看/能编辑位图）。
     */
    void getDevelopEditTarget(repositoryId, Number(assetId))
      .then((target) => target ?? currentPath())
      .catch(() => currentPath())
      .then((path) => setEditorPhoto(path))
      .catch((error: unknown) => {
        console.error("[editor] 换照片失败", error); // i18n-exempt: 控制台诊断
      });
  });

  /**
   * **落库**（覆盖式）：松手 / 点重置时把当前载荷写进 catalog。
   *
   * 拖动中每帧都写库会把单写者线程淹掉，而且没有任何意义 ——
   * 所以高频那条路是 `setEditorParams`（只发内存参数给渲染线程），
   * 这一条只在**松手**时走一次。
   */
  const commitDevelop = (): void => {
    const assetId = current()?.id;
    const repositoryId = store.repositoryId();
    if (assetId === null || assetId === undefined || repositoryId === null) return;
    if (!props.store.developDirty()) return;
    const rev = props.store.developRev();
    const payload = props.store.developPayload();
    void commitDevelopStack(repositoryId, Number(assetId), {
      values: payload.values,
      curves: payload.curves,
      // 色温基线跟着一起存（否则缩略图那条路会用另一个基线渲染出另一种颜色）
      asShotK: payload.asShotTemperature,
    })
      .then(() => {
        // 只标「已落库」：库里回读的那一份与刚发出去的一致（Rust 侧会回读一遍验证）。
        // 撤销标签由 `browse` 的 undoState 统一显示（编辑与标记共用一套撤销栈），
        // 这里不再存第二份。
        props.store.markCommitted(rev);
        /*
         * 缩略图要重取：编辑结果变了，旧的那张（SOOC）不该再显示。
         * **只失效当前这一张**（`refresh`）—— `clear()` 会把整条胶片带每一格的 URL 都回收，
         * 松一次手整条带子全量重画（2026-09-24 人类报的「最严重」那一条）。
         */
        const path = currentPath();
        if (path !== null) thumbs.refresh(path);
      })
      .catch((error: unknown) => {
        // 落库失败不静默：画面还是对的，但下次换照片会丢 —— 必须让人知道
        console.error("[editor] 编辑栈落库失败", error); // i18n-exempt: 控制台诊断
        setDevelopError(String(error));
      });
  };

  /** 重置全部：库里清空 + 参数回默认（一次点击两个动作，别只做一半）。 */
  const resetDevelop = (): void => {
    const assetId = current()?.id;
    const repositoryId = store.repositoryId();
    props.store.resetParams();
    if (assetId === null || assetId === undefined || repositoryId === null) return;
    void resetDevelopStack(repositoryId, Number(assetId)).catch((error: unknown) => {
      console.error("[editor] 重置编辑栈失败", error); // i18n-exempt: 控制台诊断
      setDevelopError(String(error));
    });
  };

  /**
   * **撤销 / 重做之后重读编辑栈**。
   *
   * 显影参数与曲线都进的是**同一个撤销栈**（`browse` 的标记操作与编辑共用一套历史），
   * 所以 Ctrl+Z 很可能改的就是编辑栈 —— 必须重新读回来，否则画面与库里会对不上。
   */
  createEffect(() => {
    const tick = store.undoTick();
    if (tick === 0) return; // 初次挂载不必重读（下面的换照片分支会读）
    const assetId = current()?.id;
    const repositoryId = store.repositoryId();
    if (assetId === null || assetId === undefined || repositoryId === null) return;
    void getDevelopStack(repositoryId, Number(assetId))
      .then((stack) => {
        if (stack === null) return;
        props.store.loadDevelop(stack.values, stack.curves);
      })
      .catch((error: unknown) => {
        console.error("[editor] 撤销后重读编辑栈失败", error); // i18n-exempt: 控制台诊断
        setDevelopError(String(error));
      });
  });

  /**
   * **换照片：先把库里那份编辑栈读回来**（`latest` 存储位）。
   *
   * 两条防线：
   *
   * 1. 读回来之前**记下 rev**，回来时 rev 变了（用户已经动过）就**不覆盖**用户的改动；
   * 2. 离开这张照片时（effect 的 cleanup）如果还有没落库的改动，**先存到上一张上** ——
   *    松手落库是主路径，这一条是兜底（拖动中途换照片、或松手事件被别的东西吃掉）。
   */
  /**
   * **preview 的刷新**（`IMAGING.md` §4）：编辑器**进 / 出**两个节点各一次。
   *
   * 这是**后台那一路** —— 不等它（一张 1920 的 AVIF 要几百毫秒），失败也只记日志：
   * 预览没备好最多让下次看图慢一点，不该影响编辑。
   */
  const refreshPreview = (path: string | null): void => {
    if (path === null || path === "") return;
    void refreshDevelopPreview(path).catch((error: unknown) => {
      console.error("[editor] 预览图刷新失败", error); // i18n-exempt: 控制台诊断
    });
  };

  let lastPhoto: { repositoryId: string; assetId: number; path: string | null } | null = null;
  createEffect(() => {
    const assetId = current()?.id;
    const repositoryId = store.repositoryId();
    const path = currentPath();
    onCleanup(() => {
      // 换照片 / 卸载：① 还没落库的改动存到**上一张**上；② 把 preview 更新到最后状态
      const previous = lastPhoto;
      if (previous === null) return;
      if (untrack(() => props.store.developDirty())) {
        const payload = untrack(() => props.store.developPayload());
        void commitDevelopStack(previous.repositoryId, previous.assetId, {
          values: payload.values,
          curves: payload.curves,
        })
          // 落库**之后**才刷新：preview 读的就是库里的编辑栈（先刷新会拿到旧栈）
          .then(() => refreshPreview(previous.path))
          .catch((error: unknown) => {
            console.error("[editor] 切换照片前落库失败", error); // i18n-exempt: 控制台诊断
          });
      } else {
        // 没改动也刷一次：上一次落库已经把缓存作废了（「退出编辑」那个节点）
        refreshPreview(previous.path);
      }
    });

    if (assetId === null || assetId === undefined || repositoryId === null) {
      lastPhoto = null;
      return;
    }
    const id = Number(assetId);
    lastPhoto = { repositoryId, assetId: id, path };
    // 「进编辑」那个节点：把 preview 备好（缓存命中时只读一次，很便宜）
    refreshPreview(path);
    const revAtRequest = props.store.developRev();
    void getDevelopStack(repositoryId, id)
      .then((stack) => {
        if (stack === null) return;
        // 读的过程中用户已经动过：**不要**用库里那份盖掉他的改动
        if (props.store.developRev() !== revAtRequest) return;
        props.store.loadDevelop(stack.values, stack.curves);
      })
      .catch((error: unknown) => {
        console.error("[editor] 读编辑栈失败", error); // i18n-exempt: 控制台诊断
        setDevelopError(String(error));
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
    resetDevelop,
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
  /**
   * 这张照片加了**二级锁**（不可编辑）吗。
   *
   * 锁的语义就是「不可编辑」（`AGENTS.md` §11.5 的标记体系），所以参数面板整列禁用 ——
   * 让拉杆能拖、拖完又被后端跳过，比禁用更糟。
   */
  const locked = (): boolean => {
    const id = current()?.id;
    if (id === null || id === undefined) return false;
    return (store.itemById(Number(id))?.lockLevel ?? 0) >= 2;
  };

  const enabled = (): boolean => current() !== null && !locked();

  /** 右栏「信息」页签的**文件级 EXIF**（按需读文件头，不落库；毫秒级） */
  const [fileExif, setFileExif] = createSignal<FileExif | null>(null);
  createEffect(() => {
    const path = currentPath();
    if (path === null) {
      setFileExif(null);
      return;
    }
    let cancelled = false;
    void readFileExif(path)
      .then((file) => {
        if (!cancelled) setFileExif(file);
      })
      .catch(() => {
        if (!cancelled) setFileExif(null);
      });
    onCleanup(() => {
      cancelled = true;
    });
  });

  /**
   * 「信息」页签的字段（人类 2026-09-24 的口径）：
   * **只收 flowbar 没有的**（机型/镜头/ISO/快门/光圈/焦距/尺寸/格式都在 flowbar 右侧）；
   * 与调节最紧的色温置顶。格式化只走 `exif-strip` / `lib/datetime` 的现成实现。
   */
  const info = createMemo<EditorPhotoInfo | null>(() => {
    const item: AssetItem | null = store.anchorItem();
    if (item === null) return null;
    const baseline = props.store.asShotTemperature();
    const current = props.store.paramValue("temperature");
    const taken = fileExif()?.takenAtMs ?? null;
    const offset = fileExif()?.takenAtOffsetMin;
    return {
      fileName: item.fileName,
      relativePath: item.relPath,
      temperatureBaseline: baseline === null ? null : `${Math.round(baseline)} K`,
      temperatureCurrent: `${Math.round(current)} K`,
      exposureBias: formatExposureBias(fileExif()?.exposureBiasEv ?? null) ?? null,
      takenAt:
        taken === null
          ? null
          : formatDateTime(taken, offset === null ? undefined : offset, locale()),
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
            onCommit={commitDevelop}
            onReset={resetDevelop}
            error={developError()}
            locked={locked()}
            zoom={props.store.renderState()?.zoom ?? null}
            onZoomBy={(factor) => sendIntent({ kind: "zoomBy", factor })}
            onZoomTo={zoomTo}
          />
        </aside>
      </div>
    </div>
  );
}

/** 无障碍名（胶片带的把手等组件的默认文案来自语言包，这里只留一个锚点）。 */
export const EDITOR_WORKSPACE_LABEL = t("flow.edit");
