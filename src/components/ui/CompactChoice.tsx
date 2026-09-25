/** 紧凑单选：直方图通道、曲线通道及面板内二选一共用的按钮形态。 */
import { For, type JSX } from "solid-js";

export interface CompactChoiceOption<T extends string> {
  value: T;
  label: string;
  ariaLabel?: string;
  disabled?: boolean;
  dirty?: boolean;
}

export interface CompactChoiceProps<T extends string> {
  value: T | null;
  options: readonly CompactChoiceOption<T>[];
  label: string;
  onValueChange: (value: T) => void;
  /** 直方图再次点击选中通道时恢复全通道显示。 */
  onClear?: () => void;
  class?: string;
  optionClass?: string;
  dataName?: string;
}

export function CompactChoice<T extends string>(props: CompactChoiceProps<T>): JSX.Element {
  return (
    <div class={["flex items-center gap-1", props.class ?? ""].filter(Boolean).join(" ")} role="group" aria-label={props.label}>
      <For each={props.options}>
        {(option) => (
          <button
            type="button"
            aria-pressed={props.value === option.value}
            aria-label={option.ariaLabel ?? option.label}
            title={option.ariaLabel ?? option.label}
            disabled={option.disabled}
            data-compact-choice={option.value}
            data-curve-channel={props.dataName === "curve" ? option.value : undefined}
            data-curve-dirty={props.dataName === "curve" ? (option.dirty ? "yes" : "no") : undefined}
            onClick={() => props.value === option.value && props.onClear ? props.onClear() : props.onValueChange(option.value)}
            class={[
              "flex h-5 min-w-6 items-center justify-center rounded-ui px-1 text-fs-0 font-semibold transition-colors",
              props.value === option.value ? "bg-state-selected text-fg-1" : "text-fg-3 hover:bg-state-hover hover:text-fg-2",
              option.dirty ? "bg-state-hover text-fg-1" : "",
              props.optionClass ?? "",
            ].filter(Boolean).join(" ")}
          >
            {option.label}
          </button>
        )}
      </For>
    </div>
  );
}
