/**
 * 编辑**左列**（2026-10-12 改版，崔总口述；画稿 `design/editor.md` §3.1 / §3.13）。
 *
 * ```text
 * ┌──────────────────────────┐
 * │ LUT 面板 或 定稿面板        │  ← 上方：带滚动列表（toolsbar left 的两个开关切换）
 * │ …（列表滚动）…             │
 * ├──────────────────────────┤
 * │ [总览][信息]              │  ← 下方：一个页签组，**贴着底部固定**
 * │ 预览 + 视野框 + 缩放行      │     总览 = 预览（随照片比例）+ 缩放控制
 * │ （信息：完整列表，内部滚动） │     信息 = 高度 ×1.8，挤占上方列表的空间
 * └──────────────────────────┘
 * ```
 *
 * 三条纪律：
 *
 * 1. **总览 / 信息跟着左列一起消失**：整列由 `store.leftVisible()` 控制，
 *    关掉时连底部块一起不占位置（不是「总览常驻」）；
 * 2. 总览用的是与**浏览右栏同一个** `PreviewFrame`，视野框走
 *    `overviewVisibleRect()`（同一个 `visibleRect()` 变换）；
 * 3. 定稿的「定稿为新 Issue」主按钮**住在这个面板里**（原来在 toolsbar right），
 *    贴底、整宽、主色 —— 图标与文案与原来一致（`editor.issue.toolbar`）。
 */

import { createEffect, createMemo, createSignal, For, onCleanup, Show, type JSX } from "solid-js";
import {
  IconBookmark,
  IconLoader2,
  IconPencil,
  IconTrash,
} from "@tabler/icons-solidjs";

import { Button } from "../../components/ui/Button.tsx";
import { MetadataRows } from "../../components/ui/MetadataRows.tsx";
import { SegmentedControl } from "../../components/ui/SegmentedControl.tsx";
import { PreviewFrame } from "../../components/ui/PreviewFrame.tsx";
import { t } from "../../i18n/index.ts";
import { formatDay } from "../../lib/datetime.ts";
import type { MessageKey } from "../../i18n/index.ts";
import type { ExifTag } from "../../api/types.ts";
import type { DevelopStack } from "../../api/editor.ts";
import type { IssueLibrary, Issue, IssueSelection } from "../../api/issues.ts";
import type { ShiftLikeEvent } from "../../lib/easy-destroy.ts";
import type { ThumbQueue } from "../../components/ui/thumb-queue.ts";
import type { ViewerPhoto } from "../../components/ui/viewer/index.ts";
import type { EditorStore } from "./store.ts";
import { LutPanel } from "./lut-panel.tsx";
import { EditorZoomControl } from "./zoom-control.tsx";
import { overviewVisibleRect } from "./overview-rect.ts";
import { previewFrameAspect } from "../../lib/preview-frame.ts";
import { EDITOR_LEFT_BOTTOM_INFO, EDITOR_LEFT_WIDTH } from "./layout.ts";

/* ══════════════════════════════════════════════════════════════
 * 入参形状
 * ══════════════════════════════════════════════════════════════ */

/**
 * 「信息」页签要的字段（**结构类型**，由工作区拼好传进来）。
 *
 * 为什么不让本模块自己去查：`features/*` 之间不许互相 import（分层规则），
 * 而照片数据属于浏览那一侧 —— 工作区把两块接起来正是它的职责（`memory/ARCHITECTURE.md` §2）。
 * 这里的字段**都是显示字符串**（格式化的唯一实现在 `features/exif-strip/exif-format.ts`
 * 与 `lib/datetime.ts`）。
 *
 * 包含 flowbar 中已有的拍摄参数；调节相关项置顶，后面列出文件内全部可读标签。
 */
