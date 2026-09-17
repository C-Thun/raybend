/**
 * StateWatermark —— 网格区的**状态水印**（载入 / 空 / 错误 / 提示）。
 *
 * ## 它是什么
 *
 * 照片网格没有内容可画时，整块区域浮一块**大图标 + 一句话**。
 * 关键在「印在面上」：**没有卡片、没有边框、没有底色** —— 一旦套个容器
 * （哪怕是圆角浅底），观感立刻从「印痕」变成「一个浮在中间的控件」，
 * 那正是 2026-09-17 人类明确否掉的方向（「不要简单粗暴地拿个主色图标贴上去」）。
 *
 * 所以这个组件的全部视觉手段只有三样：
 *   1. **低浓度**：图标 `--fg-3`、文字 `--fg-2`，整块再乘一道不透明度；
 *   2. **大**：图标 64px、细笔画（`stroke-width=1`）—— 像蚀刻而不是像按钮；
 *   3. **缓慢的动态**（仅载入态）：呼吸 + 一道高光扫过，见 `styles/motion.css`
 *      的「状态水印」一节（那一类动画的时长口径与反馈类不同，别混）。
 *
 * ## 四个语义（同一组件，不拆）
 *
 * | 语义 | `animate` | `tone` | 用在 |
 * | --- | --- | --- | --- |
 * | 载入中 | ✅ | muted | 扫描目录 / 读文件头缓存 |
 * | 空 | — | muted | 目录里没有照片、库里没有照片 |
 * | 错误 | — | error | 目录/库读不了（配 `action` 给「重试」） |
 * | 提示 | — | muted | 还没选目录、还没选库 |
 *
 * ## 两条实现纪律
 *
 * 1. **延时出现**（`rb-watermark-reveal`，120ms）：载入快的时候（命中缓存/小目录）
 *    这一段还没画出来就结束了 —— 否则每次打开旧目录都会**闪一帧**水印，
 *    看起来像界面在抖。
 * 2. **动效只包住图标与文字**：`action`（重试）不参与呼吸 ——
 *    它是可点的东西，浓度必须稳定在能读、能瞄的状态。
 */

import { Show, splitProps, type JSX } from "solid-js";

export interface StateWatermarkProps {
  /** 大图标。调用方给（约定 64px / `stroke-width={1}`），组件只负责摆位 */
  icon: JSX.Element;
  /** 一句话（**已经是译好的文案**，本组件不碰 i18n —— 它属于基础元素层） */
  text: string;
  /** 载入态：延时出现 + 缓慢呼吸 + 高光扫过。空态 / 提示态不传 */
  animate?: boolean;
  /** 错误态用 `--danger`；其余用次级前景色 */
  tone?: "muted" | "error";
  /** 可选动作（例：重试）。它是**唯一**吃点击的东西 */
  action?: { label: string; run: () => void };
  class?: string;
}

export function StateWatermark(props: StateWatermarkProps) {
  const [local, rest] = splitProps(props, [
    "icon",
    "text",
    "animate",
    "tone",
    "action",
    "class",
  ]);

  const animate = (): boolean => Boolean(local.animate);
  const toneClass = (): string =>
    local.tone === "error" ? "text-danger" : "text-fg-3";

  return (
    <div
      {...rest}
      class={[
        "flex min-h-0 min-w-0 flex-1 items-center justify-center p-panel-pad",
        local.class ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
      // 载入态让读屏知道「这里在忙」；空态/提示态不需要播报（静态文案自己会被读到）
      role={animate() ? "status" : undefined}
      aria-live={animate() ? "polite" : undefined}
    >
      <div
        class={[
          "flex flex-col items-center gap-3 text-center",
          animate() ? "rb-watermark-reveal" : "opacity-90",
        ].join(" ")}
      >
        {/*
          高光要被水印块裁住（`overflow-hidden`），所以图标 + 文字包在一个 relative 里；
          `rb-watermark-breathe` 也挂在它上面 —— 呼吸只作用于印痕本身，不作用于下面的按钮。
        */}
        <div
          class={[
            "relative flex flex-col items-center gap-3 overflow-hidden px-8 py-1",
            animate() ? "rb-watermark-breathe" : "",
          ]
            .filter(Boolean)
            .join(" ")}
        >
          <span class={["flex items-center justify-center", toneClass()].join(" ")} aria-hidden="true">
            {local.icon}
          </span>
          <p
            class={[
              "max-w-96 text-fs-2 break-words",
              local.tone === "error" ? "text-danger" : "text-fg-2",
            ].join(" ")}
          >
            {local.text}
          </p>

          <Show when={animate()}>
            <span
              aria-hidden="true"
              class="rb-watermark-sheen pointer-events-none absolute inset-y-0 start-0 w-1/2"
            />
          </Show>
        </div>

        <Show when={local.action}>
          {(action) => (
            <button
              type="button"
              class="cursor-pointer text-fs-1 text-fg-2 underline hover:text-fg-1"
              onClick={() => action().run()}
            >
              {action().label}
            </button>
          )}
        </Show>
      </div>
    </div>
  );
}
