/**
 * Dialog / ConfirmDialog —— 模态弹窗（memory/DESIGN.md §10.1 #9）。
 *
 * 用在建库、`easy destroy` 确认这类地方。
 *
 * 状态：
 *   关       不渲染任何东西（含遮罩）
 *   开       内容面 `surface-layer` + **0.5px `--dialog-ring` 内描边**；遮罩由弹窗栈最底层画一次
 *   标题     17px semibold，正文 13px 次级色
 *   动作区   右下角：取消（secondary）+ 确定（primary）
 *
 * 风格纪律：**不做阴影、不做毛玻璃**（§6）。「浮起来」靠两层：
 * 遮罩压暗后面的内容 + 弹窗自己用最亮的浮层面。**描边是无边线设计的唯一常驻例外** ——
 * 两层弹窗叠放时底色完全相同，没有阴影可依，缺这条线边界就会糊在一起（崔总 2026-10-24）。
 *
 * 叠放纪律（`lib/dialog-stack.ts`）：
 *   * 遮罩只由**最底层**画；二级/三级弹窗自动透明，调用方不需要也不能手写 `scrim`；
 *   * 后开的弹窗永远在上（`--dialog-layer` 按栈深抬 z）；
 *   * 下层弹窗不可点击、焦点不进入 —— 由 Ark/Zag 的 modal 行为承担（body pointer-events + 焦点陷阱栈）。
 *
 * 无障碍（焦点陷阱、Esc 关闭、外点关闭、`aria-modal`、标题关联）交给 Ark。
 * 位置器**自己套 `Portal`**（原因同 `Tooltip`）。
 */

import { Dialog as ArkDialog } from "@ark-ui/solid";
import { IconX } from "@tabler/icons-solidjs";
import { Show, splitProps, untrack, type JSX } from "solid-js";
import { Portal } from "solid-js/web";
import { t } from "../../i18n";
import { blurActive } from "../../lib/dom-focus.ts";
import { createDialogLayer } from "../../lib/dialog-stack.ts";
import { Button, IconButton } from "./Button.tsx";

/**
 * 标题栏在弹窗打开时仍要能拖、能关窗口。
 *
 * Ark 的 modal 会把 `body` 设成 `pointer-events: none`，titlebar 会被一起禁掉
 * （它又不在弹窗的 Portal 里）。`persistentElements` 是 Ark 给的正规出口：
 * 它会把这些元素重新设为 `pointer-events: auto`，并把它们从「外点关窗」的判定里排除。
 */
const PERSISTENT_ELEMENTS = [
  () => document.querySelector<HTMLElement>("[data-app-titlebar]"),
];

export interface DialogFrameProps {
  open: boolean;
  onOpenChange?: (open: boolean) => void;
  /** 打开时的初始焦点元素；确认类弹窗必须传（见 `DialogProps.initialFocusEl`） */
  initialFocusEl?: () => HTMLElement | null;
  /** 位置器额外类（命令面板用 `items-start pt-[10vh]`；默认垂直居中） */
  positionerClass?: string;
  /** 内容面额外类（尺寸、平移、overflow 等；基础面与描边由这里统一给） */
  contentClass?: string;
  /** 内容面的附加属性（命令面板的 `data-command-palette` 等） */
  contentAttrs?: Record<string, string | number | boolean>;
  children?: JSX.Element;
  role?: "dialog" | "alertdialog";
}

/**
 * `DialogFrame` —— 所有模态弹窗共用的 Ark 三件套（Root / Backdrop / Positioner / Content）。
 *
 * 它只管「层级、遮罩、边框、外点/Esc 的焦点清理」；标题、按钮、正文布局归 `Dialog` 或
 * 命令面板自己。抽出来的理由：命令面板原本复制了同一套 Ark 结构，边框与遮罩机制
 * 一旦只写在 `Dialog` 里，第二个实现就会漏。
 */
