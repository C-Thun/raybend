/**
 * 浏览工作区：三列（`memory/FUNCTION-BROWSE.md` §1、`design/browse.md` §2）。
 *
 * ```text
 * ┌────────────┬──────────────────────────────┬──────────────┐
 * │ 左列 300   │ 中列（弹性，surface-bar）     │ 右列 300     │
 * │ 库目录选择器 │ 照片网格 + 底部控制条         │ 信息栏        │
 * └────────────┴──────────────────────────────┴──────────────┘
 * ```
 *
 * 这是**组装层**：把 feature 与工作区共享状态接起来，自己不做业务判断
 * （`memory/ARCHITECTURE.md` §2）。库列表在这里拉一次 —— 属于工作区自己的生命周期。
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
  on,
  onMount,
  Show,
  untrack,
  type JSX,
} from "solid-js";

import {
  getThumbBytes,
  getViewImage,
  onCatalogChanged,
} from "../../api/db.ts";
import type { RepositoryStateStore } from "../../features/repositories/state.ts";
import {
  AssetInfo,
  BrowseLeftColumn,
  OrganizationPanel,
  type OrganizationSelection,
  type BrowseStore,
} from "../../features/browse/index.ts";
import { addPhotosToBucket, type AssetIdentity, type PhotoBucket } from "../../api/organization.ts";
import { FilterBar } from "../../features/browse/FilterBar.tsx";
import { browseSource } from "../../features/browse/grid-source.ts";
import { createOrganizationSource } from "../../features/browse/organization-source.ts";
import { beginPhotoDrag, dragIdsFor, photoDrag } from "../../features/browse/photo-drag.ts";
import {
  createPhotoViewingController,
  PhotoViewingStage,
} from "../../features/photo-grid/index.ts";
import { StateWatermark } from "../../components/ui/StateWatermark.tsx";
import {
  IconAlbumOff,
  IconAlertTriangle,
  IconFolder,
  IconPhoto,
  IconPhotoOff,
  IconTrash,
} from "@tabler/icons-solidjs";
import { createThumbQueue } from "../../components/ui/thumb-queue.ts";
import { buildFullscreenTarget } from "../../lib/fullscreen-target.ts";
import type { TilesViewingInfo } from "../../components/ui/tiles/index.ts";
import { createViewerStore, photosFromSource, viewingInfoOf } from "../../components/ui/viewer/index.ts";
import {
  browseDisplayByTime,
  browseDisplayTileStep,
  commitBrowseDisplayTileStep,
  setBrowseDisplayByTime,
  setBrowseDisplayTileStep,
} from "../../lib/display-prefs.ts";
import { browseInfoMode, cycleBrowseTileInfo } from "../../components/ui/tile-info.ts";
import { locale, t } from "../../i18n/index.ts";
import { Button } from "../../components/ui/Button.tsx";
import { ConfirmDialog, Dialog } from "../../components/ui/Dialog.tsx";
import { DragPreview } from "../../components/ui/DragPreview.tsx";
import type { ToastStore } from "../../components/ui/Toast.tsx";
import { registerBrowseActions } from "../../features/browse/actions.ts";
import { SplitHandle } from "../../components/ui/SplitHandle.tsx";
import { nudgeWidth, resizeWidth } from "../../lib/column-resize.ts";
import { joinPath } from "../../lib/paths.ts";
import { assetItemExif, type SelectedFileMetadata } from "../../features/exif-strip/index.ts";
import { ensureIssueSnapshots, getIssueLibrary, type IssueLibrary } from "../../api/issues.ts";
import { createBrowseDisplay } from "../../features/browse/display-variants.ts";
import { retainBrowseInfo, type BrowseInfoSnapshot } from "../../features/browse/info-snapshot.ts";
import { displayedReference, readDisplayVariantKey } from "../../lib/display-variant.ts";
import { displaySizeOf, issueOf, sourceBaseOf } from "../../lib/display-size.ts";
import { getExportSnapshots, getExportVariantImage, getExportVariantDetails } from "../../api/export.ts";
import { LAYOUT_BOUNDS } from "../../lib/layout-prefs.ts";

/**
 * 侧栏宽度的上下限 = **唯一那一份**（`lib/layout-prefs.ts` 的 `LAYOUT_BOUNDS`）。
 *
 * 早先这里手抄过一份边界，改存储侧时拖拽侧不会同步。现在两边共用一条；
 * 2026-09-20 下限与 import left 对齐为 220px。
 */
const SIDEBAR_BOUNDS = LAYOUT_BOUNDS.browseLeftWidth;
import type { AssetItem, BrowseSort, DeleteFailure, PurgeResult, RecycleResult } from "../../api/types.ts";

