/**
 * 编辑左栏：**LUT 面板**（`design/editor.md` §3.1、`prompts/editor.pd`）。
 *
 * ```text
 * LUT                          [+分类] [导入]
 * ├ Travel (3)     ▼          ← 点标题展开；展开时其他分类自动收起；再点全部收起
 * │  ┌────┐ ┌────┐            ← 一行 2 个 tile：上面预览、下面名字
 * │  │预览│ │预览│
 * │  └────┘ └────┘
 * │  名字    名字
 * └ Portrait (0)
 * ```
 *
 * M3-W6c：分类和条目写入 `app.db`；导入后按 4:3 WebP 封面展示，
 * 选中 LUT 与应用开关随 latest 编辑栈保存。
 *
 * 面板宽度**固定 280、不可拖**（`.pd`：「占据整个left，不可调宽度」）——
 * 所以这里没有把手，也不需要 `SplitHandle`。
 */

import { For, Index, Show, createSignal, createEffect, onCleanup, onMount, type JSX } from "solid-js";
import type { LutLayer } from "../../api/editor.ts";
import { SLIDER_KEYS, createGestureEnd } from "../../lib/slider-gesture.ts";
import {
  IconCheck,
  IconChevronDown,
  IconChevronRight,
  IconFolderPlus,
  IconPalette,
  IconUpload,
} from "@tabler/icons-solidjs";

import { Button } from "../../components/ui/Button.tsx";
import { Slider } from "../../components/ui/Slider.tsx";
import { RemoveButton } from "../../components/ui/RemoveButton.tsx";
import { createRowDragSort, rowShiftTransition } from "../../components/ui/RowDragSort.tsx";
import { Switch } from "../../components/ui/Form.tsx";
import { EasyDestroyButton } from "../../components/ui/EasyDestroy.tsx";
import { Dialog } from "../../components/ui/Dialog.tsx";
import { t } from "../../i18n/index.ts";
import { lutCount, visibleLutCategories, type LutEntry } from "../../lib/lut-library.ts";
import { MAX_LUT_LAYERS } from "./store.ts";
import {
  cancelLutImport,
  getLutCover,
  onLutImportProgress,
  type LutImportProgress,
} from "../../api/lut.ts";
import { LutImportProgressView } from "./lut-import-progress.tsx";
import type { EditorStore } from "./store.ts";
import { EDITOR_LEFT_WIDTH } from "./layout.ts";

export interface LutPanelProps {
  store: EditorStore;
  class?: string;
  onCreateCategory?: (name: string) => Promise<boolean>;
  onImport?: (categoryId: string) => Promise<string | null>;
  onToggle?: () => void;
  /** 链发生变化（一次手势 = 一次调用）—— 工作区据此 `confirmEdit()`。 */
  onChainChange?: () => void;
  onHide?: (id: string) => Promise<void>;
}

