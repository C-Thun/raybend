import type { ShiftLikeEvent } from "../../lib/easy-destroy.ts";
import type { LensQueryState } from "./lens-query.ts";
/**
 * 编辑右栏：**三个各自独立、同时在场**的页签组（`DESIGN.md` §14.9、`design/editor.md` §2.2）。
 *
 * ```text
 * ┌──────────────────────────┐   第 1 组：总览 · 定稿 · 信息
 * │ [总览][定稿][信息]        │     总览 = 预览 + 直方图；定稿 = issue 列表；信息 = 文件 / 相机 / 尺寸
 * ├──────────────────────────┤
 * │ [影调][色彩][清晰度][镜头] │   第 2 组：调整参数（拉杆）
 * ├──────────────────────────┤
 * │ [曲线]                    │   第 3 组：曲线编辑器
 * └──────────────────────────┘
 * ```
 *
 * 三组**同时在场**（不是一条总页签 + 子页签 —— 人类 2026-09-23 特意纠正过）；
 * 组间只用间距分隔，**不加分隔线**（无边线设计）。页签条直接复用
 * `components/ui/SegmentedControl.tsx`（凹槽 + 主色胶囊，与设计稿同形）。
 *
 * 工具控制块（裁切 / 旋转）出现在**最顶部**（`.pd`：「workspace right顶部出现控制块」），
 * 只在对应工具激活时出现；三工具互斥由 store 保证。
 *
 * 本波的诚实边界（界面上有 `PendingNote` 明说）：
 * 拉杆改数值不改画面（W3）、issue 与编辑栈未落库（W3）、曲线是恒等曲线（W3）、
 * 裁切/旋转的画布草稿由 Rust 渲染线程持有，控制块只传选项与确认。
 */

import { For, Show, createEffect, createMemo, createSignal, onCleanup, type JSX } from "solid-js";
import { IconChevronDown, IconLoader2, IconPencil, IconTrash } from "@tabler/icons-solidjs";

import { Button, IconButton } from "../../components/ui/Button.tsx";
import { MetadataRows } from "../../components/ui/MetadataRows.tsx";
import type { ExifTag } from "../../api/types.ts";
import type { DevelopStack } from "../../api/editor.ts";
import type { IssueLibrary, Issue } from "../../api/issues.ts";
import { SegmentedControl } from "../../components/ui/SegmentedControl.tsx";
import { HistogramPanel } from "../../components/ui/HistogramPanel.tsx";
import { Switch } from "../../components/ui/Form.tsx";
import { Menu } from "../../components/ui/Menu.tsx";
import { Dialog } from "../../components/ui/Dialog.tsx";
import { appliedLensProfile, chosenLensProfile, searchLensProfiles, suggestedLensProfiles } from "./lens-options.ts";
import { t } from "../../i18n/index.ts";
import type { MessageKey } from "../../i18n/index.ts";
import type { LensMatch } from "../../api/types.ts";
import type { BaseCurveLibrary } from "../../api/editor.ts";
import { CompactChoice } from "../../components/ui/CompactChoice.tsx";
import { HISTOGRAM_SAMPLES, type HistogramCounts } from "../../lib/histogram.ts";
import type { ThumbQueue } from "../../components/ui/thumb-queue.ts";
import type { ViewerPhoto } from "../../components/ui/viewer/index.ts";
import { PreviewFrame } from "../../components/ui/PreviewFrame.tsx";
import {
  ANGLE_SPEC,
  CROP_RATIOS,
  GROUP_LABEL_KEY,
  PARAM_GROUPS,
  cropRatioLabel,
  paramsInGroup,
  type ParamGroup,
} from "./params.ts";
import { PendingNote } from "./parts.tsx";
import { SliderRow } from "./SliderRow.tsx";
import type { EditorStore } from "./store.ts";
import { CurveEditor } from "./CurveEditor.tsx";
import { EditorZoomControl } from "./zoom-control.tsx";

/* ══════════════════════════════════════════════════════════════
 * 入参形状
 * ══════════════════════════════════════════════════════════════ */

