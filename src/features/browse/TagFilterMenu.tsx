/**
 * 标签**筛选**菜单（崔总 2026-10-11：导出需要标签筛选）。
 *
 * 三条边界：
 *
 * 1. **只筛不设置** —— 不像浏览工具条的标签按钮那样打开 `TagDialog`，点这里的标签只改筛选条件；
 * 2. **不识别** —— 没有 AI 识别标签的入口（那是浏览侧的事）；
 * 3. **组内或**：多个标签条件是「命中任一」（与 `filter.tags` 的既有语义一致，`store/query.rs`）。
 *
 * 菜单点一条**不关**（`closeOnSelect={false}`）：筛标签天然是多选动作，反复开关菜单很烦。
 * 标签词典来自 `browseStore.tags()`（挂载时拉一次，失败退回 `#id` 显示）；当前有标签条件时
 * 触发器保持「点击后」的主色底，收起菜单也能看出它在生效。
 */

import { onMount, type JSX } from "solid-js";
import { IconTag } from "@tabler/icons-solidjs";

import { Menu } from "../../components/ui/Menu.tsx";
import { ButtonTip } from "../../components/ui/Tooltip.tsx";
import { t } from "../../i18n/index.ts";
import type { BrowseStore } from "./store.ts";

export function TagFilterMenu(props: { store: BrowseStore }): JSX.Element {
  onMount(() => void props.store.loadTags());

  const ids = (): readonly number[] => props.store.filter().tags ?? [];
  const label = (): string => t("browse.filterByTag");

  return (
    <Menu
      label={label()}
      closeOnSelect={false}
      items={props.store.tags().map((tag) => ({
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
          <ButtonTip content={label()} placement="bottom">
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