export interface EditorPhotoInfo {
  tags: ExifTag[];
  fileName: string;
  relativePath: string;
  temperatureBaseline: string | null;
  exposureBias: string | null;
  iso: string | null;
  shutter: string | null;
  aperture: string | null;
  focal: string | null;
  cameraMake: string | null;
  cameraModel: string | null;
  lens: string | null;
  dimensions: string | null;
  orientation: string | null;
  takenAt: string | null;
  datetimeRaw: string | null;
  software: string | null;
  gps: string | null;
  format: string | null;
  kind: string | null;
}

export interface EditorLeftPanelProps {
  store: EditorStore;
  /** 有没有可以编辑的照片（空态下控件禁用） */
  enabled: boolean;
  /* ── 底部总览 ── */
  /** 当前那张（胶片带的锚点） */
  current: ViewerPhoto | null;
  /** 总览专用 Screen 全图队列 */
  overviewImages: ThumbQueue;
  /** Rust 视口的当前缩放（`null` = 还没有渲染状态） */
  zoom: number | null;
  /** 渲染状态读数（视野框要从它算；没有就不画框） */
  renderState?: () => import("../../api/types.ts").EditorRenderState | null;
  onZoomBy?: (factor: number) => void;
  onZoomTo?: (zoom: number) => void;
  /* ── 信息页签 ── */
  info: EditorPhotoInfo | null;
  /* ── 定稿面板 ── */
  issues: IssueLibrary | null;
  /** 正在生成的定稿（确认后立刻现身，预览位转圈；成功后由真实条目接替） */
  pendingIssue?: { name: string; sourceBase: "raw" | "sooc" } | null;
  issueFocusTick: number;
  /**
   * 选中态的**本地先行值**（崔总 2026-09-28：切定稿时面板要立刻亮，不能等后端）。
   * 点击后由工作区立刻写入，`issues.selection` 追上来或换图时清空；`null` = 没在先行。
   */
  issueSelectionOverride?: IssueSelection | null;
  onSelectIssue: (stack: DevelopStack, selection: IssueSelection) => void;
  onDeleteIssue: (issue: Issue, event: ShiftLikeEvent) => void;
  /** 点定稿名字旁的笔：由工作区弹出改名弹窗 */
  onRenameIssue: (issue: Issue) => void;
  issueThumbs: ThumbQueue;
  issueThumbKey: (choice: string) => string;
  /** 「定稿为新 Issue」能不能点（原来 toolsbar right 那个按钮的判据） */
  canFinalize: () => boolean;
  onFinalize: () => void;
  /* ── LUT 面板（透传；面板本体仍在 `lut-panel.tsx`） ── */
  onCreateCategory: (name: string) => Promise<boolean>;
  onImport: (category: string) => Promise<string | null>;
  onSelectLut: (id: string) => void;
  onToggleLut: () => void;
  onHideLut: (id: string) => Promise<void>;
  class?: string;
}

type LeftBottomTab = "view" | "info";

