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
 * 裁切/旋转的画布交互在 W5。
 */

import { For, Show, createSignal, type JSX } from "solid-js";

import { Button } from "../../components/ui/Button.tsx";
import { SegmentedControl } from "../../components/ui/SegmentedControl.tsx";
import { HistogramPanel } from "../../components/ui/HistogramPanel.tsx";
import { Switch } from "../../components/ui/Form.tsx";
import { Menu } from "../../components/ui/Menu.tsx";
import { t } from "../../i18n/index.ts";
import type { MessageKey } from "../../i18n/index.ts";
import type { HistogramCounts } from "../../lib/histogram.ts";
import type { ThumbQueue } from "../../components/ui/thumb-queue.ts";
import type { ViewerPhoto } from "../../components/ui/viewer/index.ts";
import { PREVIEW_FRAME_ASPECT } from "../../lib/preview-frame.ts";
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
import type { CurveChannel, EditorStore } from "./store.ts";

/* ══════════════════════════════════════════════════════════════
 * 入参形状
 * ══════════════════════════════════════════════════════════════ */

/**
 * 「信息」页签要的字段（**结构类型**，由工作区从 `AssetItem` / `ExifData` 拼好传进来）。
 *
 * 为什么不让本模块自己去查：`features/*` 之间不许互相 import（分层规则），
 * 而照片数据属于浏览那一侧 —— 工作区把两块接起来正是它的职责（`ARCHITECTURE.md` §2）。
 * 这里的字段**都是显示字符串**（格式化的唯一实现在 `features/exif-strip/exif-format.ts`）。
 */
export interface EditorPhotoInfo {
  fileName: string;
  relativePath: string;
  format: string | null;
  camera: string | null;
  lens: string | null;
  iso: string | null;
  shutter: string | null;
  aperture: string | null;
  focal: string | null;
  dimensions: string | null;
  megapixels: string | null;
}

export interface EditorPanelsProps {
  store: EditorStore;
  /** 有没有可以编辑的照片（空态下整列控件禁用） */
  enabled: boolean;
  /** 当前那张（胶片带的锚点） */
  current: ViewerPhoto | null;
  /** 信息行（没有选中就是 `null`） */
  info: EditorPhotoInfo | null;
  /** 与胶片带**共用**的缩略图队列（总览的小图用它，不再另取一遍） */
  thumbs: ThumbQueue;
  /** 直方图取数（Rust 算的；注入进来，本模块不碰 `api`） */
  loadHistogram: (path: string, bins: number) => Promise<HistogramCounts | null>;
  /**
   * 参数**松手**了（拖动结束 / 点了重置）—— 工作区拿它落库。
   *
   * 拖动过程中只改画面不落库（`AGENTS.md` 的口径：松手才落库），
   * 所以这里只给一个「可以存了」的信号，具体存什么由 store 的载荷决定。
   */
  onCommit?: () => void;
  /** 「全部重置」—— 与单项不同：它要**同时**清库（工作区负责），所以单独一个口子 */
  onReset?: () => void;
  /** 落库 / 读库失败的原因（有值就显示一行提示 —— 不静默吞掉） */
  error?: string | null;
  /** 这张照片被二级锁锁住（不可编辑）—— 整列禁用 + 一句话说明 */
  locked?: boolean;
  class?: string;
}

/* ══════════════════════════════════════════════════════════════
 * 组件
 * ══════════════════════════════════════════════════════════════ */

type ViewTab = "view" | "issues" | "info";
type CurveTab = "curve";