export interface BrowseWorkspaceProps {
  organizationPanel: "library" | "buckets" | "tags";
  organizationRevision?: number;
  onNewBucket: () => void;
  onEditBucket: (bucket: PhotoBucket) => void;
  onOrganizationChanged?: () => void;
  onOrganizationSelection?: (photos: AssetIdentity[]) => void;
  onOrganizationViewChange?: (active: boolean) => void;
  onOrganizationAnchor?: (photo: AssetIdentity | null, item: AssetItem | null) => void;
  onOrganizationItems?: (items: AssetItem[]) => void;
  onOrganizationActions?: (actions: { selectAll: () => void; clearSelection: () => void } | null) => void;
  onOrganizationBucketSelection?: (bucketId: number | null) => void;
  onExternalEditor?: (target:import("../../lib/external-editor.ts").ExternalTarget)=>void;
  store: BrowseStore;
  repositories: RepositoryStateStore;
  selectedMetadata: SelectedFileMetadata;
  /**
   * 点库卡片上的齿轮 → 打开**库设置**。
   *
   * 弹窗本身住在组装层（`App.tsx`）：导入侧与浏览侧点开的是同一个
   * `LibrarySettingsDialog`，而两个工作区都不该各自造一份弹窗状态。
   */
  onOpenLibrarySettings?: (repositoryId: string) => void;
  /**
   * 提示通道（`components/ui/Toast.tsx`）：删除这类**改磁盘**的动作必须给回执 ——
   * 谁删了什么、几个被锁挡住、哪几个失败了，都在提示与失败清单里说清楚。
   */
  toast?: ToastStore;
  /** 左列 / 右列宽度（像素，受控；拖拽松手时通过下面的回调落盘） */
  leftWidth?: number;
  /**
   * 右列宽度**不在 props 里**：它如今只有一份定义 —— `tokens.css` 的 `--panel-w-right`
   *（workspace 右列，所有工作流通用），容器上直接用 `w-panel-w-right`。
   *
   * 为什么之前是 props：那时 browse 右列有一个独立常量（`layout-prefs.ts` 的
   * `BROWSE_RIGHT_WIDTH`），与导入侧的令牌是**两份定义** —— 同一个东西两处写。
   * 人类 2026-09-23 定「统一宽度」，于是常量与 props 一起拆了。
   */
  onLeftWidthChange?: (width: number) => void;
  /** browse 自己的胶片带尺寸档位；与 import 分开存进 app.db。 */
  filmStripStep: number;
  onFilmStripStepChange: (step: number) => void;
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
  const [organizationSelection, setOrganizationSelection] = createSignal<OrganizationSelection>(null);
  const organizationPhotos = () => organizationSelection()?.kind === "tag" || organizationSelection()?.kind === "bucket";
  const organizationInfoAligned = () => {
    const id = organization.selection().anchor;
    const entry = id ? organization.entryById(id) : null;
    return entry !== null && entry !== undefined &&
      store.repositoryId() === entry.repositoryId && store.anchorItem()?.id === entry.assetId;
  };
  createEffect(() => props.onOrganizationViewChange?.(organizationPhotos()));
  createEffect(() => {
    const selected = organizationSelection();
    props.onOrganizationBucketSelection?.(selected?.kind === "bucket" ? selected.id : null);
  });
  const anchor = createMemo(() => store.anchorItem());
  const repositories = props.repositories.list;
  const reposLoading = () => props.repositories.status() === "loading" || props.repositories.status() === "idle";
  const reposError = props.repositories.error;
  /*
   * 「按时间」与格子尺寸档位走 browse 自己的设备级偏好（`lib/display-prefs.ts`）：
   * 与 import 复用同一套迁移/持久化实现，但两边的值互相隔离并可跨会话还原。
   * 以前这里是本地信号，切走再回来会重置。
   */
  const tileStep = browseDisplayTileStep;
  const setTileStep = setBrowseDisplayTileStep;
  const grouped = browseDisplayByTime;
  const setGrouped = setBrowseDisplayByTime;
  /**
   * 库列表是否展开（`memory/FUNCTION-BROWSE.md` §4.2）。
   *
   * 状态住在工作区而不是左列里：**收起要由网格侧的点击触发**，
   * 而那个点击发生在左列之外。切走工作流时整个工作区卸载 → 自然回到缩起态（
   * 这也正是三条收起条件里的「切换 flow」）。
   */
  const [libsExpanded, setLibsExpanded] = createSignal(false);
  /**
   * 排序的六个键（`specs/M2-W2-tail.md` 3.5；`deletedAt` 是回收站专用，正常视图的控件里不出现）。
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
    deletedAt: () => t("browse.sortDeletedAt"),
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
  /*
   * 右列宽度**固定**（`--panel-w-right`，不参与拖拽）：人类 2026-09-19 定「把手只加在左侧边界」，
   * 2026-09-23 又把它统一成**所有工作流共用的一个值** —— 所以这里不再算宽度，
   * 容器上直接挂 `w-panel-w-right`（与导入侧那一列同一个类名、同一个令牌）。
   */
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
  const [focusId, setFocusId] = createSignal<string | undefined>(undefined);
  /** 待确认的删除（张数；`null` = 没在确认） */
  const [pendingDelete, setPendingDelete] = createSignal<number | null>(null);
  /** 待确认的清空回收站（`true` = 弹着强确认） */
  const [pendingEmpty, setPendingEmpty] = createSignal(false);
  /** 删除失败清单（有它就弹模态逐条列出来） */
  const [deleteFailures, setDeleteFailures] = createSignal<DeleteFailure[]>([]);
  /*
   * 看图（`components/ui/viewer/` 的共享件）。
   *
   * 取图走**统一取图口**（`getViewImage` → Rust 侧 `display` 模块，
   * 见 `specs/M2-W2.md` §2.1）：屏幕档由它决定「给原图还是渲染」，
   * 小图（秒显的底）还是网格那套缓存。这里只负责「谁触发打开」。
   */
  const display = createBrowseDisplay({
    snapshot: async(repository,asset,choice)=>{
      const [snapshot]=await getExportSnapshots(repository,[{assetId:asset,variant:choice}]);
      if(!snapshot)throw Error(t("export.error.incomplete"));return snapshot;
    },details:getExportVariantDetails,
    // 选中定稿变体：后台把该定稿的三档图补齐（静默、幂等；失败下一次再试）
    onVariantSelected: (repository, asset, choice) => {
      if (choice.startsWith("issue:")) void ensureIssueSnapshots(repository, asset);
    },
  });
  createEffect(()=>display.context(JSON.stringify([store.repositoryId(),store.scopePath()])));
  onCleanup(display.dispose);
  /*
   * 三个档一起走这一张口（2026-10-11）：
   *   screen = **载入占位**（1920，快）；full = **真身**（该变体自己的像素）；
   *   grid = 网格/胶片带小图。变体键（定稿/编辑结果）走 `export_variant_image`，
   *   普通文件路径走 `view_image` —— 后者对「没编辑过的位图 + full」直接给原文件字节。
   */
  const loadDisplay = (key:string,size:"screen"|"grid"|"full")=>{
    const target=readDisplayVariantKey(key);
    return target?getExportVariantImage(target.repositoryId,target.reference,size,target.captured):
      size==='screen'?getViewImage(key,'screen'):size==='full'?getViewImage(key,'full'):getThumbBytes(key,'grid');
  };
  const viewer = createViewerStore({
    loadScreen:key=>loadDisplay(key,'screen'),
    loadFull:key=>loadDisplay(key,'full'),
    loadThumb:key=>loadDisplay(key,'grid'),
  });

