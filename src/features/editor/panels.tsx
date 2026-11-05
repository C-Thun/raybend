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

import { For, Show, createEffect, createMemo, createSignal, on, onCleanup, type JSX } from "solid-js";
import { IconChevronDown, IconColorSwatch, IconExternalLink, IconLoader2, IconPencil } from "@tabler/icons-solidjs";

import { Button, IconButton } from "../../components/ui/Button.tsx";
import { SegmentedControl } from "../../components/ui/SegmentedControl.tsx";
import { HistogramPanel } from "../../components/ui/HistogramPanel.tsx";
import { Switch } from "../../components/ui/Form.tsx";
import { ProfileSelect } from "../../components/ui/ProfileSelect.tsx";
import { DisplayColorStatus } from "../../components/ui/DisplayColorStatus.tsx";
import { ShimmerText } from "../../components/ui/ShimmerText.tsx";
import { AreaGlyph } from "./area-glyph.tsx";
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
import { SliderRow } from "./SliderRow.tsx";
import { StateWatermark } from "../../components/ui/StateWatermark.tsx";
import type { EditorStore } from "./store.ts";
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
  /**
   * **错误上报**（统一消息通道）：面板里不再显示内联红字，`AGENTS.md` §2.29。
   * 工作区把 `props.toast.show` 包一层传进来。
   */
  onError?: (message: string) => void;
  /**
   * 这张照片被二级锁锁住（不可编辑）—— 整列禁用 + 一句话说明。
   *
   * ❗**错误不走这里**（崔总 2026-11-01）：落库 / 读库 / 取样失败一律走
   * `components/ui/Toast.tsx` 的统一消息通道，面板里不再塞内联红字。
   */
  locked?: boolean;
  onOpenColorProfiles: () => void;
  profileEntries: readonly ColorProfileEntry[];
  onRefreshProfiles: () => void;
  onApplyColor: (profileId: string | null) => Promise<void>;
  colorBatchCount:number;
  onReviewColorBatch:(profileId:string|null)=>Promise<void>;
  class?: string;
}

/* ══════════════════════════════════════════════════════════════
 * 组件
 * ══════════════════════════════════════════════════════════════ */

