/**
 * `easy destroy` 的**组件封装**（DESIGN.md §12.2）。
 *
 * 逻辑在 `src/lib/easy-destroy.ts`（判定 + 待确认动作），这里只做呈现：
 * 一个 `RemoveButton`（禁行图标、点击区 ≥22×22）+ 一个 `ConfirmDialog`。
 *
 * 用法：
 * ```tsx
 * <EasyDestroyButton onRemove={() => removeDir(dir)} />
 * ```
 * 普通点击 → 弹确认；**按住 `Shift` 再点 → 直接执行**（`shouldSkipConfirm` 判定，
 * 键盘触发（`Shift+Enter`）走同一条路，因为 `KeyboardEvent` 同样带 `shiftKey`）。
 *
 * 语义红线（AGENTS.md §11.3）：这是**移除**（从当前集合里拿掉），不是删磁盘文件。
 * 所以：禁行图标、文案不含「删除」、确认按钮用主色而不是红色。
 */

import { Show, splitProps, type JSX } from "solid-js";
import { t } from "../../i18n";
import { createEasyDestroy } from "../../lib/easy-destroy.ts";
import { ConfirmDialog } from "./Dialog.tsx";
import { RemoveButton } from "./RemoveButton.tsx";

export interface EasyDestroyButtonProps {
  /** 确认后要做的移除动作 */
  onRemove: () => void | Promise<void>;
  /** 按钮的无障碍名（如「从已选目录里移除 D:\\Photos」）。默认「移除」 */
  label?: string;
  /** 确认框正文。默认「确定要移除吗？」 */
  confirmMessage?: string;
  confirmTitle?: string;
  /** 图标大小（省略时走密度令牌） */
  size?: number | string;
  disabled?: boolean;
  class?: string;
}

export function EasyDestroyButton(props: EasyDestroyButtonProps) {
  const [local] = splitProps(props, [
    "onRemove",
    "label",
    "confirmMessage",
    "confirmTitle",
    "size",
    "disabled",
    "class",
  ]);

  const destroy = createEasyDestroy();

  const skipHint = () => t("common.easy_destroy.shift_hint");

  return (
    <>
      <RemoveButton
        label={local.label ?? t("common.remove")}
        size={local.size}
        disabled={local.disabled}
        class={local.class}
        onClick={(event) => {
          destroy.request(
            local.confirmMessage ?? t("common.easy_destroy.confirm"),
            () => local.onRemove(),
            event,
            local.confirmTitle,
          );
        }}
      />
      <ConfirmDialog
        open={destroy.pending() !== null}
        title={destroy.pending()?.title}
        message={destroy.pending()?.message ?? ""}
        onConfirm={destroy.confirm}
        onCancel={destroy.cancel}
      />
      {/*
        把「Shift 可跳过确认」写在弹窗里 —— 这条捷径只有被看见才会被用。
        这里用 `Show` 而不是把它塞进 ConfirmDialog 的 description，
        是为了不把范式细节耦合进那个通用弹窗。
      */}
      <Show when={destroy.pending() !== null}>
        <span class="sr-only">{skipHint()}</span>
      </Show>
    </>
  );
}

/** 需要自定义外观时的裸范式钩子（如 tile 上的排除按钮） */
export type { EasyDestroyController } from "../../lib/easy-destroy.ts";

export interface EasyDestroyHostProps {
  open: boolean;
  message: string;
  title?: string;
  onConfirm: () => void;
  onCancel: () => void;
  description?: JSX.Element;
}

/** 由调用方自己持有控制器时用的确认框外壳 */
export function EasyDestroyHost(props: EasyDestroyHostProps) {
  return (
    <ConfirmDialog
      open={props.open}
      title={props.title}
      message={props.message}
      onConfirm={props.onConfirm}
      onCancel={props.onCancel}
    />
  );
}