export function EditorPanels(props: EditorPanelsProps): JSX.Element {
  const [viewTab, setViewTab] = createSignal<ViewTab>("view");
  const [paramTab, setParamTab] = createSignal<ParamGroup>("tone");
  const [curveTab] = createSignal<CurveTab>("curve");

  return (
    <div
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
        <CropBlock store={props.store} enabled={props.enabled} />
      </Show>
      <Show when={props.store.tool() === "rotate"}>
        <RotateBlock store={props.store} enabled={props.enabled} />
      </Show>
      <Show when={props.store.tool() === "compare"}>
        <CompareBlock />
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
            thumbs={props.thumbs}
            loadHistogram={props.loadHistogram}
          />
        </Show>
        <Show when={viewTab() === "issues"}>
          <IssuesTab enabled={props.enabled} />
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
          <For each={paramsInGroup(paramTab())}>
            {(spec) => (
              <SliderRow
                spec={spec}
                value={props.store.paramValue(spec.id)}
                /* 本波没接进管线的（清晰度 / 镜头）**禁用**并写明哪一波接 ——
                   一个能拖但没反应的拉杆比一个禁用的拉杆更糟 */
                disabled={!props.enabled || !spec.wired}
                onValueChange={(value) => props.store.setParam(spec.id, value)}
                onValueCommit={() => props.onCommit?.()}
              />
            )}
          </For>
          {/* 镜头页签多两块非拉杆控件（`.pd`：配置文件 + 启用校正开关） */}
          <Show when={paramTab() === "lens"}>
            <LensExtras enabled={props.enabled} />
          </Show>
        </div>
        <div class="flex items-center justify-between gap-2">
          <Show
            when={props.error == null}
            fallback={
              <p class="flex-1 text-fs-0 leading-snug text-danger" data-editor-develop-error>
                {props.error}
              </p>
            }
          >
            <PendingNote
              text={
                props.locked === true
                  ? t("editor.panel.locked")
                  : paramTab() === "tone" || paramTab() === "color"
                    ? t("editor.panel.live")
                    : t("editor.panel.w4Later")
              }
              class="flex-1"
            />
          </Show>
          <Button
            variant="ghost"
            disabled={!props.enabled}
            onClick={() => {
              if (props.onReset !== undefined) {
                props.onReset();
                return;
              }
              props.store.resetParams();
              props.onCommit?.();
            }}
          >
            {t("editor.panel.resetAll")}
          </Button>
        </div>
      </section>

      {/* ── 第 3 组：曲线 ─────────────────────────────────── */}
      <section class="flex flex-col gap-2" data-editor-group="curve">
        <SegmentedControl
          value={curveTab()}
          onValueChange={() => undefined}
          label={t("editor.group.curve")}
          options={[{ value: "curve", label: t("editor.group.curve") }]}
        />
        <CurveTab store={props.store} enabled={props.enabled} />
      </section>
    </div>
  );
}

/* ══════════════════════════════════════════════════════════════
 * 第 1 组的三块内容
 * ══════════════════════════════════════════════════════════════ */

function OverviewTab(props: {
  current: ViewerPhoto | null;
  thumbs: ThumbQueue;
  loadHistogram: (path: string, bins: number) => Promise<HistogramCounts | null>;
}): JSX.Element {
  const path = (): string | null => props.current?.path ?? null;
  const thumb = (): string | null => {
    const file = path();
    if (file === null) return null;
    props.thumbs.request(file);
    return props.thumbs.get(file).url;
  };

  return (
    <>
      {/*
        预览：与浏览右栏同一个 4:3 框口径（`lib/preview-frame.ts`），
        用的也是**与胶片带同一份缩略图缓存** —— 不为「编辑里再看一眼」再取一遍图。
      */}
      <div
        class="flex w-full items-center justify-center rounded-ui bg-surface-bar p-2"
        style={{ "aspect-ratio": String(PREVIEW_FRAME_ASPECT) }}
        data-editor-preview
      >
        <Show
          when={thumb()}
          fallback={
            <span class="text-fs-2 text-fg-3">
              {path() === null ? t("editor.empty.noSelection") : t("common.loading")}
            </span>
          }
        >
          {(url) => (
            <img
              class="block h-full w-full object-contain"
              src={url()}
              alt=""
              draggable={false}
            />
          )}
        </Show>
      </div>
      <HistogramPanel
        load={props.loadHistogram}
        path={path()}
        title={t("browse.histogram")}
        emptyText={t("browse.histogramEmpty")}
      />
    </>
  );
}

