/**
 * `ExifStrip` —— flowbar 右侧的图片信息区（`design/main.md` §2.2）。
 *
 * 三组信息，每组是一个 `easy copy`（悬停出细边框、点击复制整组）：
 *   机型 + 镜头 ｜ 焦距 + 光圈 + 快门 + 感光度 ｜ 尺寸 + 像素数 + 格式
 *
 * 排版是「**值 + 间隔点**」而不是「标签 + 值」：这一行高度有限，且值本身有强可识别性
 * （`DC-G9` 一眼就是机型）。字段名放在两处补足语义：
 *   - 复制出去的文本带字段名（粘到别处还看得懂）
 *   - 每个值的原生 `title`（悬停指出这是焦距还是光圈）
 *
 * **空态**：没有照片或元数据不全时显示 `未选择照片`。注意空态不是错误 ——
 * 导入工作区里一开始本来就没选中照片。
 */

import { For, Show, splitProps } from "solid-js";
import { EasyCopy } from "../../components/ui/EasyCopy.tsx";
import { t } from "../../i18n";
import { groupExif } from "./grouping.ts";
import type { ExifData } from "./types.ts";

export interface ExifStripProps {
  /** 当前照片的 EXIF；`null` / 缺字段都不影响渲染（组会自动消失） */
  data?: ExifData | null;
  class?: string;
}

export function ExifStrip(props: ExifStripProps) {
  const [local, rest] = splitProps(props, ["data", "class"]);
  const groups = () => groupExif(local.data ?? null, (key) => t(key));

  return (
    <Show
      when={groups().length > 0}
      fallback={
        <span class="shrink-0 text-fs-1 text-fg-3">{t("exif.empty")}</span>
      }
    >
      <div
        {...rest}
        class={[
          "flex min-w-0 items-center gap-1 overflow-hidden",
          local.class ?? "",
        ]
          .filter(Boolean)
          .join(" ")}
      >
        <For each={groups()}>
          {(group) => (
            /*
             * 组**不压缩**（`shrink-0`），放不下时由最外层容器裁切。
             *
             * 为什么不给每个值加 `truncate`：那样在窄容器里会变成
             * `DC…`、`12…`、`f/…` 一片残片，反而什么都读不出来。
             * 现在的行为是「从左往右能显示多少完整值就显示多少」——
             * 符合「扫一眼看参数」的用途（实测：陈列室里三组曾叠成一团，
             * 原因是组内文字溢出却没有裁切）。
             */
            <EasyCopy value={group.copyText} class="shrink-0 overflow-hidden">
              <span class="flex shrink-0 items-center gap-1.5 text-fs-1 whitespace-nowrap">
                <For each={group.parts}>
                  {(part, index) => (
                    <>
                      <Show when={index() > 0}>
                        {/* 间隔点只是视觉分隔，不该被读屏念出来 */}
                        <span class="text-fg-3" aria-hidden="true">
                          ·
                        </span>
                      </Show>
                      <span
                        class={[
                          "shrink-0 tnum",
                          part.emphasis ? "text-fg-1" : "text-fg-2",
                        ].join(" ")}
                        title={part.labelKey ? t(part.labelKey) : undefined}
                      >
                        {part.text}
                      </span>
                    </>
                  )}
                </For>
              </span>
            </EasyCopy>
          )}
        </For>
      </div>
    </Show>
  );
}