export function EditorLeftPanel(props: EditorLeftPanelProps): JSX.Element {
  const [tab, setTab] = createSignal<LeftBottomTab>("view");
  /*
   * 点「定稿」页签（`issueFocusTick`）只切左列面板，不重置阅读位置 ——
   * 与原来右栏第 1 组的规则一致（崔总 2026-09-28：阅读位置是用户的）。
   */
  createEffect(() => {
    if (props.issueFocusTick > 0) props.store.setLeftPanel("issues");
  });

  const rect = createMemo(() => overviewVisibleRect(props.renderState?.() ?? null));
  /*
   * 当前照片的**外框比例**（夹在 3:2～2:3）—— 以 CSS 变量传给底部块：
   * 信息态的高度公式（`layout.ts`）要靠它把「总览态多高」算出来。
   */
  const aspect = createMemo(() =>
    previewFrameAspect(props.current?.natural?.width, props.current?.natural?.height),
  );

  return (
    <div
      data-editor-left
      class={["flex min-h-0 flex-1 flex-col bg-surface-main", props.class ?? ""].filter(Boolean).join(" ")}
      style={{ width: `${EDITOR_LEFT_WIDTH}px` }}
    >
      {/*
        上方：LUT / 定稿 两个面板**同一时刻只开一个**（toolsbar left 切换）。
        `min-h-0 flex-1` 让列表区吃掉剩余高度，底部块才钉得住。
      */}
      <div class="flex min-h-0 flex-1 flex-col" data-editor-left-top>
        <Show when={props.store.leftPanel() === "lut"}>
          <LutPanel store={props.store} onCreateCategory={props.onCreateCategory} onImport={props.onImport}
            onSelect={props.onSelectLut} onToggle={props.onToggleLut} onHide={props.onHideLut} />
        </Show>
        <Show when={props.store.leftPanel() === "issues"}>
          <IssuesPanel enabled={props.enabled} store={props.store} library={props.issues}
            selectionOverride={props.issueSelectionOverride ?? null}
            pending={props.pendingIssue ?? null}
            onSelect={props.onSelectIssue} onDelete={props.onDeleteIssue} onRename={props.onRenameIssue}
            thumbs={props.issueThumbs} thumbKey={props.issueThumbKey}
            canFinalize={props.canFinalize} onFinalize={props.onFinalize} />
        </Show>
      </div>

      {/*
        下方：**贴着底部固定**的页签组。总览态高度由内容决定（预览随比例）；
        信息态整块 ×1.8（`layout.ts`），把上方面板挤短 —— 信息内容自己滚动。
      */}
      <div
        class="flex shrink-0 flex-col gap-2 px-panel-pad pb-panel-pad pt-2"
        data-editor-left-bottom
        data-tab={tab()}
        style={{
          "--preview-aspect": String(aspect()),
          ...(tab() === "info" ? { height: EDITOR_LEFT_BOTTOM_INFO } : {}),
        }}
      >
        <SegmentedControl
          class="w-full shrink-0"
          value={tab()}
          onValueChange={(value) => setTab(value as LeftBottomTab)}
          label={t("editor.group.view")}
          options={[
            { value: "view", label: t("editor.group.view") },
            { value: "info", label: t("editor.group.info") },
          ]}
        />
        <Show when={tab() === "view"}>
          <div class="flex flex-col gap-2" data-editor-overview>
            <PreviewFrame
              src={overviewThumb(props)}
              natural={props.current?.natural ?? null}
              visibleRect={rect()}
              emptyText={props.current === null ? t("editor.empty.noSelection") : t("common.loading")}
            />
            {/* 缩放控制仍在总览图片下方（原来在右栏「总览」里，跟着总览一起搬过来） */}
            <EditorZoomControl
              zoom={props.zoom}
              enabled={props.enabled}
              onZoomBy={(factor) => props.onZoomBy?.(factor)}
              onZoomTo={(zoom) => props.onZoomTo?.(zoom)}
            />
          </div>
        </Show>
        <Show when={tab() === "info"}>
          <div class="min-h-0 flex-1" data-editor-info-host>
            <InfoTab info={props.info} />
          </div>
        </Show>
      </div>
    </div>
  );
}

/** 总览图源：走完整 Screen 档（长边 ≤1920），按需请求（队列自己管缓存）。 */
function overviewThumb(props: EditorLeftPanelProps): string | null {
  const path = props.current?.path ?? null;
  if (path === null) return null;
  props.overviewImages.request(path, true);
  return props.overviewImages.get(path).url;
}

/* ══════════════════════════════════════════════════════════════
 * 定稿面板
 * ══════════════════════════════════════════════════════════════ */

