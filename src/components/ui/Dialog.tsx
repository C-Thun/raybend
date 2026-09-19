/**
 * Dialog / ConfirmDialog —— 模态弹窗（DESIGN.md §10.1 #9）。
 *
 * 用在建库、`easy destroy` 确认这类地方。
 *
 * 状态：
 *   关       不渲染任何东西（含遮罩）
 *   开       遮罩 `bg-scrim`（由令牌派生，两主题都是**压暗**）+ 浮层底 `surface-layer`
 *   标题     13px semibold，正文 12px 次级色
 *   动作区   右下角：取消（secondary）+ 确定（primary）
 *
 * 风格纪律：**不做阴影、不做毛玻璃**（§6）。所以「浮起来」只靠两层：
 * 遮罩压暗后面的内容 + 弹窗自己用最亮的浮层面。这也是为什么弹窗不需要描边。
 *
 * 无障碍（焦点陷阱、Esc 关闭、外点关闭、`aria-modal`、标题关联）交给 Ark。
 * 位置器**自己套 `Portal`**（原因同 `Tooltip`）。
 */

import { Dialog as ArkDialog } from "@ark-ui/solid";
import { IconX } from "@tabler/icons-solidjs";
import { Show, splitProps, type JSX } from "solid-js";
import { Portal } from "solid-js/web";
import { t } from "../../i18n";
import { blurActive } from "../../lib/dom-focus.ts";
import { Button, IconButton } from "./Button.tsx";

export interface DialogProps {
  open: boolean;
  onOpenChange?: (open: boolean) => void;
  title?: string;
  description?: string;
  /** 弹窗主体 */
  children?: JSX.Element;
  /** 右下角动作区 */
  footer?: JSX.Element;
  /** 右上角那个关闭按钮的无障碍名，默认「关闭」 */
  closeLabel?: string;
  /**
   * 遮罩要不要压暗背景。默认要（`bg-scrim`）。
   *
   * **二级确认弹窗传 `false`**（人类 2026-09-19）：一级弹窗已经压暗过一次，
   * 二级再压一次就是「半透叠半透」—— 背景越叠越黑，看起来像界面坏了。
   * 透明遮罩仍然拦住点击（该有的模态语义一个不少）。
   */
  scrim?: boolean;
  class?: string;
}

export function Dialog(props: DialogProps) {
  const [local] = splitProps(props, [
    "open",
    "onOpenChange",
    "title",
    "description",
    "children",
    "footer",
    "closeLabel",
    "scrim",
    "class",
  ]);

  return (
    <ArkDialog.Root
      open={local.open}
      onOpenChange={(details) => local.onOpenChange?.(details.open)}
      // 点遮罩（外点）与按 Esc 都会关窗：**关之前先把焦点摘掉**。
      // 否则焦点一直留在触发按钮上，WebView2 会把它当键盘焦点画一圈（`lib/dom-focus.ts` 有完整根因）。
      // 顺序不能反 —— 反了会看着圈闪一下。
      onInteractOutside={() => blurActive()}
      onEscapeKeyDown={() => blurActive()}
      lazyMount
      unmountOnExit
      role="dialog"
    >
      <Portal>
        <ArkDialog.Backdrop
          class={["fixed inset-0", local.scrim === false ? "bg-transparent" : "bg-scrim"].join(" ")}
        />
        <ArkDialog.Positioner class="fixed inset-0 flex items-center justify-center p-4">
          <ArkDialog.Content
            class={[
              /*
               * 骨架照设计稿（main.pen `Dialog / 新建库`）：**padding 16 / gap 12**，
               * 且内边距**不吃密度档**（`--dialog-pad`）—— 紧凑档缩到 6px 会「贴边」。
               */
              "flex w-full min-w-72 max-w-md flex-col gap-3 rounded-ui bg-surface-layer p-(--dialog-pad) outline-none",
              local.class ?? "",
            ].join(" ")}
          >
            {/*
              标题与右上角的叉**要对齐**：给标题一个 24px 的行盒（`leading-6`），
              和叉的图标按钮同高，`items-start` 下两者的中心就重合了。
              （2026-09-16 人类反馈：之前标题比叉低一截。标题字号也照设计稿
              从 13px 提到 **17px semibold** —— 设计稿里的弹窗标题就是 17。）
            */}
            <div class="flex items-start gap-2">
              <div class="min-w-0 flex-1">
                <Show when={local.title}>
                  <ArkDialog.Title class="text-[17px] leading-6 font-semibold text-fg-1">
                    {local.title}
                  </ArkDialog.Title>
                </Show>
                <Show when={local.description}>
                  <ArkDialog.Description class="mt-1 text-[13px] leading-normal text-fg-2">
                    {local.description}
                  </ArkDialog.Description>
                </Show>
              </div>
              <ArkDialog.CloseTrigger asChild={(triggerProps) => (
                <IconButton
                  {...triggerProps()}
                  label={local.closeLabel ?? t("common.close")}
                >
                  <IconX size={14} aria-hidden="true" />
                </IconButton>
              )} />
            </div>

            <Show when={local.children}>
              <div class="min-w-0 text-[13px] leading-normal text-fg-1">
                {local.children}
              </div>
            </Show>

            <Show when={local.footer}>
              {/*
                底部按钮按设计稿高 **32px**（`Dialog Footer` 里的 `Button` 就是 32 高）。
                用后代选择器统一抬高，而不是让每个调用点自己写 `size` ——
                弹窗底部的按钮理应一律同高，散在各处迟早会不一致。
              */}
              <div class="flex items-center justify-end gap-2 [&>button]:h-8 [&>button]:px-4 [&>button]:leading-none">
                {local.footer}
              </div>
            </Show>
          </ArkDialog.Content>
        </ArkDialog.Positioner>
      </Portal>
    </ArkDialog.Root>
  );
}

export interface ConfirmDialogProps {
  open: boolean;
  /** 已经是翻译后的文案 */
  message: string;
  title?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  /** 遮罩是否压暗（二级确认传 `false`；见 `DialogProps.scrim`） */
  scrim?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

/**
 * 确认框 —— `easy destroy`（§12.2）的呈现层。
 *
 * 注意「确定」用的是 **primary（主色）** 而不是危险红：
 * 移除/排除**不是破坏性删除**（AGENTS.md §11.3），
 * 而且在 `danger` 令牌定案前（`design/main.md` §7 待决项）也不该有红色按钮。
 */
export function ConfirmDialog(props: ConfirmDialogProps) {
  return (
    <Dialog
      open={props.open}
      onOpenChange={(open) => {
        if (!open) props.onCancel();
      }}
      title={props.title}
      /*
       * 「二级确认」用**透明遮罩**（人类 2026-09-19）：一级窗口已经在压暗背景了，
       * 这里再叠一层半透就是越叠越黑。给了 `scrim: false` 就交给调用方决定。
       */
      {...(props.scrim === undefined ? {} : { scrim: props.scrim })}
      footer={
        <>
          <Button variant="secondary" onClick={props.onCancel}>
            {props.cancelLabel ?? t("common.cancel")}
          </Button>
          <Button variant="primary" onClick={props.onConfirm}>
            {props.confirmLabel ?? t("common.confirm")}
          </Button>
        </>
      }
    >
      {props.message}
    </Dialog>
  );
}
