/**
 * `SuggestInput` —— **带建议列表的行内输入框**（崔总 2026-11-03 定的填入方式）。
 *
 * 用在右栏那些「基本靠人定、没有可读来源」的字段（作者 / 描述 / 地理四项）：
 * focus 就在下面弹出候选列表（最近输入 + 库内已有值），输入即前缀筛选，
 * `↑`/`↓` 选，`Enter` 采用，`Esc` **先关列表**（再按一次才是取消编辑）。
 *
 * 为什么不用 Ark 的 Combobox：这里的控件本质是「**自由文本 + 可选建议**」——
 * 值永远以输入框里那串字为准，候选只是加速；而 Ark 的 combobox 把「选中集合」
 * 当第一公民，与「失焦即提交」「回车即提交」的行内编辑语义会互相拉扯。
 * 键盘模型与 `TagDialog` 的输入即搜**完全一致**（同一套 ↑↓/Enter 处理），
 * 筛选逻辑在 `lib/suggest.ts`（纯函数、有单测）——复用的是那两处，不是新写一套。
 *
 * 列表走 `Portal` + `position: fixed`：右栏是滚动容器，列表放在里面会被裁掉。
 */

import { createEffect, createMemo, createSignal, For, onCleanup, onMount, Show, type JSX } from "solid-js";
import { Portal } from "solid-js/web";

import { filterSuggestions, moveHighlight, type Suggestion } from "../../lib/suggest.ts";
import { createTokenPx } from "./tokens.ts";

export interface SuggestInputProps {
  /** 进入编辑时的初值 */
  value: string;
  /** 无障碍名（字段名，如「作者」） */
  label: string;
  placeholder?: string;
  /** 候选（已合并好，顺序即展示顺序） */
  suggestions: readonly Suggestion[];
  maxLength?: number;
  disabled?: boolean;
  class?: string;
  /** 提交（回车 / 选中候选 / 失焦）；空串表示清空该字段 */
  onCommit: (value: string) => void;
  /** 取消（Esc 且列表已收起） */
  onCancel: () => void;
}

/** 列表最多显示几行（超出滚动）。8 行 ≈ 一屏可见的常用值。 */
const VISIBLE_ROWS = 8;

