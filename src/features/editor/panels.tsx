import type { LensQueryState } from "./lens-query.ts";
/**
 * 编辑右栏（2026-10-12 改版，崔总口述；`design/editor.md` §2.2）。
 *
 * ```text
 * ┌──────────────────────────┐   直方图（**固定**在这一栏顶部，不参与滚动）
 * │ 直方图                    │
 * ├──────────────────────────┤   ↓ 以下整体可滚动
 * │ [影调][色彩][清晰度][镜头] │   调整参数（拉杆）
 * ├──────────────────────────┤
 * │ [曲线][预设][色彩管理]     │   曲线 / 预设 / 色彩管理
 * └──────────────────────────┘
 * ```
 *
 * 两个页签组**同时在场**（不是一条总页签 + 子页签 —— 人类 2026-09-23 特意纠正过）；
 * 组间只用间距分隔，**不加分隔线**（无边线设计）。页签条直接复用
 * `components/ui/SegmentedControl.tsx`（凹槽 + 主色胶囊，与设计稿同形）。
 *
 * 原来住在这里的「总览 / 定稿 / 信息」第 1 组已整体搬到**左列**（`left-panel.tsx`）：
 * 总览与信息成为左列底部的页签组，定稿成为 `toolsbar left` 的另一个面板。
 *
 * 工具控制块（裁切 / 旋转）出现在**最顶部**（`.pd`：「workspace right 顶部出现控制块」），
 * 只在对应工具激活时出现（固定在直方图之上，不随滚动跑掉）；三工具互斥由 store 保证。
 *
 * 本波的诚实边界（界面上有 `PendingNote` 明说）：
 * 拉杆改数值不改画面（W3）、曲线是恒等曲线（W3）、
 * 裁切/旋转的画布草稿由 Rust 渲染线程持有，控制块只传选项与确认。
 */

import { For, Show, createEffect, createMemo, createSignal, onCleanup, type JSX } from "solid-js";
import { IconChevronDown, IconExternalLink, IconLoader2, IconPencil } from "@tabler/icons-solidjs";

import { Button, IconButton } from "../../components/ui/Button.tsx";
import { SegmentedControl } from "../../components/ui/SegmentedControl.tsx";
import { HistogramPanel } from "../../components/ui/HistogramPanel.tsx";
import { Switch } from "../../components/ui/Form.tsx";
import { ProfileSelect } from "../../components/ui/ProfileSelect.tsx";
import { DisplayColorStatus } from "../../components/ui/DisplayColorStatus.tsx";
import type { ColorProfileEntry } from "../../api/color.ts";
import { outputColorChoice, outputColorFromChoice } from "../../lib/color-model.ts";
import { Menu } from "../../components/ui/Menu.tsx";
import { Dialog } from "../../components/ui/Dialog.tsx";
import { appliedLensProfile, chosenLensProfile, searchLensProfiles, suggestedLensProfiles } from "./lens-options.ts";
import { t } from "../../i18n/index.ts";
import type { MessageKey } from "../../i18n/index.ts";
import type { LensMatch } from "../../api/types.ts";
import type { BaseCurveLibrary } from "../../api/editor.ts";
import { CompactChoice } from "../../components/ui/CompactChoice.tsx";
import { HISTOGRAM_SAMPLES, type HistogramCounts } from "../../lib/histogram.ts";
import type { ViewerPhoto } from "../../components/ui/viewer/index.ts";
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
import { selectAllOnMount } from "../../lib/dom-focus.ts";
import { PresetPanel } from "./preset-panel.tsx";
import { SliderRow } from "./SliderRow.tsx";
import type { EditorStore } from "./store.ts";
import type { PresetGroup, PresetMutationOutcome } from "../../lib/presets.ts";
import { CurveEditor } from "./CurveEditor.tsx";

/* ══════════════════════════════════════════════════════════════
 * 入参形状
 * ══════════════════════════════════════════════════════════════ */

/**
 * 「信息」页签要的字段、以及「定稿面板」的类型都在 `left-panel.tsx`
 * （第 1 组整体搬到左列，2026-10-12）。
 */

