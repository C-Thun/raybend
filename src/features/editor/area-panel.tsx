/**
 * **编辑区（area）面板** —— 左列顶部那一列层（`design/editor.md` 2026-10-31，
 * 画稿 `Components / Editor / 编辑区（area）` = `E6tDP`）。
 *
 * ```text
 * ┌──────────────────────────┐
 * │ 编辑区        [新控制点][新全图] │
 * ├──────────────────────────┤
 * │ ⚙ 默认             🔒      │  默认层：不可删、恒可用（右侧是锁，不是开关）
 * │ ▢ 天空              ⏻  🚫  │  全图层
 * │ ● 控制点 1          ⏻  🚫  │  控制点（圆点 = 它在画布上的颜色）
 * └──────────────────────────┘
 * ```
 *
 * # 口径（违反任何一条都要返工）
 *
 * 1. **列表自上而下 = 计算顺序**，新建层追加到末尾（`specs/editor-area.md` §4.4）；
 * 2. 行高 30、行内**不放滑杆**（范围走画布 handle）；名字超长右缘渐隐（`.fade-x-end`）；
 * 3. 删除用**禁行图标**（`AGENTS.md` §11.3），不用叉号；
 * 4. 满 32 层只灰两个新建按钮，标题不灰；
 * 5. 默认层右侧是**锁**：它恒可用，「永远开着的开关」是在骗人。
 */

import { For, Show, createSignal, onCleanup, onMount, type JSX } from "solid-js";
import {
  IconFocusCentered,
  IconLock,
  IconRefresh,
  IconSquarePlus,
} from "@tabler/icons-solidjs";

import { Button } from "../../components/ui/Button.tsx";
import { RemoveButton } from "../../components/ui/RemoveButton.tsx";
import { createRowDragSort, rowShiftTransition } from "../../components/ui/RowDragSort.tsx";
import { AreaGlyph } from "./area-glyph.tsx";
import { Switch } from "../../components/ui/Form.tsx";
import { clearFlowHint, showFlowHint } from "../../lib/flow-hint.ts";
import { t } from "../../i18n/index.ts";
import { EDITOR_LEFT_WIDTH } from "./layout.ts";
import { MAX_AREAS } from "./store.ts";
import type { EditorStore } from "./store.ts";
import type { Area } from "../../api/editor.ts";

/**
 * 控制点八色（`memory/DESIGN.md` §15.5；与 `design/editor.pen` 的 `point-0..7` 同名）。
 *
 * 色值住在 `src/styles/tokens.css`（`--point-0..7`）—— 组件里只引用令牌
 * （`lint:colors` 的规矩）；层里存的是**槽位下标**，换色板不改任何已有数据。
 */
export const POINT_COLORS = [
  "var(--point-0)",
  "var(--point-1)",
  "var(--point-2)",
  "var(--point-3)",
  "var(--point-4)",
  "var(--point-5)",
  "var(--point-6)",
  "var(--point-7)",
] as const;

export interface AreaPanelProps {
  store: EditorStore;
  enabled: boolean;
  /** 一次结构动作结束（新建 / 删除 / 排序 / 改名 / 启停）—— 工作区据此 `confirmEdit()`。 */
  onConfirm: () => void;
  /**
   * 重新取样全部控制点（修复 2026-11-02 之前的方向取样 bug）。
   * 没有控制点 / 不传时按钮不出现。
   */
  onResampleAll?: () => void;
  class?: string;
}

/** 层名的显示文本（空名字回落到类型默认名）。 */
function areaLabel(area: Area): string {
  if (area.name !== "") return area.name;
  return t(area.type === "point" ? "editor.area.point" : "editor.area.full");
}

/**
 * flowbar 提示的 owner（`lib/flow-hint.ts` 的归属串）。
 *
 * 两行共用同一个 owner：鼠标一次只能指一行，且离开时只清自己那条 —— 先划过A 再划过 B，
 * A 的 `pointerleave` 不会把 B 刚提的那句吃掉。
 */
const HINT_OWNER = "editor.area.row";