export function LutPanel(props: LutPanelProps): JSX.Element {
  const [creating, setCreating] = createSignal(false);
  const [draftName, setDraftName] = createSignal("");
  const [duplicate, setDuplicate] = createSignal(false);
  const [importOpen, setImportOpen] = createSignal(false);
  const [importCategory, setImportCategory] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [stopping, setStopping] = createSignal(false);
  const [importProgress, setImportProgress] = createSignal<LutImportProgress | null>(null);
  const [importSummary, setImportSummary] = createSignal<string | null>(null);
  /*
   * 拖动排序（指针手势，**不用 HTML5 DnD**：原生拖影是浏览器截图、没法做「整行提起」）。
   *
   * 拖动中：源行隐形（保留占位），被拖的行本体改为 fixed 幽灵跟着光标；
   * 其它行用 transform 让位（140ms 过渡），目标槽位画一块主色占位块。
   * 松手才落库（撤销的一个节点）。
   */
  let chainList: HTMLDivElement | undefined;

  /*
   * 进度走 Rust 的事件（`lut://import-progress`）。
   * 订阅放在面板挂载时：**必须在发命令之前就位**，否则前几条事件会丢
   * （`listen` 是异步的，而用户点「导入」的时刻我们控制不了）。
   */
  onMount(() => {
    let stop: (() => void) | undefined;
    let disposed = false;
    void onLutImportProgress((progress) => setImportProgress(progress)).then((unsubscribe) => {
      if (disposed) unsubscribe();
      else stop = unsubscribe;
    });
    onCleanup(() => {
      disposed = true;
      stop?.();
    });
  });

  const importCategoryName = (): string =>
    categories().find((category) => category.id === importCategory())?.name ?? importCategory();

  /**
   * 跑一次导入。
   *
   * ❗**只有真导过（拿到摘要）才关窗**：取消目录选择 / 出错时返回 `null`，
   * 那时关窗会让人以为「点了没反应」。
   */
  async function submitImport(): Promise<void> {
    if (busy()) return;
    setBusy(true);
    setStopping(false);
    setImportSummary(null);
    // 第一条事件还没到（含原生目录选择器开着的那段）→ 界面说「正在准备…」，不伪造扫描进度
    setImportProgress(null);
    try {
      const summary = (await props.onImport?.(importCategory())) ?? null;
      setImportSummary(summary);
      if (summary !== null) setImportOpen(false);
    } finally {
      setBusy(false);
      setStopping(false);
      setImportProgress(null);
    }
  }

  /** 底栏「取消」：导入中 = **真中止**（当前文件完成后停，已导入的保留）；否则 = 关窗。 */
  async function requestCancelImport(): Promise<void> {
    if (!busy()) {
      setImportOpen(false);
      return;
    }
    if (stopping()) return;
    setStopping(true);
    await cancelLutImport();
  }

  const categories = () => visibleLutCategories(props.store.lutCategories());
  const total = () => lutCount(categories());
  /** 链里有没有「文件已不可用」的层（有就提示，但**不禁用**其它层）。 */
  const chainMissing = (): boolean => {
    const entries = props.store.lutCategories().flatMap((category) => category.entries);
    return props.store.lutChain().some(
      (layer) => entries.find((entry) => entry.id === layer.id)?.available !== true,
    );
  };
  const entryName = (id: string): string =>
    props.store.lutCategories().flatMap((category) => category.entries)
      .find((entry) => entry.id === id)?.name ?? id;
  /** 链的写入口：**一次手势 = 一次 confirm**（撤销的一个节点）。 */
  const editChain = (mutate: () => void): void => {
    mutate();
    props.onChainChange?.();
  };

  function openCreate(): void {
    setDraftName("");
    setDuplicate(false);
    setCreating(true);
  }

  async function submitCreate(): Promise<void> {
    if (busy()) return;
    setBusy(true);
    try {
      const created = props.onCreateCategory === undefined
        ? props.store.addCategory(draftName()) : await props.onCreateCategory(draftName());
      if (!created) { setDuplicate(true); return; }
      setCreating(false);
    } finally { setBusy(false); }
  }

  /**
   * 整行拖动排序 —— **共享实现**（`components/ui/RowDragSort.tsx`，与编辑区层同一份）。
   *
   * 行内的开关 / 拉杆 / 删除不参与整行拖动（`label` 必须列进来：Ark 的 Switch 根节点
   * 就是 `<label>`，漏掉它的话「点开关」会先起一次整行拖动）。
   */
  const sort = createRowDragSort({
    list: () => chainList,
    rowSelector: "[data-lut-layer]",
    count: () => props.store.lutChain().length,
    noDragSelector: "[data-lut-no-drag]",
    scrollSelector: "[data-lut-scroll]",
    onDrop: (from, to) => editChain(() => props.store.moveLutLayer(from, to)),
    ghost: (index) => {
      const layer = props.store.lutChain()[index];
      if (layer === undefined) return null;
      return (
        <LayerRowBody
          layer={layer}
          index={index}
          name={entryName(layer.id)}
          onToggle={() => undefined}
          onStrength={() => undefined}
          onStrengthCommit={() => undefined}
          onStrengthReset={() => undefined}
          onRemove={() => undefined}
        />
      );
    },
  });

  return (
    <div
      data-editor-lut-panel
      class={["flex min-h-0 flex-1 flex-col bg-surface-main", props.class ?? ""]
        .filter(Boolean)
        .join(" ")}
      style={{ width: `${EDITOR_LEFT_WIDTH}px` }}
    >
      {/*
        标题行：「LUT」+ 行尾两个图标按钮（新建分类 / 导入 LUT）。
        文字按钮会和 LUT 名挤在一起，所以用图标 + 内置提示（`design/editor.md` §3.1）。
        ❗提示走按钮自己的 `tip`，**不要在外面再套一层 `<Tooltip>`** —— 那就是两台
        tooltip machine 抢同一指针路径（2026-10-11 修掉的「移到下一个按钮不弹」）。
      */}
      <div class="flex shrink-0 items-center gap-1 px-panel-pad pt-panel-pad pb-1.5">
        <span class="flex-1 text-fs-3 font-semibold text-fg-1">
          {t("editor.lut.title")}
        </span>
        <Button
          tip={t("editor.lut.newCategory")}
          variant="ghost"
          aria-label={t("editor.lut.newCategory")}
          icon={<IconFolderPlus size={14} />}
          onClick={openCreate}
        />
        <Button
          tip={t("editor.lut.import")}
          variant="ghost"
          data-lut-import
          aria-label={t("editor.lut.import")}
          icon={<IconUpload size={14} />}
          onClick={() => { setImportCategory(categories()[0]?.id ?? ""); setImportOpen(true); }}
        />
      </div>

      <div class="flex shrink-0 items-center justify-between px-panel-pad pb-1 text-fs-1 text-fg-2">
        <span>{t("editor.lut.apply")}</span>
        {/*
          开关一律用全系统同一份 `Switch`（`memory/DESIGN.md` 的控件分工）。
          这里曾经是一颗原生 `<input type="checkbox">`（崔总多次指出）—— 原生控件
          不吃主题令牌，深浅色下都足一截，而且键盘/读屏行为跟别处不一致。
        */}
        <Switch
          checked={props.store.lutEnabled()}
          disabled={props.store.lutChain().length === 0}
          onCheckedChange={(checked) => {
            props.store.setLutEnabled(checked);
            props.onToggle?.();
          }}
          label={t("editor.lut.apply")}
        />
      </div>
      <Show when={importSummary()}><p role="status" class="px-panel-pad pb-2 text-fs-0 text-fg-2">{importSummary()}</p></Show>
      <Show when={chainMissing()}><p class="px-panel-pad pb-1 text-fs-0 text-danger">{t("editor.lut.selectedMissing")}</p></Show>
      <div
        data-lut-scroll
        class="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto px-panel-pad pb-panel-pad"
      >
        <Show when={props.store.lutChain().length > 0}>
          <div class="shrink-0 px-panel-pad pb-1">
            <div
              data-lut-chain
              ref={(element) => { chainList = element; }}
              class="relative flex flex-col gap-1.5"
              classList={{ "select-none": sort.from() !== null }}
            >
              {/* 目标槽位的占位块（共享件画的） */}
              {sort.slot()}
              {/*
                **必须是 `<Index>`，不能换成 `<For>`**（2026-10-31 的 bug）：
                `<For>` 按对象引用比对，而强度每拖一格 store 都会换掉那一条 layer 对象，
                于是整行 DOM 被销毁重建 —— Ark 滑块的指针捕获跟着没了，拖一下就断、
                一次只能动几个百分点。`<Index>` 按**位置**复用 DOM，行内容走响应式更新。
                拖动排序本来就是按位置算位移（`lib/row-drag-sort.ts`），语义也正好对上。
              */}
              <Index each={props.store.lutChain()}>
                {(layer, index) => (
                  <div
                    data-lut-layer={layer().id}
                    class="relative rounded-ui bg-surface-track"
                    classList={{ invisible: sort.from() === index }}
                    style={{
                      transform: `translateY(${sort.shift(index)}px)`,
                      transition: rowShiftTransition(sort.from() !== null),
                      "touch-action": "none",
                    }}
                    onPointerDown={(event) => sort.start(event, index)}
                  >
                    <LayerRowBody
                      layer={layer()}
                      index={index}
                      name={entryName(layer().id)}
                      onToggle={(enabled) => editChain(() => props.store.setLutLayerEnabled(index, enabled))}
                      onStrength={(strength) => props.store.setLutLayerStrength(index, strength)}
                      onStrengthCommit={() =>
                        editChain(() => props.store.commitLutLayerStrength(
                          index, props.store.lutChain()[index]?.strength ?? 0))}
                      onStrengthReset={() =>
                        editChain(() => props.store.commitLutLayerStrength(index, 1))}
                      onRemove={() => editChain(() => props.store.removeLutLayer(index))}
                    />
                  </div>
                )}
              </Index>
            </div>
          </div>
        </Show>
          <Show
            when={categories().length > 0}
            fallback={
              <div
                class="flex flex-1 flex-col items-center justify-center gap-2 px-4 text-center"
                data-lut-empty
              >
                <IconPalette size={48} stroke-width={1} class="text-fg-3 opacity-60" aria-hidden="true" />
                <p class="text-fs-2 text-fg-2">{t("editor.lut.empty")}</p>
                <p class="text-fs-0 text-fg-3">{t("editor.lut.emptyHint")}</p>
              </div>
            }
          >
            <For each={categories()}>
              {(category) => {
                const expanded = (): boolean => props.store.expandedCategory() === category.id;
                return (
                  <div data-lut-category={category.id} class="flex flex-col">
                    {/* 分类标题行：chevron + 名字 + 数量 */}
                    <button
                      type="button"
                      class="flex h-row-h w-full items-center gap-1.5 rounded-ui px-1.5 text-left hover:bg-state-hover"
                      aria-expanded={expanded()}
                      onClick={() => props.store.toggleCategory(category.id)}
                    >
                      <Show
                        when={expanded()}
                        fallback={<IconChevronRight size={12} class="shrink-0 text-fg-3" />}
                      >
                        <IconChevronDown size={12} class="shrink-0 text-fg-3" />
                      </Show>
                      <span class="min-w-0 flex-1 overflow-hidden whitespace-nowrap fade-x-end text-fs-2 text-fg-1">
                        {category.name}
                      </span>
                      <span class="shrink-0 text-fs-0 tabular-nums text-fg-3">
                        {t("editor.lut.count").replace("{n}", String(category.entries.length))}
                      </span>
                    </button>

                    <Show when={expanded()}>
                      <Show
                        when={category.entries.length > 0}
                        fallback={
                          <p class="px-1.5 py-2 text-fs-0 text-fg-3">
                            {t("editor.lut.emptyCategory")}
                          </p>
                        }
                      >
                        <div class="grid grid-cols-2 gap-2 px-1.5 py-1.5">
                          <For each={category.entries}>
                            {(entry) => <LutTile entry={entry}
                              base={props.store.lutChain()[0]?.id === entry.id}
                              inChain={props.store.lutChain().some((layer) => layer.id === entry.id)}
                              canAdd={props.store.lutChain().length < MAX_LUT_LAYERS}
                              onSelect={() => editChain(() => props.store.selectLutAsBase(entry.id))}
                              onToggleChain={() => editChain(() => props.store.toggleLutInChain(entry.id))}
                              onHide={() => props.onHide?.(entry.id)} />}
                          </For>
                        </div>
                      </Show>
                    </Show>
                  </div>
                );
              }}
            </For>

          </Show>
        </div>

        {/* 跟光标的整行幽灵（共享件：Portal + fixed，只做视觉） */}
        {sort.ghost()}

        {/*
          导入弹窗：选择分类 → 跑导入。
          导入期间换成**进度状态**（扫描中走不确定态、复制中给确定值），
          底栏的「取消」是真中止（已导入的保留）—— 画稿 `design/editor.md` §3.1.2。
          关窗（Esc / 遮罩）与「取消」同一条口径：不能“关掉窗口让它偷偷继续”。
        */}
        <Dialog
          open={importOpen()}
          onOpenChange={(open) => {
            if (open) setImportOpen(true);
            else void requestCancelImport();
          }}
          title={t("editor.lut.import")}
          footer={
            <>
              <Button variant="secondary" disabled={stopping()} onClick={() => void requestCancelImport()}>
                {t("common.cancel")}
              </Button>
              <Button
                variant="primary"
                disabled={busy() || importCategory() === ""}
                onClick={() => void submitImport()}
              >
                {t("editor.lut.import")}
              </Button>
            </>
          }
        >
          <Show
            when={busy()}
            fallback={
              <label class="flex flex-col gap-2 text-fs-1 text-fg-2">
                {t("editor.lut.category")}
                <select
                  class="rounded-ui bg-surface-track px-2 py-1.5 text-fg-1"
                  value={importCategory()}
                  onChange={(event) => setImportCategory(event.currentTarget.value)}
                >
                  <For each={categories()}>
                    {(category) => <option value={category.id}>{category.name}</option>}
                  </For>
                </select>
              </label>
            }
          >
            <Show
              when={importProgress()}
              fallback={
                // 目录选择器还开着 / 命令刚发出，还没有第一条事件
                <p role="status" class="text-fs-1 text-fg-2">{t("import.preparing")}</p>
              }
            >
              <LutImportProgressView
                progress={importProgress()}
                stopping={stopping()}
                categoryName={importCategoryName()}
              />
            </Show>
          </Show>
        </Dialog>

        {/* 库里一个 LUT 都没有时的说明（分类建了但空着时也有用） */}
      <Show when={categories().length > 0 && total() === 0}>
        <p class="shrink-0 px-panel-pad pb-panel-pad text-fs-0 text-fg-3">
          {t("editor.lut.emptyHint")}
        </p>
      </Show>

      {/* 新建分类：一个小模态（与全系统的弹窗同一套 —— 取消在左、确认在右） */}
      <Dialog
        open={creating()}
        onOpenChange={(open) => setCreating(open)}
        title={t("editor.lut.newCategoryTitle")}
        footer={
          <>
            <Button variant="secondary" onClick={() => setCreating(false)}>
              {t("common.cancel")}
            </Button>
            <Button
              variant="primary"
              disabled={draftName().trim() === ""}
              onClick={submitCreate}
            >
              {t("common.confirm")}
            </Button>
          </>
        }
      >
        <label class="flex flex-col gap-1.5">
          <span class="text-fs-2 text-fg-2">{t("editor.lut.newCategory")}</span>
          <input
            data-lut-category-input
            class="h-row-h rounded-ui bg-surface-track px-2 text-fs-2 text-fg-1 outline-none focus-visible:ring-1 focus-visible:ring-focus-ring"
            placeholder={t("editor.lut.categoryPlaceholder")}
            value={draftName()}
            autofocus
            onInput={(event) => {
              setDraftName(event.currentTarget.value);
              setDuplicate(false);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                submitCreate();
              }
            }}
          />
        </label>
        <Show when={duplicate()}>
          <p data-lut-category-duplicate class="mt-1.5 text-fs-0 text-danger">
            {t("editor.lut.categoryExists")}
          </p>
        </Show>
      </Dialog>
    </div>
  );
}