/** 定稿页签：W1 只有一条写死的 SOOC + 「编辑中」占位（issue 落库在 W3）。 */
function IssuesTab(props: { enabled: boolean }): JSX.Element {
  return (
    <div class="flex flex-col gap-2" data-editor-issues>
      <div class="flex items-center gap-2 rounded-ui bg-surface-track px-2 py-1.5">
        <span class="rounded-ui bg-brand px-1.5 py-0.5 text-fs-0 text-fg-on-brand">
          {t("editor.issue.sooc")}
        </span>
        <span class="min-w-0 flex-1 truncate text-fs-1 text-fg-3">
          {t("editor.issue.soocHint")}
        </span>
      </div>
      <div class="flex items-center gap-2 rounded-ui bg-surface-track px-2 py-1.5">
        <span class="rounded-ui bg-state-selected px-1.5 py-0.5 text-fs-0 text-fg-1">
          {t("editor.issue.current")}
        </span>
      </div>
      <Button variant="secondary" disabled={!props.enabled}>
        {t("editor.issue.finalize")}
      </Button>
      <PendingNote text={t("editor.issue.later")} />
    </div>
  );
}

/** 信息页签：文件 / 相机 / 尺寸 + 编辑（`.pd` 的三段 + 一行编辑状态）。 */
function InfoTab(props: { info: EditorPhotoInfo | null }): JSX.Element {
  const rows = (): { labelKey: MessageKey; value: string | null }[] => {
    const info = props.info;
    if (info === null) return [];
    return [
      { labelKey: "editor.info.name", value: info.fileName },
      { labelKey: "editor.info.path", value: info.relativePath },
      { labelKey: "exif.format", value: info.format },
      { labelKey: "exif.camera", value: info.camera },
      { labelKey: "exif.lens", value: info.lens },
      { labelKey: "exif.iso", value: info.iso },
      { labelKey: "exif.shutter", value: info.shutter },
      { labelKey: "exif.aperture", value: info.aperture },
      { labelKey: "exif.focal", value: info.focal },
      { labelKey: "exif.dimensions", value: info.dimensions },
      { labelKey: "exif.megapixels", value: info.megapixels },
    ];
  };

  return (
    <div class="flex flex-col gap-1" data-editor-info>
      <Show
        when={props.info !== null}
        fallback={<p class="py-2 text-fs-1 text-fg-3">{t("exif.empty")}</p>}
      >
        <For each={rows()}>
          {(row) => (
            <Show when={row.value !== null && row.value !== ""}>
              <div class="flex items-baseline gap-2">
                <span class="w-16 shrink-0 text-fs-0 text-fg-3">{t(row.labelKey)}</span>
                <span class="min-w-0 flex-1 truncate text-fs-1 text-fg-1">{row.value}</span>
              </div>
            </Show>
          )}
        </For>
      </Show>
    </div>
  );
}

/** 镜头页签的两块非拉杆控件（配置文件 + 启用校正）。 */
function LensExtras(props: { enabled: boolean }): JSX.Element {
  const [enabled, setEnabled] = createSignal(false);
  return (
    <div class="flex flex-col gap-2 pt-1">
      <div class="flex flex-col gap-1">
        <span class="text-fs-0 text-fg-2">{t("editor.lens.profile")}</span>
        <Menu
          items={[{ value: "none", label: t("editor.lens.profileNone"), selected: true }]}
          onSelect={() => undefined}
          label={t("editor.lens.profile")}
          placement="bottom-start"
        >
          {(triggerProps) => (
            <Button
              {...triggerProps()}
              variant="secondary"
              disabled={!props.enabled}
              class="w-full justify-between"
            >
              {t("editor.lens.profileNone")}
            </Button>
          )}
        </Menu>
      </div>
      <Switch
        checked={enabled()}
        onCheckedChange={setEnabled}
        disabled={!props.enabled}
        label={t("editor.lens.enable")}
      />
      <PendingNote text={t("editor.panel.w4Later")} />
    </div>
  );
}