export interface EditorPanelsProps {
  store: EditorStore;
  /** 有没有可以编辑的照片（空态下整列控件禁用） */
  enabled: boolean;
  /** 当前那张（胶片带的锚点） */
  current: ViewerPhoto | null;
  lensQuery: LensQueryState;
  onRefreshLens: () => void;
  baseCurveLibrary: BaseCurveLibrary | null;
  onSelectBaseCurve: (id: string | null) => void;
  onRenameBaseCurve: (id: string, name: string) => Promise<void>;
  /** 直方图取数（Rust 算的；注入进来，本模块不碰 `api`） */
  loadHistogram: (path: string, bins: number) => Promise<HistogramCounts | null>;
  /**
   * 参数**松手**了（拖动结束 / 点了重置）—— 工作区拿它落库。
   *
   * 拖动过程中只改画面不落库（`AGENTS.md` 的口径：松手才落库），
   * 所以这里只给一个「可以存了」的信号，具体存什么由 store 的载荷决定。
   */
  onConfirm?: () => void;
  onToolConfirm?: () => void;
  /** 落库 / 读库失败的原因（有值就显示一行提示 —— 不静默吞掉） */
  error?: string | null;
  /** 这张照片被二级锁锁住（不可编辑）—— 整列禁用 + 一句话说明 */
  locked?: boolean;
  /* ── 预设（specs/editor-presets.md）：持久化动作由工作区注入 ── */
  onCreatePresetDirectory: (name: string) => Promise<PresetMutationOutcome>;
  onCreatePreset: (name: string, directoryId: string, groups: readonly PresetGroup[]) => Promise<PresetMutationOutcome>;
  onRenamePresetDirectory: (id: string, name: string) => Promise<PresetMutationOutcome>;
  onDeletePreset: (id: string) => Promise<void>;
  onDeletePresetDirectory: (id: string) => Promise<void>;
  onMovePresets: (ids: readonly string[], directoryId: string) => Promise<void>;
  onOpenColorProfiles: () => void;
  profileEntries: readonly ColorProfileEntry[];
  onRefreshProfiles: () => void;
  onApplyColor: (profileId: string | null) => Promise<void>;
  colorBatchCount:number;
  onReviewColorBatch:(profileId:string|null)=>Promise<void>;
  onApplyPreset: (snapshot: import("../../lib/presets.ts").PresetSnapshot) => Promise<void>;
  class?: string;
}

/* ══════════════════════════════════════════════════════════════
 * 组件
 * ══════════════════════════════════════════════════════════════ */

type CurveTab = "curve" | "preset" | "color";

/**
 * 第 1 组（总览 / 定稿 / 信息）的等高规则已随 2026-10-12 改版作废：
 * 总览与信息成了左列底部的页签组（`left-panel.tsx`，高度规则在 `layout.ts`），
 * 定稿成了 toolsbar left 的另一个面板。右栏现在只有「直方图（固定）+ 可滚动参数区」。
 */

