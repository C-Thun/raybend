/**
 * 总览页的**缩放控制**（扁扁一条）：`− [100%] ＋`。
 *
 * # 为什么需要它
 *
 * 编辑器视口**没有**浏览/看图那套「鼠标靠近才浮出来的按钮」——
 * 缩放倍率得有个稳定的读数与入口（人类 2026-09-24 定：放在右栏「总览」下）。
 *
 * # 事实源在 Rust
 *
 * `Viewport.zoom` 是唯一真相（图像像素 → 物理像素；`1.0` = 100% = 1:1）。
 * 前端不自己记缩放，只显示 `renderState().zoom` 并发**意图**：
 *
 * * `−` / `＋`：`zoomBy` 一档（`EDITOR_ZOOM_STEP`，与看图命令 `viewer.zoomIn/Out` 同一档）；
 * * 手动输入：按当前值折算成 `zoomBy` 的倍率 —— 夹取（`MIN_ZOOM` 0.01 / `MAX_ZOOM` 64）
 *   由 Rust 侧做，这里只做输入合法性兜底。
 */

import { createSignal, type JSX } from "solid-js";
import { IconMinus, IconPlus } from "@tabler/icons-solidjs";

import { Button } from "../../components/ui/Button.tsx";
import { Input } from "../../components/ui/Form.tsx";
import { t } from "../../i18n/index.ts";

import { EDITOR_ZOOM_STEP } from "./viewport.tsx";

/** 手动输入的百分比范围（与 Rust 的 `MIN_ZOOM` / `MAX_ZOOM` 对齐：1% – 6400%）。 */
const MIN_PERCENT = 1;
const MAX_PERCENT = 6400;

export interface EditorZoomControlProps {
  /** Rust 视口的当前缩放（`null` = 还没有渲染状态：显示 `—` 且禁用） */
  zoom: number | null;
  /** 有没有可编辑的照片（空态禁用） */
  enabled: boolean;
  /** 按倍率缩放一档（Rust 侧夹取） */
  onZoomBy: (factor: number) => void;
  /** 手动输入的目标缩放（`1.0` = 100%）；由调用方折算成倍率 */
  onZoomTo: (zoom: number) => void;
  class?: string;
}

export function EditorZoomControl(props: EditorZoomControlProps): JSX.Element {
  /** 正在编辑的输入文本；`null` = 不在编辑（显示真实缩放） */
  const [draft, setDraft] = createSignal<string | null>(null);

  const percent = (): number | null =>
    props.zoom === null || !Number.isFinite(props.zoom) ? null : Math.round(props.zoom * 100);

  const shown = (): string => {
    const editing = draft();
    if (editing !== null) return editing;
    const value = percent();
    return value === null ? "—" : `${value}%`;
  };

  const usable = (): boolean => props.enabled && percent() !== null;

  /** 输入提交：认不出 / 超范围 → 回到真实值（不制造一次无效意图） */
  const commit = (): void => {
    const text = draft();
    if (text === null) return;
    setDraft(null);
    const parsed = Number.parseFloat(text.replace("%", "").trim());
    if (!Number.isFinite(parsed)) return;
    const clamped = Math.min(MAX_PERCENT, Math.max(MIN_PERCENT, parsed));
    props.onZoomTo(clamped / 100);
  };

  return (
    <div
      class={["flex items-center justify-center gap-1", props.class ?? ""]
        .filter(Boolean)
        .join(" ")}
      data-editor-zoom
    >
      <Button
        variant="ghost"
        size="sm"
        icon={<IconMinus size={14} />}
        aria-label={t("viewer.zoom_out")}
        title={t("viewer.zoom_out")}
        disabled={!usable()}
        onClick={() => props.onZoomBy(1 / EDITOR_ZOOM_STEP)}
      />
      <Input
        class="w-16 text-center tabular-nums"
        value={shown()}
        disabled={!usable()}
        aria-label={t("editor.zoom.label")}
        title={t("editor.zoom.label")}
        /* 进入编辑时换成**裸数字**（带上 `%` 编辑起来别扭），提交后再回到真实值 */
        onFocus={() => setDraft(percent() === null ? "" : String(percent()))}
        onInput={(event) => setDraft(event.currentTarget.value)}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.currentTarget.blur();
          } else if (event.key === "Escape") {
            setDraft(null);
            event.currentTarget.blur();
          }
        }}
      />
      <Button
        variant="ghost"
        size="sm"
        icon={<IconPlus size={14} />}
        aria-label={t("viewer.zoom_in")}
        title={t("viewer.zoom_in")}
        disabled={!usable()}
        onClick={() => props.onZoomBy(EDITOR_ZOOM_STEP)}
      />
    </div>
  );
}
