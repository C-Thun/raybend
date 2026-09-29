/**
 * `ImportConfirmDialog` —— 点「导入」后的**确认弹窗**（崔总 2026-09-28 口述）。
 *
 * ```text
 * ┌────────────────────────────────────────────┐
 * │ 确认导入                              [✕]  │
 * │ 已选目录                                    │
 * │  ┌────────────────────────────────────────┐ │
 * │  │ 📁 D:\照片\2026-08-15                   │ │
 * │  │ 📁 E:\卡\DCIM          包含子目录       │ │
 * │  └────────────────────────────────────────┘ │
 * │                     ⌄                       │  ← 宽扁的向下箭头：方向 = 送到下面去
 * │ 导入到                                      │
 * │  ┌────────────────────────────────────────┐ │
 * │  │ ▣ 测试库                    1,234 张    │ │
 * │  └────────────────────────────────────────┘ │
 * │                        [取消] [开始导入]    │
 * └────────────────────────────────────────────┘
 * ```
 *
 * ## 为什么要它
 *
 * 导入是**不可逆的大动作**（往库里拷文件、写库、建缩略图），点一下就开工太轻率；
 * 而这时最该看清的两件事 —— 「选了哪些目录」「进哪个库」—— 恰好分居左右两列，
 * 中间隔着照片网格，一眼看不全。这个弹窗把它们摆成上下一条流水线，看一眼就能拍板。
 *
 * ## 三条纪律
 *
 * 1. **只展示，不改清单**：目录行与库卡片都走只读形态（`SelectedDirs readOnly`、
 *    `RepositoryCard interactive={false}`）—— 要改去左列/右列改，这里只管确认。
 * 2. **不重写第二份列表**：上面是既有的「已选目录」组件，下面是全项目唯一一份库卡片。
 * 3. **确认后才开工**：点确认先关自己，再把活交给工作区原来那条路（预检 → 开工），
 *    随后顶上来的是既有的「导入中」进度窗 —— 这里不碰进度状态。
 */

import { IconChevronDown } from "@tabler/icons-solidjs";
import { Show } from "solid-js";
import { Button } from "../../components/ui/Button.tsx";
import { Dialog } from "../../components/ui/Dialog.tsx";
import { RepositoryCard } from "../../components/ui/RepositoryCard.tsx";
import { SelectedDirs } from "../../features/selected-dirs/index.ts";
import type { CheckedDir } from "../../lib/checked-dir.ts";
import type { GroupingLocale } from "../../lib/format.ts";
import { t } from "../../i18n/index.ts";

export interface ImportConfirmDialogProps {
  open: boolean;
  /** 已勾选、准备导入的目录（**只读展示**：范围含不含子目录要在这里看得见） */
  dirs: readonly CheckedDir[];
  /** 目标库（`null` = 还没选库；那时确认按钮是禁用的，与右列的导入按钮同一门槛） */
  repository: { name: string; displayPath: string | null; photosCount: number | null; online: boolean } | null;
  onCancel: () => void;
  onConfirm: () => void;
  locale?: GroupingLocale;
}

export function ImportConfirmDialog(props: ImportConfirmDialogProps) {
  return (
    <Dialog
      open={props.open}
      onOpenChange={(open) => {
        if (!open) props.onCancel();
      }}
      title={t("import.confirm.title")}
      footer={
        <>
          <Button variant="secondary" onClick={() => props.onCancel()}>
            {t("common.cancel")}
          </Button>
          <Button
            variant="primary"
            disabled={props.repository === null || props.dirs.length === 0}
            onClick={() => props.onConfirm()}
          >
            {t("import.confirm.start")}
          </Button>
        </>
      }
    >
      <div data-import-confirm class="flex flex-col gap-2">
        <p class="text-fs-1 tracking-wide text-fg-2 uppercase">
          {t("source.selected")}
        </p>
        {/*
          目录多的时候这里会很长：**弹窗自己滚**（外面那层没有滚动），
          最多占窗口三成高，剩下的留给箭头与库卡片。
        */}
        <div data-import-confirm-dirs class="max-h-[30vh] min-h-0 overflow-auto">
          <SelectedDirs entries={props.dirs} readOnly />
        </div>

        {/* 箭头：宽一点、扁一点（`IconChevronDown` 就是最扁的那一档），只表达方向 */}
        <div data-import-confirm-arrow class="flex items-center justify-center py-1" aria-hidden="true">
          <IconChevronDown size={56} stroke-width={1.5} class="text-fg-3" />
        </div>

        <p class="text-fs-1 tracking-wide text-fg-2 uppercase">
          {t("import.confirm.into")}
        </p>
        <Show when={props.repository}>
          {(repository) => (
            <div data-import-confirm-repo>
              <RepositoryCard
                name={repository().name}
                displayPath={repository().displayPath}
                photosCount={repository().photosCount}
                online={repository().online}
                selected
                interactive={false}
                locale={props.locale ?? "zh-CN"}
              />
            </div>
          )}
        </Show>
      </div>
    </Dialog>
  );
}
