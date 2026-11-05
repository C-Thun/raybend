/**
 * 标签**筛选**菜单（崔总 2026-10-11：导出需要标签筛选；2026-10-25 加搜索）。
 *
 * 三条边界：
 *
 * 1. **只筛不设置** —— 不像浏览工具条的标签按钮那样打开 `TagDialog`，点这里的标签只改筛选条件；
 * 2. **不识别** —— 没有 AI 识别标签的入口（那是浏览侧的事）；
 * 3. **组内或**：多个标签条件是「命中任一」（与 `filter.tags` 的既有语义一致，`store/query.rs`）。
 *
 * 标签词典是全局的，系统硬上限 5000（`store::tags::MAX_TAGS`）。菜单**不**平铺
 * 5000 条：默认给常用的一批 + 已选中的，输入时在词典里搜（`TagDialog` 的输入即搜
 * 是服务端 `tag_list`；这里量级只有 5000，客户端过滤即可，免去防抖与 IPC）。
 *
 * 菜单点一条**不关**（`closeOnSelect={false}`）：筛标签天然是多选动作，反复开关菜单很烦。
 */

import { createMemo, createSignal, onMount, type JSX } from "solid-js";
import { IconSearch, IconTag } from "@tabler/icons-solidjs";

import { Menu } from "../../components/ui/Menu.tsx";
import { ButtonTip } from "../../components/ui/Tooltip.tsx";
import { t } from "../../i18n/index.ts";
import type { BrowseStore } from "./store.ts";
import { filterTagMenu } from "./tag-filter.ts";

export function TagFilterMenu(props: { store: BrowseStore }): JSX.Element {
  onMount(() => void props.store.loadTags());

  const [query, setQuery] = createSignal("");
  const ids = (): readonly number[] => props.store.filter().tags ?? [];
  const label = (): string => t("browse.filterByTag");
  const allTags = () => props.store.tags();

  const items = createMemo(() => filterTagMenu(allTags(), ids(), query()));

  const header = (
    <div class="flex flex-col gap-1">
      <div class="flex h-7 items-center gap-1.5 rounded-ui bg-surface-bar px-2">
        <IconSearch size={12} class="shrink-0 text-fg-3" aria-hidden="true" />
        <input
          class="min-w-0 flex-1 bg-transparent text-fs-2 text-fg-1 outline-none placeholder:text-fg-3"
          placeholder={t("tags.searchPlaceholder")}
          aria-label={t("tags.searchPlaceholder")}
          value={query()}
          onInput={(event) => setQuery(event.currentTarget.value)}
          /*
           * 方向键留给 Ark 在菜单项之间移动；其余按键（输入、回车、空格）不往上冒 ——
           * 否则回车会触发菜单的「选中高亮项」，在搜索框里按回车就误选了标签。
           */
          onKeyDown={(event) => {
            if (event.key !== "ArrowDown" && event.key !== "ArrowUp") event.stopPropagation();
          }}
        />
      </div>
      {items().length === 0 ? (
        <p class="px-1 text-fs-0 text-fg-3">{t("browse.tagsNoMatch")}</p>
      ) : (
        query().trim() === "" &&
        allTags().length > items().length && (
          <p class="px-1 text-fs-0 text-fg-3">
            {t("tags.searchHint").replace("{n}", String(allTags().length))}
          </p>
        )
      )}
    </div>
  );

  return (
    <Menu
      label={label()}
      closeOnSelect={false}
      header={header}
      items={items().map((tag) => ({
        value: String(tag.id),
        label: tag.name,
        selected: ids().includes(tag.id),
      }))}
      onSelect={(value) => {
        const id = Number(value);
        if (!Number.isSafeInteger(id)) return;
        const current = ids();
        props.store.patchFilter({
          tags: current.includes(id)
            ? current.filter((one) => one !== id)
            : [...current, id],
        });
        setQuery("");
      }}
    >
      {(trigger) => (
        <button
          {...trigger}
          type="button"
          aria-label={label()}
          class={[
            "flex size-7 items-center justify-center rounded-ui",
            ids().length > 0
              ? "bg-state-selected text-fg-1"
              : "text-fg-2 hover:bg-state-hover hover:text-fg-1",
          ].join(" ")}
        >
          {/*
            提示挂在内层的图标上：菜单触发器已经把 `ref`/键盘事件占在按钮上，
            再叠一层 Tooltip 的触发属性会互相踩；内层铺满按钮、命中不丢。
          */}
          <ButtonTip content={label()}>
            {(tipProps) => (
              <span {...tipProps} class="flex size-full items-center justify-center">
                <IconTag size={16} />
              </span>
            )}
          </ButtonTip>
        </button>
      )}
    </Menu>
  );
}