/** 定稿面板：该图的 issue 列表（含 SOOC / RAW / 最近编辑三种基准）+ 定稿主按钮。 */
function IssuesPanel(props: {
  enabled: boolean;
  store: EditorStore;
  library: IssueLibrary | null;
  selectionOverride: IssueSelection | null;
  pending: { name: string; sourceBase: "raw" | "sooc" } | null;
  onSelect: (stack: DevelopStack, selection: IssueSelection) => void;
  onDelete: (issue: Issue, event: ShiftLikeEvent) => void;
  onRename: (issue: Issue) => void;
  thumbs: ThumbQueue;
  thumbKey: (choice: string) => string;
  canFinalize: () => boolean;
  onFinalize: () => void;
}): JSX.Element {
  const isSelected = (kind: "sooc" | "raw" | "latest" | number): boolean => {
    // 先行值优先：点击那一刻就亮；后端的 selection 追上来后两者自然一致
    const selected = props.selectionOverride ?? props.library?.selection;
    return typeof kind === "number" ? typeof selected === "object" && selected.issue === kind : selected === kind;
  };
  const source = (base: "sooc" | "raw"): DevelopStack => ({
    sourceBase: base, values: {}, curves: {}, asShotK: props.store.asShotTemperature(),
    lensProfile: null, lensEnabled: null, baseCurveProfile: null, baseCurvePoints: null,
    lutId: null, lutEnabled: null, nrMethod: null, geometry: null,
  });
  return (
    <div data-editor-issues class="flex min-h-0 flex-1 flex-col">
      <div class="flex shrink-0 items-center gap-1 px-panel-pad pt-panel-pad pb-1.5">
        <span class="flex-1 text-fs-3 font-semibold text-fg-1">{t("editor.group.issues")}</span>
      </div>
      <div class="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto px-panel-pad pb-2">
        <For each={(["sooc", "raw"] as const)}>{(base) =>
          <button type="button" disabled={!props.enabled || !(base === "raw" ? props.store.editBaseAvailable().raw : props.store.editBaseAvailable().bitmap)}
            class="flex min-h-12 items-center gap-2 rounded-ui px-1 text-left hover:bg-state-hover disabled:opacity-50"
            classList={{ "bg-state-selected": isSelected(base) }} onClick={() => props.onSelect(source(base), base)}>
            <IssueThumb imageKey={props.thumbKey(base)} thumbs={props.thumbs} />
            <span class="block text-fs-2 text-fg-1">{base.toUpperCase()}</span>
          </button>}
        </For>

        <div class="flex min-h-12 items-center gap-2 rounded-ui px-1" classList={{ "bg-state-selected": isSelected("latest") }}>
          <IssueThumb imageKey={props.thumbKey("latest")} thumbs={props.thumbs} />
          <span class="block text-fs-2 text-fg-1">{t("editor.issue.latest")}</span>
        </div>
        {/*
          正在生成的定稿（占位）：确认弹窗一关它就现身，预览位转圈（崔总 2026-09-29：
          「先生出条目，生成完再显示出图」，不再是几秒无事发生）。
          列表按 created_at 倒序，新定稿落在最顶 —— 占位也放最顶，接替时无跳变。
          不可点选：后端还在渲染，选它没有意义。
        */}
        <Show when={props.pending}>{(pending) =>
          <div class="flex min-h-12 items-center gap-1 rounded-ui px-1" data-editor-issue-pending aria-busy="true">
            <span class="flex h-10 w-14 shrink-0 items-center justify-center overflow-hidden rounded-ui bg-surface-bar">
              <IconLoader2 size={16} class="animate-spin text-fg-3" aria-hidden="true" />
            </span>
            <span class="flex min-w-0 flex-1 flex-col">
              <span class="fade-x-end block overflow-hidden whitespace-nowrap text-fs-2 leading-5 text-fg-1"
                title={pending().name}>{pending().name}</span>
              <span class="fade-x-end block overflow-hidden whitespace-nowrap text-fs-0 leading-5 text-fg-3">{pending().sourceBase.toUpperCase()} · {t("editor.issue.generating")}</span>
            </span>
          </div>}
        </Show>
        <For each={props.library?.issues ?? []}>{(issue) =>
          <div class="group flex min-h-12 items-center gap-1 rounded-ui px-1 hover:bg-state-hover"
            classList={{ "bg-state-selected": isSelected(issue.id) }}>
            <button type="button" disabled={!props.enabled} onClick={() => props.onSelect(issue.stack, { issue: issue.id })}
              class="flex min-w-0 flex-1 items-center gap-2 text-left disabled:opacity-50">
              <IssueThumb imageKey={props.thumbKey(`issue:${issue.id}`)} thumbs={props.thumbs} />
              {/*
                文字区固定**两行**（名称 / 基准+日期），两行都是 `leading-5`（20px）——
                右边那列图标于是能与两行一一对齐（崔总 2026-10-09），中间那片也因此更宽。
                第一行**不撑行宽**：宽度由这一列给足，超出部分右端渐隐（`.fade-x-end`，
                没超出时渐隐区是空白，看不出来）；整名仍可以在悬停提示里读到。
              */}
              <span class="flex min-w-0 flex-1 flex-col">
                <span class="fade-x-end block overflow-hidden whitespace-nowrap text-fs-2 leading-5 text-fg-1"
                  title={issue.name}>{issue.name}</span>
                <span class="block overflow-hidden whitespace-nowrap fade-x-end text-fs-0 leading-5 text-fg-3">{issue.sourceBase.toUpperCase()} · {formatDay(issue.createdAt)}</span>
              </span>
            </button>
            {/* 动作列：上=改名（对齐第一行）、下=删除（对齐第二行），与两行文字同高 */}
            <span class="flex shrink-0 flex-col">
              <button type="button" class="grid h-5 w-6 place-items-center rounded-ui text-fg-3 opacity-0 hover:text-fg-1 group-hover:opacity-100"
                aria-label={t("editor.issue.renameTitle")} title={t("editor.issue.renameTitle")}
                disabled={!props.enabled}
                onClick={() => props.onRename(issue)}><IconPencil size={14} /></button>
              <button type="button" class="grid h-5 w-6 place-items-center rounded-ui text-fg-3 opacity-0 hover:text-danger group-hover:opacity-100"
                aria-label={t("editor.issue.delete")} title={t("editor.issue.delete")}
                onClick={(event) => props.onDelete(issue, event)}><IconTrash size={14} /></button>
            </span>
          </div>}
        </For>
        <Show when={(props.library?.issues.length ?? 0) === 0 && props.pending === null}><p class="px-1 py-2 text-fs-0 text-fg-3">{t("editor.issue.empty")}</p></Show>
      </div>

      {/*
        「定稿为新 Issue」：从 toolsbar right 搬进来的（崔总 2026-10-12）——
        贴底、整宽的大按钮，图标与文案与原来一致（`editor.issue.toolbar`）。
      */}
      <div class="shrink-0 px-panel-pad pb-panel-pad pt-2">
        <Button variant="primary" class="w-full" icon={<IconBookmark size={14} />}
          disabled={!props.canFinalize()} onClick={props.onFinalize}>
          {t("editor.issue.toolbar")}
        </Button>
      </div>
    </div>
  );
}