export function AreaPanel(props: AreaPanelProps): JSX.Element {
  const [renaming, setRenaming] = createSignal<number | null>(null);
  let list: HTMLDivElement | undefined;

  const rows = () => props.store.layerRows();

  /*
   * `Esc` 取消「武装」（规格 §5.6）：只把按钮点回常态，**没有层要撤**。
   * 只在真的武装时接这个键 —— 其余时候 `Esc` 该归对话框 / 工具 / 看图那些地方。
   */
  onMount(() => {
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key !== "Escape" || props.store.pointArmed() !== true) return;
      event.preventDefault();
      event.stopPropagation();
      // 只是取消武装：**没有建过任何层**，所以不需要落库
      props.store.disarmPointArea();
    };
    document.addEventListener("keydown", onKeyDown, true);
    onCleanup(() => document.removeEventListener("keydown", onKeyDown, true));
  });
  // 面板卸载（切到 LUT / 定稿面板）时别把提示留在流栏上
  onCleanup(() => clearFlowHint(HINT_OWNER));
  const armed = () => props.store.pointArmed();
  /**
   * **控制点只在 RAW 基准下可用**（崔总 2026-11-01）：取样与掩膜都以「解码后的线性像素」
   * 为前提，SOOC（相机直出位图）这条路没有那套输入。
   *
   * 这里只**禁用新建**；已有的控制点层不删不隐藏 —— 面板里置灰、画布上半透明，
   * 切回 RAW 立刻恢复（数据一个字都不改）。
   */
  const pointAvailable = () => props.store.editBase() === "raw";
  const full = () => props.store.areasFull();
  const hasPoints = () => rows().some((row) => row.kind === "area" && row.area.type === "point");

  /**
   * 整行拖动排序 —— **与 LUT 链共用同一份实现**（`components/ui/RowDragSort.tsx`）：
   * 拖动阈值、出窗取消、让位动画、目标槽、跟随光标的整行幽灵全部一致
   * （崔总 2026-11-01：系统里同一件事只允许一种手感）。
   *
   * 行内的开关 / 删除 / 改名输入不参与拖动（`[data-area-no-drag]`）。
   */
  const sort = createRowDragSort({
    list: () => list,
    rowSelector: "[data-area-row]",
    count: () => rows().length,
    noDragSelector: "[data-area-no-drag]",
    onDrop: (from, to) => {
      props.store.moveAreaRow(from, to);
      props.onConfirm();
    },
    // 幽灵 = 同一行的静态副本（不带交互）。行高 30，内容与躺着的行一致。
    ghost: (index) => {
      const row = rows()[index];
      if (row === undefined) return null;
      return row.kind === "default" ? (
        <div class="flex h-[30px] items-center gap-1.5 px-1.5">
          <AreaGlyph kind="default" />
          <span class="min-w-0 flex-1 overflow-hidden whitespace-nowrap fade-x-end text-fs-1 text-fg-1">
            {t("editor.area.default")}
          </span>
          <IconLock size={13} class="shrink-0 text-fg-3" aria-hidden="true" />
        </div>
      ) : (
        <div class="flex h-[30px] items-center gap-1.5 px-1.5">
          <AreaGlyph kind={row.area.type} color={row.area.color} size={13} />
          <span class="min-w-0 flex-1 overflow-hidden whitespace-nowrap fade-x-end text-fs-1 text-fg-1">
            {areaLabel(row.area)}
          </span>
        </div>
      );
    },
  });

  return (
    <div
      data-editor-area-panel
      class={["flex min-h-0 flex-1 flex-col bg-surface-main", props.class ?? ""]
        .filter(Boolean)
        .join(" ")}
      style={{ width: `${EDITOR_LEFT_WIDTH}px` }}
    >
      <div class="flex shrink-0 items-center gap-1 px-panel-pad pt-panel-pad pb-1.5">
        <span class="flex-1 text-fs-3 font-semibold text-fg-1">{t("editor.area.title")}</span>
        <Show when={hasPoints() && props.onResampleAll !== undefined}>
          <Button
            tip={t("editor.area.resampleAll")}
            variant="ghost"
            aria-label={t("editor.area.resampleAll")}
            disabled={!props.enabled || !pointAvailable()}
            icon={<IconRefresh size={14} />}
            onClick={() => props.onResampleAll?.()}
          />
        </Show>
        <Button
          tip={full()
            ? t("editor.area.maximum", { count: MAX_AREAS })
            : pointAvailable() ? t("editor.area.newPoint") : t("editor.area.pointNeedsRaw")}
          variant={armed() ? "accent" : "ghost"}
          selected={armed()}
          aria-label={t("editor.area.newPoint")}
          aria-pressed={armed()}
          disabled={!props.enabled || full() || !pointAvailable()}
          icon={<IconFocusCentered size={14} />}
          onClick={() => {
            if (!props.enabled || !pointAvailable()) return;
            props.store.armPointArea();
          }}
        />
        <Button
          tip={full() ? t("editor.area.maximum", { count: MAX_AREAS }) : t("editor.area.newFull")}
          variant="ghost"
          aria-label={t("editor.area.newFull")}
          disabled={!props.enabled || full()}
          icon={<IconSquarePlus size={14} />}
          onClick={() => {
            props.store.createArea();
            props.onConfirm();
          }}
        />
      </div>

      {/*
        武装态只在**标题栏按钮上**表达（主色亮起）+ flowbar 提示区那一句；
        面板里不再多一条说明，更不画「待落点幽灵行」（崔总 2026-11-01：
        点之前不该有任何层，也不该有一条虚线占位）。
      */}

      <div
        ref={(element) => { list = element; }}
        class="relative flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto px-panel-pad pb-panel-pad"
        classList={{ "select-none": sort.from() !== null }}
        data-area-list
      >
        {/* 目标槽位的占位块（共享件画的，与 LUT 链同一个观感） */}
        {sort.slot()}
        <For each={rows()}>
          {(row, index) => {
            if (row.kind === "default") {
              return (
                <div
                  data-area-row
                  data-area-default
                  class="flex h-[30px] shrink-0 items-center gap-1.5 rounded-ui px-1.5"
                  onPointerEnter={() => showFlowHint(HINT_OWNER, t("editor.area.rowHintDefault"))}
                  onPointerLeave={() => clearFlowHint(HINT_OWNER)}
                  style={{
                    transform: `translateY(${sort.shift(index())}px)`,
                    transition: rowShiftTransition(sort.from() !== null),
                    "touch-action": "none",
                  }}
                  classList={{
                    "bg-state-selected": props.store.activeAreaId() === null,
                    "hover:bg-state-hover": props.store.activeAreaId() !== null,
                    invisible: sort.from() === index(),
                  }}
                  onPointerDown={(event) => sort.start(event, index())}
                  onClick={() => props.store.selectArea(null)}
                >
                  <AreaGlyph kind="default" />
                  <span class="min-w-0 flex-1 overflow-hidden whitespace-nowrap fade-x-end text-fs-1 text-fg-1">
                    {t("editor.area.default")}
                  </span>
                  {/* 默认层恒可用：画锁，不画一个永远开着的开关（画稿 #2） */}
                  <IconLock
                    size={13}
                    class="shrink-0 text-fg-3"
                    aria-label={t("editor.area.default")}
                  />
                </div>
              );
            }
            const area = row.area;
            return (
              <div
                data-area-row
                data-area-id={area.id}
                class={[
                  "flex h-[30px] shrink-0 items-center gap-1.5 rounded-ui px-1.5",
                  props.store.activeAreaId() === area.id ? "bg-state-selected" : "hover:bg-state-hover",
                  // 关掉的层、以及 SOOC 下不参与计算的控制点：整行降透
                  area.enabled && (area.type !== "point" || pointAvailable()) ? "" : "opacity-60",
                ].join(" ")}
                classList={{ invisible: sort.from() === index() }}
                onPointerEnter={() => showFlowHint(HINT_OWNER, t("editor.area.rowHint"))}
                onPointerLeave={() => clearFlowHint(HINT_OWNER)}
                style={{
                  transform: `translateY(${sort.shift(index())}px)`,
                  transition: rowShiftTransition(sort.from() !== null),
                  "touch-action": "none",
                }}
                onPointerDown={(event) => sort.start(event, index())}
                onClick={() => props.store.selectArea(area.id)}
                onDblClick={() => {
                  if (!props.enabled) return;
                  setRenaming(area.id);
                }}
              >
                <AreaGlyph kind={area.type} color={area.color} size={13} />
                <Show
                  when={renaming() === area.id}
                  fallback={
                    <span class="min-w-0 flex-1 overflow-hidden whitespace-nowrap fade-x-end text-fs-1 text-fg-1">
                      {areaLabel(area)}
                    </span>
                  }
                >
                  <input
                    data-area-rename
                    class="h-6 min-w-0 flex-1 rounded-ui bg-surface-bar px-1.5 text-fs-1 text-fg-1 outline-none"
                    value={area.name}
                    maxLength={30}
                    aria-label={t("editor.area.namePlaceholder")}
                    ref={(element) => {
                      queueMicrotask(() => {
                        element.focus();
                        element.select();
                      });
                    }}
                    onPointerDown={(event) => event.stopPropagation()}
                    onClick={(event) => event.stopPropagation()}
                    onBlur={(event) => {
                      const name = event.currentTarget.value.trim();
                      if (name !== area.name) {
                        props.store.renameArea(area.id, name);
                        props.onConfirm();
                      }
                      setRenaming(null);
                    }}
                    onKeyDown={(event) => {
                      if (event.key === "Enter" && !event.isComposing) {
                        event.preventDefault();
                        event.currentTarget.blur();
                      } else if (event.key === "Escape") {
                        event.preventDefault();
                        event.currentTarget.value = area.name;
                        event.currentTarget.blur();
                      }
                    }}
                  />
                </Show>
                <Switch
                  checked={area.enabled}
                  disabled={!props.enabled}
                  label={areaLabel(area)}
                  onCheckedChange={(enabled) => {
                    props.store.setAreaEnabled(area.id, enabled);
                    props.onConfirm();
                  }}
                />
                {/* 删除 = **全项目唯一**那一枚禁行图标（`RemoveButton`，AGENTS §11.3）：
                    导入「最近」、area、定稿、LUT 四处必须是同一枚 —— 不要再手写 IconBan / IconTrash。 */}
                <RemoveButton
                  label={t("editor.area.remove")}
                  disabled={!props.enabled}
                  onClick={() => {
                    props.store.removeArea(area.id);
                    props.onConfirm();
                  }}
                />
              </div>
            );
          }}
        </For>

      </div>
      {/* 跟光标的整行幽灵（与 LUT 链同一份实现：Portal + fixed，只做视觉） */}
      {sort.ghost()}
    </div>
  );
}