type CurveTab = "curve" | "swatch" | "color";

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

  /*
   * **全局阶段只属于默认层**（`specs/editor-area.md` §4.3）：不在默认层时，
   * 那些页签与控件**整块不出现**（崔总 2026-11-01：不要置灰 + 提示，直接隐藏）。
   *
   * 页签列表是从“当前层能不能用”推出来的，不是写死的三份列表；
   * 切层时若当前页签已不存在，退回“影调” / “曲线”（不能让分段控件指向一个不存在的值）。
   */
  const globalStages = (): boolean => props.store.onDefaultLayer();
  const paramGroups = (): readonly ParamGroup[] => PARAM_GROUPS
    .filter((group) => globalStages() || (group !== "detail" && group !== "lens"));
  const curveTabs = (): readonly { value: string; label: string }[] => [
    { value: "curve", label: t("editor.group.curve") },
    /*
     * 色盘：**空面板**（崔总 2026-11-05 预留四色轮调色位；这里只有页签与空态）。
     *
     * 英文页签只写控件名 **“Wheels”**（2026-11-05 定，与 `Curve` 同一个命名法）：三个页签
     * 与“Color mgmt”挤在 268px 内容区里，全名会把这一行顶出去；完整词由**正文**负责
     * （空态 "No color-wheel settings yet"，将来面板自己的标题），页签只是它的短别名。
     * 中文侧「色盘」两字已经最紧，**不跟着缩**。
     */
    { value: "swatch", label: t("editor.group.swatch") },
    ...(globalStages()
      ? [{ value: "color", label: t("editor.colorManagement.title") }]
      : []),
  ];
  createEffect(() => {
    if (!paramGroups().includes(paramTab())) setParamTab("tone");
  });
  createEffect(() => {
    if (!globalStages() && curveTab() === "color") setCurveTab("curve");
  });

  /*
   * 层名条上的**一次性光条**（`ShimmerText` + `.rb-shimmer-mask`，与导出「开始导出」同一套）。
   *
   * 只在**切层**时走一遍：右栏整套参数跟着换了对象，光条是「换了」的信号。
   * 首次挂载不走（进编辑器就闪一下是噪声）；语言切换 / 改名也不走。
   */
  const [sweeping, setSweeping] = createSignal(false);
  let sweepTimer: number | undefined;
  onCleanup(() => {
    if (sweepTimer !== undefined) window.clearTimeout(sweepTimer);
  });
  createEffect(on(() => props.store.activeAreaId(), (_id, previous) => {
    if (previous === undefined) return; // 首次挂载不闪
    // 连续切层要把动画**从头**放：同一帧里 false → true，光带才会重新扫
    setSweeping(false);
    queueMicrotask(() => setSweeping(true));
    if (sweepTimer !== undefined) window.clearTimeout(sweepTimer);
    sweepTimer = window.setTimeout(() => setSweeping(false), 860);
  }, { defer: true }));

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
        /*
         * `!mb-0`：`HistogramPanel` 自带 `mb-3`，而这一栏的父级还有 `gap-3` ——
         * 两层叠起来会让**层名条上方**的留白是下方的一倍多（崔总 2026-11-01 截图指出）。
         * 这里清零，上下都只留父级的 `gap`（层名条自己再 `-my-1` 收 4px）。
         */
        class="shrink-0 !mb-0"
        load={props.loadHistogram}
        path={props.current?.path ?? null}
        countsOverride={props.store.renderState() === null
          ? undefined
          : props.store.renderState()?.histogram ?? null}
        title={t("browse.histogram")}
        emptyText={t("browse.histogramEmpty")}
      />

      {/*
        **当前层名条**（`design/editor.md` 2026-10-31 画稿 SPyhh 的平化版，崔总 2026-11-01）：

        * **无边框、无底色**（平面设计）—— 它读起来是「直方图下面的一行小标题」，不是一块面板；
        * 左边一枚**主色**类型记号（与层列表同一个 `AreaGlyph`，形状即层类型）；
        * 文字比别处稍大一号并加粗（13px / semibold）：这一行的唯一职责就是回答「现在在改哪一层」；
        * 上下各收 4px（父级 `gap-3` 的 12 变成视觉 8）：它是一行**标题**，不是一块面板，
          原来 16+16 的上下留白在真机上确实太空；
        * **切层时名字上走一遍光条**（`ShimmerText`，与导出「开始导出」同一个 `.rb-shimmer-mask`）——
          右栏的内容整套换掉了，光条是「换了」的信号，比只看文字变一下更容易注意到。
      */}
      <Show when={props.current !== null}>
        <div
          data-layer-name-bar
          class="flex h-6 shrink-0 items-center gap-1.5 -my-1"
          title={props.store.activeAreaName()}
        >
          <AreaGlyph
            kind={props.store.activeArea()?.type ?? "default"}
            color={props.store.activeArea()?.color ?? 0}
            class="text-brand"
          />
          {/*
            `fade-x-end` 放在**外层**（`ShimmerText` 的盒子上）：光条那一层是它的兄弟，
            渐隐必须同时作用于两遍内容，否则被光扫过的那份会在渐隐区外露出整段文字。
          */}
          <ShimmerText
            class="min-w-0 flex-1 fade-x-end"
            active={sweeping()}
            once
            highlight="text-brand"
          >
            <span class="w-full whitespace-nowrap text-fs-1 font-semibold text-fg-1">
              {props.store.activeAreaName()}
            </span>
          </ShimmerText>
        </div>
      </Show>

      {/* ↓ 以下（参数组 + 曲线组）是**可滚动区** */}
      <div class="scroll-y-reserved flex min-h-0 flex-1 flex-col gap-3" data-editor-param-scroll>

      {/* ── 第 2 组：影调 / 色彩 / 清晰度 / 镜头 ───────────── */}
      <section class="flex flex-col gap-2" data-editor-group="params">
        {/*
          全局阶段（镜头 / 清晰度 / 局部色调 / 色彩管理）只属于默认层
          （`specs/editor-area.md` §4.3）。不在默认层时这些页签与控件**整块不出现**——
          不置灰、不放提示条（崔总 2026-11-01：「我说了是隐藏」；`AGENTS.md` §2.20）。
        */}
        <SegmentedControl
          value={paramTab()}
          onValueChange={(value) => setParamTab(value as ParamGroup)}
          /*
           * `label` 是**这一条页签带的组名**（读屏报“组名, 选项卡列表”），不是默认页签的名字。
           * 以前传的是 `editor.group.tone`/`editor.group.curve`，读屏会念成「影调 / 曲线」
           * 却罩着四个 / 三个不同页签（2026-11-05 改）。
           */
          label={t("editor.tabs.basic")}
          options={paramGroups().map((group) => ({
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
                  {
                    value: "fast",
                    label: t("editor.detail.nrFast"),
                    disabled: !props.enabled || !props.store.onDefaultLayer(),
                  },
                  {
                    value: "high",
                    label: t("editor.detail.nrHigh"),
                    disabled: !props.enabled || !props.store.onDefaultLayer(),
                  },
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
            <LensExtras store={props.store} enabled={props.enabled && props.store.onDefaultLayer()} onConfirm={props.onConfirm}
              queryState={props.lensQuery} onRefresh={props.onRefreshLens} onError={props.onError} />
          </Show>
          <For each={paramsInGroup(paramTab())
            .filter((spec) => globalStages() || spec.target !== "global")}>
            {(spec) => (
              <SliderRow
                spec={spec}
                value={props.store.paramValue(spec.id)}
                /* 参数表的 wired 字段与 Rust 契约保持一致；global 项只在默认层可用。 */
                disabled={!props.enabled || !spec.wired
                  || (spec.target === "global" && !props.store.onDefaultLayer())}
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
        <Show when={props.locked === true}>
          <p class="text-fs-0 leading-snug text-fg-3" data-editor-panel-locked>
            {t("editor.panel.locked")}
          </p>
        </Show>
      </section>

      {/* ── 第 3 组：曲线 ─────────────────────────────────── */}
      <section class="flex flex-col gap-2" data-editor-group="curve">
        {/* 曲线 · 预设 · 色彩管理：输入操作在新管线接通前保持只读。 */}
        <SegmentedControl
          value={curveTab()}
          onValueChange={(value) => setCurveTab(value as CurveTab)}
          label={t("editor.tabs.advanced")}
          options={curveTabs()}
        />
        {/*
         * 三个页签**各自渲染**（2026-11-05）：预设搬去左列面板后，原先「曲线留位 + 预设
         * 绝对定位填满同一格」的等高技巧就没有第二个对象了 —— 那套容器只会给色盘留下
         * 一整块空白（正是崔总 2026-10-10 报过的「色彩面板上部一大块空白」的同一个坑）。
         */}
        <Show when={curveTab() === "curve"}>
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
        </Show>
        <Show when={curveTab() === "swatch"}>
          <SwatchPanel />
        </Show>
        <Show when={curveTab() === "color"}>
          <ColorManagementTab store={props.store} enabled={props.enabled && props.store.onDefaultLayer()} hasPhoto={props.current !== null} profileEntries={props.profileEntries} onRefreshProfiles={props.onRefreshProfiles} onApplyColor={props.onApplyColor} onOpenProfiles={props.onOpenColorProfiles} colorBatchCount={props.colorBatchCount} onReviewColorBatch={props.onReviewColorBatch} onError={props.onError} />
        </Show>
      </section>
      </div>
    </div>
  );
}

/**
 * 色盘（英文页签 **“Wheels”**，完整词 color wheels 留在正文）—— **空面板占位**（崔总 2026-11-05）。
 *
 * 位置：右栏第 3 组，夹在 `曲线` 与 `色彩管理` 之间。将来这里放类似 DaVinci 的
 * 四色轮调色（阴影 / 中间调 / 高光 / 偏移）。
 *
 * 现在**刻意什么都不做**：没有控件、没有默认值、不进 profile / 哈希 / 撤销 / XMP。
 * 放一个空态水印只为说明「这一格是留给它的」，不是功能入口 —— 所以不画假旋钮。
 */
function SwatchPanel(): JSX.Element {
  return (
    <div class="flex min-h-32 flex-1 items-center justify-center rounded-ui bg-surface-bar" data-editor-swatch>
      <StateWatermark icon={<IconColorSwatch size={64} stroke-width={1} />} text={t("editor.swatch.empty")} />
    </div>
  );
}

function ColorManagementTab(props: { store: EditorStore; enabled: boolean; hasPhoto: boolean; profileEntries: readonly ColorProfileEntry[]; onRefreshProfiles: () => void; onApplyColor: (profileId: string | null) => Promise<void>; onOpenProfiles: () => void; colorBatchCount:number;onReviewColorBatch:(profileId:string|null)=>Promise<void>; onError?: (message: string) => void }): JSX.Element {
  const [choice,setChoice] = createSignal("auto");
  const [busy,setBusy] = createSignal(false);

  const color = () => props.store.colorState();
  createEffect(() => { const source=color()?.source; setChoice(source?.kind === "assigned_rgb_icc" ? source.profile_id : "auto"); });
  const apply = async (automatic=false,batch=false): Promise<void> => {
    if (!props.enabled || busy()) return;
    const selected = automatic ? "auto" : choice();
    const id = selected === "srgb" ? props.profileEntries.find(entry=>entry.key === "builtin:srgb-v1")?.profileId : selected;
    if (!id) return;
    setBusy(true);
    try { await (batch ? props.onReviewColorBatch : props.onApplyColor)(id === "auto" ? null : id); }
    catch(error) { props.onError?.(String(error)); }
    finally { setBusy(false); }
  };
  return (
    <div class="flex flex-col gap-2 text-fs-1" data-editor-color-management>
      <div class="flex flex-col gap-2">
        <div class="font-semibold text-fg-1">{t("editor.colorManagement.input")}</div>
        <ProfileSelect role="input" allowAutomatic label={t("editor.colorManagement.input")} class="w-full" entries={props.profileEntries} value={choice()} disabled={!props.enabled || busy()} onFocus={props.onRefreshProfiles} onChange={setChoice} />
        <div class="flex gap-2"><Button class="flex-1" variant="primary" disabled={!props.enabled || busy()} onClick={()=>void apply()}>{t(busy() ? "editor.colorManagement.preparing" : "editor.colorManagement.apply")}</Button>
          <Button class="flex-1" variant="secondary" disabled={!props.enabled || busy()} onClick={()=>void apply(true)}>{t("editor.colorManagement.restore")}</Button></div>
        <Button variant="secondary" disabled={!props.enabled || busy() || props.colorBatchCount<2} onClick={()=>void apply(false,true)}>{t("editor.colorManagement.batch",{count:props.colorBatchCount})}</Button>
        <Show when={color()}>{value=><p class="text-fs-0 text-fg-3">{t(`editor.colorManagement.source.${value().source.kind}` as Parameters<typeof t>[0])}</p>}</Show>

      </div>
      <div class="rounded-ui bg-surface-bar p-3">
        <div class="font-semibold text-fg-1">{t("editor.colorManagement.working")}</div>
        <p class="mt-1 text-fg-2">{t(color() ? "editor.colorManagement.linear" : "editor.colorManagement.legacy")}</p>
      </div>
      <div class="rounded-ui bg-surface-bar p-3">
        <div class="font-semibold text-fg-1">{t("editor.colorManagement.display")}</div>
        <div class="mt-2"><DisplayColorStatus status={props.store.renderState()?.displayColor ?? null} detail={false} /></div>
      </div>
      <div class="rounded-ui bg-surface-bar p-3">
        {/*
          「相对色度」是打样采用的渲染意图（固定值，不做选择器）。
          它跟在开关同一行右侧 —— 去掉之前那句「RGB · 相对色度。只改变查看；…」的固定说明
          （崔总 2026-11-01：界面不挂无变量的静态解释）。
        */}
        <div class="flex items-center justify-between gap-2">
          <Switch label={t("editor.colorManagement.proof")} checked={props.store.proofEnabled()} disabled={!props.hasPhoto} onCheckedChange={props.store.setProofEnabled} />
          <span class="text-fs-0 text-fg-3">{t("editor.colorManagement.intent")}</span>
        </div>
        <ProfileSelect role="output" label={t("editor.colorManagement.proofTarget")} class="mt-2 w-full" entries={props.profileEntries} value={outputColorChoice(props.store.proofTarget())} onFocus={props.onRefreshProfiles} onChange={value=>{const target=outputColorFromChoice(value);if(target)props.store.setProofTarget(target);}} />
        <div class="mt-2"><Switch label={t("editor.colorManagement.gamutWarning")} checked={props.store.proofWarning()} disabled={!props.store.proofEnabled()} onCheckedChange={props.store.setProofWarning} /></div>
      </div>
      <button
        type="button"
        class="flex min-h-9 items-center justify-between rounded-ui px-3 text-left font-medium text-brand hover:bg-state-hover"
        onClick={props.onOpenProfiles}
      >
        {t("editor.colorManagement.manage")}
        <IconExternalLink size={16} aria-hidden="true" />
      </button>
    </div>
  );
}

/** 镜头页签的两块非拉杆控件（配置文件 + 启用校正）。
 *
 * 口径（人类 2026-09-25 拍板）：**配置文件负责自动/库校正，手动拉杆是微调叠加其上**；
 * 「启用校正」开关**只管配置文件那一半** —— 关掉它之后手动拉杆照常生效
 * （与 Lightroom 的 Lens Corrections 面板一致）。 */
function LensExtras(props: { store: EditorStore; enabled: boolean; onConfirm?: () => void;
  queryState: LensQueryState; onRefresh: () => void; onError?: (message: string) => void }): JSX.Element {
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
  /*
   * 查询失败走**统一消息通道**：这里只保留「重试」动作，不再把错误正文
   * 塞在参数面板里（`AGENTS.md` §2.29）。
   */
  createEffect(() => {
    if (props.queryState.status !== "error") return;
    const message = props.queryState.error ?? t(props.queryState.failure === "timeout"
      ? "editor.lens.timeout"
      : props.queryState.failure === "unavailable"
        ? "editor.lens.unavailable"
        : "editor.lens.failed");
    props.onError?.(message);
  });
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
      <Button variant="secondary" onClick={props.onRefresh}>{t("editor.lens.retry")}</Button>
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
  /**
   * **错误上报**（统一消息通道）：面板里不再显示内联红字，`AGENTS.md` §2.29。
   * 工作区把 `props.toast.show` 包一层传进来。
   */
  onError?: (message: string) => void;
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
        <Button variant="secondary" disabled={!props.enabled || props.store.autoAdjusting() || !props.store.onDefaultLayer()} class="h-auto min-h-8 w-full justify-between py-1.5"
          onClick={() => setBaseOpen(true)}>{baseLabel()}</Button>
        <p class="text-fs-0 text-fg-3">{t("editor.baseCurve.cameraOnly")}</p>
      </div>
      <Dialog open={baseOpen()} onOpenChange={setBaseOpen} title={t("editor.baseCurve.label")}>
        <div class="max-h-64 overflow-y-auto" data-base-curve-results>
          <button type="button" class="w-full rounded-ui px-2 py-1.5 text-left text-fs-1 text-fg-1 hover:bg-state-hover"
            disabled={!props.enabled || props.store.autoAdjusting() || !props.store.onDefaultLayer()}
            aria-pressed={props.store.baseCurveProfile() === "none"}
            onClick={() => { props.onSelectBaseCurve("none"); setBaseOpen(false); }}>
            {t("editor.baseCurve.none")}</button>
          <Show when={(props.baseCurveLibrary?.profiles.length ?? 0) === 0}>
            <p class="px-2 py-2 text-fs-0 text-fg-3">{t("editor.baseCurve.noProfiles")}</p>
          </Show>
          <For each={props.baseCurveLibrary?.profiles ?? []}>{profile =>
            <div class="flex items-center gap-1 rounded-ui hover:bg-state-hover">
              <button type="button" class="min-w-0 flex-1 overflow-hidden whitespace-nowrap fade-x-end rounded-ui px-2 py-1.5 text-left text-fs-1 text-fg-1"
                disabled={!props.enabled || props.store.autoAdjusting() || !props.store.onDefaultLayer()}
                aria-pressed={props.store.baseCurveProfile() === profile.id}
                onClick={() => { props.onSelectBaseCurve(profile.id); setBaseOpen(false); }}>
                {profile.name}</button>
              <IconButton label={t("editor.baseCurve.rename")} disabled={!props.enabled || props.store.autoAdjusting() || !props.store.onDefaultLayer()}
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