/** 载入某支 LUT 的封面（分类 tile 与链行的缩略图共用这一份取图逻辑）。 */
function useLutCover(id: () => string): () => string | null {
  const [cover, setCover] = createSignal<string | null>(null);
  createEffect(() => {
    const value = id();
    let active = true;
    void getLutCover(value).then((bytes) => {
      if (!active || bytes === null) return;
      const url = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: "image/webp" }));
      setCover((previous) => { if (previous !== null) URL.revokeObjectURL(previous); return url; });
    }).catch(() => undefined);
    onCleanup(() => { active = false; const previous = cover(); if (previous !== null) URL.revokeObjectURL(previous); });
  });
  return cover;
}

/** 链行左侧的缩略图（56×42，占满两层高；口径与分类 tile 的封面同一份）。 */
function LutRowThumb(props: { id: string }): JSX.Element {
  const cover = useLutCover(() => props.id);
  return <div class="aspect-[4/3] h-[42px] w-[56px] shrink-0 overflow-hidden rounded-[2px] bg-surface-bar">
    <Show when={cover()}>{(url) => <img src={url()} alt="" class="h-full w-full object-cover" />}</Show>
  </div>;
}

/**
 * 应用层一行的本体（缩略图 + 名称/单层开关 + 强度/数值/删除）。
 *
 * 单独成组件是因为拖动幽灵要把**同一份内容**再挂到一个 `fixed` 层上：
 * 写两遍就会「拖起来的样子和躺着的不是同一个东西」（`AGENTS.md` §2.12）。
 */
