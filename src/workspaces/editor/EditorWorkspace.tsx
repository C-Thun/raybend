import { onCatalogChanged } from "../../api/db.ts";
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
  batch,
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
  developSettingsOf,
  confirmEditorTool,
  getDevelopEditTarget,
  getDevelopStack,
  getLensMatch,
  getBaseCurveProfiles,
  fitBaseCurveAndAutoAdjust,
  getEditorRenderState,
  refreshDevelopPreview,
  renameBaseCurveProfile,
  setEditorParams,
  setEditorPhoto,
  setEditorReferenceIssue,
  unbindEditorRenderer,
  sendEditorViewportIntent,
} from "../../api/editor.ts";
import { createEasyDestroy, type ShiftLikeEvent } from "../../lib/easy-destroy.ts";
import { EasyDestroyHost } from "../../components/ui/EasyDestroy.tsx";
import { createLatestCoalescer } from "../../lib/editor-intent.ts";
import { getLutLibrary, createLutCategory, importLutDirectory, hideLut, type LutLibrary } from "../../api/lut.ts";
import { getIssueLibrary, createIssue, deleteIssue, getIssueThumb, type IssueLibrary, type Issue } from "../../api/issues.ts";
import { pendingLegacyLutCategories, markLegacyLutCategoriesImported } from "../../lib/editor-prefs.ts";
import { pickDirectory } from "../../api/dialog.ts";
import type { DevelopStack } from "../../api/editor.ts";
import { newLutCategoryId } from "../../lib/lut-library.ts";
import { Button } from "../../components/ui/Button.tsx";
import { Dialog } from "../../components/ui/Dialog.tsx";
import { isTauriRuntime } from "../../api/tauri-env.ts";
import type {
  DevelopParamsPayload,
  EditorRenderState,
  EditorViewportIntent,
} from "../../api/types.ts";
import type { AssetItem, RepositoryView } from "../../api/types.ts";
import type { BaseCurveLibrary } from "../../api/editor.ts";
import { getHistogram, getThumbBytes, getViewImage, listRepositories } from "../../api/db.ts";
import { formatDateTime } from "../../lib/datetime.ts";
import { browseSource } from "../../features/browse/grid-source.ts";
import type { BrowseStore } from "../../features/browse/store.ts";
import {
  EditorPanels,
  createLensQuery,
  EditorViewport,
  EDITOR_ZOOM_STEP,
  LutPanel,
  createEditorStrip,
  editorEmptyKind,
  editorVisibleRenderState,
  type EditorPhotoInfo,
  type EditorStore,
} from "../../features/editor/index.ts";
import { registerEditorActions, type EditorActions } from "../../features/editor/actions.ts";
import {
  formatAperture, formatDimensions, formatExposureBias, formatFocalLength,
  formatIso, formatShutter,
  assetItemExif,
  type SelectedFileMetadata,
} from "../../features/exif-strip/index.ts";
import { createThumbQueue } from "../../components/ui/thumb-queue.ts";
import { ConfirmDialog } from "../../components/ui/Dialog.tsx";
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
  selectedMetadata: SelectedFileMetadata;
  /** browse 自己的胶片带尺寸档位（editor 用**自己那一份**，见 `lib/film-strip-prefs.ts`） */
  filmStripStep: number;
  onFilmStripStepChange: (step: number) => void;
  /** 「去导入」（空态里那颗按钮） */
  onOpenImport: () => void;
  class?: string;
}