/**
 * 「信息」页签要的字段（**结构类型**，由工作区拼好传进来）。
 *
 * 为什么不让本模块自己去查：`features/*` 之间不许互相 import（分层规则），
 * 而照片数据属于浏览那一侧 —— 工作区把两块接起来正是它的职责（`ARCHITECTURE.md` §2）。
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

export interface EditorPanelsProps {
  store: EditorStore;
  /** 有没有可以编辑的照片（空态下整列控件禁用） */
  enabled: boolean;
  /** 当前那张（胶片带的锚点） */
  current: ViewerPhoto | null;
  /** 信息行（没有选中就是 `null`） */
  info: EditorPhotoInfo | null;
  lensQuery: LensQueryState;
  onRefreshLens: () => void;
  baseCurveLibrary: BaseCurveLibrary | null;
  issues: IssueLibrary | null;
  issueFocusTick: number;
  onSelectIssue: (stack: DevelopStack) => void;
  onDeleteIssue: (issue: Issue, event: ShiftLikeEvent) => void;
  loadIssueThumb: (issueId: number) => Promise<Uint8Array | null>;
  onSelectBaseCurve: (id: string | null) => void;
  onRenameBaseCurve: (id: string, name: string) => Promise<void>;
  /** 总览专用 Screen 全图队列；胶片带继续使用 grid 小图。 */
  overviewImages: ThumbQueue;
  /** 直方图取数（Rust 算的；注入进来，本模块不碰 `api`） */
  loadHistogram: (path: string, bins: number) => Promise<HistogramCounts | null>;
  /**
   * 参数**松手**了（拖动结束 / 点了重置）—— 工作区拿它落库。
   *
   * 拖动过程中只改画面不落库（`AGENTS.md` 的口径：松手才落库），
   * 所以这里只给一个「可以存了」的信号，具体存什么由 store 的载荷决定。
   */
  onCommit?: () => void;
  onToolConfirm?: () => void;
  /** 落库 / 读库失败的原因（有值就显示一行提示 —— 不静默吞掉） */
  error?: string | null;
  /** 这张照片被二级锁锁住（不可编辑）—— 整列禁用 + 一句话说明 */
  locked?: boolean;
  /** Rust 视口的当前缩放（`null` = 还没有渲染状态）—— 总览页的缩放控制用 */
  zoom?: number | null;
  /** 按倍率缩放一档（工作区把意图发给 Rust） */
  onZoomBy?: (factor: number) => void;
  /** 手动输入的目标缩放（`1.0` = 100%） */
  onZoomTo?: (zoom: number) => void;
  class?: string;
}

/* ══════════════════════════════════════════════════════════════
 * 组件
 * ══════════════════════════════════════════════════════════════ */

type ViewTab = "view" | "issues" | "info";
type CurveTab = "curve";