function LayerRowBody(props: {
  layer: LutLayer;
  index: number;
  name: string;
  onToggle: (enabled: boolean) => void;
  onStrength: (strength: number) => void;
  /** **手势真的结束了**（指针松开 / 键盘收尾）—— 不是 Zag 的 `onValueChangeEnd`（见下）。 */
  onStrengthCommit: () => void;
  /** 双击强度行 = 回到 100% 并开启该层。 */
  onStrengthReset: () => void;
  onRemove: () => void;
}): JSX.Element {
  /*
   * 「松手」只认自己的手势（`lib/slider-gesture.ts`）。
   *
   * ❗不要改回 Zag 的 `onValueCommit`：它在**外部 value 回灌**时也会派发，
   * 而我们每拖一格都会写 store → 值回灌 → 于是每格都「提交」一次：
   * 拖到 0% 的那一格就把该层关掉；层重新打开后任何一次回灌又会按旧值改写开关
   * （2026-10-31 崔总报的「点开关把效果抬到 100%」）。
   */
  const gesture = createGestureEnd(() => props.onStrengthCommit());
  onCleanup(() => gesture.dispose());
  return (
    <div class="group/row flex items-center gap-1.5 p-1.5">
      <LutRowThumb id={props.layer.id} />
      <div class="flex min-w-0 flex-1 flex-col gap-1">
        <div class="flex items-center gap-1.5">
          <span class="min-w-0 flex-1 overflow-hidden whitespace-nowrap fade-x-end text-fs-0 text-fg-1">
            {props.name}
          </span>
          <Switch
            checked={props.layer.enabled}
            onCheckedChange={props.onToggle}
            label={t("editor.lut.layerEnabled")}
          />
        </div>
        <div
          data-lut-no-drag
          class="flex items-center gap-1.5"
          onPointerDown={(event) => {
            // 删除按钮在这一行里，点它不算滑杆手势
            if (event.button !== 0) return;
            if ((event.target as HTMLElement | null)?.closest("button") !== null) return;
            gesture.start(event);
          }}
          onKeyUp={(event) => {
            // 键盘调整：松键即收尾（Zag 在按键时已经改过值）
            if (SLIDER_KEYS.has(event.key)) props.onStrengthCommit();
          }}
          onDblClick={(event) => {
            // 与曝光/反差等参数同口径：双击回到默认 100%。
            // 走 commit 版本：关着的层也会被这次「回到 100%」一并打开。
            event.preventDefault();
            event.stopPropagation();
            props.onStrengthReset();
          }}
        >
          <Slider
            class="min-w-0 flex-1"
            value={Math.round(props.layer.strength * 100)}
            min={0}
            max={100}
            label={t("editor.lut.strength")}
            onValueChange={(value) => props.onStrength(value / 100)}
            // 开关只由「手势结束」那一步落定（0% = 关、>0% = 开，只动这一层，不碰总开关）
          />
          <span class="w-8 shrink-0 text-right text-fs-0 tabular-nums text-fg-2">
            {Math.round(props.layer.strength * 100)}%
          </span>
          <RemoveButton
            class="opacity-0 transition-opacity group-hover/row:opacity-100 group-focus-within/row:opacity-100"
            label={t("editor.lut.removeLayer")}
            onClick={props.onRemove}
          />
        </div>
      </div>
    </div>
  );
}

