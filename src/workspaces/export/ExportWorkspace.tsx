/** 导出组装层：共用库目录列、PhotoGrid、Tile 和双 TilesShell；队列状态由 App 持有。 */
import {
  createEffect,
  createMemo,
  createSignal,
  For,
  onCleanup,
  onMount,
  on,
  Show,
  untrack,
} from "solid-js";
import {
  IconFolderOpen,
  IconList,
  IconPhoto,
  IconUpload, IconPlayerSkipForward, IconClock, IconLoader2, IconCheck, IconAlertTriangle,
} from "@tabler/icons-solidjs";
import {
  BrowseLeftColumn,
  type BrowseStore,
} from "../../features/browse/index.ts";
import { browseSource } from "../../features/browse/grid-source.ts";
import { PhotoGrid } from "../../features/photo-grid/PhotoGrid.tsx";
import { TilesShell } from "../../components/ui/tiles/TilesShell.tsx";
import { Tile } from "../../components/ui/Tile.tsx";
import { Button, IconButton } from "../../components/ui/Button.tsx";
import { Input, RadioChoices } from "../../components/ui/Form.tsx";
import { Slider } from "../../components/ui/Slider.tsx";
import { SegmentedControl } from "../../components/ui/SegmentedControl.tsx";
import { SplitStack } from "../../components/ui/SplitStack.tsx";
import { SplitHandle } from "../../components/ui/SplitHandle.tsx";
import { ConfirmDialog, Dialog } from "../../components/ui/Dialog.tsx";
import { EasyDestroyButton } from "../../components/ui/EasyDestroy.tsx";
import { StateWatermark } from "../../components/ui/StateWatermark.tsx";
import { createThumbQueue } from "../../components/ui/thumb-queue.ts";
import {
  getThumbBytes,
  onCatalogChanged,
} from "../../api/db.ts";
import { getVariantThumb } from "../../api/export.ts";
import { openFullscreen } from "../../api/fullscreen.ts";
import { pickDirectory } from "../../api/dialog.ts";
import type { RepositoryStateStore } from "../../features/repositories/state.ts";
import type { SelectedFileMetadata } from "../../features/exif-strip/index.ts";
import { nudgeWidth, resizeWidth } from "../../lib/column-resize.ts";
import { LAYOUT_BOUNDS } from "../../lib/layout-prefs.ts";
import { clickMode } from "../../lib/selection.ts";
import { joinPath } from "../../lib/paths.ts";
import {
  variantKey,
  exportGalleryState, mainVariant, variantSuffix,
  formatSupportsQuality, EXPORT_FORMATS, EXISTING_FILE_POLICIES,
  type VariantSummary,
} from "../../lib/export-model.ts";
import { t } from "../../i18n/index.ts";
import { Portal } from "solid-js/web";
import { trackPointerDrag } from "../../lib/pointer-drag.ts";
import { animateCount } from "../../lib/count-transition.ts";
import { registerExportActions } from "./actions.ts";
import {
  exportGallerySource,
  exportQueueSource,
  queueImageKey,
} from "./source.ts";
import { EXPORT_TOP_SIZE_BOUNDS } from "../../lib/export-prefs.ts";
import { tileSizeAt } from "../../lib/tile-flow.ts";
import type { ExportStore } from "./store.ts";