export function EditorPanels(props: EditorPanelsProps): JSX.Element {
  let scrollHost: HTMLDivElement | undefined;
  createEffect(() => {
    const tool = props.store.tool();
    if (tool === "crop" || tool === "rotate") scrollHost?.scrollTo({ top: 0, behavior: "instant" });
  });
  const [viewTab, setViewTab] = createSignal<ViewTab>("view");
  createEffect(() => { if (props.issueFocusTick > 0) { setViewTab("issues"); scrollHost?.scrollTo({ top: 0, behavior: "instant" }); } });
  const [paramTab, setParamTab] = createSignal<ParamGroup>("tone");
  const [curveTab] = createSignal<CurveTab>("curve");

  return (
    <div
      ref={scrollHost}
      data-editor-panels
      class={[
        /* 滚动条落在预留空间里；横向 padding 走密度令牌（与 browse 右栏同一口径） */
        "flex min-h-0 flex-1 scroll-y-reserved flex-col gap-3 bg-surface-main pl-panel-pad pr-panel-pad-scroll py-panel-pad",
        props.class ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
    >
      {/* 工具控制块（裁切 / 旋转）——只在工具激活时出现，三工具互斥 */}
      <Show when={props.store.tool() === "crop"}>
        <CropBlock store={props.store} enabled={props.enabled} onConfirm={props.onToolConfirm} />
      </Show>
      <Show when={props.store.tool() === "rotate"}>
        <RotateBlock store={props.store} enabled={props.enabled} onConfirm={props.onToolConfirm} />
      </Show>

      {/* ── 第 1 组：总览 / 定稿 / 信息 ───────────────────── */}
      <section class="flex flex-col gap-2" data-editor-group="view">
        <SegmentedControl
          value={viewTab()}
          onValueChange={(value) => setViewTab(value as ViewTab)}
          label={t("editor.group.view")}
          options={[
            { value: "view", label: t("editor.group.view") },
            { value: "issues", label: t("editor.group.issues") },
            { value: "info", label: t("editor.group.info") },
          ]}
        />
        <Show when={viewTab() === "view"}>
          <OverviewTab
            current={props.current}
            overviewImages={props.overviewImages}
            loadHistogram={props.loadHistogram}
            store={props.store}
            enabled={props.enabled}
            zoom={props.zoom ?? null}
            {...(props.onZoomBy === undefined ? {} : { onZoomBy: props.onZoomBy })}
            {...(props.onZoomTo === undefined ? {} : { onZoomTo: props.onZoomTo })}
          />
        </Show>
        <Show when={viewTab() === "issues"}>
          <IssuesTab enabled={props.enabled} store={props.store} library={props.issues}
            onSelect={props.onSelectIssue} onDelete={props.onDeleteIssue} loadThumb={props.loadIssueThumb} />
        </Show>
        <Show when={viewTab() === "info"}>
          <InfoTab info={props.info} />
        </Show>
      </section>

      {/* ── 第 2 组：影调 / 色彩 / 清晰度 / 镜头 ───────────── */}
      <section class="flex flex-col gap-2" data-editor-group="params">
        <SegmentedControl
          value={paramTab()}
          onValueChange={(value) => setParamTab(value as ParamGroup)}
          label={t("editor.group.tone")}
          options={PARAM_GROUPS.map((group) => ({
            value: group,
            label: t(GROUP_LABEL_KEY[group]),
          }))}
        />
        <div class="flex flex-col gap-2.5">
          {/* 清晰度页签多一块：**降噪方式**（快速 / 高质量）—— 人类 2026-09-25 拍板
              「两档都做，先保底再移植」：快速档实时跟手，高质量档在后台跑（见 `specs/M3-W4.md` §2.1） */}
          <Show when={paramTab() === "detail"}>
            <div class="flex items-center justify-between gap-2 pb-0.5">
              <span class="text-fs-0 text-fg-2">{t("editor.detail.nrMethod")}</span>
              <CompactChoice
                value={props.store.nrMethod() ?? "fast"}
                label={t("editor.detail.nrMethod")}
                options={[
                  { value: "fast", label: t("editor.detail.nrFast") },
                  { value: "high", label: t("editor.detail.nrHigh") },
                ]}
                onValueChange={(value) => {
                  props.store.setNrMethod(value === "high" ? "high" : null);
                  props.onCommit?.();
                }}
              />
            </div>
          </Show>
          <Show when={paramTab() === "detail"}>
            <Show
              when={props.store.nrMethod() === "high" && props.store.renderState()?.nrPending}
              fallback={<p class="text-fs-0 text-fg-3" role="status">{props.store.nrMethod() !== "high"
                ? t("editor.detail.nrFastHint")
                : props.store.renderState()?.nrError ? t("editor.detail.nrFailed") : t("editor.detail.nrHighHint")}</p>}
            >
              <div data-high-denoise-pending role="status" aria-live="polite"
                class="flex items-center gap-2 rounded-ui border border-brand-2 bg-brand-2 px-2 py-1.5 text-fs-1 font-semibold text-fg-on-brand">
                <IconLoader2 size={15} class="shrink-0 animate-spin" aria-hidden="true" />
                <span>{t("editor.detail.nrPending")}</span>
              </div>
            </Show>
          </Show>
          {/* 镜头页签多两块非拉杆控件（`.pd`：配置文件 + 启用校正开关） */}
          <Show when={paramTab() === "lens"}>
            <LensExtras store={props.store} enabled={props.enabled} onCommit={props.onCommit}
              queryState={props.lensQuery} onRefresh={props.onRefreshLens} />
          </Show>
          <For each={paramsInGroup(paramTab())}>
            {(spec) => (
              <SliderRow
                spec={spec}
                value={props.store.paramValue(spec.id)}
                /* 参数表的 wired 字段与 Rust 契约保持一致。 */
                disabled={!props.enabled || !spec.wired}
                onValueChange={(value) => props.store.setParam(spec.id, value)}
                onValueCommit={() => props.onCommit?.()}
                onReset={() => {
                  props.store.endParamDrag();
                  props.store.resetParam(spec.id);
                  props.onCommit?.();
                }}
                /* 拖动中只算预览档、松手补全尺寸（人类 2026-09-24，`store.beginParamDrag`） */
                onDragStart={() => props.store.beginParamDrag()}
                onDragEnd={() => props.store.endParamDrag()}
              />
            )}
          </For>
        </div>
        <Show when={props.error != null || props.locked === true}>
          <p class="text-fs-0 leading-snug text-danger" data-editor-develop-error>
            {props.error ?? t("editor.panel.locked")}
          </p>
        </Show>
      </section>

      {/* ── 第 3 组：曲线 ─────────────────────────────────── */}
      <section class="flex flex-col gap-2" data-editor-group="curve">
        <SegmentedControl
          value={curveTab()}
          onValueChange={() => undefined}
          label={t("editor.group.curve")}
          options={[{ value: "curve", label: t("editor.group.curve") }]}
        />
        <CurveTab
          store={props.store}
          enabled={props.enabled}
          current={props.current}
          loadHistogram={props.loadHistogram}
          onCommit={props.onCommit}
          onRenameBaseCurve={props.onRenameBaseCurve}
          baseCurveLibrary={props.baseCurveLibrary}
          onSelectBaseCurve={props.onSelectBaseCurve}
        />
      </section>
    </div>
  );
}

/* ══════════════════════════════════════════════════════════════
 * 第 1 组的三块内容
 * ══════════════════════════════════════════════════════════════ */

function OverviewTab(props: {
  current: ViewerPhoto | null;
  overviewImages: ThumbQueue;
  loadHistogram: (path: string, bins: number) => Promise<HistogramCounts | null>;
  store: EditorStore;
  enabled: boolean;
  zoom: number | null;
  onZoomBy?: (factor: number) => void;
  onZoomTo?: (zoom: number) => void;
}): JSX.Element {
  const path = (): string | null => props.current?.path ?? null;
  const thumb = (): string | null => {
    const file = path();
    if (file === null) return null;
    props.overviewImages.request(file, true);
    return props.overviewImages.get(file).url;
  };

  return (
    <>
      {/*
        预览与浏览右栏共用 `PreviewFrame`，图源走完整 Screen 档（长边 ≤1920）。
        胶片带的 grid 小图会按 3:1 裁切，不能拿来做全图总览。
      */}
      <PreviewFrame
        src={thumb()}
        natural={props.current?.natural ?? null}
        emptyText={path() === null ? t("editor.empty.noSelection") : t("common.loading")}
      />
      {/* 缩放控制仍在总览图片下方，按用户本轮要求不移动。 */}
      <EditorZoomControl
        zoom={props.zoom}
        enabled={props.enabled}
        onZoomBy={(factor) => props.onZoomBy?.(factor)}
        onZoomTo={(zoom) => props.onZoomTo?.(zoom)}
      />
      <HistogramPanel
        load={props.loadHistogram}
        path={path()}
        countsOverride={props.store.renderState() === null
          ? undefined
          : props.store.renderState()?.histogram ?? null}
        title={t("browse.histogram")}
        emptyText={t("browse.histogramEmpty")}
      />
    </>
  );
}

/** 定稿页签：选中态来自当前配置哈希；用户定稿永不覆盖。 */
function IssuesTab(props: { enabled: boolean; store: EditorStore; library: IssueLibrary | null;
  onSelect: (stack: DevelopStack) => void; onDelete: (issue: Issue, event: ShiftLikeEvent) => void;
  loadThumb: (issueId: number) => Promise<Uint8Array | null> }): JSX.Element {
  const isSelected = (kind: "sooc" | "raw" | "latest" | number): boolean => {
    const selected = props.library?.selection;
    return typeof kind === "number" ? typeof selected === "object" && selected.issue === kind : selected === kind;
  };
  const source = (base: "sooc" | "raw"): DevelopStack => ({
    sourceBase: base, values: {}, curves: {}, asShotK: props.store.asShotTemperature(),
    lensProfile: null, lensEnabled: null, baseCurveProfile: null, baseCurvePoints: null,
    lutId: null, lutEnabled: null, nrMethod: null, geometry: null,
  });
  return <div class="flex flex-col gap-1" data-editor-issues>
    <For each={(["sooc", "raw"] as const)}>{(base) =>
      <button type="button" disabled={!props.enabled || !(base === "raw" ? props.store.editBaseAvailable().raw : props.store.editBaseAvailable().bitmap)}
        class="flex min-h-12 items-center gap-2 rounded-ui px-1 text-left hover:bg-state-hover disabled:opacity-50"
        classList={{ "bg-state-selected": isSelected(base) }} onClick={() => props.onSelect(source(base))}>
        <span class="h-10 w-14 shrink-0 rounded-ui bg-surface-bar" />
        <span class="min-w-0 flex-1"><span class="block text-fs-2 text-fg-1">{base.toUpperCase()}</span>
          <span class="block text-fs-0 text-fg-3">{t(base === "raw" ? "editor.issue.rawHint" : "editor.issue.soocHint")}</span></span>
      </button>}
    </For>
    <div class="flex min-h-12 items-center gap-2 rounded-ui px-1" classList={{ "bg-state-selected": isSelected("latest") }}>
      <span class="h-10 w-14 shrink-0 rounded-ui bg-surface-bar" />
      <span class="min-w-0 flex-1"><span class="block text-fs-2 text-fg-1">{t("editor.issue.latest")}</span>
        <span class="block text-fs-0 text-fg-3">{t("editor.issue.latestHint")}</span></span>
    </div>
    <For each={props.library?.issues ?? []}>{(issue) =>
      <div class="group flex min-h-12 items-center gap-1 rounded-ui px-1 hover:bg-state-hover"
        classList={{ "bg-state-selected": isSelected(issue.id) }}>
        <button type="button" disabled={!props.enabled} onClick={() => props.onSelect(issue.stack)}
          class="flex min-w-0 flex-1 items-center gap-2 text-left disabled:opacity-50">
          <IssueThumb issueId={issue.id} load={props.loadThumb} />
          <span class="min-w-0 flex-1"><span class="block truncate text-fs-2 text-fg-1">{issue.name}</span>
            <span class="block text-fs-0 text-fg-3">{issue.sourceBase.toUpperCase()} · {new Date(issue.createdAt).toLocaleString()}</span></span>
        </button>
        <button type="button" class="opacity-0 group-hover:opacity-100 text-fg-3 hover:text-danger"
          aria-label={t("editor.issue.delete")} title={t("editor.issue.delete")}
          onClick={(event) => props.onDelete(issue, event)}><IconTrash size={14} /></button>
      </div>}
    </For>
    <Show when={(props.library?.issues.length ?? 0) === 0}><p class="px-1 py-2 text-fs-0 text-fg-3">{t("editor.issue.empty")}</p></Show>
  </div>;
}

function IssueThumb(props: { issueId: number; load: (id: number) => Promise<Uint8Array | null> }): JSX.Element {
  const [url, setUrl] = createSignal<string | null>(null);
  createEffect(() => {
    const id = props.issueId;
    let active = true;
    void props.load(id).then((bytes) => {
      if (!active || bytes === null) return;
      const next = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: "image/avif" }));
      setUrl((previous) => { if (previous !== null) URL.revokeObjectURL(previous); return next; });
    }).catch(() => undefined);
    onCleanup(() => { active = false; const previous = url(); if (previous !== null) URL.revokeObjectURL(previous); });
  });
  return <span class="flex h-10 w-14 shrink-0 overflow-hidden rounded-ui bg-surface-bar">
    <Show when={url()}>{(src) => <img src={src()} alt="" class="h-full w-full object-cover" />}</Show>
  </span>;
}