/**
 * 分类列表里的一支 LUT（`design/editor.md` §3.1.3 的两条通道）。
 *
 * * 点本体 = **换到第 1 层**（底层）：封面 1px 主色内描边；
 * * 点右上角圆圈 = **加/减层**：在链里常显实心勾，不在链里则指向时出现空圈。
 *
 * 两个命中区分开（不能嵌套按钮）：外层是 `div`，tile 与圆圈是两个并列的 `<button>`。
 */
function LutTile(props: {
  entry: LutEntry;
  base: boolean;
  inChain: boolean;
  canAdd: boolean;
  onSelect: () => void;
  onToggleChain: () => void;
  onHide: () => void | Promise<void>;
}): JSX.Element {
  const cover = useLutCover(() => props.entry.id);
  return <div class="group relative min-w-0">
    <button type="button" disabled={props.entry.available === false} onClick={props.onSelect}
      data-lut-tile={props.entry.id}
      class="block aspect-[4/3] w-full overflow-hidden rounded-ui bg-surface-bar disabled:opacity-50"
      classList={{ "ring-1 ring-brand": props.base }}>
      <Show when={cover()}>{(url) => <img src={url()} alt="" class="h-full w-full object-cover" />}</Show>
    </button>
    {/*
      圆圈：**定尺寸的圆**（以前是文本 `✓` 靠行高撑高，成了竖椭圆 —— 崔总 2026-09-28 报的）。
      在链里 → 实心 + 勾（常显）；不在链里 → 指向才出现的空圈（加一层）。
    */}
    <Show when={props.entry.available !== false}>
      <button
        type="button"
        data-lut-chain-toggle={props.entry.id}
        aria-label={t("editor.lut.toggleChain")}
        aria-pressed={props.inChain}
        disabled={!props.inChain && !props.canAdd}
        onClick={(event) => { event.stopPropagation(); props.onToggleChain(); }}
        class={[
          "absolute right-1 top-1 flex size-4.5 items-center justify-center rounded-full transition-opacity",
          "disabled:opacity-30",
          props.inChain
            ? "bg-brand text-fg-on-brand opacity-100"
            : "bg-surface-main/90 text-brand ring-1 ring-brand opacity-0 group-hover:opacity-100 group-focus-within:opacity-100",
        ].join(" ")}
      >
        <Show when={props.inChain}><IconCheck size={12} /></Show>
      </button>
    </Show>
    <div class="flex items-center gap-0.5">
      <span class="min-w-0 flex-1 overflow-hidden whitespace-nowrap fade-x-end text-center text-fs-0 text-fg-2" title={props.entry.name}>{props.entry.name}</span>
      <EasyDestroyButton class="opacity-0 group-hover:opacity-100 group-focus-within:opacity-100"
        label={t("editor.lut.remove")}
        confirmTitle={t("editor.lut.removeTitle")} confirmLabel={t("editor.lut.removeAction")}
        confirmMessage={t("editor.lut.removeConfirm").replace("{name}", props.entry.name)}
        onRemove={props.onHide} />
    </div>
    <Show when={props.entry.available === false}><span class="text-fs-0 text-danger">{t("editor.lut.missing")}</span></Show>
  </div>;
}
