/**
 * 表单原语（DESIGN.md §10.1 #3、#4、#6 + Switch）。
 *
 * 交互与无障碍逻辑交给 Ark UI（焦点管理、键盘、aria 这些自己写容易漏），
 * 样式按本项目的令牌与反馈规则改写。
 *
 * 状态与反馈规则（DESIGN.md §5）：指向=辅色底，选中/开启=主色底。
 */

import { Checkbox as ArkCheckbox, Switch as ArkSwitch } from "@ark-ui/solid";
import type { JSX } from "solid-js";
import { Show, splitProps } from "solid-js";

/* ══════════════════════════════════════════════════════════════
 * Checkbox —— 方形勾选（用于「包含子目录」等属性）
 * ══════════════════════════════════════════════════════════════ */

export interface CheckboxProps {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  label?: string;
  disabled?: boolean;
  class?: string;
}

export function Checkbox(props: CheckboxProps) {
  return (
    <ArkCheckbox.Root
      checked={props.checked}
      onCheckedChange={(d) => props.onCheckedChange(Boolean(d.checked))}
      disabled={props.disabled}
      class={["inline-flex items-center gap-1.5", props.class ?? ""].join(" ")}
    >
      <ArkCheckbox.Control
        class={[
          "flex size-checkbox-size shrink-0 items-center justify-center rounded-[2px] transition-colors",
          "border border-fg-3",
          "data-[state=checked]:border-brand data-[state=checked]:bg-brand",
          "data-[disabled]:opacity-60",
        ].join(" ")}
      >
        <ArkCheckbox.Indicator class="text-fg-on-brand">
          <svg viewBox="0 0 12 12" class="size-2.5" aria-hidden="true">
            <path
              d="M2.5 6.2 4.8 8.5 9.5 3.8"
              fill="none"
              stroke="currentColor"
              stroke-width="1.8"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
        </ArkCheckbox.Indicator>
      </ArkCheckbox.Control>
      <Show when={props.label}>
        <ArkCheckbox.Label class="cursor-pointer text-[11px] text-fg-2">
          {props.label}
        </ArkCheckbox.Label>
      </Show>
      <ArkCheckbox.HiddenInput />
    </ArkCheckbox.Root>
  );
}

/* ══════════════════════════════════════════════════════════════
 * RadioCircle —— 圆形主色勾选框（DESIGN.md §12.4）
 *
 * 表达「已加入待处理集合」（勾选），**可多选**，
 * 与表达「当前正在浏览」的整行主色底（选中）是两套独立状态。
 * ══════════════════════════════════════════════════════════════ */

export interface RadioCircleProps {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  /** 无障碍名。纯圆圈没有可读文本，必填。 */
  label: string;
  disabled?: boolean;
  class?: string;
}

export function RadioCircle(props: RadioCircleProps) {
  const [local] = splitProps(props, ["checked", "label"]);

  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={local.checked}
      aria-label={local.label}
      title={local.label}
      disabled={props.disabled}
      onClick={(e) => {
        e.stopPropagation(); // 勾选不应同时触发「选中该行」
        props.onCheckedChange(!props.checked);
      }}
      class={[
        "flex size-checkbox-size shrink-0 items-center justify-center rounded-full transition-colors",
        local.checked
          ? "bg-brand text-fg-on-brand hover:bg-brand"
          : "border-[1.5px] border-fg-3 hover:bg-state-hover",
        props.disabled ? "pointer-events-none opacity-60" : "cursor-pointer",
        props.class ?? "",
      ].join(" ")}
    >
      <Show when={local.checked}>
        <svg viewBox="0 0 12 12" class="size-2.5" aria-hidden="true">
          <path
            d="M2.5 6.2 4.8 8.5 9.5 3.8"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
      </Show>
    </button>
  );
}

/* ══════════════════════════════════════════════════════════════
 * Switch —— 属性开关（如「包含子目录」「避免重复导入」）
 *
 * 注意与 `ToggleBlock`（按下式按钮，见 DESIGN.md §12.8）区分：
 *   Switch      = 左右滑动的圆点开关，表达「某个属性开/关」
 *   ToggleBlock = 方块按下，表达「切换到某个模式」
 * ══════════════════════════════════════════════════════════════ */

export interface SwitchProps {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  label: string;
  disabled?: boolean;
  class?: string;
}

export function Switch(props: SwitchProps) {
  return (
    <ArkSwitch.Root
      checked={props.checked}
      onCheckedChange={(d) => props.onCheckedChange(Boolean(d.checked))}
      disabled={props.disabled}
      aria-label={props.label}
      class={["inline-flex items-center", props.class ?? ""].join(" ")}
    >
      <ArkSwitch.Control
        class={[
          "flex shrink-0 cursor-pointer items-center rounded-full p-[2px] transition-colors",
          "h-switch-h w-switch-w",
          "bg-surface-track data-[state=checked]:bg-brand",
          "data-[disabled]:cursor-default data-[disabled]:opacity-60",
        ].join(" ")}
      >
        <ArkSwitch.Thumb
          class={[
            "rounded-full bg-fg-3 transition-transform",
            "size-switch-knob",
            "data-[state=checked]:translate-x-[calc(var(--switch-w)-var(--switch-knob)-4px)]",
            "data-[state=checked]:bg-fg-on-brand",
          ].join(" ")}
        />
      </ArkSwitch.Control>
      <ArkSwitch.HiddenInput />
    </ArkSwitch.Root>
  );
}

/* ══════════════════════════════════════════════════════════════
 * Input —— 文本框。**唯一保留描边的控件之一**（DESIGN.md §6）
 * ══════════════════════════════════════════════════════════════ */

export interface InputProps extends JSX.InputHTMLAttributes<HTMLInputElement> {
  invalid?: boolean;
}

export function Input(props: InputProps) {
  const [local, rest] = splitProps(props, ["invalid", "class"]);

  return (
    <input
      {...rest}
      class={[
        "h-6 rounded-ui bg-surface-track px-2 text-[12px] text-fg-1 transition-colors",
        "border border-transparent outline-none",
        "placeholder:text-fg-3",
        "hover:border-fg-3 focus:border-brand",
        "disabled:pointer-events-none disabled:opacity-60",
        local.invalid ? "border-brand-2" : "",
        local.class ?? "",
      ].join(" ")}
    />
  );
}