/** 信息页签：只展示拍摄时的原始文件信息，按调节相关项、拍摄与文件分组。 */
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
    <div class="flex max-h-96 flex-col gap-3 overflow-y-auto pr-1" data-editor-info>
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

/** 镜头页签的两块非拉杆控件（配置文件 + 启用校正）。
 *
 * 口径（人类 2026-09-25 拍板）：**配置文件负责自动/库校正，手动拉杆是微调叠加其上**；
 * 「启用校正」开关**只管配置文件那一半** —— 关掉它之后手动拉杆照常生效
 * （与 Lightroom 的 Lens Corrections 面板一致）。 */
function LensExtras(props: { store: EditorStore; enabled: boolean; onCommit?: () => void;
  queryState: LensQueryState; onRefresh: () => void }): JSX.Element {
  const [open, setOpen] = createSignal(false);
  const [query, setQuery] = createSignal("");
  const match = (): LensMatch | null => props.queryState.data;
  const chosen = (): string | null => props.store.lensProfile();
  const selected = () => chosenLensProfile(match(), chosen());
  const profileLabel = () => {
    const profile = selected();
    return profile === null ? t("editor.lens.choose") : `${profile.maker} ${profile.model}`;
  };
  const applied = () => appliedLensProfile(match(), chosen(), props.store.lensEnabled() ?? true);
  const appliedLabel = () => {
    const state = applied();
    if (state.mode === "disabled") return t("editor.lens.disabled");
    if (state.profile === null) return t("editor.lens.activeNone");
    return t("editor.lens.activeManual", { profile: `${state.profile.maker} ${state.profile.model}` });
  };
  const choose = (key: string): void => {
    props.store.setLensProfile(key);
    if (key !== "none") props.store.setLensEnabled(true);
    props.onCommit?.();
    setOpen(false);
  };
  const profiles = () => {
    if (match() === null) return [];
    const suggested = new Set(query().trim() === "" ? suggestions().map((p) => p.key) : []);
    return searchLensProfiles(match()!, query()).filter((p) => !suggested.has(p.key));
  };
  const suggestions = () => match() === null ? [] : suggestedLensProfiles(match()!);
  const row = (key: string, text: string): JSX.Element => (
    <button type="button" class="w-full rounded-ui px-2 py-1.5 text-left text-fs-1 text-fg-1 hover:bg-state-hover"
      aria-pressed={chosen() === key || (chosen() === null && key === "none")}
      onClick={() => choose(key)}>{text}</button>
  );
  const requestStatus = (): JSX.Element => <>
    <Show when={props.queryState.status === "loading"}>
      <p role="status" class="flex items-center gap-1 text-fs-0 text-fg-2" data-lens-loading>
        <IconLoader2 size={13} class="animate-spin" />{t("editor.lens.loading")}
      </p>
    </Show>
    <Show when={props.queryState.status === "error"}>
      <div role="alert" class="flex flex-col gap-1" data-lens-error>
        <p class="break-words text-fs-0 text-danger">{t(props.queryState.failure === "timeout"
          ? "editor.lens.timeout" : props.queryState.failure === "unavailable"
          ? "editor.lens.unavailable" : "editor.lens.failed")}</p>
        <Show when={props.queryState.error}><p class="break-words text-fs-0 text-fg-2">{props.queryState.error}</p></Show>
        <Button variant="secondary" onClick={props.onRefresh}>{t("editor.lens.retry")}</Button>
      </div>
    </Show>
    <Show when={props.queryState.status === "ready" && match()?.detected === null}>
      <p role="status" class="text-fs-0 text-fg-2">{t(match()?.lensName ? "editor.lens.notFound" : "editor.lens.noExif")}</p>
    </Show>
    <For each={match()?.warnings ?? []}>{warning => <p class="break-words text-fs-0 text-fg-2">{warning}</p>}</For>
  </>;
  return (
    <div class="flex flex-col gap-2 pt-1">
      <div class="flex flex-col gap-1">
        <span class="text-fs-0 text-fg-2">{t("editor.lens.profile")}</span>
        <p class="break-words text-fs-0 text-fg-2" role="status" data-lens-applied>{appliedLabel()}</p>
        <Button variant="secondary" disabled={!props.enabled} class="h-auto min-h-8 w-full justify-between py-1.5"
          onClick={() => { setQuery(""); setOpen(true); props.onRefresh(); }}>
          <span class="min-w-0 whitespace-normal break-words text-left">{profileLabel()}</span>
        </Button>
        <Show when={!open()}>{requestStatus()}</Show>
      </div>
      <p class="text-fs-0 text-fg-3">{t("editor.lens.manualHint")}</p>
      <Switch checked={props.store.lensEnabled() ?? true}
        onCheckedChange={(value) => { props.store.setLensEnabled(value); props.onCommit?.(); }}
        disabled={!props.enabled} label={t("editor.lens.enable")} />
      <Dialog open={open()} onOpenChange={setOpen} title={t("editor.lens.choose")}>
        <div class="flex flex-col gap-2">
          {requestStatus()}
          <input type="search" value={query()} onInput={(event) => setQuery(event.currentTarget.value)}
            placeholder={t("editor.lens.search")} aria-label={t("editor.lens.search")}
            class="h-8 w-full rounded-ui bg-surface-track px-2 text-fs-1 text-fg-1 outline-none" />
          <div class="max-h-64 overflow-y-auto" data-lens-results>
            {row("none", t("editor.lens.profileNone"))}
            <Show when={query().trim() === "" && suggestions().length > 0}>
              <p class="px-2 py-1 text-fs-0 font-semibold text-fg-2">{t("editor.lens.suggestions")}</p>
              <For each={suggestions()}>{(profile) => row(profile.key, `${profile.maker} ${profile.model}`)}</For>
            </Show>
            <Show when={match() !== null}>
              <p class="px-2 py-1 text-fs-0 font-semibold text-fg-2">{t("editor.lens.results")}</p>
              <For each={profiles()}>{(profile) => row(profile.key, `${profile.maker} ${profile.model}`)}</For>
              <Show when={props.queryState.status === "ready" && profiles().length === 0 && (query().trim() !== "" || suggestions().length === 0)}>
                <p class="px-2 py-2 text-fs-0 text-fg-3">{t("editor.lens.noResults")}</p>
              </Show>
            </Show>
          </div>
        </div>
      </Dialog>
    </div>
  );
}

