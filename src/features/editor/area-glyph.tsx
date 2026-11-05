/**
 * **编辑区（area）的类型图标** —— 全项目唯一一份。
 *
 * 三个地方要看同一种「这是什么层」的记号：
 * * 左列层列表的行首（`area-panel.tsx`）；
 * * 左列拖动时的整行幽灵（同一份行内容）；
 * * 右栏直方图下面的**当前层名条**（`panels.tsx`）。
 *
 * 形状口径（`design/editor.md` 2026-10-31 表 #1）：
 *
 * | 层 | 记号 | 理由 |
 * | --- | --- | --- |
 * | `默认` | 滑杆（`sliders-horizontal`） | 它是全局功能的宿主 |
 * | `全图` | 方块 | 「整幅」的最简形 |
 * | `控制点` | 圆点 | 就是它在画布上的圆片 |
 *
 * 层名条里用**主色**画它（崔总 2026-11-01）：「你现在改的是哪一层」要比一行灰字更抓眼；
 * 列表行里仍用注释色（一行里已经有名字与开关，再上主色会吵）。
 */

import type { JSX } from "solid-js";
import { IconAdjustmentsHorizontal, IconSquare } from "@tabler/icons-solidjs";

import { POINT_COLORS } from "./area-panel.tsx";

export type AreaGlyphKind = "default" | "full" | "point";

export interface AreaGlyphProps {
  kind: AreaGlyphKind;
  /** 控制点色槽（只有 `point` 用得到） */
  color?: number;
  /** 图标尺寸（px）；圆点按它的 0.78 画（视觉面积与 14px 描边图标相当） */
  size?: number;
  class?: string;
}

export function AreaGlyph(props: AreaGlyphProps): JSX.Element {
  const size = (): number => props.size ?? 14;
  const tone = (): string => props.class ?? "text-fg-2";
  return (
    <>
      {props.kind === "default" ? (
        <IconAdjustmentsHorizontal size={size()} class={`shrink-0 ${tone()}`} aria-hidden="true" />
      ) : props.kind === "full" ? (
        <IconSquare size={size()} class={`shrink-0 ${tone()}`} aria-hidden="true" />
      ) : (
        <span
          class={`shrink-0 rounded-full ${tone()}`}
          style={{
            width: `${size() * 0.78}px`,
            height: `${size() * 0.78}px`,
            "background-color": POINT_COLORS[(props.color ?? 0) % POINT_COLORS.length],
          }}
          aria-hidden="true"
        />
      )}
    </>
  );
}
