/**
 * Tile —— 网格基本单元（DESIGN.md §10.1 #11）。
 *
 * 是**图标 tile 与图片 tile 的公共骨架**：上方一个定尺寸的画面区，下方一条字幕。
 * 它只负责「一个格子长什么样」，不含虚拟化、不含批量选择逻辑（那些在 `PhotoGrid` 里）。
 *
 * 尺寸来自 `--tile-cell-w` / `--tile-cell-h` / `--caption-h` ——
 * 这三个是**运行时会被 JS 改写的变量**（缩放滑块决定 tile 宽，见 §12.6 的换行数学），
 * 所以这里不写死像素。
 *
 * 状态：
 *   默认     画面区 = 中间调占位面，字幕 = 次级色
 *   指向     **辅色底**叠加在整块上（§5：指向 = 辅色底）
 *   选中     **主色低透明度底**（大面积用低浓度，§5.1），字幕提到 fg-1
 *   空态     画面区居中显示图标（该格没有可显示的图）
 *   加载中   骨架呼吸 + 不显示字幕内容（尺寸不跳 —— 画面区高宽由令牌固定）
 *   禁用     前景降次级，不响应指针
 *
 * 键盘：`Space` = 选中（同点击），`Enter` = 打开（同双击）。
 * 这与列表的通用约定一致 —— `Enter` 是「进去看」，不是「选中」。
 */

import type { JSX } from "solid-js";
import { Show, splitProps } from "solid-js";

export interface TileProps {
  /** 文件名（字幕主行） */
  label: string;
  /** 字幕副行（如文件类型、尺寸） */
  sublabel?: string;
  /** 图片地址（object URL / data URL / asset URL） */
  src?: string;
  /** 无图时的占位图标（如「文件夹」用文件夹图标） */
  icon?: JSX.Element;
  selected?: boolean;
  disabled?: boolean;
  /** 缩略图还没就绪 */
  loading?: boolean;
  /** 该格确实没有内容（与 loading 不同：一个在等，一个就是空的） */
  empty?: boolean;
  /** 浮在右上角的动作区（悬停才由调用方显示，Tile 只留位置） */
  actions?: JSX.Element;
  onClick?: JSX.EventHandlerUnion<HTMLDivElement, MouseEvent>;
  /** 双击 / 回车触发（切到查看） */
  onActivate?: () => void;
  class?: string;
}

export function Tile(props: TileProps) {
  const [local, rest] = splitProps(props, [
    "label",
    "sublabel",
    "src",
    "icon",
    "selected",
    "disabled",
    "loading",
    "empty",
    "actions",
    "onClick",
    "onActivate",
    "class",
  ]);

  const interactive = () => !local.disabled;

  return (
    <div
      {...rest}
      role="option"
      aria-selected={Boolean(local.selected)}
      aria-disabled={local.disabled || undefined}
      tabindex={interactive() ? 0 : -1}
      class={[
        "group/tile relative flex shrink-0 flex-col overflow-hidden rounded-ui transition-colors",
        "outline-none focus-visible:ring-1 focus-visible:ring-focus-ring",
        // 选中：大面积用主色低透明度底（§5.1）
        local.selected ? "bg-state-selected" : "bg-transparent",
        // 指向：辅色底；已选中时保持主色底不回落（§5）
        interactive() && !local.selected ? "hover:bg-state-hover" : "",
        interactive() ? "cursor-pointer" : "cursor-default text-fg-3",
        local.class ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
      onClick={local.onClick}
      onDblClick={() => local.onActivate?.()}
      onKeyDown={(event) => {
        if (event.key === "Enter") {
          event.preventDefault();
          local.onActivate?.();
          return;
        }
        if (event.key === " ") {
          // Space 默认会滚动容器，必须挡掉
          event.preventDefault();
          event.currentTarget.click();
        }
      }}
    >
      {/* ── 画面区 ─────────────────────────────────────────── */}
      <div
        class="relative flex items-center justify-center overflow-hidden bg-surface-track"
        style={{
          width: "var(--tile-cell-w)",
          height: "var(--tile-cell-h)",
        }}
      >
        <Show
          when={local.src && !local.loading}
          fallback={
            <span
              class={[
                "flex items-center justify-center text-fg-3",
                local.loading ? "animate-pulse" : "",
              ].join(" ")}
              aria-hidden="true"
            >
              {local.loading ? (
                <span class="size-4 animate-spin rounded-full border border-current border-t-transparent" />
              ) : (
                (local.icon ?? <span class="size-5 rounded-ui bg-fg-3/20" />)
              )}
            </span>
          }
        >
          {/*
            `draggable=false`：桌面应用里拖拽图片会把 webview 变成「拖文件」状态，
            与后续要做的 tile 拖选冲突。
          */}
          <img
            src={local.src}
            alt={local.label}
            draggable={false}
            class="size-full object-cover"
          />
        </Show>

        {/* 空态标记：唯一的视觉差异是图标更淡 —— 不额外加描边或底色（§6 无边线） */}
        <Show when={local.empty && !local.loading}>
          <span class="absolute inset-0 flex items-center justify-center">
            <span class="text-[11px] text-fg-3">—</span>
          </span>
        </Show>

        {/* 动作区：默认隐藏，指向或键盘聚焦时出现 */}
        <Show when={local.actions}>
          <div class="absolute top-1 right-1 hidden group-hover/tile:flex group-focus-within/tile:flex">
            {local.actions}
          </div>
        </Show>
      </div>

      {/* ── 字幕条 ─────────────────────────────────────────── */}
      <div
        class="flex min-w-0 flex-col justify-center px-1"
        style={{ height: "var(--caption-h)" }}
      >
        <span
          class={[
            "truncate text-[11px] leading-tight",
            local.selected ? "text-fg-1" : "text-fg-2",
            local.loading ? "opacity-40" : "",
          ].join(" ")}
          title={local.label}
        >
          {local.label}
        </span>
        <Show when={local.sublabel}>
          <span class="truncate text-[10px] leading-tight text-fg-3 tnum">
            {local.sublabel}
          </span>
        </Show>
      </div>
    </div>
  );
}