/* ══════════════════════════════════════════════════════════════
 * 第 3 组：曲线
 * ══════════════════════════════════════════════════════════════ */

/**
 * 曲线（W1：**恒等曲线**的静态示意）。
 *
 * 真正的编辑器（控制点、通道、预设）在 W3 与管线一起做 —— 现在画一条对角线，
 * 是为了让页签组有形状、也让「加了曲线之后会是这么大一块」这件事可评审。
 * 见 `design/editor.md` §7 的说明：曲线是**唯一自研**的控件（Ark 没有对应件）。
 */
function CurveTab(props: {
  store: EditorStore;
  enabled: boolean;
  current: ViewerPhoto | null;
  loadHistogram: (path: string, bins: number) => Promise<HistogramCounts | null>;
  onCommit?: () => void;
  onToolConfirm?: () => void;
  baseCurveLibrary: BaseCurveLibrary | null;
  onSelectBaseCurve: (id: string | null) => void;
  onRenameBaseCurve: (id: string, name: string) => Promise<void>;
}): JSX.Element {
  const [baseOpen, setBaseOpen] = createSignal(false);
  const selectedBase = () => props.baseCurveLibrary?.profiles.find((profile) => profile.id === props.store.baseCurveProfile());
  const baseLabel = () => selectedBase()?.name ?? t(props.store.baseCurveProfile() === "none"
    ? "editor.baseCurve.none" : "editor.baseCurve.empty");
  const [renameTarget, setRenameTarget] = createSignal<string | null>(null);
  const [renameName, setRenameName] = createSignal("");
  const [renameBusy, setRenameBusy] = createSignal(false);
  const [renameError, setRenameError] = createSignal<string | null>(null);
  let renameRevision = 0;
  const closeRename = (): void => {
    renameRevision++;
    setRenameTarget(null);
    setRenameBusy(false);
    setRenameError(null);
  };
  const validName = (): boolean => {
    const name = renameName().trim();
    return name.length > 0 && Array.from(name).length <= 80 && !/[\u0000-\u001f\u007f-\u009f]/u.test(name);
  };
  const profileContext = createMemo(() => JSON.stringify([
    props.current?.path, props.store.editBase(),
    props.baseCurveLibrary?.cameraMake, props.baseCurveLibrary?.cameraModel,
  ]));
  createEffect(() => {
    profileContext();
    closeRename();
    setBaseOpen(false);
  });
  const rename = async (): Promise<void> => {
    const id = renameTarget();
    if (id === null || renameBusy() || !validName() || !props.enabled) return;
    const revision = ++renameRevision;
    setRenameBusy(true);
    setRenameError(null);
    try {
      await props.onRenameBaseCurve(id, renameName().trim());
      if (revision === renameRevision) closeRename();
    } catch (error) {
      if (revision === renameRevision) setRenameError(String(error));
    } finally {
      if (revision === renameRevision) setRenameBusy(false);
    }
  };
  /*
   * 背景那层直方图：与总览页签**同一个取数口**（`image_histogram`，Rust 算的）。
   *
   * Tauri 运行时直接复用 GPU 显影线程随真帧算出的直方图；浏览器设计预览
   * 没有显影线程，才退回共享的文件直方图口。
   */
  const [histogram, setHistogram] = createSignal<HistogramCounts | null>(null);
  createEffect(() => {
    if (props.store.renderState() !== null) return;
    const path = props.current?.path ?? null;
    if (path === null) {
      setHistogram(null);
      return;
    }
    let cancelled = false;
    void props
      .loadHistogram(path, HISTOGRAM_SAMPLES)
      .then((counts) => {
        if (!cancelled) setHistogram(counts);
      })
      .catch(() => {
        // 直方图取不到只是背景少一层：不弹错、不挡操作
        if (!cancelled) setHistogram(null);
      });
    onCleanup(() => {
      cancelled = true;
    });
  });

  return (
    <div class="flex flex-col gap-2">
    <CurveEditor
      store={props.store}
      histogram={props.store.renderState() === null
        ? histogram()
        : props.store.renderState()?.histogram ?? null}
      disabled={!props.enabled}
      onCommit={props.onCommit}
      onDragStart={() => props.store.beginParamDrag()}
      onDragEnd={() => props.store.endParamDrag()}
    />
    <Show when={props.store.editBase() === "raw" && props.baseCurveLibrary?.cameraMake && props.baseCurveLibrary?.cameraModel}>
      <div class="flex flex-col gap-1" data-base-curve-selector>
        <span class="text-fs-0 text-fg-2">{t("editor.baseCurve.label")}</span>
        <Button variant="secondary" disabled={!props.enabled || props.store.autoAdjusting()} class="h-auto min-h-8 w-full justify-between py-1.5"
          onClick={() => setBaseOpen(true)}>{baseLabel()}</Button>
        <p class="text-fs-0 text-fg-3">{t("editor.baseCurve.cameraOnly")}</p>
      </div>
      <Dialog open={baseOpen()} onOpenChange={setBaseOpen} title={t("editor.baseCurve.label")}>
        <div class="max-h-64 overflow-y-auto" data-base-curve-results>
          <button type="button" class="w-full rounded-ui px-2 py-1.5 text-left text-fs-1 text-fg-1 hover:bg-state-hover"
            aria-pressed={props.store.baseCurveProfile() === "none"}
            onClick={() => { props.onSelectBaseCurve("none"); setBaseOpen(false); }}>
            {t("editor.baseCurve.none")}</button>
          <Show when={(props.baseCurveLibrary?.profiles.length ?? 0) === 0}>
            <p class="px-2 py-2 text-fs-0 text-fg-3">{t("editor.baseCurve.noProfiles")}</p>
          </Show>
          <For each={props.baseCurveLibrary?.profiles ?? []}>{profile =>
            <div class="flex items-center gap-1 rounded-ui hover:bg-state-hover">
              <button type="button" class="min-w-0 flex-1 truncate rounded-ui px-2 py-1.5 text-left text-fs-1 text-fg-1"
                disabled={!props.enabled || props.store.autoAdjusting()}
                aria-pressed={props.store.baseCurveProfile() === profile.id}
                onClick={() => { props.onSelectBaseCurve(profile.id); setBaseOpen(false); }}>
                {profile.name}</button>
              <IconButton label={t("editor.baseCurve.rename")} disabled={!props.enabled || props.store.autoAdjusting()}
                onClick={() => { setBaseOpen(false); setRenameName(profile.name); setRenameError(null); setRenameTarget(profile.id); }}>
                <IconPencil size={14} aria-hidden="true" />
              </IconButton>
            </div>
          }</For>
        </div>
      </Dialog>
      <Dialog open={renameTarget() !== null} onOpenChange={(open) => { if (!open) closeRename(); }}
        title={t("editor.baseCurve.rename")}
        footer={<>
          <Button variant="secondary" onClick={closeRename}>{t("common.cancel")}</Button>
          <Button variant="primary" disabled={renameBusy() || !validName() || !props.enabled} onClick={() => void rename()}>{t("common.save")}</Button>
        </>}>
        <input type="text" value={renameName()} maxLength={160} autofocus
          aria-label={t("editor.baseCurve.rename")} disabled={renameBusy()}
          class="h-9 w-full rounded-ui bg-surface-track px-2 text-fs-1 text-fg-1 outline-none"
          onInput={(event) => setRenameName(event.currentTarget.value)}
          onKeyDown={(event) => { if (event.key === "Enter" && !event.isComposing) { event.preventDefault(); void rename(); } }} />
        <Show when={renameError()}><p role="alert" class="mt-2 text-fs-0 text-fg-2">{renameError()}</p></Show>
      </Dialog>
    </Show>
    </div>
  );
}

