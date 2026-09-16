/**
 * Tile —— 网格基本单元（DESIGN.md §10.1 #11）。
 *
 * 是**图标 tile 与图片 tile 的公共骨架**：上方一个定尺寸的画面区，下方一条字幕。
 * 它只负责「一个格子长什么样」，不含虚拟化、不含批量选择逻辑（那些在 `PhotoGrid` 里）。
 *
 * ## 排版口径（2026-09-16 人类反馈后重做）
 *
 * ```text
 * ┌──────────────────────────────┐  ← 整块一个圆角容器，四周有 --tile-pad 内边距
 * │  ╭────────────────────────╮  │
 * │  │        画面            │  │  ← 图片**四角都圆**（不是只圆上面两角：
 * │  │              [动作槽]  │  │     上一版图片下缘是直角，看着像被切开）
 * │  ╰────────────────────────╯  │
 * │  P1000156            [ORF]   │  ← 文件名（**去掉后缀**）+ 扩展名做成标签
 * └──────────────────────────────┘     下缘圆角由容器给出
 * ```
 *
 * * **边角**：容器 `rounded-ui` + 图片自己也 `rounded-ui` —— 图片不再与容器边缘平齐，
 *   所以四角一致；容器下缘的圆角也就顺理成章。
 * * **间距**：四周 `--tile-pad`、图与字之间 `--tile-gap`（都在 `tokens.css`，两档密度不同）。
 * * **信息**：只显示文件名（去后缀）+ **扩展名标签**；不再把后缀重复写两遍。
 *   扩展名是 RAW/JPG 配对的唯一线索，所以留着，但做成小标签而不是第二行小字。
 * * **选中**：容器主色低透明度底 + 画面外一圈主色细环（图片本身就是内容，
 *   只靠底色在照片上分不出来）。指向用辅色底（§5 全局规则）。
 * * **扩展位**（现在不显示，位置先留好）：画面**右上角**是动作槽（排除等，指向/聚焦/选中时出现）；
 *   文件名那一行的**右端**留给星标/旗标这类「属性」；画面**左下角**留给以后要叠在图上
 *   的信息（评级、RAW 配对角标）。三处都写在这里，免得以后到处试位置。
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
  /**
   * 文件名行右端的**标签**（现在放扩展名，如 `ORF`）。
   * 不放第二行小字：后缀已经写在文件名里了，重复两遍正是上一版被指出的问题。
   */
  tag?: string;
  /** 「属性」类信息的扩展位（星标等，未来的）。放在文件名行右端。 */
  extra?: JSX.Element;
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
    "tag",
    "extra",
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

  /**
   * 显示用的文件名：**去掉末尾的扩展名**。
   *
   * 判据刻意保守：只在「最后一个点后面是 1~5 位字母数字」时才当后缀 ——
   * `IMG.2024.raw` 这类名字去掉 `.2024.raw` 就毁了（只去最后一段）。
   */
  const displayName = (): string => {
    const name = local.label;
    const dot = name.lastIndexOf(".");
    if (dot <= 0 || dot === name.length - 1) return name;
    const tail = name.slice(dot + 1);
    return /^[A-Za-z0-9]{1,5}$/.test(tail) ? name.slice(0, dot) : name;
  };

  return (
    <div
      {...rest}
      role="option"
      aria-selected={Boolean(local.selected)}
      aria-disabled={local.disabled || undefined}
      tabindex={interactive() ? 0 : -1}
      class={[
        "group/tile relative flex shrink-0 flex-col rounded-ui transition-colors",
        // 四周内边距与图文间距都走令牌（两档密度不同 —— 上一版写死，紧凑与宽松一个样）
        "p-(--tile-pad) gap-(--tile-gap)",
        "outline-none focus-visible:ring-1 focus-visible:ring-focus-ring",
        // 选中：主色低透明度底 + 画面外一圈主色细环（照片上只靠底色分不出来）
        local.selected
          ? "bg-state-selected ring-2 ring-brand"
          : "bg-transparent",
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
        // 四角都圆：图片不再与容器边缘平齐，所以不会出现「上圆下方」
        class="relative flex items-center justify-center overflow-hidden rounded-ui bg-surface-main"
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
            <span class="text-fs-1 text-fg-3">—</span>
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
        class="flex min-w-0 items-center gap-1"
        style={{ height: "var(--caption-h)" }}
      >
        <span
          class={[
            "min-w-0 flex-1 truncate text-fs-1 leading-tight",
            local.selected ? "text-fg-1" : "text-fg-2",
            local.loading ? "opacity-40" : "",
          ].join(" ")}
          // 悬停给完整名字（含后缀）—— 显示上省略后缀是为了不重复，不是要藏起来
          title={local.label}
        >
          {displayName()}
        </span>
        <Show when={local.tag}>
          <span class="shrink-0 rounded-ui bg-surface-track px-1 text-fs-0 leading-tight text-fg-3">
            {local.tag}
          </span>
        </Show>
        {/*
          扩展位（将来）：星标 / 旗标 / 颜色标记放这一行的**右端**。
          它们属于「这张照片的属性」，放在文件名行比叠在图上更好读，也不挡画面。
        */}
        <Show when={local.extra}>{local.extra}</Show>
      </div>
    </div>
  );
}
