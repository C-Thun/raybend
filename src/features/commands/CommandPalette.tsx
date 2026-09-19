/**
 * 命令面板（`BROWSE.md` §8 浮层、`plans/M2-W3.md` §4 阶段 1）。
 *
 * ```text
 * ┌──────────────────────────────────────────────┐
 * │ 🔍 输入命令…                                  │
 * ├──────────────────────────────────────────────┤
 * │ 视图                                          │
 * │  按时间                                       │
 * │  信息档位（循环）                      i      │
 * │ 编辑                                          │
 * │  撤销                                Ctrl+Z  │
 * └──────────────────────────────────────────────┘
 * ```
 *
 * ## 三条口径
 *
 * 1. **只列此刻适用的命令**（`when()` 为假的不显示）——面板是启动器，不是能力清单；
 *    「看得见但暗着」那种表达归菜单（`enabled()`）。
 * 2. **每行右侧显示当前键位**（覆盖表改了它就跟着变 —— 与分发器读的是同一份）；
 *    没绑键的命令显示 `—`（不是留白：留白会让人以为显示坏了）。
 * 3. **空查询 = 最近使用**（`lib/shortcuts.ts` 的 `recent`），按最近顺序；
 *    搜到东西就按匹配分排（`lib/command-match.ts`）。
 *
 * ## 键盘
 *
 * `↑`/`↓` 移动、`Enter` 执行、`Esc` 关（Ark 的 dialog 自带 Esc）。
 * 这些都是**面板内部**的键，不经过分发器（面板开着时分发器被 `blocked` 挡住）。
 */

import { Dialog as ArkDialog } from "@ark-ui/solid";
import { createEffect, createMemo, createSignal, For, onCleanup, Show } from "solid-js";
import { Portal } from "solid-js/web";

import { rankCommands } from "../../lib/command-match.ts";
import { chordOf, type CommandSpec } from "../../lib/commands.ts";
import { shortcutOverrides, recentCommands } from "../../lib/shortcuts.ts";
import { t } from "../../i18n/index.ts";
import type { MessageKey } from "../../i18n/index.ts";

export interface CommandPaletteProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** 全部命令（内部按 `when()` 过滤） */
  commands: readonly CommandSpec[];
  /** 执行一条命令（组装层负责：关面板 + 记最近 + 真的跑） */
  onRun: (command: CommandSpec) => void;
}

/** 一条命令 → 面板里那一行要显示的东西 */
interface PaletteRow {
  command: CommandSpec;
  title: string;
  group: string;
}

export function CommandPalette(props: CommandPaletteProps) {
  const [query, setQuery] = createSignal("");
  const [cursor, setCursor] = createSignal(0);

  /** 分组名（`cmd.group.<group>`）；认不出来就显示原 group（不装没有） */
  const groupLabel = (group: string): string => {
    const key = `cmd.group.${group}` as MessageKey;
    const text = t(key);
    return text === key ? group : text;
  };

  const rows = createMemo<PaletteRow[]>(() =>
    props.commands
      .filter((command) => command.when?.() !== false)
      .map((command) => ({
        command,
        title: t(command.titleKey as MessageKey),
        group: groupLabel(command.group),
      })),
  );

  const ranked = createMemo<PaletteRow[]>(() => {
    const ordered = rankCommands(
      query(),
      rows().map((row) => ({ id: row.command.id, title: row.title, group: row.group })),
      recentCommands(),
    );
    const byId = new Map(rows().map((row) => [row.command.id, row]));
    return ordered.flatMap((item) => {
      const row = byId.get(item.id);
      return row === undefined ? [] : [row];
    });
  });

  /** 打开时清空查询、光标回到第一条（每次打开都是干净的） */
  createEffect(() => {
    if (props.open) {
      setQuery("");
      setCursor(0);
    }
  });

  /** 结果变了把光标夹回范围（否则 ↑↓ 会停在一条已经不存在的行上） */
  createEffect(() => {
    const count = ranked().length;
    if (cursor() >= count) setCursor(count === 0 ? 0 : count - 1);
  });

  const move = (delta: -1 | 1): void => {
    const count = ranked().length;
    if (count === 0) return;
    setCursor((current) => (current + delta + count) % count);
  };

  const run = (row: PaletteRow | undefined): void => {
    if (row === undefined) return;
    if (row.command.enabled?.() === false) return;
    props.onRun(row.command);
  };

  const onInputKeyDown = (event: KeyboardEvent): void => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      move(1);
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      move(-1);
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      run(ranked()[cursor()]);
    }
  };

  return (
    <ArkDialog.Root
      open={props.open}
      onOpenChange={(details) => props.onOpenChange(details.open)}
      lazyMount
      unmountOnExit
      role="dialog"
    >
      <Portal>
        <ArkDialog.Backdrop class="fixed inset-0 bg-scrim" />
        {/* 偏上摆放（命令面板的惯例：下面留给结果展开，视线不用来回跑） */}
        <ArkDialog.Positioner class="fixed inset-0 flex items-start justify-center p-4 pt-[10vh]">
          <ArkDialog.Content
            data-command-palette="open"
            class="flex max-h-[60vh] w-full max-w-xl flex-col overflow-hidden rounded-ui bg-surface-layer outline-none"
          >
            <input
              ref={(element) => {
                // 打开即聚焦（Ark 的焦点陷阱会先给 content，这里把光标抢到输入行）
                queueMicrotask(() => element.focus());
                onCleanup(() => element.blur());
              }}
              value={query()}
              onInput={(event) => {
                setQuery(event.currentTarget.value);
                setCursor(0);
              }}
              onKeyDown={onInputKeyDown}
              placeholder={t("palette.placeholder")}
              aria-label={t("palette.placeholder")}
              class="h-10 shrink-0 border-b border-b-surface-track bg-transparent px-3 text-fs-3 text-fg-1 outline-none placeholder:text-fg-3"
            />

            <Show
              when={ranked().length > 0}
              fallback={
                <p class="px-3 py-6 text-center text-fs-2 text-fg-3">{t("palette.empty")}</p>
              }
            >
              <ul class="min-h-0 flex-1 overflow-y-auto py-1" role="listbox">
                <For each={ranked()}>
                  {(row, index) => (
                    <>
                      {/* 分组小标题：只在换了组时出现 */}
                      <Show when={index() === 0 || ranked()[index() - 1].group !== row.group}>
                        <li class="px-3 pt-2 pb-1 text-fs-0 text-fg-3" role="presentation">
                          {row.group}
                        </li>
                      </Show>
                      <li
                        data-command-item={row.command.id}
                        role="option"
                        aria-selected={index() === cursor()}
                        onMouseMove={() => setCursor(index())}
                        onClick={() => run(row)}
                        class={[
                          "flex cursor-pointer items-center gap-2 px-3 py-1.5 text-fs-2",
                          index() === cursor()
                            ? "bg-state-hover text-fg-1"
                            : "text-fg-2",
                          row.command.enabled?.() === false ? "opacity-50" : "",
                        ]
                          .filter(Boolean)
                          .join(" ")}
                      >
                        <span class="min-w-0 flex-1 truncate">{row.title}</span>
                        <span
                          data-command-chord={row.command.id}
                          class="shrink-0 text-fs-0 text-fg-3 tnum"
                        >
                          {chordOf(row.command, shortcutOverrides()) ?? "—"}
                        </span>
                      </li>
                    </>
                  )}
                </For>
              </ul>
            </Show>
          </ArkDialog.Content>
        </ArkDialog.Positioner>
      </Portal>
    </ArkDialog.Root>
  );
}