/* ══════════════════════════════════════════════════════════════
 * 工具控制块
 * ══════════════════════════════════════════════════════════════ */

/** 裁切控制块（`.pd` 的六件套：比例下拉 / 反转 / 取消比例 / 横纵输入 / 取消 + 确认）。 */
function CropBlock(props: { store: EditorStore; enabled: boolean; onConfirm?: () => void }): JSX.Element {
  const ratioId = (): string => props.store.cropRatioId();

  return (
    <section
      data-editor-tool-block="crop"
      class="flex flex-col gap-2 rounded-ui bg-surface-track p-2"
    >
      <div class="grid grid-cols-2 items-center gap-x-2 gap-y-1">
        <Menu
          items={CROP_RATIOS.map((item) => ({
            value: item.id, label: cropRatioLabel(item, t), selected: item.id === ratioId(),
          })).concat([{ value: "custom", label: t("editor.crop.custom"), selected: ratioId() === "custom" }])}
          onSelect={(value) => props.store.setCropRatioId(value)}
          label={t("editor.crop.ratio")}
          placement="bottom-start"
        >
          {(triggerProps) => (
            <Button {...triggerProps()} variant="ghost" disabled={!props.enabled}
              aria-label={`${t("editor.crop.ratio")}: ${ratioLabelOf(ratioId(), t)}`}
              class="h-row-h w-full min-w-0 justify-between bg-surface-layer text-fg-1">
              <span class="min-w-0 truncate">{ratioLabelOf(ratioId(), t)}</span>
              <IconChevronDown size={14} aria-hidden="true" class="shrink-0 text-fg-2" />
            </Button>
          )}
        </Menu>
        <label class="flex min-w-0 items-center gap-1">
          <span class="w-10 shrink-0 text-right text-fs-0 text-fg-2">{t("editor.crop.width")}</span>
          <input type="number" min={0.01} step="any"
            class="h-row-h min-w-0 flex-1 rounded-ui bg-surface-bar px-2 text-fs-1 tabular-nums text-fg-1 outline-none"
            value={props.store.cropWidth()} disabled={!props.enabled}
            onInput={(event) => props.store.setCropSize(Number(event.currentTarget.value), props.store.cropHeight())}
            onChange={(event) => { event.currentTarget.value = String(props.store.cropWidth()); }} />
        </label>
        <label class="col-start-2 row-start-2 flex min-w-0 items-center gap-1">
          <span class="w-10 shrink-0 text-right text-fs-0 text-fg-2">{t("editor.crop.height")}</span>
          <input type="number" min={0.01} step="any"
            class="h-row-h min-w-0 flex-1 rounded-ui bg-surface-bar px-2 text-fs-1 tabular-nums text-fg-1 outline-none"
            value={props.store.cropHeight()} disabled={!props.enabled}
            onInput={(event) => props.store.setCropSize(props.store.cropWidth(), Number(event.currentTarget.value))}
            onChange={(event) => { event.currentTarget.value = String(props.store.cropHeight()); }} />
        </label>
        <div class="col-start-1 row-start-2 flex h-row-h items-center gap-1">
          <Button variant="ghost" disabled={!props.enabled}
            aria-label={t("editor.crop.flip")} title={t("editor.crop.flip")}
            icon={<span aria-hidden="true">⇄</span>} onClick={() => props.store.flipCropRatio()} />
          <Button variant="ghost" disabled={!props.enabled}
            aria-label={t("editor.crop.unlink")} title={t("editor.crop.unlink")}
            icon={<span aria-hidden="true">⛓</span>} onClick={() => props.store.unlinkCropRatio()} />
        </div>
      </div>

      {/* 取消在左、确认在右（`AGENTS.md` §11.5，全系统统一） */}
      <div class="flex items-center justify-end gap-1.5">
        <Button variant="secondary" onClick={() => props.store.closeTool()}>
          {t("common.cancel")}
        </Button>
        <Button variant="primary" disabled={!props.enabled} onClick={() => props.onConfirm?.()}>
          {t("common.confirm")}
        </Button>
      </div>
    </section>
  );
}