export interface ExportWorkspaceProps {
  store: ExportStore;
  repositories: RepositoryStateStore;
  browse: BrowseStore;
  selectedMetadata: SelectedFileMetadata;
  leftWidth: number;
  onLeftWidthChange(width: number): void;
  onOpenLibrarySettings(id: string): void;
}
export function ExportWorkspace(props: ExportWorkspaceProps) {
  const store = props.store;
  const prefs = store.preferences;
  const repositories = props.repositories.list;
  const reposLoading = () => props.repositories.status() === "loading" || props.repositories.status() === "idle";
  const reposError = props.repositories.error;
  const [libsExpanded, setLibsExpanded] = createSignal(false);
  const [leftWidth, setLeftWidth] = createSignal(props.leftWidth);
  let dragStart = 0;
  const [resetOpen, setResetOpen] = createSignal(false);
  const [allIssues, setAllIssues] = createSignal<number|null>(null);
  const root = () =>
    repositories().find((r) => r.id === props.browse.repositoryId())?.root ??
    props.repositories.lastVerifiedRoot(props.browse.repositoryId() ?? "");
  const photoThumbs = createThumbQueue({
    load: (path) => getThumbBytes(path, "grid"),
  });
  // variant 图与队列图共用一个限流/LRU 队列，队列取入队快照。
  const issueThumbs = createThumbQueue({
    load: async (key) => {
      if (!key.startsWith("[")) return getThumbBytes(key, "grid");
      // 键是自己 JSON.stringify 出来的；解析不了属于内部错误，抛给队列的 error 态。
      let parsed: unknown;
      try { parsed = JSON.parse(key); }
      catch { throw new Error(`坏掉的定稿图键：${key}`); } // i18n-exempt: 队列内部诊断，不上界面
      const [repo, reference, hash] = parsed as [
        string,
        { assetId: number; variant: string },
        string | null,
      ];
      const captured = [...store.queues().values()]
        .flat()
        .find((item) => queueImageKey(item) === key)?.snapshot;
      return getVariantThumb(
        repo,
        reference,
        "grid",
        captured?.profileHash === hash ? captured : undefined,
      );
    },
  });
  const base = browseSource({
    store: props.browse,
    root,
    thumbs: photoThumbs,
    tileStep: () => prefs.value().topStep,
    setTileStep: (topStep) => prefs.update({ topStep }, false),
    commitTileStep: () => prefs.commit(),
    grouped: () => prefs.value().grouped,
    infoMode: () => prefs.value().info,
  });
  untrack(() => store.context(props.browse.repositoryId(), props.browse.scopePath() ?? ""));
  createEffect(on(() => [props.browse.repositoryId(), props.browse.scopePath()] as const, ([repo, scope]) => {
    store.context(repo, scope ?? "");
    photoThumbs.clear();
    issueThumbs.clear();
  }, {defer:true}));
  const gallery = exportGallerySource(base, store, issueThumbs);
  const assets = createMemo(() =>
    Array.from({ length: gallery.count() }, (_, i) =>
      Number(gallery.idAt?.(i)),
    ).filter((id) => id > 0),
  );
  createEffect(on(() => {
    const id = props.browse.repositoryId();
    const repository = id === null ? undefined : props.repositories.byId(id);
    return repository?.online ? `${id}:${repository.connection?.generation ?? "0"}` : null;
  }, (session, previous) => {
    if (session === null || session === previous) return;
    // 离线保留现有内容；恢复后重读当前代次，不卸载网格。
    untrack(() => {
      const dir = root();
      for (const id of assets()) {
        const asset = props.browse.itemById(id);
        if (dir && asset) {
          const path = joinPath(dir, asset.relPath);
          if (photoThumbs.get(path).status !== "idle") photoThumbs.refresh(path);
        }
        for (const variant of store.listFor(id)) {
          const key = JSON.stringify([store.repository(), variant.reference, variant.profileHash]);
          if (issueThumbs.get(key).status !== "idle") issueThumbs.refresh(key);
        }
      }
      void store.invalidate(assets());
    });
  }, {defer:true}));
  const queue = exportQueueSource(store, issueThumbs);
  const currentIssue = createMemo(
    () =>
      store
        .refsFor(assets())
        .find(
          (v) =>
            variantKey(store.repository() ?? "", v.reference) ===
            store.selection().anchor,
        ) ?? null,
  );
  const queueCurrent = () => store.queueItems().find(item=>item.id===store.queueSelection().anchor) ?? null;
  const fullscreenTarget = () => {
    const current = currentIssue(), item=queueCurrent();
    if(store.activeArea()==="queue")return item===null?null:{items:[{
      id:item.id,path:joinPath(item.root,item.snapshot.relPath),fileName:item.snapshot.name,
      exportVariant:{repositoryId:item.repositoryId,reference:item.snapshot.reference,captured:item.snapshot},
    }],index:0};
    const repo=store.repository(),dir=root();
    if(current===null||repo===null||dir===null)return null;
    return {items:[{id:variantKey(repo,current.reference),path:joinPath(dir,current.relPath),fileName:current.name,
      exportVariant:{repositoryId:repo,reference:current.reference}}],index:0};
  };
  const showIssue = (variant: VariantSummary) => {
    const repo = store.repository(),
      dir = root();
    if (repo === null || dir === null) return;
    void openFullscreen(
      [
        {
          id: variantKey(repo, variant.reference),
          path: joinPath(dir, variant.relPath),
          fileName: variant.name,
          exportVariant: { repositoryId: repo, reference: variant.reference },
        },
      ],
      0,
    ).catch(store.reportError);
  };
  const refreshRepos = props.repositories.load;
  createEffect(() => {
    const v = currentIssue(), dir = root(), item=queueCurrent();
    const path = store.activeArea()==="queue" ? (item===null?null:joinPath(item.root,item.snapshot.relPath)) : (v===null||dir===null?null:joinPath(dir,v.relPath));
    if (untrack(props.selectedMetadata.path) !== path) props.selectedMetadata.select(path);
  });
  onMount(() => {
    store.syncRuntime();
    const focus=()=>store.syncRuntime();window.addEventListener("focus",focus);onCleanup(()=>window.removeEventListener("focus",focus));
    void refreshRepos();
    store.invalidate([...store.variants().keys()]);
    void props.browse.refresh().catch(store.reportError);
    registerExportActions({
      viewing: () => false,
      filmVisible: () => false,
      fullscreenTarget,
      enqueue: () => {
        if (store.organizationHandoff().length > 0) { void store.enqueueOrganizationHandoff(); return; }
        const dir = root();
        if (dir !== null) void store.enqueue(dir);
      },
      selectAll: () =>
        store.selectAllActive(allIssues()===null?assets():[allIssues()!]),
      requestReset: () => setResetOpen(true),
      remove: store.removeSelected,
    });
    let dead = false;
    let off: (() => void) | undefined;
    void onCatalogChanged((change) => {
      if (change.repositoryId !== store.repository()) return;
      if(change.assetIds.length > 0) void store.invalidate(change.assetIds);
      for (const path of change.paths)
        if (photoThumbs.get(path).status !== "idle") photoThumbs.refresh(path);
      // File changes may keep the profile hash. Refresh only affected displayed
      // images, retaining the old decoded image until its replacement is ready.
      const paths = new Set(change.paths);
      const dir = root();
      if (dir) for (const id of change.assetIds) for (const v of store.listFor(id)) {
        const key = JSON.stringify([store.repository(),v.reference,v.profileHash]);
        if (paths.has(joinPath(dir,v.relPath)) && issueThumbs.get(key).status!=="idle") issueThumbs.refresh(key);
      }
      void refreshRepos();
    }).then((unsub) => {
      if (dead) unsub();
      else off = unsub;
    });
    onCleanup(() => {
      dead = true;
      off?.();
    });
  });
  onCleanup(() => {
    registerExportActions(null);
    photoThumbs.clear();
    issueThumbs.clear();
    props.selectedMetadata.select(null);
  });
  let checkTimer: ReturnType<typeof setTimeout> | undefined;
  createEffect(() => {
    const candidate = store.draft();
    clearTimeout(checkTimer);
    if (!candidate.name || !candidate.directory) return;
    checkTimer = setTimeout(
      () => void store.check(candidate).catch(store.reportError),
      250,
    );
  });
  onCleanup(() => clearTimeout(checkTimer));
  const cycleInfo = () => {
    const info = ["off", "marks", "marks-name"] as const;
    prefs.update({ info: info[(info.indexOf(prefs.value().info) + 1) % 3] });
  };
  const queueCenter = (
      <span class="flex min-w-0 items-center gap-2">
        <span class="truncate rounded-ui bg-state-selected px-2 text-fs-1 text-fg-1">
          {store.selectedPreset()?.name ?? t("export.pickPreset")}
        </span>
        <IconButton
          label={t("export.listMode")}
          selected={prefs.value().queueList}
          onClick={() => prefs.update({ queueList: !prefs.value().queueList })}
        >
          <IconList size={14} />
        </IconButton>
      </span>
  );
  const bar = (bottom = false) => ({
    count: bottom ? queue.count() : gallery.count(),
    countLabel: bottom
      ? t("export.queueCount", {
          remaining: store.progress(store.selectedPreset()?.id ?? "").remaining,
          total: queue.count(),
        })
      : store.organizationHandoff().length > 0
        ? t("export.handoffCount").replace("{n}", String(store.organizationHandoff().length))
        : undefined,
    selectedCount: bottom ? store.queueSelection().ids.size : store.organizationHandoff().length > 0 ? 0 : store.selection().ids.size,
    byTime: prefs.value().grouped,
    onByTimeChange: (grouped: boolean) => prefs.update({ grouped }),
    infoMode: prefs.value().info,
    onInfoToggle: cycleInfo,
    tileStep: bottom ? prefs.value().queueStep : prefs.value().topStep,
    onTileStepChange: (step: number) =>
      prefs.update(bottom ? { queueStep: step } : { topStep: step }, false),
    onTileStepCommit: () => prefs.commit(),
    compactControls: bottom,
    centerContent: bottom ? queueCenter : undefined,
    label: bottom
      ? (store.selectedPreset()?.name ?? t("export.pickPreset"))
      : props.browse.scopePath(),
  });
  const galleryEmpty = () => {
    const state = exportGalleryState({repository: store.repository(),scope: props.browse.scopePath(),count: gallery.status()==="loading" || (reposLoading() && root()===null) ? 0 : gallery.count(),loading: props.browse.loading() || gallery.status()==="loading" || reposLoading(),error: store.error() ?? props.browse.error()});
    if (state === null) return null;
    return <StateWatermark
      animate={state === "loading"}
      icon={<IconPhoto size={64} />}
      tone={state === "error" ? "error" : "muted"}
      text={state === "repository" ? t("browse.noRepository") : state === "directory" ? t("browse.pickDirectory") : state === "loading" ? t("browse.loading") : state === "error" ? (store.error() ?? props.browse.error() ?? "") : t("export.noIssues")}
    />;
  };
  /**
   * 拖动落点：预设卡片（`kind: "preset"`）或下方队列区（`kind: "queue"`）。
   *
   * 队列区**只在有选中预设时**才接得住 —— 没有预设就没有地方可放，
   * 这与 toolsbar 上「送入队列」按钮的启用条件同一门槛（不亮高亮、不响应松手）。
   */
  type DragTarget = {kind:"preset";id:string}|{kind:"queue"}|null;
  type DragBatch = {variants:readonly VariantSummary[];repository:string;x:number;y:number;target:DragTarget};
  const [dragBatch,setDragBatch]=createSignal<DragBatch|null>(null);
  const [countDrops,setCountDrops]=createSignal(new Map<string,{from:number;to?:number}>());
  let cancelDrag:(()=>void)|undefined;
  let clickTimer:ReturnType<typeof setTimeout>|undefined;
  const blockClick=(event:MouseEvent)=>{event.preventDefault();event.stopImmediatePropagation();};
  const clearDragClick=()=>{clearTimeout(clickTimer);window.removeEventListener("click",blockClick,true);};
  const hitTarget=(point:{x:number;y:number}):DragTarget=>{
    const hit=document.elementFromPoint(point.x,point.y);
    const preset=hit?.closest<HTMLElement>("[data-export-preset]")?.dataset.exportPreset;
    if(preset)return {kind:"preset",id:preset};
    if(hit?.closest("[data-export-area=queue]") && store.selectedPreset()!==null)return {kind:"queue"};
    return null;
  };
  const presetTarget=(id:string)=>{const target=dragBatch()?.target;return target?.kind==="preset" && target.id===id;};
  const queueTarget=()=>dragBatch()?.target?.kind==="queue";
  async function dropIntoPreset(batch:DragBatch,id:string):Promise<void>{
    const dir=root();if(!dir || batch.repository!==store.repository())return;
    const request={from:store.progress(id).total};
    setCountDrops(old=>new Map(old).set(id,request));
    const added=await store.enqueue(dir,{presetId:id,references:batch.variants.map(v=>v.reference),preserveSelection:true});
    setCountDrops(old=>{const next=new Map(old);if(old.get(id)!==request)return old;
      if(added>0)next.set(id,{from:request.from,to:store.progress(id).total});else next.delete(id);return next;});
  }
  /** 拖到下方队列区：与 toolsbar 的「送入队列」同一个落点 —— 当前选中的预设。 */
  function dropIntoQueue(batch:DragBatch):void{
    const id=store.selectedPreset()?.id;
    if(id!==undefined)void dropIntoPreset(batch,id);
  }
  function beginIssueDrag(event:PointerEvent):void{
    if(event.button!==0 || event.ctrlKey || event.metaKey || event.shiftKey || store.busy())return;
    const target=event.target as HTMLElement;
    if(target.closest("button,input,select,[role=slider]"))return;
    const small=target.closest<HTMLElement>("[data-export-issue]");
    const cell=target.closest<HTMLElement>("[data-grid-item]");
    const variant=small ? store.selectedVariants().find(v=>v.reference.variant===small.dataset.exportIssue && small.closest('[data-grid-item]')?.getAttribute('data-grid-item')===String(v.reference.assetId))
      : cell ? mainVariant(store.listFor(Number(cell.dataset.gridItem))) : null;
    if(!variant || !store.selection().ids.has(variantKey(store.repository()??"",variant.reference)))return;
    const variants=[...store.selectedVariants()];const repository=store.repository();if(!repository)return;
    cancelDrag?.();
    cancelDrag=trackPointerDrag(event,{threshold:6,cancelOutsideWindow:true,
      start:point=>{clearDragClick();window.addEventListener("click",blockClick,true);variants.slice(0,4).forEach(v=>issueThumbs.request(JSON.stringify([repository,v.reference,v.profileHash]),true));setDragBatch({variants,repository,...point,target:null});},
      move:point=>setDragBatch(old=>old?{...old,...point,target:hitTarget(point)}:null),
      end:(point,cancelled,started)=>{const batch=dragBatch();setDragBatch(null);
        if(started){clickTimer=setTimeout(clearDragClick,0);if(!cancelled&&batch){const target=hitTarget(point);
          if(target?.kind==="preset")void dropIntoPreset(batch,target.id);
          else if(target?.kind==="queue")dropIntoQueue(batch);}}
      },
    });
  }
  createEffect(()=>{
    const batch=dragBatch();if(!batch)return;
    if(batch.repository!==store.repository() || batch.variants.some(v=>!store.selection().ids.has(variantKey(batch.repository,v.reference))))cancelDrag?.();
  });
  onCleanup(()=>{cancelDrag?.();clearDragClick();});
  function PresetQueueCount(p:{id:string}){
    const [shown,setShown]=createSignal(store.progress(p.id).total);
    const [animating,setAnimating]=createSignal(false);
    let cancel=()=>{};
    createEffect(on(()=>countDrops().get(p.id),request=>{
      cancel();setAnimating(false);
      if(!request){setShown(store.progress(p.id).total);return;}
      setShown(request.from);
      if(request.to===undefined)return;
      if(window.matchMedia('(prefers-reduced-motion: reduce)').matches){setShown(request.to);return;}
      setAnimating(request.from!==request.to);
      cancel=animateCount(request.from,request.to,value=>{setShown(value);if(value===request.to)setAnimating(false);});
    }));
    createEffect(()=>{const total=store.progress(p.id).total;untrack(()=>{
      const request=countDrops().get(p.id);
      if(!request || (request.to!==undefined && total!==request.to)){cancel();setAnimating(false);setShown(total);}
    });});
    onCleanup(()=>cancel());
    return <span data-export-preset-count={p.id} class="shrink-0 text-fg-2 tnum" classList={{'rb-export-count-bump':animating()}}>
      {Math.max(0,store.progress(p.id).remaining-(store.progress(p.id).total-shown()))}/{shown()}
    </span>;
  }
  function QueueBadge(p: { status?: string; error?: string|null }) {
    const text=()=>p.status?t(`export.status.${p.status}` as "export.status.pending"):"";
    return <Show when={p.status}><span data-export-status={p.status} title={p.status === "waiting" ? t(p.error === "storage.wait.target" ? "export.waiting_target" : "export.waiting_library") : p.error??text()} aria-label={text()} class="pointer-events-none absolute left-1 top-1 rounded-ui bg-surface-layer p-0.5 text-fg-1">
      <Show when={(p.status==="pending" || p.status==="waiting")}><IconClock size={14}/></Show>
      <Show when={p.status==="running"}><IconLoader2 size={14} class="animate-spin"/></Show>
      <Show when={p.status==="done"}><IconCheck size={14}/></Show>
      <Show when={p.status==="skipped"}><IconPlayerSkipForward size={14}/></Show>
      <Show when={p.status==="failed"}><IconAlertTriangle size={14} class="text-danger"/></Show>
    </span></Show>;
  }
  function IssueTile(p: { variant: VariantSummary; size: number }) {
    const key=()=>JSON.stringify([store.repository(),p.variant.reference,p.variant.profileHash]);
    const state=()=>store.queueState(p.variant.reference,p.variant.profileHash);
    createEffect(()=>{ const value=key(); issueThumbs.request(value); onCleanup(()=>issueThumbs.cancel(value)); });
    return <div data-export-issue={p.variant.reference.variant} class="relative" title={p.variant.name} style={{width:`${p.size}px`,height:`${p.size}px`,"--tile-cell":`${p.size}px`}}>
      <Tile keyboardActivate={false} label={p.variant.name} src={issueThumbs.get(key()).url} approximate={issueThumbs.get(key()).approximate} aspect={1} minimal selectionFrame
        selected={store.selection().ids.has(variantKey(store.repository()??"",p.variant.reference))}
        disabled={store.locked(p.variant)} selectionLocked={store.locked(p.variant)}
        loading={issueThumbs.get(key()).status==="loading"}
        onClick={event=>{event.stopPropagation();void store.selectIssue(p.variant.reference,clickMode(event,true),assets()).catch(store.reportError);}}
        onActivate={()=>showIssue(p.variant)}/>
      <QueueBadge status={state()?.status} error={state()?.error}/>
      {/* 导出尾号（specs/export-issue-ordinal.md）：I00–I99 / ISO / IRA / ILA，右上角小徽标 */}
      <span data-export-suffix={variantSuffix(p.variant)} class="pointer-events-none absolute right-1 top-1 rounded-ui bg-surface-layer px-1 text-fs-0 leading-4 text-fg-2">{variantSuffix(p.variant)}</span>
    </div>;
  }
  function IssueStrip(p: { assetId: number }) {
    const small=()=>Math.max(1,(tileSizeAt(prefs.value().topStep, EXPORT_TOP_SIZE_BOUNDS)-4)/2);
    return <div data-export-issues class="pt-1">
      <div class="grid grid-cols-2 gap-1"><For each={store.smallFor(p.assetId).slice(0,6)}>{v=><IssueTile variant={v} size={small()}/>}</For></div>
      <Show when={store.smallFor(p.assetId).length>6}><button type="button" class="flex h-5 items-center rounded-ui text-fs-0 text-fg-2 hover:text-fg-1" onClick={()=>{store.focusArea("gallery");setAllIssues(p.assetId);}}>{t("export.allIssues")}</button></Show>
    </div>;
  }
  function QueueFailure(p: { id: string }) {
    const item = () => store.queueItems().find(entry => entry.id === p.id);
    return <Show when={item()?.status === "failed"}>
      <div data-export-queue-error class={prefs.value().queueList ? "min-w-0 flex-1" : "mt-1 flex h-16 min-w-0 flex-col justify-between"}>
        <p class="line-clamp-2 break-words text-fs-0 text-danger" title={item()?.error === "storage.released" ? t("repo.connection.released") : item()?.error ?? t("export.failedUnknown")}>
          {t("export.failureReason", {reason: item()?.error === "storage.released" ? t("repo.connection.released") : item()?.error ?? t("export.failedUnknown")})}
        </p>
        <Button data-export-retry class="self-start" size="sm" onClick={() => store.retry([p.id])}>{t("export.retry")}</Button>
      </div>
    </Show>;
  }
  const queueOverlay=(id:string)=>{
    const item=()=>store.queueItems().find(i=>i.id===id);
    return <QueueBadge status={item()?.status} error={item()?.error}/>;
  };
  const top = (
    <div data-export-area="gallery" onDragStart={event=>event.preventDefault()} class="flex min-h-0 flex-1 border" classList={{"border-brand-2":store.activeArea()==="gallery","border-surface-layer":store.activeArea()!=="gallery"}} onPointerDown={event=>{store.focusArea("gallery");beginIssueDrag(event);}} onFocusIn={()=>store.focusArea("gallery")}><TilesShell sizeBounds={EXPORT_TOP_SIZE_BOUNDS} bar={bar()}>
      <PhotoGrid
        source={gallery}
        commandEnter
        cellExtra={(item) => <IssueStrip assetId={Number(item.id)} />}
        cellOverlay={item=>{const main=()=>mainVariant(store.listFor(Number(item.id)));const state=()=>main()?store.queueState(main()!.reference,main()!.profileHash):undefined;return <QueueBadge status={state()?.status} error={state()?.error}/>;}}
        activate={(id) => {
          const v = mainVariant(store.listFor(Number(id)));
          if (v !== null) showIssue(v);
        }}
        watermark={galleryEmpty}
        onInteract={() => setLibsExpanded(false)}
      />
    </TilesShell></div>
  );
  const bottom = (
    <div data-export-area="queue" data-export-drop={queueTarget()?"queue":undefined} class="flex min-h-0 flex-1 border" classList={{"border-brand":queueTarget(),"border-brand-2":store.activeArea()==="queue"&&!queueTarget(),"border-surface-layer":store.activeArea()!=="queue"&&!queueTarget()}} onPointerDown={()=>store.focusArea("queue")} onFocusIn={()=>store.focusArea("queue")}><TilesShell bar={bar(true)}>
      <PhotoGrid
        source={queue}
        listMode={prefs.value().queueList}
        commandEnter
        cellOverlay={(item) => queueOverlay(item.id)}
        cellExtra={(item) => <QueueFailure id={item.id} />}
        activate={(id) => {
          const item=store.queueItems().find(entry=>entry.id===id);
          if(item)void openFullscreen([{id:item.id,path:joinPath(item.root,item.snapshot.relPath),fileName:item.snapshot.name,
            exportVariant:{repositoryId:item.repositoryId,reference:item.snapshot.reference,captured:item.snapshot}}],0).catch(store.reportError);
        }}
        watermark={() => queue.count()>0 ? null : (
          <StateWatermark
            icon={<IconUpload size={64} />}
            text={
              store.selectedPreset() === null
                ? t("export.pickPreset")
                : t("export.emptyQueue")
            }
          />
        )}
      />
    </TilesShell></div>
  );
  const ratio = untrack(() => prefs.value().ratio);
  const fieldError = (field: string) => store.validation().errors[field];
  return (
    <div data-export-workspace class="flex min-h-0 flex-1 overflow-hidden">
      <div
        data-export-left
        class="flex min-h-0 shrink-0 flex-col"
        style={{ width: `${leftWidth()}px` }}
      >
        <BrowseLeftColumn
          store={props.browse}
          repositories={repositories()}
          reposLoading={reposLoading()}
          reposError={reposError()}
          libsExpanded={libsExpanded()}
          onExpandLibs={() => setLibsExpanded(true)}
          onCollapseLibs={() => setLibsExpanded(false)}
          onOpenSettings={props.onOpenLibrarySettings}
          isRemounting={props.repositories.isRemounting}
          onRemount={(id) =>
            void props.repositories.remount(id)
          }
        />
      </div>
      <SplitHandle
        orientation="vertical"
        aria-label={t("common.resize_left")}
        tabindex="0"
        onDragStart={() => {
          dragStart = leftWidth();
        }}
        onDrag={(dx) =>
          setLeftWidth(
            resizeWidth({
              start: dragStart,
              dx,
              bounds: LAYOUT_BOUNDS.browseLeftWidth,
            }),
          )
        }
        onDragEnd={() => props.onLeftWidthChange(leftWidth())}
        onKeyDown={(event) => {
          if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
          event.preventDefault();
          const width = nudgeWidth(
            leftWidth(),
            event.key === "ArrowRight" ? 8 : -8,
            LAYOUT_BOUNDS.browseLeftWidth,
          );
          setLeftWidth(width);
          props.onLeftWidthChange(width);
        }}
      />
      <div
        data-export-mid
        class="flex min-h-0 min-w-0 flex-1 flex-col bg-surface-bar"
      >
        <Show when={store.error() || props.browse.error()}>
          <p
            role="alert"
            class="shrink-0 break-words p-2 text-fs-1 text-danger"
          >
            {store.error() ?? props.browse.error()}
          </p>
        </Show>
        <SplitStack
          class="flex-1"
          segments={[
            {
              id: t("export.gallery"),
              defaultSize: ratio * 100,
              minSize: 20,
              content: top,
            },
            {
              id: t("export.queue"),
              defaultSize: (1 - ratio) * 100,
              minSize: 20,
              content: bottom,
            },
          ]}
          onResizeEnd={(sizes) =>
            prefs.update({ ratio: (sizes[0] ?? 58) / 100 })
          }
        />
      </div>
      <aside
        data-export-right
        class="flex w-panel-w-right min-h-0 shrink-0 flex-col gap-3 bg-surface-main p-panel-pad"
      >
        <section data-export-presets class="flex min-h-24 flex-1 flex-col gap-2">
        <div class="flex shrink-0 items-center justify-between">
          <span class="text-fs-2 text-fg-2">{t("export.presetList")}</span>
        </div>
        <div data-export-preset-list class="min-h-0 flex-1 overflow-auto">
          <For each={store.presets()} fallback={<p class="text-fs-1 text-fg-3">{t("export.noPresets")}</p>}>
            {(preset) => (
              /*
                卡片是「容器 + 两个动作」（2026-09-28）：点正文 = 选中这个预设；
                右下的移除（easy destroy）删预设。「未完成/总数」计数挪到右上，
                给移除让出右下角。`data-export-preset` 挂在容器上：拖放命中与冒烟都靠它。
              */
              <div
                data-export-preset={preset.id}
                onClick={(event) => {
                  // 点卡片任意处 = 选中这个预设（旧卡片的整卡可点语义）；
                  // 点里面的按钮（选择/移除）不重复处理。
                  if ((event.target as HTMLElement).closest("button")) return;
                  store.choosePreset(preset.id);
                }}
                class="border-2 mb-1 flex w-full items-center gap-2 rounded-ui p-2 text-fs-1"
                classList={{
                  "border-brand-2":presetTarget(preset.id),
                  "border-transparent":!presetTarget(preset.id),
                  "bg-state-selected":store.selectedPreset()?.id===preset.id,
                  "bg-surface-track":store.selectedPreset()?.id!==preset.id,
                  "hover:bg-state-hover":store.selectedPreset()?.id!==preset.id,
                }}
              >
                <button
                  type="button"
                  class="min-w-0 flex-1 text-left"
                  aria-pressed={store.selectedPreset()?.id===preset.id}
                  onClick={() => store.choosePreset(preset.id)}
                >
                  <span class="block truncate text-fg-1">{preset.name}</span>
                  <span class="block truncate text-fs-0 text-fg-3">
                    {preset.format.toUpperCase()} ·{" "}
                    {preset.sizeMode === "original" ? t("export.originalSize") : preset.sizeMode === "percent" ? preset.percent + "%" : preset.maxEdge + "px"}
                  </span>
                </button>
                <div class="flex shrink-0 flex-col items-end justify-end gap-0.5">
                  <div class="flex items-center gap-1">
                    <Show when={store.enabled().has(preset.id)}><IconCheck size={14} class="shrink-0 text-brand" aria-label={t("export.start")}/></Show>
                    <Show when={store.progress(preset.id).total > 0}>
                      <PresetQueueCount id={preset.id}/>
                    </Show>
                  </div>
                  <EasyDestroyButton
                    label={t("export.preset.delete")}
                    confirmTitle={t("export.preset.deleteTitle")}
                    confirmMessage={t("export.preset.deleteConfirm").replace("{name}", preset.name)}
                    confirmLabel={t("export.preset.deleteAction")}
                    onRemove={() => void store.deletePreset(preset.id)}
                  />
                </div>
              </div>
            )}
          </For>
        </div>
        </section>
        <section data-export-settings class="flex min-h-0 max-h-[calc(100%_-_108px)] shrink-0 flex-col gap-2">
        <form
          id="export-preset-form"
          data-export-form
          class="flex min-h-0 shrink flex-col gap-2 overflow-auto"
          onSubmit={(event) => {
            event.preventDefault();
            void store.save();
          }}
        >
          <span data-export-form-title class="text-fs-2 text-fg-2">
            {t(store.matchingPreset() === null ? "export.newPreset" : "export.editPreset")}
          </span>
          <label class="flex flex-col gap-1 text-fs-1 text-fg-2">
            {t("export.name")}
            <Input
              value={store.draft().name}
              invalid={!!fieldError("name")}
              onInput={(event) =>
                store.edit({ name: event.currentTarget.value })
              }
            />
          </label>
          <div class="text-fs-1 text-fg-2">{t("export.format")}</div>
          <SegmentedControl
            label={t("export.format")}
            value={store.draft().format}
            options={EXPORT_FORMATS.map((value) => ({ value, label: value === "jpeg" ? "JPG" : value.toUpperCase() }))}
            onValueChange={(format) => store.edit({ format })}
          />
          <Show when={formatSupportsQuality(store.draft().format)}>
          <div data-export-quality class="flex justify-between text-fs-1 text-fg-2">
            <span>{t("export.quality")}</span>
            <span class="tnum">{store.draft().quality}</span>
          </div>
          <Slider
            value={store.draft().quality}
            min={1}
            max={100}
            label={t("export.quality")}
            onValueChange={(quality) => store.edit({ quality })}
          />
          </Show>
          <div data-export-size class="flex flex-col gap-1 text-fs-1 text-fg-2">
            <span>{t("export.size")}</span>
            <RadioChoices label={t("export.size")} value={store.draft().sizeMode}
              options={[{value:"original",label:t("export.keepSize")},{value:"percent",label:t("export.percentSize")},{value:"maxEdge",label:t("export.maxEdge")}]}
              onValueChange={sizeMode=>store.edit({sizeMode, ...(sizeMode==="maxEdge" && store.draft().maxEdge===0?{maxEdge:2048}:{})})}
              trailing={mode => mode==="original" ? null : <span class="flex shrink-0 items-center gap-1">
                <Input data-export-size-value={mode} class="w-24 min-w-0 text-right tnum" type="number" min="1" max={mode==="percent"?100:65535}
                  aria-label={t(mode==="percent"?"export.percentSize":"export.maxEdge")}
                  disabled={store.draft().sizeMode!==mode}
                  value={mode==="percent"?store.draft().percent:store.draft().maxEdge}
                  invalid={!!fieldError(mode==="percent"?"percent":"maxEdge")}
                  onInput={event=>store.edit({[mode==="percent"?"percent":"maxEdge"]:event.currentTarget.value===""?NaN:Number(event.currentTarget.value)})}/>
                <span class="w-4 text-fg-3">{mode==="percent"?"%":"px"}</span>
              </span>}/>
          </div>
          <label class="flex flex-col gap-1 text-fs-1 text-fg-2">
            {t("export.directory")}
            <span class="flex gap-1">
              <Input
                class="min-w-0 flex-1"
                value={store.draft().directory}
                invalid={!!fieldError("directory")}
                onInput={(event) =>
                  store.edit({ directory: event.currentTarget.value })
                }
              />
              <IconButton
                label={t("export.pickDirectory")}
                onClick={() =>
                  void pickDirectory({ title: t("export.pickDirectory") })
                    .then((dir) => {
                      if (dir !== null) store.edit({ directory: dir });
                    })
                    .catch(store.reportError)
                }
              >
                <IconFolderOpen size={16} />
              </IconButton>
            </span>
          </label>
          <div data-export-existing-file class="flex flex-col gap-1 text-fs-1 text-fg-2">
            <span>{t("export.existingFile")}</span>
            <SegmentedControl label={t("export.existingFile")} value={store.draft().existingFile}
              options={EXISTING_FILE_POLICIES.map(value=>({value,label:t(`export.existingFile.${value}`)}))}
              onValueChange={existingFile=>store.edit({existingFile})}/>
          </div>
          <label class="flex flex-col gap-1 text-fs-1 text-fg-2">
            {t("export.template")}
            <Input
              value={store.draft().template}
              invalid={!!fieldError("template")}
              onInput={(event) =>
                store.edit({ template: event.currentTarget.value })
              }
            />
          </label>
          <p class="text-fs-0 text-fg-3">{t("export.templateHelp")}</p>
          <For each={Object.entries(store.validation().errors)}>
            {([field, message]) => (
              <p role="alert" class="break-words text-fs-0 text-danger">
                {t("export.invalidField", {
                  field: t(
                    field === "name"
                      ? "export.name"
                      : field === "directory"
                        ? "export.directory"
                        : field === "template"
                          ? "export.template"
                          : ["maxEdge","sizeMode","percent"].includes(field)
                            ? "export.size"
                            : field === "quality"
                              ? "export.quality"
                              : field === "existingFile" ? "export.existingFile" : "export.format",
                  ),
                  message: message === field ? t("export.invalid") : message,
                })}
              </p>
            )}
          </For>
          <For each={store.validation().warnings}>
            {(message) => <p class="text-fs-0 text-fg-3">{message}</p>}
          </For>
        </form>
        <div data-export-actions class="flex shrink-0 items-center gap-2">
          <Button data-export-save type="submit" form="export-preset-form" class="shrink-0"
            variant={store.matchingPreset() ? "secondary" : "accent"}
            disabled={!store.canSave()}
            loading={store.busy()}>
            {t(store.matchingPreset() ? "export.update" : "export.add")}
          </Button>
          <Button data-export-run class="min-w-0 flex-1" variant="primary" disabled={!store.canRun()} onClick={store.toggleRun}>
            <span class="relative grid overflow-hidden" aria-busy={store.progress(store.selectedPreset()?.id??"").processing}>
              <span class="col-start-1 row-start-1 flex items-center gap-1.5" classList={{"opacity-50":store.progress(store.selectedPreset()?.id??"").processing}}><IconUpload size={14}/>{t(store.enabled().has(store.selectedPreset()?.id??"")?"export.stop":"export.start")}</span>
              <Show when={store.progress(store.selectedPreset()?.id??"").processing}><span aria-hidden="true" data-export-run-processing class="rb-shimmer-mask pointer-events-none col-start-1 row-start-1 flex items-center gap-1.5"><IconUpload size={14}/>{t(store.enabled().has(store.selectedPreset()?.id??"")?"export.stop":"export.start")}</span></Show>
            </span>
          </Button>
        </div>
        <p data-export-save-hint class="shrink-0 text-fs-0 text-fg-3">{t("export.saveBeforeStart")}</p>
        </section>
      </aside>
      <Show when={dragBatch()}>{batch=><Portal>
        {/*
          指针预览：整叠卡片的**中心落在指针上**（再 +8px，看着像「捏着」而不是「贴着」）。

          旧版把卡片摆在指针右下 100px 开外（容器内 26/38 的定位 + 14px 偏移），拖动时
          目光要在指针与幽灵之间来回跳；现在中心对齐，瞄准半径只剩一张卡片。

          也不再向窗口内夹取：夹取会在指针出窗时把幽灵钉在窗口边缘，看着像拖动卡死了。
          出窗已经由 `cancelOutsideWindow` 中止，幽灵跟着指针走就行。
        */}
        <div data-export-drag-preview aria-hidden="true" class="pointer-events-none fixed z-(--z-modal) opacity-80" style={{left:`${batch().x+8}px`,top:`${batch().y+8}px`,transform:"translate(-50%,-50%)"}}>
          <div class="relative size-[132px]">
            <For each={batch().variants.slice(0,4)}>{(v,i)=><div class="rb-export-drag-card absolute inset-0 overflow-hidden rounded-ui border border-fg-3 bg-surface-layer" style={{transform:`translate(${i()*9}px,${-i()*3}px) rotate(${(i()-(Math.min(4,batch().variants.length)-1)/2)*8}deg)`,'transform-origin':'50% 85%'}}>
              <Show when={issueThumbs.get(JSON.stringify([batch().repository,v.reference,v.profileHash])).url} fallback={<IconPhoto size={48} class="m-auto mt-10 text-fg-3"/>}>{url=><img class="size-full object-contain" src={url()} alt="" draggable={false}/>}</Show>
            </div>}</For>
            <span data-export-drag-count class="absolute bottom-0 right-0 rounded-ui bg-surface-layer px-2 py-1 text-fs-2 text-fg-1 tnum">{batch().variants.length}</span>
          </div>
        </div>
      </Portal>}</Show>
      <Dialog open={store.limitOpen()} onOpenChange={store.setLimitOpen} title={t("export.limit")}><p class="text-fg-2">{t("export.limitBody")}</p></Dialog>
      <Dialog open={allIssues()!==null} onOpenChange={open=>{if(!open)setAllIssues(null);}} title={t("export.allIssues")} size="wide">
        <div data-export-all-issues class="grid max-h-[60vh] grid-cols-[repeat(auto-fit,minmax(136px,1fr))] gap-2 overflow-auto">
          <For each={allIssues()===null?[]:store.refsFor([allIssues()!])}>{v=><IssueTile variant={v} size={136}/>}</For>
        </div>
        <div class="flex gap-2"><Button disabled={store.selection().ids.size===0} onClick={store.clear}>{t("export.clear")}</Button><Button disabled={!store.canEnqueue()} onClick={()=>{const dir=root();if(dir)void store.enqueue(dir);}}>{t("export.enqueue")}</Button><Button disabled={!store.canRemove()} onClick={store.removeSelected}>{t("export.remove")}</Button></div>
      </Dialog>
      <ConfirmDialog
        open={resetOpen()}
        onCancel={() => setResetOpen(false)}
        title={t("export.reset")}
        message={t("export.resetConfirm")}
        onConfirm={() => {
          store.reset();
          issueThumbs.clear();
          setResetOpen(false);
        }}
      />
    </div>
  );
}
