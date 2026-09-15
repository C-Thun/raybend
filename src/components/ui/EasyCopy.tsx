/**
 * `easy copy` —— 悬停即知可复制（DESIGN.md §12.1）。
 *
 * 范式（用户指定的四个阶段）：
 *   1. 悬停某个组 → 该组加一圈**淡淡的细边框**（固定品牌主色、约 40% 透明度），
 *      同时内容上方**紧挨着**弹出 `点击复制` 小窗（窄 padding）
 *   2. 弹出动画：由下向上（`translateY` + 淡入）
 *   3. 点击 → 复制**整组**内容；`点击复制` 小窗**再向上弹**，紧接着下方**又弹出一个** `已复制`
 *   4. 离开 → 边框与提示淡出
 *
 * 分组是**内容本身的结构**，不是鼠标划过的顺序：调用方把「相机型号 + 镜头」包成一个
 * `EasyCopy`，把「焦距/光圈/快门/ISO」包成另一个，同组内容就一起亮、一起复制。
 *
 * 两条实现纪律：
 *   - 边框用 `outline` 而不是 `border`：outline 不参与布局，出现时**不会把内容挤动 1px**
 *   - 边框用 `outline-*` 而非 `ring-*`：DESIGN.md §6 不做阴影，ring 在 Tailwind v4 里
 *     是 box-shadow 实现，语义上就违反纪律了
 *
 * 复制失败时**什么都不显示**（不翻转成「已复制」）—— 假成功比不反馈更糟。
 */

import { createSignal, onCleanup, Show, splitProps, type JSX } from "solid-js";
import { t } from "../../i18n";
import { copyText } from "../../lib/clipboard.ts";
import { Tooltip } from "./Tooltip.tsx";

export interface EasyCopyProps {
  /** 复制到剪贴板的内容（整组，一般用 `\n` 或 ` · ` 连接） */
  value: string;
  /** 组内容 */
  children: JSX.Element;
  /** 关掉（如内容还没加载出来） */
  disabled?: boolean;
  /** 气泡文案覆盖（默认走 i18n） */
  hintLabel?: string;
  doneLabel?: string;
  class?: string;
}

/** 「已复制」提示停留多久（ms） */
const COPIED_LINGER_MS = 1500;

export function EasyCopy(props: EasyCopyProps) {
  const [local] = splitProps(props, [
    "value",
    "children",
    "disabled",
    "hintLabel",
    "doneLabel",
    "class",
  ]);

  const [hovered, setHovered] = createSignal(false);
  const [copied, setCopied] = createSignal(false);
  /**
   * 弹出动画的重放计数。
   * 初值 1 而不是 0：`Show when` 需要真值，否则第一次悬停时提示是空的。
   */
  const [popKey, setPopKey] = createSignal(1);

  let lingerTimer: ReturnType<typeof setTimeout> | undefined;
  const stopLinger = () => {
    if (lingerTimer !== undefined) clearTimeout(lingerTimer);
    lingerTimer = undefined;
  };
  onCleanup(stopLinger);

  const open = () => hovered() || copied();

  const handleClick = async () => {
    const ok = await copyText(local.value);
    if (!ok) return; // 不显示「已复制」——不能骗用户
    setCopied(true);
    setPopKey((n) => n + 1);
    stopLinger();
    lingerTimer = setTimeout(() => {
      setCopied(false);
      lingerTimer = undefined;
    }, COPIED_LINGER_MS);
  };

  return (
    <Tooltip
      open={open()}
      onOpenChange={(next) => {
        setHovered(next);
        // 每次重新弹出都重放一次「由下向上」的动画
        if (next) setPopKey((n) => n + 1);
      }}
      placement="top"
      openDelay={120}
      disabled={local.disabled}
      content={
        <span class="flex flex-col items-center gap-0.5">
          <Show when={popKey()} keyed>
            <span class="rb-pop-up">
              {local.hintLabel ?? t("common.easy_copy.hint")}
            </span>
          </Show>
          <Show when={copied()} keyed>
            <span class="rb-fade-in font-medium">
              {local.doneLabel ?? t("common.easy_copy.done")}
            </span>
          </Show>
        </span>
      }
    >
      {(triggerProps) => (
        <span
          {...triggerProps()}
          onClick={handleClick}
          class={[
            "inline-flex min-w-0 cursor-pointer items-center gap-1.5 rounded-ui px-1 py-0.5",
            "transition-colors",
            // §12.1：固定品牌主色、约 40% 透明度（令牌里就是 color-mix 出来的）
            open()
              ? "-outline-offset-1 outline-1 outline-ring-easy-copy"
              : "outline-1 outline-transparent -outline-offset-1",
            local.disabled ? "pointer-events-none opacity-60" : "",
            local.class ?? "",
          ]
            .filter(Boolean)
            .join(" ")}
        >
          {local.children}
        </span>
      )}
    </Tooltip>
  );
}