/** 比例 id → 下拉里那一行的文案（找不到就当「自定义」）。 */
function ratioLabelOf(id: string, translate: (key: MessageKey) => string): string {
  const item = CROP_RATIOS.find((candidate) => candidate.id === id);
  return item === undefined ? translate("editor.crop.custom") : cropRatioLabel(item, translate);
}

/** 旋转控制块（角度拉杆 + 重置 + 拉线提示 + 取消 / 确认）。 */
function RotateBlock(props: { store: EditorStore; enabled: boolean; onConfirm?: () => void }): JSX.Element {
  return (
    <section
      data-editor-tool-block="rotate"
      class="flex flex-col gap-2 rounded-ui bg-surface-track p-2"
    >
      <SliderRow
        spec={ANGLE_SPEC}
        value={props.store.angle()}
        disabled={!props.enabled}
        onValueChange={(value) => props.store.setAngle(value)}
        onReset={() => props.store.resetAngle()}
      />
      <div class="flex items-center justify-between gap-2">
        <PendingNote text={t("editor.rotate.hint")} class="flex-1" />
        <Button
          variant="ghost"
          disabled={!props.enabled}
          onClick={() => props.store.resetAngle()}
        >
          {t("editor.rotate.reset")}
        </Button>
      </div>
      <div class="flex items-center justify-end gap-1.5">
        <Button variant="secondary" onClick={() => props.store.closeTool()}>
          {t("common.cancel")}
        </Button>
        <Button variant="primary" disabled={!props.enabled} onClick={() => props.onConfirm?.()}>
          {t("common.confirm")}
        </Button>
      </div>
    </section>
  );
}
