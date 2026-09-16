/**
 * `RepositoryFooter` —— 右列底部的动作区（`design/main.md` §3.3，自上而下）。
 *
 * ```text
 * 已选择 3 个目录 · 1 248 张照片        已排除 12 张   ← 让人在按下之前就知道要带进多少东西
 * [        导入        ]            ← 左右都选中才可点
 * ☑ 避免重复导入                    ← 默认勾上；顺序上它是按钮的「修饰」
 * ```
 *
 * 两个数字的口径（2026-09-16 人类要求把「排除」这条链路做通）：
 *   - 左边那个 M = 勾选目录的照片总数 **− 落在这些目录里的排除数**（`importPhotoCount`）；
 *   - 右边那个 K = 落在已勾选目录里的排除数，**有排除时才显示**。
 * 两者都由工作区算好传进来 —— 这里只管排版（本身不做业务判断）。
 *
 * 「左右都选中才可点」的规则（`design/main.md` §3.3）：
 *   * 左边：至少勾选了一个目录
 *   * 右边：选中了一个库，**而且那个库在线**（离线的库写不进去）
 */

import { Show } from "solid-js";
import { Button } from "../../components/ui/Button.tsx";
import { Checkbox } from "../../components/ui/Form.tsx";
import { t } from "../../i18n/index.ts";
import { formatCount, type GroupingLocale } from "../../lib/format.ts";
import type { CheckedDir } from "../../lib/checked-dir.ts";
import { importPhotoCount } from "../../lib/excluded.ts";

export interface RepositoryFooterProps {
  /** 已勾选的目录（左边） */
  checkedDirs: readonly CheckedDir[];
  /** 已勾选目录的已知照片总数；`null` = 还有没数完的 */
  photoCount: number | null;
  /** 落在已勾选目录里的排除张数（0 则不显示右边那句） */
  excludedCount?: number;
  /** 有没有选中库、以及它是否在线（右边） */
  hasRepository: boolean;
  repositoryOnline: boolean;
  avoidDuplicates: boolean;
  onAvoidDuplicatesChange: (value: boolean) => void;
  /** 按下导入（M1-6 才真的执行；这里只做可用性与事件） */
  onImport: () => void;
  locale?: GroupingLocale;
  class?: string;
}

export function RepositoryFooter(props: RepositoryFooterProps) {
  const locale = (): GroupingLocale => props.locale ?? "zh-CN";
  const canImport = () =>
    props.checkedDirs.length > 0 &&
    props.hasRepository &&
    props.repositoryOnline;

  const excludedCount = (): number => props.excludedCount ?? 0;

  const summary = () =>
    t("common.selected_summary", {
      dirs: props.checkedDirs.length,
      photos:
        props.photoCount === null
          ? t("source.counting")
          : formatCount(
              importPhotoCount(props.photoCount, excludedCount()) ?? 0,
              locale(),
            ),
    });

  return (
    <div class={["flex flex-col gap-1.5", props.class ?? ""].join(" ")}>
      {/* 选中统计（放在按钮上面：按之前先看清代价）；排除数靠右，不抢主句 */}
      <p class="flex items-baseline gap-2 text-fs-1 text-fg-2">
        <span class="min-w-0 flex-1 truncate">{summary()}</span>
        <Show when={excludedCount() > 0}>
          <span class="shrink-0 text-fg-3 tnum">
            {t("import.excluded_count", { n: formatCount(excludedCount(), locale()) })}
          </span>
        </Show>
      </p>

      <Button
        variant="primary"
        class="w-full"
        style={{ height: "var(--action-btn-h)" }}
        disabled={!canImport()}
        onClick={props.onImport}
      >
        {t("repo.import")}
      </Button>

      {/* 为什么不能点：一句话说清缺哪一边 */}
      <Show when={!canImport()}>
        <p class="text-fs-0 text-fg-3">{t("repo.import_hint")}</p>
      </Show>

      <Checkbox
        checked={props.avoidDuplicates}
        onCheckedChange={props.onAvoidDuplicatesChange}
        label={t("repo.avoid_duplicates")}
      />
    </div>
  );
}