export function EditorPanels(props: EditorPanelsProps): JSX.Element {
  /*
   * 右栏**不许自己跳顶**（崔总 2026-09-28，点名过两次）：点裁切/旋转只是把工具块
   * 插在这一栏顶部，阅读位置是用户的，不能替他重置。
   * 以前两处都 `scrollHost.scrollTo({top:0})`，看着像界面在乱跳；要定位到具体条目
   * 时用 `scrollIntoView({block:"nearest"})` 那种最小位移，不要整栏重置。
   */
  const [paramTab, setParamTab] = createSignal<ParamGroup>("tone");
  const curveTab = props.store.advancedTab;
  const setCurveTab = props.store.setAdvancedTab;

  return (
    <div
      data-editor-panels
      class={[
        /* 横向 padding 走密度令牌（与 browse 右栏同一口径）；滚动只发生在下半个可滚动区里 */
        "flex min-h-0 flex-1 flex-col gap-3 bg-surface-main pl-panel-pad pr-panel-pad-scroll py-panel-pad",
        props.class ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
    >
      {/* 工具控制块（裁切 / 旋转）——只在工具激活时出现，三工具互斥；固定在直方图之上 */}
      <Show when={props.store.tool() === "crop"}>
        <CropBlock store={props.store} enabled={props.enabled} onConfirm={props.onToolConfirm} />
      </Show>
      <Show when={props.store.tool() === "rotate"}>
        <RotateBlock store={props.store} enabled={props.enabled} onConfirm={props.onToolConfirm} />
      </Show>

      {/*
        直方图**固定**在这一栏顶部，不参与滚动（崔总 2026-10-12）。
        取数与缓存都在 `HistogramPanel` 里（与浏览右栏共用一份）。
      */}
      <HistogramPanel
        class="shrink-0"
        load={props.loadHistogram}
        path={props.current?.path ?? null}
        countsOverride={props.store.renderState() === null
          ? undefined
          : props.store.renderState()?.histogram ?? null}
        title={t("browse.histogram")}
        emptyText={t("browse.histogramEmpty")}
      />

      {/* ↓ 以下（参数组 + 曲线组）是**可滚动区** */}
      <div class="scroll-y-reserved flex min-h-0 flex-1 flex-col gap-3" data-editor-param-scroll>

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
                  props.onConfirm?.();
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
            <LensExtras store={props.store} enabled={props.enabled} onConfirm={props.onConfirm}
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
                onValueCommit={() => props.onConfirm?.()}
                onReset={() => {
                  props.store.endParamDrag();
                  props.store.resetParam(spec.id);
                  props.onConfirm?.();
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
        {/* 曲线 · 预设 · 色彩管理：输入操作在新管线接通前保持只读。 */}
        <SegmentedControl
          value={curveTab()}
          onValueChange={(value) => setCurveTab(value as CurveTab)}
          label={t("editor.group.curve")}
          options={[
            { value: "curve", label: t("editor.group.curve") },
            { value: "preset", label: t("editor.preset.title") },
            { value: "color", label: t("editor.colorManagement.title") },
          ]}
        />
        {/*
         * 曲线 / 预设共用一个内容容器：曲线在流内定高（切到预设时隐藏但**留位**），
         * 预设绝对定位填满同一格 —— 切页签高度不伸缩；预设树在容器里 flex 填充、
         * 内部滚动。审计 2026-10-04：此前两页签各挂各的、只有自然高度，
         * 预设多时整条右栏被撑长。
         *
         * 色彩管理页签**不属于这个容器**（内容跟在容器后面）：切到色彩时整格 `hidden`
         * —— 否则曲线那份留位会原样空在色彩内容上方，就是「色彩面板上部一大块空白」
         * （崔总 2026-10-10）。等高只管曲线 ↔ 预设两页签。
         */}
        <div class="relative flex min-h-0 flex-col" classList={{ hidden: curveTab() === "color" }} data-editor-advanced-body>
          <div classList={{ invisible: curveTab() !== "curve" }} data-editor-curve-slot>
            <CurveTab
              store={props.store}
              enabled={props.enabled}
              current={props.current}
              loadHistogram={props.loadHistogram}
              onConfirm={props.onConfirm}
              onRenameBaseCurve={props.onRenameBaseCurve}
              baseCurveLibrary={props.baseCurveLibrary}
              onSelectBaseCurve={props.onSelectBaseCurve}
            />
          </div>
          <Show when={curveTab() === "preset"}>
            <div class="absolute inset-0 flex min-h-0 flex-col" data-editor-preset-slot>
              <PresetPanel
                store={props.store}
                enabled={props.enabled}
                onConfirm={props.onConfirm}
                createDirectory={props.onCreatePresetDirectory}
                createPreset={props.onCreatePreset}
                renameDirectory={props.onRenamePresetDirectory}
                deletePreset={props.onDeletePreset}
                deleteDirectory={props.onDeletePresetDirectory}
                movePresets={props.onMovePresets}
                applyPreset={props.onApplyPreset}
                class="min-h-0 flex-1"
              />
            </div>
          </Show>
        </div>
        <Show when={curveTab() === "color"}>
          <ColorManagementTab store={props.store} enabled={props.enabled} hasPhoto={props.current !== null} profileEntries={props.profileEntries} onRefreshProfiles={props.onRefreshProfiles} onApplyColor={props.onApplyColor} onOpenProfiles={props.onOpenColorProfiles} colorBatchCount={props.colorBatchCount} onReviewColorBatch={props.onReviewColorBatch} />
        </Show>
      </section>
      </div>
    </div>
  );
}

function ColorManagementTab(props: { store: EditorStore; enabled: boolean; hasPhoto: boolean; profileEntries: readonly ColorProfileEntry[]; onRefreshProfiles: () => void; onApplyColor: (profileId: string | null) => Promise<void>; onOpenProfiles: () => void; colorBatchCount:number;onReviewColorBatch:(profileId:string|null)=>Promise<void> }): JSX.Element {
  const [choice,setChoice] = createSignal("auto");
  const [busy,setBusy] = createSignal(false);
  const [error,setError] = createSignal<string|null>(null);
  const color = () => props.store.colorState();
  createEffect(() => { const source=color()?.source; setChoice(source?.kind === "assigned_rgb_icc" ? source.profile_id : "auto"); setError(null); });
  const apply = async (automatic=false,batch=false): Promise<void> => {
    if (!props.enabled || busy()) return;
    const selected = automatic ? "auto" : choice();
    const id = selected === "srgb" ? props.profileEntries.find(entry=>entry.key === "builtin:srgb-v1")?.profileId : selected;
    if (!id) return;
    setBusy(true); setError(null);
    try { await (batch ? props.onReviewColorBatch : props.onApplyColor)(id === "auto" ? null : id); }
    catch(error) { setError(String(error)); }
    finally { setBusy(false); }
  };
  return (
    <div class="flex flex-col gap-2 text-fs-1" data-editor-color-management>
      <div class="flex flex-col gap-2">
        <div class="font-semibold text-fg-1">{t("editor.colorManagement.input")}</div>
        <p class="mt-1 leading-relaxed text-fg-2">
          {t(!props.hasPhoto ? "editor.colorManagement.noPhoto" : color() ? "editor.colorManagement.frozen" : "editor.colorManagement.inputPending")}
        </p>
        <ProfileSelect role="input" allowAutomatic label={t("editor.colorManagement.input")} class="w-full" entries={props.profileEntries} value={choice()} disabled={!props.enabled || busy()} onFocus={props.onRefreshProfiles} onChange={setChoice} />
        <div class="flex gap-2"><Button class="flex-1" variant="primary" disabled={!props.enabled || busy()} onClick={()=>void apply()}>{t(busy() ? "editor.colorManagement.preparing" : "editor.colorManagement.apply")}</Button>
          <Button class="flex-1" variant="secondary" disabled={!props.enabled || busy()} onClick={()=>void apply(true)}>{t("editor.colorManagement.restore")}</Button></div>
        <Button variant="secondary" disabled={!props.enabled || busy() || props.colorBatchCount<2} onClick={()=>void apply(false,true)}>{t("editor.colorManagement.batch",{count:props.colorBatchCount})}</Button>
        <Show when={color()}>{value=><p class="text-fs-0 text-fg-3">{t(`editor.colorManagement.source.${value().source.kind}` as Parameters<typeof t>[0])}</p>}</Show>
        <Show when={error()}>{message=><p role="alert" class="text-fs-0 text-danger">{message()}</p>}</Show>
      </div>
      <div class="rounded-ui bg-surface-bar p-3">
        <div class="font-semibold text-fg-1">{t("editor.colorManagement.working")}</div>
        <p class="mt-1 text-fg-2">{t(color() ? "editor.colorManagement.linear" : "editor.colorManagement.legacy")}</p>
      </div>
      <div class="rounded-ui bg-surface-bar p-3">
        <div class="font-semibold text-fg-1">{t("editor.colorManagement.display")}</div>
        <div class="mt-2"><DisplayColorStatus status={props.store.renderState()?.displayColor ?? null} /></div>
      </div>
      <div class="rounded-ui bg-surface-bar p-3">
        <Switch label={t("editor.colorManagement.proof")} checked={props.store.proofEnabled()} disabled={!props.hasPhoto} onCheckedChange={props.store.setProofEnabled} />
        <ProfileSelect role="output" label={t("editor.colorManagement.proofTarget")} class="mt-2 w-full" entries={props.profileEntries} value={outputColorChoice(props.store.proofTarget())} onFocus={props.onRefreshProfiles} onChange={value=>{const target=outputColorFromChoice(value);if(target)props.store.setProofTarget(target);}} />
        <div class="mt-2"><Switch label={t("editor.colorManagement.gamutWarning")} checked={props.store.proofWarning()} disabled={!props.store.proofEnabled()} onCheckedChange={props.store.setProofWarning} /></div>
        <p class="mt-1 text-fs-0 text-fg-3">{t("editor.colorManagement.proofLimit")}</p>
      </div>
      <button
        type="button"
        class="flex min-h-9 items-center justify-between rounded-ui px-3 text-left font-medium text-brand hover:bg-state-hover"
        onClick={props.onOpenProfiles}
      >
        {t("editor.colorManagement.manage")}
        <IconExternalLink size={16} aria-hidden="true" />
      </button>
      <p class="px-1 text-fs-0 leading-relaxed text-fg-3">{t("editor.colorManagement.cacheHint")}</p>
    </div>
  );
}

/** 镜头页签的两块非拉杆控件（配置文件 + 启用校正）。
 *
 * 口径（人类 2026-09-25 拍板）：**配置文件负责自动/库校正，手动拉杆是微调叠加其上**；
 * 「启用校正」开关**只管配置文件那一半** —— 关掉它之后手动拉杆照常生效
 * （与 Lightroom 的 Lens Corrections 面板一致）。 */
function LensExtras(props: { store: EditorStore; enabled: boolean; onConfirm?: () => void;
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
    props.onConfirm?.();
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
        onCheckedChange={(value) => { props.store.setLensEnabled(value); props.onConfirm?.(); }}
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
  onConfirm?: () => void;
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
      onConfirm={props.onConfirm}
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
              <button type="button" class="min-w-0 flex-1 overflow-hidden whitespace-nowrap fade-x-end rounded-ui px-2 py-1.5 text-left text-fs-1 text-fg-1"
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
        <input type="text" value={renameName()} maxLength={160} autofocus ref={selectAllOnMount}
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
              <span class="min-w-0 overflow-hidden whitespace-nowrap fade-x-end fade-x-end-fit">{ratioLabelOf(ratioId(), t)}</span>
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