/* ══════════════════════════════════════════════════════════════
 * 第 3 组：曲线
 * ══════════════════════════════════════════════════════════════ */

const CURVE_CHANNEL_LABEL: Record<CurveChannel, string> = {
  rgb: "RGB",
  r: "R",
  g: "G",
  b: "B",
};

/**
 * 曲线（W1：**恒等曲线**的静态示意）。
 *
 * 真正的编辑器（控制点、通道、预设）在 W3 与管线一起做 —— 现在画一条对角线，
 * 是为了让页签组有形状、也让「加了曲线之后会是这么大一块」这件事可评审。
 * 见 `design/editor.md` §7 的说明：曲线是**唯一自研**的控件（Ark 没有对应件）。
 */
function CurveTab(props: { store: EditorStore; enabled: boolean }): JSX.Element {
  return (
    <div class="flex flex-col gap-2" data-editor-curve>
      <SegmentedControl
        value={props.store.curveChannel()}
        onValueChange={(value) =>
          props.store.setCurveChannel(value as CurveChannel)
        }
        label={t("editor.group.curve")}
        options={(["rgb", "r", "g", "b"] as const).map((channel) => ({
          value: channel,
          label: CURVE_CHANNEL_LABEL[channel],
        }))}
      />
      <div class="rounded-ui bg-surface-bar p-2">
        <svg
          viewBox="0 0 100 100"
          class="block h-32 w-full"
          role="img"
          aria-label={t("editor.group.curve")}
        >
          {/* 4×4 网格（设计稿：网格 + 曲线 + 控制点） */}
          <For each={[25, 50, 75]}>
            {(at) => (
              <>
                <line x1={at} y1={0} x2={at} y2={100} stroke="var(--surface-layer)" stroke-width="0.4" />
                <line x1={0} y1={at} x2={100} y2={at} stroke="var(--surface-layer)" stroke-width="0.4" />
              </>
            )}
          </For>
          <rect x={0} y={0} width={100} height={100} fill="none" stroke="var(--surface-layer)" stroke-width="0.4" />
          {/* 恒等曲线（左下 → 右上）：W3 起换成真实控制点 */}
          <line x1={0} y1={100} x2={100} y2={0} stroke="var(--brand)" stroke-width="1.2" />
          <circle cx={50} cy={50} r={2.2} fill="var(--brand)" />
        </svg>
      </div>
      <PendingNote text={t("editor.curve.later")} />
    </div>
  );
}

/* ══════════════════════════════════════════════════════════════
 * 工具控制块
 * ══════════════════════════════════════════════════════════════ */