  /*
   * 缩略图队列：**网格与胶片带共用同一个**（`specs/M2-W2.md` 2.1）。
   *
   * 为什么提到工作区这一层：两个消费方都在中列，展示的也是同一个目录的照片 ——
   * 各建一个的话，同一张照片会被取两遍（多一趟 IPC + 两份内存），
   * 而共用之后「网格里已缓存的，胶片带里立刻就有」。
   * 换库时清空（否则不同库的同名相对路径会串图，M2-W1 踩过）。
   */
  const thumbs = createThumbQueue({
    load: async (path) => {
      const bytes = await loadDisplay(path, "grid");
      return bytes ?? null;
    },
  });
  const organization = createOrganizationSource({
    repositories,
    thumbs,
    tileStep,
    setTileStep,
    commitTileStep: commitBrowseDisplayTileStep,
    infoMode: browseInfoMode,
    filter: store.filter,
    onPartialFailure: (repositoryIds) => props.toast?.show({ tone: "danger",
      message: t("org.queryPartial").replace("{n}", String(repositoryIds.length)) }),
  });
  onMount(() => {
    props.onOrganizationActions?.({ selectAll: organization.selectAll, clearSelection: organization.clearSelection });
    onCleanup(() => props.onOrganizationActions?.(null));
  });
  createEffect(on(() => [organizationSelection(),
    repositories().filter((repo) => repo.online).map((repo) => `${repo.id}:${repo.connection?.generation}`).join("|"),
    props.organizationRevision, JSON.stringify(store.filter())] as const, ([selected]) => {
    void organization.load(selected);
  }));
  createEffect(() => {
    const selected = organization.selection().ids;
    props.onOrganizationSelection?.([...selected].flatMap((id) => {
      const entry = organization.entryById(id);
      return entry ? [{ repositoryId: entry.repositoryId, assetId: entry.assetId }] : [];
    }));
    untrack(organization.ensureSelected);
  });
  createEffect(() => {
    const selected = organization.selection().ids;
    props.onOrganizationItems?.([...selected].flatMap((id) => {
      const item = organization.itemById(id);
      return item ? [item] : [];
    }));
  });
  createEffect(() => {
    const id = organizationPhotos() ? organization.selection().anchor : null;
    const entry = id ? organization.entryById(id) : null;
    props.onOrganizationAnchor?.(entry ? { repositoryId: entry.repositoryId, assetId: entry.assetId } : null,
      id ? organization.itemById(id) : null);
  });
  const displaySession = createMemo(() => {
    const row = props.repositories.byId(store.repositoryId() ?? "");
    return row?.online ? `${row.id}:${row.connection?.generation ?? "0"}` : null;
  });
  createEffect(on(displaySession, session => {
    if (session === null) return;
    thumbs.clear();
    if (viewer.current()) viewer.reloadCurrent();
  }));
  onCleanup(() => thumbs.clear());
  const [issueLibrary, setIssueLibrary] = createSignal<IssueLibrary | null>(null);
  const [issueBusy, setIssueBusy] = createSignal(false);
  const [issueError, setIssueError] = createSignal<string | null>(null);
  const [issueRefreshTick, setIssueRefreshTick] = createSignal(0);
  const [issueResolvedContext, setIssueResolvedContext] = createSignal<string | null>(null);
  const issueContext=createMemo(()=>JSON.stringify([store.repositoryId(),store.anchorItem()?.id??null,store.undoTick(),issueRefreshTick(),locale()]));
  let issueAssetKey='';
  createEffect(on(issueContext,()=>{
    const context = issueContext();
    setIssueResolvedContext(null);
    const repositoryId=store.repositoryId(),assetId=store.anchorItem()?.id;
    const key=JSON.stringify([repositoryId,assetId]);
    if(key!==issueAssetKey){issueAssetKey=key;setIssueLibrary(null);setIssueError(null);}
    if(repositoryId===null||assetId===undefined)return;
    let active=true;
    void getIssueLibrary(repositoryId,assetId,locale()==='en-US')
      .then(library=>{if(active){setIssueLibrary(library);setIssueResolvedContext(context);}})
      .catch(error=>{if(active){setIssueLibrary(null);setIssueError(String(error));setIssueResolvedContext(context);}});
    onCleanup(()=>{active=false;});
  }));
  createEffect(() => {
    const tick = store.undoTick();
    if (tick === 0) return;
    const item = store.anchorItem();
    const base = root();
    if (item === null || base === null) return;
    const path = joinPath(base, item.relPath);
    thumbs.refresh(path);
    if (viewer.current()?.id === String(item.id)) viewer.reloadCurrent();
  });
  const selectBrowseIssue = async (choice: string): Promise<void> => {
    const repositoryId=store.repositoryId(), item=store.anchorItem();
    if(!repositoryId||!item||issueBusy())return;
    setIssueBusy(true);setIssueError(null);
    try {await display.select(repositoryId,item.id,choice);}
    catch(error){setIssueError(String(error));}
    finally{setIssueBusy(false);}
  };
  createEffect(()=>{
    const library=issueLibrary(),item=store.anchorItem();
    if(!library||!item)return;
    const choice=display.get(item.id)?.choice;
    if(choice?.startsWith('issue:')&&!library.issues.some(issue=>`issue:${issue.id}`===choice))display.reset(item.id);
  });


  /*
   * view / film / compare 的状态、锚点与尺寸补读只在共享控制器里维护。
   * 两个工作区的差异只剩数据适配：这里把库 id 转成绝对路径交给 browse store。
   */
  const viewing = createPhotoViewingController({
    viewer,
    /* browse 保持 M2 的**四档**（人类 2026-09-23 明确：browse 不动） */
    chromeMode: () => "browse",
    selection: () => organizationPhotos() ? organization.selection() : store.selection(),
    setAnchor: (id) => organizationPhotos() ? organization.source.setAnchor(id) : store.setAnchor(Number(id)),
    naturalOf: (id) => organizationPhotos() ? organization.source.naturalOf(id) : display.get(Number(id))?.natural ?? store.naturalOf(Number(id)),
    ensureNatural: (ids) => {
      if (organizationPhotos()) return organization.source.ensureNatural(ids.flatMap((id) => {
        const item = organization.source.itemById(id);
        return item ? [{ id, path: item.path }] : [];
      }));
      const base = root();
      if (base === null) return;
      const entries = ids.flatMap((id) => {
        const item = store.itemById(Number(id));
        return item === null ? [] : [{ id: item.id, path: joinPath(base, item.relPath) }];
      });
      return store.ensureNatural(entries);
    },
    onPreparingViewer: () => setLibsExpanded(false),
  });