function IssueThumb(props: { imageKey: string; thumbs: ThumbQueue }): JSX.Element {
  /*
   * **旧图留守**：键一变就 `Show` 掉会看到缩略图位闪一下（崔总 2026-10-10：
   * 生成过程中面板在闪）——新图（或新 key 的缓存）就绪前，把上一张留在原位；
   * 只有**换照片**（key 里的 repo/asset 变了）才清空：别的照片的旧图不能顶在格子里。
   */
  const [shown, setShown] = createSignal<{ url: string; approximate: boolean } | null>(null);
  let identity: string | null = null;
  const identityOf = (key: string): string | null => {
    if (key === "") return null;
    try {
      const parsed = JSON.parse(key) as [string, number];
      return `${parsed[0]}:${parsed[1]}`;
    } catch {
      return key; // 坏 key：整串当身份（换掉就清）
    }
  };
  createEffect(() => {
    const key = props.imageKey;
    if (key === "") return;
    props.thumbs.request(key);
    onCleanup(() => props.thumbs.cancel(key));
  });
  createEffect(() => {
    const key = props.imageKey;
    const next = identityOf(key);
    if (next !== identity) {
      identity = next;
      setShown(null);
    }
    const entry = props.thumbs.get(key);
    if (entry.url !== null) {
      setShown({ url: entry.url, approximate: entry.approximate === true });
    }
  });
  return <span class="flex h-10 w-14 shrink-0 overflow-hidden rounded-ui bg-surface-bar">
    <Show when={shown()}>{(value) => <img src={value().url} alt="" class="h-full w-full object-cover"
      classList={{ "photo-raw-approximate": value().approximate }} />}</Show>
  </span>;
}