/** 裁切控制块（`.pd` 的六件套：比例下拉 / 反转 / 取消比例 / 横纵输入 / 取消 + 确认）。 */
function CropBlock(props: { store: EditorStore; enabled: boolean }): JSX.Element {
  const ratioId = (): string => props.store.cropRatioId();

  return (
    <section
      data-editor-tool-block="crop"
      class="flex flex-col gap-2 rounded-ui bg-surface-track p-2"
    >
      <div class="flex items-end gap-1">
        <div class="flex min-w-0 flex-1 flex-col gap-1">
          <span class="text-fs-0 text-fg-2">{t("editor.crop.ratio")}</span>
          <Menu
            items={CROP_RATIOS.map((item) => ({
              value: item.id,
              label: cropRatioLabel(item, t),
              selected: item.id === ratioId(),
            })).concat(
              ratioId() === "custom"
                ? [{ value: "custom", label: t("editor.crop.custom"), selected: true }]
                : [],
            )}
            onSelect={(value) => props.store.setCropRatioId(value)}
            label={t("editor.crop.ratio")}
            placement="bottom-start"
          >
            {(triggerProps) => (
              <Button
                {...triggerProps()}
                variant="secondary"
                disabled={!props.enabled}
                class="w-full justify-between"
              >
                {ratioLabelOf(ratioId(), t)}
              </Button>
            )}
          </Menu>
        </div>
        <Button
          variant="ghost"
          disabled={!props.enabled}
          aria-label={t("editor.crop.flip")}
          title={t("editor.crop.flip")}
          icon={<span aria-hidden="true">⇄</span>}
          onClick={() => props.store.flipCropRatio()}
        />
        <Button
          variant="ghost"
          disabled={!props.enabled}
          aria-label={t("editor.crop.unlink")}
          title={t("editor.crop.unlink")}
          icon={<span aria-hidden="true">⛓</span>}
          onClick={() => props.store.unlinkCropRatio()}
        />
      </div>

      <div class="flex items-center gap-2">
        <label class="flex min-w-0 flex-1 flex-col gap-1">
          <span class="text-fs-0 text-fg-2">{t("editor.crop.width")}</span>
          <input
            type="number"
            min={1}
            class="h-row-h w-full rounded-ui bg-surface-bar px-2 text-fs-1 tabular-nums text-fg-1 outline-none"
            value={props.store.cropWidth()}
            disabled={!props.enabled}
            onInput={(event) =>
              props.store.setCropSize(
                Number(event.currentTarget.value) || 0,
                props.store.cropHeight(),
              )
            }
          />
        </label>
        <label class="flex min-w-0 flex-1 flex-col gap-1">
          <span class="text-fs-0 text-fg-2">{t("editor.crop.height")}</span>
          <input
            type="number"
            min={1}
            class="h-row-h w-full rounded-ui bg-surface-bar px-2 text-fs-1 tabular-nums text-fg-1 outline-none"
            value={props.store.cropHeight()}
            disabled={!props.enabled}
            onInput={(event) =>
              props.store.setCropSize(
                props.store.cropWidth(),
                Number(event.currentTarget.value) || 0,
              )
            }
          />
        </label>
      </div>

      {/* 取消在左、确认在右（`AGENTS.md` §11.5，全系统统一） */}
      <div class="flex items-center justify-end gap-1.5">
        <Button variant="secondary" onClick={() => props.store.closeTool()}>
          {t("common.cancel")}
        </Button>
        <Button variant="primary" disabled={!props.enabled} onClick={() => props.store.closeTool()}>
          {t("common.confirm")}
        </Button>
      </div>
      <PendingNote text={t("editor.crop.later")} />
    </section>
  );
}

/** 比例 id → 下拉里那一行的文案（找不到就当「自定义」）。 */
function ratioLabelOf(id: string, translate: (key: MessageKey) => string): string {
  const item = CROP_RATIOS.find((candidate) => candidate.id === id);
  return item === undefined ? translate("editor.crop.custom") : cropRatioLabel(item, translate);
}

/** 旋转控制块（角度拉杆 + 重置 + 拉线提示 + 取消 / 确认）。 */
function RotateBlock(props: { store: EditorStore; enabled: boolean }): JSX.Element {
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
        <Button variant="primary" disabled={!props.enabled} onClick={() => props.store.closeTool()}>
          {t("common.confirm")}
        </Button>
      </div>
      <PendingNote text={t("editor.rotate.later")} />
    </section>
  );
}

/** 对比控制块：`.pd` 说「进入这个模式下**没有**控制块」——分屏与 SOOC 切换在 W5。 */
function CompareBlock(): JSX.Element {
  return (
    <section
      data-editor-tool-block="compare"
      class="flex flex-col gap-1 rounded-ui bg-surface-track p-2"
    >
      <span class="text-fs-2 text-fg-1">{t("editor.compare.result")}</span>
      <PendingNote text={t("editor.compare.later")} />
    </section>
  );
}

