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
 *   3. **缓慢的动态**（仅载入态）：一道光在**字形与图标本身**上流过，见 `styles/motion.css`
      的「状态水印」一节（那一类动画的时长口径与反馈类不同，别混）。
 *
 * ⚠️ **载入态的流光会把内容画两遍**（底下常态一遍、上面亮一档一遍，用移动的 `mask` 裁成光带）：
 * 所以 `icon` 要传**新造的 JSX**（内联写法 `icon={<IconPhoto size={64} />}`），
 * 不要传一个存起来的元素对象 —— 那样两遍会抢同一个 DOM 节点。
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
 * 1. **延时出现**（`rb-watermark-reveal`，默认 120ms）：载入快的时候（命中缓存/小目录）
 *    这一段还没画出来就结束了 —— 否则每次打开旧目录都会**闪一帧**水印，
 *    看起来像界面在抖。
 *    读**大库**这种「可能真慢」的场景可以把 `delayMs` 调到 1500：1.5 秒内出图就
 *    完全不显示（人类 2026-09-23 定的保底口径）。
 * 2. **动效只包住图标与文字**：`action`（重试）不参与流光 ——
 *    它是可点的东西，浓度必须稳定在能读、能瞄的状态。
 */

import { Show, splitProps, type JSX } from "solid-js";

export interface StateWatermarkProps {
  /** 大图标。调用方给（约定 64px / `stroke-width={1}`），组件只负责摆位 */
  icon: JSX.Element;
  /** 一句话（**已经是译好的文案**，本组件不碰 i18n —— 它属于基础元素层） */
  text: string;
  /** 载入态：延时出现 + 流光扫过。空态 / 提示态不传 */
  animate?: boolean;
  /** 延时出现的毫秒数（只对 `animate` 有意义）。默认 120ms；读大库可给 1500 */
  delayMs?: number;
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
    "delayMs",
    "tone",
    "action",
    "class",
  ]);

  const animate = (): boolean => Boolean(local.animate);
  const toneClass = (): string =>
    local.tone === "error" ? "text-danger" : "text-fg-3";

  /**
   * 水印内容（图标 + 一句话）。
   *
   * `highlight` = true 时是**流光层**那份：颜色亮一档，靠外层 `mask` 只露出光带那一段。
   * 两遍都由这一个函数造（每次访问 `local.icon` 都是**新的一遍**，见文件头的 ⚠️）。
   */
  const content = (highlight: boolean): JSX.Element => (
    <>
      <span
        class={[
          "flex items-center justify-center",
          highlight ? "text-fg-1" : toneClass(),
        ].join(" ")}
        aria-hidden="true"
      >
        {local.icon}
      </span>
      <p
        class={[
          "max-w-96 text-fs-2 break-words",
          highlight
            ? "text-fg-1"
            : local.tone === "error"
              ? "text-danger"
              : "text-fg-2",
        ].join(" ")}
      >
        {local.text}
      </p>
    </>
  );

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
        // 延时出现：`animation-delay` 内联覆盖掉样式表里那个 120ms（大库用 1500）
        style={
          animate() && local.delayMs !== undefined
            ? { "animation-delay": `${local.delayMs}ms` }
            : undefined
        }
      >
        {/*
          图标 + 文字包在同一个**网格单元**里：底层与流光层是同一个单元格的两层，
          几何天然逐像素一致（人类 2026-09-23 报「流光轮廓比字高几个像素」——
          旧实现底层走普通流（受父级 px-8 py-1 内缩），流光层却是 `absolute inset-0`
          （不含 padding）→ 高光整体上移 4px）。
          用网格堆叠而不是「给两层写同一份 padding」：后者靠两处保持一致，
          下次改一处又会错位；堆叠靠的是同一个格子，错不了。
          `overflow-hidden` 保证移动的遮罩不会溢出到网格上。
        */}
        <div class="relative grid overflow-hidden px-8 py-1">
          {/* 底层：常态浓度（不含任何动画） */}
          <div
            data-watermark-base="on"
            class="col-start-1 row-start-1 flex flex-col items-center gap-3"
          >
            {content(false)}
          </div>

          {/*
            流光层（人类 2026-09-23 定）：同一内容再画一遍、颜色亮一档，
            用移动的 `mask` 裁成一道扫过的光带 —— 变亮的是**字与图案自己的笔画**，
            而不是盖在它们上面的一块渐变方块（旧做法被否掉的原因）。
          */}
          <Show when={animate()}>
            <div
              aria-hidden="true"
              data-watermark-shimmer="on"
              class="rb-shimmer-mask pointer-events-none col-start-1 row-start-1 flex flex-col items-center gap-3"
            >
              {content(true)}
            </div>
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