export function EditorWorkspace(props: EditorWorkspaceProps): JSX.Element {
  const store = props.browse;
  const [sourceRevision, setSourceRevision] = createSignal(0);
  const [repositories, setRepositories] = createSignal<RepositoryView[]>([]);

  /** 胶片带/网格小图仍共用 grid 队列；总览取完整的 Screen 图。 */
  const thumbs = createThumbQueue({
    load: async (path) => (await getThumbBytes(path, "grid")) ?? null,
  });
  const overviewImages = createThumbQueue({
    load: (path) => getViewImage(path, "screen"),
    concurrency: 2,
    maxEntries: 24,
  });
  onMount(() => {
    let disposed = false;
    let off: (() => void) | undefined;
    void onCatalogChanged((change) => {
      if (change.repositoryId !== store.repositoryId()) return;
      for (const path of change.paths) {
        if (thumbs.get(path).status !== "idle") thumbs.refresh(path);
        if (overviewImages.get(path).status !== "idle") overviewImages.refresh(path);
      }
      if (change.assetIds.includes(Number(currentAssetId()))) setSourceRevision((value) => value + 1);
      void listRepositories().then(setRepositories).catch((error: unknown) => store.reportError(error));
    }).then((unsubscribe) => { if (disposed) unsubscribe(); else off = unsubscribe; });
    onCleanup(() => { disposed = true; off?.(); });
  });
  onCleanup(() => {
    thumbs.clear();
    overviewImages.clear();
  });

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

  const applyLutLibrary = (library: LutLibrary): void => {
    props.store.setLutCategories(library.categories.map((category) => ({
      ...category,
      entries: library.entries.filter((entry) => entry.categoryId === category.id).map((entry) => ({
        id: entry.id, name: entry.originalFilename, available: entry.available,
        coverAvailable: entry.coverAvailable, hidden: entry.hidden,
      })),
    })));
  };
  onMount(() => {
    void getLutLibrary(pendingLegacyLutCategories())
      .then((library) => { if (library !== null) { applyLutLibrary(library); markLegacyLutCategoriesImported(); } })
      .catch((error: unknown) => setDevelopError(String(error)));
  });
  const createCategory = async (name: string): Promise<boolean> => {
    const trimmed = name.trim();
    if (trimmed === "" || props.store.lutCategories().some((item) => item.name.toLowerCase() === trimmed.toLowerCase())) return false;
    const id = newLutCategoryId(props.store.lutCategories());
    try {
      const library = await createLutCategory(id, trimmed);
      if (library === null) return props.store.addCategory(trimmed);
      applyLutLibrary(library);
      props.store.toggleCategory(id);
      return true;
    } catch (error) { setDevelopError(String(error)); return false; }
  };
  const importDirectory = async (categoryId: string): Promise<string | null> => {
    if (lutBusy()) return null;
    const path = await pickDirectory({ title: t("editor.lut.import") });
    if (path === null) return null;
    setLutBusy(true);
    try {
      const result = await importLutDirectory(path, categoryId);
      if (result !== null) {
        applyLutLibrary(result.library);
        if (result.skipped.length > 0) setDevelopError(result.skipped.slice(0, 3).join("；"));
        return t("editor.lut.importSummary").replace("{imported}", String(result.imported))
          .replace("{restored}", String(result.restored)).replace("{duplicates}", String(result.duplicates))
          .replace("{failed}", String(result.skipped.length));
      }
    } catch (error) { setDevelopError(String(error)); }
    finally { setLutBusy(false); }
    return null;
  };
  const hideLutEntry = async (id: string): Promise<void> => {
    try { const library = await hideLut(id); if (library !== null) applyLutLibrary(library); }
    catch (error) { setDevelopError(String(error)); }
  };

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
  // 只把稳定的 id 当作换图依赖：缩略图刷新会换 ViewerPhoto 对象引用。
  const currentAssetId = createMemo(() => current()?.id ?? null);
  const [compareReference, setCompareReference] = createSignal<string>("sooc");
  let compareReferenceSequence = 0;
  let lastCompareReferenceKey = "";
  let toolAssetId: string | null | undefined;
  createEffect(() => {
    const id = currentAssetId();
    if (toolAssetId !== undefined && id !== toolAssetId) {
      props.store.closeTool();
      setCompareReference("sooc");
    }
    toolAssetId = id;
  });
  // 先读这张的 latest 来源和参数，再允许首帧显影；避免先解 RAW、随后因旧栈是 SOOC 又解一次。
  const [developReadyAssetId, setDevelopReadyAssetId] = createSignal<string | null>(null);

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
  createEffect(() => {
    const path = currentPath();
    if (path !== null) thumbs.request(path, true);
  });
  // 目标文件由 Rust 解析（RAW / SOOC）。状态轮询里旧照片的帧不能冒充它。
  const [expectedPhotoPath, setExpectedPhotoPath] = createSignal<string | null>(null);

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
  const [resetOpen, setResetOpen] = createSignal(false);
  let resetTarget: string | null = null;
  const [lutBusy, setLutBusy] = createSignal(false);
  const [issueLibrary, setIssueLibrary] = createSignal<IssueLibrary | null>(null);
  const [issueFocusTick, setIssueFocusTick] = createSignal(0);
  const [finalizeOpen, setFinalizeOpen] = createSignal(false);
  const [finalizeName, setFinalizeName] = createSignal("");
  const [finalizeBusy, setFinalizeBusy] = createSignal(false);
  const issueDestroy = createEasyDestroy();

  const applyRenderState = (state: EditorRenderState | null): void => {
    const expected = untrack(expectedPhotoPath);
    const visibleState = editorVisibleRenderState(state, expected);
    const currentFrame = visibleState?.paintedPath != null;
    props.store.setRenderState(visibleState);
    props.store.setHoleActive(
      currentFrame && state !== null && state.bound && state.ready,
    );
    /*
     * 拍摄色温（K）由渲染线程从 RAW 元数据算出来 —— 它是**色温拉杆的基线**
     * （`AGENTS.md` §11.5：载入照片时标尺要挪到照片自己的色温上）。
     * 写进 store 之后，载荷里的 `asShotTemperature` 跟着变 ⇒ 参数自动重发一次。
     */
    props.store.setAsShotTemperature(
      state?.photoPath === expected ? (state?.asShotTemperature ?? null) : null,
    );
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
      // 镜头配置要后端读这张照片的拍摄参数 —— 载荷走的是**高频**那条路，
      // 所以这里只传两个标量（`null` = 还没有当前照片，后端就不解析镜头）
      const assetId = currentAssetId();
      void setEditorParams(
        store.repositoryId(),
        assetId === null || assetId === undefined ? null : Number(assetId),
        payload,
      ).catch((error: unknown) => {
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

  const rotationSender = createLatestCoalescer<number>({
    send: (degrees) => sendIntent({ kind: "setRotation", degrees }),
  });
  onCleanup(rotationSender.dispose);

  /** 工具草稿仅驻留在 Rust；前端发送选项和原始指针事实。 */
  createEffect(() => {
    if (!rendererBound()) return;
    const tool = props.store.tool();
    rotationSender.dispose();
    const initialRatio = untrack(() => {
      if (tool !== "crop") return null;
      const selected = props.store.cropRatio();
      const size = props.store.renderState()?.originalImage;
      return selected.id === "original" && size != null && size.height > 0
        ? size.width / size.height : selected.ratio;
    });
    sendIntent({ kind: "setTool", tool: tool === "compare" ? null : tool, initialRatio });
    if (tool === "rotate") props.store.setAngle(untrack(() => props.store.geometry()?.rotation ?? 0));
  });
  createEffect(() => {
    if (!rendererBound() || props.store.tool() !== "rotate") return;
    rotationSender.push(props.store.angle());
  });
  let seenToolRevision = 0;
  createEffect(() => {
    const state = props.store.renderState();
    if (state === null) return;
    const previousRevision = seenToolRevision;
    seenToolRevision = state.toolRevision;
    // 只认本次新增的拉直结果；退出期间也消费修订，旧快照不能覆盖刚进入的角度。
    if (state.toolRevision <= previousRevision || props.store.tool() !== "rotate") return;
    props.store.setAngle(state.rotation);
  });
  let lastCropKey = "";
  createEffect(() => {
    if (!rendererBound() || props.store.tool() !== "crop") { lastCropKey = ""; return; }
    const selected = props.store.cropRatio();
    const size = props.store.renderState()?.originalImage;
    const ratio = selected.id === "original" && size != null && size.height > 0
      ? size.width / size.height : selected.ratio;
    const key = `${selected.id}:${ratio ?? "free"}:${size?.width ?? 0}:${size?.height ?? 0}`;
    if (key === lastCropKey) return;
    lastCropKey = key;
    sendIntent({ kind: "setCropRatio", ratio });
  });

  createEffect(() => {
    if (props.store.editBase() !== "raw" && compareReference() === "raw") setCompareReference("sooc");
  });
  createEffect(() => {
    const bound = rendererBound();
    const tool = props.store.tool();
    const choice = compareReference();
    const assetId = currentAssetId();
    const repositoryId = store.repositoryId();
    const path = expectedPhotoPath();
    const library = issueLibrary();
    if (!bound || tool !== "compare" || assetId == null || repositoryId === null || path === null) {
      lastCompareReferenceKey = "";
      return;
    }
    const issueId = choice.startsWith("issue:") ? Number(choice.slice(6)) : null;
    if (issueId !== null && (!Number.isSafeInteger(issueId) || !library?.issues.some((issue) => issue.id === issueId))) return;
    const key = `${repositoryId}:${assetId}:${path}:${choice}`;
    if (lastCompareReferenceKey === key) return;
    lastCompareReferenceKey = key;
    const sequence = ++compareReferenceSequence;
    if (issueId !== null) {
      void setEditorReferenceIssue(repositoryId, Number(assetId), issueId, path, sequence)
        .catch((error: unknown) => {
          if (sequence === compareReferenceSequence && expectedPhotoPath() === path) setDevelopError(String(error));
        });
    } else if (choice === "sooc" || choice === "raw") {
      sendIntent({ kind: "setReferenceBase", base: choice, sequence });
    }
  });

  /** 三工具互斥由 store 管；渲染线程只接收对比开关并保存分线。 */
  createEffect(() => {
    if (!rendererBound()) return;
    const enabled = props.store.tool() === "compare";
    sendIntent({ kind: "setCompare", enabled });
  });

  /** 锚点一变就换纹理（渲染线程自己负责解码与两档切换）。 */
  createEffect(() => {
    if (!rendererBound()) return;
    const assetId = currentAssetId();
    const repositoryId = store.repositoryId();
    const editBase = props.store.editBase();
    sourceRevision();
    let active = true;
    onCleanup(() => {
      active = false;
    });
    setExpectedPhotoPath(null);
    setCompareReference("sooc");
    untrack(() => applyRenderState(props.store.renderState()));
    if (assetId === null || assetId === undefined || repositoryId === null) {
      void setEditorPhoto(null).catch(() => undefined);
      return;
    }
    if (developReadyAssetId() !== assetId) return;
    /*
     * **编辑落在哪个文件上**（人类 2026-09-24）：默认 RAW，总览图下的 SOOC / RAW 按钮
     * 可切（`store.editBase()` 是这个 effect 的依赖 —— 切一下就重新解析并重解这张图）。
     * 哪个文件、路径怎么拼由 Rust 侧解析（`develop_edit_target`）—— 前端不拼路径。
     * 解析失败就退回当前显示的路径（至少还能看/能编辑位图）。
     */
    void getDevelopEditTarget(repositoryId, Number(assetId), editBase)
      .then((target) => {
        if (!active) return null;
        // 两侧可用性给总览的切换按钮用：缺文件的那一侧禁用，不让用户白点
        props.store.setEditBaseAvailable({
          bitmap: target?.hasBitmap ?? false,
          raw: target?.hasRaw ?? false,
        });
        if (target?.actualBase && target.actualBase !== props.store.editBase()) {
          props.store.setEditBase(target.actualBase);
        }
        return target?.path ?? currentPath();
      })
      .catch(() => (active ? currentPath() : null))
      .then((path) => {
        if (!active) return;
        setExpectedPhotoPath(path);
        untrack(() => applyRenderState(props.store.renderState()));
        return setEditorPhoto(path);
      })
      .then(() => {
        if (!active) return;
        // 换图命令会作废上一张仍在解析的参数请求。照片入队后再补发
        // 当前参数，保证「空栈的新图」也不会短暂套着上一张的调整。
        paramsSender.push(props.store.developPayload());
        paramsSender.flush();
      })
      .catch((error: unknown) => {
        if (!active) return;
        console.error("[editor] 换照片失败", error); // i18n-exempt: 控制台诊断
      });
  });

  // IPC 调用的完成顺序可能与发起顺序不同；同一会话的编辑写入和 preview
  // 生成串行，避免一次旧的松手结果在较新的版本之后写回 latest 缓存。
  let persistTail: Promise<void> = Promise.resolve();
  const persist = <T,>(task: () => Promise<T>): Promise<T> => {
    const pending = persistTail.then(task, task);
    persistTail = pending.then(() => undefined, () => undefined);
    return pending;
  };

  const currentDevelopStack = (): DevelopStack => {
    const payload = props.store.developPayload();
    return { values: payload.values, curves: payload.curves, asShotK: payload.asShotTemperature,
      sourceBase: props.store.editBase(), lensProfile: payload.lensProfile,
      baseCurveProfile: props.store.baseCurveProfile(),
      baseCurvePoints: props.store.baseCurvePoints()?.map(([x,y]) => [x,y] as [number,number]) ?? null,
      autoAdjust: props.store.autoAdjustBaseline(),
      lutId: props.store.lutId(), lutEnabled: props.store.lutEnabledSetting(),
      lensEnabled: payload.lensEnabled, nrMethod: payload.nrMethod, geometry: payload.geometry };
  };

  /**
   * **落库**（覆盖式）：松手 / 点重置时把当前载荷写进 catalog。
   *
   * 拖动中每帧都写库会把单写者线程淹掉，而且没有任何意义 ——
   * 所以高频那条路是 `setEditorParams`（只发内存参数给渲染线程），
   * 这一条只在**松手**时走一次。
   */
  const commitDevelop = (): void => {
    const assetId = currentAssetId();
    const repositoryId = store.repositoryId();
    if (assetId === null || assetId === undefined || repositoryId === null) return;
    if (developReadyAssetId() !== assetId) return;
    if (!props.store.developDirty()) return;
    const rev = props.store.developRev();
    const path = currentPath();
    const stack = currentDevelopStack();
    void persist(async () => {
      const committed = await commitDevelopStack(repositoryId, Number(assetId), stack);
      store.noteDevelopCommit(committed);
      if (path !== null) {
        try {
          await refreshDevelopPreview(path);
        } catch (error) {
          console.error("[editor] 预览图刷新失败", error); // i18n-exempt: 控制台诊断
        }
      }
    })
      .then(() => {
        if (currentAssetId() === assetId && props.store.developRev() === rev) {
          props.store.markCommitted(rev);
        }
        if (path !== null) {
          thumbs.refresh(path);
          overviewImages.refresh(path);
        }
      })
      .catch((error: unknown) => {
        // 落库失败不静默：画面还是对的，但下次换照片会丢 —— 必须让人知道
        console.error("[editor] 编辑栈落库失败", error); // i18n-exempt: 控制台诊断
        setDevelopError(String(error));
      });
  };

  const confirmTool = (): void => {
    rotationSender.flush();
    const cropSetting = props.store.tool() === "crop"
      ? { id: props.store.cropRatioId(), width: props.store.cropWidth(), height: props.store.cropHeight() }
      : null;
    void confirmEditorTool().then((geometry) => {
      if (geometry === null) return;
      const confirmed = {
        ...geometry,
        cropRatio: cropSetting ?? props.store.geometry()?.cropRatio ?? geometry.cropRatio ?? null,
      };
      batch(() => {
        props.store.closeTool();
        props.store.setGeometry(confirmed);
      });
      commitDevelop();
    }).catch((error: unknown) => setDevelopError(String(error)));
  };

  const canReset = (): boolean => enabled() && !props.store.autoAdjusting() && props.store.resetStage() !== "none";
  const resetContext = (): string => `${store.repositoryId()}:${currentAssetId()}:${props.store.editBase()}`;
  const resetDevelop = (stage: "edits" | "automatic"): void => {
    if (!canReset() || props.store.resetStage() !== stage) return;
    props.store.resetDevelop(stage);
    commitDevelop();
  };
  const requestReset = (): void => {
    if (!canReset()) return;
    if (props.store.resetStage() === "automatic") { resetDevelop("automatic"); return; }
    resetTarget = resetContext();
    setResetOpen(true);
  };
  createEffect(() => {
    const context = resetContext();
    if (resetOpen() && resetTarget !== context) setResetOpen(false);
  });

  /**
   * **撤销 / 重做之后重读编辑栈**。
   *
   * 显影参数与曲线都进的是**同一个撤销栈**（`browse` 的标记操作与编辑共用一套历史），
   * 所以 Ctrl+Z 很可能改的就是编辑栈 —— 必须重新读回来，否则画面与库里会对不上。
   */
  createEffect(() => {
    const tick = store.undoTick();
    if (tick === 0) return; // 初次挂载不必重读（下面的换照片分支会读）
    const assetId = currentAssetId();
    const repositoryId = store.repositoryId();
    const path = currentPath();
    if (assetId === null || assetId === undefined || repositoryId === null) return;
    void getDevelopStack(repositoryId, Number(assetId))
      .then(async (stack) => {
        if (stack === null || currentAssetId() !== assetId) return;
        props.store.loadDevelop(stack.values, stack.curves, developSettingsOf(stack));
        if (path !== null) {
          await refreshDevelopPreview(path);
          thumbs.refresh(path);
          overviewImages.refresh(path);
        }
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

  const lensQuery = createLensQuery(getLensMatch);
  const [baseCurveLibrary, setBaseCurveLibrary] = createSignal<BaseCurveLibrary | null>(null);
  let autoAdjustRevision = 0;
  onCleanup(() => { autoAdjustRevision++; lensQuery.dispose(); });

  createEffect(() => {
    const assetId = currentAssetId();
    const repositoryId = store.repositoryId();
    const path = currentPath();
    let active = true;
    const previous =
      assetId === null || assetId === undefined || repositoryId === null
        ? null
        : { repositoryId, assetId: Number(assetId), path };
    onCleanup(() => {
      active = false;
      // 换照片 / 卸载：① 还没落库的改动存到**上一张**上；② 把 preview 更新到最后状态
      if (previous === null) return;
      if (untrack(() => props.store.developDirty())) {
        const stack = untrack(currentDevelopStack);
        void persist(async () => {
          await commitDevelopStack(previous.repositoryId, previous.assetId, stack);
          // 落库之后才刷新，preview 才能读到这份栈。
          if (previous.path !== null) await refreshDevelopPreview(previous.path);
        })
          .catch((error: unknown) => {
            console.error("[editor] 切换照片前落库失败", error); // i18n-exempt: 控制台诊断
          });
      } else {
        // 没改动也刷一次：上一次落库已经把缓存作废了（「退出编辑」那个节点）
        refreshPreview(previous.path);
      }
    });

    setDevelopReadyAssetId(null);
    autoAdjustRevision++;
    lensQuery.select(previous?.repositoryId ?? null, previous?.assetId ?? null);
    setBaseCurveLibrary(null);
    props.store.setAutoAdjusting(false);
    if (previous === null) return;
    const id = previous.assetId;
    void getBaseCurveProfiles(previous.repositoryId, id).then((library) => {
      if (active) setBaseCurveLibrary(library);
    }).catch((error: unknown) => {
      if (active) setDevelopError(String(error));
    });
    // 旧图的参数不能在新图载入期间继续被送进显影线程。
    untrack(() => {
      props.store.loadDevelop({}, {}, {});
      props.store.setAsShotTemperature(null);
      setDevelopError(null);
    });
    // 过渡帧会直接读现有 latest / SOOC / RAW 内嵌预览。此时不再并发生成
    // latest：它会争用 RAW worker 和显影内存；最后状态在离开照片时再刷新。
    // 只取一次快照，不能让 developRev 成为这个换图 effect 的依赖：
    // 否则每次拖拉杆都会重新读栈、刷新 preview、甚至把正在编辑的图换掉。
    const revAtRequest = untrack(() => props.store.developRev());
    void getDevelopStack(previous.repositoryId, id)
      .then((stack) => {
        if (!active) return;
        // 读的过程中用户已经动过：**不要**用库里那份盖掉他的改动
        if (stack !== null && props.store.developRev() === revAtRequest) {
          props.store.loadDevelop(stack.values, stack.curves, developSettingsOf(stack));
        }
        setDevelopReadyAssetId(previous.assetId.toString());
      })
      .catch((error: unknown) => {
        if (!active) return;
        console.error("[editor] 读编辑栈失败", error); // i18n-exempt: 控制台诊断
        setDevelopError(String(error));
        setDevelopReadyAssetId(previous.assetId.toString());
      });

  });

  const canAutoAdjust = (): boolean => enabled() && props.store.editBase() === "raw"
    && props.store.editBaseAvailable().raw && props.store.editBaseAvailable().bitmap
    && baseCurveLibrary()?.cameraMake != null && baseCurveLibrary()?.cameraModel != null
    && !props.store.autoAdjusting();

  const selectBaseCurve = (id: string | null): void => {
    if (!enabled() || props.store.editBase() !== "raw" || baseCurveLibrary()?.cameraMake == null) return;
    const profile = baseCurveLibrary()?.profiles.find((entry) => entry.id === id);
    if (id !== "none" && profile === undefined) return;
    props.store.setBaseCurve(id, profile?.points ?? null);
    commitDevelop();
  };

  const renameBaseCurve = async (id: string, name: string): Promise<void> => {
    const assetId = currentAssetId();
    const repositoryId = store.repositoryId();
    if (!enabled() || assetId == null || repositoryId === null) throw new Error(t("editor.baseCurve.failed"));
    const profile = await renameBaseCurveProfile(repositoryId, Number(assetId), id, name);
    if (profile === null) throw new Error(t("editor.baseCurve.renameFailed"));
    if (currentAssetId() !== assetId || store.repositoryId() !== repositoryId) return;
    setBaseCurveLibrary((library) => library === null ? null : {
      ...library, profiles: library.profiles.map((entry) => entry.id === id ? profile : entry),
    });
  };

  const autoAdjust = async (): Promise<void> => {
    if (!canAutoAdjust()) return;
    const request = ++autoAdjustRevision;
    const photo = currentAssetId();
    const repository = store.repositoryId();
    const base = props.store.editBase();
    const rev = props.store.developRev();
    props.store.setAutoAdjusting(true);
    try {
      const result = await fitBaseCurveAndAutoAdjust(repository!, Number(photo));
      if (request !== autoAdjustRevision || photo !== currentAssetId() ||
          repository !== store.repositoryId() || base !== props.store.editBase() ||
          rev !== props.store.developRev()) return;
      if (result === null) throw new Error(t("editor.baseCurve.failed"));
      const lens = await lensQuery.refresh();
      if (request !== autoAdjustRevision || photo !== currentAssetId() ||
          repository !== store.repositoryId() || base !== props.store.editBase() ||
          rev !== props.store.developRev()) return;
      batch(() => {
        props.store.setBaseCurve(result.profile.id, result.profile.points);
        props.store.applyAutoAdjust({
          values: { exposure: result.exposure, contrast: result.contrast, saturation: result.saturation },
          lensProfile: lens?.detected?.key ?? null,
          lensEnabled: lens?.detected ? true : null,
          nrMethod: null,
        });
      });
      setBaseCurveLibrary((current) => current === null ? current : {
        ...current,
        profiles: [...current.profiles.filter((profile) => profile.id !== result.profile.id), result.profile],
      });
      setDevelopError(null);
      commitDevelop();
    } catch (error) {
      if (request === autoAdjustRevision) setDevelopError(String(error));
    } finally {
      if (request === autoAdjustRevision) props.store.setAutoAdjusting(false);
    }
  };

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
    comparing: () => props.store.tool() === "compare",
    cycleChrome: () => props.store.cycleTab(),
    resetChrome: () => props.store.resetChrome(),
    hasPhoto: () => current() !== null,
    resetDevelop: requestReset,
    canReset,
    commitDevelop,
    autoAdjust: () => void autoAdjust(),
    canAutoAdjust,
    canFinalize: () => enabled() && (issueLibrary()?.canFinalize ?? false),
    finalize: () => { setFinalizeName(issueLibrary()?.suggestedName ?? ""); setFinalizeOpen(true); },
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
    const id = currentAssetId();
    if (id === null || id === undefined) return false;
    return (store.itemById(Number(id))?.lockLevel ?? 0) >= 2;
  };

  const enabled = (): boolean =>
    current() !== null && developReadyAssetId() === currentAssetId() && !locked();


  createEffect(() => {
    const assetId = currentAssetId();
    const repositoryId = store.repositoryId();
    const ready = developReadyAssetId();
    props.store.developRev();
    if (assetId === null || assetId === undefined || repositoryId === null || ready !== assetId) {
      setIssueLibrary(null);
      return;
    }
    const stack = currentDevelopStack();
    let active = true;
    const timer = window.setTimeout(() => {
      void getIssueLibrary(repositoryId, Number(assetId), locale() === "en-US", stack)
        .then((library) => { if (active) setIssueLibrary(library); })
        .catch((error: unknown) => { if (active) setDevelopError(String(error)); });
    }, 60);
    onCleanup(() => { active = false; window.clearTimeout(timer); });
  });
  const selectIssue = (stack: DevelopStack): void => {
    if (!enabled()) return;
    props.store.applyDevelop(stack.values, stack.curves, developSettingsOf(stack));
    commitDevelop();
  };
  const finalize = async (): Promise<void> => {
    if (!enabled() || !issueLibrary()?.canFinalize || finalizeBusy()) return;
    const repositoryId = store.repositoryId();
    const assetId = currentAssetId();
    if (repositoryId === null || assetId === null || assetId === undefined) return;
    const name = finalizeName().trim();
    if (name === "") return;
    setFinalizeBusy(true);
    try {
      commitDevelop();
      await persistTail;
      const library = await createIssue(repositoryId, Number(assetId), name, locale() === "en-US");
      if (library !== null) {
        setIssueLibrary(library);
        setFinalizeOpen(false);
        setIssueFocusTick((value) => value + 1);
        if (library.snapshotError !== null) setDevelopError(library.snapshotError);
      }
    } catch (error) { setDevelopError(String(error)); }
    finally { setFinalizeBusy(false); }
  };
  const requestDeleteIssue = (target: Issue, event: ShiftLikeEvent): void => {
    const repositoryId = store.repositoryId();
    const assetId = currentAssetId();
    if (repositoryId === null || assetId == null) return;
    issueDestroy.request(t("editor.issue.deleteConfirm").replace("{name}", target.name), async () => {
      const current = () => store.repositoryId() === repositoryId && currentAssetId() === assetId;
      if (current() && compareReference() === `issue:${target.id}`) setCompareReference("sooc");
      try {
        const library = await deleteIssue(repositoryId, Number(assetId), target.id, locale() === "en-US");
        if (current() && library !== null) setIssueLibrary(library);
      } catch (error) { if (current()) setDevelopError(String(error)); }
    }, event, t("editor.issue.deleteTitle"));
  };
  createEffect(() => { store.repositoryId(); currentAssetId(); issueDestroy.cancel(); });
  onCleanup(issueDestroy.cancel);

  createEffect(() => {
    const item = store.anchorItem();
    props.selectedMetadata.select(currentPath(), item === null ? null : assetItemExif(item));
  });
  createEffect(() => {
    const path = currentPath();
    const lens = lensQuery.state().data?.lensName;
    if (path !== null && path === props.selectedMetadata.path() && lens?.trim()) {
      props.selectedMetadata.enrich({ lens });
    }
  });

  /**
   * 「信息」页签的字段（人类 2026-09-24 的口径）：
   * 展示拍摄时文件中的原始信息；色温只用拍摄基线，不混入当前调整值。
   * 格式化只走 `exif-strip` / `lib/datetime` 的现成实现。
   */
  const info = createMemo<EditorPhotoInfo | null>(() => {
    const item: AssetItem | null = store.anchorItem();
    if (item === null) return null;
    const exif = props.selectedMetadata.file();
    const baseline = props.store.asShotTemperature();
    const taken = exif?.takenAtMs ?? null;
    const offset = exif?.takenAtOffsetMin;
    return {
      fileName: item.fileName,
      relativePath: item.relPath,
      tags: exif?.tags ?? [],
      temperatureBaseline: baseline === null ? null : `${Math.round(baseline)} K`,
      exposureBias: formatExposureBias(exif?.exposureBiasEv ?? null) ?? null,
      iso: formatIso(exif?.iso ?? undefined) ?? null,
      shutter: formatShutter(exif?.exposureMs == null ? undefined : exif.exposureMs / 1000) ?? null,
      aperture: formatAperture(exif?.fNumber ?? undefined) ?? null,
      focal: formatFocalLength(exif?.focalMm ?? undefined) ?? null,
      cameraMake: exif?.cameraMake ?? null,
      cameraModel: exif?.cameraModel ?? null,
      lens: exif?.lens ?? lensQuery.state().data?.lensName ?? null,
      dimensions: formatDimensions(exif?.width ?? undefined, exif?.height ?? undefined) ?? null,
      orientation: exif?.orientation == null ? null : String(exif.orientation),
      takenAt: taken === null ? null : formatDateTime(taken, offset === null ? undefined : offset, locale()),
      datetimeRaw: exif?.datetimeRaw ?? null,
      software: exif?.software ?? null,
      gps: exif?.gpsLat == null || exif?.gpsLon == null ? null : `${exif.gpsLat.toFixed(5)}, ${exif.gpsLon.toFixed(5)}`,
      format: exif?.ext?.toUpperCase() ?? null,
      kind: exif?.kind ?? null,
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
          <LutPanel store={props.store} onCreateCategory={createCategory} onImport={importDirectory}
            onSelect={(id) => { if (!enabled()) return; props.store.setLut(id, true); commitDevelop(); }}
            onToggle={commitDevelop} onHide={hideLutEntry} />
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
            tool={props.store.tool}
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
          <PhotoStatusBar info={viewingInfoOf(current())}
            compare={props.store.tool() === "compare" ? {
              reference: compareReference().startsWith("issue:")
                ? (issueLibrary()?.issues.find((entry) => `issue:${entry.id}` === compareReference())?.name ?? t("editor.issue.latest"))
                : t(compareReference() === "raw" ? "editor.base.raw" : "editor.base.sooc"),
              result: t("editor.compare.result"),
              choices: [
                { value: "sooc", label: t("editor.base.sooc"), selected: compareReference() === "sooc", disabled: !props.store.editBaseAvailable().bitmap },
                { value: "raw", label: t("editor.base.raw"), selected: compareReference() === "raw", disabled: props.store.editBase() !== "raw" },
                ...(issueLibrary()?.issues ?? []).map((issue) => ({
                  value: `issue:${issue.id}`, label: `${issue.name} · ${issue.sourceBase.toUpperCase()}`,
                  selected: compareReference() === `issue:${issue.id}`,
                })),
              ],
              onReferenceChange: (value: string) => {
                if (value === "sooc" || value === "raw" || value.startsWith("issue:")) setCompareReference(value);
              },
            } : undefined} />
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
            lensQuery={lensQuery.state()}
            baseCurveLibrary={baseCurveLibrary()}
            issues={issueLibrary()}
            issueFocusTick={issueFocusTick()}
            onSelectIssue={selectIssue}
            onDeleteIssue={requestDeleteIssue}
            loadIssueThumb={(issueId) => {
              const repo = store.repositoryId(); const asset = currentAssetId();
              return repo === null || asset === null || asset === undefined ? Promise.resolve(null)
                : getIssueThumb(repo, Number(asset), issueId, "strip");
            }}
            onSelectBaseCurve={selectBaseCurve}
            onRenameBaseCurve={renameBaseCurve}
            onRefreshLens={() => { void lensQuery.refresh(); }}
            overviewImages={overviewImages}
            loadHistogram={loadHistogram}
            onCommit={commitDevelop}
            onToolConfirm={confirmTool}
            error={developError() ?? store.error()}
            locked={locked()}
            zoom={props.store.renderState()?.zoom ?? null}
            onZoomBy={(factor) => sendIntent({ kind: "zoomBy", factor })}
            onZoomTo={zoomTo}
          />
        </aside>
      </div>
      <Dialog open={finalizeOpen()} onOpenChange={setFinalizeOpen} title={t("editor.issue.newTitle")}
        footer={<><Button variant="secondary" onClick={() => setFinalizeOpen(false)}>{t("common.cancel")}</Button>
          <Button variant="primary" disabled={finalizeBusy() || finalizeName().trim() === ""}
            onClick={() => void finalize()}>{t("editor.issue.save")}</Button></>}>
        <input class="w-full rounded-ui bg-surface-track px-2 py-2 text-fg-1" value={finalizeName()}
          maxlength={80} aria-label={t("editor.issue.name")}
          onInput={(event) => setFinalizeName(event.currentTarget.value)} />
      </Dialog>
      <EasyDestroyHost open={issueDestroy.pending() !== null} title={issueDestroy.pending()?.title}
        message={issueDestroy.pending()?.message ?? ""} confirmLabel={t("editor.issue.delete")}
        onCancel={issueDestroy.cancel} onConfirm={issueDestroy.confirm} />
      <ConfirmDialog
        open={resetOpen()}
        title={t("editor.reset.title")}
        message={t("editor.reset.confirm")}
        description={<p class="mt-2 text-fg-2">{t("editor.reset.keepBaseCurve")}</p>}
        confirmLabel={t("editor.reset.action")}
        onCancel={() => setResetOpen(false)}
        onConfirm={() => { const valid = resetTarget === resetContext(); setResetOpen(false); if (valid) resetDevelop("edits"); }}
      />
    </div>
  );
}

/** 无障碍名（胶片带的把手等组件的默认文案来自语言包，这里只留一个锚点）。 */
export const EDITOR_WORKSPACE_LABEL = t("flow.edit");