  /**
   * 空态 / 加载 / 错误的水印（文案是浏览侧的：没选库 / 没选目录 / 空库）。
   * 这些条件本来写在 `BrowseGrid` 里 —— 那是**视图配置**，谁开这个视图谁给文案。
   */
  function gridWatermark(): JSX.Element | null {
    if (organizationPhotos()) {
      if (organization.error()) return <StateWatermark tone="error" icon={<IconAlertTriangle size={64} />}
        text={organization.error() ?? ""} action={{ label: t("common.retry"), run: () => void organization.load(organizationSelection()) }} />;
      if (organization.source.status() === "loading") return <StateWatermark animate delayMs={1500}
        icon={<IconPhoto size={64} />} text={t("browse.loading")} />;
      if (!repositories().some((repo) => repo.online && repo.root !== null))
        return <StateWatermark icon={<IconAlbumOff size={64} />} text={t("org.mountLibrary")} />;
      if (organization.source.count() === 0) return <StateWatermark icon={<IconPhotoOff size={64} />}
        text={t("org.noMatchingPhotos")} />;
      return null;
    }
    if (store.repositoryId() === null) {
      return (
        <StateWatermark
          icon={<IconAlbumOff size={64} stroke-width={1} />}
          text={t("browse.noRepository")}
        />
      );
    }
    if (store.scopePath() === null) {
      return (
        <StateWatermark
          icon={<IconFolder size={64} stroke-width={1} />}
          text={t("browse.pickDirectory")}
        />
      );
    }
    if (store.error() !== null && store.total() === 0) {
      return (
        <StateWatermark
          tone="error"
          icon={<IconAlertTriangle size={64} stroke-width={1} />}
          text={t("browse.load_error", { message: store.error() ?? "" })}
          action={{ label: t("common.retry"), run: () => void store.reload() }}
        />
      );
    }
    if (store.loading() && store.total() === 0) {
      return (
        /*
         * 读库的载入水印（人类 2026-09-23 定）：
         *
         *   * 动画就是共享的那套**流光**（光在字与图案上流过，见 `StateWatermark`）；
         *   * `delayMs` 给 **1500**：读库一般很快（命中会话缓存时几十毫秒），
         *     给 1.5 秒保底 —— 这期间就出图的话**完全不显示**水印，不闪一帧。
         *     只有真慢（大库、冷启动、机械盘）才会看到它，那时它是「它在忙」的凭据。
         */
        <StateWatermark
          animate
          delayMs={1500}
          icon={<IconPhoto size={64} stroke-width={1} />}
          text={t("browse.loading")}
        />
      );
    }
    if (store.total() === 0) {
      return (
        <StateWatermark
          icon={<IconPhotoOff size={64} stroke-width={1} />}
          text={t("browse.empty_lib")}
        />
      );
    }
    return null;
  }

  /*
   * `Tab` 循环四态（`memory/FUNCTION-BROWSE.md` §5.4）：默认 → 仅关左 → 关两侧 → 仅 view。
   *
   * 监听挂在window上而不是看图件里：这是**外壳**的事（左右两列与胶片带的显隐），
   * 看图件只负责照片本身的缩放/平移（职责分开，换渲染层时这里不用动）。
   */
  /*
   * 键盘（`specs/M2-W2-tail.md` 5.2）：「事件 → 意图」的映射在 `features/browse/keys.ts`
   * （纯函数、逐条有测），这里只把意图落到 store / viewer / 弹窗上。
   *
   * 冲突与分工：
   * * `←`/`→`、`Esc` 在看图里走看图命令；单张 view 的 `Enter` 内建退出，compare 的
   *   `Enter` 走命令切换胶片带范围；这里只在网格里执行 `move`；
   * * **数字打星只在网格里生效** —— 看图里的 `0` / `1` 是既有的缩放快捷键（适配 / 100%），
   *   不让打星覆盖它；看图时打标用 `P` / `X` / `U` 与工具条；
   * * `Delete` 永远要确认（人类 2026-09-19 的批注：删除能批量，不需要 easy destroy）。
   */
  /*
   * 键盘与「主要操作」的入口统一交给**命令注册表**（`specs/M2-W3.md` §2.5）：
   * 这里只把自己那份**动作**注册进 `features/browse/actions.ts`，
   * 命令（`mark.rating.3` / `nav.open` / `viewer.close` / `view.chrome.cycle` …）通过它取用。
   *
   * 为什么不再自己挂 window 监听：那样「改键」会落空 ——
   * 用户把「打 3 星」改到别的键，这里却仍然只认 `3`。
   *
   * 分工（与 W2 的键位表一致；意图表 `lib/viewer-keys.ts` 仍保留作参考与单测）：
   * * 看图里的 `Esc` / 方向键 / `+−01` → `components/ui/viewer/actions.ts`（看图件自己注册）；
   * * 网格里的数字打星、`P/X/U`、`Delete`、`Ctrl+A`、`←/→`、`i` → 命令；
   * * **内建**（不走注册表）：单张 view 的 `Enter` 返回、网格里的 roving focus、弹窗与菜单内部；
   *   compare 的 `Enter` 已登记为 `viewer.compareOnly` 命令。
   */
  onMount(() => {
    registerBrowseActions({
      viewing: () => viewer.state().active,
      comparing: viewing.comparing,
      filmVisible: viewing.filmVisible,
      requestDelete: () => setPendingDelete(store.selectedCount()),
      requestRestoreRecycle: () => void runRestoreRecycle(),
      requestEmptyRecycle: () => setPendingEmpty(true),
      requestPurgeRecycle: () => void runPurgeRecycle(),
      moveFocus,
      openViewer: viewing.requestOpen,
      cycleChrome: viewing.cycleChrome,
      resetChrome: viewing.resetChrome,
      toggleCompareStrip: viewing.toggleCompareStrip,
      fullscreenTarget,
      canExternalEditor:()=>store.anchorItem()!==null && !store.anchorItem()!.missing && issueLibrary()!==null && !issueBusy(),
      externalEditor:()=>{
        const item=store.anchorItem(),repo=store.repositoryId(),library=issueLibrary();
        if(!item||!repo||!library||item.missing||issueBusy())return;
        const choice=display.get(item.id);
        const latest=library.selection==='raw'&&!item.isRaw&&!item.hasRaw?'sooc':library.selection;
        props.onExternalEditor?.({...choice?.target,repositoryId:repo,reference:choice?.target.reference??displayedReference(item.id,'latest',latest),name:choice?.target.captured?.name??t('editor.issue.latest')});
      },
    });
    onCleanup(() => {
      // 诊断（生产也打）：注册/注销的时序 —— 命令与工具条按钮都靠这个槽（slot）
      console.debug("[browse] actions unregistered");
      registerBrowseActions(null);
    });
    console.debug("[browse] actions registered");
  });
  /** 移动「当前那张」（网格里 `←` / `→`）：等价于点它一下 —— 与点击同一套选择语义 */
  function moveFocus(delta: -1 | 1): void {
    const source = gridSource();
    const total = source.count();
    if (total === 0) return;
    const current = focusIndex();
    const next =
      current === undefined
        ? delta > 0
          ? 0
          : total - 1
        : Math.min(total - 1, Math.max(0, current + delta));
    const item = source.itemAt(next);
    if (item === null) {
      // 那一段还没取到（分页）：让它去取，并把焦点先挪过去，下一按就能落上
      void source.ensureRange?.(next, next + 1);
      setFocusIndex(next);
      return;
    }
    setFocusIndex(next);
    source.select(item.id, "replace");
    setFocusId(item.id);
  }