/* ══════════════════════════════════════════════════════════════
 * 信息页签
 * ══════════════════════════════════════════════════════════════ */

/** 信息页签：只展示拍摄时的原始文件信息，按调节相关项、拍摄与文件分组（内部滚动）。 */
function InfoTab(props: { info: EditorPhotoInfo | null }): JSX.Element {
  const groups = (): {
    titleKey: MessageKey;
    rows: { labelKey: MessageKey; value: string | null }[];
  }[] => {
    const info = props.info;
    if (info === null) return [];
    return [
      {
        titleKey: "editor.info.adjustGroup",
        rows: [
          { labelKey: "editor.info.temperatureBaseline", value: info.temperatureBaseline },
          { labelKey: "editor.info.exposureBias", value: info.exposureBias },
          { labelKey: "editor.info.iso", value: info.iso },
          { labelKey: "editor.info.shutter", value: info.shutter },
          { labelKey: "editor.info.aperture", value: info.aperture },
          { labelKey: "editor.info.focal", value: info.focal },
        ],
      },
      {
        titleKey: "editor.info.shotGroup",
        rows: [
          { labelKey: "editor.info.cameraMake", value: info.cameraMake },
          { labelKey: "editor.info.cameraModel", value: info.cameraModel },
          { labelKey: "editor.info.lens", value: info.lens },
          { labelKey: "editor.info.takenAt", value: info.takenAt },
          { labelKey: "editor.info.datetimeRaw", value: info.datetimeRaw },
          { labelKey: "editor.info.software", value: info.software },
          { labelKey: "editor.info.gps", value: info.gps },
        ],
      },
      {
        titleKey: "editor.info.fileGroup",
        rows: [
          { labelKey: "editor.info.name", value: info.fileName },
          { labelKey: "editor.info.path", value: info.relativePath },
          { labelKey: "editor.info.dimensions", value: info.dimensions },
          { labelKey: "editor.info.orientation", value: info.orientation },
          { labelKey: "editor.info.format", value: info.format },
          { labelKey: "editor.info.kind", value: info.kind },
        ],
      },
    ];
  };

  return (
    <div class="flex h-full min-h-0 flex-col gap-3 overflow-y-auto pr-1" data-editor-info>
      <Show
        when={props.info !== null}
        fallback={<p class="py-2 text-fs-1 text-fg-3">{t("exif.empty")}</p>}
      >
        <For each={groups()}>
          {(group) => (
            <div class="flex flex-col gap-1" data-editor-info-group>
              <span class="text-fs-0 font-semibold text-fg-2">{t(group.titleKey)}</span>
              <MetadataRows rows={group.rows.map((row) => ({ label: t(row.labelKey), value: row.value }))} />
            </div>
          )}
        </For>
        <Show when={(props.info?.tags.length ?? 0) > 0}>
          <div class="flex flex-col gap-1" data-editor-exif-tags>
            <span class="text-fs-0 font-semibold text-fg-2">{t("editor.info.allExif")}</span>
            <MetadataRows rows={(props.info?.tags ?? []).map((tag) => ({ label: tag.ifd + " · " + tag.tag, value: tag.value }))} />
          </div>
        </Show>
      </Show>
    </div>
  );
}