export function DialogFrame(props: DialogFrameProps) {
  const layer = createDialogLayer(() => props.open);
  /** 栈深 → `index.css` 的弹窗 z 公式；关闭时也写 1，避免读到空值 */
  const layerStyle = (): JSX.CSSProperties => ({
    "--dialog-layer": String(Math.max(1, layer.depth())),
  });

  return (
    <ArkDialog.Root
      open={props.open}
      onOpenChange={(details) => props.onOpenChange?.(details.open)}
      initialFocusEl={props.initialFocusEl}
      persistentElements={PERSISTENT_ELEMENTS}
      // 点遮罩（外点）与按 Esc 都会关窗：**关之前先把焦点摘掉**。
      // 否则焦点一直留在触发按钮上，WebView2 会把它当键盘焦点画一圈（`lib/dom-focus.ts` 有完整根因）。
      // 顺序不能反 —— 反了会看着圈闪一下。
      onInteractOutside={() => blurActive()}
      onEscapeKeyDown={() => blurActive()}
      lazyMount
      unmountOnExit
      role={props.role ?? "dialog"}
    >
      <Portal>
        <ArkDialog.Backdrop
          class={[
            "fixed inset-0",
            layer.paintsScrim() ? "bg-scrim" : "bg-transparent",
          ].join(" ")}
          style={layerStyle()}
        />
        <ArkDialog.Positioner
          class={[
            "fixed inset-0 flex justify-center p-4",
            props.positionerClass ?? "items-center",
          ].join(" ")}
          style={layerStyle()}
        >
          <ArkDialog.Content
            class={[
              "rounded-ui bg-surface-layer inset-ring-(length:--dialog-ring-w) inset-ring-(--dialog-ring) outline-none",
              props.contentClass ?? "",
            ].join(" ")}
            {...props.contentAttrs}
          >
            {props.children}
          </ArkDialog.Content>
        </ArkDialog.Positioner>
      </Portal>
    </ArkDialog.Root>
  );
}

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
  size?: "md" | "wide" | "settings";
  /**
   * 打开时的初始焦点元素。
   *
   * **确认类弹窗必须传**（崔总 2026-10-11）：不传时 Ark 把焦点给第一个可聚焦元素 ——
   * 也就是右上角的关闭叉 —— 于是**回车等于关窗**，用户以为按了确认其实取消了。
   * 口径：**回车 = 前进/确认，Esc = 关闭**。
   */
  initialFocusEl?: () => HTMLElement | null;
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
    "size",
    "initialFocusEl",
    "class",
  ]);

  return (
    <DialogFrame
      open={local.open}
      onOpenChange={local.onOpenChange}
      initialFocusEl={local.initialFocusEl}
      contentClass={[
        /*
         * 骨架照设计稿（main.pen `Dialog / 新建库`）：**padding 16 / gap 12**，
         * 且内边距**不吃密度档**（`--dialog-pad`）—— 紧凑档缩到 6px 会「贴边」。
         */
        "flex w-full min-w-72 flex-col gap-3 p-(--dialog-pad)",
        local.size === "settings" ? "h-160 max-h-dvh max-w-5xl" : local.size === "wide" ? "max-w-2xl" : "max-w-md",
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

      <Show when={untrack(() => local.children) != null}>
        <div class={local.size === "settings" ? "flex min-h-0 flex-1 text-[13px] leading-normal text-fg-1" : "min-w-0 text-[13px] leading-normal text-fg-1"}>
          {untrack(() => local.children)}
        </div>
      </Show>

      <Show when={untrack(() => local.footer) != null}>
        {/*
          底部按钮按设计稿高 **32px**（`Dialog Footer` 里的 `Button` 就是 32 高）。
          用后代选择器统一抬高，而不是让每个调用点自己写 `size` ——
          弹窗底部的按钮理应一律同高，散在各处迟早会不一致。
        */}
        <div class="flex items-center justify-end gap-2 [&>button]:h-8 [&>button]:px-4 [&>button]:leading-none">
          {untrack(() => local.footer)}
        </div>
      </Show>
    </DialogFrame>
  );
}

export interface ConfirmDialogProps {
  open: boolean;
  /** 已经是翻译后的文案 */
  message: string;
  title?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  /** 正文下方的补充内容，仅用于说明本次操作后果。 */
  description?: JSX.Element;
  size?: "md" | "wide";
  onConfirm: () => void;
  onCancel: () => void;
}

/**
 * 确认框 —— `easy destroy`（§12.2）的呈现层。
 *
 * 注意「确定」用的是 **primary（主色）** 而不是危险红：
 * 移除/排除**不是破坏性删除**（AGENTS.md §11.3），
 * 而且在 `danger` 令牌定案前（`design/main.md` §7 待决项）也不该有红色按钮。
 *
 * 二级确认的遮罩**不需要调用方声明**：`DialogFrame` 的栈会自动让它透明。
 */
export function ConfirmDialog(props: ConfirmDialogProps) {
  // 「确定」按钮的 ref：打开时焦点落它（**回车 = 确认**，Esc = 关闭）—— 见 `DialogProps.initialFocusEl`
  let confirmEl: HTMLButtonElement | undefined;
  return (
    <Dialog
      open={props.open}
      onOpenChange={(open) => {
        if (!open) props.onCancel();
      }}
      initialFocusEl={() => confirmEl ?? null}
      title={props.title}
      size={props.size}
      footer={
        <>
          <Button variant="secondary" onClick={props.onCancel}>
            {props.cancelLabel ?? t("common.cancel")}
          </Button>
          <Button variant="primary" ref={(el) => { confirmEl = el; }} onClick={props.onConfirm}>
            {props.confirmLabel ?? t("common.confirm")}
          </Button>
        </>
      }
    >
      {props.message}
      <Show when={props.description}><p class="mt-3 text-fs-1 text-fg-3">{props.description}</p></Show>
    </Dialog>
  );
}