  /**
   * 写操作抛出来的错误必须**说出来**（2026-10-11 真机教训）：
   * 后端报「数据库错误：cannot start a transaction within a transaction」时，
   * 界面因为只 `void` 掉 Promise，看起来就是「点了没反应」——用户只能靠猜。
   * `store.error()` 的水印只在列表为空时出现，所以这里直接走 toast。
   */
  function reportFailure(error: unknown): void {
    props.toast?.show({
      tone: "danger",
      message: error instanceof Error ? error.message : String(error),
    });
  }

  /** 移入回收站的结果提示（按钮路径与拖动路径共用一份）。 */
  function reportRecycle(result: RecycleResult | null): void {
    if (result === null) return;
    if (result.moved > 0) {
      props.toast?.show({
        tone: "success",
        message: t("browse.deleteDone").replace("{n}", String(result.moved)),
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
    if (result.failed.length > 0) setDeleteFailures([...result.failed]);
  }

  /** 真删的结果提示（按钮路径与拖动路径共用一份）。 */
  function reportPurge(result: PurgeResult | null): void {
    if (result === null) return;
    if (result.purged > 0) {
      props.toast?.show({
        tone: "danger",
        message: t("browse.recyclePurgeDone").replace("{n}", String(result.purged)),
      });
    }
    if (result.failed.length > 0) setDeleteFailures([...result.failed]);
  }

  /** 删除选中的照片（按钮 / `Delete` 键）：移入库内回收站。 */
  async function runDelete(): Promise<void> {
    setPendingDelete(null);
    try {
      reportRecycle(await store.removeSelected());
    } catch (error) {
      reportFailure(error);
    }
  }

  /** 拖动删除（**按拖动的那批 ids**：拖未选中项时只删它）。 */
  async function runDeleteIds(ids: readonly string[]): Promise<void> {
    try {
      reportRecycle(await store.removeIds(ids));
    } catch (error) {
      reportFailure(error);
    }
  }

  /** 还原选中的照片：结果用提示说清楚（重名改名时多说一句）。 */
  async function runRestoreRecycle(): Promise<void> {
    try {
      const result = await store.restoreSelected();
      if (result === null) return;
      if (result.restored > 0) {
        props.toast?.show({
          tone: "success",
          message: t("browse.recycleRestoreDone").replace("{n}", String(result.restored)),
        });
      }
      if (result.renamed > 0) {
        props.toast?.show({ tone: "info", message: t("browse.recycleRestoreRenamed") });
      }
      if (result.failed.length > 0) setDeleteFailures([...result.failed]);
    } catch (error) {
      reportFailure(error);
    }
  }

  /** 真删选中的照片（命令/动作槽入口）。 */
  async function runPurgeRecycle(): Promise<void> {
    try {
      reportPurge(await store.purgeSelected());
    } catch (error) {
      reportFailure(error);
    }
  }

  /** 真删拖动的那批（回收站内拖动粉碎）。 */
  async function runPurgeRecycleIds(ids: readonly string[]): Promise<void> {
    try {
      reportPurge(await store.purgeIds(ids));
    } catch (error) {
      reportFailure(error);
    }
  }

  /** 拖到相片桶：把身份映射成 `AssetIdentity`（跨库来源用 entry 里的身份）。 */
  async function dropIdsIntoBucket(
    bucketId: number,
    name: string,
    ids: readonly string[],
  ): Promise<void> {
    const photos: AssetIdentity[] = ids.flatMap((value) => {
      if (organizationPhotos()) {
        const entry = organization.entryById(value);
        return entry ? [{ repositoryId: entry.repositoryId, assetId: entry.assetId }] : [];
      }
      const repositoryId = store.repositoryId();
      const assetId = Number(value);
      return repositoryId && Number.isSafeInteger(assetId) ? [{ repositoryId, assetId }] : [];
    });
    if (photos.length === 0) return;
    try {
      await addPhotosToBucket(bucketId, photos);
      props.onOrganizationChanged?.();
      props.toast?.show({
        tone: "success",
        message: t("org.addedCount").replace("{n}", String(photos.length)),
      });
      void name;
    } catch (error) {
      store.reportError(error);
    }
  }

  /** 清空回收站（永久；强确认之后才走到这里）。 */
  async function runEmptyRecycle(): Promise<void> {
    setPendingEmpty(false);
    try {
      const result = await store.emptyRecycle();
      if (result === null) return;
      if (result.purged > 0) {
        props.toast?.show({
          tone: "success",
          message: t("browse.recycleEmptyDone").replace("{n}", String(result.purged)),
        });
      }
      if (result.failed.length > 0) setDeleteFailures([...result.failed]);
    } catch (error) {
      reportFailure(error);
    }
  }


  /*
   * 进浏览时把当前目录重读一遍（人类 2026-09-22 报的「导入完回浏览看不到新照片」）。
   *
   * 症状：先进浏览、再去导入、回浏览 —— 新导入的照片不在列表里，得重启程序才看得到。
   * 根因：列表只在**查询变化**时重新加载（`setRepository` / `setScope` / 筛选 / 排序），
   * 而「导入往里加了东西」不改变查询 —— 于是那份清单一直停在上一次读的样子。
   *
   * 为什么直接重读、不先做个「库变了吗」的判定（人类 2026-09-22 问过这笔账）：
   * 重读的代价就是**再付一次「进目录首屏」的钱** —— 而浏览的范围是**一个目录**
   *（真实库就是几百到几千张，时间线亚毫秒级）。就算拿 10 万张挤在一个目录里的
   *最坏情况考它（`cargo run --release -p raybend --example query-bench`）：
   *取页 0.5ms + 时间线 P95 142ms + 分面 P95 79ms，两百多毫秒、一次。
   * 换来的是「看到的就是库里的」这条硬口径（`AGENTS.md` §2.13），而代价是
   * 一个每次都要维护的「什么算更新」（updated_at / 版本号）——那个机制一旦漏埋一处，
   * 就变成「偶尔不更新」这种最难查的 bug。所以：**宁可多读一遍，不建第二套真相**。
   */
  onMount(() => {
    void store.refresh();
  });

  onMount(() => { void props.repositories.load().then(() => {
    const list = repositories();
    if (store.repositoryId() === null && list.length > 0) {
      const preferred = list.find(r => r.online && r.lastOpenedAt !== null) ?? list.find(r => r.online) ?? list[0];
      store.setRepository(preferred.id);
    }
  }); });
  const refreshRepositories = props.repositories.load;
  const remountLibrary = props.repositories.remount;

  /** 库根目录（库内相对路径 → 绝对路径） */
  const root = createMemo(
    () => {
      const id = store.repositoryId(); repositories();
      return id === null ? null : props.repositories.lastVerifiedRoot(id);
    },
  );
  createEffect(() => {
    const item = anchor();
    const base = root();
    const path = item !== null && base !== null ? joinPath(base, item.relPath) : null;
    /*
     * 尺寸 + 文件类型 = **当前显示变体**的（`lib/display-size.ts` 是唯一读口）。
     * 读 `display.get` 与 `issueLibrary()` 就在这里建立响应式依赖 —— 切变体 / 定稿改名删除
     * 都会重发一次 select，flowinfo 的「尺寸 / MP / 文件类型」跟着变
     * （`specs/display-size-source.md` §6.1）。
     *
     * 尺寸优先用变体装载时那份 snapshot（切变体时毫秒级拿到，与列表项**同源**
     * —— 都是 Rust `store::sizes::display_size` 的答案）：文件行刚被懒回填过时，
     * 列表项还是载入时那份旧值，而 snapshot 是新的。默认变体（latest）没有 snapshot，
     * 走列表项（后端读侧已按当前栈现算过）。
     */
    const choice = item === null ? undefined : display.get(item.id)?.choice;
    const issue = item === null ? undefined : issueOf(issueLibrary(), choice);
    const snapped = item === null ? undefined : display.get(item.id)?.natural;
    const size = snapped ?? (item === null ? null : displaySizeOf(item, choice, issue));
    props.selectedMetadata.select(
      path,
      item === null ? null : assetItemExif(item),
      item === null || size === null
        ? null
        : { width: size.width, height: size.height, base: sourceBaseOf(item, choice, issue) },
    );
  });

  /**
   * 网格数据源（适配器）：把浏览 store 包成网格契约。
   *
   * 依赖里带上 `thumbs`（与胶片带共用那一条队列）与档位三个回调（受控）——
   * 网格自己不持有这些状态。
   *
   * ⚠️ 它**必须排在 `root` 之后**：`createMemo` 会立刻求值一次，
   * 而 `root()` 在它声明之前访问会触发 TDZ（真机冒烟实测：
   * `ReferenceError: Cannot access 'root' before initialization`，
   * 表现为「点了『浏览』状态切了但界面不动」）。
   */
  const gridSource = createMemo(() => organizationPhotos() ? organization.source :
    display.adapt(browseSource({
      store,
      // 传取值函数（不是值）：库列表是异步来的，见 `BrowseSourceDeps.root` 的说明
      root,
      thumbs,
      tileStep,
      setTileStep,
      // 档位落盘：拖拽结束时写一次（与导入侧同一份实现、不同作用域）
      commitTileStep: () => commitBrowseDisplayTileStep(),
      grouped,
      infoMode: browseInfoMode,
    })),
  );

  /** 拖动中的照片 id（网格半透明表现；`gridSource().selection().ids` 是字符串口径）。 */
  const draggingIds = createMemo(() => new Set(photoDrag()?.ids ?? []));
  createEffect(on(organizationSelection, () => {
    if (viewer.state().active) viewer.close();
  }, { defer: true }));

  // Page-cache refreshes can temporarily contain holes. Only a display-choice change
  // replaces viewer images; ordinary paging must preserve the active viewer.
  createEffect(on(display.choices,()=>viewer.syncPhotos(photosFromSource(gridSource())),{defer:true}));

  onMount(() => {
    let disposed = false;
    let off: (() => void) | undefined;
    void onCatalogChanged((change) => {
      if (change.repositoryId !== store.repositoryId()) return;
      if(change.assetIds.includes(store.anchorItem()?.id ?? -1))setIssueRefreshTick(value=>value+1);
      for (const path of change.paths) if (thumbs.get(path).status !== "idle") thumbs.refresh(path);
      viewer.invalidatePaths(change.paths);
      void refreshRepositories();
    }).then((unsubscribe) => { if (disposed) unsubscribe(); else off = unsubscribe; });
    onCleanup(() => { disposed = true; off?.(); });
  });

  /**
   * 右栏（与 flowbar 的 flowinfo）显示谁：多选时是**锚点**那张（`memory/FUNCTION-BROWSE.md` §5.10）。
   *
   * 规则本身住在 store 的 `anchorItem()` 里 —— 组装层读的是**同一个方法**，
   * 所以「右栏显示谁」与「flowinfo 显示谁」不可能两边走偏。
   */
  const info = createMemo<BrowseInfoSnapshot | null>((previous) => {
    const item = anchor(), base = root();
    return retainBrowseInfo(previous, {
      context: JSON.stringify([store.repositoryId(), store.scopePath()]),
      item, path: item && base ? joinPath(base, item.relPath) : null,
      metadata: props.selectedMetadata.settled(),
      issuesReady: issueResolvedContext() === issueContext(),
      library: issueLibrary(), error: issueError(),
    });
  }, null);
  const infoPending = () => info()?.item.id !== anchor()?.id ||
    props.selectedMetadata.settled() === null || issueResolvedContext() !== issueContext();

  /**
   * 全屏看图要的清单（flowbar 那个全屏按钮 + `viewer.fullscreen` 命令）。
   *
   * 显示序用的是**与网格同一个函数、同一个数据源**（`photosFromSource(gridSource())`）——
   * 所以全屏里的 ←/→ 与网格里的邻居逐张一致，筛选 / 排序也一致；
   * 这里不另建一份清单（§2.12）。
   */
  const fullscreenTarget = createMemo(() =>
    buildFullscreenTarget(photosFromSource(gridSource()), gridSource().selection().anchor),
  );

  /**
   * 状态条中间那段的**左侧**：`库名 / 最后一级目录`（`memory/FUNCTION-BROWSE.md` §5.10）。
   *
   * 文件名**不在这里拼**：那条状态条是两侧共用的组件，它自己会在后面接上
   * 「· 当前那张的文件名」（人类 2026-09-19：导入侧以前漏了这一段，统一时一起补）。
   */
  const currentLead = createMemo(() => {
    const repoName =
      repositories().find((r) => r.id === store.repositoryId())?.name ?? "";
    const scope = store.scopePath();
    const last = scope === null ? "" : (scope.split("/").pop() ?? "");
    return [repoName, last].filter((s) => s !== "").join(" / ");
  });

  /**
   * 看图态那条状态栏的内容（人类 2026-09-20：与 tiles **同一条**，只换内容）。
   *
   * 字段来自看图件当前那张（`ViewerPhoto.marks` / `flag`）—— 调用方本来就有，
   * 不为了显示四个数再问一次后端。
   */
  const viewingInfo = (): TilesViewingInfo => viewingInfoOf(viewer.current());

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
            // 看图第 ② 档起左列**藏起来但不卸载**：卸载会把目录树的展开状态与滚动位置清掉，
            // 按一下 Tab 就白跑一趟（而且回来要重新读盘）。
            viewing.showsLeft() ? "" : "hidden",
          ]
            .filter(Boolean)
            .join(" ")}
          style={{ width: `${leftWidth()}px` }}
        >
          <Show when={props.organizationPanel === "library"} fallback={
            <OrganizationPanel
              mode={props.organizationPanel === "tags" ? "tags" : "buckets"}
              refreshKey={props.organizationRevision}
              repositories={repositories()}
              selection={organizationSelection()}
              onSelectTag={(tag) => setOrganizationSelection({ kind: "tag", key: tag.name.normalize("NFC").toLowerCase() })}
              onSelectBucket={(bucket) => setOrganizationSelection({ kind: "bucket", id: bucket.id })}
              onSelectDirectory={(repositoryId, path) => {
                setOrganizationSelection({ kind: "directory", repositoryId, path });
                store.setRepository(repositoryId);
                store.setScope(path);
              }}
              onNewBucket={props.onNewBucket}
              onEditBucket={props.onEditBucket}
              onBucketDeleted={(bucket) => {
                // 删的正是当前在看的桶：清选中回退库目录浏览，并明确提示（规格 §9）
                setOrganizationSelection((current) => current?.kind === "bucket" && current.id === bucket.id ? null : current);
                props.toast?.show({ tone: "info", message: t("org.bucketDeletedFallback").replace("{name}", bucket.name) });
              }}
              onError={(error) => store.reportError(error)}
              onChanged={props.onOrganizationChanged}
            />
          }>
          <BrowseLeftColumn
            store={store}
            onSelectDirectory={(repositoryId, path) => setOrganizationSelection({ kind: "directory", repositoryId, path })}
            repositories={repositories()}
            reposLoading={reposLoading()}
            reposError={reposError()}
            libsExpanded={libsExpanded()}
            onExpandLibs={() => setLibsExpanded(true)}
            onCollapseLibs={() => setLibsExpanded(false)}
            onOpenSettings={(id) => props.onOpenLibrarySettings?.(id)}
            onRemount={(id) => void remountLibrary(id)}
            isRemounting={props.repositories.isRemounting}
          />
          </Show>
        </aside>

        {/* 拖拽手柄：左列 ↔ 中列（看图第 ② 档起与左列一起收起来） */}
        <Show when={viewing.showsLeft()}>
          <ColumnHandle
            onDragStart={beginLeftResize}
            onDrag={dragLeft}
            onDragEnd={endLeftResize}
            onNudge={nudgeLeft}
          />
        </Show>

      {/* 中列 */}
      <main
        class={[
          "relative flex min-h-0 min-w-0 flex-1 flex-col",
          // 回收站语境：tiles 底 = 低浓辅色（`specs/library-recycle-bin.md` §4.2 的 `--state-recycle`）
          store.recycleMode() ? "bg-state-recycle" : "bg-surface-bar",
        ].join(" ")}
        /*
         * 四态在 DOM 上留痕：左右两列的显隐已经生效（就是那两个 `hidden`），
         * **胶片带那一条要等 2.1 才落地** —— 但状态本身现在就可观测，
         * 冒烟断言与以后接胶片带都读这两个属性，不用另加一个全局状态。
         */
        data-chrome={viewing.chrome()}
        data-film={viewing.filmVisible() ? "on" : "off"}
      >
        {/** 筛选结果区（chips + 共 N 张 + 任一/全部）：看图时不占位置 */}
        <Show when={!viewer.state().active && !store.recycleMode()}>
          <FilterBar store={store} count={organizationPhotos() ? organization.source.count() : undefined} />
        </Show>

        {/*
          拖动热区（`specs/library-recycle-bin.md` §5.1）：拖动开始才出现，浮在网格左上。
          photos 侧 = 移入回收站；回收站侧 = 永久删除（粉碎）。命中判定用它的矩形。
        */}
        <Show when={photoDrag() !== null && !organizationPhotos()}>
          <div
            data-photo-drop-zone
            class={[
              // 正方块、无文字（崔总 2026-10-11）：命中态只靠外轮廓加一圈
              "absolute left-3 top-3 z-20 grid size-14 place-items-center rounded-heat bg-heat-surface select-none",
              // 凸面（内高光 + 内暗影）+ 外圈（浅色主题浅阴影 / 深色主题浅发光）
              "shadow-[inset_0_1px_0_var(--heat-light),inset_0_-1px_1px_var(--heat-shade),0_2px_10px_var(--heat-outer)]",
              // 命中（松手可生效）：深色外轮廓 —— 底本身就是辅色，再闪辅色看不出来；
              // 回收站里拖到它 = 真删，用危险色。
              photoDrag()?.target?.kind === "recycle"
                ? store.recycleMode()
                  ? "outline outline-2 outline-offset-1 outline-danger"
                  : "outline outline-2 outline-offset-1 outline-fg-1"
                : "",
            ].join(" ")}
          >
            <IconTrash
              size={26}
              class={[
                // 底是辅色 ⇒ 图标用品牌色上的前景色；回收站里拖到它 = 真删，用危险色
                photoDrag()?.target?.kind === "recycle" && store.recycleMode()
                  ? "text-danger"
                  : "text-fg-on-brand",
              ].join(" ")}
            />
          </div>
        </Show>

        {/**
         * 照片区：网格 + 看图盖层；看图时它下面接胶片带（`design/browse.md` §2.5）。
         *
         * `flex flex-col` 不能省：网格自己的根节点靠 `flex-1` 撑高，
         * 外面换成普通块级盒子的话它会直接塌成 0 高（虚拟列表一格都不渲染）。
         */}
        <PhotoViewingStage
          source={gridSource()}
          controller={viewing}
          thumbs={thumbs}
          filmStripStep={props.filmStripStep}
          onFilmStripStepChange={props.onFilmStripStepChange}
          tilesBar={{
            count: gridSource().count(),
            selectedCount: gridSource().selection().ids.size,
            label: organizationPhotos() ? (organizationSelection()?.kind === "tag" ? t("org.tags") : t("org.buckets")) : currentLead(),
            fileName: organizationPhotos() ? organization.itemById(gridSource().selection().anchor ?? "")?.fileName ?? null : anchor()?.fileName ?? null,
            byTime: organizationPhotos() ? false : grouped(),
            onByTimeChange: (value) => setGrouped(value),
            infoMode: browseInfoMode(),
            onInfoToggle: cycleBrowseTileInfo,
            tileStep: tileStep(),
            onTileStepChange: setTileStep,
            onTileStepCommit: () => commitBrowseDisplayTileStep(),
            sort: organizationPhotos() || store.recycleMode() ? undefined : {
              keys: Object.keys(SORT_LABELS) as NonNullable<BrowseSort["key"]>[],
              value: store.sort().key ?? "takenAt",
              labelOf: (key) => SORT_LABELS[key as NonNullable<BrowseSort["key"]>](),
              desc: store.sort().desc === true,
              onKeyChange: (key) =>
                store.setSort({ ...store.sort(), key: key as NonNullable<BrowseSort["key"]> }),
              onDirectionToggle: () =>
                store.setSort({ ...store.sort(), desc: !store.sort().desc }),
            },
          }}
          viewingInfo={viewingInfo()}
          focusId={focusId()}
          pinsKey={organizationPhotos() ? organization.source.scopeKey() : `${store.repositoryId() ?? ""}:${store.scopePath() ?? ""}:${store.filterMode() ? "on" : "off"}:${JSON.stringify(store.filter())}`}
          onInteract={() => setLibsExpanded(false)}
          onFocusIndex={(index) => { setFocusIndex(index); setFocusId(undefined); }}
          draggingIds={draggingIds}
          onPhotoPointerDown={(id, event) => {
            const ids = dragIdsFor(id, gridSource().selection().ids);
            beginPhotoDrag(event, {
              ids,
              onRecycle: () => {
                // 诊断（生产也打）：拖动链路的入口 —— 看到这一行说明热区命中与松手都对
                console.debug("[recycle] drop on heat zone", {
                  ids,
                  recycle: store.recycleMode(),
                });
                // 热区只在单库来源下渲染；回收站里拖到它 = 真删（粉碎），照片侧 = 移入回收站
                if (store.recycleMode()) void runPurgeRecycleIds(ids);
                else void runDeleteIds(ids);
              },
              onBucket: (bucketId, name) => void dropIdsIntoBucket(bucketId, name, ids),
            });
          }}
          watermark={() => gridWatermark()}
        />
      </main>

      {/* 拖动预览：**与 export 同一个组件**（`components/ui/DragPreview.tsx`） */}
      <Show when={photoDrag()}>
        {(drag) => (
          <DragPreview
            x={drag().x}
            y={drag().y}
            count={drag().ids.length}
            items={drag().ids.slice(0, 4).map((photoId) => {
              const entry = gridSource().itemById(photoId);
              if (entry === null) return {};
              const thumb = thumbs.get(entry.path);
              if (thumb.status === "idle") thumbs.request(entry.path);
              return { url: () => thumbs.get(entry.path).url };
            })}
          />
        )}
      </Show>

      {/* 右列**没有把手**：宽度固定（人类 2026-09-19 定「把手只加左边界」） */}

      {/* 右列（宽度 = `--panel-w-right`，所有工作流通用；不支持拖拽） */}
      <aside
        class={[
          "w-panel-w-right flex shrink-0 flex-col bg-surface-main",
          viewing.showsRight() ? "" : "hidden",
        ]
          .filter(Boolean)
          .join(" ")}
      >
        {/*
          右栏在看图态换成**预览 + 直方图**（`memory/FUNCTION-BROWSE.md` §5.9）——
          看图件的 store 本身就是「当前看哪张 + 看到哪一块」的唯一事实来源，直接传进去。
        */}
        <Show when={!organizationPhotos() || organizationInfoAligned()} fallback={<div class="p-panel-pad text-fs-1 text-fg-2">
          <p class="font-semibold text-fg-1">{organization.itemById(gridSource().selection().anchor ?? "")?.fileName ?? t("org.selectPhoto")}</p>
          <p>{repositories().find((repo) => repo.id === organization.entryById(gridSource().selection().anchor ?? "")?.repositoryId)?.name ?? ""}</p>
        </div>}><AssetInfo
              store={store}
              item={info()?.item ?? null}
              fileExif={info()?.file ?? null}
              pending={infoPending()}
              viewer={viewer.state().active ? viewer : null}
              /* 对比态不画视野框：好几个窗口，一个框描述不了 */
              comparing={viewing.comparing()}
              /* 「所属库」那一行：名字住在工作区（它拿着库列表） */
              displayedIssueChoice={display.get(info()?.item.id ?? -1)?.choice ?? "latest"}
              displayHistogram={display.get(info()?.item.id ?? -1)?.histogram}
              issueLibrary={info()?.library ?? null}
              issueBusy={issueBusy()}
              issueError={info()?.error ?? null}
              onSelectIssue={(choice) => void selectBrowseIssue(choice)}
              repositoryName={
                repositories().find((repo) => repo.id === store.repositoryId())?.name ?? null
              }
            /></Show>
      </aside>
      </div>

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

      {/* 清空回收站：**永久删除**，强确认（`specs/library-recycle-bin.md` §1） */}
      <ConfirmDialog
        open={pendingEmpty()}
        title={t("browse.recycleEmptyTitle")}
        message={t("browse.recycleEmptyConfirm").replace("{n}", String(store.recycleSummary().count))}
        onConfirm={() => void runEmptyRecycle()}
        onCancel={() => setPendingEmpty(false)}
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