export function SuggestInput(props: SuggestInputProps): JSX.Element {
  const [draft, setDraft] = createSignal(props.value);
  const [open, setOpen] = createSignal(props.suggestions.length > 0);
  const [focused, setFocused] = createSignal(false);
  const [highlight, setHighlight] = createSignal(-1);
  const [rect, setRect] = createSignal<{ left: number; top: number; width: number } | null>(null);
  let inputEl: HTMLInputElement | undefined;
  const rowHeight = createTokenPx("--row-h", 26);

  // 行内编辑的入口是「点值」：输入框挂上后必须能直接打字（旧实现也有这一步）。
  onMount(() => {
    inputEl?.focus();
    inputEl?.select();
  });

  // 候选可能比 focus 晚到（库内统计是一次 IPC）：到达时若仍在输入框里，自动展开。
  // Esc 主动关列表后不在本次编辑里反复抢回来；下一次输入/聚焦才重新允许。
  let dismissed = false;
  const filtered = createMemo(() => filterSuggestions(props.suggestions, draft()));
  createEffect(() => {
    if (focused() && props.suggestions.length > 0 && !dismissed) setOpen(true);
  });

  /** 贴在输入框下方；下面放不下就翻到上方（右栏常常已经在屏幕底部）。 */
  const measure = (): void => {
    if (inputEl === undefined) return;
    const box = inputEl.getBoundingClientRect();
    const estimated = Math.min(filtered().length, VISIBLE_ROWS) * rowHeight() + 10;
    const below = box.bottom + 4;
    const top = below + estimated > window.innerHeight ? Math.max(4, box.top - estimated - 4) : below;
    setRect({ left: box.left - 4, top, width: box.width + 8 });
  };

  createEffect(() => {
    if (!open()) return;
    measure();
    window.addEventListener("scroll", measure, true);
    window.addEventListener("resize", measure);
    onCleanup(() => {
      window.removeEventListener("scroll", measure, true);
      window.removeEventListener("resize", measure);
    });
  });
  // 候选变了（库里刚写入新值）也要重新量一次高度
  createEffect(() => {
    filtered();
    if (open()) measure();
  });

  /**
   * 一次编辑只允许有一个结局（提交或取消）。
   *
   * 为什么需要：回车提交换来的「父组件卸载输入框」可能再触发一次 `blur`，
   * 而取消（Esc）卸载也可能触发 `blur` —— 没有这道闸，一次输入会发两次写库。
   */
  let finished = false;
  const commit = (value: string): void => {
    if (finished) return;
    finished = true;
    setOpen(false);
    props.onCommit(value);
  };
  const cancel = (): void => {
    if (finished) return;
    finished = true;
    setOpen(false);
    props.onCancel();
  };

  const pick = (suggestion: Suggestion): void => {
    commit(suggestion.value);
    // 选完把焦点交回调用方（行内编辑框会被卸载）
  };

  const onKeyDown = (event: KeyboardEvent): void => {
    // 中文输入法确认候选词的回车/上箭头不能当命令用（与编辑器里其它输入框同一条守卫）
    if (event.isComposing) return;
    if (event.key === "ArrowDown") {
      event.preventDefault();
      if (!open()) setOpen(true);
      setHighlight((current) => moveHighlight(current, 1, filtered().length));
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      setHighlight((current) => moveHighlight(current, -1, filtered().length));
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      const index = highlight();
      const chosen = open() ? filtered()[index] : undefined;
      commit(chosen?.value ?? draft());
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      // 列表**看得见**时才关列表；列表本来就没显示（无候选）就直接取消编辑
      if (open() && filtered().length > 0) {
        setOpen(false);
        dismissed = true;
        return;
      }
      cancel();
    }
  };

  return (
    <>
      <input
        ref={inputEl}
        class={[
          "min-w-0 flex-1 rounded-ui bg-surface-bar px-1.5 py-0.5 text-fs-2 text-fg-1 outline-none",
          props.class ?? "",
        ]
          .filter(Boolean)
          .join(" ")}
        value={draft()}
        aria-label={props.label}
        aria-autocomplete="list"
        aria-expanded={open() && filtered().length > 0}
        maxLength={props.maxLength}
        disabled={props.disabled}
        placeholder={props.placeholder}
        onInput={(event) => {
          setDraft(event.currentTarget.value);
          setHighlight(-1);
          dismissed = false;
          if (!open()) setOpen(true);
        }}
        onFocus={() => {
          setFocused(true);
          dismissed = false;
          setOpen(props.suggestions.length > 0);
        }}
        onBlur={() => {
          setFocused(false);
          commit(draft());
        }}
        onKeyDown={onKeyDown}
      />
      <Portal>
        <Show when={open() && rect() && filtered().length > 0}>
          <div
            data-suggest-list
            role="listbox"
            aria-label={props.label}
            class="fixed z-(--z-popover) overflow-y-auto rounded-ui bg-surface-layer py-1 inset-ring-(length:--dialog-ring-w) inset-ring-(--dialog-ring)"
            style={{
              left: `${rect()!.left}px`,
              top: `${rect()!.top}px`,
              width: `${rect()!.width}px`,
              "max-height": `calc(var(--row-h) * ${VISIBLE_ROWS} + 8px)`,
            }}
            /*
             * 列表容器**不能**整体 preventDefault：那会把滚动条的拖动也吃掉。
             * 只在候选项上阻止默认行为（见每一项的 `onPointerDown`），
             * 这样既不让输入框在点击候选时提前 blur，也保留滚轮/滚动条。
             */
          >
            <For each={filtered()}>
              {(item, index) => (
                <div
                  role="option"
                  aria-selected={highlight() === index()}
                  data-suggest-item
                  class={[
                    "flex h-row-h cursor-pointer items-center gap-2 px-2 text-fs-2",
                    highlight() === index()
                      ? "bg-state-hover text-fg-1"
                      : "text-fg-2",
                  ].join(" ")}
                  onPointerEnter={() => setHighlight(index())}
                  // 按下候选时保持输入框焦点；否则 blur 会先于 click 提交草稿。
                  onPointerDown={(event) => event.preventDefault()}
                  onClick={() => pick(item)}
                >
                  <span class="min-w-0 flex-1 overflow-hidden whitespace-nowrap fade-x-end">
                    {item.value}
                  </span>
                  {/* 库内已有值显示「用过几次」；只来自最近输入的没有次数 */}
                  <Show when={item.count !== null}>
                    <span class="tnum shrink-0 text-fs-0 text-fg-3">{item.count}</span>
                  </Show>
                </div>
              )}
            </For>
          </div>
        </Show>
      </Portal>
    </>
  );
}
